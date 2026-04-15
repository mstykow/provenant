//! Parser for Julia Project.toml and Manifest.toml files.
//!
//! Extracts package metadata, dependencies, and license information from
//! Julia package manager (Pkg.jl) manifest files.
//!
//! # Supported Formats
//! - Project.toml (package metadata)
//! - Manifest.toml (resolved dependency tree)
//!
//! # Key Features
//! - Dependency extraction with UUID tracking
//! - `is_pinned` analysis based on Manifest.toml resolved versions
//! - Package URL (purl) generation
//! - Compat section version constraint extraction
//!
//! # Implementation Notes
//! - Uses toml crate for parsing
//! - Julia packages are identified by UUID
//! - Project.toml `[deps]` lists direct dependencies by name → UUID
//! - Manifest.toml `[[deps]]` entries contain resolved version + tree SHA

use crate::models::{DatasourceId, Dependency, PackageData, PackageType, Party};
use crate::parser_warn as warn;
use crate::parsers::utils::{
    MAX_ITERATION_COUNT, RecursionGuard, read_file_to_string, truncate_field,
};
use packageurl::PackageUrl;
use std::path::Path;
use toml::Value;

use super::PackageParser;
use super::license_normalization::{
    DeclaredLicenseMatchMetadata, build_declared_license_data, empty_declared_license_data,
    normalize_spdx_expression,
};

const FIELD_NAME: &str = "name";
const FIELD_UUID: &str = "uuid";
const FIELD_VERSION: &str = "version";
const FIELD_LICENSE: &str = "license";
const FIELD_AUTHORS: &str = "authors";
const FIELD_REPOSITORY: &str = "repository";
const FIELD_DEPS: &str = "deps";
const FIELD_COMPAT: &str = "compat";
const FIELD_TARGETS: &str = "targets";
const FIELD_HOMEPAGE: &str = "homepage";

pub struct JuliaProjectTomlParser;

impl PackageParser for JuliaProjectTomlParser {
    const PACKAGE_TYPE: PackageType = PackageType::Julia;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let toml_content = match read_julia_toml(path) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read or parse Project.toml at {:?}: {}", path, e);
                return vec![default_project_package_data()];
            }
        };

        let name = toml_content
            .get(FIELD_NAME)
            .and_then(|v| v.as_str())
            .map(|s| truncate_field(s.to_string()));

        let _uuid = toml_content
            .get(FIELD_UUID)
            .and_then(|v| v.as_str())
            .map(String::from);

        let version = toml_content
            .get(FIELD_VERSION)
            .and_then(|v| v.as_str())
            .map(|s| truncate_field(s.to_string()));

        let raw_license = toml_content
            .get(FIELD_LICENSE)
            .and_then(|v| v.as_str())
            .map(|s| truncate_field(s.to_string()));

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

        let extracted_license_statement = raw_license.clone().map(truncate_field);

        let dependencies = extract_project_dependencies(&toml_content);

        let purl = create_package_url(&name, &version);

        let repository_url = toml_content
            .get(FIELD_REPOSITORY)
            .and_then(|v| v.as_str())
            .map(|s| truncate_field(s.to_string()));

        let homepage_url = toml_content
            .get(FIELD_HOMEPAGE)
            .and_then(|v| v.as_str())
            .map(|s| truncate_field(s.to_string()));

        let description = None;

        let extra_data = extract_project_extra_data(&toml_content);

        let is_private = false;

        vec![PackageData {
            package_type: Some(Self::PACKAGE_TYPE),
            namespace: None,
            name,
            version,
            qualifiers: None,
            subpath: None,
            primary_language: Some("Julia".to_string()),
            description,
            release_date: None,
            parties: extract_parties(&toml_content),
            keywords: Vec::new(),
            homepage_url,
            download_url: None,
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
            file_references: Vec::new(),
            is_private,
            is_virtual: false,
            extra_data,
            dependencies,
            repository_homepage_url: None,
            repository_download_url: None,
            api_data_url: None,
            datasource_id: Some(DatasourceId::JuliaProjectToml),
            purl,
        }]
    }

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("Project.toml"))
    }
}

pub struct JuliaManifestTomlParser;

impl PackageParser for JuliaManifestTomlParser {
    const PACKAGE_TYPE: PackageType = PackageType::Julia;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let toml_content = match read_julia_toml(path) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read or parse Manifest.toml at {:?}: {}", path, e);
                return vec![];
            }
        };

        extract_manifest_packages(&toml_content)
    }

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("Manifest.toml"))
    }
}

fn read_julia_toml(path: &Path) -> Result<Value, String> {
    let content =
        read_file_to_string(path, None).map_err(|e| format!("Failed to read file: {}", e))?;
    toml::from_str(&content).map_err(|e| format!("Failed to parse TOML: {}", e))
}

fn create_package_url(name: &Option<String>, version: &Option<String>) -> Option<String> {
    name.as_ref().and_then(|name| {
        let mut package_url = match PackageUrl::new(PackageType::Julia.as_str(), name) {
            Ok(p) => p,
            Err(e) => {
                warn!(
                    "Failed to create PackageUrl for julia package '{}': {}",
                    name, e
                );
                return None;
            }
        };

        if let Some(v) = version
            && let Err(e) = package_url.with_version(v)
        {
            warn!(
                "Failed to set version '{}' for julia package '{}': {}",
                v, name, e
            );
            return None;
        }

        Some(truncate_field(package_url.to_string()))
    })
}

fn extract_parties(toml_content: &Value) -> Vec<Party> {
    let mut parties = Vec::new();

    if let Some(authors) = toml_content.get(FIELD_AUTHORS).and_then(|v| v.as_array()) {
        for author in authors.iter().take(MAX_ITERATION_COUNT) {
            if let Some(author_str) = author.as_str() {
                parties.push(Party {
                    r#type: None,
                    role: Some("author".to_string()),
                    name: Some(truncate_field(author_str.trim().to_string())),
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

fn extract_project_dependencies(toml_content: &Value) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    let deps_table = match toml_content.get(FIELD_DEPS).and_then(|v| v.as_table()) {
        Some(table) => table,
        None => return dependencies,
    };

    let compat_table = toml_content.get(FIELD_COMPAT).and_then(|v| v.as_table());

    for (dep_name, dep_value) in deps_table.iter().take(MAX_ITERATION_COUNT) {
        let uuid = dep_value.as_str().map(String::from);

        let extracted_requirement = compat_table
            .and_then(|ct| ct.get(dep_name))
            .and_then(|v| v.as_str())
            .map(|s| truncate_field(s.to_string()));

        let is_pinned = extracted_requirement
            .as_deref()
            .is_some_and(is_julia_version_pinned);

        let purl = match PackageUrl::new(PackageType::Julia.as_str(), dep_name) {
            Ok(p) => truncate_field(p.to_string()),
            Err(e) => {
                warn!(
                    "Failed to create PackageUrl for julia dependency '{}': {}",
                    dep_name, e
                );
                continue;
            }
        };

        let mut extra_data_map = std::collections::HashMap::new();
        if let Some(ref uuid_val) = uuid {
            extra_data_map.insert("uuid".to_string(), serde_json::json!(uuid_val));
        }

        dependencies.push(Dependency {
            purl: Some(purl),
            extracted_requirement,
            scope: Some("dependencies".to_string()),
            is_runtime: Some(true),
            is_optional: None,
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

    dependencies
}

fn extract_manifest_packages(toml_content: &Value) -> Vec<PackageData> {
    let mut packages = Vec::new();

    let deps_table = match toml_content.get(FIELD_DEPS).and_then(|v| v.as_table()) {
        Some(table) => table,
        None => return packages,
    };

    for (dep_name, dep_value) in deps_table.iter().take(MAX_ITERATION_COUNT) {
        let dep_entries = match dep_value.as_array() {
            Some(entries) => entries,
            None => continue,
        };

        for dep_entry in dep_entries.iter().take(MAX_ITERATION_COUNT) {
            let name = Some(truncate_field(dep_name.clone()));

            let uuid = dep_entry
                .get(FIELD_UUID)
                .and_then(|v| v.as_str())
                .map(String::from);

            let version = dep_entry
                .get(FIELD_VERSION)
                .and_then(|v| v.as_str())
                .map(|s| truncate_field(s.to_string()));

            let purl = create_package_url(&name, &version);

            let tree_hash = dep_entry
                .get("git-tree-sha1")
                .and_then(|v| v.as_str())
                .map(String::from);

            let source_url = dep_entry
                .get("url")
                .and_then(|v| v.as_str())
                .map(|s| truncate_field(s.to_string()));

            let mut extra_data_map = std::collections::HashMap::new();
            if let Some(ref uuid_val) = uuid {
                extra_data_map.insert("uuid".to_string(), serde_json::json!(uuid_val));
            }
            if let Some(ref tree_hash_val) = tree_hash {
                extra_data_map.insert("tree_hash".to_string(), serde_json::json!(tree_hash_val));
            }
            if let Some(ref source_url_val) = source_url {
                extra_data_map.insert("url".to_string(), serde_json::json!(source_url_val));
            }

            packages.push(PackageData {
                package_type: Some(PackageType::Julia),
                namespace: None,
                name,
                version,
                qualifiers: None,
                subpath: None,
                primary_language: Some("Julia".to_string()),
                description: None,
                release_date: None,
                parties: Vec::new(),
                keywords: Vec::new(),
                homepage_url: None,
                download_url: None,
                size: None,
                sha1: None,
                md5: None,
                sha256: None,
                sha512: None,
                bug_tracking_url: None,
                code_view_url: None,
                vcs_url: source_url,
                copyright: None,
                holder: None,
                declared_license_expression: None,
                declared_license_expression_spdx: None,
                license_detections: Vec::new(),
                other_license_expression: None,
                other_license_expression_spdx: None,
                other_license_detections: Vec::new(),
                extracted_license_statement: None,
                notice_text: None,
                source_packages: Vec::new(),
                file_references: Vec::new(),
                is_private: false,
                is_virtual: false,
                extra_data: if extra_data_map.is_empty() {
                    None
                } else {
                    Some(extra_data_map)
                },
                dependencies: Vec::new(),
                repository_homepage_url: None,
                repository_download_url: None,
                api_data_url: None,
                datasource_id: Some(DatasourceId::JuliaManifestToml),
                purl,
            });
        }
    }

    packages
}

fn extract_project_extra_data(
    toml_content: &Value,
) -> Option<std::collections::HashMap<String, serde_json::Value>> {
    use serde_json::json;
    let mut extra_data = std::collections::HashMap::new();

    if let Some(uuid) = toml_content.get(FIELD_UUID).and_then(|v| v.as_str()) {
        extra_data.insert("uuid".to_string(), json!(uuid));
    }

    if let Some(targets) = toml_content.get(FIELD_TARGETS) {
        extra_data.insert("targets".to_string(), toml_to_json(targets));
    }

    if let Some(compat) = toml_content.get(FIELD_COMPAT) {
        extra_data.insert("compat".to_string(), toml_to_json(compat));
    }

    if let Some(deps) = toml_content.get(FIELD_DEPS) {
        extra_data.insert("deps".to_string(), toml_to_json(deps));
    }

    if let Some(extras) = toml_content.get("extras") {
        extra_data.insert("extras".to_string(), toml_to_json(extras));
    }

    if let Some(sources) = toml_content.get("sources") {
        extra_data.insert("sources".to_string(), toml_to_json(sources));
    }

    if extra_data.is_empty() {
        None
    } else {
        Some(extra_data)
    }
}

fn toml_to_json(value: &toml::Value) -> serde_json::Value {
    toml_to_json_inner(value, &mut RecursionGuard::depth_only())
}

fn toml_to_json_inner(value: &toml::Value, guard: &mut RecursionGuard<()>) -> serde_json::Value {
    if guard.descend() {
        warn!("Recursion depth exceeded in toml_to_json, returning Null");
        return serde_json::Value::Null;
    }

    let result = match value {
        toml::Value::String(s) => serde_json::json!(s),
        toml::Value::Integer(i) => serde_json::json!(i),
        toml::Value::Float(f) => serde_json::json!(f),
        toml::Value::Boolean(b) => serde_json::json!(b),
        toml::Value::Array(a) => {
            serde_json::Value::Array(a.iter().map(|v| toml_to_json_inner(v, guard)).collect())
        }
        toml::Value::Table(t) => {
            let map: serde_json::Map<String, serde_json::Value> = t
                .iter()
                .map(|(k, v)| (k.clone(), toml_to_json_inner(v, guard)))
                .collect();
            serde_json::Value::Object(map)
        }
        toml::Value::Datetime(d) => serde_json::json!(d.to_string()),
    };
    guard.ascend();
    result
}

fn default_project_package_data() -> PackageData {
    PackageData {
        package_type: Some(PackageType::Julia),
        datasource_id: Some(DatasourceId::JuliaProjectToml),
        ..Default::default()
    }
}

fn is_julia_version_pinned(version_str: &str) -> bool {
    let trimmed = version_str.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.contains('^')
        || trimmed.contains('~')
        || trimmed.contains('>')
        || trimmed.contains('<')
        || trimmed.contains('*')
    {
        return false;
    }
    trimmed.matches('.').count() >= 2
}

crate::register_parser!(
    "Julia Project.toml manifest",
    &["**/Project.toml"],
    "julia",
    "Julia",
    Some("https://pkgdocs.julialang.org/v1/toml-files/"),
);

crate::register_parser!(
    "Julia Manifest.toml resolved dependencies",
    &["**/Manifest.toml"],
    "julia",
    "Julia",
    Some("https://pkgdocs.julialang.org/v1/toml-files/"),
);
