//! Parser for Bower package manifests (bower.json).
//!
//! Extracts package metadata, dependencies, and license information from
//! bower.json files used by the legacy Bower JavaScript package manager.
//!
//! # Supported Formats
//! - bower.json (manifest)
//! - .bower.json (alternative manifest)
//!
//! # Key Features
//! - Dependency extraction (dependencies, devDependencies)
//! - License extraction (string or array format)
//! - Author parsing (string or object format)
//! - VCS repository URL extraction
//! - Private package detection
//!
//! # Implementation Notes
//! - Uses serde_json for JSON parsing
//! - Graceful error handling: logs warnings and returns default on parse failure
//! - Authors field can be string, object, or array of either

use crate::models::{DatasourceId, Dependency, PackageData, PackageType, Party};
use crate::parser_warn as warn;
use crate::parsers::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};
use packageurl::PackageUrl;
use serde_json::Value;
use std::path::Path;

use super::PackageParser;
use super::license_normalization::{
    DeclaredLicenseMatchMetadata, build_declared_license_data, combine_normalized_licenses,
    empty_declared_license_data, normalize_declared_license_key, normalize_spdx_declared_license,
};

const FIELD_NAME: &str = "name";
const FIELD_VERSION: &str = "version";
const FIELD_DESCRIPTION: &str = "description";
const FIELD_LICENSE: &str = "license";
const FIELD_KEYWORDS: &str = "keywords";
const FIELD_AUTHORS: &str = "authors";
const FIELD_HOMEPAGE: &str = "homepage";
const FIELD_REPOSITORY: &str = "repository";
const FIELD_DEPENDENCIES: &str = "dependencies";
const FIELD_DEV_DEPENDENCIES: &str = "devDependencies";
const FIELD_PRIVATE: &str = "private";

/// Bower package parser for bower.json manifests.
///
/// Supports legacy Bower JavaScript package manager format with all
/// standard fields including dependencies, devDependencies, authors, and licenses.
pub struct BowerJsonParser;

impl PackageParser for BowerJsonParser {
    const PACKAGE_TYPE: PackageType = PackageType::Bower;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let json = match read_and_parse_json(path) {
            Ok(json) => json,
            Err(e) => {
                warn!("Failed to read or parse bower.json at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        let name = json
            .get(FIELD_NAME)
            .and_then(|v| v.as_str())
            .map(|s| truncate_field(s.to_string()));

        // If name is missing, the package is considered private
        let is_private = if name.is_none() {
            true
        } else {
            json.get(FIELD_PRIVATE)
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        };

        let version = json
            .get(FIELD_VERSION)
            .and_then(|v| v.as_str())
            .map(|s| truncate_field(s.to_string()));

        let description = json
            .get(FIELD_DESCRIPTION)
            .and_then(|v| v.as_str())
            .map(|s| truncate_field(s.to_string()));

        let extracted_license_statement = extract_license_statement(&json);
        let (declared_license_expression, declared_license_expression_spdx, license_detections) =
            normalize_bower_declared_license(&json, extracted_license_statement.as_deref());
        let declared_license_expression = declared_license_expression.map(truncate_field);
        let declared_license_expression_spdx = declared_license_expression_spdx.map(truncate_field);
        let keywords = extract_keywords(&json);
        let parties = extract_parties(&json);
        let homepage_url = json
            .get(FIELD_HOMEPAGE)
            .and_then(|v| v.as_str())
            .map(|s| truncate_field(s.to_string()));

        let vcs_url = extract_vcs_url(&json);
        let dependencies = extract_dependencies(&json, FIELD_DEPENDENCIES, "dependencies", true);
        let dev_dependencies =
            extract_dependencies(&json, FIELD_DEV_DEPENDENCIES, "devDependencies", false);

        vec![PackageData {
            package_type: Some(Self::PACKAGE_TYPE),
            namespace: None,
            name,
            version,
            qualifiers: None,
            subpath: None,
            primary_language: Some("JavaScript".to_string()),
            description,
            release_date: None,
            parties,
            keywords,
            homepage_url,
            download_url: None,
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            bug_tracking_url: None,
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
            extracted_license_statement,
            notice_text: None,
            source_packages: Vec::new(),
            file_references: Vec::new(),
            is_private,
            is_virtual: false,
            extra_data: None,
            dependencies: [dependencies, dev_dependencies].concat(),
            repository_homepage_url: None,
            repository_download_url: None,
            api_data_url: None,
            datasource_id: Some(DatasourceId::BowerJson),
            purl: None,
        }]
    }

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .is_some_and(|name| name == "bower.json" || name == ".bower.json")
    }
}

/// Reads and parses a JSON file
fn read_and_parse_json(path: &Path) -> Result<Value, String> {
    let content =
        read_file_to_string(path, None).map_err(|e| format!("Failed to read file: {}", e))?;
    serde_json::from_str(&content).map_err(|e| format!("Failed to parse JSON: {}", e))
}

/// Extracts license statement from the license field.
/// Can be a string or an array of strings.
fn extract_license_statement(json: &Value) -> Option<String> {
    json.get(FIELD_LICENSE)
        .and_then(|license_value| match license_value {
            Value::String(s) => {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(truncate_field(trimmed.to_string()))
                }
            }
            Value::Array(licenses) => {
                let license_strings: Vec<String> = licenses
                    .iter()
                    .take(MAX_ITERATION_COUNT)
                    .filter_map(|v| v.as_str())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect();

                if license_strings.is_empty() {
                    None
                } else {
                    Some(truncate_field(license_strings.join(" AND ")))
                }
            }
            _ => None,
        })
}

fn normalize_bower_declared_license(
    json: &Value,
    extracted_license_statement: Option<&str>,
) -> (
    Option<String>,
    Option<String>,
    Vec<crate::models::LicenseDetection>,
) {
    match json.get(FIELD_LICENSE) {
        Some(Value::Array(licenses)) => {
            let normalized = licenses
                .iter()
                .take(MAX_ITERATION_COUNT)
                .filter_map(|value| value.as_str().map(str::trim))
                .filter(|value| !value.is_empty())
                .map(normalize_declared_license_key)
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
        _ => normalize_spdx_declared_license(extracted_license_statement),
    }
}

/// Extracts keywords from the keywords field.
fn extract_keywords(json: &Value) -> Vec<String> {
    json.get(FIELD_KEYWORDS)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .take(MAX_ITERATION_COUNT)
                .filter_map(|v| v.as_str())
                .map(|s| truncate_field(s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Extracts parties (authors) from the authors field.
/// Authors can be strings or objects with name, email, and homepage fields.
fn extract_parties(json: &Value) -> Vec<Party> {
    let mut parties = Vec::new();

    if let Some(authors) = json.get(FIELD_AUTHORS).and_then(|v| v.as_array()) {
        for author in authors.iter().take(MAX_ITERATION_COUNT) {
            if let Some(party) = extract_party_from_author(author) {
                parties.push(party);
            }
        }
    }

    parties
}

/// Extracts a single party from an author value (string or object).
fn extract_party_from_author(author: &Value) -> Option<Party> {
    match author {
        Value::String(s) => {
            let (name, email) = parse_author_string(s);
            Some(Party {
                r#type: Some("person".to_string()),
                role: Some("author".to_string()),
                name: name.map(truncate_field),
                email: email.map(truncate_field),
                url: None,
                organization: None,
                organization_url: None,
                timezone: None,
            })
        }
        Value::Object(obj) => {
            let name = obj
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| truncate_field(s.to_string()));
            let email = obj
                .get("email")
                .and_then(|v| v.as_str())
                .map(|s| truncate_field(s.to_string()));
            let url = obj
                .get("homepage")
                .and_then(|v| v.as_str())
                .map(|s| truncate_field(s.to_string()));

            Some(Party {
                r#type: Some("person".to_string()),
                role: Some("author".to_string()),
                name,
                email,
                url,
                organization: None,
                organization_url: None,
                timezone: None,
            })
        }
        _ => Some(Party {
            r#type: Some("person".to_string()),
            role: Some("author".to_string()),
            name: Some(truncate_field(format!("{:?}", author))),
            email: None,
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        }),
    }
}

/// Parses author string in "Name <email>" format.
/// Returns (name, email) tuple with both as Option<String>.
fn parse_author_string(author_str: &str) -> (Option<String>, Option<String>) {
    if let Some(email_start) = author_str.find('<')
        && let Some(email_end) = author_str.find('>')
        && email_start < email_end
    {
        let name = author_str[..email_start].trim();
        let email = author_str[email_start + 1..email_end].trim();

        let name = if name.is_empty() {
            None
        } else {
            Some(truncate_field(name.to_string()))
        };
        let email = if email.is_empty() {
            None
        } else {
            Some(truncate_field(email.to_string()))
        };

        return (name, email);
    }

    let trimmed = author_str.trim();
    if trimmed.is_empty() {
        (None, None)
    } else {
        (Some(truncate_field(trimmed.to_string())), None)
    }
}

/// Extracts VCS URL from the repository field.
/// Repository can be an object with type and url fields.
fn extract_vcs_url(json: &Value) -> Option<String> {
    json.get(FIELD_REPOSITORY).and_then(|repo| {
        if let Some(repo_obj) = repo.as_object() {
            let repo_type = repo_obj.get("type").and_then(|v| v.as_str());
            let repo_url = repo_obj.get("url").and_then(|v| v.as_str());

            match (repo_type, repo_url) {
                (Some(t), Some(u)) if !t.is_empty() && !u.is_empty() => {
                    Some(truncate_field(format!("{}+{}", t, u)))
                }
                _ => None,
            }
        } else {
            None
        }
    })
}

/// Extracts dependencies from a dependency field.
fn extract_dependencies(
    json: &Value,
    field: &str,
    scope: &str,
    is_runtime: bool,
) -> Vec<Dependency> {
    json.get(field)
        .and_then(|deps| deps.as_object())
        .map_or_else(Vec::new, |deps| {
            deps.iter()
                .take(MAX_ITERATION_COUNT)
                .filter_map(|(name, requirement)| {
                    let requirement_str = requirement.as_str()?;
                    let package_url =
                        PackageUrl::new(BowerJsonParser::PACKAGE_TYPE.as_str(), name).ok()?;

                    Some(Dependency {
                        purl: Some(truncate_field(package_url.to_string())),
                        extracted_requirement: Some(truncate_field(requirement_str.to_string())),
                        scope: Some(scope.to_string()),
                        is_runtime: Some(is_runtime),
                        is_optional: Some(!is_runtime),
                        is_pinned: None,
                        is_direct: Some(true),
                        resolved_package: None,
                        extra_data: None,
                    })
                })
                .collect()
        })
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(BowerJsonParser::PACKAGE_TYPE),
        primary_language: Some("JavaScript".to_string()),
        datasource_id: Some(DatasourceId::BowerJson),
        ..Default::default()
    }
}

crate::register_parser!(
    "Bower package manifest",
    &["**/bower.json", "**/.bower.json"],
    "bower",
    "JavaScript",
    Some("https://bower.io"),
);
