use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;

use crate::models::{DatasourceId, FileInfo, Package, TopLevelDependency};
use packageurl::PackageUrl;

struct DbPathConfig {
    datasource_ids: &'static [DatasourceId],
    path_suffix: &'static str,
}

const DB_PATH_CONFIGS: &[DbPathConfig] = &[
    DbPathConfig {
        datasource_ids: &[DatasourceId::AlpineInstalledDb],
        path_suffix: "lib/apk/db/installed",
    },
    DbPathConfig {
        datasource_ids: &[DatasourceId::RpmInstalledDatabaseBdb],
        path_suffix: "var/lib/rpm/Packages",
    },
    DbPathConfig {
        datasource_ids: &[DatasourceId::RpmInstalledDatabaseNdb],
        path_suffix: "usr/lib/sysimage/rpm/Packages.db",
    },
    DbPathConfig {
        datasource_ids: &[DatasourceId::RpmInstalledDatabaseSqlite],
        path_suffix: "usr/lib/sysimage/rpm/rpmdb.sqlite",
    },
    DbPathConfig {
        datasource_ids: &[DatasourceId::DebianInstalledStatusDb],
        path_suffix: "var/lib/dpkg/status",
    },
    DbPathConfig {
        datasource_ids: &[DatasourceId::DebianDistrolessInstalledDb],
        path_suffix: "var/lib/dpkg/status.d/",
    },
];

const RPM_DATASOURCE_IDS: &[DatasourceId] = &[
    DatasourceId::RpmInstalledDatabaseBdb,
    DatasourceId::RpmInstalledDatabaseNdb,
    DatasourceId::RpmInstalledDatabaseSqlite,
];
const RPM_YUMDB_PATH_SUFFIX: &str = "var/lib/yum/yumdb/";
const CONDA_META_PATH_SEGMENT: &str = "conda-meta/";
const PYTHON_METADATA_DATASOURCE_IDS: &[DatasourceId] = &[
    DatasourceId::PypiWheelMetadata,
    DatasourceId::PypiSdistPkginfo,
];
const PYTHON_SITE_PACKAGES_SEGMENTS: &[&str] = &["site-packages/", "dist-packages/"];
const DEBIAN_INSTALLED_SUPPLEMENTAL_DATASOURCE_IDS: &[DatasourceId] = &[
    DatasourceId::DebianInstalledFilesList,
    DatasourceId::DebianInstalledMd5Sums,
];

struct PythonMetadataResolution {
    base_path: String,
    allowed_root: String,
}

pub fn resolve_file_references(
    files: &mut [FileInfo],
    packages: &mut [Package],
    dependencies: &mut [TopLevelDependency],
) {
    let path_index = build_path_index(&*files);

    for package in packages.iter_mut() {
        if package.datasource_ids.contains(&DatasourceId::AboutFile)
            && let Some(datafile_path) = package.datafile_paths.first()
        {
            let root = Path::new(datafile_path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            let file_references = collect_file_references(
                files,
                &path_index,
                datafile_path,
                &package.datasource_ids,
                &[DatasourceId::AboutFile],
                package.purl.as_deref(),
            );

            let mut missing_refs = Vec::new();
            for file_ref in &file_references {
                let resolved_path = if root.is_empty() {
                    file_ref.path.clone()
                } else {
                    format!("{}/{}", root, file_ref.path.trim_start_matches('/'))
                };
                if let Some(&file_idx) = path_index.get(&resolved_path) {
                    let package_uid = package.package_uid.clone();
                    if !files[file_idx].for_packages.contains(&package_uid) {
                        files[file_idx].for_packages.push(package_uid);
                    }
                } else {
                    missing_refs.push(file_ref.path.clone());
                }
            }

            if !missing_refs.is_empty() {
                missing_refs.sort();
                let missing_refs_json: Vec<serde_json::Value> = missing_refs
                    .into_iter()
                    .map(|path| serde_json::json!({"path": path}))
                    .collect();

                let extra_data = package.extra_data.get_or_insert_with(HashMap::new);
                extra_data.insert(
                    "missing_file_references".to_string(),
                    serde_json::Value::Array(missing_refs_json),
                );
            }
            continue;
        }

        if is_conda_meta_package(package)
            && let Some(conda_meta_path) = package
                .datafile_paths
                .iter()
                .find(|path| path.contains(CONDA_META_PATH_SEGMENT))
            && let Some(root) = compute_conda_root(Some(conda_meta_path.as_str()))
        {
            let file_references = collect_file_references(
                files,
                &path_index,
                conda_meta_path,
                &package.datasource_ids,
                &[DatasourceId::CondaMetaJson],
                package.purl.as_deref(),
            );

            let mut missing_refs = Vec::new();
            for file_ref in &file_references {
                let resolved_path = format!("{}{}", root, file_ref.path.trim_start_matches('/'));
                if let Some(&file_idx) = path_index.get(&resolved_path) {
                    let package_uid = package.package_uid.clone();
                    if !files[file_idx].for_packages.contains(&package_uid) {
                        files[file_idx].for_packages.push(package_uid);
                    }
                } else {
                    missing_refs.push(file_ref.path.clone());
                }
            }

            if !missing_refs.is_empty() {
                missing_refs.sort();
                let missing_refs_json: Vec<serde_json::Value> = missing_refs
                    .into_iter()
                    .map(|path| serde_json::json!({"path": path}))
                    .collect();

                let extra_data = package.extra_data.get_or_insert_with(HashMap::new);
                extra_data.insert(
                    "missing_file_references".to_string(),
                    serde_json::Value::Array(missing_refs_json),
                );
            }
            continue;
        }

        if let Some(config) = find_db_config(package) {
            let datafile_path = match package.datafile_paths.first() {
                Some(path) => path,
                None => continue,
            };

            let root = compute_root(datafile_path, config.path_suffix);

            let mut file_references = collect_file_references(
                files,
                &path_index,
                datafile_path,
                &package.datasource_ids,
                config.datasource_ids,
                package.purl.as_deref(),
            );

            if is_debian_installed_package(package) {
                merge_file_references(
                    &mut file_references,
                    collect_debian_installed_file_references(files, package),
                );
            }

            let mut missing_refs = Vec::new();

            for file_ref in &file_references {
                let ref_path = file_ref.path.trim_start_matches('/');
                let resolved_path = if root.is_empty() {
                    ref_path.to_string()
                } else {
                    format!("{}{}", root, ref_path)
                };

                if let Some(&file_idx) = path_index.get(&resolved_path) {
                    let package_uid = package.package_uid.clone();
                    if !files[file_idx].for_packages.contains(&package_uid) {
                        files[file_idx].for_packages.push(package_uid);
                    }
                } else {
                    missing_refs.push(file_ref.path.clone());
                }
            }

            if !missing_refs.is_empty() {
                missing_refs.sort();
                let missing_refs_json: Vec<serde_json::Value> = missing_refs
                    .into_iter()
                    .map(|path| serde_json::json!({"path": path}))
                    .collect();

                let extra_data = package.extra_data.get_or_insert_with(HashMap::new);
                extra_data.insert(
                    "missing_file_references".to_string(),
                    serde_json::Value::Array(missing_refs_json),
                );
            }

            if is_rpm_package(package)
                && let Some(namespace) = resolve_rpm_namespace(files, &path_index, &root)
            {
                apply_rpm_namespace(files, package, dependencies, &namespace);
            }
            continue;
        }

        if let Some(python_resolution) = find_python_metadata_root(package) {
            let datafile_path = match package
                .datafile_paths
                .iter()
                .find(|path| is_python_metadata_layout(path))
            {
                Some(path) => path,
                None => continue,
            };

            let file_references = collect_file_references(
                files,
                &path_index,
                datafile_path,
                &package.datasource_ids,
                PYTHON_METADATA_DATASOURCE_IDS,
                package.purl.as_deref(),
            );

            let mut missing_refs = Vec::new();
            for file_ref in &file_references {
                let Some(resolved_path) = normalize_relative_path(
                    &python_resolution.base_path,
                    &python_resolution.allowed_root,
                    &file_ref.path,
                ) else {
                    missing_refs.push(file_ref.path.clone());
                    continue;
                };

                if let Some(&file_idx) = path_index.get(&resolved_path) {
                    let package_uid = package.package_uid.clone();
                    if !files[file_idx].for_packages.contains(&package_uid) {
                        files[file_idx].for_packages.push(package_uid);
                    }
                } else {
                    missing_refs.push(file_ref.path.clone());
                }
            }

            if !missing_refs.is_empty() {
                missing_refs.sort();
                let missing_refs_json: Vec<serde_json::Value> = missing_refs
                    .into_iter()
                    .map(|path| serde_json::json!({"path": path}))
                    .collect();

                let extra_data = package.extra_data.get_or_insert_with(HashMap::new);
                extra_data.insert(
                    "missing_file_references".to_string(),
                    serde_json::Value::Array(missing_refs_json),
                );
            }
        }
    }
}

fn is_python_metadata_layout(path: &str) -> bool {
    path.ends_with("/METADATA") || path.ends_with("/PKG-INFO")
}

fn find_python_metadata_root(package: &Package) -> Option<PythonMetadataResolution> {
    let datafile_path = package
        .datafile_paths
        .iter()
        .find(|path| is_python_metadata_layout(path))?;

    if !package
        .datasource_ids
        .iter()
        .any(|datasource_id| PYTHON_METADATA_DATASOURCE_IDS.contains(datasource_id))
    {
        return None;
    }

    for segment in PYTHON_SITE_PACKAGES_SEGMENTS {
        if let Some(idx) = datafile_path.rfind(segment) {
            if datafile_path.ends_with("/METADATA") {
                let root_end = idx + segment.len();
                let root = datafile_path[..root_end].to_string();
                return Some(PythonMetadataResolution {
                    base_path: root.clone(),
                    allowed_root: root,
                });
            }

            if datafile_path.ends_with("/PKG-INFO") {
                let parent = Path::new(datafile_path).parent()?;
                let allowed_root = datafile_path[..idx + segment.len()].to_string();
                return Some(PythonMetadataResolution {
                    base_path: parent.to_string_lossy().to_string(),
                    allowed_root,
                });
            }
        }
    }

    if datafile_path.ends_with(".egg-info/PKG-INFO") {
        let metadata_parent = Path::new(datafile_path).parent()?;
        let project_root = metadata_parent.parent()?;
        let project_root = project_root.to_string_lossy().to_string();
        return Some(PythonMetadataResolution {
            base_path: project_root.clone(),
            allowed_root: project_root,
        });
    }

    None
}

fn normalize_relative_path(base: &str, allowed_root: &str, relative: &str) -> Option<String> {
    let joined = Path::new(base).join(relative.trim_start_matches('/'));
    let mut normalized = Path::new("").to_path_buf();

    for component in joined.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }

    let normalized_str = normalized.to_string_lossy().to_string();
    if Path::new(&normalized_str).starts_with(Path::new(allowed_root)) {
        Some(normalized_str)
    } else {
        None
    }
}

fn is_conda_meta_package(package: &Package) -> bool {
    package
        .datasource_ids
        .contains(&DatasourceId::CondaMetaJson)
}

fn compute_conda_root(datafile_path: Option<&str>) -> Option<String> {
    let path = datafile_path?;
    let idx = path.rfind(CONDA_META_PATH_SEGMENT)?;
    Some(path[..idx].to_string())
}

pub fn merge_rpm_yumdb_metadata(files: &mut [FileInfo], packages: &mut Vec<Package>) {
    let yumdb_indices: Vec<usize> = packages
        .iter()
        .enumerate()
        .filter_map(|(idx, package)| {
            package
                .datasource_ids
                .contains(&DatasourceId::RpmYumdb)
                .then_some(idx)
        })
        .collect();
    let mut removal_indices = Vec::new();

    for yumdb_idx in yumdb_indices {
        let yumdb_package = packages[yumdb_idx].clone();
        let Some(yumdb_path) = yumdb_package.datafile_paths.first() else {
            continue;
        };
        let yumdb_root = compute_root(yumdb_path, RPM_YUMDB_PATH_SUFFIX);
        let yumdb_arch = yumdb_package
            .qualifiers
            .as_ref()
            .and_then(|qualifiers| qualifiers.get("arch"));

        let Some(target_idx) = packages.iter().enumerate().find_map(|(idx, package)| {
            if idx == yumdb_idx || !is_rpm_package(package) {
                return None;
            }

            let config = find_db_config(package)?;
            let datafile_path = package.datafile_paths.first()?;
            let target_root = compute_root(datafile_path, config.path_suffix);
            let target_arch = package
                .qualifiers
                .as_ref()
                .and_then(|qualifiers| qualifiers.get("arch"));

            (target_root == yumdb_root
                && package.name == yumdb_package.name
                && package.version == yumdb_package.version
                && target_arch == yumdb_arch)
                .then_some(idx)
        }) else {
            continue;
        };

        let target_package_uid = packages[target_idx].package_uid.clone();
        {
            let target = &mut packages[target_idx];
            target
                .datafile_paths
                .extend(yumdb_package.datafile_paths.clone());
            target
                .datasource_ids
                .extend(yumdb_package.datasource_ids.clone());

            if let Some(yumdb_extra) = yumdb_package.extra_data.clone()
                && !yumdb_extra.is_empty()
            {
                let extra_data = target.extra_data.get_or_insert_with(HashMap::new);
                let mut merged_yumdb = extra_data
                    .get("yumdb")
                    .and_then(|value| value.as_object().cloned())
                    .unwrap_or_default();
                for (key, value) in yumdb_extra {
                    merged_yumdb.insert(key, value);
                }
                extra_data.insert("yumdb".to_string(), serde_json::Value::Object(merged_yumdb));
            }
        }

        for file in files.iter_mut() {
            for package_uid in &mut file.for_packages {
                if *package_uid == yumdb_package.package_uid {
                    *package_uid = target_package_uid.clone();
                }
            }
        }

        removal_indices.push(yumdb_idx);
    }

    removal_indices.sort_unstable();
    removal_indices.dedup();
    for idx in removal_indices.into_iter().rev() {
        packages.remove(idx);
    }
}

fn build_path_index(files: &[FileInfo]) -> HashMap<String, usize> {
    files
        .iter()
        .enumerate()
        .map(|(idx, file)| (file.path.clone(), idx))
        .collect()
}

fn find_db_config(package: &Package) -> Option<&'static DbPathConfig> {
    for config in DB_PATH_CONFIGS {
        for &config_dsid in config.datasource_ids {
            for &pkg_dsid in &package.datasource_ids {
                if config_dsid == pkg_dsid {
                    return Some(config);
                }
            }
        }
    }
    None
}

fn compute_root(datafile_path: &str, suffix: &str) -> String {
    if let Some(pos) = datafile_path.rfind(suffix) {
        let root = &datafile_path[..pos];
        if root.is_empty() {
            String::new()
        } else {
            root.to_string()
        }
    } else {
        String::new()
    }
}

fn collect_file_references(
    files: &[FileInfo],
    path_index: &HashMap<String, usize>,
    datafile_path: &str,
    package_datasource_ids: &[DatasourceId],
    config_datasource_ids: &[DatasourceId],
    package_purl: Option<&str>,
) -> Vec<crate::models::FileReference> {
    let file_idx = match path_index.get(datafile_path) {
        Some(&idx) => idx,
        None => return Vec::new(),
    };

    let file = &files[file_idx];
    let mut refs = Vec::new();

    for pkg_data in &file.package_data {
        let dsid_matches = pkg_data.datasource_id.is_some_and(|dsid| {
            package_datasource_ids.contains(&dsid) || config_datasource_ids.contains(&dsid)
        });

        if !dsid_matches {
            continue;
        }

        let purl_matches = match (package_purl, pkg_data.purl.as_deref()) {
            (Some(pkg_purl), Some(data_purl)) => pkg_purl == data_purl,
            _ => true,
        };

        if purl_matches {
            refs.extend(pkg_data.file_references.clone());
        }
    }

    refs
}

fn is_rpm_package(package: &Package) -> bool {
    for &dsid in &package.datasource_ids {
        for &rpm_dsid in RPM_DATASOURCE_IDS {
            if rpm_dsid == dsid {
                return true;
            }
        }
    }
    false
}

fn is_debian_installed_package(package: &Package) -> bool {
    package
        .datasource_ids
        .contains(&DatasourceId::DebianInstalledStatusDb)
        || package
            .datasource_ids
            .contains(&DatasourceId::DebianDistrolessInstalledDb)
}

fn collect_debian_installed_file_references(
    files: &[FileInfo],
    package: &Package,
) -> Vec<crate::models::FileReference> {
    let mut refs = Vec::new();

    for file in files {
        for pkg_data in &file.package_data {
            let Some(dsid) = pkg_data.datasource_id else {
                continue;
            };
            if !DEBIAN_INSTALLED_SUPPLEMENTAL_DATASOURCE_IDS.contains(&dsid) {
                continue;
            }

            if pkg_data.name != package.name {
                continue;
            }
            if !debian_installed_namespace_matches(&pkg_data.namespace, &package.namespace) {
                continue;
            }
            if !debian_installed_arch_matches(&pkg_data.qualifiers, &package.qualifiers) {
                continue;
            }

            merge_file_references(&mut refs, pkg_data.file_references.clone());
        }
    }

    refs
}

fn debian_installed_namespace_matches(
    supplemental_namespace: &Option<String>,
    package_namespace: &Option<String>,
) -> bool {
    match (
        supplemental_namespace.as_deref(),
        package_namespace.as_deref(),
    ) {
        (None, _) => true,
        (Some("debian"), Some("ubuntu")) => true,
        (Some(left), Some(right)) => left == right,
        (Some(_), None) => true,
    }
}

fn debian_installed_arch_matches(
    supplemental_qualifiers: &Option<HashMap<String, String>>,
    package_qualifiers: &Option<HashMap<String, String>>,
) -> bool {
    let supplemental_arch = supplemental_qualifiers
        .as_ref()
        .and_then(|qualifiers| qualifiers.get("arch"));
    let package_arch = package_qualifiers
        .as_ref()
        .and_then(|qualifiers| qualifiers.get("arch"));

    match (supplemental_arch, package_arch) {
        (Some(left), Some(right)) => left == right,
        (Some(_), None) => false,
        _ => true,
    }
}

fn merge_file_references(
    target: &mut Vec<crate::models::FileReference>,
    incoming: Vec<crate::models::FileReference>,
) {
    for file_ref in incoming {
        if let Some(existing) = target
            .iter_mut()
            .find(|existing| existing.path == file_ref.path)
        {
            if existing.size.is_none() {
                existing.size = file_ref.size;
            }
            if existing.sha1.is_none() {
                existing.sha1 = file_ref.sha1.clone();
            }
            if existing.md5.is_none() {
                existing.md5 = file_ref.md5.clone();
            }
            if existing.sha256.is_none() {
                existing.sha256 = file_ref.sha256.clone();
            }
            if existing.sha512.is_none() {
                existing.sha512 = file_ref.sha512.clone();
            }
            if existing.extra_data.is_none() {
                existing.extra_data = file_ref.extra_data.clone();
            }
        } else {
            target.push(file_ref);
        }
    }
}

fn resolve_rpm_namespace(
    files: &[FileInfo],
    path_index: &HashMap<String, usize>,
    root: &str,
) -> Option<String> {
    let os_release_paths = [
        format!("{}etc/os-release", root),
        format!("{}usr/lib/os-release", root),
    ];

    for os_release_path in &os_release_paths {
        if let Some(&file_idx) = path_index.get(os_release_path) {
            let file = &files[file_idx];
            for pkg_data in &file.package_data {
                if pkg_data.datasource_id == Some(DatasourceId::EtcOsRelease)
                    && let Some(namespace) = &pkg_data.namespace
                {
                    return Some(namespace.clone());
                }
            }
        }
    }

    None
}

fn replace_uid_base(old_uid: &str, new_purl: &str) -> String {
    if let Some((_, suffix)) = old_uid.split_once("?uuid=") {
        return format!("{}?uuid={}", new_purl, suffix);
    }

    if let Some((_, suffix)) = old_uid.split_once("&uuid=") {
        let separator = if new_purl.contains('?') { '&' } else { '?' };
        return format!("{}{separator}uuid={suffix}", new_purl);
    }

    old_uid.to_string()
}

fn rewrite_purl_namespace(existing_purl: &str, namespace: &str) -> Option<String> {
    let parsed = PackageUrl::from_str(existing_purl).ok()?;
    let mut updated = PackageUrl::new(parsed.ty(), parsed.name()).ok()?;

    updated.with_namespace(namespace).ok()?;

    if let Some(version) = parsed.version() {
        updated.with_version(version).ok()?;
    }

    if let Some(subpath) = parsed.subpath() {
        updated.with_subpath(subpath).ok()?;
    }

    for (key, value) in parsed.qualifiers() {
        updated
            .add_qualifier(key.to_string(), value.to_string())
            .ok()?;
    }

    Some(updated.to_string())
}

fn apply_rpm_namespace(
    files: &mut [FileInfo],
    package: &mut Package,
    dependencies: &mut [TopLevelDependency],
    namespace: &str,
) {
    let old_package_uid = package.package_uid.clone();

    package.namespace = Some(namespace.to_string());

    if let Some(current_purl) = package.purl.as_deref()
        && let Some(updated_purl) = rewrite_purl_namespace(current_purl, namespace)
    {
        package.purl = Some(updated_purl.clone());
        package.package_uid = replace_uid_base(&old_package_uid, &updated_purl);
    }

    for file in files.iter_mut() {
        for package_uid in &mut file.for_packages {
            if *package_uid == old_package_uid {
                *package_uid = package.package_uid.clone();
            }
        }
    }

    for dep in dependencies.iter_mut() {
        if dep.for_package_uid.as_deref() == Some(old_package_uid.as_str()) {
            dep.for_package_uid = Some(package.package_uid.clone());
        }

        if dep.for_package_uid.as_deref() == Some(package.package_uid.as_str()) {
            dep.namespace = Some(namespace.to_string());

            if let Some(current_purl) = dep.purl.as_deref()
                && let Some(updated_purl) = rewrite_purl_namespace(current_purl, namespace)
            {
                dep.purl = Some(updated_purl.clone());
                dep.dependency_uid = replace_uid_base(&dep.dependency_uid, &updated_purl);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{FileReference, FileType, PackageData, PackageType};

    #[test]
    fn test_find_root_from_path() {
        assert_eq!(
            compute_root("rootfs/lib/apk/db/installed", "lib/apk/db/installed"),
            "rootfs/"
        );
        assert_eq!(
            compute_root("lib/apk/db/installed", "lib/apk/db/installed"),
            ""
        );
        assert_eq!(
            compute_root("container/var/lib/rpm/Packages", "var/lib/rpm/Packages"),
            "container/"
        );
        assert_eq!(
            compute_root("var/lib/rpm/Packages", "var/lib/rpm/Packages"),
            ""
        );
    }

    #[test]
    fn test_resolve_basic_alpine() {
        let mut files = vec![
            FileInfo {
                name: "installed".to_string(),
                base_name: "installed".to_string(),
                extension: String::new(),
                path: "lib/apk/db/installed".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 100,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![PackageData {
                    datasource_id: Some(DatasourceId::AlpineInstalledDb),
                    purl: Some("pkg:alpine/musl@1.2.3".to_string()),
                    name: Some("musl".to_string()),
                    file_references: vec![
                        FileReference {
                            path: "lib/libc.so".to_string(),
                            size: None,
                            sha1: None,
                            md5: None,
                            sha256: None,
                            sha512: None,
                            extra_data: None,
                        },
                        FileReference {
                            path: "usr/bin/ldconfig".to_string(),
                            size: None,
                            sha1: None,
                            md5: None,
                            sha256: None,
                            sha512: None,
                            extra_data: None,
                        },
                    ],
                    ..Default::default()
                }],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
            FileInfo {
                name: "libc.so".to_string(),
                base_name: "libc".to_string(),
                extension: "so".to_string(),
                path: "lib/libc.so".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 200,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
            FileInfo {
                name: "ldconfig".to_string(),
                base_name: "ldconfig".to_string(),
                extension: String::new(),
                path: "usr/bin/ldconfig".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 300,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
        ];

        let mut packages = vec![Package {
            package_type: Some(PackageType::Alpine),
            namespace: None,
            name: Some("musl".to_string()),
            version: Some("1.2.3".to_string()),
            qualifiers: None,
            subpath: None,
            primary_language: None,
            description: None,
            release_date: None,
            parties: vec![],
            keywords: vec![],
            homepage_url: None,
            download_url: None,
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            bug_tracking_url: None,
            code_view_url: None,
            vcs_url: None,
            copyright: None,
            holder: None,
            declared_license_expression: None,
            declared_license_expression_spdx: None,
            license_detections: vec![],
            other_license_expression: None,
            other_license_expression_spdx: None,
            other_license_detections: vec![],
            extracted_license_statement: None,
            notice_text: None,
            source_packages: vec![],
            is_private: false,
            is_virtual: false,
            extra_data: None,
            repository_homepage_url: None,
            repository_download_url: None,
            api_data_url: None,
            purl: Some("pkg:alpine/musl@1.2.3".to_string()),
            package_uid: "pkg:alpine/musl@1.2.3?uuid=test-uuid".to_string(),
            datafile_paths: vec!["lib/apk/db/installed".to_string()],
            datasource_ids: vec![DatasourceId::AlpineInstalledDb],
        }];

        let mut dependencies = vec![];

        resolve_file_references(&mut files, &mut packages, &mut dependencies);

        assert_eq!(files[1].for_packages.len(), 1);
        assert_eq!(
            files[1].for_packages[0],
            "pkg:alpine/musl@1.2.3?uuid=test-uuid"
        );
        assert_eq!(files[2].for_packages.len(), 1);
        assert_eq!(
            files[2].for_packages[0],
            "pkg:alpine/musl@1.2.3?uuid=test-uuid"
        );
    }

    #[test]
    fn test_resolve_missing_refs() {
        let mut files = vec![FileInfo {
            name: "installed".to_string(),
            base_name: "installed".to_string(),
            extension: String::new(),
            path: "lib/apk/db/installed".to_string(),
            file_type: FileType::File,
            mime_type: None,
            size: 100,
            date: None,
            sha1: None,
            md5: None,
            sha256: None,
            programming_language: None,
            package_data: vec![PackageData {
                datasource_id: Some(DatasourceId::AlpineInstalledDb),
                purl: Some("pkg:alpine/test@1.0".to_string()),
                name: Some("test".to_string()),
                file_references: vec![
                    FileReference {
                        path: "missing/file1.txt".to_string(),
                        size: None,
                        sha1: None,
                        md5: None,
                        sha256: None,
                        sha512: None,
                        extra_data: None,
                    },
                    FileReference {
                        path: "another/missing.so".to_string(),
                        size: None,
                        sha1: None,
                        md5: None,
                        sha256: None,
                        sha512: None,
                        extra_data: None,
                    },
                ],
                ..Default::default()
            }],
            license_expression: None,
            license_detections: vec![],
            copyrights: vec![],
            holders: vec![],
            authors: vec![],
            emails: vec![],
            urls: vec![],
            for_packages: vec![],
            scan_errors: vec![],
            is_source: None,
            source_count: None,
            is_legal: false,
            is_manifest: false,
            is_readme: false,
            is_top_level: false,
            is_key_file: false,
            tallies: None,
        }];

        let mut packages = vec![Package {
            package_type: Some(PackageType::Alpine),
            namespace: None,
            name: Some("test".to_string()),
            version: Some("1.0".to_string()),
            qualifiers: None,
            subpath: None,
            primary_language: None,
            description: None,
            release_date: None,
            parties: vec![],
            keywords: vec![],
            homepage_url: None,
            download_url: None,
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            bug_tracking_url: None,
            code_view_url: None,
            vcs_url: None,
            copyright: None,
            holder: None,
            declared_license_expression: None,
            declared_license_expression_spdx: None,
            license_detections: vec![],
            other_license_expression: None,
            other_license_expression_spdx: None,
            other_license_detections: vec![],
            extracted_license_statement: None,
            notice_text: None,
            source_packages: vec![],
            is_private: false,
            is_virtual: false,
            extra_data: None,
            repository_homepage_url: None,
            repository_download_url: None,
            api_data_url: None,
            purl: Some("pkg:alpine/test@1.0".to_string()),
            package_uid: "pkg:alpine/test@1.0?uuid=test-uuid".to_string(),
            datafile_paths: vec!["lib/apk/db/installed".to_string()],
            datasource_ids: vec![DatasourceId::AlpineInstalledDb],
        }];

        let mut dependencies = vec![];

        resolve_file_references(&mut files, &mut packages, &mut dependencies);

        assert!(packages[0].extra_data.is_some());
        let extra_data = packages[0].extra_data.as_ref().unwrap();
        assert!(extra_data.contains_key("missing_file_references"));

        let missing = extra_data.get("missing_file_references").unwrap();
        assert!(missing.is_array());
        let missing_array = missing.as_array().unwrap();
        assert_eq!(missing_array.len(), 2);
        assert_eq!(missing_array[0]["path"], "another/missing.so");
        assert_eq!(missing_array[1]["path"], "missing/file1.txt");
    }

    #[test]
    fn test_resolve_rpm_namespace() {
        let mut files = vec![
            FileInfo {
                name: "Packages".to_string(),
                base_name: "Packages".to_string(),
                extension: String::new(),
                path: "rootfs/var/lib/rpm/Packages".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 100,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![PackageData {
                    datasource_id: Some(DatasourceId::RpmInstalledDatabaseBdb),
                    purl: Some("pkg:rpm/bash@5.0".to_string()),
                    name: Some("bash".to_string()),
                    file_references: vec![],
                    ..Default::default()
                }],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
            FileInfo {
                name: "os-release".to_string(),
                base_name: "os-release".to_string(),
                extension: String::new(),
                path: "rootfs/etc/os-release".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 50,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![PackageData {
                    datasource_id: Some(DatasourceId::EtcOsRelease),
                    namespace: Some("fedora".to_string()),
                    name: Some("fedora".to_string()),
                    ..Default::default()
                }],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
        ];

        let mut packages = vec![Package {
            package_type: Some(PackageType::Rpm),
            namespace: None,
            name: Some("bash".to_string()),
            version: Some("5.0".to_string()),
            qualifiers: None,
            subpath: None,
            primary_language: None,
            description: None,
            release_date: None,
            parties: vec![],
            keywords: vec![],
            homepage_url: None,
            download_url: None,
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            bug_tracking_url: None,
            code_view_url: None,
            vcs_url: None,
            copyright: None,
            holder: None,
            declared_license_expression: None,
            declared_license_expression_spdx: None,
            license_detections: vec![],
            other_license_expression: None,
            other_license_expression_spdx: None,
            other_license_detections: vec![],
            extracted_license_statement: None,
            notice_text: None,
            source_packages: vec![],
            is_private: false,
            is_virtual: false,
            extra_data: None,
            repository_homepage_url: None,
            repository_download_url: None,
            api_data_url: None,
            purl: Some("pkg:rpm/bash@5.0".to_string()),
            package_uid: "pkg:rpm/bash@5.0?uuid=test-uuid".to_string(),
            datafile_paths: vec!["rootfs/var/lib/rpm/Packages".to_string()],
            datasource_ids: vec![DatasourceId::RpmInstalledDatabaseBdb],
        }];

        let mut dependencies = vec![TopLevelDependency {
            purl: Some("pkg:rpm/readline@8.0".to_string()),
            extracted_requirement: None,
            scope: None,
            is_runtime: Some(true),
            is_optional: None,
            is_pinned: None,
            is_direct: None,
            resolved_package: None,
            extra_data: None,
            dependency_uid: "pkg:rpm/readline@8.0?uuid=dep-uuid".to_string(),
            for_package_uid: Some("pkg:rpm/bash@5.0?uuid=test-uuid".to_string()),
            datafile_path: "rootfs/var/lib/rpm/Packages".to_string(),
            datasource_id: DatasourceId::RpmInstalledDatabaseBdb,
            namespace: None,
        }];

        resolve_file_references(&mut files, &mut packages, &mut dependencies);

        assert_eq!(packages[0].namespace, Some("fedora".to_string()));
        assert_eq!(packages[0].purl.as_deref(), Some("pkg:rpm/fedora/bash@5.0"));
        assert!(
            packages[0]
                .package_uid
                .starts_with("pkg:rpm/fedora/bash@5.0?uuid=")
        );
        assert_eq!(dependencies[0].namespace, Some("fedora".to_string()));
        assert_eq!(
            dependencies[0].purl.as_deref(),
            Some("pkg:rpm/fedora/readline@8.0")
        );
        assert_eq!(
            dependencies[0].for_package_uid.as_deref(),
            Some(packages[0].package_uid.as_str())
        );
    }

    #[test]
    fn test_merge_rpm_yumdb_metadata() {
        let mut files = vec![
            FileInfo {
                name: "Packages".to_string(),
                base_name: "Packages".to_string(),
                extension: String::new(),
                path: "rootfs/var/lib/rpm/Packages".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 1,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec!["pkg:rpm/bash@5.0-1.el8?uuid=rpm-uuid".to_string()],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
            FileInfo {
                name: "from_repo".to_string(),
                base_name: "from_repo".to_string(),
                extension: String::new(),
                path: "rootfs/var/lib/yum/yumdb/p/abc123-bash-5.0-1.el8.x86_64/from_repo"
                    .to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 1,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec!["pkg:rpm/bash@5.0-1.el8?uuid=yumdb-uuid".to_string()],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
        ];

        let mut packages = vec![
            Package {
                package_type: Some(PackageType::Rpm),
                namespace: None,
                name: Some("bash".to_string()),
                version: Some("5.0-1.el8".to_string()),
                qualifiers: Some(
                    std::iter::once(("arch".to_string(), "x86_64".to_string())).collect(),
                ),
                subpath: None,
                primary_language: None,
                description: None,
                release_date: None,
                parties: vec![],
                keywords: vec![],
                homepage_url: None,
                download_url: None,
                size: None,
                sha1: None,
                md5: None,
                sha256: None,
                sha512: None,
                bug_tracking_url: None,
                code_view_url: None,
                vcs_url: None,
                copyright: None,
                holder: None,
                declared_license_expression: None,
                declared_license_expression_spdx: None,
                license_detections: vec![],
                other_license_expression: None,
                other_license_expression_spdx: None,
                other_license_detections: vec![],
                extracted_license_statement: None,
                notice_text: None,
                source_packages: vec![],
                is_private: false,
                is_virtual: false,
                extra_data: None,
                repository_homepage_url: None,
                repository_download_url: None,
                api_data_url: None,
                purl: Some("pkg:rpm/bash@5.0-1.el8?arch=x86_64".to_string()),
                package_uid: "pkg:rpm/bash@5.0-1.el8?uuid=rpm-uuid".to_string(),
                datafile_paths: vec!["rootfs/var/lib/rpm/Packages".to_string()],
                datasource_ids: vec![DatasourceId::RpmInstalledDatabaseBdb],
            },
            Package {
                package_type: Some(PackageType::Rpm),
                namespace: None,
                name: Some("bash".to_string()),
                version: Some("5.0-1.el8".to_string()),
                qualifiers: Some(
                    std::iter::once(("arch".to_string(), "x86_64".to_string())).collect(),
                ),
                subpath: None,
                primary_language: None,
                description: None,
                release_date: None,
                parties: vec![],
                keywords: vec![],
                homepage_url: None,
                download_url: None,
                size: None,
                sha1: None,
                md5: None,
                sha256: None,
                sha512: None,
                bug_tracking_url: None,
                code_view_url: None,
                vcs_url: None,
                copyright: None,
                holder: None,
                declared_license_expression: None,
                declared_license_expression_spdx: None,
                license_detections: vec![],
                other_license_expression: None,
                other_license_expression_spdx: None,
                other_license_detections: vec![],
                extracted_license_statement: None,
                notice_text: None,
                source_packages: vec![],
                is_private: false,
                is_virtual: true,
                extra_data: Some(
                    [
                        (
                            "from_repo".to_string(),
                            serde_json::Value::String("baseos".to_string()),
                        ),
                        (
                            "releasever".to_string(),
                            serde_json::Value::String("8".to_string()),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
                repository_homepage_url: None,
                repository_download_url: None,
                api_data_url: None,
                purl: Some("pkg:rpm/bash@5.0-1.el8?arch=x86_64".to_string()),
                package_uid: "pkg:rpm/bash@5.0-1.el8?uuid=yumdb-uuid".to_string(),
                datafile_paths: vec![
                    "rootfs/var/lib/yum/yumdb/p/abc123-bash-5.0-1.el8.x86_64/from_repo".to_string(),
                ],
                datasource_ids: vec![DatasourceId::RpmYumdb],
            },
        ];

        merge_rpm_yumdb_metadata(&mut files, &mut packages);

        assert_eq!(packages.len(), 1);
        assert!(packages[0].datasource_ids.contains(&DatasourceId::RpmYumdb));
        assert!(
            packages[0]
                .datafile_paths
                .iter()
                .any(|path| path.contains("var/lib/yum/yumdb"))
        );
        let yumdb = packages[0]
            .extra_data
            .as_ref()
            .and_then(|extra| extra.get("yumdb"))
            .and_then(|value| value.as_object())
            .unwrap();
        assert_eq!(yumdb["from_repo"], "baseos");
        assert_eq!(yumdb["releasever"], "8");
        assert_eq!(
            files[1].for_packages,
            vec!["pkg:rpm/bash@5.0-1.el8?uuid=rpm-uuid".to_string()]
        );
    }

    #[test]
    fn test_strip_leading_slash() {
        let mut files = vec![
            FileInfo {
                name: "installed".to_string(),
                base_name: "installed".to_string(),
                extension: String::new(),
                path: "lib/apk/db/installed".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 100,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![PackageData {
                    datasource_id: Some(DatasourceId::AlpineInstalledDb),
                    purl: Some("pkg:alpine/test@1.0".to_string()),
                    name: Some("test".to_string()),
                    file_references: vec![FileReference {
                        path: "/lib/test.so".to_string(),
                        size: None,
                        sha1: None,
                        md5: None,
                        sha256: None,
                        sha512: None,
                        extra_data: None,
                    }],
                    ..Default::default()
                }],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
            FileInfo {
                name: "test.so".to_string(),
                base_name: "test".to_string(),
                extension: "so".to_string(),
                path: "lib/test.so".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 200,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
        ];

        let mut packages = vec![Package {
            package_type: Some(PackageType::Alpine),
            namespace: None,
            name: Some("test".to_string()),
            version: Some("1.0".to_string()),
            qualifiers: None,
            subpath: None,
            primary_language: None,
            description: None,
            release_date: None,
            parties: vec![],
            keywords: vec![],
            homepage_url: None,
            download_url: None,
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            bug_tracking_url: None,
            code_view_url: None,
            vcs_url: None,
            copyright: None,
            holder: None,
            declared_license_expression: None,
            declared_license_expression_spdx: None,
            license_detections: vec![],
            other_license_expression: None,
            other_license_expression_spdx: None,
            other_license_detections: vec![],
            extracted_license_statement: None,
            notice_text: None,
            source_packages: vec![],
            is_private: false,
            is_virtual: false,
            extra_data: None,
            repository_homepage_url: None,
            repository_download_url: None,
            api_data_url: None,
            purl: Some("pkg:alpine/test@1.0".to_string()),
            package_uid: "pkg:alpine/test@1.0?uuid=test-uuid".to_string(),
            datafile_paths: vec!["lib/apk/db/installed".to_string()],
            datasource_ids: vec![DatasourceId::AlpineInstalledDb],
        }];

        let mut dependencies = vec![];

        resolve_file_references(&mut files, &mut packages, &mut dependencies);

        assert_eq!(files[1].for_packages.len(), 1);
        assert_eq!(
            files[1].for_packages[0],
            "pkg:alpine/test@1.0?uuid=test-uuid"
        );
    }

    #[test]
    fn test_resolve_python_metadata_file_references() {
        let mut files = vec![
            FileInfo {
                name: "METADATA".to_string(),
                base_name: "METADATA".to_string(),
                extension: String::new(),
                path: "venv/lib/python3.11/site-packages/click-8.0.4.dist-info/METADATA"
                    .to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 100,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![PackageData {
                    datasource_id: Some(DatasourceId::PypiWheelMetadata),
                    purl: Some("pkg:pypi/click@8.0.4".to_string()),
                    name: Some("click".to_string()),
                    version: Some("8.0.4".to_string()),
                    file_references: vec![
                        FileReference {
                            path: "click/__init__.py".to_string(),
                            size: None,
                            sha1: None,
                            md5: None,
                            sha256: None,
                            sha512: None,
                            extra_data: None,
                        },
                        FileReference {
                            path: "click/core.py".to_string(),
                            size: None,
                            sha1: None,
                            md5: None,
                            sha256: None,
                            sha512: None,
                            extra_data: None,
                        },
                        FileReference {
                            path: "click-8.0.4.dist-info/LICENSE.rst".to_string(),
                            size: None,
                            sha1: None,
                            md5: None,
                            sha256: None,
                            sha512: None,
                            extra_data: None,
                        },
                    ],
                    ..Default::default()
                }],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
            FileInfo {
                name: "__init__.py".to_string(),
                base_name: "__init__".to_string(),
                extension: "py".to_string(),
                path: "venv/lib/python3.11/site-packages/click/__init__.py".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 5,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
            FileInfo {
                name: "core.py".to_string(),
                base_name: "core".to_string(),
                extension: "py".to_string(),
                path: "venv/lib/python3.11/site-packages/click/core.py".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 10,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
            FileInfo {
                name: "LICENSE.rst".to_string(),
                base_name: "LICENSE".to_string(),
                extension: "rst".to_string(),
                path: "venv/lib/python3.11/site-packages/click-8.0.4.dist-info/LICENSE.rst"
                    .to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 20,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
        ];

        let mut packages = vec![Package {
            package_type: Some(PackageType::Pypi),
            namespace: None,
            name: Some("click".to_string()),
            version: Some("8.0.4".to_string()),
            qualifiers: None,
            subpath: None,
            primary_language: None,
            description: None,
            release_date: None,
            parties: vec![],
            keywords: vec![],
            homepage_url: None,
            download_url: None,
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            bug_tracking_url: None,
            code_view_url: None,
            vcs_url: None,
            copyright: None,
            holder: None,
            declared_license_expression: None,
            declared_license_expression_spdx: None,
            license_detections: vec![],
            other_license_expression: None,
            other_license_expression_spdx: None,
            other_license_detections: vec![],
            extracted_license_statement: None,
            notice_text: None,
            source_packages: vec![],
            is_private: false,
            is_virtual: false,
            extra_data: None,
            repository_homepage_url: None,
            repository_download_url: None,
            api_data_url: None,
            purl: Some("pkg:pypi/click@8.0.4".to_string()),
            package_uid: "pkg:pypi/click@8.0.4?uuid=test-uuid".to_string(),
            datafile_paths: vec![
                "venv/lib/python3.11/site-packages/click-8.0.4.dist-info/METADATA".to_string(),
            ],
            datasource_ids: vec![DatasourceId::PypiWheelMetadata],
        }];

        let mut dependencies = vec![];

        resolve_file_references(&mut files, &mut packages, &mut dependencies);

        assert_eq!(files[1].for_packages.len(), 1);
        assert_eq!(files[2].for_packages.len(), 1);
        assert_eq!(files[3].for_packages.len(), 1);
        assert_eq!(
            files[2].for_packages[0],
            "pkg:pypi/click@8.0.4?uuid=test-uuid"
        );
    }

    #[test]
    fn test_resolve_python_pkg_info_installed_files_references() {
        let mut files = vec![
            FileInfo {
                name: "PKG-INFO".to_string(),
                base_name: "PKG-INFO".to_string(),
                extension: String::new(),
                path: "venv/lib/python3.11/site-packages/examplepkg.egg-info/PKG-INFO".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 100,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![PackageData {
                    datasource_id: Some(DatasourceId::PypiSdistPkginfo),
                    purl: Some("pkg:pypi/examplepkg@1.0.0".to_string()),
                    name: Some("examplepkg".to_string()),
                    version: Some("1.0.0".to_string()),
                    file_references: vec![FileReference {
                        path: "../examplepkg/core.py".to_string(),
                        size: None,
                        sha1: None,
                        md5: None,
                        sha256: None,
                        sha512: None,
                        extra_data: None,
                    }],
                    ..Default::default()
                }],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
            FileInfo {
                name: "core.py".to_string(),
                base_name: "core".to_string(),
                extension: "py".to_string(),
                path: "venv/lib/python3.11/site-packages/examplepkg/core.py".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 10,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
        ];

        let mut packages = vec![Package {
            package_type: Some(PackageType::Pypi),
            namespace: None,
            name: Some("examplepkg".to_string()),
            version: Some("1.0.0".to_string()),
            qualifiers: None,
            subpath: None,
            primary_language: None,
            description: None,
            release_date: None,
            parties: vec![],
            keywords: vec![],
            homepage_url: None,
            download_url: None,
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            bug_tracking_url: None,
            code_view_url: None,
            vcs_url: None,
            copyright: None,
            holder: None,
            declared_license_expression: None,
            declared_license_expression_spdx: None,
            license_detections: vec![],
            other_license_expression: None,
            other_license_expression_spdx: None,
            other_license_detections: vec![],
            extracted_license_statement: None,
            notice_text: None,
            source_packages: vec![],
            is_private: false,
            is_virtual: false,
            extra_data: None,
            repository_homepage_url: None,
            repository_download_url: None,
            api_data_url: None,
            purl: Some("pkg:pypi/examplepkg@1.0.0".to_string()),
            package_uid: "pkg:pypi/examplepkg@1.0.0?uuid=test-uuid".to_string(),
            datafile_paths: vec![
                "venv/lib/python3.11/site-packages/examplepkg.egg-info/PKG-INFO".to_string(),
            ],
            datasource_ids: vec![DatasourceId::PypiSdistPkginfo],
        }];

        let mut dependencies = vec![];

        resolve_file_references(&mut files, &mut packages, &mut dependencies);

        assert_eq!(
            files[1].for_packages,
            vec!["pkg:pypi/examplepkg@1.0.0?uuid=test-uuid".to_string()]
        );
    }

    #[test]
    fn test_resolve_python_metadata_file_references_in_dist_packages() {
        let mut files = vec![
            FileInfo {
                name: "METADATA".to_string(),
                base_name: "METADATA".to_string(),
                extension: String::new(),
                path: "usr/lib/python3/dist-packages/click-8.0.4.dist-info/METADATA".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 100,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![PackageData {
                    datasource_id: Some(DatasourceId::PypiWheelMetadata),
                    purl: Some("pkg:pypi/click@8.0.4".to_string()),
                    name: Some("click".to_string()),
                    version: Some("8.0.4".to_string()),
                    file_references: vec![FileReference {
                        path: "click/core.py".to_string(),
                        size: None,
                        sha1: None,
                        md5: None,
                        sha256: None,
                        sha512: None,
                        extra_data: None,
                    }],
                    ..Default::default()
                }],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
            FileInfo {
                name: "core.py".to_string(),
                base_name: "core".to_string(),
                extension: "py".to_string(),
                path: "usr/lib/python3/dist-packages/click/core.py".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 10,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
        ];

        let mut packages = vec![Package {
            package_type: Some(PackageType::Pypi),
            namespace: None,
            name: Some("click".to_string()),
            version: Some("8.0.4".to_string()),
            qualifiers: None,
            subpath: None,
            primary_language: None,
            description: None,
            release_date: None,
            parties: vec![],
            keywords: vec![],
            homepage_url: None,
            download_url: None,
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            bug_tracking_url: None,
            code_view_url: None,
            vcs_url: None,
            copyright: None,
            holder: None,
            declared_license_expression: None,
            declared_license_expression_spdx: None,
            license_detections: vec![],
            other_license_expression: None,
            other_license_expression_spdx: None,
            other_license_detections: vec![],
            extracted_license_statement: None,
            notice_text: None,
            source_packages: vec![],
            is_private: false,
            is_virtual: false,
            extra_data: None,
            repository_homepage_url: None,
            repository_download_url: None,
            api_data_url: None,
            purl: Some("pkg:pypi/click@8.0.4".to_string()),
            package_uid: "pkg:pypi/click@8.0.4?uuid=test-uuid".to_string(),
            datafile_paths: vec![
                "usr/lib/python3/dist-packages/click-8.0.4.dist-info/METADATA".to_string(),
            ],
            datasource_ids: vec![DatasourceId::PypiWheelMetadata],
        }];

        let mut dependencies = vec![];

        resolve_file_references(&mut files, &mut packages, &mut dependencies);

        assert_eq!(
            files[1].for_packages,
            vec!["pkg:pypi/click@8.0.4?uuid=test-uuid".to_string()]
        );
    }

    #[test]
    fn test_python_metadata_file_references_do_not_assign_outside_packages_dirs() {
        let mut files = vec![
            FileInfo {
                name: "METADATA".to_string(),
                base_name: "METADATA".to_string(),
                extension: String::new(),
                path: "project/metadata/METADATA".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 100,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![PackageData {
                    datasource_id: Some(DatasourceId::PypiWheelMetadata),
                    purl: Some("pkg:pypi/examplepkg@1.0.0".to_string()),
                    name: Some("examplepkg".to_string()),
                    version: Some("1.0.0".to_string()),
                    file_references: vec![FileReference {
                        path: "examplepkg/core.py".to_string(),
                        size: None,
                        sha1: None,
                        md5: None,
                        sha256: None,
                        sha512: None,
                        extra_data: None,
                    }],
                    ..Default::default()
                }],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
            FileInfo {
                name: "core.py".to_string(),
                base_name: "core".to_string(),
                extension: "py".to_string(),
                path: "project/examplepkg/core.py".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 10,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
        ];

        let mut packages = vec![Package {
            package_type: Some(PackageType::Pypi),
            namespace: None,
            name: Some("examplepkg".to_string()),
            version: Some("1.0.0".to_string()),
            qualifiers: None,
            subpath: None,
            primary_language: None,
            description: None,
            release_date: None,
            parties: vec![],
            keywords: vec![],
            homepage_url: None,
            download_url: None,
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            bug_tracking_url: None,
            code_view_url: None,
            vcs_url: None,
            copyright: None,
            holder: None,
            declared_license_expression: None,
            declared_license_expression_spdx: None,
            license_detections: vec![],
            other_license_expression: None,
            other_license_expression_spdx: None,
            other_license_detections: vec![],
            extracted_license_statement: None,
            notice_text: None,
            source_packages: vec![],
            is_private: false,
            is_virtual: false,
            extra_data: None,
            repository_homepage_url: None,
            repository_download_url: None,
            api_data_url: None,
            purl: Some("pkg:pypi/examplepkg@1.0.0".to_string()),
            package_uid: "pkg:pypi/examplepkg@1.0.0?uuid=test-uuid".to_string(),
            datafile_paths: vec!["project/metadata/METADATA".to_string()],
            datasource_ids: vec![DatasourceId::PypiWheelMetadata],
        }];

        let mut dependencies = vec![];

        resolve_file_references(&mut files, &mut packages, &mut dependencies);

        assert!(files[1].for_packages.is_empty());
    }

    #[test]
    fn test_python_sources_file_references_do_not_escape_project_root() {
        let mut files = vec![
            FileInfo {
                name: "PKG-INFO".to_string(),
                base_name: "PKG-INFO".to_string(),
                extension: String::new(),
                path: "project/PyJPString.egg-info/PKG-INFO".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 100,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![PackageData {
                    datasource_id: Some(DatasourceId::PypiSdistPkginfo),
                    purl: Some("pkg:pypi/PyJPString@0.0.3".to_string()),
                    name: Some("PyJPString".to_string()),
                    version: Some("0.0.3".to_string()),
                    file_references: vec![FileReference {
                        path: "../../outside.py".to_string(),
                        size: None,
                        sha1: None,
                        md5: None,
                        sha256: None,
                        sha512: None,
                        extra_data: None,
                    }],
                    ..Default::default()
                }],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
            FileInfo {
                name: "outside.py".to_string(),
                base_name: "outside".to_string(),
                extension: "py".to_string(),
                path: "outside.py".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 10,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
        ];

        let mut packages = vec![Package {
            package_type: Some(PackageType::Pypi),
            namespace: None,
            name: Some("PyJPString".to_string()),
            version: Some("0.0.3".to_string()),
            qualifiers: None,
            subpath: None,
            primary_language: None,
            description: None,
            release_date: None,
            parties: vec![],
            keywords: vec![],
            homepage_url: None,
            download_url: None,
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            bug_tracking_url: None,
            code_view_url: None,
            vcs_url: None,
            copyright: None,
            holder: None,
            declared_license_expression: None,
            declared_license_expression_spdx: None,
            license_detections: vec![],
            other_license_expression: None,
            other_license_expression_spdx: None,
            other_license_detections: vec![],
            extracted_license_statement: None,
            notice_text: None,
            source_packages: vec![],
            is_private: false,
            is_virtual: false,
            extra_data: None,
            repository_homepage_url: None,
            repository_download_url: None,
            api_data_url: None,
            purl: Some("pkg:pypi/PyJPString@0.0.3".to_string()),
            package_uid: "pkg:pypi/PyJPString@0.0.3?uuid=test-uuid".to_string(),
            datafile_paths: vec!["project/PyJPString.egg-info/PKG-INFO".to_string()],
            datasource_ids: vec![DatasourceId::PypiSdistPkginfo],
        }];

        let mut dependencies = vec![];

        resolve_file_references(&mut files, &mut packages, &mut dependencies);

        assert!(files[1].for_packages.is_empty());
        let missing = packages[0]
            .extra_data
            .as_ref()
            .and_then(|extra| extra.get("missing_file_references"))
            .and_then(|value| value.as_array())
            .expect("missing_file_references should be recorded");
        assert_eq!(missing.len(), 1);
    }

    #[test]
    fn test_resolve_debian_installed_file_references_from_status_db() {
        let mut files = vec![
            FileInfo {
                name: "status".to_string(),
                base_name: "status".to_string(),
                extension: String::new(),
                path: "rootfs/var/lib/dpkg/status".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 100,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![PackageData {
                    datasource_id: Some(DatasourceId::DebianInstalledStatusDb),
                    package_type: Some(PackageType::Deb),
                    namespace: Some("debian".to_string()),
                    name: Some("bash".to_string()),
                    version: Some("5.2-1".to_string()),
                    purl: Some("pkg:deb/debian/bash@5.2-1?arch=amd64".to_string()),
                    ..Default::default()
                }],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
            FileInfo {
                name: "bash.list".to_string(),
                base_name: "bash".to_string(),
                extension: "list".to_string(),
                path: "rootfs/var/lib/dpkg/info/bash.list".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 40,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![PackageData {
                    datasource_id: Some(DatasourceId::DebianInstalledFilesList),
                    package_type: Some(PackageType::Deb),
                    namespace: Some("debian".to_string()),
                    name: Some("bash".to_string()),
                    purl: Some("pkg:deb/debian/bash".to_string()),
                    file_references: vec![
                        FileReference {
                            path: "/bin/bash".to_string(),
                            size: None,
                            sha1: None,
                            md5: None,
                            sha256: None,
                            sha512: None,
                            extra_data: None,
                        },
                        FileReference {
                            path: "/usr/share/doc/bash/copyright".to_string(),
                            size: None,
                            sha1: None,
                            md5: None,
                            sha256: None,
                            sha512: None,
                            extra_data: None,
                        },
                    ],
                    ..Default::default()
                }],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
            FileInfo {
                name: "bash.md5sums".to_string(),
                base_name: "bash".to_string(),
                extension: "md5sums".to_string(),
                path: "rootfs/var/lib/dpkg/info/bash.md5sums".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 40,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![PackageData {
                    datasource_id: Some(DatasourceId::DebianInstalledMd5Sums),
                    package_type: Some(PackageType::Deb),
                    namespace: Some("debian".to_string()),
                    name: Some("bash".to_string()),
                    purl: Some("pkg:deb/debian/bash".to_string()),
                    file_references: vec![FileReference {
                        path: "bin/bash".to_string(),
                        size: None,
                        sha1: None,
                        md5: Some("77506afebd3b7e19e937a678a185b62e".to_string()),
                        sha256: None,
                        sha512: None,
                        extra_data: None,
                    }],
                    ..Default::default()
                }],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
            FileInfo {
                name: "bash".to_string(),
                base_name: "bash".to_string(),
                extension: String::new(),
                path: "rootfs/bin/bash".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 20,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
            FileInfo {
                name: "copyright".to_string(),
                base_name: "copyright".to_string(),
                extension: String::new(),
                path: "rootfs/usr/share/doc/bash/copyright".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 20,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
        ];

        let mut packages = vec![Package {
            package_type: Some(PackageType::Deb),
            namespace: Some("debian".to_string()),
            name: Some("bash".to_string()),
            version: Some("5.2-1".to_string()),
            qualifiers: Some(HashMap::from([("arch".to_string(), "amd64".to_string())])),
            subpath: None,
            primary_language: None,
            description: None,
            release_date: None,
            parties: vec![],
            keywords: vec![],
            homepage_url: None,
            download_url: None,
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            bug_tracking_url: None,
            code_view_url: None,
            vcs_url: None,
            copyright: None,
            holder: None,
            declared_license_expression: None,
            declared_license_expression_spdx: None,
            license_detections: vec![],
            other_license_expression: None,
            other_license_expression_spdx: None,
            other_license_detections: vec![],
            extracted_license_statement: None,
            notice_text: None,
            source_packages: vec![],
            is_private: false,
            is_virtual: false,
            extra_data: None,
            repository_homepage_url: None,
            repository_download_url: None,
            api_data_url: None,
            purl: Some("pkg:deb/debian/bash@5.2-1?arch=amd64".to_string()),
            package_uid: "pkg:deb/debian/bash@5.2-1?arch=amd64&uuid=test-uuid".to_string(),
            datafile_paths: vec!["rootfs/var/lib/dpkg/status".to_string()],
            datasource_ids: vec![DatasourceId::DebianInstalledStatusDb],
        }];

        let mut dependencies = vec![];
        resolve_file_references(&mut files, &mut packages, &mut dependencies);

        assert_eq!(
            files[3].for_packages,
            vec!["pkg:deb/debian/bash@5.2-1?arch=amd64&uuid=test-uuid".to_string()]
        );
        assert_eq!(
            files[4].for_packages,
            vec!["pkg:deb/debian/bash@5.2-1?arch=amd64&uuid=test-uuid".to_string()]
        );
    }

    #[test]
    fn test_resolve_debian_installed_file_references_matches_ubuntu_package_namespace() {
        let mut files = vec![
            FileInfo {
                name: "status".to_string(),
                base_name: "status".to_string(),
                extension: String::new(),
                path: "rootfs/var/lib/dpkg/status".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 100,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![PackageData {
                    datasource_id: Some(DatasourceId::DebianInstalledStatusDb),
                    package_type: Some(PackageType::Deb),
                    namespace: Some("ubuntu".to_string()),
                    name: Some("bash".to_string()),
                    version: Some("5.2-1ubuntu1".to_string()),
                    purl: Some("pkg:deb/ubuntu/bash@5.2-1ubuntu1?arch=amd64".to_string()),
                    ..Default::default()
                }],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
            FileInfo {
                name: "bash.list".to_string(),
                base_name: "bash".to_string(),
                extension: "list".to_string(),
                path: "rootfs/var/lib/dpkg/info/bash.list".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 40,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![PackageData {
                    datasource_id: Some(DatasourceId::DebianInstalledFilesList),
                    package_type: Some(PackageType::Deb),
                    namespace: Some("debian".to_string()),
                    name: Some("bash".to_string()),
                    purl: Some("pkg:deb/debian/bash".to_string()),
                    file_references: vec![FileReference {
                        path: "/bin/bash".to_string(),
                        size: None,
                        sha1: None,
                        md5: None,
                        sha256: None,
                        sha512: None,
                        extra_data: None,
                    }],
                    ..Default::default()
                }],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
            FileInfo {
                name: "bash".to_string(),
                base_name: "bash".to_string(),
                extension: String::new(),
                path: "rootfs/bin/bash".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 20,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
        ];

        let mut packages = vec![Package {
            package_type: Some(PackageType::Deb),
            namespace: Some("ubuntu".to_string()),
            name: Some("bash".to_string()),
            version: Some("5.2-1ubuntu1".to_string()),
            qualifiers: Some(HashMap::from([("arch".to_string(), "amd64".to_string())])),
            subpath: None,
            primary_language: None,
            description: None,
            release_date: None,
            parties: vec![],
            keywords: vec![],
            homepage_url: None,
            download_url: None,
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            bug_tracking_url: None,
            code_view_url: None,
            vcs_url: None,
            copyright: None,
            holder: None,
            declared_license_expression: None,
            declared_license_expression_spdx: None,
            license_detections: vec![],
            other_license_expression: None,
            other_license_expression_spdx: None,
            other_license_detections: vec![],
            extracted_license_statement: None,
            notice_text: None,
            source_packages: vec![],
            is_private: false,
            is_virtual: false,
            extra_data: None,
            repository_homepage_url: None,
            repository_download_url: None,
            api_data_url: None,
            purl: Some("pkg:deb/ubuntu/bash@5.2-1ubuntu1?arch=amd64".to_string()),
            package_uid: "pkg:deb/ubuntu/bash@5.2-1ubuntu1?arch=amd64&uuid=test-uuid".to_string(),
            datafile_paths: vec!["rootfs/var/lib/dpkg/status".to_string()],
            datasource_ids: vec![DatasourceId::DebianInstalledStatusDb],
        }];

        let mut dependencies = vec![];
        resolve_file_references(&mut files, &mut packages, &mut dependencies);

        assert_eq!(
            files[2].for_packages,
            vec!["pkg:deb/ubuntu/bash@5.2-1ubuntu1?arch=amd64&uuid=test-uuid".to_string()]
        );
    }

    #[test]
    fn test_resolve_debian_installed_file_references_respects_arch_qualifier() {
        let mut files = vec![
            FileInfo {
                name: "status".to_string(),
                base_name: "status".to_string(),
                extension: String::new(),
                path: "rootfs/var/lib/dpkg/status".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 100,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![PackageData {
                    datasource_id: Some(DatasourceId::DebianInstalledStatusDb),
                    package_type: Some(PackageType::Deb),
                    namespace: Some("debian".to_string()),
                    name: Some("libc6".to_string()),
                    version: Some("2.36-1".to_string()),
                    purl: Some("pkg:deb/debian/libc6@2.36-1?arch=amd64".to_string()),
                    qualifiers: Some(HashMap::from([("arch".to_string(), "amd64".to_string())])),
                    ..Default::default()
                }],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
            FileInfo {
                name: "libc6:amd64.list".to_string(),
                base_name: "libc6:amd64".to_string(),
                extension: "list".to_string(),
                path: "rootfs/var/lib/dpkg/info/libc6:amd64.list".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 20,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![PackageData {
                    datasource_id: Some(DatasourceId::DebianInstalledFilesList),
                    package_type: Some(PackageType::Deb),
                    namespace: Some("debian".to_string()),
                    name: Some("libc6".to_string()),
                    qualifiers: Some(HashMap::from([("arch".to_string(), "amd64".to_string())])),
                    purl: Some("pkg:deb/debian/libc6?arch=amd64".to_string()),
                    file_references: vec![FileReference {
                        path: "/lib/x86_64-linux-gnu/libc.so.6".to_string(),
                        size: None,
                        sha1: None,
                        md5: None,
                        sha256: None,
                        sha512: None,
                        extra_data: None,
                    }],
                    ..Default::default()
                }],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
            FileInfo {
                name: "libc6:i386.list".to_string(),
                base_name: "libc6:i386".to_string(),
                extension: "list".to_string(),
                path: "rootfs/var/lib/dpkg/info/libc6:i386.list".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 20,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![PackageData {
                    datasource_id: Some(DatasourceId::DebianInstalledFilesList),
                    package_type: Some(PackageType::Deb),
                    namespace: Some("debian".to_string()),
                    name: Some("libc6".to_string()),
                    qualifiers: Some(HashMap::from([("arch".to_string(), "i386".to_string())])),
                    purl: Some("pkg:deb/debian/libc6?arch=i386".to_string()),
                    file_references: vec![FileReference {
                        path: "/lib/i386-linux-gnu/libc.so.6".to_string(),
                        size: None,
                        sha1: None,
                        md5: None,
                        sha256: None,
                        sha512: None,
                        extra_data: None,
                    }],
                    ..Default::default()
                }],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
            FileInfo {
                name: "libc.so.6".to_string(),
                base_name: "libc.so".to_string(),
                extension: "6".to_string(),
                path: "rootfs/lib/x86_64-linux-gnu/libc.so.6".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 10,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
            FileInfo {
                name: "libc.so.6".to_string(),
                base_name: "libc.so".to_string(),
                extension: "6".to_string(),
                path: "rootfs/lib/i386-linux-gnu/libc.so.6".to_string(),
                file_type: FileType::File,
                mime_type: None,
                size: 10,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: vec![],
                license_expression: None,
                license_detections: vec![],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                is_source: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                tallies: None,
            },
        ];

        let mut packages = vec![Package {
            package_type: Some(PackageType::Deb),
            namespace: Some("debian".to_string()),
            name: Some("libc6".to_string()),
            version: Some("2.36-1".to_string()),
            qualifiers: Some(HashMap::from([("arch".to_string(), "amd64".to_string())])),
            subpath: None,
            primary_language: None,
            description: None,
            release_date: None,
            parties: vec![],
            keywords: vec![],
            homepage_url: None,
            download_url: None,
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            bug_tracking_url: None,
            code_view_url: None,
            vcs_url: None,
            copyright: None,
            holder: None,
            declared_license_expression: None,
            declared_license_expression_spdx: None,
            license_detections: vec![],
            other_license_expression: None,
            other_license_expression_spdx: None,
            other_license_detections: vec![],
            extracted_license_statement: None,
            notice_text: None,
            source_packages: vec![],
            is_private: false,
            is_virtual: false,
            extra_data: None,
            repository_homepage_url: None,
            repository_download_url: None,
            api_data_url: None,
            purl: Some("pkg:deb/debian/libc6@2.36-1?arch=amd64".to_string()),
            package_uid: "pkg:deb/debian/libc6@2.36-1?arch=amd64&uuid=test-uuid".to_string(),
            datafile_paths: vec!["rootfs/var/lib/dpkg/status".to_string()],
            datasource_ids: vec![DatasourceId::DebianInstalledStatusDb],
        }];

        let mut dependencies = vec![];
        resolve_file_references(&mut files, &mut packages, &mut dependencies);

        assert_eq!(
            files[3].for_packages,
            vec!["pkg:deb/debian/libc6@2.36-1?arch=amd64&uuid=test-uuid".to_string()]
        );
        assert!(files[4].for_packages.is_empty());
    }
}
