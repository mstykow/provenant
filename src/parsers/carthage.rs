use std::path::Path;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};
use crate::parser_warn as warn;
use crate::parsers::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};
use packageurl::PackageUrl;

use super::PackageParser;

pub struct CarthageCartfileParser;

impl PackageParser for CarthageCartfileParser {
    const PACKAGE_TYPE: PackageType = PackageType::Carthage;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read Cartfile at {:?}: {}", path, e);
                return vec![default_cartfile_package_data()];
            }
        };

        let dependencies = parse_cartfile_lines(&content, false);

        vec![PackageData {
            package_type: Some(Self::PACKAGE_TYPE),
            primary_language: Some("Objective-C".to_string()),
            dependencies,
            datasource_id: Some(DatasourceId::CarthageCartfile),
            ..Default::default()
        }]
    }

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .is_some_and(|name| name == "Cartfile" || name == "Cartfile.private")
    }
}

pub struct CarthageCartfileResolvedParser;

impl PackageParser for CarthageCartfileResolvedParser {
    const PACKAGE_TYPE: PackageType = PackageType::Carthage;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read Cartfile.resolved at {:?}: {}", path, e);
                return vec![default_cartfile_resolved_package_data()];
            }
        };

        let dependencies = parse_cartfile_lines(&content, true);

        vec![PackageData {
            package_type: Some(Self::PACKAGE_TYPE),
            primary_language: Some("Objective-C".to_string()),
            dependencies,
            datasource_id: Some(DatasourceId::CarthageCartfileResolved),
            ..Default::default()
        }]
    }

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .is_some_and(|name| name == "Cartfile.resolved")
    }
}

#[derive(Debug, PartialEq)]
enum OriginType {
    Github,
    Git,
    Binary,
}

struct ParsedLine {
    origin: OriginType,
    source: String,
    version_spec: Option<String>,
}

fn parse_cartfile_lines(content: &str, is_resolved: bool) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    for line in content.lines().take(MAX_ITERATION_COUNT) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some(parsed) = parse_line(line) else {
            warn!("Failed to parse Cartfile line: {}", line);
            continue;
        };

        let purl_version = if is_resolved {
            parsed.version_spec.as_deref()
        } else {
            None
        };

        let (purl, name) = match parsed.origin {
            OriginType::Github => make_github_purl(&parsed.source, purl_version),
            OriginType::Git => make_git_dep_info(&parsed.source),
            OriginType::Binary => make_binary_dep_info(&parsed.source),
        };

        let extracted_requirement = parsed.version_spec.map(truncate_field);

        let is_pinned = if is_resolved { Some(true) } else { None };

        dependencies.push(Dependency {
            purl: purl.map(truncate_field),
            extracted_requirement,
            scope: Some("dependencies".to_string()),
            is_runtime: None,
            is_optional: None,
            is_pinned,
            is_direct: Some(true),
            resolved_package: None,
            extra_data: name.map(|n| {
                let mut map = std::collections::HashMap::new();
                map.insert("name".to_string(), serde_json::json!(n));
                map
            }),
        });
    }

    dependencies
}

fn parse_line(line: &str) -> Option<ParsedLine> {
    let (origin, rest) = if let Some(rest) = line.strip_prefix("github") {
        (OriginType::Github, rest.trim())
    } else if let Some(rest) = line.strip_prefix("git") {
        (OriginType::Git, rest.trim())
    } else if let Some(rest) = line.strip_prefix("binary") {
        (OriginType::Binary, rest.trim())
    } else {
        return None;
    };

    let (source, remaining) = extract_quoted_string(rest)?;

    let version_spec = extract_version_spec(remaining.trim());

    Some(ParsedLine {
        origin,
        source,
        version_spec,
    })
}

fn extract_quoted_string(s: &str) -> Option<(String, &str)> {
    let s = s.trim();
    if !s.starts_with('"') {
        return None;
    }
    let rest = &s[1..];
    let end = rest.find('"')?;
    Some((rest[..end].to_string(), &rest[end + 1..]))
}

fn extract_version_spec(s: &str) -> Option<String> {
    let s = strip_inline_comment(s.trim());
    if s.is_empty() || s.starts_with('#') {
        return None;
    }

    let spec = if let Some(rest) = s.strip_prefix("~>") {
        format!("~> {}", rest.trim())
    } else if let Some(rest) = s.strip_prefix(">=") {
        format!(">= {}", rest.trim())
    } else if let Some(rest) = s.strip_prefix("==") {
        format!("== {}", rest.trim())
    } else if s.starts_with('"') {
        let (version, _) = extract_quoted_string(s)?;
        version
    } else {
        s.to_string()
    };

    if spec.is_empty() { None } else { Some(spec) }
}

fn strip_inline_comment(s: &str) -> &str {
    s.find('#').map_or(s, |i| s[..i].trim_end())
}

fn make_github_purl(source: &str, version: Option<&str>) -> (Option<String>, Option<String>) {
    let parts: Vec<&str> = source.splitn(2, '/').collect();
    if parts.len() != 2 {
        warn!("Invalid GitHub source in Cartfile: {}", source);
        return (None, Some(source.to_string()));
    }

    let namespace = parts[0];
    let name = parts[1];

    let purl = match PackageUrl::new("github", name) {
        Ok(mut p) => {
            if let Err(e) = p.with_namespace(namespace) {
                warn!(
                    "Failed to set namespace for github purl '{}': {}",
                    source, e
                );
                return (None, Some(name.to_string()));
            }
            if let Some(v) = version
                && let Err(e) = p.with_version(v)
            {
                warn!(
                    "Failed to set version '{}' for github purl '{}': {}",
                    v, source, e
                );
            }
            Some(p.to_string())
        }
        Err(e) => {
            warn!("Failed to create PackageUrl for github '{}': {}", source, e);
            None
        }
    };

    (purl, Some(name.to_string()))
}

fn make_git_dep_info(source: &str) -> (Option<String>, Option<String>) {
    let name = source
        .rsplit('/')
        .next()
        .map(|s| s.strip_suffix(".git").unwrap_or(s))
        .filter(|s| !s.is_empty())
        .map(String::from);

    (None, name)
}

fn make_binary_dep_info(source: &str) -> (Option<String>, Option<String>) {
    let name = source
        .rsplit('/')
        .next()
        .and_then(|s| s.strip_suffix(".json"))
        .filter(|s| !s.is_empty())
        .map(String::from);

    (None, name)
}

fn default_cartfile_package_data() -> PackageData {
    PackageData {
        package_type: Some(PackageType::Carthage),
        primary_language: Some("Objective-C".to_string()),
        datasource_id: Some(DatasourceId::CarthageCartfile),
        ..Default::default()
    }
}

fn default_cartfile_resolved_package_data() -> PackageData {
    PackageData {
        package_type: Some(PackageType::Carthage),
        primary_language: Some("Objective-C".to_string()),
        datasource_id: Some(DatasourceId::CarthageCartfileResolved),
        ..Default::default()
    }
}

crate::register_parser!(
    "Carthage Cartfile dependency manifest",
    &["**/Cartfile", "**/Cartfile.private"],
    "carthage",
    "Objective-C",
    Some("https://github.com/Carthage/Carthage/blob/master/Documentation/Artifacts.md"),
);

crate::register_parser!(
    "Carthage Cartfile.resolved pinned dependencies",
    &["**/Cartfile.resolved"],
    "carthage",
    "Objective-C",
    Some("https://github.com/Carthage/Carthage/blob/master/Documentation/Artifacts.md"),
);
