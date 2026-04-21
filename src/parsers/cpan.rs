// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Parser for CPAN Perl package manifests.
//!
//! Extracts package metadata, dependencies, and author information from
//! CPAN distribution files used by Perl modules.
//!
//! # Supported Formats
//! - META.json (CPAN::Meta::Spec v2.0+)
//! - META.yml (CPAN::Meta::Spec v1.4)
//! - MANIFEST (file list)
//!
//! # Key Features
//! - Full metadata extraction from META.json and META.yml (beyond Python stub handlers)
//! - Dependency extraction for all CPAN dependency scopes (runtime, build, test, configure)
//! - Author party information extraction
//! - Repository URL extraction
//! - File references from MANIFEST
//!
//! # Implementation Notes
//! - Uses serde_json for JSON parsing
//! - Uses yaml_serde for YAML parsing
//! - Python reference has stub-only handlers with no parse() method
//! - This is a BEYOND PARITY implementation - we extract complete metadata

use std::path::Path;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use serde_json::Value as JsonValue;
use yaml_serde::Value as YamlValue;

use crate::models::{DatasourceId, Dependency, FileReference, PackageData, PackageType, Party};
use crate::parsers::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};

use super::PackageParser;
use super::license_normalization::{
    DeclaredLicenseMatchMetadata, NormalizedDeclaredLicense, build_declared_license_data,
    combine_normalized_licenses, empty_declared_license_data, normalize_declared_license_key,
    normalize_spdx_expression,
};

const FIELD_NAME: &str = "name";
const FIELD_VERSION: &str = "version";
const FIELD_ABSTRACT: &str = "abstract";
const FIELD_DESCRIPTION: &str = "description";
const FIELD_LICENSE: &str = "license";
const FIELD_AUTHOR: &str = "author";
const FIELD_RESOURCES: &str = "resources";
const FIELD_PREREQS: &str = "prereqs";
const FIELD_REQUIRES: &str = "requires";
const FIELD_BUILD_REQUIRES: &str = "build_requires";
const FIELD_TEST_REQUIRES: &str = "test_requires";
const FIELD_CONFIGURE_REQUIRES: &str = "configure_requires";

/// CPAN META.json parser for CPAN::Meta::Spec v2.0+ metadata.
///
/// Extracts complete metadata from META.json files including dependencies
/// from all scopes (runtime, build, test, configure).
pub struct CpanMetaJsonParser;

impl PackageParser for CpanMetaJsonParser {
    const PACKAGE_TYPE: PackageType = PackageType::Cpan;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "META.json")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let json = match read_and_parse_json(path) {
            Ok(json) => json,
            Err(e) => {
                warn!("Failed to parse META.json at {:?}: {}", path, e);
                return vec![default_package_data(DatasourceId::CpanMetaJson)];
            }
        };

        let name = json
            .get(FIELD_NAME)
            .and_then(|v| v.as_str())
            .map(|s| truncate_field(s.to_string()));

        let version = extract_version_from_json(&json);

        let description = json
            .get(FIELD_ABSTRACT)
            .and_then(|v| v.as_str())
            .map(|s| truncate_field(s.to_string()));

        let extracted_license_statement = extract_license_from_json(&json);
        let (declared_license_expression, declared_license_expression_spdx, license_detections) =
            normalize_cpan_declared_license(
                json.get(FIELD_LICENSE),
                extracted_license_statement.as_deref(),
            );
        let declared_license_expression = declared_license_expression.map(truncate_field);
        let declared_license_expression_spdx = declared_license_expression_spdx.map(truncate_field);
        let parties = extract_parties_from_json(&json);
        let dependencies = extract_dependencies_from_json(&json);
        let (homepage_url, vcs_url, code_view_url, bug_tracking_url) =
            extract_resources_from_json(&json);

        vec![PackageData {
            package_type: Some(Self::PACKAGE_TYPE),
            name,
            version,
            description,
            declared_license_expression,
            declared_license_expression_spdx,
            license_detections,
            extracted_license_statement,
            parties,
            dependencies,
            homepage_url,
            vcs_url,
            code_view_url,
            bug_tracking_url,
            primary_language: Some("Perl".to_string()),
            datasource_id: Some(DatasourceId::CpanMetaJson),
            ..Default::default()
        }]
    }
}

/// CPAN META.yml parser for CPAN::Meta::Spec v1.4 metadata.
///
/// Extracts complete metadata from META.yml files with legacy dependency structure.
pub struct CpanMetaYmlParser;

impl PackageParser for CpanMetaYmlParser {
    const PACKAGE_TYPE: PackageType = PackageType::Cpan;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "META.yml")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let yaml = match read_and_parse_yaml(path) {
            Ok(yaml) => yaml,
            Err(e) => {
                warn!("Failed to parse META.yml at {:?}: {}", path, e);
                return vec![default_package_data(DatasourceId::CpanMetaYml)];
            }
        };

        let name = yaml
            .get(FIELD_NAME)
            .and_then(|v| v.as_str())
            .map(|s| truncate_field(s.to_string()));

        let version = extract_version_from_yaml(&yaml);

        let description = yaml
            .get(FIELD_ABSTRACT)
            .or_else(|| yaml.get(FIELD_DESCRIPTION))
            .and_then(|v| v.as_str())
            .map(|s| truncate_field(s.to_string()));

        let extracted_license_statement = extract_license_from_yaml(&yaml);
        let (declared_license_expression, declared_license_expression_spdx, license_detections) =
            normalize_cpan_declared_license(
                yaml.get(YamlValue::String(FIELD_LICENSE.to_string())),
                extracted_license_statement.as_deref(),
            );
        let declared_license_expression = declared_license_expression.map(truncate_field);
        let declared_license_expression_spdx = declared_license_expression_spdx.map(truncate_field);
        let parties = extract_parties_from_yaml(&yaml);
        let dependencies = extract_dependencies_from_yaml(&yaml);
        let (homepage_url, vcs_url, bug_tracking_url) = extract_resources_from_yaml(&yaml);

        vec![PackageData {
            package_type: Some(Self::PACKAGE_TYPE),
            name,
            version,
            description,
            declared_license_expression,
            declared_license_expression_spdx,
            license_detections,
            extracted_license_statement,
            parties,
            dependencies,
            homepage_url,
            vcs_url,
            bug_tracking_url,
            primary_language: Some("Perl".to_string()),
            datasource_id: Some(DatasourceId::CpanMetaYml),
            ..Default::default()
        }]
    }
}

/// CPAN MANIFEST parser for module file lists.
///
/// Extracts file references from MANIFEST files (simple line-by-line format).
pub struct CpanManifestParser;

impl PackageParser for CpanManifestParser {
    const PACKAGE_TYPE: PackageType = PackageType::Cpan;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "MANIFEST")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read MANIFEST at {:?}: {}", path, e);
                return vec![default_package_data(DatasourceId::CpanManifest)];
            }
        };

        let file_references = content
            .lines()
            .take(MAX_ITERATION_COUNT)
            .filter(|line| !line.trim().is_empty())
            .filter(|line| !line.trim().starts_with('#'))
            .map(|line| {
                let path = line.split_whitespace().next().unwrap_or(line);
                FileReference {
                    path: truncate_field(path.to_string()),
                    size: None,
                    sha1: None,
                    md5: None,
                    sha256: None,
                    sha512: None,
                    extra_data: None,
                }
            })
            .collect();

        vec![PackageData {
            package_type: Some(Self::PACKAGE_TYPE),
            file_references,
            primary_language: Some("Perl".to_string()),
            datasource_id: Some(DatasourceId::CpanManifest),
            ..Default::default()
        }]
    }
}

fn default_package_data(datasource_id: DatasourceId) -> PackageData {
    PackageData {
        package_type: Some(CpanMetaJsonParser::PACKAGE_TYPE),
        primary_language: Some("Perl".to_string()),
        datasource_id: Some(datasource_id),
        ..Default::default()
    }
}

fn read_and_parse_json(path: &Path) -> Result<serde_json::Map<String, JsonValue>, String> {
    let content =
        read_file_to_string(path, None).map_err(|e| format!("Failed to read file: {}", e))?;
    let json: JsonValue =
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse JSON: {}", e))?;
    json.as_object()
        .cloned()
        .ok_or_else(|| "Root JSON is not an object".to_string())
}

fn read_and_parse_yaml(path: &Path) -> Result<yaml_serde::Mapping, String> {
    let content =
        read_file_to_string(path, None).map_err(|e| format!("Failed to read file: {}", e))?;
    let yaml: YamlValue =
        yaml_serde::from_str(&content).map_err(|e| format!("Failed to parse YAML: {}", e))?;
    yaml.as_mapping()
        .cloned()
        .ok_or_else(|| "Root YAML is not a mapping".to_string())
}

fn extract_version_from_json(json: &serde_json::Map<String, JsonValue>) -> Option<String> {
    json.get(FIELD_VERSION).and_then(|v| match v {
        JsonValue::String(s) => Some(truncate_field(s.clone())),
        JsonValue::Number(n) => Some(truncate_field(n.to_string())),
        _ => None,
    })
}

fn extract_version_from_yaml(yaml: &yaml_serde::Mapping) -> Option<String> {
    yaml.get(YamlValue::String(FIELD_VERSION.to_string()))
        .and_then(|v| match v {
            YamlValue::String(s) => Some(truncate_field(s.clone())),
            YamlValue::Number(n) => Some(truncate_field(n.to_string())),
            _ => None,
        })
}

fn extract_license_from_json(json: &serde_json::Map<String, JsonValue>) -> Option<String> {
    json.get(FIELD_LICENSE).and_then(|v| match v {
        JsonValue::String(s) => Some(truncate_field(s.clone())),
        JsonValue::Array(arr) => {
            let licenses: Vec<String> = arr
                .iter()
                .take(MAX_ITERATION_COUNT)
                .filter_map(|item| item.as_str().map(|s| truncate_field(s.to_string())))
                .collect();
            if licenses.is_empty() {
                None
            } else {
                Some(truncate_field(licenses.join(" AND ")))
            }
        }
        _ => None,
    })
}

fn extract_license_from_yaml(yaml: &yaml_serde::Mapping) -> Option<String> {
    yaml.get(YamlValue::String(FIELD_LICENSE.to_string()))
        .and_then(|v| match v {
            YamlValue::String(s) => Some(truncate_field(s.clone())),
            YamlValue::Sequence(arr) => {
                let licenses: Vec<String> = arr
                    .iter()
                    .take(MAX_ITERATION_COUNT)
                    .filter_map(|item| item.as_str().map(|s| truncate_field(s.to_string())))
                    .collect();
                if licenses.is_empty() {
                    None
                } else {
                    Some(truncate_field(licenses.join(" AND ")))
                }
            }
            _ => None,
        })
}

fn normalize_cpan_declared_license(
    raw_license: Option<&impl LicenseValueAdapter>,
    extracted_license_statement: Option<&str>,
) -> (
    Option<String>,
    Option<String>,
    Vec<crate::models::LicenseDetection>,
) {
    let Some(raw_license) = raw_license else {
        return empty_declared_license_data();
    };
    let normalized = raw_license
        .license_values()
        .into_iter()
        .map(|value| normalize_cpan_license_value(&value))
        .collect::<Option<Vec<_>>>();

    if let Some(normalized) = normalized
        && let Some(combined) = combine_normalized_licenses(normalized, " AND ")
    {
        return build_declared_license_data(
            combined,
            DeclaredLicenseMatchMetadata::single_line(
                extracted_license_statement.unwrap_or_default(),
            ),
        );
    }

    empty_declared_license_data()
}

trait LicenseValueAdapter {
    fn license_values(&self) -> Vec<String>;
}

impl LicenseValueAdapter for JsonValue {
    fn license_values(&self) -> Vec<String> {
        match self {
            JsonValue::String(value) => vec![truncate_field(value.trim().to_string())],
            JsonValue::Array(values) => values
                .iter()
                .take(MAX_ITERATION_COUNT)
                .filter_map(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|s| truncate_field(s.to_string()))
                .collect(),
            _ => Vec::new(),
        }
    }
}

impl LicenseValueAdapter for YamlValue {
    fn license_values(&self) -> Vec<String> {
        match self {
            YamlValue::String(value) => vec![truncate_field(value.trim().to_string())],
            YamlValue::Sequence(values) => values
                .iter()
                .take(MAX_ITERATION_COUNT)
                .filter_map(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|s| truncate_field(s.to_string()))
                .collect(),
            _ => Vec::new(),
        }
    }
}

fn normalize_cpan_license_value(value: &str) -> Option<NormalizedDeclaredLicense> {
    match value.trim() {
        "perl_5" | "Perl_5" => Some(NormalizedDeclaredLicense::new(
            "gpl-1.0-plus OR artistic-perl-1.0",
            "GPL-1.0-or-later OR Artistic-1.0-Perl",
        )),
        "artistic_2" => Some(NormalizedDeclaredLicense::new(
            "artistic-2.0",
            "Artistic-2.0",
        )),
        "apache_2_0" => Some(NormalizedDeclaredLicense::new("apache-2.0", "Apache-2.0")),
        other => normalize_spdx_expression(other).or_else(|| normalize_declared_license_key(other)),
    }
}

fn extract_parties_from_json(json: &serde_json::Map<String, JsonValue>) -> Vec<Party> {
    json.get(FIELD_AUTHOR)
        .and_then(|v| v.as_array())
        .map_or_else(Vec::new, |authors| {
            authors
                .iter()
                .take(MAX_ITERATION_COUNT)
                .filter_map(|author| {
                    author.as_str().map(|s| {
                        let (name, email) = parse_author_string(s);
                        Party {
                            r#type: Some("person".to_string()),
                            role: Some("author".to_string()),
                            name,
                            email,
                            url: None,
                            organization: None,
                            organization_url: None,
                            timezone: None,
                        }
                    })
                })
                .collect()
        })
}

fn extract_parties_from_yaml(yaml: &yaml_serde::Mapping) -> Vec<Party> {
    yaml.get(YamlValue::String(FIELD_AUTHOR.to_string()))
        .and_then(|v| v.as_sequence())
        .map_or_else(Vec::new, |authors| {
            authors
                .iter()
                .take(MAX_ITERATION_COUNT)
                .filter_map(|author| {
                    author.as_str().map(|s| {
                        let (name, email) = parse_author_string(s);
                        Party {
                            r#type: Some("person".to_string()),
                            role: Some("author".to_string()),
                            name,
                            email,
                            url: None,
                            organization: None,
                            organization_url: None,
                            timezone: None,
                        }
                    })
                })
                .collect()
        })
}

fn parse_author_string(author_str: &str) -> (Option<String>, Option<String>) {
    if let Some(email_start) = author_str.find('<')
        && let Some(email_end) = author_str.find('>')
        && email_start < email_end
    {
        let name = author_str[..email_start].trim();
        let email = author_str[email_start + 1..email_end].trim();
        return (
            if name.is_empty() {
                None
            } else {
                Some(truncate_field(name.to_string()))
            },
            if email.is_empty() {
                None
            } else {
                Some(truncate_field(email.to_string()))
            },
        );
    }
    let trimmed = author_str.trim();
    (
        if trimmed.is_empty() {
            None
        } else {
            Some(truncate_field(trimmed.to_string()))
        },
        None,
    )
}

fn extract_resources_from_json(
    json: &serde_json::Map<String, JsonValue>,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    let resources = match json.get(FIELD_RESOURCES).and_then(|v| v.as_object()) {
        Some(r) => r,
        None => return (None, None, None, None),
    };

    let homepage_url = resources
        .get("homepage")
        .and_then(|v| v.as_str())
        .map(|s| truncate_field(s.to_string()));

    let vcs_url = resources.get("repository").and_then(|v| match v {
        JsonValue::String(s) => Some(truncate_field(s.clone())),
        JsonValue::Object(obj) => obj
            .get("url")
            .and_then(|u| u.as_str())
            .map(|s| truncate_field(s.to_string())),
        _ => None,
    });

    let code_view_url = resources
        .get("repository")
        .and_then(|v| v.as_object())
        .and_then(|obj| {
            obj.get("web")
                .and_then(|u| u.as_str())
                .map(|s| truncate_field(s.to_string()))
        });

    let bug_tracking_url = resources.get("bugtracker").and_then(|v| match v {
        JsonValue::String(s) => Some(truncate_field(s.clone())),
        JsonValue::Object(obj) => obj
            .get("web")
            .and_then(|u| u.as_str())
            .map(|s| truncate_field(s.to_string())),
        _ => None,
    });

    (homepage_url, vcs_url, code_view_url, bug_tracking_url)
}

fn extract_resources_from_yaml(
    yaml: &yaml_serde::Mapping,
) -> (Option<String>, Option<String>, Option<String>) {
    let resources = match yaml
        .get(YamlValue::String(FIELD_RESOURCES.to_string()))
        .and_then(|v| v.as_mapping())
    {
        Some(r) => r,
        None => return (None, None, None),
    };

    let homepage_url = resources
        .get(YamlValue::String("homepage".to_string()))
        .and_then(|v| v.as_str())
        .map(|s| truncate_field(s.to_string()));

    let vcs_url = resources
        .get(YamlValue::String("repository".to_string()))
        .and_then(|v| v.as_str())
        .map(|s| truncate_field(s.to_string()));

    let bug_tracking_url = resources
        .get(YamlValue::String("bugtracker".to_string()))
        .and_then(|v| v.as_str())
        .map(|s| truncate_field(s.to_string()));

    (homepage_url, vcs_url, bug_tracking_url)
}

fn extract_dependencies_from_json(json: &serde_json::Map<String, JsonValue>) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    let prereqs = match json.get(FIELD_PREREQS).and_then(|v| v.as_object()) {
        Some(p) => p,
        None => return dependencies,
    };

    // Extract runtime dependencies
    if let Some(runtime) = prereqs.get("runtime").and_then(|v| v.as_object())
        && let Some(requires) = runtime.get("requires").and_then(|v| v.as_object())
    {
        dependencies.extend(extract_dependency_group(requires, "runtime", true, false));
    }

    // Extract build dependencies
    if let Some(build) = prereqs.get("build").and_then(|v| v.as_object())
        && let Some(requires) = build.get("requires").and_then(|v| v.as_object())
    {
        dependencies.extend(extract_dependency_group(requires, "build", false, false));
    }

    // Extract test dependencies
    if let Some(test) = prereqs.get("test").and_then(|v| v.as_object())
        && let Some(requires) = test.get("requires").and_then(|v| v.as_object())
    {
        dependencies.extend(extract_dependency_group(requires, "test", false, false));
    }

    // Extract configure dependencies
    if let Some(configure) = prereqs.get("configure").and_then(|v| v.as_object())
        && let Some(requires) = configure.get("requires").and_then(|v| v.as_object())
    {
        dependencies.extend(extract_dependency_group(
            requires,
            "configure",
            false,
            false,
        ));
    }

    dependencies
}

fn extract_dependencies_from_yaml(yaml: &yaml_serde::Mapping) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    // META.yml v1.4 has flat dependency structure
    if let Some(requires) = yaml
        .get(YamlValue::String(FIELD_REQUIRES.to_string()))
        .and_then(|v| v.as_mapping())
    {
        dependencies.extend(extract_yaml_dependency_group(
            requires, "runtime", true, false,
        ));
    }

    if let Some(build_requires) = yaml
        .get(YamlValue::String(FIELD_BUILD_REQUIRES.to_string()))
        .and_then(|v| v.as_mapping())
    {
        dependencies.extend(extract_yaml_dependency_group(
            build_requires,
            "build",
            false,
            false,
        ));
    }

    if let Some(test_requires) = yaml
        .get(YamlValue::String(FIELD_TEST_REQUIRES.to_string()))
        .and_then(|v| v.as_mapping())
    {
        dependencies.extend(extract_yaml_dependency_group(
            test_requires,
            "test",
            false,
            false,
        ));
    }

    if let Some(configure_requires) = yaml
        .get(YamlValue::String(FIELD_CONFIGURE_REQUIRES.to_string()))
        .and_then(|v| v.as_mapping())
    {
        dependencies.extend(extract_yaml_dependency_group(
            configure_requires,
            "configure",
            false,
            false,
        ));
    }

    dependencies
}

fn extract_dependency_group(
    deps: &serde_json::Map<String, JsonValue>,
    scope: &str,
    is_runtime: bool,
    is_optional: bool,
) -> Vec<Dependency> {
    deps.iter()
        .take(MAX_ITERATION_COUNT)
        .filter_map(|(name, version)| {
            if name == "perl" {
                return None;
            }

            let purl = PackageUrl::new("cpan", name).ok().map(|p| p.to_string());
            let purl = purl.map(truncate_field);

            let extracted_requirement = match version {
                JsonValue::String(s) => Some(truncate_field(s.clone())),
                JsonValue::Number(n) => Some(truncate_field(n.to_string())),
                _ => None,
            };

            Some(Dependency {
                purl,
                extracted_requirement,
                scope: Some(truncate_field(scope.to_string())),
                is_runtime: Some(is_runtime),
                is_optional: Some(is_optional),
                is_pinned: None,
                is_direct: Some(true),
                resolved_package: None,
                extra_data: None,
            })
        })
        .collect()
}

fn extract_yaml_dependency_group(
    deps: &yaml_serde::Mapping,
    scope: &str,
    is_runtime: bool,
    is_optional: bool,
) -> Vec<Dependency> {
    deps.iter()
        .take(MAX_ITERATION_COUNT)
        .filter_map(|(key, value)| {
            let name = key.as_str()?;

            if name == "perl" {
                return None;
            }

            let purl = PackageUrl::new("cpan", name).ok().map(|p| p.to_string());
            let purl = purl.map(truncate_field);

            let extracted_requirement = match value {
                YamlValue::String(s) => Some(truncate_field(s.clone())),
                YamlValue::Number(n) => Some(truncate_field(n.to_string())),
                _ => None,
            };

            Some(Dependency {
                purl,
                extracted_requirement,
                scope: Some(truncate_field(scope.to_string())),
                is_runtime: Some(is_runtime),
                is_optional: Some(is_optional),
                is_pinned: None,
                is_direct: Some(true),
                resolved_package: None,
                extra_data: None,
            })
        })
        .collect()
}

crate::register_parser!(
    "CPAN Perl META.json",
    &["**/META.json"],
    "cpan",
    "Perl",
    Some("https://metacpan.org/pod/CPAN::Meta::Spec"),
);

crate::register_parser!(
    "CPAN Perl META.yml",
    &["**/META.yml"],
    "cpan",
    "Perl",
    Some("https://metacpan.org/pod/CPAN::Meta::Spec"),
);

crate::register_parser!(
    "CPAN Perl MANIFEST",
    &["**/MANIFEST"],
    "cpan",
    "Perl",
    Some("https://metacpan.org/pod/Module::Manifest"),
);
