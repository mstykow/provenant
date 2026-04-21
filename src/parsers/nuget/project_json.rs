// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};
use crate::parser_warn as warn;

use super::super::PackageParser;
use super::super::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};
use super::{
    build_nuget_party, build_nuget_purl, build_nuget_urls, default_package_data,
    insert_extra_string,
};

pub struct ProjectJsonParser;

impl PackageParser for ProjectJsonParser {
    const PACKAGE_TYPE: PackageType = PackageType::Nuget;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "project.json")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read project.json at {:?}: {}", path, e);
                return vec![default_package_data(Some(DatasourceId::NugetProjectJson))];
            }
        };

        let parsed: serde_json::Value = match serde_json::from_str(&content) {
            Ok(value) => value,
            Err(e) => {
                warn!("Failed to parse project.json at {:?}: {}", path, e);
                return vec![default_package_data(Some(DatasourceId::NugetProjectJson))];
            }
        };

        vec![parse_project_json_manifest(&parsed)]
    }
}

pub struct ProjectLockJsonParser;

impl PackageParser for ProjectLockJsonParser {
    const PACKAGE_TYPE: PackageType = PackageType::Nuget;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "project.lock.json")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read project.lock.json at {:?}: {}", path, e);
                return vec![default_package_data(Some(
                    DatasourceId::NugetProjectLockJson,
                ))];
            }
        };

        let parsed: serde_json::Value = match serde_json::from_str(&content) {
            Ok(value) => value,
            Err(e) => {
                warn!("Failed to parse project.lock.json at {:?}: {}", path, e);
                return vec![default_package_data(Some(
                    DatasourceId::NugetProjectLockJson,
                ))];
            }
        };

        vec![parse_project_lock_manifest(&parsed)]
    }
}

fn parse_project_json_manifest(parsed: &serde_json::Value) -> PackageData {
    let name = parsed
        .get("name")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let version = parsed
        .get("version")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let description = parsed
        .get("description")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let homepage_url = parsed
        .get("projectUrl")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let extracted_license_statement = parsed
        .get("license")
        .or_else(|| parsed.get("licenseUrl"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());

    let mut parties = Vec::new();
    if let Some(authors) = parsed.get("authors") {
        let author_name = if let Some(value) = authors.as_str() {
            Some(value.to_string())
        } else {
            authors.as_array().map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| entry.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
        };

        if let Some(author_name) = author_name.filter(|value| !value.is_empty()) {
            parties.push(build_nuget_party("author", author_name));
        }
    }

    let mut dependencies = Vec::new();

    if let Some(root_dependencies) = parsed
        .get("dependencies")
        .and_then(|value| value.as_object())
    {
        for (dependency_name, dependency_spec) in root_dependencies.iter().take(MAX_ITERATION_COUNT)
        {
            if let Some(dependency) =
                parse_project_json_dependency(dependency_name, dependency_spec, None)
            {
                dependencies.push(dependency);
            }
        }
    }

    if let Some(frameworks) = parsed.get("frameworks").and_then(|value| value.as_object()) {
        for (framework, framework_value) in frameworks.iter().take(MAX_ITERATION_COUNT) {
            let Some(framework_dependencies) = framework_value
                .get("dependencies")
                .and_then(|value| value.as_object())
            else {
                continue;
            };

            for (dependency_name, dependency_spec) in
                framework_dependencies.iter().take(MAX_ITERATION_COUNT)
            {
                if let Some(dependency) = parse_project_json_dependency(
                    dependency_name,
                    dependency_spec,
                    Some(framework.clone()),
                ) {
                    dependencies.push(dependency);
                }
            }
        }
    }

    let (repository_homepage_url, repository_download_url, api_data_url) =
        build_nuget_urls(name.as_deref(), version.as_deref());

    PackageData {
        datasource_id: Some(DatasourceId::NugetProjectJson),
        package_type: Some(PackageType::Nuget),
        name: name.clone().map(truncate_field),
        version: version.clone().map(truncate_field),
        purl: build_nuget_purl(name.as_deref(), version.as_deref()),
        description: description.map(truncate_field),
        homepage_url: homepage_url.map(truncate_field),
        parties,
        dependencies,
        extracted_license_statement: extracted_license_statement.map(truncate_field),
        repository_homepage_url,
        repository_download_url,
        api_data_url,
        ..default_package_data(Some(DatasourceId::NugetProjectJson))
    }
}

fn parse_project_json_dependency(
    dependency_name: &str,
    dependency_spec: &serde_json::Value,
    scope: Option<String>,
) -> Option<Dependency> {
    let mut extra_data = serde_json::Map::new();

    let requirement = match dependency_spec {
        serde_json::Value::String(version) => Some(version.clone()),
        serde_json::Value::Object(object) => {
            let requirement = object
                .get("version")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string());
            insert_extra_string(
                &mut extra_data,
                "include",
                object
                    .get("include")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string()),
            );
            insert_extra_string(
                &mut extra_data,
                "exclude",
                object
                    .get("exclude")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string()),
            );
            insert_extra_string(
                &mut extra_data,
                "type",
                object
                    .get("type")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string()),
            );
            requirement
        }
        _ => return None,
    };

    Some(Dependency {
        purl: build_nuget_purl(Some(dependency_name), None),
        extracted_requirement: requirement,
        scope,
        is_runtime: Some(true),
        is_optional: Some(false),
        is_pinned: Some(false),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: if extra_data.is_empty() {
            None
        } else {
            Some(extra_data.into_iter().collect())
        },
    })
}

fn parse_project_lock_manifest(parsed: &serde_json::Value) -> PackageData {
    let mut dependencies = Vec::new();

    if let Some(groups) = parsed
        .get("projectFileDependencyGroups")
        .and_then(|value| value.as_object())
    {
        for (framework, entries) in groups.iter().take(MAX_ITERATION_COUNT) {
            let Some(entries) = entries.as_array() else {
                continue;
            };

            for entry in entries
                .iter()
                .take(MAX_ITERATION_COUNT)
                .filter_map(|value| value.as_str())
            {
                if let Some(dependency) = parse_project_lock_dependency(
                    entry,
                    (!framework.is_empty()).then(|| framework.clone()),
                ) {
                    dependencies.push(dependency);
                }
            }
        }
    }

    PackageData {
        datasource_id: Some(DatasourceId::NugetProjectLockJson),
        package_type: Some(PackageType::Nuget),
        dependencies,
        ..default_package_data(Some(DatasourceId::NugetProjectLockJson))
    }
}

fn parse_project_lock_dependency(entry: &str, scope: Option<String>) -> Option<Dependency> {
    let trimmed = entry.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut parts = trimmed.split_whitespace();
    let name = parts.next()?;
    let requirement = parts.collect::<Vec<_>>().join(" ");

    Some(Dependency {
        purl: build_nuget_purl(Some(name), None),
        extracted_requirement: (!requirement.is_empty()).then_some(requirement),
        scope,
        is_runtime: Some(true),
        is_optional: Some(false),
        is_pinned: Some(false),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: None,
    })
}

crate::register_parser!(
    ".NET project.json manifest",
    &["**/project.json"],
    "nuget",
    "C#",
    Some("https://learn.microsoft.com/en-us/nuget/archive/project-json"),
);

crate::register_parser!(
    ".NET project.lock.json lockfile",
    &["**/project.lock.json"],
    "nuget",
    "C#",
    Some("https://learn.microsoft.com/en-us/nuget/archive/project-json"),
);
