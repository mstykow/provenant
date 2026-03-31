use std::path::Path;

use glob::Pattern;

use crate::models::{DatasourceId, FileInfo, Package, PackageData, TopLevelDependency};

use super::AssemblerConfig;

struct PendingDependency {
    dependency: crate::models::Dependency,
    datafile_path: String,
    datasource_id: DatasourceId,
}

/// Assemble a single package from sibling files in a directory.
///
/// Iterates over `sibling_file_patterns` in order, finds matching files among
/// `file_indices`, and merges their package data into a single `Package`.
/// Dependencies from all matched files are hoisted to the top level.
///
/// Returns `None` if no files with valid package data are found.
pub fn assemble_siblings(
    config: &AssemblerConfig,
    files: &[FileInfo],
    file_indices: &[usize],
) -> Option<(Option<Package>, Vec<TopLevelDependency>, Vec<usize>)> {
    let mut package: Option<Package> = None;
    let mut pending_dependencies = Vec::new();
    let mut affected_indices = Vec::new();
    let mut saw_unpackageable_npm_manifest = false;

    for &pattern in config.sibling_file_patterns {
        for &idx in file_indices {
            let file = &files[idx];
            let file_name = Path::new(&file.path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            if !matches_pattern(file_name, pattern) {
                continue;
            }

            if file.package_data.is_empty() {
                continue;
            }

            let mut file_used = false;

            for pkg_data in &file.package_data {
                if !is_handled_by(pkg_data, config) {
                    continue;
                }

                if pkg_data.datasource_id == Some(DatasourceId::NpmPackageJson)
                    && pkg_data.purl.is_none()
                {
                    saw_unpackageable_npm_manifest = true;
                }

                if should_skip_lock_merge(package.as_ref(), pkg_data) {
                    continue;
                }

                let datafile_path = file.path.clone();
                let Some(datasource_id) = pkg_data.datasource_id else {
                    continue;
                };
                file_used = true;

                match &mut package {
                    None => {
                        if (pkg_data.purl.is_some() || has_assemblable_identity(pkg_data))
                            && !should_skip_npm_lock_package_creation(
                                pkg_data,
                                saw_unpackageable_npm_manifest,
                            )
                        {
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

            if file_used {
                affected_indices.push(idx);
            }
        }
    }

    let for_package_uid = package.as_ref().map(|p| p.package_uid.clone());
    let dependencies: Vec<TopLevelDependency> = pending_dependencies
        .into_iter()
        .map(|pending| {
            TopLevelDependency::from_dependency(
                &pending.dependency,
                pending.datafile_path,
                pending.datasource_id,
                for_package_uid.clone(),
            )
        })
        .collect();

    if package.is_some() || !dependencies.is_empty() {
        Some((package, dependencies, affected_indices))
    } else {
        None
    }
}

/// Check if a filename matches a pattern. Supports:
/// - Exact match (e.g., "package.json")
/// - Case-insensitive match (e.g., "Cargo.toml" vs "cargo.toml")
/// - Glob-style prefix wildcard (e.g., "*.podspec" matches "MyLib.podspec")
pub(crate) fn matches_pattern(file_name: &str, pattern: &str) -> bool {
    if pattern.contains('*') {
        if let Ok(glob_pattern) = Pattern::new(pattern)
            && glob_pattern.matches(file_name)
        {
            return true;
        }

        let lower_name = file_name.to_ascii_lowercase();
        let lower_pattern = pattern.to_ascii_lowercase();
        if let Ok(glob_pattern) = Pattern::new(&lower_pattern) {
            return glob_pattern.matches(&lower_name);
        }

        false
    } else {
        file_name == pattern || file_name.eq_ignore_ascii_case(pattern)
    }
}

/// Check if a PackageData's datasource_id is handled by this assembler config.
fn is_handled_by(pkg_data: &PackageData, config: &AssemblerConfig) -> bool {
    pkg_data
        .datasource_id
        .is_some_and(|dsid| config.datasource_ids.contains(&dsid))
}

fn should_skip_lock_merge(package: Option<&Package>, pkg_data: &PackageData) -> bool {
    let Some(existing_package) = package else {
        return false;
    };

    should_skip_npm_lock_merge(existing_package, pkg_data)
        || should_skip_bun_lock_merge(existing_package, pkg_data)
        || should_skip_python_uv_lock_merge(existing_package, pkg_data)
        || should_skip_python_pip_cache_merge(existing_package, pkg_data)
}

fn should_skip_npm_lock_merge(package: &Package, pkg_data: &PackageData) -> bool {
    pkg_data.datasource_id == Some(DatasourceId::NpmPackageLockJson)
        && !npm_package_identity_matches(package, pkg_data)
}

fn should_skip_bun_lock_merge(package: &Package, pkg_data: &PackageData) -> bool {
    pkg_data
        .datasource_id
        .is_some_and(|id| matches!(id, DatasourceId::BunLock | DatasourceId::BunLockb))
        && !npm_package_identity_matches(package, pkg_data)
}

fn npm_package_identity_matches(package: &Package, pkg_data: &PackageData) -> bool {
    let Some(package_name) = normalized_identity_value(package.name.as_deref()) else {
        return false;
    };
    let Some(package_version) = normalized_identity_value(package.version.as_deref()) else {
        return false;
    };
    let Some(candidate_name) = normalized_identity_value(pkg_data.name.as_deref()) else {
        return false;
    };
    let Some(candidate_version) = normalized_identity_value(pkg_data.version.as_deref()) else {
        return false;
    };

    package_name == candidate_name && package_version == candidate_version
}

fn normalized_identity_value(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn has_assemblable_identity(pkg_data: &PackageData) -> bool {
    pkg_data.package_type.is_some() && normalized_identity_value(pkg_data.name.as_deref()).is_some()
}

fn should_skip_python_uv_lock_merge(package: &Package, pkg_data: &PackageData) -> bool {
    pkg_data.datasource_id == Some(DatasourceId::PypiUvLock)
        && package.datasource_ids.iter().any(|id| {
            matches!(
                id,
                DatasourceId::PypiPyprojectToml | DatasourceId::PypiPoetryPyprojectToml
            )
        })
        && !python_uv_identity_matches(package, pkg_data)
}

fn should_skip_python_pip_cache_merge(package: &Package, pkg_data: &PackageData) -> bool {
    pkg_data.datasource_id.is_some_and(|dsid| {
        matches!(
            dsid,
            DatasourceId::PypiWheel | DatasourceId::PypiPipOriginJson
        )
    }) && package.datasource_ids.iter().any(|dsid| {
        matches!(
            dsid,
            DatasourceId::PypiWheel | DatasourceId::PypiPipOriginJson
        )
    }) && !python_uv_identity_matches(package, pkg_data)
}

fn python_uv_identity_matches(package: &Package, pkg_data: &PackageData) -> bool {
    if let (Some(package_name), Some(candidate_name)) = (
        normalized_identity_value(package.name.as_deref()),
        normalized_identity_value(pkg_data.name.as_deref()),
    ) && package_name != candidate_name
    {
        return false;
    }

    if let (Some(package_version), Some(candidate_version)) = (
        normalized_identity_value(package.version.as_deref()),
        normalized_identity_value(pkg_data.version.as_deref()),
    ) && package_version != candidate_version
    {
        return false;
    }

    true
}

fn should_skip_npm_lock_package_creation(
    pkg_data: &PackageData,
    saw_unpackageable_npm_manifest: bool,
) -> bool {
    saw_unpackageable_npm_manifest
        && pkg_data.datasource_id == Some(DatasourceId::NpmPackageLockJson)
}
