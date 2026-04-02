use std::fs;
use std::path::Path;

use crate::models::{DatasourceId, PackageData, PackageType, Party};
use crate::parser_warn as warn;

use super::PackageParser;
use super::license_normalization::normalize_spdx_declared_license;

pub struct PubliccodeParser;

impl PackageParser for PubliccodeParser {
    const PACKAGE_TYPE: PackageType = PackageType::Publiccode;

    fn is_match(path: &Path) -> bool {
        matches!(
            path.file_name().and_then(|name| name.to_str()),
            Some("publiccode.yml" | "publiccode.yaml")
        )
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) => {
                warn!(
                    "Failed to read publiccode metadata at {:?}: {}",
                    path, error
                );
                return vec![default_package_data()];
            }
        };

        let yaml: yaml_serde::Value = match yaml_serde::from_str(&content) {
            Ok(yaml) => yaml,
            Err(error) => {
                warn!(
                    "Failed to parse publiccode metadata at {:?}: {}",
                    path, error
                );
                return vec![default_package_data()];
            }
        };

        vec![parse_publiccode(&yaml)]
    }
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(PubliccodeParser::PACKAGE_TYPE),
        datasource_id: Some(DatasourceId::PubliccodeYaml),
        ..Default::default()
    }
}

fn parse_publiccode(yaml: &yaml_serde::Value) -> PackageData {
    if yaml
        .get("publiccodeYmlVersion")
        .and_then(yaml_value_as_string)
        .is_none()
    {
        return default_package_data();
    }

    let mut package = default_package_data();
    package.name = yaml
        .get("name")
        .and_then(extract_localized_string)
        .map(str::to_string);
    package.version = yaml
        .get("softwareVersion")
        .and_then(yaml_value_as_string)
        .map(str::to_string);
    package.vcs_url = yaml
        .get("url")
        .and_then(yaml_value_as_string)
        .map(str::to_string);
    package.homepage_url = yaml
        .get("landingURL")
        .and_then(yaml_value_as_string)
        .map(str::to_string);
    package.description = yaml
        .get("longDescription")
        .and_then(extract_localized_string)
        .or_else(|| {
            yaml.get("shortDescription")
                .and_then(extract_localized_string)
        })
        .map(str::to_string);
    package.copyright = yaml
        .get("legal")
        .and_then(|legal| legal.get("mainCopyrightOwner"))
        .and_then(yaml_value_as_string)
        .or_else(|| yaml.get("repoOwner").and_then(yaml_value_as_string))
        .map(str::to_string);
    package.parties = extract_contact_parties(yaml.get("maintenance"));

    if let Some(license) = yaml
        .get("legal")
        .and_then(|legal| legal.get("license"))
        .and_then(yaml_value_as_string)
    {
        let license = license.to_string();
        package.extracted_license_statement = Some(license.clone());
        let (declared, declared_spdx, detections) = normalize_spdx_declared_license(Some(&license));
        package.declared_license_expression = declared;
        package.declared_license_expression_spdx = declared_spdx;
        package.license_detections = detections;
    }

    package
}

fn extract_localized_string(value: &yaml_serde::Value) -> Option<&str> {
    if let Some(string) = value.as_str() {
        return Some(string);
    }

    if let Some(english) = value.get("en").and_then(yaml_value_as_string) {
        return Some(english);
    }

    value
        .as_mapping()
        .and_then(|mapping| mapping.values().find_map(yaml_serde::Value::as_str))
}

fn extract_contact_parties(maintenance: Option<&yaml_serde::Value>) -> Vec<Party> {
    maintenance
        .and_then(|maintenance| maintenance.get("contacts"))
        .and_then(yaml_serde::Value::as_sequence)
        .into_iter()
        .flatten()
        .filter_map(|contact| {
            let name = contact
                .get("name")
                .and_then(yaml_value_as_string)
                .map(str::to_string);
            let email = contact
                .get("email")
                .and_then(yaml_value_as_string)
                .map(str::to_string);
            let url = contact
                .get("url")
                .and_then(yaml_value_as_string)
                .map(str::to_string);

            if name.is_none() && email.is_none() && url.is_none() {
                return None;
            }

            Some(Party {
                r#type: Some("person".to_string()),
                role: Some("maintainer".to_string()),
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

fn yaml_value_as_string(value: &yaml_serde::Value) -> Option<&str> {
    value.as_str()
}

crate::register_parser!(
    "publiccode metadata",
    &["**/publiccode.yml", "**/publiccode.yaml"],
    "publiccode",
    "YAML",
    Some("https://yml.publiccode.tools/"),
);
