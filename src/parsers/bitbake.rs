use std::collections::HashMap;
use std::path::Path;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};
use crate::parser_warn as warn;
use packageurl::PackageUrl;

use super::PackageParser;
use super::license_normalization::normalize_spdx_declared_license;
use super::utils::{read_file_to_string, truncate_field};

pub struct BitbakeRecipeParser;

impl PackageParser for BitbakeRecipeParser {
    const PACKAGE_TYPE: PackageType = PackageType::Bitbake;

    fn is_match(path: &Path) -> bool {
        path.extension().is_some_and(|ext| ext == "bb")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(content) => content,
            Err(error) => {
                warn!("Failed to read BitBake recipe at {:?}: {}", path, error);
                return vec![default_package_data()];
            }
        };

        vec![parse_recipe(&content, path)]
    }
}

fn parse_recipe(content: &str, path: &Path) -> PackageData {
    let vars = extract_variables(content);
    let (filename_name, filename_version) = parse_recipe_filename(path);

    let mut package = default_package_data();
    let mut extra_data: HashMap<String, serde_json::Value> = HashMap::new();

    let name = vars
        .get("PN")
        .cloned()
        .or(filename_name)
        .map(truncate_field);
    let version = vars
        .get("PV")
        .cloned()
        .or(filename_version)
        .map(truncate_field);

    package.name = name.clone();
    package.version = version.clone();

    if let Some(summary) = vars.get("SUMMARY") {
        package.description = Some(truncate_field(summary.clone()));
    } else if let Some(description) = vars.get("DESCRIPTION") {
        package.description = Some(truncate_field(description.clone()));
    }

    if let Some(homepage) = vars.get("HOMEPAGE") {
        package.homepage_url = Some(truncate_field(homepage.clone()));
    }

    if let Some(bugtracker) = vars.get("BUGTRACKER") {
        package.bug_tracking_url = Some(truncate_field(bugtracker.clone()));
    }

    if let Some(license) = vars.get("LICENSE") {
        let normalized = normalize_bitbake_license(license);
        package.extracted_license_statement = Some(truncate_field(normalized.clone()));
        let (declared, spdx, detections) =
            normalize_spdx_declared_license(Some(normalized.as_str()));
        package.declared_license_expression = declared;
        package.declared_license_expression_spdx = spdx;
        package.license_detections = detections;
    }

    if let Some(section) = vars.get("SECTION") {
        extra_data.insert(
            "section".to_string(),
            serde_json::Value::String(section.clone()),
        );
    }

    if let Some(src_uri) = vars.get("SRC_URI") {
        let uris: Vec<String> = src_uri
            .split_whitespace()
            .filter(|s| !s.starts_with("file://"))
            .map(|s| s.split(';').next().unwrap_or(s).to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !uris.is_empty() {
            extra_data.insert(
                "src_uri".to_string(),
                serde_json::Value::Array(uris.into_iter().map(serde_json::Value::String).collect()),
            );
        }
    }

    let inherits = extract_inherits(content);
    if !inherits.is_empty() {
        extra_data.insert(
            "inherit".to_string(),
            serde_json::Value::Array(
                inherits
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
    }

    let mut dependencies = Vec::new();

    if let Some(depends) = vars.get("DEPENDS") {
        for dep_name in parse_dependency_list(depends) {
            dependencies.push(Dependency {
                purl: build_dependency_purl(&dep_name),
                extracted_requirement: None,
                scope: Some("build".to_string()),
                is_runtime: Some(false),
                is_optional: None,
                is_pinned: None,
                is_direct: Some(true),
                resolved_package: None,
                extra_data: None,
            });
        }
    }

    for (key, value) in &vars {
        if is_rdepends_key(key) {
            for dep_name in parse_dependency_list(value) {
                dependencies.push(Dependency {
                    purl: build_dependency_purl(&dep_name),
                    extracted_requirement: None,
                    scope: Some("runtime".to_string()),
                    is_runtime: Some(true),
                    is_optional: None,
                    is_pinned: None,
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                });
            }
        }
    }

    package.dependencies = dependencies;
    package.extra_data = (!extra_data.is_empty()).then_some(extra_data);
    package.purl = name
        .as_deref()
        .and_then(|n| build_package_purl(n, version.as_deref()));

    package
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(PackageType::Bitbake),
        datasource_id: Some(DatasourceId::BitbakeRecipe),
        ..Default::default()
    }
}

fn parse_recipe_filename(path: &Path) -> (Option<String>, Option<String>) {
    let stem = match path.file_stem().and_then(|s| s.to_str()) {
        Some(s) => s,
        None => return (None, None),
    };

    match stem.split_once('_') {
        Some((name, version)) if !name.is_empty() && !version.is_empty() => {
            (Some(name.to_string()), Some(version.to_string()))
        }
        _ => (Some(stem.to_string()), None),
    }
}

fn extract_variables(content: &str) -> HashMap<String, String> {
    let mut vars: HashMap<String, String> = HashMap::new();
    let mut lines = content.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let mut full_line = trimmed.to_string();
        while full_line.ends_with('\\') {
            full_line.truncate(full_line.len() - 1);
            if let Some(next) = lines.next() {
                full_line.push(' ');
                full_line.push_str(next.trim());
            } else {
                break;
            }
        }

        if let Some((var_name, value, op)) = parse_assignment(&full_line) {
            let cleaned = strip_quotes(&value);
            match op {
                AssignOp::Set | AssignOp::Immediate => {
                    vars.insert(var_name, cleaned);
                }
                AssignOp::WeakSet | AssignOp::WeakDefault => {
                    vars.entry(var_name).or_insert(cleaned);
                }
                AssignOp::Append => {
                    vars.entry(var_name.clone())
                        .and_modify(|v| {
                            v.push(' ');
                            v.push_str(&cleaned);
                        })
                        .or_insert(cleaned);
                }
                AssignOp::Prepend => {
                    vars.entry(var_name.clone())
                        .and_modify(|v| {
                            let mut new = cleaned.clone();
                            new.push(' ');
                            new.push_str(v);
                            *v = new;
                        })
                        .or_insert(cleaned);
                }
                AssignOp::AppendNoSpace => {
                    vars.entry(var_name.clone())
                        .and_modify(|v| v.push_str(&cleaned))
                        .or_insert(cleaned);
                }
                AssignOp::PrependNoSpace => {
                    vars.entry(var_name.clone())
                        .and_modify(|v| {
                            let mut new = cleaned.clone();
                            new.push_str(v);
                            *v = new;
                        })
                        .or_insert(cleaned);
                }
            }
        }
    }

    vars
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum AssignOp {
    Set,
    Immediate,
    WeakSet,
    WeakDefault,
    Append,
    Prepend,
    AppendNoSpace,
    PrependNoSpace,
}

fn parse_assignment(line: &str) -> Option<(String, String, AssignOp)> {
    let operators: &[(&str, AssignOp)] = &[
        ("??=", AssignOp::WeakDefault),
        ("?=", AssignOp::WeakSet),
        (":=", AssignOp::Immediate),
        ("+=", AssignOp::Append),
        ("=+", AssignOp::Prepend),
        (".=", AssignOp::AppendNoSpace),
        ("=.", AssignOp::PrependNoSpace),
        ("=", AssignOp::Set),
    ];

    for (op_str, op) in operators {
        if let Some(pos) = line.find(op_str) {
            let var_part = line[..pos].trim();
            if var_part.is_empty() || !is_valid_var_name(var_part) {
                continue;
            }
            let value = line[pos + op_str.len()..].trim().to_string();
            return Some((var_part.to_string(), value, *op));
        }
    }
    None
}

fn is_valid_var_name(s: &str) -> bool {
    let base = s.split([':', '[']).next().unwrap_or(s);
    !base.is_empty()
        && base
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '{' || c == '}')
}

fn strip_quotes(s: &str) -> String {
    let trimmed = s.trim();
    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
    {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

fn extract_inherits(content: &str) -> Vec<String> {
    let mut inherits = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("inherit ") {
            for class in rest.split_whitespace() {
                if !class.starts_with('#') {
                    inherits.push(class.to_string());
                } else {
                    break;
                }
            }
        }
    }
    inherits
}

fn is_rdepends_key(key: &str) -> bool {
    key == "RDEPENDS"
        || key.starts_with("RDEPENDS:")
        || key.starts_with("RDEPENDS_")
        || key.starts_with("RDEPENDS[")
}

fn parse_dependency_list(value: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let mut chars = value.chars().peekable();
    let mut current = String::new();

    while let Some(ch) = chars.next() {
        match ch {
            ' ' | '\t' | '\n' => {
                if !current.is_empty() {
                    deps.push(std::mem::take(&mut current));
                }
            }
            '(' => {
                while let Some(&c) = chars.peek() {
                    chars.next();
                    if c == ')' {
                        break;
                    }
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        deps.push(current);
    }

    deps.into_iter()
        .filter(|d| !d.starts_with("${") && !d.contains("$"))
        .collect()
}

fn normalize_bitbake_license(license: &str) -> String {
    let mut result = String::with_capacity(license.len());
    let mut chars = license.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '&' {
            let trimmed = result.trim_end();
            result.truncate(trimmed.len());
            result.push_str(" AND ");
            while chars.peek() == Some(&' ') {
                chars.next();
            }
        } else if ch == '|' {
            let trimmed = result.trim_end();
            result.truncate(trimmed.len());
            result.push_str(" OR ");
            while chars.peek() == Some(&' ') {
                chars.next();
            }
        } else {
            result.push(ch);
        }
    }
    result
}

fn build_package_purl(name: &str, version: Option<&str>) -> Option<String> {
    let mut purl = PackageUrl::new(PackageType::Bitbake.as_str(), name).ok()?;
    if let Some(v) = version {
        purl.with_version(v).ok()?;
    }
    Some(truncate_field(purl.to_string()))
}

fn build_dependency_purl(name: &str) -> Option<String> {
    PackageUrl::new(PackageType::Bitbake.as_str(), name)
        .ok()
        .map(|purl| truncate_field(purl.to_string()))
}

crate::register_parser!(
    "Yocto BitBake recipe",
    &["**/*.bb"],
    "bitbake",
    "",
    Some(
        "https://docs.yoctoproject.org/bitbake/bitbake-user-manual/bitbake-user-manual-metadata.html"
    ),
);
