//! Parser for npm/pnpm workspace configuration files.
//!
//! Extracts workspace package patterns and monorepo structure from workspace
//! configuration files used by npm, yarn, and pnpm to define workspaces.
//!
//! # Supported Formats
//! - pnpm-workspace.yaml (YAML workspace configuration)
//!
//! # Key Features
//! - Workspace package pattern extraction (glob patterns for package locations)
//! - Monorepo structure detection and documentation
//! - Package discovery from workspace configurations
//!
//! # Implementation Notes
//! - Parses YAML format for workspace field
//! - Package patterns are glob expressions (e.g., `packages/*`, `@scoped/**`)
//! - Returns package data representing the workspace configuration itself

use crate::models::PackageData;
use crate::models::{DatasourceId, PackageType};
use std::fs;
use std::path::Path;
use yaml_serde::Value;

use super::PackageParser;

/// npm workspace parser for pnpm-workspace.yaml files.
///
/// Extracts workspace package patterns for monorepo configurations.
pub struct NpmWorkspaceParser;

impl PackageParser for NpmWorkspaceParser {
    const PACKAGE_TYPE: PackageType = PackageType::Npm;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| name == "pnpm-workspace.yaml")
            .unwrap_or(false)
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) => {
                crate::parser_warn!("Failed to read npm workspace file at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        let workspace_data: Value = match yaml_serde::from_str(&content) {
            Ok(data) => data,
            Err(e) => {
                crate::parser_warn!("Failed to parse npm workspace file at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        vec![parse_workspace_file(&workspace_data)]
    }
}

/// Returns a default empty PackageData for error cases
fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(NpmWorkspaceParser::PACKAGE_TYPE),
        datasource_id: Some(DatasourceId::PnpmWorkspaceYaml),
        ..Default::default()
    }
}

/// Parse a pnpm-workspace.yaml file and extract workspace configuration
fn parse_workspace_file(workspace_data: &Value) -> PackageData {
    // Extract the `packages` field which contains workspace patterns
    let workspaces = workspace_data.get("packages").and_then(|v| v.as_sequence());

    match workspaces {
        Some(workspace_patterns) => {
            let workspaces_vec: Vec<String> = workspace_patterns
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect();

            PackageData {
                package_type: Some(NpmWorkspaceParser::PACKAGE_TYPE),
                extra_data: if workspaces_vec.is_empty() {
                    None
                } else {
                    let mut extra = std::collections::HashMap::new();
                    extra.insert(
                        "datasource_id".to_string(),
                        serde_json::Value::String("pnpm_workspace_yaml".to_string()),
                    );
                    extra.insert(
                        "workspaces".to_string(),
                        serde_json::Value::Array(
                            workspaces_vec
                                .into_iter()
                                .map(serde_json::Value::String)
                                .collect(),
                        ),
                    );
                    Some(extra)
                },
                ..default_package_data()
            }
        }
        None => {
            // No workspaces found, return basic package data
            PackageData {
                package_type: Some(NpmWorkspaceParser::PACKAGE_TYPE),
                extra_data: {
                    let mut extra = std::collections::HashMap::new();
                    extra.insert(
                        "datasource_id".to_string(),
                        serde_json::Value::String("pnpm_workspace_yaml".to_string()),
                    );
                    Some(extra)
                },
                ..default_package_data()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_match() {
        assert!(NpmWorkspaceParser::is_match(Path::new(
            "pnpm-workspace.yaml"
        )));
        assert!(!NpmWorkspaceParser::is_match(Path::new("package.json")));
        assert!(!NpmWorkspaceParser::is_match(Path::new("pnpm-lock.yaml")));
        assert!(!NpmWorkspaceParser::is_match(Path::new("README.md")));
    }

    #[test]
    fn test_parse_workspace_with_single_package() {
        let yaml_content = r#"
packages:
  - "packages/*"
"#;

        let workspace_data: Value = yaml_serde::from_str(yaml_content).unwrap();
        let result = parse_workspace_file(&workspace_data);

        assert_eq!(result.package_type, Some(PackageType::Npm));

        let extra_data = result.extra_data.unwrap();
        assert_eq!(
            extra_data.get("datasource_id").unwrap().as_str().unwrap(),
            "pnpm_workspace_yaml"
        );
        let workspaces = extra_data.get("workspaces").unwrap().as_array().unwrap();
        assert_eq!(workspaces.len(), 1);
        assert_eq!(workspaces[0], "packages/*");
    }

    #[test]
    fn test_parse_workspace_with_multiple_packages() {
        let yaml_content = r#"
packages:
  - "packages/*"
  - "apps/*"
  - "tools/*"
"#;

        let workspace_data: Value = yaml_serde::from_str(yaml_content).unwrap();
        let result = parse_workspace_file(&workspace_data);

        let extra_data = result.extra_data.unwrap();
        let workspaces = extra_data.get("workspaces").unwrap().as_array().unwrap();
        assert_eq!(workspaces.len(), 3);
        assert_eq!(workspaces[0], "packages/*");
        assert_eq!(workspaces[1], "apps/*");
        assert_eq!(workspaces[2], "tools/*");
    }

    #[test]
    fn test_parse_workspace_with_wildcard_pattern() {
        let yaml_content = r#"
packages:
  - "*"
"#;

        let workspace_data: Value = yaml_serde::from_str(yaml_content).unwrap();
        let result = parse_workspace_file(&workspace_data);

        let extra_data = result.extra_data.unwrap();
        let workspaces = extra_data.get("workspaces").unwrap().as_array().unwrap();
        assert_eq!(workspaces.len(), 1);
        assert_eq!(workspaces[0], "*");
    }

    #[test]
    fn test_parse_workspace_with_negated_pattern() {
        let yaml_content = r#"
packages:
  - "packages/*"
  - "!packages/dont-scan-me"
"#;

        let workspace_data: Value = yaml_serde::from_str(yaml_content).unwrap();
        let result = parse_workspace_file(&workspace_data);

        let extra_data = result.extra_data.unwrap();
        let workspaces = extra_data.get("workspaces").unwrap().as_array().unwrap();
        assert_eq!(workspaces.len(), 2);
        assert_eq!(workspaces[0], "packages/*");
        assert_eq!(workspaces[1], "!packages/dont-scan-me");
    }

    #[test]
    fn test_parse_workspace_with_depth_pattern() {
        let yaml_content = r#"
packages:
  - "**/components/*"
"#;

        let workspace_data: Value = yaml_serde::from_str(yaml_content).unwrap();
        let result = parse_workspace_file(&workspace_data);

        let extra_data = result.extra_data.unwrap();
        let workspaces = extra_data.get("workspaces").unwrap().as_array().unwrap();
        assert_eq!(workspaces.len(), 1);
        assert_eq!(workspaces[0], "**/components/*");
    }

    #[test]
    fn test_parse_workspace_with_no_packages() {
        let yaml_content = r#"
name: my-workspace
"#;

        let workspace_data: Value = yaml_serde::from_str(yaml_content).unwrap();
        let result = parse_workspace_file(&workspace_data);

        assert_eq!(result.package_type, Some(PackageType::Npm));
        assert!(result.extra_data.is_some());
        let extra_data = result.extra_data.unwrap();
        assert_eq!(
            extra_data.get("datasource_id").unwrap().as_str().unwrap(),
            "pnpm_workspace_yaml"
        );
        assert!(!extra_data.contains_key("workspaces"));
    }

    #[test]
    fn test_parse_workspace_with_empty_packages_array() {
        let yaml_content = r#"
packages: []
"#;

        let workspace_data: Value = yaml_serde::from_str(yaml_content).unwrap();
        let result = parse_workspace_file(&workspace_data);

        assert_eq!(result.package_type, Some(PackageType::Npm));
        assert!(
            result.extra_data.is_none() || !result.extra_data.unwrap().contains_key("workspaces")
        );
    }

    #[test]
    fn test_default_package_data() {
        let result = default_package_data();

        assert_eq!(result.package_type, Some(PackageType::Npm));
        assert!(result.name.is_none());
        assert!(result.version.is_none());
        assert!(result.extra_data.is_none());
    }
}

crate::register_parser!(
    "pnpm workspace yaml file",
    &["**/pnpm-workspace.yaml"],
    "npm",
    "JavaScript",
    Some("https://pnpm.io/pnpm-workspace_yaml"),
);
