//! npm/pnpm workspace assembly for monorepos.
//!
//! This module implements a post-processing pass that detects npm/pnpm workspace
//! roots in already-assembled packages, removes incorrectly-created root Packages,
//! discovers workspace members across directories, creates one Package per member,
//! and correctly assigns `for_packages` associations.
//!
//! # Workspace Detection
//!
//! Workspaces are detected through:
//! - `package.json` files with `extra_data.workspaces` field (npm/yarn)
//! - `pnpm-workspace.yaml` files with `extra_data.workspaces` field (pnpm)
//!
//! # Algorithm Overview
//!
//! 1. **Find workspace roots**: Scan for files with workspace patterns
//! 2. **Discover members**: Match glob patterns against all package.json files
//! 3. **Remove root Package**: Delete the incorrectly-assembled root package
//! 4. **Create member Packages**: One Package per workspace member
//! 5. **Hoist root dependencies**: Root deps become workspace-level (for_package_uid: None)
//! 6. **Assign for_packages**: Files under members → member UID, shared → all members
//! 7. **Resolve workspace: versions**: Replace `workspace:*` with actual versions

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use log::warn;

use super::{ASSEMBLERS, AssemblerConfig, sibling_merge};
use crate::models::{DatasourceId, FileInfo, Package, PackageData, TopLevelDependency};
use crate::utils::path::{parent_dir, parent_dir_for_lookup};

/// File-local topology hint emitted by npm-family workspace root files.
pub(super) struct NpmWorkspaceRootHint {
    /// Directory path of the workspace root
    pub(super) root_dir: PathBuf,
    /// File index of the root package.json (if exists)
    pub(super) root_package_json_idx: Option<usize>,
    /// File index of pnpm-workspace.yaml (if exists)
    pub(super) pnpm_workspace_yaml_idx: Option<usize>,
    /// Workspace glob patterns
    pub(super) patterns: Vec<String>,
}

pub(super) struct NpmWorkspaceMemberDomain {
    pub(super) manifest_idx: usize,
    pub(super) dir_path: PathBuf,
}

pub(super) struct NpmWorkspaceDomain {
    pub(super) root_dir: PathBuf,
    pub(super) root_package_json_idx: Option<usize>,
    pub(super) root_dir_file_indices: Vec<usize>,
    pub(super) members: Vec<NpmWorkspaceMemberDomain>,
    pub(super) is_pnpm_with_root_package: bool,
}

/// Collect npm-family workspace hints from parser output.
pub(super) fn collect_npm_workspace_hints(files: &[FileInfo]) -> Vec<NpmWorkspaceRootHint> {
    let mut roots = Vec::new();
    let mut seen_roots: HashMap<PathBuf, NpmWorkspaceRootHint> = HashMap::new();

    // First pass: find package.json files with workspaces
    for (idx, file) in files.iter().enumerate() {
        let path = Path::new(&file.path);
        let file_name = if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            name
        } else {
            continue;
        };

        if file_name != "package.json" {
            continue;
        }

        // Check if this package.json has workspace patterns
        for pkg_data in &file.package_data {
            if pkg_data.datasource_id != Some(DatasourceId::NpmPackageJson) {
                continue;
            }

            if let Some(workspaces) = extract_workspaces(pkg_data)
                && let Some(parent) = path.parent()
            {
                let root_dir = parent.to_path_buf();
                seen_roots.insert(
                    root_dir.clone(),
                    NpmWorkspaceRootHint {
                        root_dir,
                        root_package_json_idx: Some(idx),
                        pnpm_workspace_yaml_idx: None,
                        patterns: workspaces,
                    },
                );
            }
        }
    }

    // Second pass: find pnpm-workspace.yaml files
    for (idx, file) in files.iter().enumerate() {
        let path = Path::new(&file.path);
        let file_name = if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            name
        } else {
            continue;
        };

        if file_name != "pnpm-workspace.yaml" {
            continue;
        }

        // Check if this pnpm-workspace.yaml has workspace patterns
        for pkg_data in &file.package_data {
            if pkg_data.datasource_id != Some(DatasourceId::PnpmWorkspaceYaml) {
                continue;
            }

            if let Some(workspaces) = extract_workspaces(pkg_data)
                && let Some(parent) = path.parent()
            {
                let root_dir = parent.to_path_buf();
                let root_package_json_idx = find_root_package_json_index(files, &root_dir);

                // Either create new or update existing entry
                if let Some(existing) = seen_roots.get_mut(&root_dir) {
                    existing.pnpm_workspace_yaml_idx = Some(idx);
                    if existing.root_package_json_idx.is_none() {
                        existing.root_package_json_idx = root_package_json_idx;
                    }
                    if existing.patterns.is_empty() {
                        existing.patterns = workspaces;
                    }
                } else {
                    seen_roots.insert(
                        root_dir.clone(),
                        NpmWorkspaceRootHint {
                            root_dir,
                            root_package_json_idx,
                            pnpm_workspace_yaml_idx: Some(idx),
                            patterns: workspaces,
                        },
                    );
                }
            }
        }
    }

    roots.extend(seen_roots.into_values());
    roots.sort_by(|left, right| left.root_dir.cmp(&right.root_dir));
    roots
}

pub(super) fn plan_npm_workspace_domains(
    files: &[FileInfo],
    dir_files: &HashMap<PathBuf, Vec<usize>>,
    workspace_hints: &[&NpmWorkspaceRootHint],
) -> Vec<NpmWorkspaceDomain> {
    let mut domains = Vec::new();

    for workspace_hint in workspace_hints {
        let member_indices = discover_members(files, workspace_hint);

        if member_indices.is_empty() {
            warn!(
                "No workspace members found for patterns {:?} in {:?}",
                workspace_hint.patterns, workspace_hint.root_dir
            );
            continue;
        }

        let members = member_indices
            .iter()
            .map(|&manifest_idx| {
                let dir_path = Path::new(&files[manifest_idx].path)
                    .parent()
                    .expect("workspace member manifest must have a parent directory")
                    .to_path_buf();

                NpmWorkspaceMemberDomain {
                    manifest_idx,
                    dir_path,
                }
            })
            .collect();

        let is_pnpm_with_root_package = workspace_hint.pnpm_workspace_yaml_idx.is_some()
            && workspace_hint.root_package_json_idx.is_some_and(|idx| {
                files[idx].package_data.iter().any(|pkg| {
                    pkg.datasource_id == Some(DatasourceId::NpmPackageJson)
                        && pkg.purl.is_some()
                        && !pkg.is_private
                })
            });

        domains.push(NpmWorkspaceDomain {
            root_dir: workspace_hint.root_dir.clone(),
            root_package_json_idx: workspace_hint.root_package_json_idx,
            root_dir_file_indices: dir_files
                .get(&workspace_hint.root_dir)
                .cloned()
                .unwrap_or_default(),
            members,
            is_pnpm_with_root_package,
        });
    }

    domains.sort_by(|left, right| left.root_dir.cmp(&right.root_dir));
    domains
}

fn find_root_package_json_index(files: &[FileInfo], root_dir: &Path) -> Option<usize> {
    files.iter().position(|file| {
        let path = Path::new(&file.path);
        path.parent() == Some(root_dir)
            && path.file_name().and_then(|name| name.to_str()) == Some("package.json")
    })
}

/// Extract workspace patterns from PackageData extra_data
fn extract_workspaces(pkg_data: &PackageData) -> Option<Vec<String>> {
    let extra_data = pkg_data.extra_data.as_ref()?;
    let workspaces_value = extra_data.get("workspaces")?;

    extract_workspace_patterns(workspaces_value)
}

fn extract_workspace_patterns(value: &serde_json::Value) -> Option<Vec<String>> {
    let patterns = match value {
        serde_json::Value::String(pattern) => vec![pattern.clone()],
        serde_json::Value::Array(patterns) => patterns
            .iter()
            .filter_map(|pattern| pattern.as_str().map(str::to_string))
            .collect(),
        serde_json::Value::Object(object) => object
            .get("packages")
            .and_then(extract_workspace_patterns)
            .unwrap_or_default(),
        _ => Vec::new(),
    };

    if patterns.is_empty() {
        None
    } else {
        Some(patterns)
    }
}

/// Apply a planned npm workspace domain to the assembled package graph.
pub(super) fn apply_npm_workspace_domain(
    workspace_domain: &NpmWorkspaceDomain,
    files: &mut [FileInfo],
    packages: &mut Vec<Package>,
    dependencies: &mut Vec<TopLevelDependency>,
) {
    // Step 1: Remove stale root/member packages from earlier assembly domains.
    let root_package_uid = if workspace_domain.is_pnpm_with_root_package {
        if let Some(idx) = workspace_domain.root_package_json_idx {
            remove_root_package(&files[idx], packages, dependencies);
        }

        let Some((root_package, root_dependencies)) =
            create_root_package(files, &workspace_domain.root_dir_file_indices)
        else {
            return;
        };

        let root_package_uid = root_package.package_uid.clone();
        packages.push(root_package);
        dependencies.extend(root_dependencies);
        Some(root_package_uid)
    } else if let Some(idx) = workspace_domain.root_package_json_idx {
        remove_root_package(&files[idx], packages, dependencies);
        None
    } else {
        None
    };

    remove_member_packages(
        files,
        &workspace_domain
            .members
            .iter()
            .map(|member| member.manifest_idx)
            .collect::<Vec<_>>(),
        packages,
        dependencies,
    );

    // Step 4: Create member Packages
    let member_packages = create_member_packages(files, &workspace_domain.members);

    // Build a map of member package names to versions for workspace: resolution
    let mut member_versions: HashMap<String, String> = HashMap::new();
    for (pkg, _deps) in &member_packages {
        if let (Some(name), Some(version)) = (workspace_member_name(pkg), &pkg.version) {
            member_versions.insert(name, version.clone());
        }
    }

    // Collect member UIDs for for_packages assignment
    let member_uids: Vec<String> = member_packages
        .iter()
        .map(|(pkg, _deps)| pkg.package_uid.clone())
        .collect();

    // Step 5: Handle root dependencies (hoist to workspace level)
    if let Some(idx) = workspace_domain.root_package_json_idx
        && !workspace_domain.is_pnpm_with_root_package
    {
        remove_root_level_dependencies(dependencies, &workspace_domain.root_dir);
        hoist_root_dependencies(
            files,
            idx,
            &workspace_domain.root_dir,
            dependencies,
            &member_versions,
            None,
        );
    }

    // Add member packages and dependencies to output
    for (pkg, deps) in member_packages {
        packages.push(pkg);
        dependencies.extend(deps);
    }

    // Step 6: Assign for_packages
    assign_for_packages(
        files,
        workspace_domain,
        &member_uids,
        root_package_uid.as_deref(),
    );

    // Step 7: Resolve workspace: versions in all dependencies
    resolve_workspace_versions(dependencies, &member_versions);
}

/// Discover workspace member package.json files matching the patterns
fn discover_members(files: &[FileInfo], workspace_root: &NpmWorkspaceRootHint) -> Vec<usize> {
    let mut member_indices = Vec::new();
    let mut excluded_paths = Vec::new();

    // First pass: collect exclusion patterns (patterns starting with !)
    for pattern in &workspace_root.patterns {
        if let Some(stripped) = pattern.strip_prefix('!') {
            excluded_paths.push(stripped);
        }
    }

    // Second pass: match inclusion patterns
    for (idx, file) in files.iter().enumerate() {
        let path = Path::new(&file.path);

        // Skip if not a package.json
        if path.file_name().and_then(|n| n.to_str()) != Some("package.json") {
            continue;
        }

        // Skip if not under workspace root
        if !path.starts_with(&workspace_root.root_dir) {
            continue;
        }

        // Skip root package.json itself
        if Some(idx) == workspace_root.root_package_json_idx {
            continue;
        }

        // Skip if no valid PackageData with purl
        let has_valid_package = file.package_data.iter().any(|pkg| {
            pkg.datasource_id == Some(DatasourceId::NpmPackageJson) && pkg.purl.is_some()
        });
        if !has_valid_package {
            continue;
        }

        // Check if path matches any pattern
        let relative_path = if let Ok(rel) = path.strip_prefix(&workspace_root.root_dir) {
            rel
        } else {
            continue;
        };

        let mut matched = false;
        for pattern in &workspace_root.patterns {
            if pattern.starts_with('!') {
                continue; // Exclusions handled separately
            }

            if matches_workspace_pattern(relative_path, pattern) {
                matched = true;
                break;
            }
        }

        if !matched {
            continue;
        }

        // Check exclusions
        let excluded = excluded_paths
            .iter()
            .any(|excl| matches_workspace_pattern(relative_path, excl));

        if !excluded {
            member_indices.push(idx);
        }
    }

    member_indices.sort_by(|left, right| files[*left].path.cmp(&files[*right].path));
    member_indices
}

/// Check if a path matches a workspace glob pattern
fn matches_workspace_pattern(path: &Path, pattern: &str) -> bool {
    // Convert path to string with forward slashes
    let path_str = path.to_str().unwrap_or("");

    // Handle simple patterns without wildcards
    if !pattern.contains('*') && !pattern.contains('?') {
        // Exact match: "packages/foo" → look for packages/foo/package.json
        let pattern_with_manifest = format!("{}/package.json", pattern);
        return path_str == pattern_with_manifest;
    }

    // Handle single trailing star: "packages/*" → packages/*/package.json
    if pattern.ends_with("/*") && !pattern[..pattern.len() - 2].contains('*') {
        let prefix = &pattern[..pattern.len() - 2];
        if let Some(remainder) = path_str.strip_prefix(prefix) {
            if remainder.is_empty() {
                return false;
            }
            // Check if it's exactly one level deep + package.json
            let parts: Vec<&str> = remainder.trim_start_matches('/').split('/').collect();
            return parts.len() == 2 && parts[1] == "package.json";
        }
        return false;
    }

    // Handle complex patterns with glob crate
    if let Ok(glob_pattern) = glob::Pattern::new(&format!("{}/package.json", pattern)) {
        return glob_pattern.matches(path_str);
    }

    false
}

fn remove_member_packages(
    files: &[FileInfo],
    member_indices: &[usize],
    packages: &mut Vec<Package>,
    dependencies: &mut Vec<TopLevelDependency>,
) {
    let member_paths: Vec<&str> = member_indices
        .iter()
        .map(|&idx| files[idx].path.as_str())
        .collect();

    let removed_uids: Vec<String> = packages
        .iter()
        .filter(|pkg| {
            pkg.datafile_paths
                .iter()
                .any(|dp| member_paths.contains(&dp.as_str()))
        })
        .map(|pkg| pkg.package_uid.clone())
        .collect();

    packages.retain(|pkg| !removed_uids.contains(&pkg.package_uid));
    dependencies.retain(|dep| {
        dep.for_package_uid
            .as_ref()
            .is_none_or(|uid| !removed_uids.contains(uid))
    });
}

fn remove_root_package(
    root_file: &FileInfo,
    packages: &mut Vec<Package>,
    dependencies: &mut Vec<TopLevelDependency>,
) {
    let root_purl = root_file
        .package_data
        .iter()
        .find(|pkg| pkg.datasource_id == Some(DatasourceId::NpmPackageJson))
        .and_then(|pkg| pkg.purl.as_ref())
        .cloned();

    let Some(purl) = root_purl else {
        return;
    };

    let mut removed_uid = None;
    packages.retain(|pkg| {
        if pkg.purl.as_ref() == Some(&purl) {
            removed_uid = Some(pkg.package_uid.clone());
            false
        } else {
            true
        }
    });

    if let Some(uid) = &removed_uid {
        dependencies.retain(|dep| dep.for_package_uid.as_ref() != Some(uid));
    }
}

fn remove_root_level_dependencies(dependencies: &mut Vec<TopLevelDependency>, root_dir: &Path) {
    dependencies.retain(|dependency| {
        let path = Path::new(&dependency.datafile_path);
        let is_root_level = path.parent() == Some(root_dir);
        let is_workspace_root_datasource = matches!(
            dependency.datasource_id,
            DatasourceId::NpmPackageJson
                | DatasourceId::BunLock
                | DatasourceId::BunLockb
                | DatasourceId::NpmPackageLockJson
                | DatasourceId::YarnLock
                | DatasourceId::YarnLockV1
                | DatasourceId::YarnLockV2
                | DatasourceId::PnpmLockYaml
        );

        !(is_root_level && is_workspace_root_datasource)
    });
}

fn create_root_package(
    files: &[FileInfo],
    root_file_indices: &[usize],
) -> Option<(Package, Vec<TopLevelDependency>)> {
    let (package, dependencies, _) =
        sibling_merge::assemble_siblings(npm_family_assembler_config(), files, root_file_indices)?;

    package.map(|package| (package, dependencies))
}

/// Create Package instances for each workspace member
fn create_member_packages(
    files: &[FileInfo],
    members: &[NpmWorkspaceMemberDomain],
) -> Vec<(Package, Vec<TopLevelDependency>)> {
    let mut results = Vec::new();

    for member in members {
        let file = &files[member.manifest_idx];

        // Find the first valid PackageData
        let pkg_data = if let Some(pkg) = file.package_data.iter().find(|pkg| {
            pkg.datasource_id == Some(DatasourceId::NpmPackageJson) && pkg.purl.is_some()
        }) {
            pkg
        } else {
            continue;
        };

        let datafile_path = file.path.clone();
        let datasource_id = DatasourceId::NpmPackageJson;
        let package = Package::from_package_data(pkg_data, datafile_path.clone());
        let for_package_uid = Some(package.package_uid.clone());

        // Collect dependencies
        let deps: Vec<TopLevelDependency> = pkg_data
            .dependencies
            .iter()
            .filter(|dep| dep.purl.is_some())
            .map(|dep| {
                TopLevelDependency::from_dependency(
                    dep,
                    datafile_path.clone(),
                    datasource_id,
                    for_package_uid.clone(),
                )
            })
            .collect();

        results.push((package, deps));
    }

    results
}

/// Hoist root package.json dependencies to workspace level.
///
/// If `for_package_uid` is Some, deps are assigned to that package (pnpm root).
/// If None, deps are workspace-level with no owning package.
fn hoist_root_dependencies(
    files: &[FileInfo],
    root_idx: usize,
    root_dir: &Path,
    dependencies: &mut Vec<TopLevelDependency>,
    member_versions: &HashMap<String, String>,
    for_package_uid: Option<&str>,
) {
    let root_file = &files[root_idx];

    // Find root PackageData
    let root_pkg_data = if let Some(pkg) = root_file
        .package_data
        .iter()
        .find(|pkg| pkg.datasource_id == Some(DatasourceId::NpmPackageJson))
    {
        pkg
    } else {
        return;
    };

    for dep in &root_pkg_data.dependencies {
        if dep.purl.is_some() {
            let mut top_dep = TopLevelDependency::from_dependency(
                dep,
                root_file.path.clone(),
                DatasourceId::NpmPackageJson,
                for_package_uid.map(|s| s.to_string()),
            );

            // Resolve workspace: version immediately
            if let Some(req) = &top_dep.extracted_requirement
                && req.starts_with("workspace:")
                && let Some(resolved) =
                    resolve_workspace_requirement(req, &top_dep.purl, member_versions)
            {
                top_dep.extracted_requirement = Some(resolved);
            }

            dependencies.push(top_dep);
        }
    }

    // Also hoist lockfile dependencies if they exist
    for file in files.iter() {
        let path = Path::new(&file.path);

        // Check if this is a lockfile in the same directory as root
        if path.parent() != Some(root_dir) {
            continue;
        }

        let file_name = if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            name
        } else {
            continue;
        };

        let matches_datasource = |datasource_id: DatasourceId| match file_name {
            "bun.lock" => datasource_id == DatasourceId::BunLock,
            "bun.lockb" => datasource_id == DatasourceId::BunLockb,
            ".package-lock.json" | "package-lock.json" | ".npm-shrinkwrap.json" => {
                datasource_id == DatasourceId::NpmPackageLockJson
            }
            "yarn.lock" => matches!(
                datasource_id,
                DatasourceId::YarnLock | DatasourceId::YarnLockV1 | DatasourceId::YarnLockV2
            ),
            "pnpm-lock.yaml" | "shrinkwrap.yaml" => datasource_id == DatasourceId::PnpmLockYaml,
            _ => false,
        };

        for pkg_data in &file.package_data {
            let Some(dsid) = pkg_data.datasource_id else {
                continue;
            };

            if !matches_datasource(dsid) {
                continue;
            }

            for dep in &pkg_data.dependencies {
                if dep.purl.is_some() {
                    let mut top_dep = TopLevelDependency::from_dependency(
                        dep,
                        file.path.clone(),
                        dsid,
                        for_package_uid.map(|s| s.to_string()),
                    );

                    // Resolve workspace: version
                    if let Some(req) = &top_dep.extracted_requirement
                        && req.starts_with("workspace:")
                        && let Some(resolved) =
                            resolve_workspace_requirement(req, &top_dep.purl, member_versions)
                    {
                        top_dep.extracted_requirement = Some(resolved);
                    }

                    dependencies.push(top_dep);
                }
            }
        }
    }
}

/// Assign for_packages to all files under the workspace.
///
/// For pnpm workspaces with a root package (`root_package_uid` is Some),
/// shared files are assigned to the root package only.
/// For npm/yarn workspaces, shared files are assigned to all member packages.
fn assign_for_packages(
    files: &mut [FileInfo],
    workspace_root: &NpmWorkspaceDomain,
    member_uids: &[String],
    root_package_uid: Option<&str>,
) {
    let workspace_root_str = workspace_root.root_dir.to_string_lossy().into_owned();
    let mut member_dirs: HashMap<String, String> = HashMap::new();
    for (member, uid) in workspace_root.members.iter().zip(member_uids.iter()) {
        if let Some(relative_path) =
            strip_root_prefix(&files[member.manifest_idx].path, &workspace_root_str)
        {
            member_dirs.insert(parent_dir(relative_path).to_string(), uid.clone());
        }
    }

    for file in files.iter_mut() {
        let Some(relative_path) = strip_root_prefix(&file.path, &workspace_root_str) else {
            continue;
        };

        // Clear stale for_packages assignments from sibling merge
        file.for_packages.clear();

        // Check if file is under a member's subdirectory
        if let Some(member_uid) = find_nearest_member_dir(relative_path, &member_dirs) {
            file.for_packages.push(member_uid);
            continue;
        }

        // Skip node_modules at workspace root level
        if relative_path
            .split('/')
            .next()
            .is_some_and(|component| component == "node_modules")
        {
            continue;
        }

        // Shared file: assign to root package (pnpm) or all members (npm/yarn)
        if let Some(root_uid) = root_package_uid {
            file.for_packages.push(root_uid.to_string());
        } else {
            for uid in member_uids {
                file.for_packages.push(uid.clone());
            }
        }
    }
}

fn find_nearest_member_dir(path: &str, member_dirs: &HashMap<String, String>) -> Option<String> {
    let mut current = Some(path);

    while let Some(candidate) = current {
        if let Some(uid) = member_dirs.get(candidate) {
            return Some(uid.clone());
        }

        current = parent_dir_for_lookup(candidate);
    }

    None
}

fn strip_root_prefix<'a>(path: &'a str, root: &str) -> Option<&'a str> {
    if root.is_empty() {
        return Some(path);
    }

    if path == root {
        return Some("");
    }

    path.strip_prefix(root)
        .and_then(|suffix| suffix.strip_prefix('/'))
}

fn npm_family_assembler_config() -> &'static AssemblerConfig {
    ASSEMBLERS
        .iter()
        .find(|config| {
            config
                .datasource_ids
                .contains(&DatasourceId::NpmPackageJson)
        })
        .expect("npm family assembler config must exist")
}

/// Resolve workspace: version references in all dependencies
fn resolve_workspace_versions(
    dependencies: &mut [TopLevelDependency],
    member_versions: &HashMap<String, String>,
) {
    for dep in dependencies {
        if let Some(req) = &dep.extracted_requirement
            && req.starts_with("workspace:")
            && let Some(resolved) = resolve_workspace_requirement(req, &dep.purl, member_versions)
        {
            dep.extracted_requirement = Some(resolved);
        }
    }
}

/// Resolve a single workspace: requirement to actual version
fn resolve_workspace_requirement(
    requirement: &str,
    dep_purl: &Option<String>,
    member_versions: &HashMap<String, String>,
) -> Option<String> {
    // Extract the package name from the purl
    let package_name = dep_purl
        .as_ref()
        .and_then(|purl| extract_package_name_from_purl(purl))?;

    // Look up the version
    let version = member_versions.get(&package_name)?;

    // Extract operator from workspace: prefix
    let workspace_spec = requirement.strip_prefix("workspace:")?;

    if workspace_spec == "*" || workspace_spec.is_empty() {
        // workspace:* or workspace: → use exact version
        Some(version.clone())
    } else if let Some(op) = workspace_spec.chars().next() {
        // workspace:^ → ^1.2.3
        // workspace:~ → ~1.2.3
        // workspace:>= → >=1.2.3
        if op == '^' || op == '~' || op == '>' || op == '<' || op == '=' {
            Some(format!("{}{}", workspace_spec, version))
        } else {
            // workspace:1.2.3 → use as-is
            Some(workspace_spec.to_string())
        }
    } else {
        Some(version.clone())
    }
}

fn extract_package_name_from_purl(purl: &str) -> Option<String> {
    let after_type = purl.strip_prefix("pkg:npm/")?;
    let without_query = after_type.split('?').next().unwrap_or(after_type);

    // The @ version separator is always a literal @, never URL-encoded.
    // Scoped package names use %40 for @, so rfind('@') safely finds only the version separator.
    let name_part = if let Some(at_pos) = without_query.rfind('@') {
        if at_pos > 0 {
            &without_query[..at_pos]
        } else {
            without_query
        }
    } else {
        without_query
    };

    let decoded = name_part
        .replace("%40", "@")
        .replace("%2F", "/")
        .replace("%2f", "/");

    Some(decoded)
}

fn workspace_member_name(package: &Package) -> Option<String> {
    match (package.namespace.as_deref(), package.name.as_deref()) {
        (Some(namespace), Some(name)) => Some(format!("{namespace}/{name}")),
        (None, Some(name)) => Some(name.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::PackageType;

    #[test]
    fn test_matches_workspace_pattern_exact() {
        let path = Path::new("packages/foo/package.json");
        assert!(matches_workspace_pattern(path, "packages/foo"));
        assert!(!matches_workspace_pattern(path, "packages/bar"));
    }

    #[test]
    fn test_matches_workspace_pattern_single_star() {
        let path = Path::new("packages/foo/package.json");
        assert!(matches_workspace_pattern(path, "packages/*"));

        let nested = Path::new("packages/foo/bar/package.json");
        assert!(!matches_workspace_pattern(nested, "packages/*"));

        let wrong_dir = Path::new("apps/foo/package.json");
        assert!(!matches_workspace_pattern(wrong_dir, "packages/*"));
    }

    #[test]
    fn test_matches_workspace_pattern_double_star() {
        let path = Path::new("packages/foo/package.json");
        assert!(matches_workspace_pattern(path, "packages/*"));

        let nested = Path::new("packages/foo/bar/package.json");
        assert!(matches_workspace_pattern(nested, "packages/**"));
    }

    #[test]
    fn test_extract_package_name_from_purl() {
        assert_eq!(
            extract_package_name_from_purl("pkg:npm/lodash@4.17.21"),
            Some("lodash".to_string())
        );
        assert_eq!(
            extract_package_name_from_purl("pkg:npm/@types/node@18.0.0"),
            Some("@types/node".to_string())
        );
        assert_eq!(
            extract_package_name_from_purl("pkg:npm/package@1.0.0?uuid=abc"),
            Some("package".to_string())
        );
        assert_eq!(extract_package_name_from_purl("pkg:pypi/django@3.2"), None);
        assert_eq!(
            extract_package_name_from_purl("pkg:npm/%40myorg%2Fcore"),
            Some("@myorg/core".to_string())
        );
        assert_eq!(
            extract_package_name_from_purl("pkg:npm/%40myorg%2Fcore@1.0.0"),
            Some("@myorg/core".to_string())
        );
        assert_eq!(
            extract_package_name_from_purl("pkg:npm/simple-pkg"),
            Some("simple-pkg".to_string())
        );
    }

    #[test]
    fn test_resolve_workspace_requirement() {
        let mut versions = HashMap::new();
        versions.insert("my-package".to_string(), "1.2.3".to_string());
        versions.insert("@myorg/core".to_string(), "1.0.0".to_string());

        let purl = Some("pkg:npm/my-package@1.2.3".to_string());

        assert_eq!(
            resolve_workspace_requirement("workspace:*", &purl, &versions),
            Some("1.2.3".to_string())
        );
        assert_eq!(
            resolve_workspace_requirement("workspace:^", &purl, &versions),
            Some("^1.2.3".to_string())
        );
        assert_eq!(
            resolve_workspace_requirement("workspace:~", &purl, &versions),
            Some("~1.2.3".to_string())
        );
        assert_eq!(
            resolve_workspace_requirement("workspace:", &purl, &versions),
            Some("1.2.3".to_string())
        );

        let scoped_purl = Some("pkg:npm/%40myorg%2Fcore@1.0.0".to_string());
        assert_eq!(
            resolve_workspace_requirement("workspace:^", &scoped_purl, &versions),
            Some("^1.0.0".to_string())
        );
    }

    #[test]
    fn test_extract_workspaces() {
        let mut extra_data = std::collections::HashMap::new();
        extra_data.insert(
            "workspaces".to_string(),
            serde_json::json!(["packages/*", "apps/*"]),
        );

        let pkg_data = PackageData {
            package_type: Some(PackageType::Npm),
            datasource_id: Some(DatasourceId::NpmPackageJson),
            extra_data: Some(extra_data),
            ..Default::default()
        };

        let workspaces = extract_workspaces(&pkg_data).unwrap();
        assert_eq!(workspaces.len(), 2);
        assert_eq!(workspaces[0], "packages/*");
        assert_eq!(workspaces[1], "apps/*");
    }

    #[test]
    fn test_extract_workspaces_string() {
        let pkg_data = PackageData {
            package_type: Some(PackageType::Npm),
            datasource_id: Some(DatasourceId::NpmPackageJson),
            extra_data: Some(std::collections::HashMap::from([(
                "workspaces".to_string(),
                serde_json::Value::String("packages/*".to_string()),
            )])),
            ..Default::default()
        };

        let workspaces = extract_workspaces(&pkg_data).unwrap();
        assert_eq!(workspaces, vec!["packages/*"]);
    }

    #[test]
    fn test_extract_workspaces_object_packages() {
        let pkg_data = PackageData {
            package_type: Some(PackageType::Npm),
            datasource_id: Some(DatasourceId::NpmPackageJson),
            extra_data: Some(std::collections::HashMap::from([(
                "workspaces".to_string(),
                serde_json::json!({ "packages": ["packages/*", "apps/*"] }),
            )])),
            ..Default::default()
        };

        let workspaces = extract_workspaces(&pkg_data).unwrap();
        assert_eq!(workspaces, vec!["packages/*", "apps/*"]);
    }

    #[test]
    fn test_extract_workspaces_empty() {
        let pkg_data = PackageData {
            package_type: Some(PackageType::Npm),
            datasource_id: Some(DatasourceId::NpmPackageJson),
            ..Default::default()
        };

        assert_eq!(extract_workspaces(&pkg_data), None);
    }
}
