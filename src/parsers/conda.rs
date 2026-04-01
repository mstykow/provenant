//! Parser for Conda/Anaconda package manifest files.
//!
//! Extracts package metadata and dependencies from Conda ecosystem manifest files
//! supporting both recipe definitions and environment specifications.
//!
//! # Supported Formats
//! - meta.yaml (Conda recipe metadata with Jinja2 templating support)
//! - conda.yaml/environment.yml (Conda environment dependency specifications)
//!
//! # Key Features
//! - YAML parsing for environment files
//! - Dependency extraction from dependencies and build_requirements sections
//! - Channel specification and platform detection
//! - Version constraint parsing for Conda version specifiers
//! - Package URL (purl) generation for conda packages
//! - Limited meta.yaml support (note: Jinja2 templating not fully resolved)
//!
//! # Implementation Notes
//! - Uses YAML parsing via `yaml_serde`
//! - meta.yaml: Jinja2 templates not evaluated (use rendered YAML if available)
//! - environment.yml: Full dependency specification support
//! - Graceful error handling with `warn!()` logs
//!
//! # References
//! - <https://docs.conda.io/projects/conda-build/en/latest/resources/define-metadata.html>
//! - <https://docs.conda.io/projects/conda/en/latest/user-guide/tasks/manage-environments.html>

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::parser_warn as warn;
use regex::Regex;
use yaml_serde::Value;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};

use super::PackageParser;
use super::license_normalization::{
    DeclaredLicenseMatchMetadata, build_declared_license_data_from_pair,
    normalize_spdx_declared_license,
};

fn default_package_data(datasource_id: Option<DatasourceId>) -> PackageData {
    PackageData {
        package_type: Some(CondaMetaYamlParser::PACKAGE_TYPE),
        datasource_id,
        ..Default::default()
    }
}

/// Build a PURL (Package URL) for Conda or PyPI packages
pub(crate) fn build_purl(
    package_type: &str,
    namespace: Option<&str>,
    name: &str,
    version: Option<&str>,
    _qualifiers: Option<&str>,
    _subpath: Option<&str>,
    _extras: Option<&str>,
) -> Option<String> {
    let purl = match package_type {
        "conda" => {
            if let Some(ns) = namespace {
                match version {
                    Some(v) => format!("pkg:conda/{}/{}@{}", ns, name, v),
                    None => format!("pkg:conda/{}/{}", ns, name),
                }
            } else {
                match version {
                    Some(v) => format!("pkg:conda/{}@{}", name, v),
                    None => format!("pkg:conda/{}", name),
                }
            }
        }
        "pypi" => match version {
            Some(v) => format!("pkg:pypi/{}@{}", name, v),
            None => format!("pkg:pypi/{}", name),
        },
        _ => format!("pkg:{}/{}", package_type, name),
    };
    Some(purl)
}

fn build_conda_package_purl(name: Option<&str>, version: Option<&str>) -> Option<String> {
    let name = name?;
    build_purl("conda", None, name, version, None, None, None)
}

fn yaml_value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn extract_conda_requirement_name(req: &str) -> Option<String> {
    let req = req.trim();
    if req.is_empty() {
        return None;
    }

    let req_without_ns = req.rsplit_once("::").map(|(_, rest)| rest).unwrap_or(req);

    let name = req_without_ns
        .split_whitespace()
        .next()
        .unwrap_or(req_without_ns)
        .split(['=', '<', '>', '!', '~'])
        .next()
        .unwrap_or(req_without_ns)
        .trim();

    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Conda recipe manifest (meta.yaml) parser.
///
/// Extracts package metadata and dependencies from Conda recipe files, which
/// define how to build a Conda package. Handles Jinja2 templating used in
/// recipe files for variable substitution.
pub struct CondaMetaYamlParser;

impl PackageParser for CondaMetaYamlParser {
    const PACKAGE_TYPE: PackageType = PackageType::Conda;

    fn is_match(path: &Path) -> bool {
        // Match */meta.yaml following Python reference logic
        path.file_name()
            .is_some_and(|name| name == "meta.yaml" || name == "meta.yml")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let contents = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read {}: {}", path.display(), e);
                return vec![default_package_data(Some(DatasourceId::CondaMetaYaml))];
            }
        };

        // Extract Jinja2 variables and apply crude substitution
        let variables = extract_jinja2_variables(&contents);
        let processed_yaml = apply_jinja2_substitutions(&contents, &variables);

        // Parse YAML after Jinja2 processing
        let yaml: Value = match yaml_serde::from_str(&processed_yaml) {
            Ok(y) => y,
            Err(e) => {
                warn!("Failed to parse YAML in {}: {}", path.display(), e);
                return vec![default_package_data(Some(DatasourceId::CondaMetaYaml))];
            }
        };

        let package_element = yaml.get("package").and_then(|v| v.as_mapping());
        let name = package_element
            .and_then(|p| p.get("name"))
            .and_then(yaml_value_to_string);

        let version = package_element
            .and_then(|p| p.get("version"))
            .and_then(yaml_value_to_string);

        let source = yaml.get("source").and_then(|v| v.as_mapping());
        let download_url = source
            .and_then(|s| s.get("url"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let sha256 = source
            .and_then(|s| s.get("sha256"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let about = yaml.get("about").and_then(|v| v.as_mapping());
        let homepage_url = about
            .and_then(|a| a.get("home"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let extracted_license_statement = about
            .and_then(|a| a.get("license"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let (declared_license_expression, declared_license_expression_spdx, license_detections) =
            normalize_conda_declared_license(extracted_license_statement.as_deref());

        let description = about
            .and_then(|a| a.get("summary"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let vcs_url = about
            .and_then(|a| a.get("dev_url"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let license_file = about
            .and_then(|a| a.get("license_file"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(String::from);

        // Extract dependencies from requirements sections
        let mut dependencies = Vec::new();
        let mut extra_data: HashMap<String, serde_json::Value> = HashMap::new();

        if let Some(requirements) = yaml.get("requirements").and_then(|v| v.as_mapping()) {
            for (scope_key, reqs_value) in requirements {
                let scope = scope_key.as_str().unwrap_or("unknown");
                if let Some(reqs) = reqs_value.as_sequence() {
                    for req in reqs {
                        if let Some(req_str) = req.as_str()
                            && let Some(dep) = parse_conda_requirement(req_str, scope)
                        {
                            // Filter out pip/python from dependencies, add to extra_data
                            if extract_conda_requirement_name(req_str)
                                .is_some_and(|n| n == "pip" || n == "python")
                            {
                                if let Some(arr) = extra_data
                                    .entry(scope.to_string())
                                    .or_insert_with(|| serde_json::Value::Array(vec![]))
                                    .as_array_mut()
                                {
                                    arr.push(serde_json::Value::String(req_str.to_string()))
                                }
                            } else {
                                dependencies.push(dep);
                            }
                        }
                    }
                }
            }
        }

        let mut pkg = default_package_data(Some(DatasourceId::CondaMetaYaml));
        pkg.package_type = Some(Self::PACKAGE_TYPE);
        pkg.datasource_id = Some(DatasourceId::CondaMetaYaml);
        pkg.name = name;
        pkg.version = version;
        pkg.purl = build_conda_package_purl(pkg.name.as_deref(), pkg.version.as_deref());
        pkg.download_url = download_url;
        pkg.homepage_url = homepage_url;
        pkg.declared_license_expression = declared_license_expression;
        pkg.declared_license_expression_spdx = declared_license_expression_spdx;
        pkg.license_detections = license_detections;
        pkg.extracted_license_statement = extracted_license_statement;
        pkg.description = description;
        pkg.vcs_url = vcs_url;
        pkg.sha256 = sha256;
        pkg.dependencies = dependencies;
        if let Some(license_file) = license_file {
            extra_data.insert(
                "license_file".to_string(),
                serde_json::Value::String(license_file),
            );
        }
        if !extra_data.is_empty() {
            pkg.extra_data = Some(extra_data);
        }
        vec![pkg]
    }
}

fn normalize_conda_declared_license(
    statement: Option<&str>,
) -> (
    Option<String>,
    Option<String>,
    Vec<crate::models::LicenseDetection>,
) {
    match statement.map(str::trim).filter(|value| !value.is_empty()) {
        Some("Apache Software") => build_declared_license_data_from_pair(
            "apache-2.0",
            "Apache-2.0",
            DeclaredLicenseMatchMetadata::single_line("Apache Software"),
        ),
        Some("BSD-3-Clause") => build_declared_license_data_from_pair(
            "bsd-new",
            "BSD-3-Clause",
            DeclaredLicenseMatchMetadata::single_line("BSD-3-Clause"),
        ),
        other => normalize_spdx_declared_license(other),
    }
}

/// Conda environment file (environment.yml, conda.yaml) parser.
///
/// Extracts dependencies from Conda environment files used to define reproducible
/// environments. Supports both Conda and pip dependencies, with channel specifications.
pub struct CondaEnvironmentYmlParser;

impl PackageParser for CondaEnvironmentYmlParser {
    const PACKAGE_TYPE: PackageType = PackageType::Conda;

    fn is_match(path: &Path) -> bool {
        // Python reference: path_patterns = ('*conda*.yaml', '*env*.yaml', '*environment*.yaml')
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            let lower = name.to_lowercase();
            (lower.contains("conda") || lower.contains("env") || lower.contains("environment"))
                && (lower.ends_with(".yaml") || lower.ends_with(".yml"))
        } else {
            false
        }
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let contents = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read {}: {}", path.display(), e);
                return vec![default_package_data(Some(DatasourceId::CondaYaml))];
            }
        };

        let yaml: Value = match yaml_serde::from_str(&contents) {
            Ok(y) => y,
            Err(e) => {
                warn!("Failed to parse YAML in {}: {}", path.display(), e);
                return vec![default_package_data(Some(DatasourceId::CondaYaml))];
            }
        };

        let name = yaml.get("name").and_then(|v| v.as_str()).map(String::from);

        let dependencies = extract_environment_dependencies(&yaml);

        let mut extra_data = HashMap::new();
        if let Some(channels) = yaml.get("channels").and_then(|v| v.as_sequence()) {
            let channels_vec: Vec<String> = channels
                .iter()
                .filter_map(|c| c.as_str().map(String::from))
                .collect();
            if !channels_vec.is_empty() {
                extra_data.insert("channels".to_string(), serde_json::json!(channels_vec));
            }
        }

        // Environment files are private (not published packages)
        let mut pkg = default_package_data(Some(DatasourceId::CondaYaml));
        pkg.package_type = Some(Self::PACKAGE_TYPE);
        pkg.datasource_id = Some(DatasourceId::CondaYaml);
        pkg.name = name;
        pkg.purl = build_conda_package_purl(pkg.name.as_deref(), pkg.version.as_deref());
        pkg.primary_language = Some("Python".to_string());
        pkg.dependencies = dependencies;
        pkg.is_private = true;
        if !extra_data.is_empty() {
            pkg.extra_data = Some(extra_data);
        }
        vec![pkg]
    }
}

/// Extract Jinja2-style variables from a Conda meta.yaml
///
/// Example:
/// ```ignore
/// {% set version = "0.45.0" %}
/// {% set sha256 = "abc123..." %}
/// ```
pub fn extract_jinja2_variables(content: &str) -> HashMap<String, String> {
    let mut variables = HashMap::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("{%") && trimmed.ends_with("%}") && trimmed.contains('=') {
            // Strip {% and %}
            let inner = trimmed
                .trim_start_matches("{%")
                .trim_end_matches("%}")
                .trim()
                .trim_start_matches("set")
                .trim();

            // Split on '=' to get key and value
            if let Some((key, value)) = inner.split_once('=') {
                let key = key.trim();
                let value = value.trim().trim_matches('"').trim_matches('\'');
                variables.insert(key.to_string(), value.to_string());
            }
        }
    }

    variables
}

/// Apply Jinja2 variable substitutions to YAML content
///
/// Supports:
/// - `{{ variable }}` - Simple substitution
/// - `{{ variable|lower }}` - Lowercase filter
pub fn apply_jinja2_substitutions(content: &str, variables: &HashMap<String, String>) -> String {
    let mut result = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip Jinja2 set statements (already extracted)
        if trimmed.starts_with("{%") && trimmed.ends_with("%}") && trimmed.contains('=') {
            continue;
        }

        let mut processed_line = line.to_string();

        // Apply variable substitutions
        if line.contains("{{") && line.contains("}}") {
            for (var_name, var_value) in variables {
                // Handle |lower filter
                let pattern_lower = format!("{{{{ {}|lower }}}}", var_name);
                if processed_line.contains(&pattern_lower) {
                    processed_line =
                        processed_line.replace(&pattern_lower, &var_value.to_lowercase());
                }

                // Handle normal substitution
                let pattern_normal = format!("{{{{ {} }}}}", var_name);
                processed_line = processed_line.replace(&pattern_normal, var_value);
            }
        }

        // Skip lines with unresolved Jinja2 templates (complex expressions we can't handle)
        if processed_line.contains("{{") {
            continue;
        }

        result.push(processed_line);
    }

    result.join("\n")
}

/// Parse a Conda requirement string into a Dependency
///
/// Format examples:
/// - `mccortex ==1.0` - Pinned version with space before operator
/// - `python >=3.6` - Version constraint
/// - `conda-forge::numpy=1.15.4` - Namespace and pinned version (no space)
/// - `bwa` - No version specified
pub fn parse_conda_requirement(req: &str, scope: &str) -> Option<Dependency> {
    let req = req.trim();

    // Handle namespace prefix (conda-forge::package)
    let (namespace, channel_url, req_without_ns) = parse_conda_channel_prefix(req);

    // Split on first space to separate name from version constraint
    let (name_part, version_constraint) =
        if let Some((name, constraint)) = req_without_ns.split_once(' ') {
            (name.trim(), Some(constraint.trim()))
        } else {
            (req_without_ns, None)
        };

    // Check for pinned version with `=` (no space): package=1.0
    let (name, version, is_pinned, extracted_requirement) = if name_part.contains('=') {
        let parts: Vec<&str> = name_part.splitn(2, '=').collect();
        let n = parts[0].trim();
        let v = if parts.len() > 1 {
            let parsed = parts[1].trim();
            if parsed.is_empty() {
                None
            } else {
                Some(parsed.to_string())
            }
        } else {
            None
        };
        let req = v
            .as_ref()
            .map(|ver| format!("={}", ver))
            .unwrap_or_default();
        (n, v, true, Some(req))
    } else if let Some(constraint) = version_constraint {
        // Handle space-separated constraints: package >=3.6, package ==1.0
        let version_opt = if constraint.starts_with("==") {
            Some(constraint.trim_start_matches("==").trim().to_string())
        } else {
            None
        };
        (
            name_part.trim(),
            version_opt,
            false,
            Some(constraint.to_string()),
        )
    } else {
        (name_part.trim(), None, false, Some(String::new()))
    };

    // Build PURL
    let purl = build_purl(
        "conda",
        namespace,
        name,
        version.as_deref(),
        None,
        None,
        None,
    );

    // Determine is_runtime and is_optional based on scope
    let (is_runtime, is_optional) = match scope {
        "run" => (true, false),
        _ => (false, true), // build, host, test are all optional
    };

    let mut extra_data = HashMap::new();
    if let Some(namespace) = namespace {
        extra_data.insert("channel".to_string(), serde_json::json!(namespace));
    }
    if let Some(channel_url) = channel_url {
        extra_data.insert("channel_url".to_string(), serde_json::json!(channel_url));
    }

    Some(Dependency {
        purl,
        extracted_requirement,
        scope: Some(scope.to_string()),
        is_runtime: Some(is_runtime),
        is_optional: Some(is_optional),
        is_pinned: Some(is_pinned),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: (!extra_data.is_empty()).then_some(extra_data),
    })
}

fn extract_environment_dependencies(yaml: &Value) -> Vec<Dependency> {
    let dependencies = match yaml.get("dependencies").and_then(|v| v.as_sequence()) {
        Some(d) => d,
        None => return Vec::new(),
    };

    let mut deps = Vec::new();
    for dep_value in dependencies {
        if let Some(dep_str) = dep_value.as_str() {
            if let Some(dep) = parse_environment_string_dependency(dep_str) {
                deps.push(dep);
            }
        } else if let Some(pip_deps) = dep_value.get("pip").and_then(|v| v.as_sequence()) {
            deps.extend(extract_pip_dependencies(pip_deps));
        }
    }
    deps
}

fn parse_environment_string_dependency(dep_str: &str) -> Option<Dependency> {
    let (namespace, channel_url, dep_without_ns) = parse_conda_channel_prefix(dep_str);
    create_conda_dependency(namespace, channel_url, dep_without_ns, "dependencies")
}

fn parse_conda_channel_prefix(dep_str: &str) -> (Option<&str>, Option<&str>, &str) {
    if let Some((ns, rest)) = dep_str.rsplit_once("::") {
        if ns.contains('/') || ns.contains(':') {
            (None, Some(ns), rest)
        } else {
            (Some(ns), None, rest)
        }
    } else {
        (None, None, dep_str)
    }
}

fn create_conda_dependency(
    namespace: Option<&str>,
    channel_url: Option<&str>,
    dep_without_ns: &str,
    scope: &str,
) -> Option<Dependency> {
    let dep = dep_without_ns.trim();
    let name_re = match Regex::new(r"^([A-Za-z0-9_.\-]+)") {
        Ok(re) => re,
        Err(_) => return None,
    };

    let caps = name_re.captures(dep)?;
    let name_match = caps.get(1)?;
    let name = name_match.as_str().trim();
    let rest = dep[name_match.end()..].trim();

    let (version, is_pinned, extracted_requirement) = if rest.is_empty() {
        (None, false, Some(String::new()))
    } else {
        let req_no_space = rest.replace(' ', "");
        let is_exact = req_no_space.starts_with("=") || req_no_space.starts_with("==");
        let parsed_version = if is_exact {
            Some(
                req_no_space
                    .trim_start_matches('=')
                    .trim_start_matches('=')
                    .to_string(),
            )
        } else {
            None
        };

        (parsed_version, is_exact, Some(rest.to_string()))
    };

    if name == "pip" || name == "python" {
        return None;
    }

    let purl = build_purl(
        "conda",
        namespace,
        name,
        version.as_deref(),
        None,
        None,
        None,
    );
    let mut extra_data = HashMap::new();
    if let Some(namespace) = namespace {
        extra_data.insert("channel".to_string(), serde_json::json!(namespace));
    }
    if let Some(channel_url) = channel_url {
        extra_data.insert("channel_url".to_string(), serde_json::json!(channel_url));
    }

    Some(Dependency {
        purl,
        extracted_requirement,
        scope: Some(scope.to_string()),
        is_runtime: Some(true),
        is_optional: Some(false),
        is_pinned: Some(is_pinned),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: (!extra_data.is_empty()).then_some(extra_data),
    })
}

fn extract_pip_dependencies(pip_deps: &[Value]) -> Vec<Dependency> {
    pip_deps
        .iter()
        .filter_map(|pip_dep| {
            if let Some(pip_req_str) = pip_dep.as_str()
                && let Ok(parsed_req) = pip_req_str.parse::<pep508_rs::Requirement>()
            {
                create_pip_dependency(parsed_req, "dependencies", Some(pip_req_str))
            } else {
                None
            }
        })
        .collect()
}

fn create_pip_dependency(
    parsed_req: pep508_rs::Requirement,
    scope: &str,
    raw_requirement: Option<&str>,
) -> Option<Dependency> {
    let name = parsed_req.name.to_string();

    if name == "pip" || name == "python" {
        return None;
    }

    let specs = parsed_req.version_or_url.as_ref().map(|v| match v {
        pep508_rs::VersionOrUrl::VersionSpecifier(spec) => spec.to_string(),
        pep508_rs::VersionOrUrl::Url(url) => url.to_string(),
    });

    let extracted_requirement = if let Some(raw) = raw_requirement {
        let raw = raw.trim();
        let suffix = raw.strip_prefix(&name).unwrap_or(raw).trim().to_string();
        Some(suffix)
    } else {
        Some(specs.clone().unwrap_or_default())
    };

    let version = specs.as_ref().and_then(|spec_str| {
        if spec_str.starts_with("==") {
            Some(spec_str.trim_start_matches("==").to_string())
        } else {
            None
        }
    });

    let is_pinned = specs.as_ref().map(|s| s.contains("==")).unwrap_or(false);
    let purl = build_purl("pypi", None, &name, version.as_deref(), None, None, None);

    Some(Dependency {
        purl,
        extracted_requirement,
        scope: Some(scope.to_string()),
        is_runtime: Some(true),
        is_optional: Some(false),
        is_pinned: Some(is_pinned),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: None,
    })
}

crate::register_parser!(
    "Conda package manifest and environment file",
    &[
        "**/meta.yaml",
        "**/meta.yml",
        "**/environment.yml",
        "**/environment.yaml",
        "**/env.yaml",
        "**/env.yml",
        "**/conda.yaml",
        "**/conda.yml",
        "**/*conda*.yaml",
        "**/*conda*.yml",
        "**/*env*.yaml",
        "**/*env*.yml",
        "**/*environment*.yaml",
        "**/*environment*.yml"
    ],
    "conda",
    "Python",
    Some("https://docs.conda.io/"),
);
