// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use log::debug;

use crate::models::{DatasourceId, FileInfo, Package, PackageData, PackageUid, TopLevelDependency};

pub(super) struct CargoWorkspaceRootHint {
    pub(super) root_dir: PathBuf,
    pub(super) root_cargo_toml_idx: usize,
    pub(super) root_cargo_lock_idx: Option<usize>,
    pub(super) members: Vec<String>,
    pub(super) workspace_data: WorkspaceData,
}

pub(super) struct CargoWorkspaceMemberDomain {
    pub(super) manifest_idx: usize,
    pub(super) cargo_lock_idx: Option<usize>,
    pub(super) dir_path: PathBuf,
}

pub(super) struct CargoWorkspaceDomain {
    pub(super) root_dir: PathBuf,
    pub(super) root_cargo_toml_idx: usize,
    pub(super) root_cargo_lock_idx: Option<usize>,
    pub(super) members: Vec<CargoWorkspaceMemberDomain>,
    pub(super) workspace_data: WorkspaceData,
}

pub(super) struct WorkspaceData {
    package: HashMap<String, serde_json::Value>,
    dependencies: HashMap<String, serde_json::Value>,
}

pub(super) fn collect_cargo_workspace_hints(files: &[FileInfo]) -> Vec<CargoWorkspaceRootHint> {
    let mut roots = Vec::new();

    for (idx, file) in files.iter().enumerate() {
        let path = Path::new(&file.path);
        let file_name = if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            name
        } else {
            continue;
        };

        if !file_name.eq_ignore_ascii_case("cargo.toml") {
            continue;
        }

        for pkg_data in &file.package_data {
            if pkg_data.datasource_id != Some(DatasourceId::CargoToml) {
                continue;
            }

            if let Some(workspace_info) = extract_workspace_info(pkg_data)
                && let Some(parent) = path.parent()
            {
                roots.push(CargoWorkspaceRootHint {
                    root_dir: parent.to_path_buf(),
                    root_cargo_toml_idx: idx,
                    root_cargo_lock_idx: find_cargo_lock_index(files, parent),
                    members: workspace_info.members,
                    workspace_data: workspace_info.data,
                });
            }
        }
    }

    roots
}

pub(super) fn plan_cargo_workspace_domains(
    files: &[FileInfo],
    _dir_files: &HashMap<PathBuf, Vec<usize>>,
    workspace_hints: &[&CargoWorkspaceRootHint],
) -> Vec<CargoWorkspaceDomain> {
    let mut domains = Vec::new();

    for workspace_hint in workspace_hints {
        let member_indices = discover_members(files, workspace_hint);

        if member_indices.is_empty() {
            debug!(
                "No workspace members found for patterns {:?} in {:?}",
                workspace_hint.members, workspace_hint.root_dir
            );
            continue;
        }

        let members = member_indices
            .into_iter()
            .map(|manifest_idx| {
                let dir_path = Path::new(&files[manifest_idx].path)
                    .parent()
                    .expect("cargo workspace member manifest must have a parent directory")
                    .to_path_buf();

                CargoWorkspaceMemberDomain {
                    manifest_idx,
                    cargo_lock_idx: find_cargo_lock_index(files, &dir_path),
                    dir_path,
                }
            })
            .collect();

        domains.push(CargoWorkspaceDomain {
            root_dir: workspace_hint.root_dir.clone(),
            root_cargo_toml_idx: workspace_hint.root_cargo_toml_idx,
            root_cargo_lock_idx: workspace_hint.root_cargo_lock_idx,
            members,
            workspace_data: WorkspaceData {
                package: workspace_hint.workspace_data.package.clone(),
                dependencies: workspace_hint.workspace_data.dependencies.clone(),
            },
        });
    }

    domains.sort_by(|left, right| left.root_dir.cmp(&right.root_dir));
    domains
}

fn find_cargo_lock_index(files: &[FileInfo], dir: &Path) -> Option<usize> {
    files.iter().position(|file| {
        let path = Path::new(&file.path);
        path.parent() == Some(dir)
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("cargo.lock"))
    })
}

struct WorkspaceInfo {
    members: Vec<String>,
    data: WorkspaceData,
}

fn extract_workspace_info(pkg_data: &PackageData) -> Option<WorkspaceInfo> {
    let extra_data = pkg_data.extra_data.as_ref()?;
    let workspace_value = extra_data.get("workspace")?;

    let members: Vec<String> = workspace_value
        .get("members")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    if members.is_empty() {
        return None;
    }

    let mut package_map = HashMap::new();
    if let Some(pkg_obj) = workspace_value.get("package").and_then(|v| v.as_object()) {
        for (key, value) in pkg_obj {
            package_map.insert(key.clone(), value.clone());
        }
    }

    let mut dependencies_map = HashMap::new();
    if let Some(deps_obj) = workspace_value
        .get("dependencies")
        .and_then(|v| v.as_object())
    {
        for (key, value) in deps_obj {
            dependencies_map.insert(key.clone(), value.clone());
        }
    }

    Some(WorkspaceInfo {
        members,
        data: WorkspaceData {
            package: package_map,
            dependencies: dependencies_map,
        },
    })
}

pub(super) fn apply_cargo_workspace_domain(
    workspace_root: &CargoWorkspaceDomain,
    files: &mut [FileInfo],
    packages: &mut Vec<Package>,
    dependencies: &mut Vec<TopLevelDependency>,
) {
    let keep_root_package = root_workspace_manifest_is_package(files, workspace_root);
    let mut root_package_uid = None;
    if !keep_root_package {
        remove_root_package(
            files,
            workspace_root.root_cargo_toml_idx,
            packages,
            dependencies,
        );
    } else if let Some((pkg, deps)) = create_root_package(files, workspace_root) {
        root_package_uid = Some(pkg.package_uid.clone());
        packages.push(pkg);
        dependencies.extend(deps);
    }
    remove_member_packages(
        files,
        &workspace_root
            .members
            .iter()
            .map(|member| member.manifest_idx)
            .collect::<Vec<_>>(),
        packages,
        dependencies,
    );

    let member_packages = create_member_packages(
        files,
        &workspace_root.members,
        &workspace_root.workspace_data,
    );

    let mut member_uids: Vec<PackageUid> = member_packages
        .iter()
        .map(|(pkg, _deps)| pkg.package_uid.clone())
        .collect();

    if let Some(root_uid) = &root_package_uid {
        member_uids.push(root_uid.clone());
    }

    for (pkg, deps) in member_packages {
        packages.push(pkg);
        dependencies.extend(deps);
    }

    if !keep_root_package && let Some(root_lock_idx) = workspace_root.root_cargo_lock_idx {
        hoist_root_lock_dependencies(files, root_lock_idx, dependencies);
    }

    assign_for_packages(
        files,
        workspace_root,
        &member_uids,
        root_package_uid.as_ref(),
    );
}

fn root_workspace_manifest_is_package(
    files: &[FileInfo],
    workspace_root: &CargoWorkspaceDomain,
) -> bool {
    files[workspace_root.root_cargo_toml_idx]
        .package_data
        .iter()
        .any(|pkg| pkg.datasource_id == Some(DatasourceId::CargoToml) && pkg.purl.is_some())
}

fn discover_members(files: &[FileInfo], workspace_root: &CargoWorkspaceRootHint) -> Vec<usize> {
    let mut member_indices = Vec::new();

    for (idx, file) in files.iter().enumerate() {
        let path = Path::new(&file.path);

        if !path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("cargo.toml"))
        {
            continue;
        }

        if !path.starts_with(&workspace_root.root_dir) {
            continue;
        }

        if idx == workspace_root.root_cargo_toml_idx {
            continue;
        }

        let has_valid_package = file
            .package_data
            .iter()
            .any(|pkg| pkg.datasource_id == Some(DatasourceId::CargoToml) && pkg.purl.is_some());
        if !has_valid_package {
            continue;
        }

        let relative_path = if let Ok(rel) = path.strip_prefix(&workspace_root.root_dir) {
            rel
        } else {
            continue;
        };

        let mut matched = false;
        for pattern in &workspace_root.members {
            if matches_member_pattern(relative_path, pattern) {
                matched = true;
                break;
            }
        }

        if matched {
            member_indices.push(idx);
        }
    }

    member_indices
}

fn matches_member_pattern(path: &Path, pattern: &str) -> bool {
    let parent_str = path
        .parent()
        .and_then(|parent| parent.to_str())
        .unwrap_or("");

    if !pattern.contains('*') {
        return parent_str == pattern;
    }

    if let Ok(glob_pattern) = glob::Pattern::new(pattern) {
        return glob_pattern.matches(parent_str);
    }

    false
}

fn remove_root_package(
    files: &[FileInfo],
    root_idx: usize,
    packages: &mut Vec<Package>,
    dependencies: &mut Vec<TopLevelDependency>,
) {
    let root_file = &files[root_idx];
    let root_purl = root_file
        .package_data
        .iter()
        .find(|pkg| pkg.datasource_id == Some(DatasourceId::CargoToml))
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

    let removed_uids: Vec<PackageUid> = packages
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

fn create_member_packages(
    files: &[FileInfo],
    members: &[CargoWorkspaceMemberDomain],
    workspace_data: &WorkspaceData,
) -> Vec<(Package, Vec<TopLevelDependency>)> {
    let mut results = Vec::new();

    for member in members {
        let file = &files[member.manifest_idx];

        let pkg_data =
            if let Some(pkg) = file.package_data.iter().find(|pkg| {
                pkg.datasource_id == Some(DatasourceId::CargoToml) && pkg.purl.is_some()
            }) {
                pkg
            } else {
                continue;
            };

        let mut resolved_pkg_data = pkg_data.clone();
        apply_workspace_inheritance(&mut resolved_pkg_data, workspace_data);

        let datafile_path = file.path.clone();
        let datasource_id = DatasourceId::CargoToml;
        let mut package = Package::from_package_data(&resolved_pkg_data, datafile_path.clone());

        let matched_lock_package_data = member
            .cargo_lock_idx
            .and_then(|idx| lock_package_data_for_member(files, idx, &resolved_pkg_data));

        let lock_dependencies_source = member.cargo_lock_idx.and_then(|idx| {
            matched_lock_package_data.or_else(|| first_cargo_lock_package_data(files, idx))
        });

        if let Some(lock_pkg_data) = matched_lock_package_data {
            package.update(
                lock_pkg_data,
                files[member.cargo_lock_idx.expect("lock index")]
                    .path
                    .clone(),
            );
        }

        let for_package_uid = Some(package.package_uid.clone());

        let mut deps: Vec<TopLevelDependency> = resolved_pkg_data
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

        if let Some(lock_idx) = member.cargo_lock_idx
            && let Some(lock_pkg_data) = lock_dependencies_source
        {
            deps.extend(
                lock_pkg_data
                    .dependencies
                    .iter()
                    .filter(|dep| dep.purl.is_some())
                    .map(|dep| {
                        TopLevelDependency::from_dependency(
                            dep,
                            files[lock_idx].path.clone(),
                            DatasourceId::CargoLock,
                            for_package_uid.clone(),
                        )
                    }),
            );
        }

        results.push((package, deps));
    }

    results
}

fn create_root_package(
    files: &[FileInfo],
    workspace_root: &CargoWorkspaceDomain,
) -> Option<(Package, Vec<TopLevelDependency>)> {
    let file = &files[workspace_root.root_cargo_toml_idx];
    let pkg_data = file
        .package_data
        .iter()
        .find(|pkg| pkg.datasource_id == Some(DatasourceId::CargoToml) && pkg.purl.is_some())?;

    let mut resolved_pkg_data = pkg_data.clone();
    apply_workspace_inheritance(&mut resolved_pkg_data, &workspace_root.workspace_data);

    let datafile_path = file.path.clone();
    let datasource_id = DatasourceId::CargoToml;
    let mut package = Package::from_package_data(&resolved_pkg_data, datafile_path.clone());

    let matched_lock_package_data = workspace_root
        .root_cargo_lock_idx
        .and_then(|idx| lock_package_data_for_member(files, idx, &resolved_pkg_data));

    let lock_dependencies_source = workspace_root.root_cargo_lock_idx.and_then(|idx| {
        matched_lock_package_data.or_else(|| first_cargo_lock_package_data(files, idx))
    });

    if let Some(lock_pkg_data) = matched_lock_package_data {
        package.update(
            lock_pkg_data,
            files[workspace_root.root_cargo_lock_idx.expect("lock index")]
                .path
                .clone(),
        );
    }

    let for_package_uid = Some(package.package_uid.clone());
    let mut deps: Vec<TopLevelDependency> = resolved_pkg_data
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

    if let Some(lock_idx) = workspace_root.root_cargo_lock_idx
        && let Some(lock_pkg_data) = lock_dependencies_source
    {
        deps.extend(
            lock_pkg_data
                .dependencies
                .iter()
                .filter(|dep| dep.purl.is_some())
                .map(|dep| {
                    TopLevelDependency::from_dependency(
                        dep,
                        files[lock_idx].path.clone(),
                        DatasourceId::CargoLock,
                        for_package_uid.clone(),
                    )
                }),
        );
    }

    Some((package, deps))
}

fn lock_package_data_for_member<'a>(
    files: &'a [FileInfo],
    lock_idx: usize,
    pkg_data: &PackageData,
) -> Option<&'a PackageData> {
    files[lock_idx].package_data.iter().find(|lock_pkg_data| {
        lock_pkg_data.datasource_id == Some(DatasourceId::CargoLock)
            && cargo_package_identity_matches(lock_pkg_data, pkg_data)
    })
}

fn first_cargo_lock_package_data(files: &[FileInfo], lock_idx: usize) -> Option<&PackageData> {
    files[lock_idx]
        .package_data
        .iter()
        .find(|lock_pkg_data| lock_pkg_data.datasource_id == Some(DatasourceId::CargoLock))
}

fn cargo_package_identity_matches(left: &PackageData, right: &PackageData) -> bool {
    left.name == right.name && left.version == right.version
}

fn hoist_root_lock_dependencies(
    files: &[FileInfo],
    root_lock_idx: usize,
    dependencies: &mut Vec<TopLevelDependency>,
) {
    let file = &files[root_lock_idx];

    for pkg_data in &file.package_data {
        if pkg_data.datasource_id != Some(DatasourceId::CargoLock) {
            continue;
        }

        dependencies.extend(
            pkg_data
                .dependencies
                .iter()
                .filter(|dep| dep.purl.is_some())
                .map(|dep| {
                    TopLevelDependency::from_dependency(
                        dep,
                        file.path.clone(),
                        DatasourceId::CargoLock,
                        None,
                    )
                }),
        );
    }
}

fn apply_workspace_inheritance(pkg_data: &mut PackageData, workspace_data: &WorkspaceData) {
    use packageurl::PackageUrl;

    let extra_data = if let Some(ed) = &mut pkg_data.extra_data {
        ed
    } else {
        return;
    };

    if extra_data.get("version").and_then(|v| v.as_str()) == Some("workspace")
        && let Some(version_value) = workspace_data.package.get("version")
        && let Some(version_str) = version_value.as_str()
    {
        pkg_data.version = Some(version_str.to_string());
        extra_data.remove("version");
    }

    if extra_data.get("license").and_then(|v| v.as_str()) == Some("workspace")
        && let Some(license_value) = workspace_data.package.get("license")
        && let Some(license_str) = license_value.as_str()
    {
        pkg_data.extracted_license_statement = Some(license_str.to_string());
        extra_data.remove("license");
    }

    if extra_data.get("homepage").and_then(|v| v.as_str()) == Some("workspace")
        && let Some(homepage_value) = workspace_data.package.get("homepage")
        && let Some(homepage_str) = homepage_value.as_str()
    {
        pkg_data.homepage_url = Some(homepage_str.to_string());
        extra_data.remove("homepage");
    }

    if extra_data.get("repository").and_then(|v| v.as_str()) == Some("workspace")
        && let Some(repo_value) = workspace_data.package.get("repository")
        && let Some(repo_str) = repo_value.as_str()
    {
        pkg_data.vcs_url = Some(repo_str.to_string());
        extra_data.remove("repository");
    }

    if extra_data.get("categories").and_then(|v| v.as_str()) == Some("workspace")
        && let Some(categories_value) = workspace_data.package.get("categories")
        && let Some(categories_arr) = categories_value.as_array()
    {
        let categories: Vec<String> = categories_arr
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect();
        pkg_data.keywords.extend(categories);
        extra_data.remove("categories");
    }

    if extra_data.get("edition").and_then(|v| v.as_str()) == Some("workspace")
        && let Some(edition_value) = workspace_data.package.get("edition")
        && let Some(edition_str) = edition_value.as_str()
    {
        extra_data.insert("rust_edition".to_string(), serde_json::json!(edition_str));
        extra_data.remove("edition");
    }

    if extra_data.get("rust-version").and_then(|v| v.as_str()) == Some("workspace")
        && let Some(rust_version_value) = workspace_data.package.get("rust-version")
        && let Some(rust_version_str) = rust_version_value.as_str()
    {
        extra_data.insert(
            "rust_version".to_string(),
            serde_json::json!(rust_version_str),
        );
        extra_data.remove("rust-version");
    }

    if extra_data.get("authors").and_then(|v| v.as_str()) == Some("workspace")
        && let Some(authors_value) = workspace_data.package.get("authors")
        && let Some(authors_arr) = authors_value.as_array()
    {
        use crate::parsers::utils::split_name_email;
        let parties: Vec<crate::models::Party> = authors_arr
            .iter()
            .filter_map(|v| v.as_str())
            .map(|author_str| {
                let (name, email) = split_name_email(author_str);
                crate::models::Party {
                    r#type: None,
                    role: Some("author".to_string()),
                    name,
                    email,
                    url: None,
                    organization: None,
                    organization_url: None,
                    timezone: None,
                }
            })
            .collect();
        pkg_data.parties = parties;
        extra_data.remove("authors");
    }

    if extra_data.get("readme").and_then(|v| v.as_str()) == Some("workspace")
        && let Some(readme_value) = workspace_data.package.get("readme")
        && let Some(readme_str) = readme_value.as_str()
    {
        extra_data.insert("readme_file".to_string(), serde_json::json!(readme_str));
        extra_data.remove("readme");
    }

    if let (Some(name), Some(version)) = (&pkg_data.name, &pkg_data.version)
        && let Ok(purl) = PackageUrl::new("cargo", name)
    {
        let mut purl = purl;
        let _ = purl.with_version(version);
        pkg_data.purl = Some(purl.to_string());

        pkg_data.repository_download_url = Some(format!(
            "https://crates.io/api/v1/crates/{}/{}/download",
            name, version
        ));
    }

    for dep in &mut pkg_data.dependencies {
        if let Some(dep_extra) = &dep.extra_data
            && dep_extra.get("workspace").and_then(|v| v.as_bool()) == Some(true)
        {
            let dep_name = if let Some(purl_str) = &dep.purl {
                extract_cargo_dep_name(purl_str)
            } else {
                None
            };

            if let Some(dep_name) = dep_name
                && let Some(dep_value) = workspace_data.dependencies.get(&dep_name)
            {
                if let Some(version_str) = dep_value.as_str() {
                    dep.extracted_requirement = Some(version_str.to_string());
                } else if let Some(dep_obj) = dep_value.as_object()
                    && let Some(version_str) = dep_obj.get("version").and_then(|v| v.as_str())
                {
                    dep.extracted_requirement = Some(version_str.to_string());
                }
            }
        }
    }
}

fn extract_cargo_dep_name(purl: &str) -> Option<String> {
    let after_type = purl.strip_prefix("pkg:cargo/")?;
    let without_query = after_type.split('?').next().unwrap_or(after_type);
    let name_part = without_query.split('@').next().unwrap_or(without_query);
    Some(name_part.to_string())
}

fn assign_for_packages(
    files: &mut [FileInfo],
    workspace_root: &CargoWorkspaceDomain,
    member_uids: &[PackageUid],
    root_package_uid: Option<&PackageUid>,
) {
    let mut member_dirs: Vec<PathBuf> = Vec::new();
    member_dirs.extend(
        workspace_root
            .members
            .iter()
            .map(|member| member.dir_path.clone()),
    );

    for file in files.iter_mut() {
        let path = Path::new(&file.path);
        if !path.starts_with(&workspace_root.root_dir) {
            continue;
        }

        file.for_packages.clear();

        let mut assigned = false;
        for (i, member_dir) in member_dirs.iter().enumerate() {
            if path.starts_with(member_dir) {
                file.for_packages.push(member_uids[i].clone());
                assigned = true;
                break;
            }
        }

        if assigned {
            continue;
        }

        if let Ok(rel) = path.strip_prefix(&workspace_root.root_dir)
            && let Some(first_component) = rel.components().next()
            && first_component.as_os_str() == "target"
        {
            continue;
        }

        if let Some(root_uid) = root_package_uid {
            file.for_packages.push(root_uid.clone());
            continue;
        }

        for uid in member_uids {
            file.for_packages.push(uid.clone());
        }
    }
}
