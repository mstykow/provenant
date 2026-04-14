//! Parser for Cargo.toml manifest files.
//!
//! Extracts package metadata, dependencies, and license information from
//! Rust Cargo.toml files.
//!
//! # Supported Formats
//! - Cargo.toml (manifest)
//!
//! # Key Features
//! - Dependency extraction with feature flags and optional dependencies
//! - `is_pinned` analysis (exact version vs range specifiers)
//! - Package URL (purl) generation
//! - Workspace inheritance detection (stores `"workspace"` markers in extra_data)
//!
//! # Implementation Notes
//! - Uses toml crate for parsing
//! - Version pinning: `"1.0.0"` is pinned, `"^1.0.0"` is not
//! - Graceful error handling with `warn!()` logs
//! - Direct dependencies: all in manifest are direct (no lockfile)

use crate::models::{DatasourceId, Dependency, FileReference, PackageData, PackageType, Party};
use crate::parser_warn as warn;
use crate::parsers::utils::{
    MAX_ITERATION_COUNT, read_file_to_string, split_name_email, truncate_field,
};
use packageurl::PackageUrl;
use std::path::Path;
use toml::Value;

use super::PackageParser;
use super::license_normalization::{
    DeclaredLicenseMatchMetadata, build_declared_license_data, empty_declared_license_data,
    normalize_spdx_expression,
};

const FIELD_PACKAGE: &str = "package";
const FIELD_NAME: &str = "name";
const FIELD_VERSION: &str = "version";
const FIELD_LICENSE: &str = "license";
const FIELD_LICENSE_FILE: &str = "license-file";
const FIELD_AUTHORS: &str = "authors";
const FIELD_REPOSITORY: &str = "repository";
const FIELD_HOMEPAGE: &str = "homepage";
const FIELD_DEPENDENCIES: &str = "dependencies";
const FIELD_DEV_DEPENDENCIES: &str = "dev-dependencies";
const FIELD_DEV_DEPENDENCIES_LEGACY: &str = "dev_dependencies";
const FIELD_BUILD_DEPENDENCIES: &str = "build-dependencies";
const FIELD_BUILD_DEPENDENCIES_LEGACY: &str = "build_dependencies";
const FIELD_DESCRIPTION: &str = "description";
const FIELD_KEYWORDS: &str = "keywords";
const FIELD_CATEGORIES: &str = "categories";
const FIELD_RUST_VERSION: &str = "rust-version";
const FIELD_EDITION: &str = "edition";
const FIELD_README: &str = "readme";
const FIELD_PUBLISH: &str = "publish";

/// Rust Cargo.toml manifest parser.
///
/// Extracts package metadata including dependencies (regular, dev, build),
/// license information, and crate-specific fields.
pub struct CargoParser;

impl PackageParser for CargoParser {
    const PACKAGE_TYPE: PackageType = PackageType::Cargo;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let toml_content = match read_cargo_toml(path) {
            Ok(content) => content,
            Err(_) => return Vec::new(),
        };

        let package = toml_content.get(FIELD_PACKAGE).and_then(|v| v.as_table());

        let name = package
            .and_then(|p| p.get(FIELD_NAME))
            .and_then(|v| v.as_str())
            .map(|s| truncate_field(s.to_string()));

        let version = package
            .and_then(|p| p.get(FIELD_VERSION))
            .and_then(|v| v.as_str())
            .map(|s| truncate_field(s.to_string()));

        let raw_license = package
            .and_then(|p| p.get(FIELD_LICENSE))
            .and_then(|v| v.as_str())
            .map(|s| truncate_field(s.to_string()));
        let file_references = extract_file_references(&toml_content);
        let (declared_license_expression, declared_license_expression_spdx, license_detections) =
            raw_license
                .as_deref()
                .and_then(normalize_spdx_expression)
                .map(|normalized| {
                    build_declared_license_data(
                        normalized,
                        DeclaredLicenseMatchMetadata::single_line(
                            raw_license.as_deref().unwrap_or_default(),
                        ),
                    )
                })
                .unwrap_or_else(empty_declared_license_data);

        let extracted_license_statement = raw_license.clone();

        let dependencies = extract_dependencies_for_scopes(&toml_content, &[FIELD_DEPENDENCIES]);
        let dev_dependencies = extract_dependencies_for_scopes(
            &toml_content,
            &[FIELD_DEV_DEPENDENCIES, FIELD_DEV_DEPENDENCIES_LEGACY],
        );
        let build_dependencies = extract_dependencies_for_scopes(
            &toml_content,
            &[FIELD_BUILD_DEPENDENCIES, FIELD_BUILD_DEPENDENCIES_LEGACY],
        );

        let purl = create_package_url(&name, &version);

        let homepage_url = package
            .and_then(|p| p.get(FIELD_HOMEPAGE))
            .and_then(|v| v.as_str())
            .map(|s| truncate_field(s.to_string()))
            .or_else(|| {
                name.as_ref()
                    .map(|n| format!("https://crates.io/crates/{}", n))
            });

        let repository_url = package
            .and_then(|p| p.get(FIELD_REPOSITORY))
            .and_then(|v| v.as_str())
            .map(|s| truncate_field(s.to_string()));
        let download_url = None;

        let api_data_url = generate_cargo_api_url(&name, &version);

        let repository_homepage_url = name
            .as_ref()
            .map(|n| format!("https://crates.io/crates/{}", n));

        let repository_download_url = match (&name, &version) {
            (Some(n), Some(v)) => Some(format!(
                "https://crates.io/api/v1/crates/{}/{}/download",
                n, v
            )),
            _ => None,
        };

        let description = package
            .and_then(|p| p.get(FIELD_DESCRIPTION))
            .and_then(|v| v.as_str())
            .map(|s| truncate_field(s.trim().to_string()));

        let keywords = extract_keywords_and_categories(&toml_content);

        let extra_data = extract_extra_data(&toml_content);
        let is_private = package
            .and_then(|p| p.get(FIELD_PUBLISH))
            .is_some_and(|value| matches!(value, Value::Boolean(false)));
        vec![PackageData {
            package_type: Some(Self::PACKAGE_TYPE),
            namespace: None,
            name,
            version,
            qualifiers: None,
            subpath: None,
            primary_language: Some("Rust".to_string()),
            description,
            release_date: None,
            parties: extract_parties(&toml_content),
            keywords,
            homepage_url,
            download_url,
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            bug_tracking_url: None,
            code_view_url: None,
            vcs_url: repository_url,
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
            file_references,
            is_private,
            is_virtual: false,
            extra_data,
            dependencies: [dependencies, dev_dependencies, build_dependencies].concat(),
            repository_homepage_url,
            repository_download_url,
            api_data_url,
            datasource_id: Some(DatasourceId::CargoToml),
            purl,
        }]
    }

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("cargo.toml"))
    }
}

/// Reads and parses a TOML file
fn read_cargo_toml(path: &Path) -> Result<Value, String> {
    let content =
        read_file_to_string(path, None).map_err(|e| format!("Failed to read file: {}", e))?;

    toml::from_str(&content).map_err(|e| format!("Failed to parse TOML: {}", e))
}

fn generate_cargo_api_url(name: &Option<String>, _version: &Option<String>) -> Option<String> {
    const REGISTRY: &str = "https://crates.io/api/v1/crates";
    name.as_ref().map(|name| format!("{}/{}", REGISTRY, name))
}

fn create_package_url(name: &Option<String>, version: &Option<String>) -> Option<String> {
    name.as_ref().and_then(|name| {
        let mut package_url = match PackageUrl::new(CargoParser::PACKAGE_TYPE.as_str(), name) {
            Ok(p) => p,
            Err(e) => {
                warn!(
                    "Failed to create PackageUrl for cargo package '{}': {}",
                    name, e
                );
                return None;
            }
        };

        if let Some(v) = version
            && let Err(e) = package_url.with_version(v)
        {
            warn!(
                "Failed to set version '{}' for cargo package '{}': {}",
                v, name, e
            );
            return None;
        }

        Some(package_url.to_string())
    })
}

/// Extracts party information from the `authors` field
fn extract_parties(toml_content: &Value) -> Vec<Party> {
    let mut parties = Vec::new();

    if let Some(package) = toml_content.get(FIELD_PACKAGE).and_then(|v| v.as_table())
        && let Some(authors) = package.get(FIELD_AUTHORS).and_then(|v| v.as_array())
    {
        for author in authors.iter().take(MAX_ITERATION_COUNT) {
            if let Some(author_str) = author.as_str() {
                let (name, email) = split_name_email(author_str);
                parties.push(Party {
                    r#type: None,
                    role: Some("author".to_string()),
                    name,
                    email,
                    url: None,
                    organization: None,
                    organization_url: None,
                    timezone: None,
                });
            }
        }
        if authors.len() > MAX_ITERATION_COUNT {
            warn!(
                "Authors array has {} entries, capping at MAX_ITERATION_COUNT ({})",
                authors.len(),
                MAX_ITERATION_COUNT
            );
        }
    }

    parties
}

/// Determines if a Cargo version specifier is pinned to an exact version.
///
/// A version is considered pinned if it specifies an exact version (full semver)
/// without range operators. Examples:
/// - Pinned: "1.0.0", "0.8.1"
/// - NOT pinned: "0.8" (allows patch), "^1.0.0", "~1.0.0", ">=1.0.0", "*"
fn is_cargo_version_pinned(version_str: &str) -> bool {
    let trimmed = version_str.trim();

    // Empty version is not pinned
    if trimmed.is_empty() {
        return false;
    }

    // Check for range operators that indicate unpinned versions
    if trimmed.contains('^')
        || trimmed.contains('~')
        || trimmed.contains('>')
        || trimmed.contains('<')
        || trimmed.contains('*')
        || trimmed.contains('=')
    {
        return false;
    }

    // Count dots to check if it's a full semver (major.minor.patch)
    // Pinned versions must have at least 2 dots (e.g., "1.0.0")
    // Partial versions like "0.8" or "1" are not pinned
    trimmed.matches('.').count() >= 2
}

fn extract_dependencies(toml_content: &Value, scope: &str) -> Vec<Dependency> {
    use serde_json::json;

    let mut dependencies = Vec::new();

    // Determine is_runtime based on scope
    let is_runtime = !scope.ends_with("dev-dependencies") && !scope.ends_with("build-dependencies");

    if let Some(deps_table) = toml_content.get(scope).and_then(|v| v.as_table()) {
        if deps_table.len() > MAX_ITERATION_COUNT {
            warn!(
                "Dependency table '{}' has {} entries, capping at MAX_ITERATION_COUNT ({})",
                scope,
                deps_table.len(),
                MAX_ITERATION_COUNT
            );
        }
        for (name, value) in deps_table.iter().take(MAX_ITERATION_COUNT) {
            let (extracted_requirement, is_optional, extra_data_map, is_pinned) = match value {
                Value::String(version_str) => {
                    // Simple string version: "1.0"
                    let pinned = is_cargo_version_pinned(version_str);
                    (
                        Some(version_str.to_string()),
                        false,
                        std::collections::HashMap::new(),
                        pinned,
                    )
                }
                Value::Table(table) => {
                    // Complex table format: { version = "1.0", optional = true, features = [...] }
                    let version = table
                        .get("version")
                        .and_then(|v| v.as_str())
                        .map(String::from);

                    let pinned = version.as_ref().is_some_and(|v| is_cargo_version_pinned(v));

                    let is_optional = table
                        .get("optional")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    let mut extra_data = std::collections::HashMap::new();

                    // Extract all table fields into extra_data
                    for (key, val) in table {
                        match key.as_str() {
                            "version" => {
                                // Store version in extra_data
                                if let Some(v) = val.as_str() {
                                    extra_data.insert("version".to_string(), json!(v));
                                }
                            }
                            "features" => {
                                // Extract features array
                                if let Some(features_array) = val.as_array() {
                                    let features: Vec<String> = features_array
                                        .iter()
                                        .filter_map(|f| f.as_str().map(String::from))
                                        .collect();
                                    extra_data.insert("features".to_string(), json!(features));
                                }
                            }
                            "optional" => {
                                // Skip optional flag, it's handled separately
                            }
                            _ => {
                                // Store other fields (workspace, path, git, branch, tag, rev, etc.)
                                if let Some(s) = val.as_str() {
                                    extra_data.insert(key.clone(), json!(s));
                                } else if let Some(b) = val.as_bool() {
                                    extra_data.insert(key.clone(), json!(b));
                                } else if let Some(i) = val.as_integer() {
                                    extra_data.insert(key.clone(), json!(i));
                                }
                            }
                        }
                    }

                    (version, is_optional, extra_data, pinned)
                }
                _ => {
                    // Unknown format, skip
                    continue;
                }
            };

            // Only create dependency if we have a version or it's a table with other data
            if extracted_requirement.is_some() || !extra_data_map.is_empty() {
                let purl = match PackageUrl::new(CargoParser::PACKAGE_TYPE.as_str(), name) {
                    Ok(p) => p.to_string(),
                    Err(e) => {
                        warn!(
                            "Failed to create PackageUrl for cargo dependency '{}': {}",
                            name, e
                        );
                        continue; // Skip this dependency
                    }
                };

                dependencies.push(Dependency {
                    purl: Some(purl),
                    extracted_requirement,
                    scope: Some(scope.to_string()),
                    is_runtime: Some(is_runtime),
                    is_optional: Some(is_optional),
                    is_pinned: Some(is_pinned),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: if extra_data_map.is_empty() {
                        None
                    } else {
                        Some(extra_data_map)
                    },
                });
            }
        }
    }

    dependencies
}

fn extract_dependencies_for_scopes(toml_content: &Value, scopes: &[&str]) -> Vec<Dependency> {
    scopes
        .iter()
        .flat_map(|scope| extract_dependencies(toml_content, scope))
        .collect()
}

/// Extracts keywords and categories, merging them into a single keywords array
fn extract_keywords_and_categories(toml_content: &Value) -> Vec<String> {
    let mut keywords = Vec::new();

    if let Some(package) = toml_content.get(FIELD_PACKAGE).and_then(|v| v.as_table()) {
        if let Some(kw_array) = package.get(FIELD_KEYWORDS).and_then(|v| v.as_array()) {
            if kw_array.len() > MAX_ITERATION_COUNT {
                warn!(
                    "Keywords array has {} entries, capping at MAX_ITERATION_COUNT ({})",
                    kw_array.len(),
                    MAX_ITERATION_COUNT
                );
            }
            for kw in kw_array.iter().take(MAX_ITERATION_COUNT) {
                if let Some(kw_str) = kw.as_str() {
                    keywords.push(truncate_field(kw_str.to_string()));
                }
            }
        }

        if let Some(cat_array) = package.get(FIELD_CATEGORIES).and_then(|v| v.as_array()) {
            if cat_array.len() > MAX_ITERATION_COUNT {
                warn!(
                    "Categories array has {} entries, capping at MAX_ITERATION_COUNT ({})",
                    cat_array.len(),
                    MAX_ITERATION_COUNT
                );
            }
            for cat in cat_array.iter().take(MAX_ITERATION_COUNT) {
                if let Some(cat_str) = cat.as_str() {
                    keywords.push(truncate_field(cat_str.to_string()));
                }
            }
        }
    }

    keywords
}

fn extract_file_references(toml_content: &Value) -> Vec<FileReference> {
    let mut file_references = Vec::new();

    if let Some(package) = toml_content
        .get(FIELD_PACKAGE)
        .and_then(|value| value.as_table())
    {
        for path in [
            package
                .get(FIELD_LICENSE_FILE)
                .and_then(|value| value.as_str()),
            package.get(FIELD_README).and_then(|value| value.as_str()),
        ]
        .into_iter()
        .flatten()
        {
            if file_references
                .iter()
                .any(|reference: &FileReference| reference.path == path)
            {
                continue;
            }

            file_references.push(FileReference {
                path: path.to_string(),
                size: None,
                sha1: None,
                md5: None,
                sha256: None,
                sha512: None,
                extra_data: None,
            });
        }
    }

    file_references
}

const MAX_TOML_DEPTH: usize = 50;

fn toml_to_json(value: &toml::Value, depth: usize) -> serde_json::Value {
    if depth > MAX_TOML_DEPTH {
        warn!(
            "TOML nesting depth exceeded {}, returning Null",
            MAX_TOML_DEPTH
        );
        return serde_json::Value::Null;
    }
    match value {
        toml::Value::String(s) => serde_json::json!(s),
        toml::Value::Integer(i) => serde_json::json!(i),
        toml::Value::Float(f) => serde_json::json!(f),
        toml::Value::Boolean(b) => serde_json::json!(b),
        toml::Value::Array(a) => {
            serde_json::Value::Array(a.iter().map(|v| toml_to_json(v, depth + 1)).collect())
        }
        toml::Value::Table(t) => {
            let map: serde_json::Map<String, serde_json::Value> = t
                .iter()
                .map(|(k, v)| (k.clone(), toml_to_json(v, depth + 1)))
                .collect();
            serde_json::Value::Object(map)
        }
        toml::Value::Datetime(d) => serde_json::json!(d.to_string()),
    }
}

/// Extracts extra_data fields (rust-version, edition, documentation, license-file, workspace)
fn extract_extra_data(
    toml_content: &Value,
) -> Option<std::collections::HashMap<String, serde_json::Value>> {
    use serde_json::json;
    let mut extra_data = std::collections::HashMap::new();

    if let Some(package) = toml_content.get(FIELD_PACKAGE).and_then(|v| v.as_table()) {
        if package.len() > MAX_ITERATION_COUNT {
            warn!(
                "Package table has {} entries, exceeding MAX_ITERATION_COUNT ({})",
                package.len(),
                MAX_ITERATION_COUNT
            );
        }
        if let Some(rust_version_value) = package.get(FIELD_RUST_VERSION) {
            if let Some(rust_version_str) = rust_version_value.as_str() {
                extra_data.insert("rust_version".to_string(), json!(rust_version_str));
            } else if rust_version_value
                .as_table()
                .is_some_and(|t| t.get("workspace") == Some(&toml::Value::Boolean(true)))
            {
                extra_data.insert("rust-version".to_string(), json!("workspace"));
            }
        }

        // Extract edition (or detect workspace inheritance)
        if let Some(edition_value) = package.get(FIELD_EDITION) {
            if let Some(edition_str) = edition_value.as_str() {
                extra_data.insert("rust_edition".to_string(), json!(edition_str));
            } else if edition_value
                .as_table()
                .is_some_and(|t| t.get("workspace") == Some(&toml::Value::Boolean(true)))
            {
                extra_data.insert("edition".to_string(), json!("workspace"));
            }
        }

        // Extract documentation URL
        if let Some(documentation) = package.get("documentation").and_then(|v| v.as_str()) {
            extra_data.insert("documentation_url".to_string(), json!(documentation));
        }

        // Extract license-file path
        if let Some(license_file) = package.get(FIELD_LICENSE_FILE).and_then(|v| v.as_str()) {
            extra_data.insert("license_file".to_string(), json!(license_file));
        }

        if let Some(readme_value) = package.get(FIELD_README) {
            if let Some(readme_file) = readme_value.as_str() {
                extra_data.insert("readme_file".to_string(), json!(readme_file));
            } else if let Some(readme_enabled) = readme_value.as_bool() {
                extra_data.insert("readme".to_string(), json!(readme_enabled));
            } else if readme_value
                .as_table()
                .is_some_and(|t| t.get("workspace") == Some(&toml::Value::Boolean(true)))
            {
                extra_data.insert("readme".to_string(), json!("workspace"));
            }
        }

        if let Some(publish_value) = package.get(FIELD_PUBLISH) {
            extra_data.insert("publish".to_string(), toml_to_json(publish_value, 0));
        }

        // Check for workspace inheritance markers for other fields
        // version
        if let Some(version_value) = package.get(FIELD_VERSION)
            && version_value
                .as_table()
                .is_some_and(|t| t.get("workspace") == Some(&toml::Value::Boolean(true)))
        {
            extra_data.insert("version".to_string(), json!("workspace"));
        }

        // license
        if let Some(license_value) = package.get(FIELD_LICENSE)
            && license_value
                .as_table()
                .is_some_and(|t| t.get("workspace") == Some(&toml::Value::Boolean(true)))
        {
            extra_data.insert("license".to_string(), json!("workspace"));
        }

        // homepage
        if let Some(homepage_value) = package.get(FIELD_HOMEPAGE)
            && homepage_value
                .as_table()
                .is_some_and(|t| t.get("workspace") == Some(&toml::Value::Boolean(true)))
        {
            extra_data.insert("homepage".to_string(), json!("workspace"));
        }

        // repository
        if let Some(repository_value) = package.get(FIELD_REPOSITORY)
            && repository_value
                .as_table()
                .is_some_and(|t| t.get("workspace") == Some(&toml::Value::Boolean(true)))
        {
            extra_data.insert("repository".to_string(), json!("workspace"));
        }

        // categories
        if let Some(categories_value) = package.get(FIELD_CATEGORIES)
            && categories_value
                .as_table()
                .is_some_and(|t| t.get("workspace") == Some(&toml::Value::Boolean(true)))
        {
            extra_data.insert("categories".to_string(), json!("workspace"));
        }

        // authors
        if let Some(authors_value) = package.get(FIELD_AUTHORS)
            && authors_value
                .as_table()
                .is_some_and(|t| t.get("workspace") == Some(&toml::Value::Boolean(true)))
        {
            extra_data.insert("authors".to_string(), json!("workspace"));
        }
    }

    // Extract workspace table if it exists
    if let Some(workspace_value) = toml_content.get("workspace") {
        extra_data.insert("workspace".to_string(), toml_to_json(workspace_value, 0));
    }

    if extra_data.is_empty() {
        None
    } else {
        Some(extra_data)
    }
}

crate::register_parser!(
    "Rust Cargo.toml manifest",
    &["**/Cargo.toml", "**/cargo.toml"],
    "cargo",
    "Rust",
    Some("https://doc.rust-lang.org/cargo/reference/manifest.html"),
);
