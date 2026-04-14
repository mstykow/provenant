//! Parser for RPM .spec files.
//!
//! Extracts package metadata from RPM specfiles, which define how RPM packages
//! are built. This is a beyond-parity implementation - the Python version is
//! a complete stub with "TODO: implement me!!" comments.
//!
//! # Supported Formats
//! - *.spec (RPM specfiles)
//!
//! # Key Features
//! - Preamble tag extraction (Name, Version, Release, Summary, License, etc.)
//! - Dependency extraction (BuildRequires, Requires, Provides)
//! - %description section parsing
//! - Basic macro expansion (%{name}, %{version}, %{release})
//! - %define and %global macro definitions
//! - Conditional macro handling (%{?dist})
//! - Multi-line dependency lists (comma-separated)
//! - Scoped Requires (Requires(post), Requires(preun), etc.)
//!
//! # Implementation Notes
//! - Parses only the preamble (before %prep, %build, etc. sections)
//! - Tags are case-insensitive per RPM spec format
//! - Simple macro expansion for common patterns
//! - BuildRequires dependencies have is_runtime=false, scope="build"
//! - Runtime Requires dependencies have is_runtime=true, scope="runtime"
//! - datasource_id is "rpm_specfile"

use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use regex::Regex;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType, Party};
use crate::parsers::utils::{
    MAX_ITERATION_COUNT, read_file_to_string, split_name_email, truncate_field,
};

use super::PackageParser;

static RE_CONDITIONAL_MACRO: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"%\{\?[^}]+\}").unwrap());

const PACKAGE_TYPE: PackageType = PackageType::Rpm;

/// Parser for RPM specfiles
pub struct RpmSpecfileParser;

impl PackageParser for RpmSpecfileParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("spec"))
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read RPM specfile {:?}: {}", path, e);
                return vec![PackageData {
                    package_type: Some(PACKAGE_TYPE),
                    datasource_id: Some(DatasourceId::RpmSpecfile),
                    ..Default::default()
                }];
            }
        };

        vec![parse_specfile(&content)]
    }
}

fn parse_specfile(content: &str) -> PackageData {
    let mut tags: HashMap<String, String> = HashMap::new();
    let mut macros: HashMap<String, String> = HashMap::new();
    let mut build_requires: Vec<String> = Vec::new();
    let mut requires: Vec<(String, Option<String>)> = Vec::new(); // (requirement, scope)
    let mut provides: Vec<String> = Vec::new();
    let mut description: Option<String> = None;

    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;
    let mut iterations: usize = 0;

    while i < lines.len() {
        iterations += 1;
        if iterations > MAX_ITERATION_COUNT {
            warn!(
                "RPM specfile preamble iteration limit ({}) exceeded",
                MAX_ITERATION_COUNT
            );
            break;
        }
        let line = lines[i].trim();

        // Stop at first section marker (%, but not %define/%global)
        if line.starts_with('%') && !line.starts_with("%define") && !line.starts_with("%global") {
            if is_conditional_preamble_directive(line) {
                i += 1;
                continue;
            }
            break;
        }

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            i += 1;
            continue;
        }

        // Parse %define and %global macros
        if let Some(stripped) = line
            .strip_prefix("%define")
            .or(line.strip_prefix("%global"))
        {
            let parts: Vec<&str> = stripped.trim().splitn(2, char::is_whitespace).collect();
            if parts.len() == 2 {
                macros.insert(
                    parts[0].to_string(),
                    truncate_field(parts[1].trim().to_string()),
                );
            }
            i += 1;
            continue;
        }

        // Parse Tag: Value lines
        if let Some(colon_pos) = line.find(':') {
            let tag = line[..colon_pos].trim().to_lowercase();
            let value = line[colon_pos + 1..].trim().to_string();

            match tag.as_str() {
                "buildrequires" => {
                    for dep in value.split(',').take(MAX_ITERATION_COUNT) {
                        let dep = dep.trim();
                        if !dep.is_empty() {
                            build_requires.push(dep.to_string());
                        }
                    }
                }
                t if t.starts_with("requires") => {
                    // Parse Requires, Requires(post), Requires(preun), etc.
                    let scope = if let Some(start) = t.find('(') {
                        if let Some(end) = t.find(')') {
                            Some(t[start + 1..end].to_string())
                        } else {
                            Some("runtime".to_string())
                        }
                    } else {
                        Some("runtime".to_string())
                    };

                    for dep in value.split(',').take(MAX_ITERATION_COUNT) {
                        let dep = dep.trim();
                        if !dep.is_empty() {
                            requires.push((dep.to_string(), scope.clone()));
                        }
                    }
                }
                "provides" => {
                    for prov in value.split(',').take(MAX_ITERATION_COUNT) {
                        let prov = prov.trim();
                        if !prov.is_empty() {
                            provides.push(prov.to_string());
                        }
                    }
                }
                _ => {
                    tags.insert(tag, value);
                }
            }
        }

        i += 1;
    }

    // Now parse %description section if present
    let mut desc_iterations: usize = 0;
    while i < lines.len() {
        desc_iterations += 1;
        if desc_iterations > MAX_ITERATION_COUNT {
            warn!(
                "RPM specfile description search iteration limit ({}) exceeded",
                MAX_ITERATION_COUNT
            );
            break;
        }
        let line = lines[i].trim();

        if line.starts_with("%description") {
            i += 1;
            let mut desc_lines = Vec::new();

            while i < lines.len() {
                desc_iterations += 1;
                if desc_iterations > MAX_ITERATION_COUNT {
                    warn!(
                        "RPM specfile description iteration limit ({}) exceeded",
                        MAX_ITERATION_COUNT
                    );
                    break;
                }
                let desc_line = lines[i];
                let trimmed = desc_line.trim();

                // Stop at next section
                if trimmed.starts_with('%') {
                    break;
                }

                // Don't include empty lines at start
                if !desc_lines.is_empty() || !trimmed.is_empty() {
                    desc_lines.push(desc_line);
                }

                i += 1;
            }

            // Trim trailing empty lines
            while desc_lines.last().is_some_and(|l| l.trim().is_empty()) {
                desc_lines.pop();
            }

            if !desc_lines.is_empty() {
                description = Some(desc_lines.join("\n"));
            }

            break;
        }

        i += 1;
    }

    // Extract basic metadata from tags
    let name = tags.get("name").cloned();
    let version = tags.get("version").cloned();
    let release = tags.get("release").cloned();

    // Store name and version in macros for expansion
    if let Some(ref n) = name {
        macros.insert("name".to_string(), n.clone());
    }
    if let Some(ref v) = version {
        macros.insert("version".to_string(), v.clone());
    }
    if let Some(ref r) = release {
        macros.insert("release".to_string(), r.clone());
    }

    // Expand macros in all tag values
    let mut expanded_tags: HashMap<String, String> = HashMap::new();
    for (tag, value) in tags.iter() {
        expanded_tags.insert(tag.clone(), truncate_field(expand_macros(value, &macros)));
    }

    // Get expanded values
    let name = expanded_tags.get("name").cloned();
    let version = expanded_tags.get("version").cloned();
    let release = expanded_tags.get("release").cloned();
    let summary = expanded_tags.get("summary").cloned();
    let license = expanded_tags.get("license").cloned();
    let url = expanded_tags.get("url").cloned();
    let group = expanded_tags.get("group").cloned();
    let epoch = expanded_tags.get("epoch").cloned();
    let packager = expanded_tags.get("packager").cloned();

    let download_url = expanded_tags
        .get("source")
        .or_else(|| expanded_tags.get("source0"))
        .cloned()
        .map(truncate_field);

    // Create parties
    let mut parties = Vec::new();
    if let Some(pkg) = packager {
        let (name_opt, email_opt) = split_name_email(&pkg);
        parties.push(Party {
            r#type: None,
            role: Some("packager".to_string()),
            name: name_opt,
            email: email_opt,
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }

    // Create dependencies
    let mut dependencies = Vec::new();

    for dep_str in build_requires.into_iter().take(MAX_ITERATION_COUNT) {
        let dep_str = truncate_field(expand_macros(&dep_str, &macros));
        let dep_name = extract_dep_name(&dep_str);
        let purl = build_rpm_purl(&dep_name, None).map(truncate_field);

        dependencies.push(Dependency {
            purl,
            extracted_requirement: Some(dep_str),
            scope: Some("build".to_string()),
            is_runtime: Some(false),
            is_optional: Some(false),
            is_direct: Some(true),
            is_pinned: None,
            resolved_package: None,
            extra_data: None,
        });
    }

    for (dep_str, scope) in requires.into_iter().take(MAX_ITERATION_COUNT) {
        let dep_str = truncate_field(expand_macros(&dep_str, &macros));
        let dep_name = extract_dep_name(&dep_str);
        let purl = build_rpm_purl(&dep_name, None).map(truncate_field);

        dependencies.push(Dependency {
            purl,
            extracted_requirement: Some(dep_str),
            scope,
            is_runtime: Some(true),
            is_optional: Some(false),
            is_direct: Some(true),
            is_pinned: None,
            resolved_package: None,
            extra_data: None,
        });
    }

    // Build PURL
    let purl = name
        .as_ref()
        .and_then(|n| build_rpm_purl(n, version.as_deref()))
        .map(truncate_field);

    // Build extra_data for non-standard fields
    let mut extra_data = HashMap::new();
    if let Some(r) = release {
        extra_data.insert("release".to_string(), serde_json::Value::String(r));
    }
    if let Some(e) = epoch {
        extra_data.insert("epoch".to_string(), serde_json::Value::String(e));
    }
    if let Some(g) = group {
        extra_data.insert("group".to_string(), serde_json::Value::String(g));
    }
    if !provides.is_empty() {
        let provides_json: Vec<serde_json::Value> = provides
            .into_iter()
            .take(MAX_ITERATION_COUNT)
            .map(|prov| serde_json::Value::String(truncate_field(expand_macros(&prov, &macros))))
            .collect();
        extra_data.insert(
            "provides".to_string(),
            serde_json::Value::Array(provides_json),
        );
    }

    let extra_data_opt = if extra_data.is_empty() {
        None
    } else {
        Some(extra_data)
    };

    // Use %description if available, otherwise use Summary
    let description_text = description.map(truncate_field).or(summary);

    PackageData {
        datasource_id: Some(DatasourceId::RpmSpecfile),
        package_type: Some(PACKAGE_TYPE),
        namespace: None, // RPM namespace is optional
        name,
        version,
        description: description_text,
        homepage_url: url,
        download_url,
        extracted_license_statement: license,
        parties,
        dependencies,
        purl,
        extra_data: extra_data_opt,
        ..Default::default()
    }
}

fn is_conditional_preamble_directive(line: &str) -> bool {
    [
        "%if", "%ifarch", "%ifnarch", "%ifos", "%ifnos", "%elif", "%else", "%endif",
    ]
    .iter()
    .any(|directive| line.starts_with(directive))
}

/// Expands simple macros in a string (%{name}, %{version}, %{release}, %{?dist})
fn expand_macros(s: &str, macros: &HashMap<String, String>) -> String {
    let mut result = s.to_string();

    result = RE_CONDITIONAL_MACRO.replace_all(&result, "").to_string();

    // Expand simple macros %{macro}
    for (key, value) in macros {
        let pattern = format!("%{{{}}}", key);
        result = result.replace(&pattern, value);
    }

    result = RE_CONDITIONAL_MACRO.replace_all(&result, "").to_string();

    result
}

/// Extracts the package name from a dependency string (removes version constraints)
fn extract_dep_name(dep: &str) -> String {
    let parts: Vec<&str> = dep.split(&['>', '<', '='][..]).map(|s| s.trim()).collect();

    truncate_field(parts[0].to_string())
}

/// Builds a package URL for RPM packages
fn build_rpm_purl(name: &str, version: Option<&str>) -> Option<String> {
    let mut purl = PackageUrl::new(PACKAGE_TYPE.as_str(), name).ok()?;

    if let Some(ver) = version {
        purl.with_version(ver).ok()?;
    }

    Some(purl.to_string())
}

crate::register_parser!(
    "RPM specfile",
    &["**/*.spec"],
    "rpm",
    "",
    Some("https://rpm-software-management.github.io/rpm/manual/spec.html"),
);
