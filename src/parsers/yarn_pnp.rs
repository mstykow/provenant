use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};
use crate::parser_warn as warn;
use crate::parsers::utils::npm_purl;

use super::PackageParser;

pub struct YarnPnpParser;

impl PackageParser for YarnPnpParser {
    const PACKAGE_TYPE: PackageType = PackageType::Npm;

    fn is_match(path: &Path) -> bool {
        path.file_name().and_then(|name| name.to_str()) == Some(".pnp.cjs")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) => {
                warn!("Failed to read .pnp.cjs at {:?}: {}", path, error);
                return vec![default_package_data()];
            }
        };

        match parse_yarn_pnp(&content) {
            Ok(package_data) => vec![package_data],
            Err(error) => {
                warn!("Failed to parse .pnp.cjs at {:?}: {}", path, error);
                vec![default_package_data()]
            }
        }
    }
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(YarnPnpParser::PACKAGE_TYPE),
        primary_language: Some("JavaScript".to_string()),
        datasource_id: Some(DatasourceId::YarnPnpCjs),
        ..Default::default()
    }
}

fn parse_yarn_pnp(content: &str) -> Result<PackageData, String> {
    let json_text = extract_raw_runtime_state_json(content)
        .ok_or_else(|| "RAW_RUNTIME_STATE object not found in .pnp.cjs".to_string())?;
    let runtime_state: serde_json::Value = serde_json::from_str(json_text)
        .map_err(|error| format!("invalid RAW_RUNTIME_STATE JSON: {error}"))?;

    let registry_entries = runtime_state
        .get("packageRegistryData")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "packageRegistryData missing from RAW_RUNTIME_STATE".to_string())?;

    let root_refs = registry_entries
        .iter()
        .find_map(parse_root_dependency_map)
        .unwrap_or_default();
    let mut seen_locators = HashSet::new();
    let mut dependencies = Vec::new();

    for entry in registry_entries {
        let Some(locator) = entry.get(0).and_then(serde_json::Value::as_str) else {
            continue;
        };
        if !seen_locators.insert(locator.to_string()) {
            continue;
        }
        let Some((name, reference)) = split_locator(locator) else {
            continue;
        };

        let version = reference.strip_prefix("npm:");
        dependencies.push(Dependency {
            purl: npm_purl(name, version),
            extracted_requirement: Some(reference.to_string()),
            scope: Some("dependencies".to_string()),
            is_runtime: Some(true),
            is_optional: Some(false),
            is_pinned: Some(version.is_some()),
            is_direct: Some(
                root_refs
                    .get(name)
                    .is_some_and(|root_ref| root_ref == reference),
            ),
            resolved_package: None,
            extra_data: Some(HashMap::from([(
                "locator".to_string(),
                serde_json::Value::String(locator.to_string()),
            )])),
        });
    }

    let mut package = default_package_data();
    package.dependencies = dependencies;
    package.extra_data = Some(HashMap::from([(
        "package_registry_entries".to_string(),
        serde_json::Value::from(registry_entries.len()),
    )]));
    Ok(package)
}

fn parse_root_dependency_map(entry: &serde_json::Value) -> Option<HashMap<String, String>> {
    if !entry.get(0).is_some_and(serde_json::Value::is_null) {
        return None;
    }

    let dependencies = entry.get(1)?.get("packageDependencies")?;
    Some(parse_dependency_pairs(dependencies))
}

fn parse_dependency_pairs(value: &serde_json::Value) -> HashMap<String, String> {
    if let Some(array) = value.as_array() {
        return array
            .iter()
            .filter_map(|pair| {
                let pair = pair.as_array()?;
                let name = pair.first()?.as_str()?;
                let reference = pair.get(1)?.as_str()?;
                Some((name.to_string(), reference.to_string()))
            })
            .collect();
    }

    value
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(name, reference)| {
            reference
                .as_str()
                .map(|reference| (name.clone(), reference.to_string()))
        })
        .collect()
}

fn split_locator(locator: &str) -> Option<(&str, &str)> {
    let split_at = locator.rfind('@')?;
    if split_at == 0 {
        return None;
    }
    Some((&locator[..split_at], &locator[split_at + 1..]))
}

fn extract_raw_runtime_state_json(content: &str) -> Option<&str> {
    let marker = "const RAW_RUNTIME_STATE =";
    let marker_index = content.find(marker)?;
    let after_marker = &content[marker_index + marker.len()..];
    let open_index = after_marker.find('{')?;
    let json_start = marker_index + marker.len() + open_index;

    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (offset, ch) in content[json_start..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let end = json_start + offset + ch.len_utf8();
                    return Some(&content[json_start..end]);
                }
            }
            _ => {}
        }
    }

    None
}

crate::register_parser!(
    "yarn plug and play runtime state",
    &["**/.pnp.cjs"],
    "npm",
    "JavaScript",
    Some("https://yarnpkg.com/features/pnp"),
);
