use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::str::FromStr;

use crate::models::{DatasourceId, FileInfo, Package, TopLevelDependency};
use packageurl::PackageUrl;
use strum::EnumIter;

struct DbPathConfig {
    datasource_ids: &'static [DatasourceId],
    path_suffix: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIter)]
enum FileReferenceResolverKind {
    About,
    AttachedManifest,
    CondaMeta,
    DebianExtractedDeb,
    InstalledDb,
    PythonMetadata,
    RelativeToDatafileParent,
}

struct FileReferenceResolverConfig {
    datasource_ids: &'static [DatasourceId],
    kind: FileReferenceResolverKind,
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
        datasource_ids: &[DatasourceId::RpmInstalledDatabaseSqlite],
        path_suffix: "var/lib/rpm/rpmdb.sqlite",
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
    DatasourceId::PypiEggPkginfo,
    DatasourceId::PypiEditableEggPkginfo,
];
const PYTHON_SITE_PACKAGES_SEGMENTS: &[&str] = &["site-packages/", "dist-packages/"];
const DEBIAN_INSTALLED_SUPPLEMENTAL_DATASOURCE_IDS: &[DatasourceId] = &[
    DatasourceId::DebianInstalledFilesList,
    DatasourceId::DebianInstalledMd5Sums,
];

const INSTALLED_DB_DATASOURCE_IDS: &[DatasourceId] = &[
    DatasourceId::AlpineInstalledDb,
    DatasourceId::RpmInstalledDatabaseBdb,
    DatasourceId::RpmInstalledDatabaseNdb,
    DatasourceId::RpmInstalledDatabaseSqlite,
    DatasourceId::DebianInstalledStatusDb,
    DatasourceId::DebianDistrolessInstalledDb,
];

const FILE_REFERENCE_RESOLVER_CONFIGS: &[FileReferenceResolverConfig] = &[
    FileReferenceResolverConfig {
        datasource_ids: &[DatasourceId::AboutFile],
        kind: FileReferenceResolverKind::About,
    },
    FileReferenceResolverConfig {
        datasource_ids: &[DatasourceId::CpanManifest],
        kind: FileReferenceResolverKind::AttachedManifest,
    },
    FileReferenceResolverConfig {
        datasource_ids: &[DatasourceId::CondaMetaJson],
        kind: FileReferenceResolverKind::CondaMeta,
    },
    FileReferenceResolverConfig {
        datasource_ids: &[DatasourceId::DebianMd5SumsInExtractedDeb],
        kind: FileReferenceResolverKind::DebianExtractedDeb,
    },
    FileReferenceResolverConfig {
        datasource_ids: INSTALLED_DB_DATASOURCE_IDS,
        kind: FileReferenceResolverKind::InstalledDb,
    },
    FileReferenceResolverConfig {
        datasource_ids: PYTHON_METADATA_DATASOURCE_IDS,
        kind: FileReferenceResolverKind::PythonMetadata,
    },
    FileReferenceResolverConfig {
        datasource_ids: &[DatasourceId::GradleModule],
        kind: FileReferenceResolverKind::RelativeToDatafileParent,
    },
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
    if packages.is_empty() || !has_relevant_file_reference_inputs(files) {
        return;
    }

    let path_index = build_path_index(&*files);

    for package in packages.iter_mut() {
        let Some(config) = find_file_reference_resolver(files, package) else {
            continue;
        };

        match config.kind {
            FileReferenceResolverKind::About
            | FileReferenceResolverKind::RelativeToDatafileParent => {
                resolve_relative_to_datafile_parent(
                    files,
                    &path_index,
                    package,
                    config.datasource_ids,
                );
            }
            FileReferenceResolverKind::AttachedManifest => {
                resolve_attached_manifest_file_references(
                    files,
                    &path_index,
                    package,
                    config.datasource_ids[0],
                );
            }
            FileReferenceResolverKind::CondaMeta => {
                resolve_conda_file_references(files, &path_index, package);
            }
            FileReferenceResolverKind::DebianExtractedDeb => {
                resolve_debian_extracted_deb_file_references(files, &path_index, package)
            }
            FileReferenceResolverKind::InstalledDb => {
                resolve_installed_db_file_references(files, &path_index, package, dependencies);
            }
            FileReferenceResolverKind::PythonMetadata => {
                resolve_python_metadata_file_references(files, &path_index, package);
            }
        }
    }
}

pub(super) fn has_relevant_file_reference_datasource_ids(
    file_datasource_ids: &HashSet<DatasourceId>,
) -> bool {
    FILE_REFERENCE_RESOLVER_CONFIGS.iter().any(|config| {
        config
            .datasource_ids
            .iter()
            .any(|datasource_id| file_datasource_ids.contains(datasource_id))
    })
}

fn has_relevant_file_reference_inputs(files: &[FileInfo]) -> bool {
    let file_datasource_ids: HashSet<DatasourceId> = files
        .iter()
        .flat_map(|file| {
            file.package_data
                .iter()
                .filter_map(|package_data| package_data.datasource_id)
        })
        .collect();

    has_relevant_file_reference_datasource_ids(&file_datasource_ids)
}

fn resolve_relative_to_datafile_parent(
    files: &mut [FileInfo],
    path_index: &HashMap<String, usize>,
    package: &mut Package,
    datasource_ids: &[DatasourceId],
) {
    let Some(datafile_path) = package.datafile_paths.first() else {
        return;
    };
    let root = Path::new(datafile_path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let file_references = collect_file_references(
        files,
        path_index,
        datafile_path,
        &package.datasource_ids,
        datasource_ids,
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

    record_missing_file_references(package, missing_refs);
}

fn resolve_attached_manifest_file_references(
    files: &mut [FileInfo],
    path_index: &HashMap<String, usize>,
    package: &mut Package,
    datasource_id: DatasourceId,
) {
    let Some((datafile_path, file_references)) =
        find_attached_manifest_file_references(files, package, datasource_id)
    else {
        return;
    };

    let root = Path::new(datafile_path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

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

    record_missing_file_references(package, missing_refs);
}

fn resolve_conda_file_references(
    files: &mut [FileInfo],
    path_index: &HashMap<String, usize>,
    package: &mut Package,
) {
    let Some(conda_meta_path) = package
        .datafile_paths
        .iter()
        .find(|path| path.contains(CONDA_META_PATH_SEGMENT))
    else {
        return;
    };
    let Some(root) = compute_conda_root(Some(conda_meta_path.as_str())) else {
        return;
    };

    let file_references = collect_file_references(
        files,
        path_index,
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

    record_missing_file_references(package, missing_refs);
}

fn resolve_installed_db_file_references(
    files: &mut [FileInfo],
    path_index: &HashMap<String, usize>,
    package: &mut Package,
    dependencies: &mut [TopLevelDependency],
) {
    let Some(config) = find_db_config(package) else {
        return;
    };
    let Some(datafile_path) = package.datafile_paths.first() else {
        return;
    };

    let root = compute_root(datafile_path, config.path_suffix);

    let mut file_references = collect_file_references(
        files,
        path_index,
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

    record_missing_file_references(package, missing_refs);

    if is_rpm_package(package)
        && let Some(namespace) = resolve_rpm_namespace(files, path_index, &root)
    {
        apply_rpm_namespace(files, package, dependencies, &namespace);
    }
}

fn resolve_debian_extracted_deb_file_references(
    files: &mut [FileInfo],
    path_index: &HashMap<String, usize>,
    package: &mut Package,
) {
    let Some(datafile_path) = package
        .datafile_paths
        .iter()
        .find(|path| path.ends_with("/md5sums"))
    else {
        return;
    };

    let Some(md5sums_parent) = Path::new(datafile_path).parent() else {
        return;
    };
    let Some(extracted_root) = md5sums_parent.parent() else {
        return;
    };
    let root = extracted_root.to_string_lossy().to_string();

    let Some(&file_idx) = path_index.get(datafile_path) else {
        return;
    };
    let file_references: Vec<_> = files[file_idx]
        .package_data
        .iter()
        .filter(|pkg_data| {
            pkg_data.datasource_id == Some(DatasourceId::DebianMd5SumsInExtractedDeb)
        })
        .flat_map(|pkg_data| pkg_data.file_references.clone())
        .collect();

    let mut missing_refs = Vec::new();
    for file_ref in &file_references {
        let resolved_path = if root.is_empty() {
            file_ref.path.trim_start_matches('/').to_string()
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

    record_missing_file_references(package, missing_refs);
}

fn resolve_python_metadata_file_references(
    files: &mut [FileInfo],
    path_index: &HashMap<String, usize>,
    package: &mut Package,
) {
    let Some(python_resolution) = find_python_metadata_root(package) else {
        return;
    };
    let Some(datafile_path) = package
        .datafile_paths
        .iter()
        .find(|path| is_python_metadata_layout(path))
    else {
        return;
    };

    let file_references = collect_file_references(
        files,
        path_index,
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

    record_missing_file_references(package, missing_refs);
}

fn record_missing_file_references(package: &mut Package, mut missing_refs: Vec<String>) {
    if missing_refs.is_empty() {
        return;
    }

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

fn find_file_reference_resolver(
    files: &[FileInfo],
    package: &Package,
) -> Option<&'static FileReferenceResolverConfig> {
    FILE_REFERENCE_RESOLVER_CONFIGS
        .iter()
        .find(|config| match config.kind {
            FileReferenceResolverKind::AttachedManifest => {
                config.datasource_ids.iter().any(|datasource_id| {
                    files.iter().any(|file| {
                        file.for_packages.contains(&package.package_uid)
                            && file
                                .package_data
                                .iter()
                                .any(|pkg_data| pkg_data.datasource_id == Some(*datasource_id))
                    })
                })
            }
            _ => config
                .datasource_ids
                .iter()
                .any(|datasource_id| package.datasource_ids.contains(datasource_id)),
        })
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
    let datafile_paths = &package.datafile_paths;

    for config in DB_PATH_CONFIGS {
        if !datafile_paths.is_empty()
            && !datafile_paths
                .iter()
                .any(|path| path.ends_with(config.path_suffix))
        {
            continue;
        }

        for &config_dsid in config.datasource_ids {
            for &pkg_dsid in &package.datasource_ids {
                if config_dsid == pkg_dsid {
                    return Some(config);
                }
            }
        }
    }

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

fn find_attached_manifest_file_references<'a>(
    files: &'a [FileInfo],
    package: &Package,
    datasource_id: DatasourceId,
) -> Option<(&'a str, Vec<crate::models::FileReference>)> {
    for file in files {
        if !file.for_packages.contains(&package.package_uid) {
            continue;
        }

        for pkg_data in &file.package_data {
            if pkg_data.datasource_id == Some(datasource_id) {
                return Some((&file.path, pkg_data.file_references.clone()));
            }
        }
    }

    None
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
                existing.sha1 = file_ref.sha1;
            }
            if existing.md5.is_none() {
                existing.md5 = file_ref.md5;
            }
            if existing.sha256.is_none() {
                existing.sha256 = file_ref.sha256;
            }
            if existing.sha512.is_none() {
                existing.sha512 = file_ref.sha512;
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
#[path = "file_ref_resolve_test.rs"]
mod tests;
