use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use packageurl::PackageUrl;

use crate::cache::DEFAULT_CACHE_DIR_NAME;
use crate::models::{
    DatasourceId, Dependency, FileInfo, Package, PackageData, PackageType, PackageUid,
    TopLevelDependency,
};

pub fn assemble_swift_packages(
    files: &mut [FileInfo],
    packages: &mut Vec<Package>,
    dependencies: &mut Vec<TopLevelDependency>,
) {
    let mut swift_dirs: HashMap<PathBuf, SwiftDirectoryInputs> = HashMap::new();

    for (idx, file) in files.iter().enumerate() {
        let Some(file_name) = Path::new(&file.path)
            .file_name()
            .and_then(|name| name.to_str())
        else {
            continue;
        };

        for package_data in &file.package_data {
            let Some(datasource_id) = package_data.datasource_id else {
                continue;
            };

            let root = Path::new(&file.path)
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default();

            let inputs = swift_dirs.entry(root).or_default();
            let source = SwiftSource {
                file_index: idx,
                path: file.path.clone(),
                package_data: package_data.clone(),
            };

            match datasource_id {
                DatasourceId::SwiftPackageManifestJson if is_swift_manifest_filename(file_name) => {
                    inputs.manifest = pick_manifest_source(inputs.manifest.take(), source);
                }
                DatasourceId::SwiftPackageShowDependencies
                    if file_name == "swift-show-dependencies.deplock" =>
                {
                    inputs.show_dependencies = Some(source);
                }
                DatasourceId::SwiftPackageResolved if is_swift_resolved_filename(file_name) => {
                    inputs.resolved = Some(source);
                }
                _ => {}
            }
        }
    }

    let swift_roots: Vec<PathBuf> = swift_dirs.keys().cloned().collect();

    for (root, inputs) in swift_dirs {
        let Some((created_packages, created_dependencies)) = build_swift_outputs(&inputs) else {
            continue;
        };

        let package_uids: Vec<PackageUid> = created_packages
            .iter()
            .map(|package| package.package_uid.clone())
            .collect();

        assign_swift_resources(files, &root, &package_uids, &inputs, &swift_roots);
        packages.extend(created_packages);
        dependencies.extend(created_dependencies);
    }
}

#[derive(Default)]
struct SwiftDirectoryInputs {
    manifest: Option<SwiftSource>,
    show_dependencies: Option<SwiftSource>,
    resolved: Option<SwiftSource>,
}

#[derive(Clone)]
struct SwiftSource {
    file_index: usize,
    path: String,
    package_data: PackageData,
}

fn pick_manifest_source(
    existing: Option<SwiftSource>,
    candidate: SwiftSource,
) -> Option<SwiftSource> {
    match existing {
        None => Some(candidate),
        Some(current) => {
            if manifest_priority(&candidate.path) < manifest_priority(&current.path) {
                Some(candidate)
            } else {
                Some(current)
            }
        }
    }
}

fn manifest_priority(path: &str) -> u8 {
    match Path::new(path).file_name().and_then(|name| name.to_str()) {
        Some("Package.swift.json") => 0,
        Some("Package.swift.deplock") => 1,
        _ => 2,
    }
}

fn build_swift_outputs(
    inputs: &SwiftDirectoryInputs,
) -> Option<(Vec<Package>, Vec<TopLevelDependency>)> {
    if let Some(manifest) = &inputs.manifest {
        let manifest_datasource_id = manifest.package_data.datasource_id?;

        let mut package = Package::from_package_data(&manifest.package_data, manifest.path.clone());
        let processed_dependencies = if let Some(show_dependencies) = &inputs.show_dependencies {
            package.datafile_paths.push(show_dependencies.path.clone());
            if let Some(datasource_id) = show_dependencies.package_data.datasource_id {
                package.datasource_ids.push(datasource_id);
            }
            show_dependencies.package_data.dependencies.clone()
        } else if let Some(resolved) = &inputs.resolved {
            if let Some(datasource_id) = resolved.package_data.datasource_id {
                package.datasource_ids.push(datasource_id);
            }
            package.datafile_paths.push(resolved.path.clone());
            build_resolved_fallback_dependencies(
                &manifest.package_data.dependencies,
                &resolved.package_data.dependencies,
            )
        } else {
            manifest.package_data.dependencies.clone()
        };

        let package_uid = package.package_uid.clone();
        let dependencies = hoist_dependencies(
            &processed_dependencies,
            &manifest.path,
            manifest_datasource_id,
            Some(package_uid),
        );

        return Some((vec![package], dependencies));
    }

    if let Some(show_dependencies) = &inputs.show_dependencies {
        let datasource_id = show_dependencies.package_data.datasource_id?;
        let package = Package::from_package_data(
            &show_dependencies.package_data,
            show_dependencies.path.clone(),
        );
        let package_uid = package.package_uid.clone();
        let dependencies = hoist_dependencies(
            &show_dependencies.package_data.dependencies,
            &show_dependencies.path,
            datasource_id,
            Some(package_uid),
        );

        return Some((vec![package], dependencies));
    }

    if let Some(resolved) = &inputs.resolved {
        let resolved_packages = build_packages_from_resolved_dependencies(
            &resolved.package_data.dependencies,
            &resolved.path,
        );
        if resolved_packages.is_empty() {
            return None;
        }

        return Some((resolved_packages, Vec::new()));
    }

    None
}

fn hoist_dependencies(
    dependencies: &[Dependency],
    datafile_path: &str,
    datasource_id: DatasourceId,
    package_uid: Option<PackageUid>,
) -> Vec<TopLevelDependency> {
    dependencies
        .iter()
        .filter(|dependency| dependency.purl.is_some())
        .map(|dependency| {
            TopLevelDependency::from_dependency(
                dependency,
                datafile_path.to_string(),
                datasource_id,
                package_uid.clone(),
            )
        })
        .collect()
}

fn build_resolved_fallback_dependencies(
    manifest_dependencies: &[Dependency],
    resolved_dependencies: &[Dependency],
) -> Vec<Dependency> {
    let mut processed_dependencies = manifest_dependencies.to_vec();

    for dependency in &mut processed_dependencies {
        let Some(name) = dependency_name(dependency) else {
            continue;
        };

        let Some(resolved_dependency) = resolved_dependencies
            .iter()
            .find(|resolved| dependency_name(resolved).as_deref() == Some(&name))
        else {
            continue;
        };

        if resolved_dependency.purl.is_some() {
            dependency.purl = resolved_dependency.purl.clone();
        }
        if resolved_dependency.extracted_requirement.is_some() {
            dependency.extracted_requirement = resolved_dependency.extracted_requirement.clone();
        }
        if resolved_dependency.is_pinned.is_some() {
            dependency.is_pinned = resolved_dependency.is_pinned;
        }
    }

    processed_dependencies
}

fn build_packages_from_resolved_dependencies(
    resolved_dependencies: &[Dependency],
    datafile_path: &str,
) -> Vec<Package> {
    resolved_dependencies
        .iter()
        .filter_map(|dependency| build_package_from_resolved_dependency(dependency, datafile_path))
        .collect()
}

fn build_package_from_resolved_dependency(
    dependency: &Dependency,
    datafile_path: &str,
) -> Option<Package> {
    let purl = dependency.purl.as_deref()?;
    let parsed = PackageUrl::from_str(purl).ok()?;

    Some(Package {
        package_type: Some(PackageType::Swift),
        namespace: parsed.namespace().map(|namespace| namespace.to_string()),
        name: Some(parsed.name().to_string()),
        version: parsed.version().map(|version| version.to_string()),
        qualifiers: None,
        subpath: None,
        primary_language: Some("swift".to_string()),
        description: None,
        release_date: None,
        parties: Vec::new(),
        keywords: Vec::new(),
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
        license_detections: Vec::new(),
        other_license_expression: None,
        other_license_expression_spdx: None,
        other_license_detections: Vec::new(),
        extracted_license_statement: None,
        notice_text: None,
        source_packages: Vec::new(),
        is_private: false,
        is_virtual: false,
        extra_data: None,
        repository_homepage_url: None,
        repository_download_url: None,
        api_data_url: None,
        purl: Some(purl.to_string()),
        package_uid: PackageUid::new(purl),
        datafile_paths: vec![datafile_path.to_string()],
        datasource_ids: vec![DatasourceId::SwiftPackageResolved],
    })
}

fn dependency_name(dependency: &Dependency) -> Option<String> {
    let purl = dependency.purl.as_deref()?;
    PackageUrl::from_str(purl)
        .ok()
        .map(|parsed| parsed.name().to_string())
}

fn assign_swift_resources(
    files: &mut [FileInfo],
    root: &Path,
    package_uids: &[PackageUid],
    inputs: &SwiftDirectoryInputs,
    swift_roots: &[PathBuf],
) {
    let swift_file_indices = [
        inputs.manifest.as_ref().map(|source| source.file_index),
        inputs
            .show_dependencies
            .as_ref()
            .map(|source| source.file_index),
        inputs.resolved.as_ref().map(|source| source.file_index),
    ];

    for (index, file) in files.iter_mut().enumerate() {
        let path = Path::new(&file.path);
        if !path.starts_with(root) {
            continue;
        }

        if swift_roots.iter().any(|other_root| {
            other_root != root && other_root.starts_with(root) && path.starts_with(other_root)
        }) {
            continue;
        }

        if is_internal_cache_path(path, root) {
            continue;
        }

        if swift_file_indices.contains(&Some(index)) || path.starts_with(root) {
            for package_uid in package_uids {
                if !file.for_packages.contains(package_uid) {
                    file.for_packages.push(package_uid.clone());
                }
            }
        }
    }
}

fn is_swift_manifest_filename(file_name: &str) -> bool {
    matches!(file_name, "Package.swift.json" | "Package.swift.deplock")
}

fn is_swift_resolved_filename(file_name: &str) -> bool {
    matches!(file_name, "Package.resolved" | ".package.resolved")
}

fn is_internal_cache_path(path: &Path, root: &Path) -> bool {
    path.strip_prefix(root)
        .ok()
        .and_then(|relative| relative.components().next())
        .is_some_and(|component| component.as_os_str() == DEFAULT_CACHE_DIR_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;

    fn file(path: &str, package_data: Vec<PackageData>) -> FileInfo {
        FileInfo::new(
            Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string(),
            Path::new(path)
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string(),
            Path::new(path)
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| format!(".{ext}"))
                .unwrap_or_default(),
            path.to_string(),
            crate::models::FileType::File,
            None,
            None,
            1,
            None,
            None,
            None,
            None,
            None,
            package_data,
            None,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
    }

    #[test]
    fn build_swift_outputs_keeps_manifest_root_metadata() {
        let manifest = SwiftSource {
            file_index: 0,
            path: "Package.swift.json".to_string(),
            package_data: PackageData {
                package_type: Some(PackageType::Swift),
                name: Some("RootPkg".to_string()),
                primary_language: Some("Swift".to_string()),
                homepage_url: Some("https://manifest.example/root".to_string()),
                extra_data: Some(HashMap::from([("platforms".to_string(), json!(["ios"]))])),
                dependencies: vec![Dependency {
                    purl: Some("pkg:swift/manifest-dep".to_string()),
                    extracted_requirement: None,
                    scope: Some("dependencies".to_string()),
                    is_runtime: None,
                    is_optional: Some(false),
                    is_pinned: Some(false),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                }],
                datasource_id: Some(DatasourceId::SwiftPackageManifestJson),
                purl: Some("pkg:swift/RootPkg".to_string()),
                ..Default::default()
            },
        };

        let show_dependencies = SwiftSource {
            file_index: 1,
            path: "swift-show-dependencies.deplock".to_string(),
            package_data: PackageData {
                package_type: Some(PackageType::Swift),
                name: Some("DifferentRoot".to_string()),
                version: Some("9.9.9".to_string()),
                primary_language: Some("Swift".to_string()),
                homepage_url: Some("https://showdeps.example/root".to_string()),
                extra_data: Some(HashMap::from([(
                    "from_show_dependencies".to_string(),
                    json!(true),
                )])),
                dependencies: vec![Dependency {
                    purl: Some("pkg:swift/github.com/example/showdep@1.2.3".to_string()),
                    extracted_requirement: Some("1.2.3".to_string()),
                    scope: Some("dependencies".to_string()),
                    is_runtime: None,
                    is_optional: Some(false),
                    is_pinned: Some(true),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                }],
                datasource_id: Some(DatasourceId::SwiftPackageShowDependencies),
                purl: Some("pkg:swift/DifferentRoot@9.9.9".to_string()),
                ..Default::default()
            },
        };

        let (packages, dependencies) = build_swift_outputs(&SwiftDirectoryInputs {
            manifest: Some(manifest),
            show_dependencies: Some(show_dependencies),
            resolved: None,
        })
        .expect("swift outputs should be created");

        assert_eq!(packages.len(), 1);
        let package = &packages[0];
        assert_eq!(package.name.as_deref(), Some("RootPkg"));
        assert_eq!(package.version, None);
        assert_eq!(
            package.homepage_url.as_deref(),
            Some("https://manifest.example/root")
        );
        assert_eq!(
            package
                .extra_data
                .as_ref()
                .and_then(|extra| extra.get("platforms")),
            Some(&json!(["ios"]))
        );
        assert!(
            package
                .extra_data
                .as_ref()
                .and_then(|extra| extra.get("from_show_dependencies"))
                .is_none()
        );
        assert!(
            package
                .datafile_paths
                .contains(&"Package.swift.json".to_string())
        );
        assert!(
            package
                .datafile_paths
                .contains(&"swift-show-dependencies.deplock".to_string())
        );
        assert_eq!(dependencies.len(), 1);
        assert_eq!(
            dependencies[0].purl.as_deref(),
            Some("pkg:swift/github.com/example/showdep@1.2.3")
        );
        assert_eq!(dependencies[0].datafile_path, "Package.swift.json");
        assert_eq!(
            dependencies[0].datasource_id,
            DatasourceId::SwiftPackageManifestJson
        );
    }

    #[test]
    fn assign_swift_resources_skips_nested_swift_roots() {
        let mut files = vec![
            file(
                "Package.swift.json",
                vec![PackageData {
                    datasource_id: Some(DatasourceId::SwiftPackageManifestJson),
                    purl: Some("pkg:swift/RootPkg".to_string()),
                    ..Default::default()
                }],
            ),
            file("Sources/App.swift", Vec::new()),
            file(
                "examples/demo/Package.swift.json",
                vec![PackageData {
                    datasource_id: Some(DatasourceId::SwiftPackageManifestJson),
                    purl: Some("pkg:swift/DemoPkg".to_string()),
                    ..Default::default()
                }],
            ),
            file("examples/demo/Sources/Demo.swift", Vec::new()),
        ];

        assign_swift_resources(
            &mut files,
            Path::new(""),
            &[PackageUid::from_raw(
                "pkg:swift/RootPkg?uuid=root".to_string(),
            )],
            &SwiftDirectoryInputs {
                manifest: Some(SwiftSource {
                    file_index: 0,
                    path: "Package.swift.json".to_string(),
                    package_data: PackageData {
                        datasource_id: Some(DatasourceId::SwiftPackageManifestJson),
                        ..Default::default()
                    },
                }),
                show_dependencies: None,
                resolved: None,
            },
            &[PathBuf::new(), PathBuf::from("examples/demo")],
        );

        assert_eq!(
            files[0].for_packages,
            vec![PackageUid::from_raw(
                "pkg:swift/RootPkg?uuid=root".to_string()
            )]
        );
        assert_eq!(
            files[1].for_packages,
            vec![PackageUid::from_raw(
                "pkg:swift/RootPkg?uuid=root".to_string()
            )]
        );
        assert!(files[2].for_packages.is_empty());
        assert!(files[3].for_packages.is_empty());
    }

    #[test]
    fn build_resolved_fallback_dependencies_only_enriches_manifest_known_deps() {
        let manifest_dependencies = vec![Dependency {
            purl: Some("pkg:swift/github.com/mapbox/turf-swift".to_string()),
            extracted_requirement: Some("vers:swift/>=2.8.0|<3.0.0".to_string()),
            scope: Some("dependencies".to_string()),
            is_runtime: None,
            is_optional: Some(false),
            is_pinned: Some(false),
            is_direct: Some(true),
            resolved_package: None,
            extra_data: None,
        }];

        let resolved_dependencies = vec![
            Dependency {
                purl: Some("pkg:swift/github.com/mapbox/turf-swift@2.8.0".to_string()),
                extracted_requirement: Some("2.8.0".to_string()),
                scope: Some("dependencies".to_string()),
                is_runtime: None,
                is_optional: Some(false),
                is_pinned: Some(true),
                is_direct: None,
                resolved_package: None,
                extra_data: None,
            },
            Dependency {
                purl: Some("pkg:swift/github.com/mapbox/mapbox-common-ios@24.4.0".to_string()),
                extracted_requirement: Some("24.4.0".to_string()),
                scope: Some("dependencies".to_string()),
                is_runtime: None,
                is_optional: Some(false),
                is_pinned: Some(true),
                is_direct: None,
                resolved_package: None,
                extra_data: None,
            },
        ];

        let processed =
            build_resolved_fallback_dependencies(&manifest_dependencies, &resolved_dependencies);

        assert_eq!(processed.len(), 1);
        assert_eq!(
            processed[0].purl.as_deref(),
            Some("pkg:swift/github.com/mapbox/turf-swift@2.8.0")
        );
        assert_eq!(processed[0].extracted_requirement.as_deref(), Some("2.8.0"));
        assert_eq!(processed[0].is_direct, Some(true));
        assert_eq!(processed[0].is_runtime, None);
    }
}
