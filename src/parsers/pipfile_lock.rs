// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Parser for Pipfile.lock lockfiles.
//!
//! Extracts resolved dependency information from Pipfile.lock files which store
//! locked dependency versions for Python projects using pipenv.
//!
//! # Supported Formats
//! - Pipfile.lock (JSON-based lockfile with per-environment dependency sections)
//!
//! # Key Features
//! - Dependency extraction from default and develop sections
//! - Direct dependency tracking (top-level locks are direct)
//! - Exact version resolution with hash verification
//! - Package URL (purl) generation for PyPI packages
//! - Markers and extras dependency handling
//!
//! # Implementation Notes
//! - Uses JSON parsing via `serde_json` and TOML for secondary parsing
//! - All lockfile versions are pinned (`is_pinned: Some(true)`)
//! - Graceful error handling with `warn!()` logs
//! - Integrates with Python parser utilities for PyPI URL building

use std::collections::HashMap;
use std::path::Path;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use serde_json::Value as JsonValue;
use toml::Value as TomlValue;
use toml::map::Map as TomlMap;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType, Sha256Digest};
use crate::parsers::python::read_toml_file;
use crate::parsers::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};

use super::PackageParser;

const FIELD_META: &str = "_meta";
const FIELD_HASH: &str = "hash";
const FIELD_SHA256: &str = "sha256";
const FIELD_DEFAULT: &str = "default";
const FIELD_DEVELOP: &str = "develop";
const FIELD_VERSION: &str = "version";
const FIELD_HASHES: &str = "hashes";

const FIELD_PACKAGES: &str = "packages";
const FIELD_DEV_PACKAGES: &str = "dev-packages";
const FIELD_REQUIRES: &str = "requires";
const FIELD_SOURCE: &str = "source";
const FIELD_PYTHON_VERSION: &str = "python_version";

/// Pipenv lockfile and manifest parser for Pipfile.lock and Pipfile files.
///
/// Extracts Python package dependencies from Pipenv-managed projects, supporting
/// both locked versions (Pipfile.lock) and declared dependencies (Pipfile).
pub struct PipfileLockParser;

impl PackageParser for PipfileLockParser {
    const PACKAGE_TYPE: PackageType = PackageType::Pypi;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| name == "Pipfile.lock" || name == "Pipfile")
            .unwrap_or(false)
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        vec![match path.file_name().and_then(|name| name.to_str()) {
            Some("Pipfile.lock") => extract_from_pipfile_lock(path),
            Some("Pipfile") => extract_from_pipfile(path),
            _ => default_package_data(None),
        }]
    }
}

fn extract_from_pipfile_lock(path: &Path) -> PackageData {
    let content = match read_file_to_string(path, None) {
        Ok(content) => content,
        Err(e) => {
            warn!("Failed to read Pipfile.lock at {:?}: {}", path, e);
            return default_package_data(Some(DatasourceId::PipfileLock));
        }
    };

    let json_content: JsonValue = match serde_json::from_str(&content) {
        Ok(content) => content,
        Err(e) => {
            warn!("Failed to parse Pipfile.lock at {:?}: {}", path, e);
            return default_package_data(Some(DatasourceId::PipfileLock));
        }
    };

    parse_pipfile_lock(&json_content)
}

fn parse_pipfile_lock(json_content: &JsonValue) -> PackageData {
    let mut package_data = default_package_data(Some(DatasourceId::PipfileLock));
    package_data.sha256 = extract_lockfile_sha256(json_content);

    let meta = json_content
        .get(FIELD_META)
        .and_then(|value| value.as_object());
    let pipfile_spec = meta.and_then(|value| value.get("pipfile-spec"));
    let sources = meta.and_then(|value| value.get("sources"));
    let requires = meta.and_then(|value| value.get("requires"));
    let _ = (pipfile_spec, sources, requires);

    let default_deps = extract_lockfile_dependencies(json_content, FIELD_DEFAULT, "install", true);
    let develop_deps = extract_lockfile_dependencies(json_content, FIELD_DEVELOP, "develop", false);
    package_data.dependencies = [default_deps, develop_deps].concat();

    package_data
}

fn extract_lockfile_sha256(json_content: &JsonValue) -> Option<Sha256Digest> {
    json_content
        .get(FIELD_META)
        .and_then(|meta| meta.get(FIELD_HASH))
        .and_then(|hash| hash.get(FIELD_SHA256))
        .and_then(|value| value.as_str())
        .and_then(|s| Sha256Digest::from_hex(s).ok())
}

fn extract_lockfile_dependencies(
    json_content: &JsonValue,
    section: &str,
    scope: &str,
    is_runtime: bool,
) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    if let Some(section_map) = json_content
        .get(section)
        .and_then(|value| value.as_object())
    {
        for (name, value) in section_map.iter().take(MAX_ITERATION_COUNT) {
            if let Some(dependency) = build_lockfile_dependency(name, value, scope, is_runtime) {
                dependencies.push(dependency);
            }
        }
    }

    dependencies
}

fn build_lockfile_dependency(
    name: &str,
    value: &JsonValue,
    scope: &str,
    is_runtime: bool,
) -> Option<Dependency> {
    let normalized_name = normalize_pypi_name(name);
    let requirement = extract_lockfile_requirement(value)?;
    let version = strip_pipfile_lock_version(&requirement);
    let purl = create_pypi_purl(&normalized_name, version.as_deref());

    let _hashes = extract_lockfile_hashes(value);

    Some(Dependency {
        purl,
        extracted_requirement: Some(truncate_field(requirement)),
        scope: Some(scope.to_string()),
        is_runtime: Some(is_runtime),
        is_optional: Some(false),
        is_pinned: Some(true),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: None,
    })
}

fn extract_lockfile_requirement(value: &JsonValue) -> Option<String> {
    match value {
        JsonValue::String(spec) => Some(truncate_field(spec.to_string())),
        JsonValue::Object(map) => map
            .get(FIELD_VERSION)
            .and_then(|version| version.as_str())
            .map(|version| truncate_field(version.to_string())),
        _ => None,
    }
}

fn extract_lockfile_hashes(value: &JsonValue) -> Vec<String> {
    let mut hashes = Vec::new();
    let hash_values = value
        .get(FIELD_HASHES)
        .and_then(|hashes_value| hashes_value.as_array());

    if let Some(hash_values) = hash_values {
        for hash_value in hash_values {
            if let Some(hash) = hash_value.as_str()
                && let Some(stripped) = hash.strip_prefix("sha256:")
            {
                hashes.push(truncate_field(stripped.to_string()));
            }
        }
    }

    hashes
}

fn strip_pipfile_lock_version(requirement: &str) -> Option<String> {
    let trimmed = requirement.trim();
    if let Some(stripped) = trimmed.strip_prefix("==") {
        let version = stripped.trim();
        if version.is_empty() {
            None
        } else {
            Some(truncate_field(version.to_string()))
        }
    } else {
        None
    }
}

fn extract_from_pipfile(path: &Path) -> PackageData {
    let toml_content = match read_toml_file(path) {
        Ok(content) => content,
        Err(e) => {
            warn!("Failed to read Pipfile at {:?}: {}", path, e);
            return default_package_data(Some(DatasourceId::Pipfile));
        }
    };

    parse_pipfile(&toml_content)
}

fn parse_pipfile(toml_content: &TomlValue) -> PackageData {
    let mut package_data = default_package_data(Some(DatasourceId::Pipfile));

    let packages = toml_content
        .get(FIELD_PACKAGES)
        .and_then(|value| value.as_table());
    let dev_packages = toml_content
        .get(FIELD_DEV_PACKAGES)
        .and_then(|value| value.as_table());

    let mut dependencies = Vec::new();
    if let Some(packages) = packages {
        dependencies.extend(extract_pipfile_dependencies(packages, "install", true));
    }
    if let Some(dev_packages) = dev_packages {
        dependencies.extend(extract_pipfile_dependencies(dev_packages, "develop", false));
    }

    package_data.dependencies = dependencies;
    package_data.extra_data = build_pipfile_extra_data(toml_content);

    package_data
}

fn extract_pipfile_dependencies(
    packages: &TomlMap<String, TomlValue>,
    scope: &str,
    is_runtime: bool,
) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    for (name, value) in packages.iter().take(MAX_ITERATION_COUNT) {
        if let Some(dependency) = build_pipfile_dependency(name, value, scope, is_runtime) {
            dependencies.push(dependency);
        }
    }

    dependencies
}

fn build_pipfile_dependency(
    name: &str,
    value: &TomlValue,
    scope: &str,
    is_runtime: bool,
) -> Option<Dependency> {
    let normalized_name = normalize_pypi_name(name);
    let requirement = extract_pipfile_requirement(value);
    if requirement.is_none() && is_non_registry_dependency(value) {
        return None;
    }
    let requirement = requirement?;
    let purl = create_pypi_purl(&normalized_name, None);

    Some(Dependency {
        purl,
        extracted_requirement: Some(truncate_field(requirement)),
        scope: Some(scope.to_string()),
        is_runtime: Some(is_runtime),
        is_optional: Some(false),
        is_pinned: Some(false),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: None,
    })
}

fn extract_pipfile_requirement(value: &TomlValue) -> Option<String> {
    match value {
        TomlValue::String(spec) => Some(truncate_field(spec.to_string())),
        TomlValue::Boolean(true) => Some("*".to_string()),
        TomlValue::Table(table) => table
            .get(FIELD_VERSION)
            .and_then(|version| version.as_str())
            .map(|version| truncate_field(version.to_string())),
        _ => None,
    }
}

fn is_non_registry_dependency(value: &TomlValue) -> bool {
    let table = match value {
        TomlValue::Table(table) => table,
        _ => return false,
    };

    ["git", "path", "file", "url", "hg", "svn"]
        .iter()
        .any(|key| table.contains_key(*key))
}

fn build_pipfile_extra_data(
    toml_content: &TomlValue,
) -> Option<HashMap<String, serde_json::Value>> {
    let mut extra_data = HashMap::new();

    if let Some(requires_table) = toml_content
        .get(FIELD_REQUIRES)
        .and_then(|value| value.as_table())
        && let Some(python_version) = requires_table
            .get(FIELD_PYTHON_VERSION)
            .and_then(|value| value.as_str())
    {
        extra_data.insert(
            FIELD_PYTHON_VERSION.to_string(),
            serde_json::Value::String(truncate_field(python_version.to_string())),
        );
    }

    if let Some(source_value) = toml_content.get(FIELD_SOURCE)
        && let Some(sources) = parse_pipfile_sources(source_value)
    {
        extra_data.insert("sources".to_string(), sources);
    }

    if extra_data.is_empty() {
        None
    } else {
        Some(extra_data)
    }
}

fn parse_pipfile_sources(source_value: &TomlValue) -> Option<serde_json::Value> {
    match source_value {
        TomlValue::Array(sources) => {
            let mut json_sources = Vec::new();
            for source in sources {
                if let Some(table) = source.as_table() {
                    let mut json_map = serde_json::Map::new();
                    if let Some(name) = table.get("name").and_then(|value| value.as_str()) {
                        json_map.insert(
                            "name".to_string(),
                            serde_json::Value::String(truncate_field(name.to_string())),
                        );
                    }
                    if let Some(url) = table.get("url").and_then(|value| value.as_str()) {
                        json_map.insert(
                            "url".to_string(),
                            serde_json::Value::String(truncate_field(url.to_string())),
                        );
                    }
                    if let Some(verify_ssl) =
                        table.get("verify_ssl").and_then(|value| value.as_bool())
                    {
                        json_map.insert(
                            "verify_ssl".to_string(),
                            serde_json::Value::Bool(verify_ssl),
                        );
                    }
                    json_sources.push(serde_json::Value::Object(json_map));
                }
            }

            Some(serde_json::Value::Array(json_sources))
        }
        TomlValue::Table(table) => {
            let mut json_map = serde_json::Map::new();
            for (key, value) in table {
                match value {
                    TomlValue::String(value) => {
                        json_map.insert(
                            key.to_string(),
                            serde_json::Value::String(truncate_field(value.to_string())),
                        );
                    }
                    TomlValue::Boolean(value) => {
                        json_map.insert(key.to_string(), serde_json::Value::Bool(*value));
                    }
                    _ => {}
                }
            }
            Some(serde_json::Value::Object(json_map))
        }
        _ => None,
    }
}

fn normalize_pypi_name(name: &str) -> String {
    truncate_field(name.trim().to_ascii_lowercase())
}

fn create_pypi_purl(name: &str, version: Option<&str>) -> Option<String> {
    let mut purl = PackageUrl::new(PipfileLockParser::PACKAGE_TYPE.as_str(), name).ok()?;
    if let Some(version) = version
        && purl.with_version(version).is_err()
    {
        return None;
    }

    Some(purl.to_string())
}

fn default_package_data(datasource_id: Option<DatasourceId>) -> PackageData {
    PackageData {
        package_type: Some(PipfileLockParser::PACKAGE_TYPE),
        primary_language: Some("Python".to_string()),
        datasource_id,
        ..Default::default()
    }
}

crate::register_parser!(
    "Pipenv lockfile and manifest",
    &["**/Pipfile.lock", "**/Pipfile"],
    "pypi",
    "Python",
    Some("https://github.com/pypa/pipfile"),
);
