//! Parser for Poetry poetry.lock lockfiles.
//!
//! Extracts resolved dependency information from Poetry lockfiles which use TOML format
//! to store resolved versions and metadata for Python dependencies.
//!
//! # Supported Formats
//! - poetry.lock (TOML-based lockfile with package metadata)
//!
//! # Key Features
//! - Direct vs transitive dependency tracking via `is_direct` flag
//! - Dependency groups support (main, dev, etc.) via scope field
//! - Dependency resolution with exact versions
//! - Package URL (purl) generation for PyPI packages
//! - Extra dependencies and optional package handling
//!
//! # Implementation Notes
//! - Uses TOML parsing via `toml` crate
//! - All lockfile versions are pinned (`is_pinned: Some(true)`)
//! - Graceful error handling with `warn!()` logs
//! - Integrates with Python parser utilities for PyPI URL building

use std::collections::HashMap;
use std::path::Path;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use toml::Value as TomlValue;
use toml::map::Map as TomlMap;

use crate::models::{
    DatasourceId, Dependency, PackageData, PackageType, ResolvedPackage, Sha256Digest,
};
use crate::parsers::python::{build_pypi_urls, read_toml_file};

use super::PackageParser;

const FIELD_PACKAGE: &str = "package";
const FIELD_METADATA: &str = "metadata";
const FIELD_NAME: &str = "name";
const FIELD_VERSION: &str = "version";
const FIELD_PYTHON_VERSIONS: &str = "python-versions";
const FIELD_DEPENDENCIES: &str = "dependencies";
const FIELD_EXTRAS: &str = "extras";
const FIELD_LOCK_VERSION: &str = "lock-version";

/// Poetry lockfile parser for poetry.lock files.
///
/// Extracts pinned Python package dependencies from Poetry-managed projects.
pub struct PoetryLockParser;

impl PackageParser for PoetryLockParser {
    const PACKAGE_TYPE: PackageType = PackageType::Pypi;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| name == "poetry.lock")
            .unwrap_or(false)
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let toml_content = match read_toml_file(path) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read poetry.lock at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        vec![parse_poetry_lock(&toml_content)]
    }
}

fn parse_poetry_lock(toml_content: &TomlValue) -> PackageData {
    let packages = toml_content
        .get(FIELD_PACKAGE)
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();

    let metadata = toml_content
        .get(FIELD_METADATA)
        .and_then(|value| value.as_table());

    let mut dependencies = Vec::new();
    for package in packages {
        if let Some(package_table) = package.as_table()
            && let Some(dependency) = build_dependency_from_package(package_table)
        {
            dependencies.push(dependency);
        }
    }

    PackageData {
        package_type: Some(PoetryLockParser::PACKAGE_TYPE),
        namespace: None,
        name: None,
        version: None,
        qualifiers: None,
        subpath: None,
        primary_language: Some("Python".to_string()),
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
        vcs_url: None,
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
        extra_data: build_metadata_extra_data(metadata),
        dependencies,
        repository_homepage_url: None,
        repository_download_url: None,
        api_data_url: None,
        datasource_id: Some(DatasourceId::PypiPoetryLock),
        purl: None,
    }
}

fn build_metadata_extra_data(
    metadata: Option<&TomlMap<String, TomlValue>>,
) -> Option<HashMap<String, serde_json::Value>> {
    let mut extra_data = HashMap::new();

    if let Some(metadata) = metadata {
        if let Some(python_versions) = metadata
            .get(FIELD_PYTHON_VERSIONS)
            .and_then(|value| value.as_str())
            && !python_versions.is_empty()
        {
            extra_data.insert(
                "python_version".to_string(),
                serde_json::Value::String(python_versions.to_string()),
            );
        }

        if let Some(lock_version) = metadata.get(FIELD_LOCK_VERSION) {
            let lock_version = lock_version
                .as_str()
                .map(|value| value.to_string())
                .or_else(|| lock_version.as_integer().map(|value| value.to_string()));

            if let Some(lock_version) = lock_version
                && !lock_version.is_empty()
            {
                extra_data.insert(
                    "lock_version".to_string(),
                    serde_json::Value::String(lock_version),
                );
            }
        }
    }

    if extra_data.is_empty() {
        None
    } else {
        Some(extra_data)
    }
}

fn build_dependency_from_package(package_table: &TomlMap<String, TomlValue>) -> Option<Dependency> {
    let name = package_table
        .get(FIELD_NAME)
        .and_then(|value| value.as_str())
        .map(normalize_pypi_name)?;

    let version = package_table
        .get(FIELD_VERSION)
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())?;

    let purl = create_pypi_purl(&name, Some(&version));

    let resolved_package = build_resolved_package(package_table, &name, &version);

    let is_optional = package_table
        .get("optional")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    Some(Dependency {
        purl,
        extracted_requirement: None,
        scope: None,
        is_runtime: Some(!is_optional),
        is_optional: Some(is_optional),
        is_pinned: Some(true),
        is_direct: Some(false),
        resolved_package: Some(Box::new(resolved_package)),
        extra_data: None,
    })
}

fn build_resolved_package(
    package_table: &TomlMap<String, TomlValue>,
    name: &str,
    version: &str,
) -> ResolvedPackage {
    let dependencies = extract_package_dependencies(package_table);

    let (repository_homepage_url, repository_download_url, api_data_url, purl) =
        build_pypi_urls(Some(name), Some(version));

    // Extract sha256 hash from files array (first file's hash)
    let sha256 = extract_sha256_from_files(package_table);

    ResolvedPackage {
        primary_language: Some("Python".to_string()),
        download_url: None,
        sha1: None,
        sha256: sha256.and_then(|h| Sha256Digest::from_hex(&h).ok()),
        sha512: None,
        md5: None,
        is_virtual: true,
        extra_data: None,
        dependencies,
        repository_homepage_url,
        repository_download_url,
        api_data_url,
        datasource_id: Some(DatasourceId::PypiPoetryLock),
        purl,
        ..ResolvedPackage::new(
            PoetryLockParser::PACKAGE_TYPE,
            String::new(),
            name.to_string(),
            version.to_string(),
        )
    }
}

fn extract_package_dependencies(package_table: &TomlMap<String, TomlValue>) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    if let Some(dep_table) = package_table
        .get(FIELD_DEPENDENCIES)
        .and_then(|value| value.as_table())
    {
        for (dep_name, dep_value) in dep_table {
            if let Some(dependency) = build_dependency_from_table(dep_name, dep_value) {
                dependencies.push(dependency);
            }
        }
    }

    if let Some(extras_table) = package_table
        .get(FIELD_EXTRAS)
        .and_then(|value| value.as_table())
    {
        for (extra_name, extra_values) in extras_table {
            if let Some(extra_list) = extra_values.as_array() {
                for extra in extra_list {
                    if let Some(spec) = extra.as_str()
                        && let Some(dependency) = build_dependency_from_extra(extra_name, spec)
                    {
                        dependencies.push(dependency);
                    }
                }
            }
        }
    }

    dependencies
}

fn build_dependency_from_table(dep_name: &str, dep_value: &TomlValue) -> Option<Dependency> {
    let requirement = match dep_value {
        TomlValue::String(value) => Some(value.to_string()),
        TomlValue::Table(table) => table
            .get(FIELD_VERSION)
            .and_then(|value| value.as_str())
            .map(|value| value.to_string()),
        _ => None,
    };

    let normalized_name = normalize_pypi_name(dep_name);
    let purl = create_pypi_purl(&normalized_name, None);

    Some(Dependency {
        purl,
        extracted_requirement: requirement,
        scope: Some(FIELD_DEPENDENCIES.to_string()),
        is_runtime: Some(true),
        is_optional: Some(false),
        is_pinned: Some(false),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: None,
    })
}

fn build_dependency_from_extra(extra_name: &str, spec: &str) -> Option<Dependency> {
    let (name, requirement) = parse_poetry_dependency_spec(spec)?;
    let purl = create_pypi_purl(&name, None);

    Some(Dependency {
        purl,
        extracted_requirement: requirement,
        scope: Some(extra_name.to_string()),
        is_runtime: Some(false),
        is_optional: Some(false),
        is_pinned: Some(false),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: None,
    })
}

fn parse_poetry_dependency_spec(spec: &str) -> Option<(String, Option<String>)> {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(paren_pos) = trimmed.find(" (") {
        let name_part = trimmed[..paren_pos].trim();
        let requirement_part = trimmed[paren_pos + 2..].trim();
        let requirement = requirement_part.trim_end_matches(')').trim();
        if name_part.is_empty() {
            return None;
        }
        let normalized_name = normalize_pypi_name(name_part);
        let requirement = if requirement.is_empty() {
            None
        } else {
            Some(requirement.to_string())
        };
        return Some((normalized_name, requirement));
    }

    Some((normalize_pypi_name(trimmed), None))
}

fn normalize_pypi_name(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

fn create_pypi_purl(name: &str, version: Option<&str>) -> Option<String> {
    if name.contains('[') || name.contains(']') {
        return Some(build_manual_pypi_purl(name, version));
    }

    if let Ok(mut purl) = PackageUrl::new(PoetryLockParser::PACKAGE_TYPE.as_str(), name) {
        if let Some(version) = version
            && purl.with_version(version).is_err()
        {
            return None;
        }
        return Some(purl.to_string());
    }

    Some(build_manual_pypi_purl(name, version))
}

fn build_manual_pypi_purl(name: &str, version: Option<&str>) -> String {
    let encoded_name = encode_pypi_name(name);
    let mut purl = format!("pkg:pypi/{}", encoded_name);
    if let Some(version) = version
        && !version.is_empty()
    {
        purl.push('@');
        purl.push_str(version);
    }
    purl
}

fn encode_pypi_name(name: &str) -> String {
    name.replace('[', "%5b").replace(']', "%5d")
}

fn extract_sha256_from_files(package_table: &TomlMap<String, TomlValue>) -> Option<String> {
    package_table
        .get("files")
        .and_then(|files| files.as_array())
        .and_then(|files_array| files_array.first())
        .and_then(|first_file| first_file.as_table())
        .and_then(|file_table| file_table.get("hash"))
        .and_then(|hash_value| hash_value.as_str())
        .and_then(|hash_str| hash_str.strip_prefix("sha256:").map(|s| s.to_string()))
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(PoetryLockParser::PACKAGE_TYPE),
        primary_language: Some("Python".to_string()),
        datasource_id: Some(DatasourceId::PypiPoetryLock),
        ..Default::default()
    }
}

crate::register_parser!(
    "Poetry lockfile",
    &["**/poetry.lock"],
    "pypi",
    "Python",
    Some("https://python-poetry.org/docs/basic-usage/#installing-with-poetrylock"),
);
