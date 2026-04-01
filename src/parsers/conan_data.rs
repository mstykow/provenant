//! Parser for Conan conandata.yml files.
//!
//! Extracts package metadata from `conandata.yml` files which contain
//! external source information for Conan packages.
//!
//! # Supported Formats
//! - `conandata.yml` - Conan external source metadata
//!
//! # Key Features
//! - Version-specific source URLs
//! - SHA256 checksums
//! - Multiple source mirrors support
//! - Patch metadata extraction (beyond Python which ignores patches)
//!
//! # Implementation Notes
//! - Format: YAML with `sources` dict containing version→{url, sha256}
//! - Each version can have multiple URLs (list or single string)
//! - Patches section contains version→[{patch_file, patch_description, patch_type}]
//! - Spec: https://docs.conan.io/2/tutorial/creating_packages/handle_sources_in_packages.html

use crate::models::{DatasourceId, PackageType};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::parser_warn as warn;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::models::PackageData;

use super::PackageParser;

const PACKAGE_TYPE: PackageType = PackageType::Conan;

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(PACKAGE_TYPE),
        primary_language: Some("C++".to_string()),
        datasource_id: Some(DatasourceId::ConanConanDataYml),
        ..Default::default()
    }
}

/// Parser for Conan conandata.yml files
pub struct ConanDataParser;

#[derive(Debug, Deserialize, Serialize)]
struct ConanDataYml {
    sources: Option<HashMap<String, SourceInfo>>,
    patches: Option<HashMap<String, PatchesValue>>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
enum UrlValue {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
enum PatchesValue {
    List(Vec<PatchInfo>),
    String(String),
}

#[derive(Debug, Deserialize, Serialize)]
struct PatchInfo {
    patch_file: Option<String>,
    patch_description: Option<String>,
    patch_type: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct SourceInfo {
    url: Option<UrlValue>,
    sha256: Option<String>,
}

impl PackageParser for ConanDataParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.to_str().is_some_and(|p| p.ends_with("/conandata.yml"))
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read conandata.yml file {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        parse_conandata_yml(&content)
    }
}

pub(crate) fn parse_conandata_yml(content: &str) -> Vec<PackageData> {
    let data: ConanDataYml = match yaml_serde::from_str(content) {
        Ok(d) => d,
        Err(e) => {
            warn!("Failed to parse conandata.yml: {}", e);
            return vec![default_package_data()];
        }
    };

    let Some(sources) = data.sources else {
        return vec![default_package_data()];
    };

    let mut packages = Vec::new();

    for (version, source_info) in sources {
        let mut extra_data = HashMap::new();

        let download_url = match &source_info.url {
            Some(UrlValue::Single(url)) => Some(url.clone()),
            Some(UrlValue::Multiple(urls)) if !urls.is_empty() => Some(urls[0].clone()),
            _ => None,
        };

        if let Some(UrlValue::Multiple(urls)) = &source_info.url
            && urls.len() > 1
        {
            extra_data.insert("mirror_urls".to_string(), json!(urls));
        }

        if let Some(ref patches_map) = data.patches
            && let Some(patches_value) = patches_map.get(&version)
        {
            let patches_json = match patches_value {
                PatchesValue::List(patches) => {
                    let patches_data: Vec<_> = patches
                        .iter()
                        .map(|p| {
                            json!({
                                "patch_file": p.patch_file,
                                "patch_description": p.patch_description,
                                "patch_type": p.patch_type,
                            })
                        })
                        .collect();
                    json!(patches_data)
                }
                PatchesValue::String(s) => json!(s),
            };
            extra_data.insert("patches".to_string(), patches_json);
        }

        packages.push(PackageData {
            package_type: Some(PACKAGE_TYPE),
            primary_language: Some("C++".to_string()),
            version: Some(version),
            download_url,
            sha256: source_info.sha256,
            extra_data: if extra_data.is_empty() {
                None
            } else {
                Some(extra_data)
            },
            datasource_id: Some(DatasourceId::ConanConanDataYml),
            ..Default::default()
        });
    }

    if packages.is_empty() {
        packages.push(default_package_data());
    }

    packages
}

crate::register_parser!(
    "Conan external source metadata",
    &["*/conandata.yml"],
    "conan",
    "C++",
    Some("https://docs.conan.io/2/tutorial/creating_packages/handle_sources_in_packages.html"),
);
