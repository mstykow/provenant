use std::collections::HashMap;
use std::path::Path;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use serde_json::Value;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType, Party};
use crate::parsers::utils::{MAX_ITERATION_COUNT, split_name_email, truncate_field};

use super::PackageParser;

pub struct VcpkgManifestParser;

impl PackageParser for VcpkgManifestParser {
    const PACKAGE_TYPE: PackageType = PackageType::Vcpkg;

    fn is_match(path: &Path) -> bool {
        path.file_name().and_then(|name| name.to_str()) == Some("vcpkg.json")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match crate::parsers::utils::read_file_to_string(path, None) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read vcpkg.json at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        let json: Value = match serde_json::from_str(&content) {
            Ok(json) => json,
            Err(e) => {
                warn!("Failed to parse vcpkg.json at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        vec![parse_vcpkg_manifest(path, &json)]
    }
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(PackageType::Vcpkg),
        datasource_id: Some(DatasourceId::VcpkgJson),
        ..Default::default()
    }
}

fn parse_vcpkg_manifest(path: &Path, json: &Value) -> PackageData {
    let name = get_non_empty_string(json, "name").map(truncate_field);
    let version = manifest_version(json).map(truncate_field);
    let description = get_string_or_array(json, "description").map(truncate_field);
    let homepage_url = get_non_empty_string(json, "homepage").map(truncate_field);
    let extracted_license_statement = get_string_or_array(json, "license").map(truncate_field);
    let parties = extract_maintainers(json);
    let dependencies = extract_dependencies(json);
    let extra_data = build_extra_data(path, json);

    PackageData {
        package_type: Some(PackageType::Vcpkg),
        namespace: None,
        name: name.clone(),
        version: version.clone(),
        primary_language: Some("C++".to_string()),
        description,
        parties,
        homepage_url,
        extracted_license_statement,
        is_private: name.is_none(),
        dependencies,
        extra_data,
        datasource_id: Some(DatasourceId::VcpkgJson),
        purl: name
            .as_deref()
            .and_then(|name| build_vcpkg_purl(name, version.as_deref()))
            .map(truncate_field),
        ..default_package_data()
    }
}

fn manifest_version(json: &Value) -> Option<String> {
    let version = [
        "version",
        "version-semver",
        "version-date",
        "version-string",
    ]
    .into_iter()
    .find_map(|field| get_non_empty_string(json, field));

    match (version, json.get("port-version").and_then(Value::as_i64)) {
        (Some(version), Some(port_version)) if port_version > 0 => {
            Some(format!("{}#{}", version, port_version))
        }
        (version, _) => version,
    }
}

fn extract_maintainers(json: &Value) -> Vec<Party> {
    let Some(value) = json.get("maintainers") else {
        return Vec::new();
    };

    let maintainers: Vec<String> = match value {
        Value::String(s) => vec![s.clone()],
        Value::Array(values) => values
            .iter()
            .take(MAX_ITERATION_COUNT)
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect(),
        _ => Vec::new(),
    };

    maintainers
        .into_iter()
        .map(|entry| {
            let (name, email) = split_name_email(&entry);
            Party {
                r#type: Some("person".to_string()),
                role: Some("maintainer".to_string()),
                name: name.map(truncate_field),
                email: email.map(truncate_field),
                url: None,
                organization: None,
                organization_url: None,
                timezone: None,
            }
        })
        .collect()
}

fn extract_dependencies(json: &Value) -> Vec<Dependency> {
    let mut dependencies: Vec<Dependency> = json
        .get("dependencies")
        .and_then(Value::as_array)
        .map(|deps| {
            deps.iter()
                .take(MAX_ITERATION_COUNT)
                .filter_map(parse_dependency_entry)
                .collect()
        })
        .unwrap_or_default();

    if let Some(features) = json.get("features").and_then(Value::as_object) {
        for (feature_name, feature_value) in features.iter().take(MAX_ITERATION_COUNT) {
            let Some(feature_dependencies) =
                feature_value.get("dependencies").and_then(Value::as_array)
            else {
                continue;
            };

            for dependency in feature_dependencies
                .iter()
                .take(MAX_ITERATION_COUNT)
                .filter_map(parse_dependency_entry)
                .map(|mut dependency| {
                    let mut extra_data = dependency.extra_data.take().unwrap_or_default();
                    extra_data.insert(
                        "feature".to_string(),
                        Value::String(feature_name.to_string()),
                    );
                    dependency.extra_data = Some(extra_data);
                    dependency
                })
            {
                dependencies.push(dependency);
            }
        }
    }

    dependencies
}

fn parse_dependency_entry(value: &Value) -> Option<Dependency> {
    match value {
        Value::String(name) => Some(Dependency {
            purl: build_vcpkg_purl(name, None).map(truncate_field),
            extracted_requirement: Some(truncate_field(name.clone())),
            scope: Some("dependencies".to_string()),
            is_runtime: Some(true),
            is_optional: Some(false),
            is_pinned: Some(false),
            is_direct: Some(true),
            resolved_package: None,
            extra_data: None,
        }),
        Value::Object(obj) => {
            let name = obj.get("name").and_then(Value::as_str)?.trim();
            if name.is_empty() {
                return None;
            }

            let extracted_requirement = obj
                .get("version>=")
                .and_then(Value::as_str)
                .map(|v| truncate_field(v.to_owned()))
                .or_else(|| Some(truncate_field(name.to_string())));

            let host = obj.get("host").and_then(Value::as_bool).unwrap_or(false);
            let mut extra = HashMap::new();
            for field in [
                "version>=",
                "features",
                "default-features",
                "host",
                "platform",
            ] {
                if let Some(field_value) = obj.get(field) {
                    extra.insert(field.to_string(), field_value.clone());
                }
            }

            Some(Dependency {
                purl: build_vcpkg_purl(name, None).map(truncate_field),
                extracted_requirement,
                scope: Some("dependencies".to_string()),
                is_runtime: Some(!host),
                is_optional: Some(false),
                is_pinned: Some(false),
                is_direct: Some(true),
                resolved_package: None,
                extra_data: (!extra.is_empty()).then_some(extra),
            })
        }
        _ => None,
    }
}

fn build_extra_data(path: &Path, json: &Value) -> Option<HashMap<String, Value>> {
    let mut extra = HashMap::new();
    for field in [
        "builtin-baseline",
        "overrides",
        "supports",
        "default-features",
        "features",
        "configuration",
        "vcpkg-configuration",
        "documentation",
    ] {
        if let Some(value) = json.get(field) {
            extra.insert(field.to_string(), value.clone());
        }
    }

    if !extra.contains_key("configuration")
        && !extra.contains_key("vcpkg-configuration")
        && let Some(config) = read_sibling_configuration(path)
    {
        extra.insert("configuration".to_string(), config);
    }

    (!extra.is_empty()).then_some(extra)
}

fn read_sibling_configuration(path: &Path) -> Option<Value> {
    let sibling_path = path.with_file_name("vcpkg-configuration.json");
    let content = crate::parsers::utils::read_file_to_string(&sibling_path, None).ok()?;
    match serde_json::from_str(&content) {
        Ok(value) => Some(value),
        Err(e) => {
            warn!(
                "Failed to parse sibling vcpkg-configuration.json at {:?}: {}",
                sibling_path, e
            );
            None
        }
    }
}

fn get_non_empty_string(json: &Value, field: &str) -> Option<String> {
    json.get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

fn get_string_or_array(json: &Value, field: &str) -> Option<String> {
    match json.get(field) {
        Some(Value::String(s)) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Some(Value::Array(values)) => {
            let collected: Vec<_> = values
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            (!collected.is_empty()).then(|| collected.join("\n"))
        }
        _ => None,
    }
}

fn build_vcpkg_purl(name: &str, version: Option<&str>) -> Option<String> {
    let mut purl = PackageUrl::new("generic", name).ok()?;
    purl.with_namespace("vcpkg").ok()?;
    if let Some(version) = version {
        purl.with_version(version).ok()?;
    }
    Some(purl.to_string())
}

crate::register_parser!(
    "vcpkg manifest file",
    &["**/vcpkg.json"],
    "vcpkg",
    "",
    Some("https://learn.microsoft.com/en-us/vcpkg/reference/vcpkg-json"),
);
