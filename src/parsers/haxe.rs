// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Parser for Haxe package manifests (haxelib.json).
//!
//! Extracts package metadata and dependencies from Haxe haxelib.json files.
//!
//! # Supported Formats
//! - haxelib.json (Haxe package manifest)
//!
//! # Key Features
//! - Dependency extraction with pinned/unpinned version tracking
//! - Contributor extraction with haxelib.org profile URLs
//! - License statement extraction
//! - Package URL (purl) generation
//!
//! # Implementation Notes
//! - Dependencies with empty string value mean unpinned (latest version)
//! - License must be one of: GPL, LGPL, BSD, Public, MIT, Apache
//! - All fields are extracted with graceful error handling

use crate::models::{DatasourceId, Dependency, PackageData, PackageType, Party};
use crate::parser_warn as warn;
use packageurl::PackageUrl;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use super::PackageParser;
use super::license_normalization::{
    DeclaredLicenseMatchMetadata, build_declared_license_data_from_pair,
    empty_declared_license_data,
};
use super::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};

/// Haxe package manifest (haxelib.json) parser.
///
/// Extracts package metadata, dependencies, and contributor information from
/// standard JSON haxelib.json manifest files used by the Haxe package manager.
pub struct HaxeParser;

impl PackageParser for HaxeParser {
    const PACKAGE_TYPE: PackageType = PackageType::Haxe;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "haxelib.json")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let json_content = match read_haxelib_json(path) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read or parse haxelib.json at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        let name = json_content.name.map(truncate_field);
        let version = json_content.version.map(truncate_field);

        // Generate PURL
        let purl = create_package_url(&name, &version);
        let extracted_license_statement = json_content.license.map(truncate_field);
        let (declared_license_expression, declared_license_expression_spdx, license_detections) =
            normalize_haxe_declared_license(extracted_license_statement.as_deref());

        // Generate URLs
        let (repository_homepage_url, download_url, repository_download_url) =
            if let Some(ref n) = name {
                let home = format!("https://lib.haxe.org/p/{}", n);
                if let Some(ref v) = version {
                    let dl = format!("https://lib.haxe.org/p/{}/{}/download/", n, v);
                    (Some(home), Some(dl.clone()), Some(dl))
                } else {
                    (Some(home), None, None)
                }
            } else {
                (None, None, None)
            };

        // Extract dependencies (maintain insertion order by sorting)
        let mut dependencies = Vec::new();
        let mut deps_list: Vec<_> = json_content
            .dependencies
            .into_iter()
            .take(MAX_ITERATION_COUNT)
            .collect();
        deps_list.sort_by(|a, b| a.0.cmp(&b.0));

        for (dep_name, dep_version) in deps_list {
            let is_pinned = !dep_version.is_empty();
            let dep_purl = create_dep_package_url(&dep_name, &dep_version, is_pinned);

            dependencies.push(Dependency {
                purl: dep_purl,
                extracted_requirement: None,
                scope: None,
                is_runtime: Some(true),
                is_optional: Some(false),
                is_pinned: Some(is_pinned),
                is_direct: Some(true),
                resolved_package: None,
                extra_data: None,
            });
        }

        // Extract contributors as parties
        let mut parties = Vec::new();
        for contrib in json_content
            .contributors
            .into_iter()
            .take(MAX_ITERATION_COUNT)
        {
            parties.push(Party {
                r#type: Some("person".to_string()),
                role: Some("contributor".to_string()),
                name: Some(truncate_field(contrib.clone())),
                email: None,
                url: Some(format!("https://lib.haxe.org/u/{}", contrib)),
                organization: None,
                organization_url: None,
                timezone: None,
            });
        }

        vec![PackageData {
            package_type: Some(Self::PACKAGE_TYPE),
            namespace: None,
            name,
            version,
            qualifiers: None,
            subpath: None,
            primary_language: Some("Haxe".to_string()),
            description: json_content.description.map(truncate_field),
            release_date: None,
            parties,
            keywords: json_content
                .tags
                .into_iter()
                .take(MAX_ITERATION_COUNT)
                .map(truncate_field)
                .collect(),
            homepage_url: json_content.url.map(truncate_field),
            download_url,
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
            extra_data: None,
            dependencies,
            repository_homepage_url,
            repository_download_url,
            api_data_url: None,
            datasource_id: Some(DatasourceId::HaxelibJson),
            purl,
        }]
    }
}

/// Internal structure for deserializing haxelib.json files.
#[derive(Debug, Deserialize, Serialize)]
struct HaxelibJson {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    contributors: Vec<String>,
    #[serde(default)]
    dependencies: HashMap<String, String>,
}

/// Read and parse a haxelib.json file.
fn read_haxelib_json(path: &Path) -> Result<HaxelibJson, String> {
    let content =
        read_file_to_string(path, None).map_err(|e| format!("Failed to read file: {}", e))?;

    serde_json::from_str(&content).map_err(|e| format!("Failed to parse JSON: {}", e))
}

/// Create a package URL for a Haxe package.
fn create_package_url(name: &Option<String>, version: &Option<String>) -> Option<String> {
    name.as_ref().and_then(|name| {
        let mut package_url = match PackageUrl::new("haxe", name) {
            Ok(p) => p,
            Err(e) => {
                warn!(
                    "Failed to create PackageUrl for haxe package '{}': {}",
                    name, e
                );
                return None;
            }
        };

        if let Some(v) = version
            && let Err(e) = package_url.with_version(v)
        {
            warn!(
                "Failed to set version '{}' for haxe package '{}': {}",
                v, name, e
            );
            return None;
        }

        Some(package_url.to_string())
    })
}

/// Create a package URL for a Haxe dependency.
fn create_dep_package_url(name: &str, version: &str, is_pinned: bool) -> Option<String> {
    let mut package_url = match PackageUrl::new("haxe", name) {
        Ok(p) => p,
        Err(e) => {
            warn!(
                "Failed to create PackageUrl for haxe dependency '{}': {}",
                name, e
            );
            return None;
        }
    };

    if is_pinned && let Err(e) = package_url.with_version(version) {
        warn!(
            "Failed to set version '{}' for haxe dependency '{}': {}",
            version, name, e
        );
        return None;
    }

    Some(package_url.to_string())
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(HaxeParser::PACKAGE_TYPE),
        primary_language: Some("Haxe".to_string()),
        datasource_id: Some(DatasourceId::HaxelibJson),
        ..Default::default()
    }
}

fn normalize_haxe_declared_license(
    statement: Option<&str>,
) -> (
    Option<String>,
    Option<String>,
    Vec<crate::models::LicenseDetection>,
) {
    match statement.map(str::trim).filter(|value| !value.is_empty()) {
        Some("MIT") => build_declared_license_data_from_pair(
            "mit",
            "MIT",
            DeclaredLicenseMatchMetadata::single_line("MIT"),
        ),
        _ => empty_declared_license_data(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DatasourceId;
    use std::path::PathBuf;

    #[test]
    fn test_is_match() {
        let valid_path = PathBuf::from("/some/path/haxelib.json");
        let invalid_path = PathBuf::from("/some/path/not_haxelib.json");

        assert!(HaxeParser::is_match(&valid_path));
        assert!(!HaxeParser::is_match(&invalid_path));
    }

    #[test]
    fn test_extract_from_testdata_basic() {
        let haxelib_path = PathBuf::from("testdata/haxe/basic/haxelib.json");
        let package_data = HaxeParser::extract_first_package(&haxelib_path);

        assert_eq!(package_data.package_type, Some(PackageType::Haxe));
        assert_eq!(package_data.name, Some("haxelib".to_string()));
        assert_eq!(package_data.version, Some("3.4.0".to_string()));
        assert_eq!(
            package_data.homepage_url,
            Some("https://lib.haxe.org/documentation/".to_string())
        );
        assert_eq!(
            package_data.download_url,
            Some("https://lib.haxe.org/p/haxelib/3.4.0/download/".to_string())
        );
        assert_eq!(
            package_data.repository_homepage_url,
            Some("https://lib.haxe.org/p/haxelib".to_string())
        );
        assert_eq!(
            package_data.extracted_license_statement,
            Some("GPL".to_string())
        );

        // Check PURL
        assert_eq!(
            package_data.purl,
            Some("pkg:haxe/haxelib@3.4.0".to_string())
        );

        // Check contributors extraction
        assert_eq!(package_data.parties.len(), 6);
        let names: Vec<&str> = package_data
            .parties
            .iter()
            .filter_map(|p| p.name.as_deref())
            .collect();
        assert!(names.contains(&"back2dos"));
        assert!(names.contains(&"ncannasse"));
    }

    #[test]
    fn test_extract_with_dependencies() {
        let haxelib_path = PathBuf::from("testdata/haxe/deps/haxelib.json");
        let package_data = HaxeParser::extract_first_package(&haxelib_path);

        assert_eq!(package_data.name, Some("selecthxml".to_string()));
        assert_eq!(package_data.version, Some("0.5.1".to_string()));

        // Check dependencies: tink_core (unpinned), tink_macro (pinned to 3.23)
        assert_eq!(package_data.dependencies.len(), 2);

        let pinned_deps: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|d| d.is_pinned == Some(true))
            .collect();
        assert_eq!(pinned_deps.len(), 1);
        assert!(pinned_deps[0].purl.as_ref().unwrap().contains("@3.23"));

        let unpinned_deps: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|d| d.is_pinned == Some(false))
            .collect();
        assert_eq!(unpinned_deps.len(), 1);
    }

    #[test]
    fn test_extract_with_tags() {
        let haxelib_path = PathBuf::from("testdata/haxe/tags/haxelib.json");
        let package_data = HaxeParser::extract_first_package(&haxelib_path);

        assert_eq!(package_data.name, Some("tink_core".to_string()));
        assert_eq!(package_data.version, Some("1.18.0".to_string()));
        assert_eq!(
            package_data.extracted_license_statement,
            Some("MIT".to_string())
        );

        // Check keywords extracted from tags
        assert_eq!(
            package_data.keywords,
            vec![
                "tink".to_string(),
                "cross".to_string(),
                "utility".to_string(),
                "reactive".to_string(),
                "functional".to_string(),
                "async".to_string(),
                "lazy".to_string(),
                "signal".to_string(),
                "event".to_string(),
            ]
        );
    }

    #[test]
    fn test_invalid_file() {
        let nonexistent_path = PathBuf::from("testdata/haxe/nonexistent/haxelib.json");
        let package_data = HaxeParser::extract_first_package(&nonexistent_path);

        // Should return default data with proper type and datasource
        assert_eq!(package_data.package_type, Some(PackageType::Haxe));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::HaxelibJson));
        assert!(package_data.name.is_none());
    }
}

crate::register_parser!(
    "Haxe haxelib.json package manifest",
    &["**/haxelib.json"],
    "haxe",
    "Haxe",
    Some("https://lib.haxe.org/documentation/creating-a-haxelib-package/"),
);
