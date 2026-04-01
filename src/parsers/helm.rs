use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use serde_json::Value as JsonValue;
use yaml_serde::{Mapping, Value};

use crate::models::{DatasourceId, Dependency, PackageData, PackageType, Party};

use super::PackageParser;

pub struct HelmChartYamlParser;

impl PackageParser for HelmChartYamlParser {
    const PACKAGE_TYPE: PackageType = PackageType::Helm;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "Chart.yaml")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let yaml_content = match read_yaml_file(path) {
            Ok(content) => content,
            Err(error) => {
                warn!("Failed to read Chart.yaml at {:?}: {}", path, error);
                return vec![default_package_data(Some(DatasourceId::HelmChartYaml))];
            }
        };

        vec![parse_chart_yaml(&yaml_content)]
    }
}

pub struct HelmChartLockParser;

impl PackageParser for HelmChartLockParser {
    const PACKAGE_TYPE: PackageType = PackageType::Helm;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "Chart.lock")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let yaml_content = match read_yaml_file(path) {
            Ok(content) => content,
            Err(error) => {
                warn!("Failed to read Chart.lock at {:?}: {}", path, error);
                return vec![default_package_data(Some(DatasourceId::HelmChartLock))];
            }
        };

        vec![parse_chart_lock(&yaml_content)]
    }
}

fn read_yaml_file(path: &Path) -> Result<Value, String> {
    let content =
        fs::read_to_string(path).map_err(|error| format!("Failed to read file: {error}"))?;
    yaml_serde::from_str(&content).map_err(|error| format!("Failed to parse YAML: {error}"))
}

fn parse_chart_yaml(yaml_content: &Value) -> PackageData {
    let name = extract_string_field(yaml_content, "name");
    let version = extract_string_field(yaml_content, "version");
    let description = extract_string_field(yaml_content, "description");
    let homepage_url = extract_string_field(yaml_content, "home");
    let keywords = extract_string_list_field(yaml_content, "keywords");
    let parties = extract_maintainers(yaml_content);
    let dependencies = extract_chart_yaml_dependencies(yaml_content);
    let extra_data = build_chart_yaml_extra_data(yaml_content);

    PackageData {
        package_type: Some(PackageType::Helm),
        name: name.clone(),
        version: version.clone(),
        primary_language: Some("YAML".to_string()),
        description,
        parties,
        keywords,
        homepage_url,
        is_private: false,
        extra_data,
        dependencies,
        datasource_id: Some(DatasourceId::HelmChartYaml),
        purl: name
            .as_deref()
            .and_then(|name| build_helm_purl(name, version.as_deref())),
        ..default_package_data(Some(DatasourceId::HelmChartYaml))
    }
}

fn parse_chart_lock(yaml_content: &Value) -> PackageData {
    let dependencies = extract_chart_lock_dependencies(yaml_content);

    let mut extra_data = HashMap::new();
    if let Some(digest) = extract_string_field(yaml_content, "digest") {
        extra_data.insert("digest".to_string(), JsonValue::String(digest));
    }
    if let Some(generated) = extract_string_field(yaml_content, "generated") {
        extra_data.insert("generated".to_string(), JsonValue::String(generated));
    }

    let mut package_data = default_package_data(Some(DatasourceId::HelmChartLock));
    package_data.dependencies = dependencies;
    package_data.extra_data = (!extra_data.is_empty()).then_some(extra_data);
    package_data
}

fn extract_chart_yaml_dependencies(yaml_content: &Value) -> Vec<Dependency> {
    let Some(entries) = yaml_content
        .get("dependencies")
        .and_then(Value::as_sequence)
    else {
        return Vec::new();
    };

    entries
        .iter()
        .filter_map(Value::as_mapping)
        .filter_map(parse_chart_yaml_dependency)
        .collect()
}

fn parse_chart_yaml_dependency(mapping: &Mapping) -> Option<Dependency> {
    let name = mapping_get(mapping, "name").and_then(yaml_value_to_string)?;
    let version = mapping_get(mapping, "version").and_then(yaml_value_to_string);
    let repository = mapping_get(mapping, "repository").and_then(yaml_value_to_string);
    let condition = mapping_get(mapping, "condition").and_then(yaml_value_to_string);
    let alias = mapping_get(mapping, "alias").and_then(yaml_value_to_string);
    let tags = mapping_get(mapping, "tags")
        .map(extract_string_values)
        .unwrap_or_default();
    let import_values = mapping_get(mapping, "import-values").and_then(yaml_to_json);

    let mut extra_data = HashMap::new();
    if let Some(repository) = repository {
        extra_data.insert("repository".to_string(), JsonValue::String(repository));
    }
    if let Some(condition) = condition.clone() {
        extra_data.insert("condition".to_string(), JsonValue::String(condition));
    }
    if let Some(alias) = alias {
        extra_data.insert("alias".to_string(), JsonValue::String(alias));
    }
    if !tags.is_empty() {
        extra_data.insert(
            "tags".to_string(),
            JsonValue::Array(tags.into_iter().map(JsonValue::String).collect()),
        );
    }
    if let Some(import_values) = import_values {
        extra_data.insert("import_values".to_string(), import_values);
    }

    Some(Dependency {
        purl: build_helm_purl(
            &name,
            version
                .as_deref()
                .filter(|value| is_exact_chart_version(value)),
        ),
        extracted_requirement: version.clone(),
        scope: Some("dependencies".to_string()),
        is_runtime: Some(true),
        is_optional: Some(condition.is_some() || extra_data.contains_key("tags")),
        is_pinned: Some(version.as_deref().is_some_and(is_exact_chart_version)),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: (!extra_data.is_empty()).then_some(extra_data),
    })
}

fn extract_chart_lock_dependencies(yaml_content: &Value) -> Vec<Dependency> {
    let Some(entries) = yaml_content
        .get("dependencies")
        .and_then(Value::as_sequence)
    else {
        return Vec::new();
    };

    entries
        .iter()
        .filter_map(Value::as_mapping)
        .filter_map(parse_chart_lock_dependency)
        .collect()
}

fn parse_chart_lock_dependency(mapping: &Mapping) -> Option<Dependency> {
    let name = mapping_get(mapping, "name").and_then(yaml_value_to_string)?;
    let version = mapping_get(mapping, "version").and_then(yaml_value_to_string)?;
    let repository = mapping_get(mapping, "repository").and_then(yaml_value_to_string);

    let mut extra_data = HashMap::new();
    if let Some(repository) = repository {
        extra_data.insert("repository".to_string(), JsonValue::String(repository));
    }

    Some(Dependency {
        purl: build_helm_purl(&name, Some(&version)),
        extracted_requirement: Some(version),
        scope: Some("dependencies".to_string()),
        is_runtime: Some(true),
        is_optional: Some(false),
        is_pinned: Some(true),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: (!extra_data.is_empty()).then_some(extra_data),
    })
}

fn build_chart_yaml_extra_data(yaml_content: &Value) -> Option<HashMap<String, JsonValue>> {
    let mut extra_data = HashMap::new();

    for (field, key) in [
        ("apiVersion", "api_version"),
        ("appVersion", "app_version"),
        ("kubeVersion", "kube_version"),
        ("type", "chart_type"),
        ("icon", "icon"),
    ] {
        if let Some(value) = extract_string_field(yaml_content, field) {
            extra_data.insert(key.to_string(), JsonValue::String(value));
        }
    }

    if let Some(value) = yaml_content.get("sources").and_then(yaml_to_json) {
        extra_data.insert("sources".to_string(), value);
    }
    if let Some(value) = yaml_content.get("annotations").and_then(yaml_to_json) {
        extra_data.insert("annotations".to_string(), value);
    }

    (!extra_data.is_empty()).then_some(extra_data)
}

fn extract_maintainers(yaml_content: &Value) -> Vec<Party> {
    let Some(maintainers) = yaml_content.get("maintainers").and_then(Value::as_sequence) else {
        return Vec::new();
    };

    maintainers
        .iter()
        .filter_map(Value::as_mapping)
        .filter_map(|mapping| {
            let name = mapping_get(mapping, "name").and_then(yaml_value_to_string)?;
            let email = mapping_get(mapping, "email").and_then(yaml_value_to_string);
            let url = mapping_get(mapping, "url").and_then(yaml_value_to_string);
            Some(Party {
                r#type: Some("person".to_string()),
                role: Some("maintainer".to_string()),
                name: Some(name),
                email,
                url,
                organization: None,
                organization_url: None,
                timezone: None,
            })
        })
        .collect()
}

fn extract_string_field(yaml_content: &Value, field: &str) -> Option<String> {
    yaml_content.get(field).and_then(yaml_value_to_string)
}

fn extract_string_list_field(yaml_content: &Value, field: &str) -> Vec<String> {
    yaml_content
        .get(field)
        .map(extract_string_values)
        .unwrap_or_default()
}

fn extract_string_values(value: &Value) -> Vec<String> {
    match value {
        Value::String(value) => vec![value.clone()],
        Value::Sequence(values) => values.iter().filter_map(yaml_value_to_string).collect(),
        _ => Vec::new(),
    }
}

fn yaml_value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn yaml_to_json(value: &Value) -> Option<JsonValue> {
    serde_json::to_value(value).ok()
}

fn mapping_get<'a>(mapping: &'a Mapping, key: &str) -> Option<&'a Value> {
    mapping.get(Value::String(key.to_string()))
}

fn is_exact_chart_version(version: &str) -> bool {
    let trimmed = version.trim();
    if trimmed.is_empty()
        || trimmed.contains('*')
        || trimmed.contains('^')
        || trimmed.contains('~')
        || trimmed.contains('>')
        || trimmed.contains('<')
        || trimmed.contains('=')
        || trimmed.contains('|')
        || trimmed.contains(',')
        || trimmed.contains(' ')
    {
        return false;
    }

    let core = trimmed
        .split_once(['-', '+'])
        .map(|(core, _)| core)
        .unwrap_or(trimmed);

    !core
        .split('.')
        .any(|segment| matches!(segment, "x" | "X" | "*"))
}

fn build_helm_purl(name: &str, version: Option<&str>) -> Option<String> {
    let mut purl = PackageUrl::new(PackageType::Helm.as_str(), name).ok()?;
    if let Some(version) = version {
        purl.with_version(version).ok()?;
    }
    Some(purl.to_string())
}

fn default_package_data(datasource_id: Option<DatasourceId>) -> PackageData {
    PackageData {
        package_type: Some(PackageType::Helm),
        datasource_id,
        ..Default::default()
    }
}

crate::register_parser!(
    "Helm chart metadata",
    &["**/Chart.yaml", "**/Chart.lock"],
    "helm",
    "YAML",
    Some("https://helm.sh/docs/topics/charts/"),
);
