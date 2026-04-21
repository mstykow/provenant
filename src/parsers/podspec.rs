// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Parser for CocoaPods .podspec manifest files.
//!
//! Extracts package metadata and dependencies from .podspec files which define
//! CocoaPods package specifications using Ruby DSL syntax.
//!
//! # Supported Formats
//! - *.podspec (CocoaPods package specification files)
//! - .podspec files (same format, different naming convention)
//!
//! # Key Features
//! - Metadata extraction (name, version, summary, description, license)
//! - Author/contributor information parsing with email handling
//! - Homepage and source repository URL extraction
//! - Dependency declaration parsing with version constraints
//! - Support for development dependencies
//! - Regex-based Ruby DSL parsing (no full Ruby AST required)
//!
//! # Implementation Notes
//! - Uses regex for pattern matching in Ruby DSL syntax
//! - Supports multi-line string values and Ruby hash syntax
//! - Dependency version constraints are parsed from DSL
//! - Graceful error handling with `warn!()` logs on parse failures

use std::path::Path;
use std::sync::LazyLock;

use crate::parser_warn as warn;
use md5::{Digest, Md5};
use packageurl::PackageUrl;
use regex::Regex;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType, Party};
use crate::parsers::PackageParser;
use crate::parsers::license_normalization::normalize_spdx_declared_license;
use crate::parsers::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};

/// Parses CocoaPods specification files (.podspec).
///
/// Extracts package metadata from .podspec files using regex-based Ruby DSL parsing.
///
/// # Extracted Fields
/// - Name, version, summary, description
/// - Homepage, license, source URLs
/// - Author information (including author hashes)
/// - Dependencies with version constraints
///
/// # Heredoc Support
/// Handles multiline descriptions: `s.description = <<-DESC ... DESC`
pub struct PodspecParser;

impl PackageParser for PodspecParser {
    const PACKAGE_TYPE: PackageType = PackageType::Cocoapods;

    fn is_match(path: &Path) -> bool {
        path.extension().is_some_and(|ext| {
            ext == "podspec"
                && path
                    .file_name()
                    .is_some_and(|name| !name.to_string_lossy().ends_with(".json.podspec"))
        })
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        let name = extract_field(&content, &NAME_PATTERN).map(truncate_field);
        let version = extract_field(&content, &VERSION_PATTERN).map(truncate_field);
        let summary = extract_field(&content, &SUMMARY_PATTERN).map(truncate_field);
        let description =
            merge_summary_and_description(summary.as_deref(), extract_description(&content))
                .map(truncate_field);
        let homepage_url = extract_field(&content, &HOMEPAGE_PATTERN).map(truncate_field);
        let license = extract_license_statement(&content).map(truncate_field);
        let (declared_license_expression, declared_license_expression_spdx, license_detections) =
            normalize_podspec_declared_license(&content, license.as_deref());
        let source = extract_source_url(&content).map(truncate_field);
        let authors = extract_authors(&content);

        let parties = authors
            .into_iter()
            .map(|(name, email)| Party {
                r#type: Some("person".to_string()),
                name: Some(truncate_field(name)),
                email: email.map(truncate_field),
                url: None,
                role: Some("author".to_string()),
                organization: None,
                organization_url: None,
                timezone: None,
            })
            .collect();

        let dependencies = extract_dependencies(&content);
        let mut extra_data = serde_json::Map::new();
        if let Some(raw_license) = extract_field(&content, &LICENSE_PATTERN)
            && let Some(license_file) = extract_ruby_hash_file(&raw_license)
        {
            extra_data.insert(
                "license_file".to_string(),
                serde_json::Value::String(license_file),
            );
        }
        let repository_homepage_url = name
            .as_ref()
            .map(|n| format!("https://cocoapods.org/pods/{}", n));
        let repository_download_url = match (source.as_deref(), version.as_deref()) {
            (Some(vcs_url), Some(version_str)) => get_repo_base_url(vcs_url)
                .map(|base| format!("{}/archive/refs/tags/{}.zip", base, version_str)),
            _ => None,
        };
        let code_view_url = match (source.as_deref(), version.as_deref()) {
            (Some(vcs_url), Some(version_str)) => {
                get_repo_base_url(vcs_url).map(|base| format!("{}/tree/{}", base, version_str))
            }
            _ => None,
        };
        let bug_tracking_url = source
            .as_deref()
            .and_then(get_repo_base_url)
            .map(|base| format!("{}/issues/", base));
        let api_data_url = match (name.as_deref(), version.as_deref()) {
            (Some(name_str), Some(version_str)) => get_hashed_path(name_str).map(|hashed| {
                format!(
                    "https://raw.githubusercontent.com/CocoaPods/Specs/blob/master/Specs/{}/{}/{}/{}.podspec.json",
                    hashed, name_str, version_str, name_str
                )
            }),
            _ => None,
        };
        let purl = if let Some(name_str) = &name {
            let purl_result = PackageUrl::new(Self::PACKAGE_TYPE.as_str(), name_str)
                .or_else(|_| PackageUrl::new("generic", name_str));
            match purl_result {
                Ok(mut purl) => {
                    if let Some(version_str) = &version {
                        let _ = purl.with_version(version_str);
                    }
                    Some(truncate_field(purl.to_string()))
                }
                Err(_) => None,
            }
        } else {
            None
        };

        vec![PackageData {
            package_type: Some(Self::PACKAGE_TYPE),
            namespace: None,
            name,
            version,
            qualifiers: None,
            subpath: None,
            primary_language: Some("Objective-C".to_string()),
            description,
            release_date: None,
            parties,
            keywords: Vec::new(),
            homepage_url,
            download_url: None,
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            bug_tracking_url,
            code_view_url,
            vcs_url: source,
            copyright: None,
            holder: None,
            declared_license_expression,
            declared_license_expression_spdx,
            license_detections,
            other_license_expression: None,
            other_license_expression_spdx: None,
            other_license_detections: Vec::new(),
            extracted_license_statement: license,
            notice_text: None,
            source_packages: Vec::new(),
            file_references: Vec::new(),
            extra_data: (!extra_data.is_empty()).then_some(extra_data.into_iter().collect()),
            dependencies,
            repository_homepage_url,
            repository_download_url,
            api_data_url,
            datasource_id: Some(DatasourceId::CocoapodsPodspec),
            purl,
            is_private: false,
            is_virtual: false,
        }]
    }
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(PodspecParser::PACKAGE_TYPE),
        primary_language: Some("Objective-C".to_string()),
        datasource_id: Some(DatasourceId::CocoapodsPodspec),
        ..Default::default()
    }
}

static NAME_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.name\s*=\s*(.+)").expect("valid regex"));
static VERSION_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.version\s*=\s*(.+)").expect("valid regex"));
static SUMMARY_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.summary\s*=\s*(.+)").expect("valid regex"));
static DESCRIPTION_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.description\s*=\s*(.+)").expect("valid regex"));
static HOMEPAGE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.homepage\s*=\s*(.+)").expect("valid regex"));
static LICENSE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.license\s*=\s*(.+)").expect("valid regex"));
static SOURCE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.source\s*=\s*(.+)").expect("valid regex"));
static AUTHOR_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.authors?\s*=\s*(.+)").expect("valid regex"));
static SOURCE_GIT_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#":git\s*=>\s*['\"]([^'\"]+)['\"]"#).expect("valid regex"));
static SOURCE_HTTP_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#":http\s*=>\s*['\"]([^'\"]+)['\"]"#).expect("valid regex"));

static DEPENDENCY_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
    r#"(?:s\.)?(?:dependency|add_dependency|add_(?:runtime|development)_dependency)\s+['"]([^'"]+)['"](?:\s*,\s*(.+))?"#
).expect("valid regex")
});

fn extract_license_statement(content: &str) -> Option<String> {
    extract_field(content, &LICENSE_PATTERN).map(|value| normalize_ruby_hash_literal(&value))
}

fn normalize_podspec_declared_license(
    content: &str,
    extracted_license_statement: Option<&str>,
) -> (
    Option<String>,
    Option<String>,
    Vec<crate::models::LicenseDetection>,
) {
    let Some(raw_license) = extract_field(content, &LICENSE_PATTERN) else {
        return super::license_normalization::empty_declared_license_data();
    };
    let normalized_candidate = if raw_license.contains("=>") || raw_license.contains('=') {
        extract_ruby_hash_type(&raw_license)
            .map(|license_type| canonicalize_cocoapods_license_type(&license_type))
    } else {
        extracted_license_statement.map(canonicalize_cocoapods_license_type)
    };

    normalize_spdx_declared_license(normalized_candidate.as_deref())
}

fn extract_ruby_hash_file(raw_license: &str) -> Option<String> {
    let normalized = raw_license.replace("=>", "=");
    let file_regex = Regex::new(r#":file\s*=\s*['\"]([^'\"]+)['\"]"#).ok()?;
    file_regex
        .captures(&normalized)
        .and_then(|caps| caps.get(1))
        .map(|value| value.as_str().trim().to_string())
        .filter(|value| !value.is_empty())
}

fn canonicalize_cocoapods_license_type(value: &str) -> String {
    match value.trim() {
        "Apache License, Version 2.0" => "Apache-2.0".to_string(),
        other => other.to_string(),
    }
}

fn extract_ruby_hash_type(raw_license: &str) -> Option<String> {
    let normalized = raw_license.replace("=>", "=");
    let type_regex = Regex::new(r#":type\s*=\s*['\"]([^'\"]+)['\"]"#).ok()?;
    type_regex
        .captures(&normalized)
        .and_then(|caps| caps.get(1))
        .map(|value| value.as_str().trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_ruby_hash_literal(value: &str) -> String {
    if !value.contains('=') && !value.contains("=>") {
        return value.to_string();
    }

    value
        .replace("=>", "=")
        .replace(['\'', '"'], "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Extract a single field using a regex pattern
fn extract_field(content: &str, pattern: &Regex) -> Option<String> {
    for line in content.lines().take(MAX_ITERATION_COUNT) {
        let cleaned_line = pre_process(line);
        if let Some(value) = pattern.captures(&cleaned_line).and_then(|caps| caps.get(1)) {
            return Some(clean_string(value.as_str()));
        }
    }
    None
}

/// Extract description, handling multiline heredoc format
fn extract_description(content: &str) -> Option<String> {
    let lines: Vec<&str> = content.lines().take(MAX_ITERATION_COUNT).collect();

    for (i, line) in lines.iter().enumerate() {
        let cleaned = pre_process(line);
        if let Some(value) = DESCRIPTION_PATTERN
            .captures(&cleaned)
            .and_then(|caps| caps.get(1))
        {
            let value_str = value.as_str();

            if value_str.contains("<<-") {
                return extract_multiline_description(&lines, i);
            } else {
                return Some(clean_string(value_str));
            }
        }
    }
    None
}

fn merge_summary_and_description(
    summary: Option<&str>,
    description: Option<String>,
) -> Option<String> {
    match (
        summary.map(str::trim).filter(|s| !s.is_empty()),
        description,
    ) {
        (Some(summary), Some(description)) if description.starts_with(summary) => Some(description),
        (Some(summary), Some(description)) => Some(format!("{}\n{}", summary, description)),
        (Some(summary), None) => Some(summary.to_string()),
        (None, description) => description,
    }
}

/// Extract multiline description in heredoc format
fn extract_multiline_description(lines: &[&str], start_index: usize) -> Option<String> {
    let start_line = lines.get(start_index)?;

    // Extract the delimiter (e.g., "DESC" from "<<-DESC")
    let delimiter = start_line
        .split("<<-")
        .nth(1)?
        .trim()
        .trim_matches(|c| c == '"' || c == '\'');

    let mut description_lines = Vec::new();
    let mut found_start = false;

    for line in lines.iter().take(MAX_ITERATION_COUNT).skip(start_index) {
        if !found_start && line.contains("<<-") {
            found_start = true;
            continue;
        }

        if found_start {
            let trimmed = line.trim();
            if trimmed == delimiter {
                break;
            }
            description_lines.push(*line);
        }
    }

    if description_lines.is_empty() {
        None
    } else {
        Some(description_lines.join("\n").trim().to_string())
    }
}

/// Extract authors (can be single or multiple)
fn extract_authors(content: &str) -> Vec<(String, Option<String>)> {
    let mut authors = Vec::new();

    for line in content.lines().take(MAX_ITERATION_COUNT) {
        let cleaned_line = pre_process(line);
        if let Some(value) = AUTHOR_PATTERN
            .captures(&cleaned_line)
            .and_then(|caps| caps.get(1))
        {
            let value_str = value.as_str();

            if value_str.contains("=>") {
                for part in value_str.split(',') {
                    if let Some((name, email)) = parse_author_hash_entry(part) {
                        authors.push((name, Some(email)));
                    }
                }
            } else {
                let cleaned = clean_string(value_str);
                let (name, email) = parse_author_string(&cleaned);
                authors.push((name, email));
            }
        }
    }

    authors
}

fn extract_source_url(content: &str) -> Option<String> {
    for line in content.lines().take(MAX_ITERATION_COUNT) {
        let cleaned_line = pre_process(line);
        let Some(value) = SOURCE_PATTERN
            .captures(&cleaned_line)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str())
        else {
            continue;
        };

        if let Some(caps) = SOURCE_GIT_PATTERN.captures(value)
            && let Some(url) = caps.get(1)
        {
            return Some(clean_string(url.as_str()));
        }

        if let Some(caps) = SOURCE_HTTP_PATTERN.captures(value)
            && let Some(url) = caps.get(1)
        {
            return Some(clean_string(url.as_str()));
        }

        return Some(clean_string(value));
    }

    None
}

/// Parse author from hash entry format: "Name" => "email"
fn parse_author_hash_entry(entry: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = entry.split("=>").collect();
    if parts.len() == 2 {
        let name = clean_string(parts[0].trim())
            .trim()
            .trim_matches(['\'', '"'])
            .to_string();
        let email = clean_string(parts[1].trim())
            .trim()
            .trim_matches(['\'', '"'])
            .to_string();
        Some((name, email))
    } else {
        None
    }
}

/// Parse author from string, extracting email if present
fn parse_author_string(author: &str) -> (String, Option<String>) {
    if let Some(email_start) = author.find('<')
        && let Some(email_end) = author.find('>')
    {
        let name = author[..email_start].trim().to_string();
        let email = author[email_start + 1..email_end].trim().to_string();
        return (name, Some(email));
    }
    (author.to_string(), None)
}

/// Extract dependencies from podspec
fn extract_dependencies(content: &str) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    for line in content.lines().take(MAX_ITERATION_COUNT) {
        let cleaned_line = pre_process(line);
        if let Some(caps) = DEPENDENCY_PATTERN.captures(&cleaned_line) {
            let method = caps.get(0).map(|m| m.as_str()).unwrap_or("");
            let name = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let version_req = caps.get(2).map(|m| clean_string(m.as_str()));

            if let Some(dep) = create_dependency(name, version_req, method) {
                dependencies.push(dep);
            }
        }
    }

    dependencies
}

/// Create a Dependency from name and version requirement
fn create_dependency(name: &str, version_req: Option<String>, method: &str) -> Option<Dependency> {
    if name.is_empty() {
        return None;
    }

    let purl = PackageUrl::new("cocoapods", name).ok()?;

    // Determine if version is pinned (exact version)
    let is_pinned = version_req
        .as_ref()
        .map(|v| !v.contains(&['~', '>', '<', '='][..]))
        .unwrap_or(false);

    let is_development = method.contains("add_development_dependency");

    Some(Dependency {
        purl: Some(truncate_field(purl.to_string())),
        extracted_requirement: version_req.map(truncate_field),
        scope: Some(
            if is_development {
                "development"
            } else {
                "runtime"
            }
            .to_string(),
        ),
        is_runtime: Some(!is_development),
        is_optional: Some(is_development),
        is_pinned: Some(is_pinned),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: None,
    })
}

/// Pre-process a line by removing comments and trimming
fn pre_process(line: &str) -> String {
    let line = if let Some(comment_pos) = line.find('#') {
        &line[..comment_pos]
    } else {
        line
    };
    line.trim().to_string()
}

/// Clean a string value by removing quotes and special characters
fn clean_string(s: &str) -> String {
    let after_removing_special_patterns = s.trim().replace("%q", "").replace(".freeze", "");

    after_removing_special_patterns
        .trim_matches(|c| {
            c == '\''
                || c == '"'
                || c == '{'
                || c == '}'
                || c == '['
                || c == ']'
                || c == '<'
                || c == '>'
        })
        .trim()
        .to_string()
}

fn get_repo_base_url(vcs_url: &str) -> Option<String> {
    if vcs_url.is_empty() {
        return None;
    }

    if vcs_url.ends_with(".git") {
        Some(vcs_url.trim_end_matches(".git").to_string())
    } else {
        Some(vcs_url.to_string())
    }
}

fn get_hashed_path(name: &str) -> Option<String> {
    if name.is_empty() {
        return None;
    }

    let mut hasher = Md5::new();
    hasher.update(name.as_bytes());
    let hash_str = hex::encode(hasher.finalize());

    Some(format!(
        "{}/{}/{}",
        &hash_str[0..1],
        &hash_str[1..2],
        &hash_str[2..3]
    ))
}

crate::register_parser!(
    "CocoaPods podspec file",
    &["**/*.podspec"],
    "cocoapods",
    "Objective-C",
    Some("https://guides.cocoapods.org/syntax/podspec.html"),
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_match() {
        assert!(PodspecParser::is_match(Path::new("AFNetworking.podspec")));
        assert!(PodspecParser::is_match(Path::new("project/MyLib.podspec")));
        assert!(!PodspecParser::is_match(Path::new(
            "AFNetworking.podspec.json"
        )));
        assert!(!PodspecParser::is_match(Path::new("Podfile")));
        assert!(!PodspecParser::is_match(Path::new("Podfile.lock")));
    }

    #[test]
    fn test_clean_string() {
        assert_eq!(clean_string("'AFNetworking'"), "AFNetworking");
        assert_eq!(clean_string("\"AFNetworking\""), "AFNetworking");
        assert_eq!(clean_string("'test'.freeze"), "test");
        assert_eq!(clean_string("%q{test}"), "test");
    }

    #[test]
    fn test_extract_simple_field() {
        let content = r#"
Pod::Spec.new do |s|
  s.name = "AFNetworking"
  s.version = "4.0.1"
end
"#;
        assert_eq!(
            extract_field(content, &NAME_PATTERN),
            Some("AFNetworking".to_string())
        );
        assert_eq!(
            extract_field(content, &VERSION_PATTERN),
            Some("4.0.1".to_string())
        );
    }

    #[test]
    fn test_extract_multiline_description() {
        let content = r#"
Pod::Spec.new do |s|
  s.description = <<-DESC
    A delightful networking library.
    Features include:
    - Modern API
  DESC
end
"#;
        let desc = extract_description(content);
        assert!(desc.is_some());
        let desc_text = desc.unwrap();
        assert!(desc_text.contains("delightful networking"));
        assert!(desc_text.contains("Modern API"));
    }

    #[test]
    fn test_extract_dependency() {
        let content = r#"
Pod::Spec.new do |s|
  s.dependency "AFNetworking", "~> 4.0"
  s.dependency "Alamofire"
end
"#;
        let deps = extract_dependencies(content);
        assert_eq!(deps.len(), 2);

        assert_eq!(deps[0].purl, Some("pkg:cocoapods/AFNetworking".to_string()));
        assert_eq!(deps[0].extracted_requirement, Some("~> 4.0".to_string()));
        assert_eq!(deps[0].is_pinned, Some(false)); // Contains ~

        assert_eq!(deps[1].purl, Some("pkg:cocoapods/Alamofire".to_string()));
        assert_eq!(deps[1].extracted_requirement, None);
    }

    #[test]
    fn test_extract_runtime_and_development_dependency_scopes() {
        let content = r#"
Pod::Spec.new do |s|
  s.add_dependency 'AFNetworking', '~> 4.0'
  s.add_runtime_dependency 'Alamofire', '~> 5.0'
  s.add_development_dependency 'Quick', '~> 7.0'
end
"#;

        let deps = extract_dependencies(content);
        assert_eq!(deps.len(), 3);

        assert_eq!(deps[0].scope.as_deref(), Some("runtime"));
        assert_eq!(deps[0].is_runtime, Some(true));
        assert_eq!(deps[0].is_optional, Some(false));

        assert_eq!(deps[1].scope.as_deref(), Some("runtime"));
        assert_eq!(deps[1].is_runtime, Some(true));
        assert_eq!(deps[1].is_optional, Some(false));

        assert_eq!(deps[2].scope.as_deref(), Some("development"));
        assert_eq!(deps[2].is_runtime, Some(false));
        assert_eq!(deps[2].is_optional, Some(true));
    }

    #[test]
    fn test_parse_author_string() {
        assert_eq!(
            parse_author_string("John Doe <john@example.com>"),
            ("John Doe".to_string(), Some("john@example.com".to_string()))
        );
        assert_eq!(
            parse_author_string("Jane Smith"),
            ("Jane Smith".to_string(), None)
        );
    }

    #[test]
    fn test_normalize_podspec_license_string() {
        let content = r#"
Pod::Spec.new do |s|
  s.license = 'Apache License, Version 2.0'
end
"#;

        let extracted = extract_license_statement(content);
        let (declared, declared_spdx, detections) =
            normalize_podspec_declared_license(content, extracted.as_deref());

        assert_eq!(declared.as_deref(), Some("apache-2.0"));
        assert_eq!(declared_spdx.as_deref(), Some("Apache-2.0"));
        assert_eq!(detections.len(), 1);
    }

    #[test]
    fn test_normalize_podspec_hash_type_only() {
        let content = r#"
Pod::Spec.new do |s|
  s.license = { :type => 'MIT', :file => 'LICENSE' }
end
"#;

        let extracted = extract_license_statement(content);
        let (declared, declared_spdx, detections) =
            normalize_podspec_declared_license(content, extracted.as_deref());

        assert_eq!(declared.as_deref(), Some("mit"));
        assert_eq!(declared_spdx.as_deref(), Some("MIT"));
        assert_eq!(detections.len(), 1);
    }

    #[test]
    fn test_podspec_license_hash_preserves_license_file_reference() {
        let content = r#"
Pod::Spec.new do |s|
  s.name = "Demo"
  s.version = "1.0.0"
  s.license = { :type => 'MIT', :file => 'LICENSE.txt' }
end
"#;

        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("Demo.podspec");
        std::fs::write(&file_path, content).unwrap();

        let package_data = PodspecParser::extract_first_package(&file_path);
        assert_eq!(package_data.license_detections.len(), 1);
        assert_eq!(
            package_data.license_detections[0].matches[0]
                .referenced_filenames
                .as_ref(),
            Some(&vec!["LICENSE.txt".to_string()])
        );
    }
}
