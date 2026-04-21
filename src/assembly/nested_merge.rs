// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::models::{DatasourceId, Dependency, FileInfo, Package, PackageData, TopLevelDependency};

use super::AssemblerConfig;

struct PendingDependency {
    dependency: Dependency,
    datafile_path: String,
    datasource_id: DatasourceId,
}

pub fn assemble_nested_patterns(
    files: &[FileInfo],
    config: &AssemblerConfig,
) -> Option<(Package, Vec<TopLevelDependency>, Vec<usize>)> {
    if !has_nested_patterns(config) {
        return None;
    }

    let matching_files = find_matching_files(files, config);
    if matching_files.is_empty() {
        return None;
    }

    let package_root = find_package_root(&matching_files, files)?;

    let sibling_indices = find_nested_siblings(&package_root, files, config);

    if sibling_indices.len() < 2 {
        return None;
    }

    if should_skip_nested_merge(&package_root, &sibling_indices, files, config) {
        return None;
    }

    assemble_from_indices(config, files, &sibling_indices)
}

fn has_nested_patterns(config: &AssemblerConfig) -> bool {
    config
        .sibling_file_patterns
        .iter()
        .any(|p| p.contains("**"))
}

fn find_matching_files(files: &[FileInfo], config: &AssemblerConfig) -> Vec<usize> {
    files
        .iter()
        .enumerate()
        .filter(|(_, file)| {
            file.package_data.iter().any(|pkg_data| {
                pkg_data
                    .datasource_id
                    .is_some_and(|dsid| config.datasource_ids.contains(&dsid))
            })
        })
        .map(|(idx, _)| idx)
        .collect()
}

const NESTED_ANCHOR_DIRS: &[&str] = &["META-INF", "debian", "data.gz-extract"];

fn find_package_root(matching_indices: &[usize], files: &[FileInfo]) -> Option<PathBuf> {
    for &idx in matching_indices {
        let file_path = Path::new(&files[idx].path);

        for &anchor in NESTED_ANCHOR_DIRS {
            if file_path.components().any(|c| c.as_os_str() == anchor) {
                let mut current = file_path;
                while let Some(parent) = current.parent() {
                    if parent
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|name| name == anchor)
                    {
                        return parent.parent().map(|p| p.to_path_buf());
                    }
                    current = parent;
                }
            }
        }

        if file_path.file_name().and_then(|n| n.to_str()) == Some("metadata.gz-extract") {
            return file_path.parent().map(|p| p.to_path_buf());
        }

        if file_path.file_name().and_then(|n| n.to_str()) == Some("pom.xml") {
            return file_path.parent().map(|p| p.to_path_buf());
        }
    }

    None
}

fn find_nested_siblings(root: &Path, files: &[FileInfo], config: &AssemblerConfig) -> Vec<usize> {
    files
        .iter()
        .enumerate()
        .filter(|(_, file)| {
            let file_path = Path::new(&file.path);

            if !file_path.starts_with(root) {
                return false;
            }

            config.sibling_file_patterns.iter().any(|pattern| {
                if pattern.contains("**") {
                    matches_nested_pattern(&file.path, pattern)
                } else {
                    file_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|name| matches_simple_pattern(name, pattern))
                }
            })
        })
        .map(|(idx, _)| idx)
        .collect()
}

fn should_skip_nested_merge(
    root: &Path,
    indices: &[usize],
    files: &[FileInfo],
    config: &AssemblerConfig,
) -> bool {
    if !config
        .datasource_ids
        .contains(&crate::models::DatasourceId::MavenPom)
    {
        return false;
    }

    let nested_pom_count = indices
        .iter()
        .filter(|&&idx| {
            files[idx].package_data.iter().any(|pkg_data| {
                pkg_data.datasource_id == Some(crate::models::DatasourceId::MavenPom)
                    && Path::new(&files[idx].path).starts_with(root)
                    && files[idx].path.contains("META-INF/maven/")
            })
        })
        .count();

    nested_pom_count > 1
}

fn should_dedupe_ruby_extracted_dependencies(config: &AssemblerConfig) -> bool {
    config
        .datasource_ids
        .contains(&crate::models::DatasourceId::GemArchiveExtracted)
}

fn dependency_identity(
    dep: &TopLevelDependency,
) -> (Option<String>, Option<String>, Option<String>) {
    (
        dep.purl.clone(),
        dep.extracted_requirement.clone(),
        dep.scope.clone(),
    )
}

fn matches_nested_pattern(file_path: &str, pattern: &str) -> bool {
    let pattern_without_prefix = pattern.strip_prefix("**/").unwrap_or(pattern);

    file_path.contains(pattern_without_prefix)
}

fn matches_simple_pattern(file_name: &str, pattern: &str) -> bool {
    if let Some(suffix) = pattern.strip_prefix('*') {
        file_name.ends_with(suffix)
            || file_name
                .to_ascii_lowercase()
                .ends_with(&suffix.to_ascii_lowercase())
    } else {
        file_name == pattern || file_name.eq_ignore_ascii_case(pattern)
    }
}

fn assemble_from_indices(
    config: &AssemblerConfig,
    files: &[FileInfo],
    indices: &[usize],
) -> Option<(Package, Vec<TopLevelDependency>, Vec<usize>)> {
    let mut package: Option<Package> = None;
    let mut pending_dependencies = Vec::new();
    let mut affected_indices = Vec::new();

    for &pattern in config.sibling_file_patterns {
        for &idx in indices {
            let file = &files[idx];
            let file_path = Path::new(&file.path);
            let file_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            let matches = if pattern.contains("**") {
                matches_nested_pattern(&file.path, pattern)
            } else {
                matches_simple_pattern(file_name, pattern)
            };

            if !matches {
                continue;
            }

            if file.package_data.is_empty() {
                continue;
            }

            affected_indices.push(idx);

            for pkg_data in &file.package_data {
                if !is_handled_by(pkg_data, config) {
                    continue;
                }

                let datafile_path = file.path.clone();
                let Some(datasource_id) = pkg_data.datasource_id else {
                    continue;
                };

                match &mut package {
                    None => {
                        if pkg_data.purl.is_some() {
                            package =
                                Some(Package::from_package_data(pkg_data, datafile_path.clone()));
                        }
                    }
                    Some(pkg) => {
                        pkg.update(pkg_data, datafile_path.clone());
                    }
                }

                for dep in &pkg_data.dependencies {
                    if dep.purl.is_some() {
                        pending_dependencies.push(PendingDependency {
                            dependency: dep.clone(),
                            datafile_path: datafile_path.clone(),
                            datasource_id,
                        });
                    }
                }
            }
        }
    }

    package.map(|pkg| {
        let for_package_uid = Some(pkg.package_uid.clone());
        let mut dependencies = Vec::new();
        let mut seen_dependency_keys: HashSet<(Option<String>, Option<String>, Option<String>)> =
            HashSet::new();

        for pending in pending_dependencies {
            let candidate = TopLevelDependency::from_dependency(
                &pending.dependency,
                pending.datafile_path,
                pending.datasource_id,
                for_package_uid.clone(),
            );

            if should_dedupe_ruby_extracted_dependencies(config) {
                let key = dependency_identity(&candidate);
                if seen_dependency_keys.insert(key) {
                    dependencies.push(candidate);
                }
            } else {
                dependencies.push(candidate);
            }
        }

        (pkg, dependencies, affected_indices)
    })
}

fn is_handled_by(pkg_data: &PackageData, config: &AssemblerConfig) -> bool {
    pkg_data
        .datasource_id
        .is_some_and(|dsid| config.datasource_ids.contains(&dsid))
}

#[cfg(test)]
#[path = "nested_merge_test.rs"]
mod tests;
