use std::collections::HashMap;
use std::path::Path;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use serde_json::{Map as JsonMap, Value as JsonValue};
use toml::Value as TomlValue;
use toml::map::Map as TomlMap;

use crate::models::{DatasourceId, Dependency, FileReference, PackageData, PackageType, Party};
use crate::parsers::conda::build_purl as build_conda_purl;
use crate::parsers::python::read_toml_file;
use crate::parsers::utils::{read_file_to_string, split_name_email};

use super::PackageParser;

const FIELD_WORKSPACE: &str = "workspace";
const FIELD_PROJECT: &str = "project";
const FIELD_NAME: &str = "name";
const FIELD_VERSION: &str = "version";
const FIELD_AUTHORS: &str = "authors";
const FIELD_DESCRIPTION: &str = "description";
const FIELD_LICENSE: &str = "license";
const FIELD_LICENSE_FILE: &str = "license-file";
const FIELD_README: &str = "readme";
const FIELD_HOMEPAGE: &str = "homepage";
const FIELD_REPOSITORY: &str = "repository";
const FIELD_DOCUMENTATION: &str = "documentation";
const FIELD_CHANNELS: &str = "channels";
const FIELD_PLATFORMS: &str = "platforms";
const FIELD_REQUIRES_PIXI: &str = "requires-pixi";
const FIELD_EXCLUDE_NEWER: &str = "exclude-newer";
const FIELD_DEPENDENCIES: &str = "dependencies";
const FIELD_PYPI_DEPENDENCIES: &str = "pypi-dependencies";
const FIELD_FEATURE: &str = "feature";
const FIELD_ENVIRONMENTS: &str = "environments";
const FIELD_TASKS: &str = "tasks";
const FIELD_PYPI_OPTIONS: &str = "pypi-options";

pub struct PixiTomlParser;

impl PackageParser for PixiTomlParser {
    const PACKAGE_TYPE: PackageType = PackageType::Pixi;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "pixi.toml")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let toml_content = match read_toml_file(path) {
            Ok(content) => content,
            Err(error) => {
                warn!("Failed to read pixi.toml at {:?}: {}", path, error);
                return vec![default_package_data(Some(DatasourceId::PixiToml))];
            }
        };

        vec![parse_pixi_toml(&toml_content)]
    }
}

pub struct PixiLockParser;

impl PackageParser for PixiLockParser {
    const PACKAGE_TYPE: PackageType = PackageType::Pixi;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "pixi.lock")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path) {
            Ok(content) => content,
            Err(error) => {
                warn!("Failed to read pixi.lock at {:?}: {}", path, error);
                return vec![default_package_data(Some(DatasourceId::PixiLock))];
            }
        };

        let (lock_content, primary_language) = match parse_pixi_lock_document(&content) {
            Ok(parsed) => parsed,
            Err(error) => {
                warn!("Failed to read pixi.lock at {:?}: {}", path, error);
                return vec![default_package_data(Some(DatasourceId::PixiLock))];
            }
        };

        vec![parse_pixi_lock(&lock_content, primary_language)]
    }
}

fn parse_pixi_toml(toml_content: &TomlValue) -> PackageData {
    let identity = toml_content
        .get(FIELD_WORKSPACE)
        .and_then(TomlValue::as_table)
        .or_else(|| {
            toml_content
                .get(FIELD_PROJECT)
                .and_then(TomlValue::as_table)
        });

    let name = identity
        .and_then(|table| table.get(FIELD_NAME))
        .and_then(TomlValue::as_str)
        .map(ToOwned::to_owned);
    let version = identity
        .and_then(|table| table.get(FIELD_VERSION))
        .and_then(toml_value_to_string);

    let mut package = default_package_data(Some(DatasourceId::PixiToml));
    package.name = name.clone();
    package.version = version.clone();
    package.primary_language = Some("TOML".to_string());
    package.description = identity
        .and_then(|table| table.get(FIELD_DESCRIPTION))
        .and_then(TomlValue::as_str)
        .map(|value| value.trim().to_string());
    package.homepage_url = identity
        .and_then(|table| table.get(FIELD_HOMEPAGE))
        .and_then(TomlValue::as_str)
        .map(ToOwned::to_owned);
    package.vcs_url = identity
        .and_then(|table| table.get(FIELD_REPOSITORY))
        .and_then(TomlValue::as_str)
        .map(ToOwned::to_owned);
    package.parties = extract_authors(identity);
    package.extracted_license_statement = identity
        .and_then(|table| table.get(FIELD_LICENSE))
        .and_then(TomlValue::as_str)
        .map(ToOwned::to_owned);
    package.file_references = extract_manifest_file_references(identity);
    package.purl = name
        .as_deref()
        .and_then(|value| build_pixi_purl(value, version.as_deref()));
    package.dependencies = extract_manifest_dependencies(toml_content);
    package.extra_data = build_manifest_extra_data(toml_content, identity);
    package
}

fn parse_pixi_lock_document(content: &str) -> Result<(JsonValue, &'static str), String> {
    match toml::from_str::<TomlValue>(content) {
        Ok(toml_content) => serde_json::to_value(toml_content)
            .map(|value| (value, "TOML"))
            .map_err(|error| format!("Failed to convert TOML lockfile: {error}")),
        Err(toml_error) => yaml_serde::from_str::<JsonValue>(content)
            .map(|value| (value, "YAML"))
            .map_err(|yaml_error| {
                format!(
                    "Failed to parse Pixi lockfile as TOML ({toml_error}) or YAML ({yaml_error})"
                )
            }),
    }
}

fn parse_pixi_lock(lock_content: &JsonValue, primary_language: &str) -> PackageData {
    let mut package = default_package_data(Some(DatasourceId::PixiLock));
    package.primary_language = Some(primary_language.to_string());

    let lock_version = lock_content.get(FIELD_VERSION).and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_str()?.parse::<i64>().ok())
    });
    let mut extra_data = HashMap::new();
    if let Some(lock_version) = lock_version {
        extra_data.insert("lock_version".to_string(), JsonValue::from(lock_version));
    }
    if let Some(env_json) = lock_content.get(FIELD_ENVIRONMENTS).cloned() {
        extra_data.insert("lock_environments".to_string(), env_json);
    }
    package.extra_data = (!extra_data.is_empty()).then_some(extra_data);

    match lock_version {
        Some(6) => package.dependencies = extract_v6_lock_dependencies(lock_content),
        Some(4) => package.dependencies = extract_v4_lock_dependencies(lock_content),
        Some(_) | None => {}
    }

    package
}

fn extract_authors(identity: Option<&TomlMap<String, TomlValue>>) -> Vec<Party> {
    identity
        .and_then(|table| table.get(FIELD_AUTHORS))
        .and_then(TomlValue::as_array)
        .into_iter()
        .flatten()
        .filter_map(TomlValue::as_str)
        .map(|author| {
            let (name, email) = split_name_email(author);
            Party {
                r#type: None,
                role: Some("author".to_string()),
                name,
                email,
                url: None,
                organization: None,
                organization_url: None,
                timezone: None,
            }
        })
        .collect()
}

fn extract_manifest_file_references(
    identity: Option<&TomlMap<String, TomlValue>>,
) -> Vec<FileReference> {
    let Some(identity) = identity else {
        return Vec::new();
    };

    let mut references = Vec::new();

    if let Some(path) = identity.get(FIELD_LICENSE_FILE).and_then(TomlValue::as_str) {
        let path = path.trim();
        if !path.is_empty() {
            references.push(FileReference {
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

    if let Some(path) = identity.get(FIELD_README).and_then(TomlValue::as_str) {
        let path = path.trim();
        if !path.is_empty() {
            let already_present = references.iter().any(|reference| reference.path == path);
            if !already_present {
                references.push(FileReference {
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
    }

    references
}

fn extract_manifest_dependencies(toml_content: &TomlValue) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    if let Some(table) = toml_content
        .get(FIELD_DEPENDENCIES)
        .and_then(TomlValue::as_table)
    {
        dependencies.extend(extract_conda_dependencies(table, None, false));
    }
    if let Some(table) = toml_content
        .get(FIELD_PYPI_DEPENDENCIES)
        .and_then(TomlValue::as_table)
    {
        dependencies.extend(extract_pypi_dependencies(table, None, false));
    }

    if let Some(feature_table) = toml_content
        .get(FIELD_FEATURE)
        .and_then(TomlValue::as_table)
    {
        for (feature_name, value) in feature_table {
            let Some(feature) = value.as_table() else {
                continue;
            };
            if let Some(table) = feature
                .get(FIELD_DEPENDENCIES)
                .and_then(TomlValue::as_table)
            {
                dependencies.extend(extract_conda_dependencies(table, Some(feature_name), true));
            }
            if let Some(table) = feature
                .get(FIELD_PYPI_DEPENDENCIES)
                .and_then(TomlValue::as_table)
            {
                dependencies.extend(extract_pypi_dependencies(table, Some(feature_name), true));
            }
        }
    }

    dependencies
}

fn extract_conda_dependencies(
    table: &TomlMap<String, TomlValue>,
    scope: Option<&str>,
    optional: bool,
) -> Vec<Dependency> {
    table
        .iter()
        .filter_map(|(name, value)| build_conda_dependency(name, value, scope, optional))
        .collect()
}

fn build_conda_dependency(
    name: &str,
    value: &TomlValue,
    scope: Option<&str>,
    optional: bool,
) -> Option<Dependency> {
    let requirement = extract_conda_requirement(value);
    let exact_requirement = match value {
        TomlValue::String(value) => Some(value.to_string()),
        TomlValue::Table(table) => table.get(FIELD_VERSION).and_then(toml_value_to_string),
        _ => None,
    };
    let pinned = exact_requirement
        .as_deref()
        .is_some_and(is_exact_constraint);
    let exact_version = exact_requirement
        .as_deref()
        .filter(|_| pinned)
        .map(|value| value.trim_start_matches('='));
    let purl = build_conda_purl("conda", None, name, exact_version, None, None, None);

    let mut extra_data = HashMap::new();
    if let TomlValue::Table(dep_table) = value {
        for key in ["channel", "build", "path", "url", "git"] {
            if let Some(val) = dep_table.get(key).and_then(toml_value_to_string) {
                extra_data.insert(key.to_string(), JsonValue::String(val));
            }
        }
    }

    Some(Dependency {
        purl,
        extracted_requirement: requirement.clone(),
        scope: scope.map(ToOwned::to_owned),
        is_runtime: Some(true),
        is_optional: Some(optional),
        is_pinned: Some(pinned),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: (!extra_data.is_empty()).then_some(extra_data),
    })
}

fn extract_pypi_dependencies(
    table: &TomlMap<String, TomlValue>,
    scope: Option<&str>,
    optional: bool,
) -> Vec<Dependency> {
    table
        .iter()
        .filter_map(|(name, value)| build_pypi_dependency(name, value, scope, optional))
        .collect()
}

fn build_pypi_dependency(
    name: &str,
    value: &TomlValue,
    scope: Option<&str>,
    optional: bool,
) -> Option<Dependency> {
    let normalized_name = normalize_pypi_name(name);
    let requirement = extract_pypi_requirement(value);
    let exact_requirement = match value {
        TomlValue::String(value) => Some(value.to_string()),
        TomlValue::Table(table) => table.get(FIELD_VERSION).and_then(toml_value_to_string),
        _ => None,
    };
    let pinned = exact_requirement
        .as_deref()
        .is_some_and(is_exact_constraint);
    let exact_version = exact_requirement
        .as_deref()
        .filter(|_| pinned)
        .map(|value| value.trim_start_matches('='));
    let purl = build_pypi_purl(&normalized_name, exact_version);

    let mut extra_data = HashMap::new();
    if let TomlValue::Table(dep_table) = value {
        for key in [
            "index",
            "path",
            "git",
            "url",
            "branch",
            "tag",
            "rev",
            "subdirectory",
        ] {
            if let Some(val) = dep_table.get(key).and_then(toml_value_to_string) {
                extra_data.insert(key.replace('-', "_"), JsonValue::String(val));
            }
        }
        if let Some(editable) = dep_table.get("editable").and_then(TomlValue::as_bool) {
            extra_data.insert("editable".to_string(), JsonValue::Bool(editable));
        }
        if let Some(extras) = dep_table.get("extras").and_then(toml_to_json) {
            extra_data.insert("extras".to_string(), extras);
        }
    }

    Some(Dependency {
        purl,
        extracted_requirement: requirement.clone(),
        scope: scope.map(ToOwned::to_owned),
        is_runtime: Some(true),
        is_optional: Some(optional),
        is_pinned: Some(pinned),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: (!extra_data.is_empty()).then_some(extra_data),
    })
}

fn build_manifest_extra_data(
    toml_content: &TomlValue,
    identity: Option<&TomlMap<String, TomlValue>>,
) -> Option<HashMap<String, JsonValue>> {
    let mut extra_data = HashMap::new();

    for (field, key) in [
        (FIELD_CHANNELS, "channels"),
        (FIELD_PLATFORMS, "platforms"),
        (FIELD_REQUIRES_PIXI, "requires_pixi"),
        (FIELD_EXCLUDE_NEWER, "exclude_newer"),
        (FIELD_LICENSE_FILE, "license_file"),
        (FIELD_README, "readme"),
        (FIELD_DOCUMENTATION, "documentation"),
    ] {
        if let Some(value) = identity
            .and_then(|table| table.get(field))
            .and_then(toml_to_json)
        {
            extra_data.insert(key.to_string(), value);
        }
    }
    if let Some(value) = toml_content.get(FIELD_ENVIRONMENTS).and_then(toml_to_json) {
        extra_data.insert("environments".to_string(), value);
    }
    if let Some(value) = toml_content.get(FIELD_TASKS).and_then(toml_to_json) {
        extra_data.insert("tasks".to_string(), value);
    }
    if let Some(value) = toml_content.get(FIELD_PYPI_OPTIONS).and_then(toml_to_json) {
        extra_data.insert("pypi_options".to_string(), value);
    }
    if let Some(feature_names) = toml_content
        .get(FIELD_FEATURE)
        .and_then(TomlValue::as_table)
        .map(|table| table.keys().cloned().collect::<Vec<_>>())
        .filter(|names| !names.is_empty())
    {
        extra_data.insert(
            "features".to_string(),
            JsonValue::Array(feature_names.into_iter().map(JsonValue::String).collect()),
        );
    }

    (!extra_data.is_empty()).then_some(extra_data)
}

fn extract_v6_lock_dependencies(lock_content: &JsonValue) -> Vec<Dependency> {
    let environment_refs = collect_v6_package_refs(lock_content);
    let Some(packages) = lock_content.get("packages").and_then(JsonValue::as_array) else {
        return Vec::new();
    };

    packages
        .iter()
        .filter_map(JsonValue::as_object)
        .filter_map(|table| build_v6_lock_dependency(table, &environment_refs))
        .collect()
}

fn collect_v6_package_refs(lock_content: &JsonValue) -> HashMap<String, Vec<JsonValue>> {
    let mut refs = HashMap::new();
    let Some(environments) = lock_content
        .get(FIELD_ENVIRONMENTS)
        .and_then(JsonValue::as_object)
    else {
        return refs;
    };

    for (env_name, env_value) in environments {
        let Some(env_table) = env_value.as_object() else {
            continue;
        };
        let channels = env_table.get(FIELD_CHANNELS).cloned();
        let indexes = env_table.get("indexes").cloned();
        let Some(package_platforms) = env_table.get("packages").and_then(JsonValue::as_object)
        else {
            continue;
        };
        for (platform, values) in package_platforms {
            let Some(entries) = values.as_array() else {
                continue;
            };
            for entry in entries {
                let Some(table) = entry.as_object() else {
                    continue;
                };
                for (kind, locator_value) in table {
                    if let Some(locator) = json_value_to_string(locator_value) {
                        let mut data = JsonMap::new();
                        data.insert(
                            "environment".to_string(),
                            JsonValue::String(env_name.clone()),
                        );
                        data.insert("platform".to_string(), JsonValue::String(platform.clone()));
                        data.insert("kind".to_string(), JsonValue::String(kind.clone()));
                        if let Some(channels) = channels.clone() {
                            data.insert("channels".to_string(), channels);
                        }
                        if let Some(indexes) = indexes.clone() {
                            data.insert("indexes".to_string(), indexes);
                        }
                        refs.entry(locator)
                            .or_default()
                            .push(JsonValue::Object(data));
                    }
                }
            }
        }
    }

    refs
}

fn build_v6_lock_dependency(
    table: &JsonMap<String, JsonValue>,
    refs: &HashMap<String, Vec<JsonValue>>,
) -> Option<Dependency> {
    if let Some(locator) = table.get("pypi").and_then(json_value_to_string) {
        let name = table
            .get(FIELD_NAME)
            .and_then(JsonValue::as_str)
            .map(normalize_pypi_name)?;
        let version = table.get(FIELD_VERSION).and_then(json_value_to_string)?;
        let mut extra = HashMap::new();
        extra.insert("source".to_string(), JsonValue::String(locator.clone()));
        if let Some(val) = table.get("requires_dist").cloned() {
            extra.insert("requires_dist".to_string(), val);
        }
        if let Some(val) = table.get("requires_python").cloned() {
            extra.insert("requires_python".to_string(), val);
        }
        for key in ["sha256", "md5"] {
            if let Some(val) = table.get(key).cloned() {
                extra.insert(key.to_string(), val);
            }
        }
        if let Some(values) = refs.get(&locator)
            && !values.is_empty()
        {
            extra.insert(
                "lock_references".to_string(),
                JsonValue::Array(values.clone()),
            );
        }
        return Some(Dependency {
            purl: build_pypi_purl(&name, Some(&version)),
            extracted_requirement: Some(version),
            scope: None,
            is_runtime: Some(true),
            is_optional: Some(false),
            is_pinned: Some(true),
            is_direct: None,
            resolved_package: None,
            extra_data: Some(extra),
        });
    }

    if let Some(locator) = table.get("conda").and_then(json_value_to_string) {
        let name = conda_name_from_locator(&locator)?;
        let version = table.get(FIELD_VERSION).and_then(json_value_to_string);
        let mut extra = HashMap::new();
        extra.insert("source".to_string(), JsonValue::String(locator.clone()));
        for key in [
            "sha256",
            "md5",
            "license",
            "license_family",
            "depends",
            "constrains",
            "purls",
        ] {
            if let Some(val) = table.get(key).cloned() {
                extra.insert(key.to_string(), val);
            }
        }
        if let Some(values) = refs.get(&locator)
            && !values.is_empty()
        {
            extra.insert(
                "lock_references".to_string(),
                JsonValue::Array(values.clone()),
            );
        }
        return Some(Dependency {
            purl: build_conda_purl("conda", None, &name, version.as_deref(), None, None, None),
            extracted_requirement: version,
            scope: None,
            is_runtime: Some(true),
            is_optional: Some(false),
            is_pinned: Some(true),
            is_direct: None,
            resolved_package: None,
            extra_data: Some(extra),
        });
    }

    None
}

fn extract_v4_lock_dependencies(lock_content: &JsonValue) -> Vec<Dependency> {
    let Some(packages) = lock_content.get("packages").and_then(JsonValue::as_array) else {
        return Vec::new();
    };

    packages
        .iter()
        .filter_map(JsonValue::as_object)
        .filter_map(build_v4_lock_dependency)
        .collect()
}

fn build_v4_lock_dependency(table: &JsonMap<String, JsonValue>) -> Option<Dependency> {
    let kind = table.get("kind").and_then(JsonValue::as_str)?;
    let name = table.get(FIELD_NAME).and_then(json_value_to_string)?;
    let version = table.get(FIELD_VERSION).and_then(json_value_to_string);
    let mut extra = HashMap::new();
    for key in [
        "url",
        "path",
        "sha256",
        "md5",
        "editable",
        "build",
        "subdir",
        "license",
        "license_family",
        "depends",
        "requires_dist",
    ] {
        if let Some(val) = table.get(key).cloned() {
            extra.insert(key.replace('-', "_"), val);
        }
    }

    Some(Dependency {
        purl: match kind {
            "pypi" => build_pypi_purl(&normalize_pypi_name(&name), version.as_deref()),
            "conda" => build_conda_purl("conda", None, &name, version.as_deref(), None, None, None),
            _ => None,
        },
        extracted_requirement: version,
        scope: None,
        is_runtime: Some(true),
        is_optional: Some(false),
        is_pinned: Some(true),
        is_direct: None,
        resolved_package: None,
        extra_data: Some(extra),
    })
}

fn extract_conda_requirement(value: &TomlValue) -> Option<String> {
    match value {
        TomlValue::String(value) => Some(value.to_string()),
        TomlValue::Table(table) => table
            .get(FIELD_VERSION)
            .and_then(toml_value_to_string)
            .or_else(|| table.get("build").and_then(toml_value_to_string)),
        _ => None,
    }
}

fn extract_pypi_requirement(value: &TomlValue) -> Option<String> {
    match value {
        TomlValue::String(value) => Some(value.to_string()),
        TomlValue::Table(table) => table
            .get(FIELD_VERSION)
            .and_then(toml_value_to_string)
            .or_else(|| table.get("path").and_then(toml_value_to_string))
            .or_else(|| table.get("git").and_then(toml_value_to_string))
            .or_else(|| table.get("url").and_then(toml_value_to_string)),
        _ => None,
    }
}

fn toml_value_to_string(value: &TomlValue) -> Option<String> {
    match value {
        TomlValue::String(value) => Some(value.clone()),
        TomlValue::Integer(value) => Some(value.to_string()),
        TomlValue::Float(value) => Some(value.to_string()),
        TomlValue::Boolean(value) => Some(value.to_string()),
        _ => None,
    }
}

fn toml_to_json(value: &TomlValue) -> Option<JsonValue> {
    serde_json::to_value(value).ok()
}

fn json_value_to_string(value: &JsonValue) -> Option<String> {
    match value {
        JsonValue::String(value) => Some(value.clone()),
        JsonValue::Number(value) => Some(value.to_string()),
        JsonValue::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn normalize_pypi_name(name: &str) -> String {
    name.trim().replace('_', "-").to_ascii_lowercase()
}

fn build_pypi_purl(name: &str, version: Option<&str>) -> Option<String> {
    let mut purl = PackageUrl::new("pypi", name).ok()?;
    if let Some(version) = version {
        purl.with_version(version).ok()?;
    }
    Some(purl.to_string())
}

fn build_pixi_purl(name: &str, version: Option<&str>) -> Option<String> {
    let mut purl = PackageUrl::new(PackageType::Pixi.as_str(), name).ok()?;
    if let Some(version) = version {
        purl.with_version(version).ok()?;
    }
    Some(purl.to_string())
}

fn is_exact_constraint(value: &str) -> bool {
    let trimmed = value.trim();
    let normalized = trimmed.trim_start_matches('=');
    !normalized.is_empty()
        && !normalized.contains('*')
        && !normalized.contains('^')
        && !normalized.contains('~')
        && !normalized.contains('>')
        && !normalized.contains('<')
        && !normalized.contains('=')
        && !normalized.contains('|')
        && !normalized.contains(',')
        && !normalized.contains(' ')
}

fn conda_name_from_locator(locator: &str) -> Option<String> {
    let file_name = locator.rsplit('/').next()?;
    let stem = file_name
        .strip_suffix(".tar.bz2")
        .or_else(|| file_name.strip_suffix(".conda"))
        .unwrap_or(file_name);
    let mut parts = stem.rsplitn(3, '-');
    let _ = parts.next()?;
    let _ = parts.next()?;
    Some(parts.next()?.to_string())
}

fn default_package_data(datasource_id: Option<DatasourceId>) -> PackageData {
    PackageData {
        package_type: Some(PackageType::Pixi),
        datasource_id,
        ..Default::default()
    }
}

crate::register_parser!(
    "Pixi workspace manifest and lockfile",
    &["**/pixi.toml", "**/pixi.lock"],
    "pixi",
    "TOML/YAML",
    Some("https://pixi.sh/latest/reference/pixi_manifest/"),
);
