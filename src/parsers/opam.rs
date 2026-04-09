//! Parser for OCaml OPAM package manager manifests.
//!
//! Extracts package metadata and dependencies from OPAM files used by the
//! OCaml ecosystem.
//!
//! # Supported Formats
//! - *.opam files (OPAM package manifests)
//! - opam files without extension
//!
//! # Key Features
//! - Field-based parsing of OPAM's custom format (key: value)
//! - Author and maintainer extraction with email parsing
//! - URL extraction for source archives, homepage, repository
//! - License statement extraction
//! - Checksum extraction (sha1, md5, sha256, sha512)
//!
//! # Implementation Notes
//! - OPAM format uses custom syntax, not JSON/YAML/TOML
//! - Strings can be quoted or unquoted
//! - Lists use bracket notation: [item1 item2]
//! - Multi-line strings use three-quote notation: """..."""

use std::path::Path;

use crate::parser_warn as warn;
use regex::Regex;

use crate::models::{
    DatasourceId, Dependency, Md5Digest, PackageData, PackageType, Party, Sha1Digest, Sha256Digest,
    Sha512Digest,
};
use crate::parsers::PackageParser;

use super::license_normalization::{
    DeclaredLicenseMatchMetadata, build_declared_license_data_from_pair,
    normalize_spdx_declared_license,
};

/// Parser for OCaml OPAM package manifest files.
///
/// Handles the OPAM file format used by the OCaml package manager.
/// Reference: <https://opam.ocaml.org/doc/Manual.html#Common-file-format>
pub struct OpamParser;

impl PackageParser for OpamParser {
    const PACKAGE_TYPE: PackageType = PackageType::Opam;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| {
            name.to_string_lossy().ends_with(".opam") || name.to_string_lossy() == "opam"
        })
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        vec![match std::fs::read_to_string(path) {
            Ok(text) => parse_opam(&text),
            Err(e) => {
                warn!("Failed to read OPAM file {:?}: {}", path, e);
                default_package_data()
            }
        }]
    }
}

/// Parsed OPAM file data
#[derive(Debug, Default)]
struct OpamData {
    name: Option<String>,
    version: Option<String>,
    synopsis: Option<String>,
    description: Option<String>,
    homepage: Option<String>,
    dev_repo: Option<String>,
    bug_reports: Option<String>,
    src: Option<String>,
    authors: Vec<String>,
    maintainers: Vec<String>,
    license: Option<String>,
    sha1: Option<Sha1Digest>,
    md5: Option<Md5Digest>,
    sha256: Option<Sha256Digest>,
    sha512: Option<Sha512Digest>,
    dependencies: Vec<(String, String)>, // (name, version_constraint)
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(OpamParser::PACKAGE_TYPE),
        primary_language: Some("Ocaml".to_string()),
        datasource_id: Some(DatasourceId::OpamFile),
        ..Default::default()
    }
}

/// Parse an OPAM file from text content
fn parse_opam(text: &str) -> PackageData {
    let opam_data = parse_opam_data(text);

    let description = build_description(&opam_data.synopsis, &opam_data.description);
    let parties = extract_parties(&opam_data.authors, &opam_data.maintainers);
    let dependencies = extract_dependencies(&opam_data.dependencies);

    let (repository_homepage_url, api_data_url, purl) =
        build_opam_urls(&opam_data.name, &opam_data.version);
    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        normalize_opam_declared_license(opam_data.license.as_deref());

    PackageData {
        package_type: Some(OpamParser::PACKAGE_TYPE),
        namespace: None,
        name: opam_data.name,
        version: opam_data.version,
        qualifiers: None,
        subpath: None,
        primary_language: Some("Ocaml".to_string()),
        description,
        release_date: None,
        parties,
        keywords: Vec::new(),
        homepage_url: opam_data.homepage,
        download_url: opam_data.src,
        size: None,
        sha1: opam_data.sha1,
        md5: opam_data.md5,
        sha256: opam_data.sha256,
        sha512: opam_data.sha512,
        bug_tracking_url: opam_data.bug_reports,
        code_view_url: None,
        vcs_url: opam_data.dev_repo,
        copyright: None,
        holder: None,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        other_license_expression: None,
        other_license_expression_spdx: None,
        other_license_detections: Vec::new(),
        extracted_license_statement: opam_data.license,
        notice_text: None,
        source_packages: Vec::new(),
        file_references: Vec::new(),
        is_private: false,
        is_virtual: false,
        extra_data: None,
        dependencies,
        repository_homepage_url,
        repository_download_url: None,
        api_data_url,
        datasource_id: Some(DatasourceId::OpamFile),
        purl,
    }
}

fn normalize_opam_declared_license(
    statement: Option<&str>,
) -> (
    Option<String>,
    Option<String>,
    Vec<crate::models::LicenseDetection>,
) {
    let Some(statement) = statement.map(str::trim).filter(|value| !value.is_empty()) else {
        return super::license_normalization::empty_declared_license_data();
    };

    match statement {
        "GPL-2.0-only" => build_declared_license_data_from_pair(
            "gpl-2.0",
            "GPL-2.0-only",
            DeclaredLicenseMatchMetadata::single_line(statement),
        ),
        "GPL-3.0-only" => build_declared_license_data_from_pair(
            "gpl-3.0",
            "GPL-3.0-only",
            DeclaredLicenseMatchMetadata::single_line(statement),
        ),
        "LGPL-3.0-only with OCaml-LGPL-linking-exception" => build_declared_license_data_from_pair(
            "lgpl-3.0 WITH ocaml-lgpl-linking-exception",
            "LGPL-3.0-only WITH OCaml-LGPL-linking-exception",
            DeclaredLicenseMatchMetadata::single_line(statement),
        ),
        _ => normalize_spdx_declared_license(Some(statement)),
    }
}

fn build_opam_urls(
    name: &Option<String>,
    version: &Option<String>,
) -> (Option<String>, Option<String>, Option<String>) {
    let repository_homepage_url = name
        .as_ref()
        .map(|_| "{https://opam.ocaml.org/packages}/{name}".to_string());

    let api_data_url = match (name, version) {
        (Some(n), Some(v)) => Some(format!(
            "https://github.com/ocaml/opam-repository/blob/master/packages/{}/{}.{}/opam",
            n, n, v
        )),
        _ => None,
    };

    let purl = match (name, version) {
        (Some(n), Some(v)) => Some(format!("pkg:opam/{}@{}", n, v)),
        (Some(n), None) => Some(format!("pkg:opam/{}", n)),
        _ => None,
    };

    (repository_homepage_url, api_data_url, purl)
}

/// Parse OPAM file text into structured data
fn parse_opam_data(text: &str) -> OpamData {
    let mut data = OpamData::default();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Parse key: value format
        if let Some((key, value)) = parse_key_value(line) {
            match key.as_str() {
                "name" => data.name = clean_value(&value),
                "version" => data.version = clean_value(&value),
                "synopsis" => data.synopsis = clean_value(&value),
                "description" => {
                    data.description = parse_multiline_string(&lines, &mut i);
                }
                "homepage" => data.homepage = clean_value(&value),
                "dev-repo" => data.dev_repo = clean_value(&value),
                "bug-reports" => data.bug_reports = clean_value(&value),
                "src" => {
                    if value.trim().is_empty() && i + 1 < lines.len() {
                        i += 1;
                        data.src = clean_value(lines[i]);
                    } else {
                        data.src = clean_value(&value);
                    }
                }
                "license" => data.license = clean_value(&value),
                "authors" => {
                    data.authors = parse_string_array(&lines, &mut i, &value);
                }
                "maintainer" => {
                    data.maintainers = parse_string_array(&lines, &mut i, &value);
                }
                "depends" => {
                    data.dependencies = parse_dependency_array(&lines, &mut i);
                }
                "checksum" => {
                    parse_checksums(&lines, &mut i, &mut data);
                }
                _ => {}
            }
        }

        i += 1;
    }

    data
}

/// Parse a key: value line
fn parse_key_value(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }

    if let Some(colon_pos) = line.find(':') {
        let key = line[..colon_pos].trim().to_string();
        let value = line[colon_pos + 1..].trim().to_string();
        Some((key, value))
    } else {
        None
    }
}

/// Clean a value by removing quotes and brackets
fn clean_value(value: &str) -> Option<String> {
    let cleaned = value
        .trim()
        .trim_matches('"')
        .trim_matches('[')
        .trim_matches(']')
        .trim();

    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned.to_string())
    }
}

/// Parse a multiline string enclosed in triple quotes
fn parse_multiline_string(lines: &[&str], i: &mut usize) -> Option<String> {
    let mut result = String::new();

    // First line might contain opening """ and some content
    if let Some((_, value)) = parse_key_value(lines[*i]) {
        result.push_str(value.trim_matches('"').trim());
    }

    *i += 1;
    while *i < lines.len() {
        let line = lines[*i];
        result.push(' ');
        result.push_str(line.trim_matches('"').trim());

        if line.contains("\"\"\"") {
            break;
        }
        *i += 1;
    }

    let cleaned = result.trim().to_string();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// Parse a string array (single-line or multiline)
fn parse_string_array(lines: &[&str], i: &mut usize, first_value: &str) -> Vec<String> {
    let mut result = Vec::new();

    let mut content = first_value.to_string();

    // If it's a multiline array (starts with [ but no matching ])
    if content.contains('[') && !content.contains(']') {
        *i += 1;
        while *i < lines.len() {
            let line = lines[*i];
            content.push(' ');
            content.push_str(line);

            if line.contains(']') {
                break;
            }
            *i += 1;
        }
    }

    // Parse the content
    let cleaned = content.trim_matches('[').trim_matches(']').trim();

    // Split by quote-delimited strings
    for part in split_quoted_strings(cleaned) {
        let p = part.trim_matches('"').trim();
        if !p.is_empty() {
            result.push(p.to_string());
        }
    }

    result
}

/// Parse dependency array
fn parse_dependency_array(lines: &[&str], i: &mut usize) -> Vec<(String, String)> {
    let mut result = Vec::new();

    *i += 1;
    while *i < lines.len() {
        let line = lines[*i];

        if line.trim().contains(']') {
            break;
        }

        if let Some((name, version)) = parse_dependency_line(line) {
            result.push((name, version));
        }

        *i += 1;
    }

    result
}

/// Parse a single dependency line: "name" {version_constraint}
fn parse_dependency_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    // Match: "name" {optional version}
    let regex = Regex::new(r#""([^"]+)"\s*(.*)$"#).ok()?;
    let caps = regex.captures(line)?;

    let name = caps.get(1)?.as_str().to_string();
    let version_part = caps.get(2)?.as_str().trim();

    // Extract the operator and version constraint
    let constraint = if version_part.is_empty() {
        String::new()
    } else {
        extract_version_constraint(version_part)
    };

    Some((name, constraint))
}

/// Extract version constraint from {>= "1.0"} format
fn extract_version_constraint(version_part: &str) -> String {
    let regex = Regex::new(r#"\{\s*([<>=!]+)\s*"([^"]*)"\s*\}"#);
    if let Ok(re) = regex
        && let Some(caps) = re.captures(version_part)
    {
        let op = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let ver = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        if !op.is_empty() && !ver.is_empty() {
            return format!("{} {}", op, ver);
        }
    }

    // If regex parsing fails, try to extract raw content
    let content = version_part
        .trim_matches('{')
        .trim_matches('}')
        .trim_matches('"')
        .trim();

    content.replace('"', "")
}

/// Parse checksums from checksum array
fn parse_checksums(lines: &[&str], i: &mut usize, data: &mut OpamData) {
    if let Some((_, first_value)) = parse_key_value(lines[*i]) {
        let inline = first_value.trim();
        if !inline.is_empty() && inline != "[" {
            if let Some((key, value)) = parse_checksum_line(inline) {
                match key.as_str() {
                    "sha1" => data.sha1 = Sha1Digest::from_hex(&value).ok(),
                    "md5" => data.md5 = Md5Digest::from_hex(&value).ok(),
                    "sha256" => data.sha256 = Sha256Digest::from_hex(&value).ok(),
                    "sha512" => data.sha512 = Sha512Digest::from_hex(&value).ok(),
                    _ => {}
                }
            }
            return;
        }
    }

    *i += 1;
    while *i < lines.len() {
        let line = lines[*i];

        if line.trim().contains(']') {
            break;
        }

        if let Some((key, value)) = parse_checksum_line(line) {
            match key.as_str() {
                "sha1" => data.sha1 = Sha1Digest::from_hex(&value).ok(),
                "md5" => data.md5 = Md5Digest::from_hex(&value).ok(),
                "sha256" => data.sha256 = Sha256Digest::from_hex(&value).ok(),
                "sha512" => data.sha512 = Sha512Digest::from_hex(&value).ok(),
                _ => {}
            }
        }

        *i += 1;
    }
}

/// Parse a single checksum line: algo=hash
fn parse_checksum_line(line: &str) -> Option<(String, String)> {
    let line = line.trim().trim_matches('"').trim();

    let regex = Regex::new(r"^(\w+)\s*=\s*(.+)$").ok()?;
    let caps = regex.captures(line)?;

    let key = caps.get(1)?.as_str().to_string();
    let value = caps.get(2)?.as_str().to_string();

    Some((key, value))
}

/// Split quoted strings like: "str1" "str2" "str3"
fn split_quoted_strings(content: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in content.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ' ' if !in_quotes => {
                if !current.is_empty() {
                    result.push(current.trim_matches('"').to_string());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        result.push(current.trim_matches('"').to_string());
    }

    result
}

/// Build description from synopsis and description
fn build_description(synopsis: &Option<String>, description: &Option<String>) -> Option<String> {
    let parts: Vec<&str> = vec![synopsis.as_deref(), description.as_deref()]
        .into_iter()
        .filter(|p| p.is_some())
        .flatten()
        .collect();

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

/// Extract parties from authors and maintainers
fn extract_parties(authors: &[String], maintainers: &[String]) -> Vec<Party> {
    let mut parties = Vec::new();

    // Add authors
    for author in authors {
        parties.push(Party {
            r#type: Some("person".to_string()),
            role: Some("author".to_string()),
            name: Some(author.clone()),
            email: None,
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }

    // Add maintainers (as email)
    for maintainer in maintainers {
        parties.push(Party {
            r#type: Some("person".to_string()),
            role: Some("maintainer".to_string()),
            name: None,
            email: Some(maintainer.clone()),
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }

    parties
}

/// Extract dependencies into Dependency objects
fn extract_dependencies(deps: &[(String, String)]) -> Vec<Dependency> {
    deps.iter()
        .map(|(name, version_constraint)| Dependency {
            purl: Some(format!("pkg:opam/{}", name)),
            extracted_requirement: Some(version_constraint.clone()),
            scope: Some("dependency".to_string()),
            is_runtime: Some(true),
            is_optional: Some(false),
            is_pinned: Some(false),
            is_direct: Some(true),
            resolved_package: None,
            extra_data: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsers::PackageParser;

    #[test]
    fn test_is_match_with_opam_extension() {
        let path = Path::new("sample.opam");
        assert!(OpamParser::is_match(path));
    }

    #[test]
    fn test_is_match_with_opam_name() {
        let path = Path::new("opam");
        assert!(OpamParser::is_match(path));
    }

    #[test]
    fn test_is_match_with_non_opam() {
        let path = Path::new("sample.txt");
        assert!(!OpamParser::is_match(path));
    }

    #[test]
    fn test_parse_key_value() {
        let (key, value) = parse_key_value("name: \"js_of_ocaml\"").unwrap();
        assert_eq!(key, "name");
        assert_eq!(value, "\"js_of_ocaml\"");
    }

    #[test]
    fn test_clean_value() {
        assert_eq!(
            clean_value("\"js_of_ocaml\""),
            Some("js_of_ocaml".to_string())
        );
        assert_eq!(clean_value("\"\""), None);
    }

    #[test]
    fn test_extract_version_constraint() {
        let result = extract_version_constraint(r#"{>= "4.02.0"}"#);
        assert_eq!(result, ">= 4.02.0");
    }

    #[test]
    fn test_parse_dependency_line() {
        let (name, version) = parse_dependency_line(r#""ocaml" {>= "4.02.0"}"#).unwrap();
        assert_eq!(name, "ocaml");
        assert_eq!(version, ">= 4.02.0");
    }

    #[test]
    fn test_parse_dependency_line_without_version() {
        let (name, version) = parse_dependency_line(r#""uchar""#).unwrap();
        assert_eq!(name, "uchar");
        assert_eq!(version, "");
    }

    #[test]
    fn test_split_quoted_strings() {
        let parts = split_quoted_strings(r#""str1" "str2""#);
        assert_eq!(parts, vec!["str1", "str2"]);
    }

    #[test]
    fn test_build_description() {
        let synopsis = Some("Short description".to_string());
        let description = Some("Long description".to_string());
        let result = build_description(&synopsis, &description);
        assert_eq!(
            result,
            Some("Short description\nLong description".to_string())
        );
    }

    #[test]
    fn test_extract_parties() {
        let authors = vec!["Author One".to_string()];
        let maintainers = vec!["maintainer@example.com".to_string()];
        let parties = extract_parties(&authors, &maintainers);

        assert_eq!(parties.len(), 2);
        assert_eq!(parties[0].name, Some("Author One".to_string()));
        assert_eq!(parties[0].role, Some("author".to_string()));
        assert_eq!(parties[1].email, Some("maintainer@example.com".to_string()));
        assert_eq!(parties[1].role, Some("maintainer".to_string()));
    }

    #[test]
    fn test_normalize_opam_declared_license_preserves_scancode_style_expression() {
        let (declared, declared_spdx, detections) = normalize_opam_declared_license(Some(
            "LGPL-3.0-only with OCaml-LGPL-linking-exception",
        ));

        assert_eq!(
            declared.as_deref(),
            Some("lgpl-3.0 WITH ocaml-lgpl-linking-exception")
        );
        assert_eq!(
            declared_spdx.as_deref(),
            Some("LGPL-3.0-only WITH OCaml-LGPL-linking-exception")
        );
        assert_eq!(detections.len(), 1);
        assert_eq!(
            detections[0].license_expression,
            "lgpl-3.0 WITH ocaml-lgpl-linking-exception"
        );
    }
}

crate::register_parser!(
    "OCaml OPAM package manifest",
    &["**/*.opam", "**/opam"],
    "opam",
    "OCaml",
    Some("https://opam.ocaml.org/doc/Manual.html"),
);
