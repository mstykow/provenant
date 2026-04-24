// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Parser for Dart/Flutter pubspec.yaml and pubspec.lock files.
//!
//! Extracts package metadata and dependencies from Dart/Flutter project manifest
//! and lockfiles using YAML format.
//!
//! # Supported Formats
//! - pubspec.yaml (Dart package manifest)
//! - pubspec.lock (Dart package lockfile with pinned versions)
//!
//! # Key Features
//! - Dependency extraction from dependencies and dev_dependencies sections
//! - Direct vs transitive dependency tracking (lockfile)
//! - Version constraint parsing for Dart's SemVer and range specifiers
//! - Package URL (purl) generation for Pub packages
//! - Author/maintainer and homepage extraction
//!
//! # Implementation Notes
//! - Uses YAML parsing via `yaml_serde`
//! - Lockfile versions are pinned (`is_pinned: Some(true)`)
//! - Graceful error handling with `warn!()` logs
//! - Supports both pub.dev and Git-hosted packages

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use crate::parser_warn as warn;
use crate::parsers::utils::{
    MAX_ITERATION_COUNT, RecursionGuard, read_file_to_string, truncate_field,
};
use packageurl::PackageUrl;
use yaml_serde::{Mapping, Value};

use crate::models::{
    DatasourceId, Dependency, PackageData, PackageType, ResolvedPackage, Sha256Digest,
};

use super::PackageParser;

const FIELD_NAME: &str = "name";
const FIELD_VERSION: &str = "version";
const FIELD_DESCRIPTION: &str = "description";
const FIELD_HOMEPAGE: &str = "homepage";
const FIELD_LICENSE: &str = "license";
const FIELD_REPOSITORY: &str = "repository";
const FIELD_AUTHOR: &str = "author";
const FIELD_AUTHORS: &str = "authors";
const FIELD_DEPENDENCIES: &str = "dependencies";
const FIELD_DEV_DEPENDENCIES: &str = "dev_dependencies";
const FIELD_DEPENDENCY_OVERRIDES: &str = "dependency_overrides";
const FIELD_ENVIRONMENT: &str = "environment";
const FIELD_ISSUE_TRACKER: &str = "issue_tracker";
const FIELD_DOCUMENTATION: &str = "documentation";
const FIELD_EXECUTABLES: &str = "executables";
const FIELD_PUBLISH_TO: &str = "publish_to";
const FIELD_ARCHIVE_URL: &str = "archive_url";
const FIELD_PLATFORMS: &str = "platforms";
const FIELD_FUNDING: &str = "funding";
const FIELD_FALSE_SECRETS: &str = "false_secrets";
const FIELD_SCREENSHOTS: &str = "screenshots";
const FIELD_TOPICS: &str = "topics";
const FIELD_IGNORED_ADVISORIES: &str = "ignored_advisories";
const FIELD_PACKAGES: &str = "packages";
const FIELD_SDKS: &str = "sdks";
const FIELD_SDK: &str = "sdk";
const FIELD_DEPENDENCY: &str = "dependency";
const FIELD_SHA256: &str = "sha256";

/// Dart pubspec.yaml manifest parser.
pub struct PubspecYamlParser;

impl PackageParser for PubspecYamlParser {
    const PACKAGE_TYPE: PackageType = PackageType::Dart;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let yaml_content = match read_yaml_file(path) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read pubspec.yaml at {:?}: {}", path, e);
                let mut package_data = default_package_data();
                package_data.datasource_id = Some(DatasourceId::PubspecYaml);
                return vec![package_data];
            }
        };

        vec![parse_pubspec_yaml(&yaml_content)]
    }

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                name == "pubspec.yaml"
                    || name.ends_with("-pubspec.yaml")
                    || name.ends_with(".pubspec.yaml")
            })
    }
}

/// Dart pubspec.lock lockfile parser.
pub struct PubspecLockParser;

impl PackageParser for PubspecLockParser {
    const PACKAGE_TYPE: PackageType = PackageType::Pubspec;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let yaml_content = match read_yaml_file(path) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read pubspec.lock at {:?}: {}", path, e);
                let mut package_data =
                    default_package_data_with_type(PubspecLockParser::PACKAGE_TYPE.as_str());
                package_data.datasource_id = Some(DatasourceId::PubspecLock);
                return vec![package_data];
            }
        };

        vec![parse_pubspec_lock(&yaml_content)]
    }

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                name == "pubspec.lock"
                    || name.ends_with("-pubspec.lock")
                    || name.ends_with(".pubspec.lock")
            })
    }
}

fn read_yaml_file(path: &Path) -> Result<Value, String> {
    let content =
        read_file_to_string(path, None).map_err(|e| format!("Failed to read file: {}", e))?;
    yaml_serde::from_str(&content).map_err(|e| format!("Failed to parse YAML: {}", e))
}

fn parse_pubspec_yaml(yaml_content: &Value) -> PackageData {
    let name = extract_string_field(yaml_content, FIELD_NAME).map(truncate_field);
    let version = extract_string_field(yaml_content, FIELD_VERSION).map(truncate_field);
    let description = extract_description_field(yaml_content).map(truncate_field);
    let homepage_url = extract_string_field(yaml_content, FIELD_HOMEPAGE).map(truncate_field);
    let raw_license = extract_string_field(yaml_content, FIELD_LICENSE).map(truncate_field);
    let vcs_url = extract_string_field(yaml_content, FIELD_REPOSITORY).map(truncate_field);
    let bug_tracking_url =
        extract_string_field(yaml_content, FIELD_ISSUE_TRACKER).map(truncate_field);
    let archive_url = extract_string_field(yaml_content, FIELD_ARCHIVE_URL).map(truncate_field);

    let parties = extract_authors(yaml_content);

    let declared_license_expression = None;
    let declared_license_expression_spdx = None;
    let license_detections = Vec::new();

    let dependencies = [
        collect_dependencies(
            yaml_content,
            FIELD_DEPENDENCIES,
            Some("dependencies"),
            true,
            false,
        ),
        collect_dependencies(
            yaml_content,
            FIELD_DEV_DEPENDENCIES,
            Some("dev_dependencies"),
            false,
            true,
        ),
        collect_dependencies(
            yaml_content,
            FIELD_DEPENDENCY_OVERRIDES,
            Some("dependency_overrides"),
            true,
            false,
        ),
        collect_dependencies(
            yaml_content,
            FIELD_ENVIRONMENT,
            Some("environment"),
            true,
            false,
        ),
    ]
    .concat();

    let extra_data = build_extra_data(yaml_content);
    let keywords = extract_string_list_field(yaml_content, FIELD_TOPICS);

    let purl = name
        .as_ref()
        .and_then(|name| build_purl(name, version.as_deref()))
        .map(truncate_field);

    let (api_data_url, repository_homepage_url, repository_download_url) =
        if let (Some(name_val), Some(version_val)) = (&name, &version) {
            (
                Some(truncate_field(format!(
                    "https://pub.dev/api/packages/{}/versions/{}",
                    name_val, version_val
                ))),
                Some(truncate_field(format!(
                    "https://pub.dev/packages/{}/versions/{}",
                    name_val, version_val
                ))),
                Some(truncate_field(format!(
                    "https://pub.dartlang.org/packages/{}/versions/{}.tar.gz",
                    name_val, version_val
                ))),
            )
        } else {
            (None, None, None)
        };

    let download_url = archive_url.or_else(|| repository_download_url.clone());

    PackageData {
        package_type: Some(PubspecYamlParser::PACKAGE_TYPE),
        namespace: None,
        name,
        version,
        qualifiers: None,
        subpath: None,
        primary_language: Some("dart".to_string()),
        description,
        release_date: None,
        parties,
        keywords,
        homepage_url,
        download_url,
        size: None,
        sha1: None,
        md5: None,
        sha256: None,
        sha512: None,
        bug_tracking_url,
        code_view_url: None,
        vcs_url,
        copyright: None,
        holder: None,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        other_license_expression: None,
        other_license_expression_spdx: None,
        other_license_detections: Vec::new(),
        extracted_license_statement: raw_license,
        notice_text: None,
        source_packages: Vec::new(),
        file_references: Vec::new(),
        is_private: yaml_content
            .get(FIELD_PUBLISH_TO)
            .and_then(Value::as_str)
            .is_some_and(|value| value.trim() == "none"),
        is_virtual: false,
        extra_data,
        dependencies,
        repository_homepage_url,
        repository_download_url,
        api_data_url,
        datasource_id: Some(DatasourceId::PubspecYaml),
        purl,
    }
}

fn parse_pubspec_lock(yaml_content: &Value) -> PackageData {
    let dependencies = extract_lock_dependencies(yaml_content);

    let mut package_data = default_package_data_with_type(PubspecLockParser::PACKAGE_TYPE.as_str());
    package_data.dependencies = dependencies;
    package_data.datasource_id = Some(DatasourceId::PubspecLock);
    package_data
}

fn extract_lock_dependencies(lock_data: &Value) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    if let Some(sdks) = lock_data.get(FIELD_SDKS).and_then(Value::as_mapping) {
        for (name_value, version_value) in sdks.iter().take(MAX_ITERATION_COUNT) {
            if let (Some(name), Some(version_str)) = (name_value.as_str(), version_value.as_str()) {
                let purl = build_dependency_purl(name, None).map(truncate_field);
                dependencies.push(Dependency {
                    purl,
                    extracted_requirement: Some(truncate_field(version_str.to_string())),
                    scope: Some("sdk".to_string()),
                    is_runtime: Some(true),
                    is_optional: Some(false),
                    is_pinned: Some(false),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                });
            }
        }
    } else if let Some(version_str) = lock_data.get(FIELD_SDK).and_then(Value::as_str) {
        let purl = build_dependency_purl("dart", None).map(truncate_field);
        dependencies.push(Dependency {
            purl,
            extracted_requirement: Some(truncate_field(version_str.to_string())),
            scope: Some("sdk".to_string()),
            is_runtime: Some(true),
            is_optional: Some(false),
            is_pinned: Some(false),
            is_direct: Some(true),
            resolved_package: None,
            extra_data: None,
        });
    }

    let Some(packages) = lock_data.get(FIELD_PACKAGES).and_then(Value::as_mapping) else {
        return dependencies;
    };

    let runtime_reachable =
        reachable_lock_packages(packages, &["direct main", "direct overridden"]);
    let dev_only_reachable = reachable_lock_packages(packages, &["direct dev"]);

    for (name_value, details_value) in packages.iter().take(MAX_ITERATION_COUNT) {
        let name = match name_value.as_str() {
            Some(value) => value,
            None => continue,
        };
        let Some(details) = details_value.as_mapping() else {
            continue;
        };

        let version = mapping_get(details, FIELD_VERSION)
            .and_then(Value::as_str)
            .map(|value| truncate_field(value.to_string()));
        let dependency_kind = mapping_get(details, FIELD_DEPENDENCY)
            .and_then(Value::as_str)
            .map(|value| truncate_field(value.to_string()));
        let (is_runtime, is_optional, is_direct) = classify_lock_dependency(
            name,
            dependency_kind.as_deref(),
            &runtime_reachable,
            &dev_only_reachable,
        );

        let is_pinned = version
            .as_ref()
            .is_some_and(|value| !value.trim().is_empty());

        let purl = build_dependency_purl(name, version.as_deref()).map(truncate_field);
        let sha256 = extract_sha256(details).and_then(|h| Sha256Digest::from_hex(&h).ok());
        let resolved_dependencies = extract_lock_package_dependencies(details);
        let resolved_package = build_resolved_package(
            name,
            &version,
            sha256,
            extract_lock_descriptor_extra_data(details),
            resolved_dependencies,
        );

        dependencies.push(Dependency {
            purl,
            extracted_requirement: version.clone().map(truncate_field),
            scope: dependency_kind,
            is_runtime: Some(is_runtime),
            is_optional: Some(is_optional),
            is_pinned: Some(is_pinned),
            is_direct: Some(is_direct),
            resolved_package: Some(Box::new(resolved_package)),
            extra_data: extract_lock_descriptor_extra_data(details),
        });
    }

    dependencies
}

fn extract_lock_package_dependencies(details: &Mapping) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    let Some(dep_map) = mapping_get(details, FIELD_DEPENDENCIES).and_then(Value::as_mapping) else {
        return dependencies;
    };

    for (name_value, requirement_value) in dep_map.iter().take(MAX_ITERATION_COUNT) {
        let name = match name_value.as_str() {
            Some(value) => value,
            None => continue,
        };

        let requirement = match dependency_requirement_from_value(requirement_value) {
            Some(value) => value,
            None => continue,
        };
        let is_pinned = is_pubspec_version_pinned(&requirement);
        let purl = if is_pinned {
            build_dependency_purl(name, Some(requirement.as_str()))
        } else {
            build_dependency_purl(name, None)
        };

        dependencies.push(Dependency {
            purl: purl.map(truncate_field),
            extracted_requirement: Some(truncate_field(requirement)),
            scope: Some(FIELD_DEPENDENCIES.to_string()),
            is_runtime: Some(true),
            is_optional: Some(false),
            is_pinned: Some(is_pinned),
            is_direct: Some(false),
            resolved_package: None,
            extra_data: None,
        });
    }

    dependencies
}

fn extract_sha256(details: &Mapping) -> Option<String> {
    let direct = mapping_get(details, FIELD_SHA256)
        .and_then(Value::as_str)
        .map(|value| value.to_string());

    if direct.is_some() {
        return direct;
    }

    mapping_get(details, FIELD_DESCRIPTION)
        .and_then(Value::as_mapping)
        .and_then(|desc_map| mapping_get(desc_map, FIELD_SHA256))
        .and_then(Value::as_str)
        .map(|value| value.to_string())
}

fn build_resolved_package(
    name: &str,
    version: &Option<String>,
    sha256: Option<Sha256Digest>,
    extra_data: Option<HashMap<String, serde_json::Value>>,
    dependencies: Vec<Dependency>,
) -> ResolvedPackage {
    ResolvedPackage {
        primary_language: Some("dart".to_string()),
        download_url: None,
        sha1: None,
        sha256,
        sha512: None,
        md5: None,
        is_virtual: true,
        extra_data,
        dependencies,
        repository_homepage_url: None,
        repository_download_url: None,
        api_data_url: None,
        datasource_id: None,
        purl: None,
        ..ResolvedPackage::new(
            PubspecLockParser::PACKAGE_TYPE,
            String::new(),
            truncate_field(name.to_string()),
            truncate_field(version.clone().unwrap_or_default()),
        )
    }
}

fn collect_dependencies(
    yaml_content: &Value,
    field: &str,
    scope: Option<&str>,
    is_runtime: bool,
    is_optional: bool,
) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    let Some(dep_map) = yaml_content.get(field).and_then(Value::as_mapping) else {
        return dependencies;
    };

    for (name_value, requirement_value) in dep_map.iter().take(MAX_ITERATION_COUNT) {
        let name = match name_value.as_str() {
            Some(value) => value,
            None => continue,
        };
        let requirement = dependency_requirement_from_value(requirement_value).map(truncate_field);
        let is_pinned = requirement
            .as_deref()
            .is_some_and(is_pubspec_version_pinned);
        let purl = if is_pinned {
            build_dependency_purl(name, requirement.as_deref())
        } else {
            build_dependency_purl(name, None)
        };

        dependencies.push(Dependency {
            purl: purl.map(truncate_field),
            extracted_requirement: requirement,
            scope: scope.map(|value| value.to_string()),
            is_runtime: Some(is_runtime),
            is_optional: Some(is_optional),
            is_pinned: Some(is_pinned),
            is_direct: Some(true),
            resolved_package: None,
            extra_data: extract_manifest_dependency_extra_data(requirement_value),
        });
    }

    dependencies
}

fn dependency_requirement_from_value(value: &Value) -> Option<String> {
    if let Some(value) = value.as_str() {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return None;
        }
        return Some(trimmed.to_string());
    }

    if let Some(value) = value.as_i64() {
        return Some(value.to_string());
    }

    if let Some(value) = value.as_f64() {
        return Some(value.to_string());
    }

    if let Some(map) = value.as_mapping() {
        return format_dependency_mapping(map, &mut RecursionGuard::depth_only());
    }

    None
}

fn format_dependency_mapping(map: &Mapping, guard: &mut RecursionGuard<()>) -> Option<String> {
    if guard.descend() {
        warn!("Recursion depth exceeded in format_dependency_mapping");
        return None;
    }

    let mut parts = Vec::new();

    for (key, value) in map.iter().take(MAX_ITERATION_COUNT) {
        let Some(key_str) = key.as_str() else {
            continue;
        };

        let value_str = if let Some(value) = value.as_str() {
            value.to_string()
        } else if let Some(value) = value.as_i64() {
            value.to_string()
        } else if let Some(value) = value.as_f64() {
            value.to_string()
        } else if let Some(nested) = value.as_mapping() {
            format_dependency_mapping(nested, guard)?
        } else {
            continue;
        };

        parts.push(format!("{}: {}", key_str, value_str));
    }

    guard.ascend();

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

fn is_pubspec_version_pinned(version: &str) -> bool {
    let trimmed = version.trim();
    if trimmed.is_empty() {
        return false;
    }

    trimmed
        .chars()
        .all(|character| character.is_ascii_digit() || character == '.')
}

fn build_purl(name: &str, version: Option<&str>) -> Option<String> {
    build_purl_with_type(PubspecYamlParser::PACKAGE_TYPE.as_str(), name, version)
}

fn build_dependency_purl(name: &str, version: Option<&str>) -> Option<String> {
    build_purl_with_type("pubspec", name, version)
}

fn build_purl_with_type(package_type: &str, name: &str, version: Option<&str>) -> Option<String> {
    let mut package_url = match PackageUrl::new(package_type, name) {
        Ok(purl) => purl,
        Err(e) => {
            warn!(
                "Failed to create PackageUrl for {} dependency '{}': {}",
                package_type, name, e
            );
            return None;
        }
    };

    if let Some(version) = version
        && let Err(e) = package_url.with_version(version)
    {
        warn!(
            "Failed to set version '{}' for {} dependency '{}': {}",
            version, package_type, name, e
        );
        return None;
    }

    Some(package_url.to_string())
}

fn extract_string_field(yaml_content: &Value, field: &str) -> Option<String> {
    yaml_content
        .get(field)
        .and_then(Value::as_str)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn extract_description_field(yaml_content: &Value) -> Option<String> {
    // For description fields, preserve trailing newlines as they are semantically
    // significant in YAML folded/literal scalars (> or |)
    yaml_content
        .get(FIELD_DESCRIPTION)
        .and_then(Value::as_str)
        .and_then(|value| {
            // Only trim leading whitespace, preserve trailing newlines
            let trimmed = value.trim_start();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
}

fn mapping_get<'a>(map: &'a Mapping, key: &str) -> Option<&'a Value> {
    map.get(Value::String(key.to_string()))
}

fn default_package_data() -> PackageData {
    default_package_data_with_type(PubspecYamlParser::PACKAGE_TYPE.as_str())
}

fn default_package_data_with_type(package_type: &str) -> PackageData {
    PackageData {
        package_type: package_type.parse::<PackageType>().ok(),
        primary_language: Some("dart".to_string()),
        ..Default::default()
    }
}

fn extract_authors(yaml_content: &Value) -> Vec<crate::models::Party> {
    use crate::models::Party;
    let mut parties = Vec::new();

    if let Some(author) = extract_string_field(yaml_content, FIELD_AUTHOR).map(truncate_field) {
        parties.push(Party {
            r#type: None,
            role: Some("author".to_string()),
            name: Some(author),
            email: None,
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }

    if let Some(authors_value) = yaml_content.get(FIELD_AUTHORS)
        && let Some(authors_array) = authors_value.as_sequence()
    {
        for author_value in authors_array.iter().take(MAX_ITERATION_COUNT) {
            if let Some(author_str) = author_value.as_str() {
                parties.push(Party {
                    r#type: None,
                    role: Some("author".to_string()),
                    name: Some(truncate_field(author_str.to_string())),
                    email: None,
                    url: None,
                    organization: None,
                    organization_url: None,
                    timezone: None,
                });
            }
        }
    }

    parties
}

fn build_extra_data(
    yaml_content: &Value,
) -> Option<std::collections::HashMap<String, serde_json::Value>> {
    use std::collections::HashMap;
    let mut extra_data = HashMap::new();

    if let Some(issue_tracker) = extract_string_field(yaml_content, FIELD_ISSUE_TRACKER) {
        extra_data.insert(
            FIELD_ISSUE_TRACKER.to_string(),
            serde_json::Value::String(issue_tracker),
        );
    }

    if let Some(documentation) = extract_string_field(yaml_content, FIELD_DOCUMENTATION) {
        extra_data.insert(
            FIELD_DOCUMENTATION.to_string(),
            serde_json::Value::String(documentation),
        );
    }

    if let Some(executables) = yaml_content.get(FIELD_EXECUTABLES) {
        // Convert yaml_serde::Value to serde_json::Value
        if let Ok(json_value) = serde_json::to_value(executables) {
            extra_data.insert(FIELD_EXECUTABLES.to_string(), json_value);
        }
    }

    if let Some(publish_to) = extract_string_field(yaml_content, FIELD_PUBLISH_TO) {
        extra_data.insert(
            FIELD_PUBLISH_TO.to_string(),
            serde_json::Value::String(publish_to),
        );
    }

    for field in [
        FIELD_PLATFORMS,
        FIELD_FUNDING,
        FIELD_FALSE_SECRETS,
        FIELD_SCREENSHOTS,
        FIELD_TOPICS,
        FIELD_IGNORED_ADVISORIES,
    ] {
        if let Some(value) = yaml_content.get(field)
            && let Ok(json_value) = serde_json::to_value(value)
        {
            extra_data.insert(field.to_string(), json_value);
        }
    }

    if extra_data.is_empty() {
        None
    } else {
        Some(extra_data)
    }
}

fn extract_string_list_field(yaml_content: &Value, field: &str) -> Vec<String> {
    yaml_content
        .get(field)
        .and_then(Value::as_sequence)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| truncate_field(value.to_string()))
        .collect()
}

fn extract_manifest_dependency_extra_data(
    requirement_value: &Value,
) -> Option<HashMap<String, serde_json::Value>> {
    requirement_value
        .as_mapping()
        .and_then(|map| serde_json::to_value(map).ok())
        .and_then(|json| json.as_object().cloned())
        .map(|map| map.into_iter().collect())
}

fn extract_lock_descriptor_extra_data(
    details: &Mapping,
) -> Option<HashMap<String, serde_json::Value>> {
    let mut extra = HashMap::new();

    if let Some(source) = mapping_get(details, "source").and_then(Value::as_str) {
        extra.insert(
            "source".to_string(),
            serde_json::Value::String(source.to_string()),
        );
    }

    if let Some(description) = mapping_get(details, FIELD_DESCRIPTION)
        && let Ok(json_value) = serde_json::to_value(description)
    {
        extra.insert("description".to_string(), json_value);
    }

    if let Some(kind) = mapping_get(details, FIELD_DEPENDENCY).and_then(Value::as_str) {
        extra.insert(
            FIELD_DEPENDENCY.to_string(),
            serde_json::Value::String(kind.to_string()),
        );
    }

    (!extra.is_empty()).then_some(extra)
}

fn reachable_lock_packages(packages: &Mapping, roots: &[&str]) -> HashSet<String> {
    let mut reachable = HashSet::new();
    let mut queue = VecDeque::new();
    let mut iterations = 0usize;

    for (name_value, details_value) in packages.iter().take(MAX_ITERATION_COUNT) {
        let Some(name) = name_value.as_str() else {
            continue;
        };
        let Some(details) = details_value.as_mapping() else {
            continue;
        };
        let kind = mapping_get(details, FIELD_DEPENDENCY).and_then(Value::as_str);
        if roots.contains(&kind.unwrap_or_default()) {
            queue.push_back(truncate_field(name.to_string()));
        }
    }

    while let Some(current) = queue.pop_front() {
        if iterations >= MAX_ITERATION_COUNT {
            warn!("Iteration count exceeded in reachable_lock_packages BFS");
            break;
        }
        iterations += 1;

        if !reachable.insert(current.clone()) {
            continue;
        }

        let Some(details_value) = packages.get(Value::String(current.clone())) else {
            continue;
        };
        let Some(details) = details_value.as_mapping() else {
            continue;
        };
        let Some(dep_map) = mapping_get(details, FIELD_DEPENDENCIES).and_then(Value::as_mapping)
        else {
            continue;
        };

        for dep_name in dep_map
            .keys()
            .filter_map(Value::as_str)
            .take(MAX_ITERATION_COUNT)
        {
            queue.push_back(truncate_field(dep_name.to_string()));
        }
    }

    reachable
}

fn classify_lock_dependency(
    name: &str,
    dependency_kind: Option<&str>,
    runtime_reachable: &HashSet<String>,
    dev_only_reachable: &HashSet<String>,
) -> (bool, bool, bool) {
    match dependency_kind {
        Some("direct main") | Some("direct overridden") => (true, false, true),
        Some("direct dev") => (false, true, true),
        Some("transitive") => {
            if runtime_reachable.contains(name) {
                (true, false, false)
            } else if dev_only_reachable.contains(name) {
                (false, true, false)
            } else {
                (true, false, false)
            }
        }
        _ => (true, false, true),
    }
}

crate::register_parser!(
    "Dart pubspec.yaml manifest",
    &["**/pubspec.yaml", "**/pubspec.lock"],
    "pub",
    "Dart",
    Some("https://dart.dev/tools/pub/pubspec"),
);

#[cfg(test)]
mod is_match_tests {
    use super::{PubspecLockParser, PubspecYamlParser};
    use crate::parsers::PackageParser;
    use std::path::PathBuf;

    #[test]
    fn test_pubspec_yaml_parser_matches_suffixed_filenames() {
        assert!(PubspecYamlParser::is_match(&PathBuf::from("pubspec.yaml")));
        assert!(PubspecYamlParser::is_match(&PathBuf::from(
            "simple-pubspec.yaml"
        )));
        assert!(PubspecYamlParser::is_match(&PathBuf::from(
            "simple.pubspec.yaml"
        )));
        assert!(!PubspecYamlParser::is_match(&PathBuf::from("pubspec.yml")));
    }

    #[test]
    fn test_pubspec_lock_parser_matches_suffixed_filenames() {
        assert!(PubspecLockParser::is_match(&PathBuf::from("pubspec.lock")));
        assert!(PubspecLockParser::is_match(&PathBuf::from(
            "dart-pubspec.lock"
        )));
        assert!(PubspecLockParser::is_match(&PathBuf::from(
            "dart.pubspec.lock"
        )));
        assert!(!PubspecLockParser::is_match(&PathBuf::from(
            "pubspec.lock.bak"
        )));
    }
}
