//! Parser for CocoaPods .podspec.json manifests.
//!
//! Extracts package metadata and dependencies from .podspec.json files used by
//! CocoaPods for iOS/macOS package management.
//!
//! # Supported Formats
//! - *.podspec.json (CocoaPods manifest JSON format)
//!
//! # Key Features
//! - Dependency extraction from dependencies dictionary
//! - License handling (both string and dict formats with "type" and "text" keys)
//! - VCS and download URL extraction from source field
//! - Author/party information parsing
//! - Full JSON storage in extra_data
//!
//! # Implementation Notes
//! - Uses serde_json for JSON parsing
//! - Handles license as both string and dict (joins dict values)
//! - Extracts dependencies from dict (key=name, value=version requirement)
//! - All dependencies have scope="dependencies" and is_runtime=true
//! - Source dict stored in extra_data["source"]

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use serde_json::Value;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType, Party};

use super::PackageParser;
use super::license_normalization::normalize_spdx_declared_license;

const FIELD_NAME: &str = "name";
const FIELD_VERSION: &str = "version";
const FIELD_SUMMARY: &str = "summary";
const FIELD_DESCRIPTION: &str = "description";
const FIELD_HOMEPAGE: &str = "homepage";
const FIELD_LICENSE: &str = "license";
const FIELD_SOURCE: &str = "source";
const FIELD_AUTHORS: &str = "authors";
const FIELD_DEPENDENCIES: &str = "dependencies";

const PRIMARY_LANGUAGE: &str = "Objective-C";

/// CocoaPods .podspec.json parser.
///
/// Parses .podspec.json manifest files from CocoaPods ecosystem.
pub struct PodspecJsonParser;

impl PackageParser for PodspecJsonParser {
    const PACKAGE_TYPE: PackageType = PackageType::Cocoapods;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let json_content = match read_json_file(path) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read .podspec.json at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        let name = json_content
            .get(FIELD_NAME)
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let version = json_content
            .get(FIELD_VERSION)
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let summary = json_content
            .get(FIELD_SUMMARY)
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let mut description = json_content
            .get(FIELD_DESCRIPTION)
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        // If summary exists and description doesn't start with summary, prepend it
        if let (Some(summary_text), Some(desc_text)) = (&summary, &description) {
            if !desc_text.starts_with(summary_text) {
                description = Some(format!("{}. {}", summary_text, desc_text));
            }
        } else if summary.is_some() && description.is_none() {
            description = summary.clone();
        }

        let homepage_url = json_content
            .get(FIELD_HOMEPAGE)
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let extracted_license_statement = extract_license_statement(&json_content);
        let (declared_license_expression, declared_license_expression_spdx, license_detections) =
            normalize_podspec_json_declared_license(
                &json_content,
                extracted_license_statement.as_deref(),
            );

        let (vcs_url, download_url) = extract_source_urls(&json_content);

        let parties = extract_parties(&json_content);

        let dependencies = extract_dependencies(&json_content);

        let mut extra_data = HashMap::new();

        // Store source dict in extra_data
        if let Some(source) = json_content.get(FIELD_SOURCE) {
            extra_data.insert("source".to_string(), source.clone());
        }

        // Store dependencies dict in extra_data if present
        if let Some(deps) = json_content.get(FIELD_DEPENDENCIES)
            && let Some(obj) = deps.as_object()
            && !obj.is_empty()
        {
            extra_data.insert(FIELD_DEPENDENCIES.to_string(), deps.clone());
        }

        if let Some(license_file) = json_content
            .get(FIELD_LICENSE)
            .and_then(|license| license.as_object())
            .and_then(|license| license.get("file"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
        {
            extra_data.insert(
                "license_file".to_string(),
                Value::String(license_file.trim().to_string()),
            );
        }

        // Store full JSON in extra_data
        extra_data.insert("podspec.json".to_string(), json_content.clone());

        let extra_data = if extra_data.is_empty() {
            None
        } else {
            Some(extra_data)
        };

        // Generate URLs using CocoaPods patterns
        let repository_homepage_url = name
            .as_ref()
            .map(|n| format!("https://cocoapods.org/pods/{}", n));
        let repository_download_url =
            if let (Some(_name_str), Some(version_str)) = (&name, &version) {
                if let Some(homepage) = &homepage_url {
                    Some(format!("{}/archive/{}.zip", homepage, version_str))
                } else if let Some(vcs) = &vcs_url {
                    let repo_base = get_repo_base_url(vcs);
                    repo_base.map(|base| format!("{}/archive/refs/tags/{}.zip", base, version_str))
                } else {
                    None
                }
            } else {
                None
            };

        let code_view_url = if let (Some(vcs), Some(version_str)) = (&vcs_url, &version) {
            let repo_base = get_repo_base_url(vcs);
            repo_base.map(|base| format!("{}/tree/{}", base, version_str))
        } else {
            None
        };

        let bug_tracking_url = vcs_url.as_ref().and_then(|vcs| {
            let repo_base = get_repo_base_url(vcs);
            repo_base.map(|base| format!("{}/issues/", base))
        });

        let api_data_url = if let (Some(name_str), Some(version_str)) = (&name, &version) {
            get_hashed_path(name_str).map(|hashed| {
                format!(
                    "https://raw.githubusercontent.com/CocoaPods/Specs/blob/master/Specs/{}/{}/{}/{}.podspec.json",
                    hashed, name_str, version_str, name_str
                )
            })
        } else {
            None
        };

        let purl = if let Some(name_str) = &name {
            let mut purl = PackageUrl::new(Self::PACKAGE_TYPE.as_str(), name_str)
                .unwrap_or_else(|_| PackageUrl::new("generic", name_str).unwrap());
            if let Some(version_str) = &version {
                let _ = purl.with_version(version_str);
            }
            Some(purl.to_string())
        } else {
            None
        };

        vec![PackageData {
            package_type: Some(Self::PACKAGE_TYPE),
            namespace: None,
            name: name.clone(),
            version: version.clone(),
            qualifiers: None,
            subpath: None,
            primary_language: Some(PRIMARY_LANGUAGE.to_string()),
            description,
            release_date: None,
            parties,
            keywords: Vec::new(),
            homepage_url,
            download_url,
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            bug_tracking_url,
            code_view_url,
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
            is_private: false,
            is_virtual: false,
            extra_data,
            dependencies,
            repository_homepage_url,
            repository_download_url,
            api_data_url,
            datasource_id: Some(DatasourceId::CocoapodsPodspecJson),
            purl,
        }]
    }

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".podspec.json"))
    }
}

/// Reads and parses a JSON file.
fn read_json_file(path: &Path) -> Result<Value, String> {
    let mut file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(|e| format!("Failed to read file: {}", e))?;
    serde_json::from_str(&contents).map_err(|e| format!("Failed to parse JSON: {}", e))
}

/// Returns a default empty PackageData.
fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(PodspecJsonParser::PACKAGE_TYPE),
        primary_language: Some(PRIMARY_LANGUAGE.to_string()),
        datasource_id: Some(DatasourceId::CocoapodsPodspecJson),
        ..Default::default()
    }
}

/// Extracts license statement from JSON.
/// Handles both string and dict formats.
fn extract_license_statement(json: &Value) -> Option<String> {
    json.get(FIELD_LICENSE).and_then(|lic| {
        if let Some(lic_str) = lic.as_str() {
            Some(lic_str.trim().to_string())
        } else if let Some(lic_obj) = lic.as_object() {
            // If license is a dict, join all values with space
            let values: Vec<String> = lic_obj
                .values()
                .filter_map(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if values.is_empty() {
                None
            } else {
                Some(values.join(" "))
            }
        } else {
            None
        }
    })
}

fn normalize_podspec_json_declared_license(
    json: &Value,
    extracted_license_statement: Option<&str>,
) -> (
    Option<String>,
    Option<String>,
    Vec<crate::models::LicenseDetection>,
) {
    let normalized_candidate = json
        .get(FIELD_LICENSE)
        .and_then(|license| {
            license
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(canonicalize_cocoapods_license_type)
                .or_else(|| {
                    license
                        .as_object()
                        .and_then(|obj| obj.get("type"))
                        .and_then(|value| value.as_str())
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(canonicalize_cocoapods_license_type)
                })
        })
        .or_else(|| extracted_license_statement.map(canonicalize_cocoapods_license_type));

    normalize_spdx_declared_license(normalized_candidate.as_deref())
}

fn canonicalize_cocoapods_license_type(value: &str) -> String {
    match value.trim() {
        "Apache License, Version 2.0" => "Apache-2.0".to_string(),
        other => other.to_string(),
    }
}

/// Extracts VCS URL and download URL from source field.
fn extract_source_urls(json: &Value) -> (Option<String>, Option<String>) {
    let mut vcs_url = None;
    let mut download_url = None;

    if let Some(source) = json.get(FIELD_SOURCE) {
        if let Some(source_obj) = source.as_object() {
            // Git URL takes precedence for vcs_url
            if let Some(git_url) = source_obj.get("git").and_then(|v| v.as_str()) {
                let git_str = git_url.trim().to_string();
                if !git_str.is_empty() {
                    vcs_url = Some(git_str);
                }
            }

            // HTTP URL is download_url
            if let Some(http_url) = source_obj.get("http").and_then(|v| v.as_str()) {
                let http_str = http_url.trim().to_string();
                if !http_str.is_empty() {
                    download_url = Some(http_str);
                }
            }
        } else if let Some(source_str) = source.as_str() {
            // If source is a string, use as vcs_url
            let source_trimmed = source_str.trim().to_string();
            if !source_trimmed.is_empty() {
                vcs_url = Some(source_trimmed);
            }
        }
    }

    (vcs_url, download_url)
}

/// Extracts party information from authors field.
fn extract_parties(json: &Value) -> Vec<Party> {
    let mut parties = Vec::new();

    if let Some(authors) = json.get(FIELD_AUTHORS) {
        if let Some(authors_obj) = authors.as_object() {
            // Authors as dict: key=name, value=url
            for (name, value) in authors_obj {
                let name_str = name.trim().to_string();
                if !name_str.is_empty() {
                    let url = value.as_str().and_then(|s| {
                        let trimmed = s.trim();
                        // Python reference adds ".com" suffix if URL doesn't have it
                        if trimmed.is_empty() {
                            None
                        } else if trimmed.contains("://") || trimmed.contains('.') {
                            Some(trimmed.to_string())
                        } else {
                            Some(format!("{}.com", trimmed))
                        }
                    });

                    parties.push(Party {
                        r#type: Some("organization".to_string()),
                        role: Some("owner".to_string()),
                        name: Some(name_str),
                        email: None,
                        url,
                        organization: None,
                        organization_url: None,
                        timezone: None,
                    });
                }
            }
        } else if let Some(authors_str) = authors.as_str() {
            // Authors as string
            let authors_trimmed = authors_str.trim().to_string();
            if !authors_trimmed.is_empty() {
                parties.push(Party {
                    r#type: Some("organization".to_string()),
                    role: Some("owner".to_string()),
                    name: Some(authors_trimmed),
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

/// Extracts dependencies from dependencies dict.
fn extract_dependencies(json: &Value) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    if let Some(deps) = json.get(FIELD_DEPENDENCIES)
        && let Some(deps_obj) = deps.as_object()
    {
        for (name, requirement) in deps_obj {
            let name_str = name.trim();
            if name_str.is_empty() {
                continue;
            }

            let requirement_str = requirement
                .as_str()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());

            let purl = Some(format!("pkg:cocoapods/{}", name_str));

            dependencies.push(Dependency {
                purl,
                extracted_requirement: requirement_str,
                scope: Some("runtime".to_string()),
                is_runtime: Some(true),
                is_optional: Some(false),
                is_pinned: None,
                is_direct: None,
                resolved_package: None,
                extra_data: None,
            });
        }
    }

    dependencies
}

/// Gets the repository base URL from a VCS URL by removing .git suffix.
fn get_repo_base_url(vcs_url: &str) -> Option<String> {
    if vcs_url.is_empty() {
        return None;
    }

    if vcs_url.ends_with(".git") {
        Some(vcs_url.trim_end_matches(".git").to_string())
    } else {
        Some(vcs_url.to_string())
    }
}

/// Computes the hashed path prefix for CocoaPods Specs repository.
///
/// Uses MD5 hash of package name to generate the path prefix (first 3 chars).
fn get_hashed_path(name: &str) -> Option<String> {
    use md5::{Digest, Md5};

    if name.is_empty() {
        return None;
    }

    // Compute MD5 hash
    let mut hasher = Md5::new();
    hasher.update(name.as_bytes());
    let result = hasher.finalize();
    let hash_str = hex::encode(result);

    if hash_str.len() >= 3 {
        Some(format!(
            "{}/{}/{}",
            &hash_str[0..1],
            &hash_str[1..2],
            &hash_str[2..3]
        ))
    } else {
        Some(hash_str)
    }
}

crate::register_parser!(
    "CocoaPods .podspec.json manifest",
    &["**/*.podspec.json"],
    "cocoapods",
    "Objective-C",
    Some("https://guides.cocoapods.org/syntax/podspec.html"),
);
