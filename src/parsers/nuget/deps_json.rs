// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};
use crate::parser_warn as warn;

use super::super::PackageParser;
use super::super::utils::{MAX_ITERATION_COUNT, read_file_to_string};
use super::{build_nuget_purl, build_nuget_urls, default_package_data};

pub struct DotNetDepsJsonParser;

impl PackageParser for DotNetDepsJsonParser {
    const PACKAGE_TYPE: PackageType = PackageType::Nuget;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".deps.json"))
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read .deps.json at {:?}: {}", path, e);
                return vec![default_package_data(Some(DatasourceId::NugetDepsJson))];
            }
        };

        let parsed: serde_json::Value = match serde_json::from_str(&content) {
            Ok(value) => value,
            Err(e) => {
                warn!("Failed to parse .deps.json at {:?}: {}", path, e);
                return vec![default_package_data(Some(DatasourceId::NugetDepsJson))];
            }
        };

        vec![parse_dotnet_deps_json(&parsed, path)]
    }
}

fn parse_dotnet_deps_json(parsed: &serde_json::Value, path: &Path) -> PackageData {
    let Some(libraries) = parsed.get("libraries").and_then(|value| value.as_object()) else {
        return default_package_data(Some(DatasourceId::NugetDepsJson));
    };

    let Some((selected_target_name, selected_target)) = select_deps_target(parsed) else {
        return default_package_data(Some(DatasourceId::NugetDepsJson));
    };

    let root_key = select_root_library_key(path, libraries, &selected_target);
    let root_dependencies = root_key
        .as_deref()
        .and_then(|root_key| selected_target.get(root_key))
        .and_then(|value| value.get("dependencies"))
        .and_then(|value| value.as_object())
        .cloned()
        .unwrap_or_default();

    let mut dependencies = Vec::new();
    let mut iteration_count: usize = 0;
    for (library_key, target_entry) in selected_target.iter().take(MAX_ITERATION_COUNT) {
        iteration_count += 1;
        if iteration_count > MAX_ITERATION_COUNT {
            warn!(
                "Iteration limit exceeded in .deps.json at {:?}; stopping at {} dependencies",
                path, MAX_ITERATION_COUNT
            );
            break;
        }
        if root_key.as_deref() == Some(library_key.as_str()) {
            continue;
        }

        let Some((name, version)) = split_library_key(library_key) else {
            continue;
        };
        let Some(library_metadata) = libraries
            .get(library_key)
            .and_then(|value| value.as_object())
        else {
            continue;
        };

        let mut extra_data = serde_json::Map::new();
        extra_data.insert(
            "target_name".to_string(),
            serde_json::Value::String(selected_target_name.clone()),
        );

        for field in [
            "type",
            "sha512",
            "path",
            "hashPath",
            "runtimeStoreManifestName",
        ] {
            if let Some(value) = library_metadata.get(field) {
                extra_data.insert(field.to_string(), value.clone());
            }
        }

        if let Some(value) = library_metadata.get("serviceable") {
            extra_data.insert("serviceable".to_string(), value.clone());
        }

        if let Some(object) = target_entry.as_object() {
            for field in ["runtime", "native", "runtimeTargets", "resources"] {
                if let Some(value) = object.get(field) {
                    extra_data.insert(field.to_string(), value.clone());
                }
            }
            if let Some(value) = object.get("compileOnly") {
                extra_data.insert("compileOnly".to_string(), value.clone());
            }
        }

        let is_direct = if root_key.is_some() {
            Some(root_dependencies.contains_key(name))
        } else {
            None
        };

        let compile_only = target_entry
            .get("compileOnly")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);

        dependencies.push(Dependency {
            purl: build_nuget_purl(Some(name), Some(version)),
            extracted_requirement: Some(version.to_string()),
            scope: Some(selected_target_name.clone()),
            is_runtime: Some(!compile_only),
            is_optional: Some(compile_only),
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

    let mut package_data = if let Some(root_key) = root_key {
        let (name, version) = split_library_key(&root_key).unwrap_or(("", ""));
        let mut package = default_package_data(Some(DatasourceId::NugetDepsJson));
        package.name = (!name.is_empty()).then(|| name.to_string());
        package.version = (!version.is_empty()).then(|| version.to_string());
        package.purl = build_nuget_purl(package.name.as_deref(), package.version.as_deref());
        let (repository_homepage_url, repository_download_url, api_data_url) =
            build_nuget_urls(package.name.as_deref(), package.version.as_deref());
        package.repository_homepage_url = repository_homepage_url;
        package.repository_download_url = repository_download_url;
        package.api_data_url = api_data_url;
        package
    } else {
        let mut package = default_package_data(Some(DatasourceId::NugetDepsJson));
        let file_stem = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(".deps.json"))
            .filter(|name| !name.trim().is_empty())
            .map(|name| name.to_string());
        package.name = file_stem.clone();
        package.purl = build_nuget_purl(file_stem.as_deref(), None);
        package
    };

    let mut extra_data = serde_json::Map::new();
    if let Some(runtime_target) = parsed
        .get("runtimeTarget")
        .and_then(|value| value.as_object())
    {
        if let Some(name) = runtime_target.get("name").and_then(|value| value.as_str()) {
            extra_data.insert(
                "runtime_target_name".to_string(),
                serde_json::Value::String(name.to_string()),
            );
            if let Some((framework, runtime_identifier)) = name.split_once('/') {
                extra_data.insert(
                    "target_framework".to_string(),
                    serde_json::Value::String(framework.to_string()),
                );
                extra_data.insert(
                    "runtime_identifier".to_string(),
                    serde_json::Value::String(runtime_identifier.to_string()),
                );
            } else {
                extra_data.insert(
                    "target_framework".to_string(),
                    serde_json::Value::String(name.to_string()),
                );
            }
        }
        if let Some(signature) = runtime_target.get("signature") {
            extra_data.insert("runtime_signature".to_string(), signature.clone());
        }
    } else {
        extra_data.insert(
            "target_name".to_string(),
            serde_json::Value::String(selected_target_name.clone()),
        );
        if let Some((framework, runtime_identifier)) = selected_target_name.split_once('/') {
            extra_data.insert(
                "target_framework".to_string(),
                serde_json::Value::String(framework.to_string()),
            );
            extra_data.insert(
                "runtime_identifier".to_string(),
                serde_json::Value::String(runtime_identifier.to_string()),
            );
        } else {
            extra_data.insert(
                "target_framework".to_string(),
                serde_json::Value::String(selected_target_name.clone()),
            );
        }
    }

    package_data.dependencies = dependencies;
    package_data.extra_data = if extra_data.is_empty() {
        None
    } else {
        Some(extra_data.into_iter().collect())
    };
    package_data
}

fn select_deps_target(
    parsed: &serde_json::Value,
) -> Option<(String, serde_json::Map<String, serde_json::Value>)> {
    let targets = parsed.get("targets")?.as_object()?;

    if let Some(runtime_target_name) = parsed
        .get("runtimeTarget")
        .and_then(|value| value.get("name"))
        .and_then(|value| value.as_str())
        && let Some(target) = targets
            .get(runtime_target_name)
            .and_then(|value| value.as_object())
    {
        return Some((runtime_target_name.to_string(), target.clone()));
    }

    if let Some((name, value)) = targets
        .iter()
        .find(|(name, value)| name.contains('/') && value.is_object())
        && let Some(target) = value.as_object()
    {
        return Some((name.clone(), target.clone()));
    }

    targets.iter().find_map(|(name, value)| {
        value
            .as_object()
            .map(|target| (name.clone(), target.clone()))
    })
}

fn select_root_library_key(
    path: &Path,
    libraries: &serde_json::Map<String, serde_json::Value>,
    target: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    let base_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(".deps.json"));

    let project_keys: Vec<String> = target
        .keys()
        .filter(|key| {
            libraries
                .get(*key)
                .and_then(|value| value.get("type"))
                .and_then(|value| value.as_str())
                == Some("project")
        })
        .cloned()
        .collect();

    if let Some(base_name) = base_name
        && let Some(matched) = project_keys.iter().find(|key| {
            split_library_key(key)
                .map(|(name, _)| name.eq_ignore_ascii_case(base_name))
                .unwrap_or(false)
        })
    {
        return Some(matched.clone());
    }

    project_keys.into_iter().next()
}

fn split_library_key(key: &str) -> Option<(&str, &str)> {
    key.rsplit_once('/')
}

crate::register_parser!(
    ".NET .deps.json runtime dependency graph",
    &["**/*.deps.json"],
    "nuget",
    "C#",
    Some("https://learn.microsoft.com/en-us/dotnet/core/dependency-loading/default-probing"),
);
