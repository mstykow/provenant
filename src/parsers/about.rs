//! Parser for AboutCode .ABOUT metadata files.
//!
//! Extracts package metadata from AboutCode .ABOUT YAML files which describe
//! software components, licenses, and related information.
//!
//! # Supported Formats
//! - .ABOUT (case-sensitive uppercase extension)
//!
//! # Key Features
//! - YAML-based metadata parsing
//! - Package URL (purl) parsing for type/namespace extraction
//! - Owner party information
//! - File reference tracking (about_resource field)
//! - License expression extraction
//! - Flexible field mapping (home_url/homepage_url)
//!
//! # Implementation Notes
//! - Uses yaml_serde for YAML parsing
//! - Uses packageurl crate for purl parsing
//! - Extension is case-sensitive and must be uppercase (.ABOUT not .about)
//! - Type can be overridden by 'type' field or extracted from 'purl' field
//! - Graceful error handling: logs warnings and returns default on parse failure

use crate::models::{DatasourceId, FileReference, PackageData, PackageType, Party};
use crate::parser_warn as warn;
use packageurl::PackageUrl;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use url::Url;
use yaml_serde::Value;

use super::PackageParser;
use super::license_normalization::{
    DeclaredLicenseMatchMetadata, build_declared_license_data, normalize_spdx_declared_license,
    normalize_spdx_expression,
};

const FIELD_TYPE: &str = "type";
const FIELD_PURL: &str = "purl";
const FIELD_PACKAGE_URL: &str = "package_url";
const FIELD_NAMESPACE: &str = "namespace";
const FIELD_NAME: &str = "name";
const FIELD_VERSION: &str = "version";
const FIELD_HOME_URL: &str = "home_url";
const FIELD_HOMEPAGE_URL: &str = "homepage_url";
const FIELD_DOWNLOAD_URL: &str = "download_url";
const FIELD_COPYRIGHT: &str = "copyright";
const FIELD_LICENSE_EXPRESSION: &str = "license_expression";
const FIELD_OWNER: &str = "owner";
const FIELD_ABOUT_RESOURCE: &str = "about_resource";

/// AboutCode .ABOUT file parser.
///
/// Parses AboutCode metadata files that contain package information,
/// licensing, and file references in YAML format.
pub struct AboutFileParser;

#[derive(Clone)]
struct InferredAboutIdentity {
    package_type: PackageType,
    namespace: Option<String>,
    name: Option<String>,
    version: Option<String>,
}

impl PackageParser for AboutFileParser {
    const PACKAGE_TYPE: PackageType = PackageType::About;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let yaml = match read_and_parse_yaml(path) {
            Ok(yaml) => yaml,
            Err(e) => {
                warn!("Failed to read or parse .ABOUT file at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        // Extract type and purl information
        let about_type = yaml
            .get(FIELD_TYPE)
            .and_then(|v| v.as_str())
            .map(String::from);

        let about_namespace = yaml
            .get(FIELD_NAMESPACE)
            .and_then(|v| v.as_str())
            .map(String::from);

        let purl_string = yaml
            .get(FIELD_PURL)
            .and_then(|v| v.as_str())
            .or_else(|| yaml.get(FIELD_PACKAGE_URL).and_then(|v| v.as_str()))
            .map(String::from);

        // Parse purl if present
        let (purl_type, purl_namespace, purl_name, purl_version) =
            if let Some(ref purl_str) = purl_string {
                match PackageUrl::from_str(purl_str) {
                    Ok(purl) => (
                        Some(purl.ty().to_string()),
                        purl.namespace().map(String::from),
                        Some(purl.name().to_string()),
                        purl.version().map(String::from),
                    ),
                    Err(e) => {
                        warn!("Failed to parse purl '{}': {}", purl_str, e);
                        (None, None, None, None)
                    }
                }
            } else {
                (None, None, None, None)
            };

        let inferred = infer_about_from_download_url(
            yaml.get(FIELD_DOWNLOAD_URL).and_then(|v| v.as_str()),
            yaml.get(FIELD_NAME)
                .and_then(yaml_value_to_string)
                .as_deref(),
            yaml.get(FIELD_VERSION)
                .and_then(yaml_value_to_string)
                .as_deref(),
        );

        let package_type = about_type
            .clone()
            .or(purl_type)
            .and_then(|s| s.parse::<crate::models::PackageType>().ok())
            .or_else(|| inferred.as_ref().map(|identity| identity.package_type))
            .unwrap_or(Self::PACKAGE_TYPE);

        // Priority: about_namespace > purl_namespace
        let namespace = about_namespace
            .clone()
            .or(purl_namespace.clone())
            .or_else(|| {
                inferred
                    .as_ref()
                    .and_then(|identity| identity.namespace.clone())
            });

        // Name and version from YAML or purl
        let name = yaml
            .get(FIELD_NAME)
            .and_then(yaml_value_to_string)
            .or(purl_name.clone())
            .or_else(|| inferred.as_ref().and_then(|identity| identity.name.clone()));

        let version = yaml
            .get(FIELD_VERSION)
            .and_then(yaml_value_to_string)
            .or(purl_version.clone())
            .or_else(|| {
                inferred
                    .as_ref()
                    .and_then(|identity| identity.version.clone())
            });

        // Homepage URL (two possible field names)
        let homepage_url = yaml
            .get(FIELD_HOME_URL)
            .and_then(|v| v.as_str())
            .or_else(|| yaml.get(FIELD_HOMEPAGE_URL).and_then(|v| v.as_str()))
            .map(String::from);

        let download_url = yaml
            .get(FIELD_DOWNLOAD_URL)
            .and_then(|v| v.as_str())
            .map(String::from);

        let copyright = yaml
            .get(FIELD_COPYRIGHT)
            .and_then(|v| v.as_str())
            .map(String::from);

        let extracted_license_statement = yaml
            .get(FIELD_LICENSE_EXPRESSION)
            .and_then(|v| v.as_str())
            .map(String::from);
        let file_references = extract_file_references(&yaml);
        let (declared_license_expression, declared_license_expression_spdx, license_detections) =
            extracted_license_statement
                .as_deref()
                .and_then(normalize_spdx_expression)
                .map(|normalized| {
                    build_declared_license_data(
                        normalized,
                        DeclaredLicenseMatchMetadata::single_line(
                            extracted_license_statement.as_deref().unwrap_or_default(),
                        ),
                    )
                })
                .unwrap_or_else(|| {
                    normalize_spdx_declared_license(extracted_license_statement.as_deref())
                });

        let vcs_url = yaml
            .get(Value::String("vcs_url".to_string()))
            .and_then(|v| v.as_str())
            .map(String::from);

        let extra_data = build_extra_data(&yaml);

        let purl = purl_string.or_else(|| {
            let name = yaml
                .get(FIELD_NAME)
                .and_then(yaml_value_to_string)
                .or(purl_name.clone())
                .or_else(|| inferred.as_ref().and_then(|identity| identity.name.clone()));
            let version = yaml
                .get(FIELD_VERSION)
                .and_then(yaml_value_to_string)
                .or(purl_version.clone())
                .or_else(|| {
                    inferred
                        .as_ref()
                        .and_then(|identity| identity.version.clone())
                });
            let namespace = about_namespace.clone().or_else(|| {
                inferred
                    .as_ref()
                    .and_then(|identity| identity.namespace.clone())
            });
            build_about_purl(
                package_type,
                namespace.as_deref(),
                name.as_deref(),
                version.as_deref(),
            )
        });

        // Owner party
        let parties = extract_owner_party(&yaml);

        // File references
        vec![PackageData {
            package_type: Some(package_type),
            namespace,
            name,
            version,
            qualifiers: None,
            subpath: None,
            primary_language: None,
            description: None,
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
            bug_tracking_url: None,
            code_view_url: None,
            vcs_url,
            copyright,
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
            file_references,
            is_private: false,
            is_virtual: false,
            extra_data,
            dependencies: Vec::new(),
            repository_homepage_url: None,
            repository_download_url: None,
            api_data_url: None,
            datasource_id: Some(DatasourceId::AboutFile),
            purl,
        }]
    }

    fn is_match(path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext == "ABOUT")
    }
}

/// Reads and parses a YAML file.
fn read_and_parse_yaml(path: &Path) -> Result<yaml_serde::Mapping, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;

    let value: Value =
        yaml_serde::from_str(&content).map_err(|e| format!("Failed to parse YAML: {}", e))?;

    match value {
        Value::Mapping(map) => Ok(map),
        _ => Err("Expected YAML mapping at root".to_string()),
    }
}

/// Converts a YAML value to a string, handling strings, numbers, and booleans.
fn yaml_value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Extracts owner party information from YAML.
fn extract_owner_party(yaml: &yaml_serde::Mapping) -> Vec<Party> {
    let owner = yaml
        .get(Value::String(FIELD_OWNER.to_string()))
        .map(|v| match v {
            Value::String(s) => s.clone(),
            _ => {
                // Convert non-string values to their debug representation
                format!("{:?}", v)
            }
        });

    if let Some(owner_name) = owner {
        if !owner_name.is_empty() {
            vec![Party {
                r#type: Some("person".to_string()),
                role: Some("owner".to_string()),
                name: Some(owner_name),
                email: None,
                url: None,
                organization: None,
                organization_url: None,
                timezone: None,
            }]
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    }
}

/// Extracts file references from YAML.
fn extract_file_references(yaml: &yaml_serde::Mapping) -> Vec<FileReference> {
    let about_resource = yaml
        .get(Value::String(FIELD_ABOUT_RESOURCE.to_string()))
        .and_then(|v| v.as_str());
    let license_file = yaml
        .get(Value::String("license_file".to_string()))
        .and_then(|v| v.as_str());
    let notice_file = yaml
        .get(Value::String("notice_file".to_string()))
        .and_then(|v| v.as_str());

    let mut refs = Vec::new();

    if let Some(path) = about_resource {
        refs.push(FileReference {
            path: path.to_string(),
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            extra_data: None,
        });
    }

    for path in [license_file, notice_file].into_iter().flatten() {
        refs.push(FileReference {
            path: path.to_string(),
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            extra_data: None,
        });
    }

    refs
}

/// Returns a default (empty) PackageData structure.
fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(PackageType::About),
        datasource_id: Some(DatasourceId::AboutFile),
        ..Default::default()
    }
}

fn infer_about_from_download_url(
    download_url: Option<&str>,
    about_name: Option<&str>,
    about_version: Option<&str>,
) -> Option<InferredAboutIdentity> {
    let url = Url::parse(download_url?).ok()?;
    let host = url.host_str()?;

    if matches!(host, "pypi.python.org" | "files.pythonhosted.org") {
        let name = about_name.map(str::to_string)?;
        let version = about_version.map(str::to_string);
        return Some(InferredAboutIdentity {
            package_type: PackageType::Pypi,
            namespace: None,
            name: Some(name),
            version,
        });
    }

    if matches!(host, "raw.githubusercontent.com" | "github.com") {
        let mut segments = url.path_segments()?;
        let owner = segments.next()?.to_string();
        let repo = segments.next()?.to_string();
        return Some(InferredAboutIdentity {
            package_type: PackageType::Github,
            namespace: Some(owner),
            name: Some(repo),
            version: None,
        });
    }

    None
}

fn build_about_purl(
    package_type: PackageType,
    namespace: Option<&str>,
    name: Option<&str>,
    version: Option<&str>,
) -> Option<String> {
    if package_type == PackageType::About {
        return None;
    }

    let name = name?;
    let mut purl = PackageUrl::new(package_type.as_str(), name).ok()?;
    if let Some(namespace) = namespace {
        purl.with_namespace(namespace).ok()?;
    }
    if let Some(version) = version {
        purl.with_version(version).ok()?;
    }
    Some(purl.to_string())
}

fn build_extra_data(
    yaml: &yaml_serde::Mapping,
) -> Option<std::collections::HashMap<String, serde_json::Value>> {
    let mut extra_data = std::collections::HashMap::new();
    for key in ["license_file", "notice_file", "notes"] {
        if let Some(value) = yaml.get(Value::String(key.to_string()))
            && let Some(value) = yaml_value_to_string(value)
        {
            extra_data.insert(key.to_string(), serde_json::Value::String(value));
        }
    }
    (!extra_data.is_empty()).then_some(extra_data)
}

crate::register_parser!(
    "AboutCode .ABOUT metadata file",
    &["**/*.ABOUT"],
    "about",
    "",
    Some("https://aboutcode-toolkit.readthedocs.io/en/latest/specification.html"),
);
