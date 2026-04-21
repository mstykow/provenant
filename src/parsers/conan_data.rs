// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

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

use crate::models::{DatasourceId, PackageType, Sha256Digest};
use std::collections::HashMap;
use std::path::Path;

use crate::parser_warn as warn;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::models::PackageData;
use crate::parsers::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};

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
    sources: Option<HashMap<String, SourcesValue>>,
    patches: Option<HashMap<String, PatchesValue>>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
enum SourcesValue {
    Single(SourceInfo),
    Multiple(Vec<SourceInfo>),
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

impl SourceInfo {
    fn primary_download_url(&self) -> Option<String> {
        match &self.url {
            Some(UrlValue::Single(url)) => Some(truncate_field(url.clone())),
            Some(UrlValue::Multiple(urls)) if !urls.is_empty() => {
                Some(truncate_field(urls[0].clone()))
            }
            _ => None,
        }
    }

    fn additional_data_json(&self) -> serde_json::Value {
        let mut entry = serde_json::Map::new();

        if let Some(url) = &self.url {
            match url {
                UrlValue::Single(value) => {
                    entry.insert("url".to_string(), json!(truncate_field(value.clone())));
                }
                UrlValue::Multiple(values) => {
                    let urls: Vec<_> = values.iter().cloned().map(truncate_field).collect();
                    entry.insert("url".to_string(), json!(urls));
                }
            }
        }

        if let Some(sha256) = &self.sha256 {
            entry.insert("sha256".to_string(), json!(sha256));
        }

        serde_json::Value::Object(entry)
    }
}

fn sources_to_infos(sources_value: SourcesValue) -> Vec<SourceInfo> {
    match sources_value {
        SourcesValue::Single(source) => vec![source],
        SourcesValue::Multiple(sources) => sources,
    }
}

impl PackageParser for ConanDataParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.to_str().is_some_and(|p| p.ends_with("/conandata.yml"))
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
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

    for (version, sources_value) in sources.into_iter().take(MAX_ITERATION_COUNT) {
        let source_infos = sources_to_infos(sources_value);
        let mut extra_data = HashMap::new();

        let primary_index = source_infos
            .iter()
            .position(|source_info| {
                source_info.url.is_some()
                    || source_info
                        .sha256
                        .as_ref()
                        .is_some_and(|value| !value.is_empty())
            })
            .unwrap_or(0);

        let primary_source = source_infos.get(primary_index);

        let download_url = primary_source.and_then(SourceInfo::primary_download_url);

        if let Some(UrlValue::Multiple(urls)) =
            primary_source.and_then(|source| source.url.as_ref())
            && urls.len() > 1
        {
            let mirror_urls: Vec<_> = urls.iter().cloned().map(truncate_field).collect();
            extra_data.insert("mirror_urls".to_string(), json!(mirror_urls));
        }

        if source_infos.len() > 1 {
            let additional_sources: Vec<_> = source_infos
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != primary_index)
                .map(|(_, source_info)| source_info.additional_data_json())
                .collect();

            if !additional_sources.is_empty() {
                extra_data.insert("additional_sources".to_string(), json!(additional_sources));
            }
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
            version: Some(truncate_field(version)),
            download_url,
            sha256: primary_source
                .and_then(|source_info| source_info.sha256.as_deref())
                .and_then(|hash| Sha256Digest::from_hex(hash).ok()),
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
