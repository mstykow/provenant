use std::fs;
use std::path::Path;

use crate::models::{DatasourceId, PackageData, PackageType, Party};
use crate::parser_warn as warn;

use super::PackageParser;
use super::license_normalization::normalize_spdx_declared_license;

pub struct CitationCffParser;

impl PackageParser for CitationCffParser {
    const PACKAGE_TYPE: PackageType = PackageType::Generic;

    fn is_match(path: &Path) -> bool {
        path.file_name().and_then(|name| name.to_str()) == Some("CITATION.cff")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) => {
                warn!("Failed to read CITATION.cff at {:?}: {}", path, error);
                return vec![default_package_data()];
            }
        };

        let yaml: yaml_serde::Value = match yaml_serde::from_str(&content) {
            Ok(yaml) => yaml,
            Err(error) => {
                warn!("Failed to parse CITATION.cff at {:?}: {}", path, error);
                return vec![default_package_data()];
            }
        };

        vec![parse_citation_cff(&yaml)]
    }
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(CitationCffParser::PACKAGE_TYPE),
        datasource_id: Some(DatasourceId::CitationCff),
        ..Default::default()
    }
}

fn parse_citation_cff(yaml: &yaml_serde::Value) -> PackageData {
    if yaml
        .get("cff-version")
        .and_then(yaml_serde::Value::as_str)
        .is_none()
    {
        return default_package_data();
    }

    let mut package = default_package_data();
    package.name = yaml
        .get("title")
        .and_then(yaml_serde::Value::as_str)
        .map(str::to_string);
    package.version = yaml
        .get("version")
        .and_then(yaml_serde::Value::as_str)
        .map(str::to_string);
    package.description = yaml
        .get("abstract")
        .and_then(yaml_serde::Value::as_str)
        .or_else(|| yaml.get("message").and_then(yaml_serde::Value::as_str))
        .map(str::to_string);
    package.homepage_url = yaml
        .get("url")
        .and_then(yaml_serde::Value::as_str)
        .map(str::to_string);
    package.vcs_url = yaml
        .get("repository-code")
        .and_then(yaml_serde::Value::as_str)
        .map(str::to_string);
    package.parties = extract_author_parties(yaml.get("authors"));

    if let Some(license) = yaml.get("license").and_then(yaml_serde::Value::as_str) {
        let license = license.to_string();
        package.extracted_license_statement = Some(license.clone());
        let (declared, declared_spdx, detections) = normalize_spdx_declared_license(Some(&license));
        package.declared_license_expression = declared;
        package.declared_license_expression_spdx = declared_spdx;
        package.license_detections = detections;
    }

    package
}

fn extract_author_parties(value: Option<&yaml_serde::Value>) -> Vec<Party> {
    value
        .and_then(yaml_serde::Value::as_sequence)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let name = entry
                .get("name")
                .and_then(yaml_serde::Value::as_str)
                .map(str::to_string)
                .or_else(|| {
                    let given = entry.get("given-names").and_then(yaml_serde::Value::as_str);
                    let family = entry
                        .get("family-names")
                        .and_then(yaml_serde::Value::as_str);
                    match (given, family) {
                        (Some(given), Some(family)) => Some(format!("{given} {family}")),
                        (Some(given), None) => Some(given.to_string()),
                        (None, Some(family)) => Some(family.to_string()),
                        (None, None) => None,
                    }
                });
            let email = entry
                .get("email")
                .and_then(yaml_serde::Value::as_str)
                .map(str::to_string);
            let url = entry
                .get("orcid")
                .and_then(yaml_serde::Value::as_str)
                .map(str::to_string);

            if name.is_none() && email.is_none() && url.is_none() {
                return None;
            }

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
        })
        .collect()
}

crate::register_parser!(
    "citation cff metadata",
    &["**/CITATION.cff"],
    "generic",
    "Text",
    Some("https://citation-file-format.github.io/"),
);
