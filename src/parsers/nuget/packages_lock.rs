use std::path::Path;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};
use crate::parser_warn as warn;
use packageurl::PackageUrl;

use super::super::PackageParser;
use super::super::utils::{MAX_ITERATION_COUNT, read_file_to_string};
use super::default_package_data;

pub struct PackagesLockParser;

impl PackageParser for PackagesLockParser {
    const PACKAGE_TYPE: PackageType = PackageType::Nuget;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("packages.lock.json"))
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read packages.lock.json at {:?}: {}", path, e);
                return vec![default_package_data(Some(DatasourceId::NugetPackagesLock))];
            }
        };

        let parsed: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                warn!("Failed to parse packages.lock.json at {:?}: {}", path, e);
                return vec![default_package_data(Some(DatasourceId::NugetPackagesLock))];
            }
        };

        let mut dependencies = Vec::new();
        let mut iteration_count: usize = 0;

        if let Some(deps_obj) = parsed.get("dependencies").and_then(|v| v.as_object()) {
            for (target_framework, packages) in deps_obj.iter().take(MAX_ITERATION_COUNT) {
                if let Some(packages_obj) = packages.as_object() {
                    for (package_name, package_info) in
                        packages_obj.iter().take(MAX_ITERATION_COUNT)
                    {
                        iteration_count += 1;
                        if iteration_count > MAX_ITERATION_COUNT {
                            warn!(
                                "Iteration limit exceeded in packages.lock.json at {:?}; stopping at {} dependencies",
                                path, MAX_ITERATION_COUNT
                            );
                            break;
                        }
                        if let Some(info_obj) = package_info.as_object() {
                            let version = info_obj
                                .get("resolved")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());

                            let requested = info_obj
                                .get("requested")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());

                            let package_type = info_obj.get("type").and_then(|v| v.as_str());

                            let is_direct = match package_type {
                                Some("Direct") => Some(true),
                                Some("Transitive") => Some(false),
                                _ => None,
                            };

                            let purl = version.as_ref().and_then(|v| {
                                PackageUrl::new("nuget", package_name).ok().map(|mut p| {
                                    let _ = p.with_version(v);
                                    p.to_string()
                                })
                            });

                            let mut extra_data = serde_json::Map::new();
                            extra_data.insert(
                                "target_framework".to_string(),
                                serde_json::Value::String(target_framework.clone()),
                            );

                            if let Some(content_hash) =
                                info_obj.get("contentHash").and_then(|v| v.as_str())
                            {
                                extra_data.insert(
                                    "content_hash".to_string(),
                                    serde_json::Value::String(content_hash.to_string()),
                                );
                            }

                            dependencies.push(Dependency {
                                purl,
                                extracted_requirement: requested.or(version),
                                scope: Some(target_framework.clone()),
                                is_runtime: Some(true),
                                is_optional: Some(false),
                                is_pinned: Some(true),
                                is_direct,
                                resolved_package: None,
                                extra_data: if extra_data.is_empty() {
                                    None
                                } else {
                                    Some(extra_data.into_iter().collect())
                                },
                            });
                        }
                    }
                }
            }
        }

        vec![PackageData {
            datasource_id: Some(DatasourceId::NugetPackagesLock),
            package_type: Some(Self::PACKAGE_TYPE),
            dependencies,
            ..default_package_data(Some(DatasourceId::NugetPackagesLock))
        }]
    }
}

crate::register_parser!(
    ".NET packages.lock.json lockfile",
    &["**/packages.lock.json"],
    "nuget",
    "C#",
    Some(
        "https://learn.microsoft.com/en-us/nuget/consume-packages/package-references-in-project-files#locking-dependencies"
    ),
);
