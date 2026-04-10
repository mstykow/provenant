//! Parser for pip requirements.txt files.
//!
//! Extracts Python package dependencies from requirements.txt files using PEP 508
//! specification parsing with support for includes, environment markers, and URLs.
//!
//! # Supported Formats
//! - requirements.txt (pip dependency specification files)
//! - Supports includes: `-r requirements.txt`, `-c constraints.txt`
//! - Supports markers: `package; python_version >= '3.6'`
//! - Supports VCS refs: `git+https://...`, `git+ssh://...`
//!
//! # Key Features
//! - PEP 508 requirement parsing with environment marker evaluation
//! - Recursive file inclusion support (`-r` and `-c` directives)
//! - VCS/URL dependency detection and handling
//! - Package URL (purl) generation for PyPI packages
//! - Line comment handling and continuation lines
//!
//! # Implementation Notes
//! - Uses PEP 508 parser from `pep508` module
//! - Recursively resolves included files relative to parent file
//! - Comments (lines starting with `#`) are skipped
//! - Environment markers are preserved for dependency filtering

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use serde_json::Value as JsonValue;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};
use crate::parsers::pep508::{Pep508Requirement, parse_pep508_requirement};

use super::PackageParser;

/// pip requirements.txt parser supporting PEP 508 dependency specifications.
///
/// Handles requirements.txt files with -r/-c includes, environment markers,
/// and VCS/URL references. Recursively resolves included requirement files.
pub struct RequirementsTxtParser;

impl PackageParser for RequirementsTxtParser {
    const PACKAGE_TYPE: PackageType = PackageType::Pypi;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        vec![extract_from_requirements_txt(path)]
    }

    fn is_match(path: &Path) -> bool {
        let filename = path.file_name().and_then(|name| name.to_str());
        let Some(name) = filename else {
            return false;
        };

        is_requirements_txt_filename(name)
            || (is_requirements_like_extension(name) && has_requirements_like_ancestor(path))
    }
}

fn is_requirements_txt_filename(name: &str) -> bool {
    if name == "requirements.txt" || name == "requires.txt" {
        return true;
    }

    let Some(stem) = name
        .strip_suffix(".txt")
        .or_else(|| name.strip_suffix(".in"))
    else {
        return false;
    };

    stem == "requirements" || stem.starts_with("requirements") || stem.ends_with("requirements")
}

fn is_requirements_like_extension(name: &str) -> bool {
    name.ends_with(".txt") || name.ends_with(".in")
}

fn has_requirements_like_ancestor(path: &Path) -> bool {
    path.parent()
        .into_iter()
        .flat_map(Path::ancestors)
        .filter_map(|ancestor| ancestor.file_name())
        .filter_map(|name| name.to_str())
        .any(is_requirements_like_dir_name)
}

fn is_requirements_like_dir_name(name: &str) -> bool {
    name == "requirements" || name.starts_with("requirements") || name.ends_with("requirements")
}

struct ParseState {
    dependencies: Vec<Dependency>,
    extra_index_urls: Vec<String>,
    index_url: Option<String>,
    includes: Vec<String>,
    constraints: Vec<String>,
    visited: HashSet<PathBuf>,
}

fn extract_from_requirements_txt(path: &Path) -> PackageData {
    let mut state = ParseState {
        dependencies: Vec::new(),
        extra_index_urls: Vec::new(),
        index_url: None,
        includes: Vec::new(),
        constraints: Vec::new(),
        visited: HashSet::new(),
    };

    let (scope, is_runtime) = scope_from_filename(path);

    parse_requirements_with_includes(path, &mut state, &scope, is_runtime);

    let mut extra_data = HashMap::new();
    if let Some(url) = state.index_url {
        extra_data.insert("index_url".to_string(), JsonValue::String(url));
    }
    if !state.extra_index_urls.is_empty() {
        extra_data.insert(
            "extra_index_urls".to_string(),
            JsonValue::Array(
                state
                    .extra_index_urls
                    .into_iter()
                    .map(JsonValue::String)
                    .collect(),
            ),
        );
    }
    if !state.includes.is_empty() {
        extra_data.insert(
            "requirements_includes".to_string(),
            JsonValue::Array(state.includes.into_iter().map(JsonValue::String).collect()),
        );
    }
    if !state.constraints.is_empty() {
        extra_data.insert(
            "constraints".to_string(),
            JsonValue::Array(
                state
                    .constraints
                    .into_iter()
                    .map(JsonValue::String)
                    .collect(),
            ),
        );
    }

    let extra_data = if extra_data.is_empty() {
        None
    } else {
        Some(extra_data)
    };

    default_package_data(state.dependencies, extra_data)
}

fn parse_requirements_with_includes(
    path: &Path,
    state: &mut ParseState,
    scope: &str,
    is_runtime: bool,
) {
    let abs_path = match path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            warn!("Cannot resolve path: {:?}", path);
            return;
        }
    };

    if state.visited.contains(&abs_path) {
        warn!("Circular include detected: {:?}", path);
        return;
    }

    state.visited.insert(abs_path.clone());

    let content = match fs::read_to_string(&abs_path) {
        Ok(c) => c,
        Err(e) => {
            warn!("Cannot read file {:?}: {}", abs_path, e);
            return;
        }
    };

    for line in collect_logical_lines(&content) {
        let cleaned = strip_inline_comment(&line);
        let trimmed = cleaned.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some(url) = parse_option_value(trimmed, "--extra-index-url") {
            state.extra_index_urls.push(url);
            continue;
        }

        if let Some(url) = parse_option_value(trimmed, "--index-url") {
            state.index_url = Some(url);
            continue;
        }

        if let Some(path_value) = parse_option_value(trimmed, "-r")
            .or_else(|| parse_option_value(trimmed, "--requirement"))
        {
            state.includes.push(path_value.clone());
            let included_path = abs_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(&path_value);

            if included_path.exists() {
                parse_requirements_with_includes(&included_path, state, scope, is_runtime);
            } else {
                warn!("Included file not found: {:?}", included_path);
            }
            continue;
        }

        if let Some(path_value) = parse_option_value(trimmed, "-c")
            .or_else(|| parse_option_value(trimmed, "--constraint"))
        {
            state.constraints.push(path_value.clone());
            let constraint_path = abs_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(&path_value);

            if constraint_path.exists() {
                parse_requirements_with_includes(&constraint_path, state, scope, is_runtime);
            } else {
                warn!("Constraint file not found: {:?}", constraint_path);
            }
            continue;
        }

        if trimmed.starts_with('-')
            && !trimmed.starts_with("-e")
            && !trimmed.starts_with("--editable")
        {
            continue;
        }

        if let Some(dependency) = build_dependency(trimmed, scope, is_runtime) {
            state.dependencies.push(dependency);
        }
    }
}

fn default_package_data(
    dependencies: Vec<Dependency>,
    extra_data: Option<HashMap<String, JsonValue>>,
) -> PackageData {
    PackageData {
        package_type: Some(RequirementsTxtParser::PACKAGE_TYPE),
        primary_language: Some("Python".to_string()),
        extra_data,
        dependencies,
        datasource_id: Some(DatasourceId::PipRequirements),
        ..Default::default()
    }
}

fn collect_logical_lines(content: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for raw_line in content.lines() {
        let line = raw_line.trim_end_matches('\r');
        let trimmed = line.trim_end();
        let is_continuation = trimmed.ends_with('\\');
        let line_without = if is_continuation {
            trimmed.trim_end_matches('\\')
        } else {
            line
        };

        if !line_without.trim().is_empty() {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(line_without.trim());
        }

        if !is_continuation && !current.is_empty() {
            lines.push(current.trim().to_string());
            current.clear();
        }
    }

    if !current.is_empty() {
        lines.push(current.trim().to_string());
    }

    lines
}

fn strip_inline_comment(line: &str) -> String {
    let mut in_single = false;
    let mut in_double = false;
    for (idx, ch) in line.char_indices() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '#' if !in_single && !in_double => {
                let prefix = &line[..idx];
                if prefix.trim_end().is_empty() || prefix.ends_with(char::is_whitespace) {
                    return prefix.trim_end().to_string();
                }
            }
            _ => {}
        }
    }
    line.to_string()
}

fn parse_option_value(line: &str, option: &str) -> Option<String> {
    let stripped = line.strip_prefix(option)?;
    let mut rest = stripped.trim();
    if let Some(rest_stripped) = rest.strip_prefix('=') {
        rest = rest_stripped.trim();
    }
    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}

fn scope_from_filename(path: &Path) -> (String, bool) {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if filename.contains("dev") {
        return ("develop".to_string(), false);
    }
    if filename.contains("test") {
        return ("test".to_string(), false);
    }
    if filename.contains("doc") {
        return ("docs".to_string(), false);
    }

    ("install".to_string(), true)
}

fn build_dependency(line: &str, scope: &str, is_runtime: bool) -> Option<Dependency> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut is_editable = false;
    let mut requirement = trimmed.to_string();
    let mut extracted_requirement = trimmed.to_string();

    if let Some(rest) = trimmed.strip_prefix("-e") {
        is_editable = true;
        requirement = rest.trim().to_string();
        extracted_requirement = format!("--editable {}", requirement);
    } else if let Some(rest) = trimmed.strip_prefix("--editable") {
        is_editable = true;
        requirement = rest.trim().to_string();
        extracted_requirement = format!("--editable {}", requirement);
    }

    let (requirement, hash_options) = split_hash_options(&requirement);
    let requirement = requirement.trim();
    if requirement.is_empty() {
        return None;
    }

    let parsed = parse_requirement(requirement);

    let pinned_version = parsed
        .specifiers
        .as_deref()
        .and_then(extract_pinned_version);
    let is_pinned = pinned_version.is_some();

    let purl = parsed
        .name
        .as_ref()
        .and_then(|name| create_pypi_purl(name, pinned_version.as_deref()));

    let mut extra_data = HashMap::new();
    extra_data.insert("is_editable".to_string(), JsonValue::Bool(is_editable));
    extra_data.insert(
        "link".to_string(),
        parsed
            .link
            .clone()
            .map(JsonValue::String)
            .unwrap_or(JsonValue::Null),
    );
    extra_data.insert(
        "hash_options".to_string(),
        JsonValue::Array(hash_options.into_iter().map(JsonValue::String).collect()),
    );
    extra_data.insert("is_constraint".to_string(), JsonValue::Bool(false));
    extra_data.insert(
        "is_archive".to_string(),
        parsed
            .is_archive
            .map(JsonValue::Bool)
            .unwrap_or(JsonValue::Null),
    );
    extra_data.insert("is_wheel".to_string(), JsonValue::Bool(parsed.is_wheel));
    extra_data.insert(
        "is_url".to_string(),
        parsed
            .is_url
            .map(JsonValue::Bool)
            .unwrap_or(JsonValue::Null),
    );
    extra_data.insert(
        "is_vcs_url".to_string(),
        parsed
            .is_vcs_url
            .map(JsonValue::Bool)
            .unwrap_or(JsonValue::Null),
    );
    extra_data.insert(
        "is_name_at_url".to_string(),
        JsonValue::Bool(parsed.is_name_at_url),
    );
    extra_data.insert(
        "is_local_path".to_string(),
        parsed
            .is_local_path
            .map(|value| value || is_editable)
            .map(JsonValue::Bool)
            .unwrap_or(JsonValue::Null),
    );

    if let Some(marker) = parsed.marker {
        extra_data.insert("markers".to_string(), JsonValue::String(marker));
    }

    Some(Dependency {
        purl,
        extracted_requirement: Some(extracted_requirement),
        scope: Some(scope.to_string()),
        is_runtime: Some(is_runtime),
        is_optional: Some(false),
        is_pinned: Some(is_pinned),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: Some(extra_data),
    })
}

fn split_hash_options(input: &str) -> (String, Vec<String>) {
    let mut filtered = Vec::new();
    let mut hashes = Vec::new();

    for token in input.split_whitespace() {
        if let Some(value) = token.strip_prefix("--hash=") {
            if !value.is_empty() {
                hashes.push(value.to_string());
            }
        } else {
            filtered.push(token);
        }
    }

    (filtered.join(" "), hashes)
}

struct ParsedRequirement {
    name: Option<String>,
    specifiers: Option<String>,
    marker: Option<String>,
    link: Option<String>,
    is_url: Option<bool>,
    is_vcs_url: Option<bool>,
    is_local_path: Option<bool>,
    is_name_at_url: bool,
    is_archive: Option<bool>,
    is_wheel: bool,
}

fn parse_requirement(input: &str) -> ParsedRequirement {
    if let Some(parsed) = parse_pep508_requirement(input) {
        if let Some(url) = parsed.url.clone() {
            return parsed_with_link(parsed, &url);
        }

        if !is_link_like(input) {
            let name = Some(normalize_pypi_name(&parsed.name));
            return ParsedRequirement {
                name,
                specifiers: parsed.specifiers,
                marker: parsed.marker,
                link: None,
                is_url: None,
                is_vcs_url: None,
                is_local_path: None,
                is_name_at_url: false,
                is_archive: None,
                is_wheel: false,
            };
        }
    }

    if let Some((name, link)) = parse_link_with_name(input) {
        let normalized_name = normalize_pypi_name(&name);
        let link_info = parse_link_flags(&link);
        return ParsedRequirement {
            name: Some(normalized_name),
            specifiers: None,
            marker: None,
            link: Some(link),
            is_url: Some(link_info.is_url),
            is_vcs_url: Some(link_info.is_vcs_url),
            is_local_path: Some(link_info.is_local_path),
            is_name_at_url: link_info.is_name_at_url,
            is_archive: link_info.is_archive,
            is_wheel: link_info.is_wheel,
        };
    }

    let link_info = parse_link_flags(input);
    ParsedRequirement {
        name: None,
        specifiers: None,
        marker: None,
        link: Some(input.to_string()),
        is_url: Some(link_info.is_url),
        is_vcs_url: Some(link_info.is_vcs_url),
        is_local_path: Some(link_info.is_local_path),
        is_name_at_url: link_info.is_name_at_url,
        is_archive: link_info.is_archive,
        is_wheel: link_info.is_wheel,
    }
}

fn parsed_with_link(parsed: Pep508Requirement, link: &str) -> ParsedRequirement {
    let name = normalize_pypi_name(&parsed.name);
    let link_info = parse_link_flags(link);
    ParsedRequirement {
        name: Some(name),
        specifiers: parsed.specifiers,
        marker: parsed.marker,
        link: Some(link.to_string()),
        is_url: Some(link_info.is_url),
        is_vcs_url: Some(link_info.is_vcs_url),
        is_local_path: Some(link_info.is_local_path),
        is_name_at_url: parsed.is_name_at_url,
        is_archive: link_info.is_archive,
        is_wheel: link_info.is_wheel,
    }
}

fn parse_link_with_name(input: &str) -> Option<(String, String)> {
    if let Some(egg) = extract_egg_name(input) {
        return Some((egg, input.to_string()));
    }
    None
}

fn extract_egg_name(input: &str) -> Option<String> {
    let fragment = input.split('#').nth(1)?;
    let egg_part = fragment.strip_prefix("egg=")?;
    let name_part = egg_part.split('&').next()?.trim();
    if name_part.is_empty() {
        return None;
    }
    let (name, _extras, _) = parse_pep508_requirement(name_part)
        .map(|parsed| (parsed.name, parsed.extras, parsed.specifiers))
        .unwrap_or_else(|| (name_part.to_string(), Vec::new(), None));
    Some(name)
}

struct LinkFlags {
    is_url: bool,
    is_vcs_url: bool,
    is_local_path: bool,
    is_name_at_url: bool,
    is_archive: Option<bool>,
    is_wheel: bool,
}

fn parse_link_flags(link: &str) -> LinkFlags {
    let trimmed = link.trim();
    let is_vcs_url = trimmed.starts_with("git+")
        || trimmed.starts_with("hg+")
        || trimmed.starts_with("svn+")
        || trimmed.starts_with("bzr+");
    let has_scheme = trimmed.contains("://") || trimmed.starts_with("file:");
    let is_local_path = trimmed.starts_with("./")
        || trimmed.starts_with("../")
        || trimmed.starts_with('/')
        || trimmed.starts_with('~')
        || trimmed.starts_with("file:");

    let is_wheel = trimmed.ends_with(".whl");
    let is_archive = if is_wheel
        || trimmed.ends_with(".zip")
        || trimmed.ends_with(".tar.gz")
        || trimmed.ends_with(".tgz")
        || trimmed.ends_with(".tar.bz2")
        || trimmed.ends_with(".tar")
    {
        Some(true)
    } else if has_scheme || is_local_path {
        Some(false)
    } else {
        None
    };

    LinkFlags {
        is_url: has_scheme || is_vcs_url,
        is_vcs_url,
        is_local_path,
        is_name_at_url: false,
        is_archive,
        is_wheel,
    }
}

fn is_link_like(input: &str) -> bool {
    let trimmed = input.trim();
    trimmed.starts_with("git+")
        || trimmed.starts_with("hg+")
        || trimmed.starts_with("svn+")
        || trimmed.starts_with("bzr+")
        || trimmed.starts_with("file:")
        || trimmed.contains("://")
        || trimmed.starts_with("./")
        || trimmed.starts_with("../")
        || trimmed.starts_with('/')
        || trimmed.starts_with('~')
}

fn extract_pinned_version(specifiers: &str) -> Option<String> {
    let trimmed = specifiers.trim();
    if trimmed.contains(',') {
        return None;
    }

    let stripped = if let Some(version) = trimmed.strip_prefix("==") {
        version
    } else if let Some(version) = trimmed.strip_prefix("===") {
        version
    } else {
        return None;
    };

    let version = stripped.trim();
    if version.is_empty() {
        None
    } else {
        Some(version.to_string())
    }
}

fn create_pypi_purl(name: &str, version: Option<&str>) -> Option<String> {
    PackageUrl::new(RequirementsTxtParser::PACKAGE_TYPE.as_str(), name)
        .ok()
        .map(|_| match version {
            Some(version) => format!("pkg:pypi/{name}@{}", encode_pypi_purl_version(version)),
            None => format!("pkg:pypi/{name}"),
        })
}

fn encode_pypi_purl_version(version: &str) -> String {
    version.replace('*', "%2A")
}

fn normalize_pypi_name(name: &str) -> String {
    let lower = name.trim().to_ascii_lowercase();
    let mut normalized = String::new();
    let mut last_was_sep = false;
    for ch in lower.chars() {
        let is_sep = matches!(ch, '-' | '_' | '.');
        if is_sep {
            if !last_was_sep {
                normalized.push('-');
                last_was_sep = true;
            }
        } else {
            normalized.push(ch);
            last_was_sep = false;
        }
    }
    normalized
}

crate::register_parser!(
    "pip requirements file",
    &[
        "**/requirements*.txt",
        "**/*requirements.txt",
        "**/requirements*.in",
        "**/*requirements.in",
        "**/requires.txt",
        "**/requirements/*.txt",
        "**/requirements/*.in",
        "**/requirements/**/*.txt",
        "**/requirements/**/*.in",
        "**/requirements*/*.txt",
        "**/requirements*/*.in",
        "**/requirements*/**/*.txt",
        "**/requirements*/**/*.in"
    ],
    "pypi",
    "Python",
    Some("https://pip.pypa.io/en/latest/reference/requirements-file-format/"),
);
