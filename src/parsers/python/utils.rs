use super::PythonParser;
use super::archive::is_likely_python_sdist_filename;
use crate::models::{DatasourceId, Dependency, PackageData, Sha256Digest};
use crate::parser_warn as warn;
use crate::parsers::PackageParser;
use crate::parsers::utils::MAX_ITERATION_COUNT;
use packageurl::PackageUrl;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::LazyLock;

static EXTRA_MARKER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"extra\s*==\s*['\"]([^'\"]+)['\"]"#).expect("extra marker regex should compile")
});

static MARKER_FIELD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(\w+)\s*(==|!=|<=|>=|<|>)\s*['\"]([^'\"]+)['\"]"#)
        .expect("marker field regex should compile")
});

static TESTS_REQUIRE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"tests_require\s*=\s*\[([^\]]+)\]").expect("tests_require regex should compile")
});

static EXTRAS_REQUIRE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"extras_require\s*=\s*\{([^}]+)\}").expect("extras_require regex should compile")
});

static EXTRAS_ENTRY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"['"]([^'"]+)['"]\s*:\s*\[([^\]]+)\]"#).expect("extras entry regex should compile")
});

static DEP_PATTERN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"['"]([^'"]+)['"]"#).expect("dep pattern regex should compile"));

static SETUP_VALUE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(\w+)\s*=\s*['"]([^'"]*)['"]"#).expect("setup value regex should compile")
});

use toml::Value as TomlValue;

pub(super) fn default_package_data(path: &Path) -> Vec<PackageData> {
    vec![PackageData {
        package_type: Some(PythonParser::PACKAGE_TYPE),
        primary_language: Some("Python".to_string()),
        datasource_id: infer_python_datasource_id(path),
        ..Default::default()
    }]
}

fn infer_python_datasource_id(path: &Path) -> Option<DatasourceId> {
    let file_name = path.file_name().and_then(|name| name.to_str());

    match file_name {
        Some("pyproject.toml") => {
            if read_toml_file(path)
                .ok()
                .and_then(|content| content.get("tool").and_then(|v| v.as_table()).cloned())
                .and_then(|tool| tool.get("poetry").and_then(|v| v.as_table()).cloned())
                .is_some()
            {
                Some(DatasourceId::PypiPoetryPyprojectToml)
            } else {
                Some(DatasourceId::PypiPyprojectToml)
            }
        }
        Some(name)
            if name == "setup.py" || name.ends_with("_setup.py") || name.ends_with("-setup.py") =>
        {
            Some(DatasourceId::PypiSetupPy)
        }
        Some("setup.cfg") => Some(DatasourceId::PypiSetupCfg),
        Some("PKG-INFO") => Some(detect_pkg_info_datasource_id(path)),
        Some("METADATA") if super::is_installed_wheel_metadata_path(path) => {
            Some(DatasourceId::PypiWheelMetadata)
        }
        Some("pypi.json") => Some(DatasourceId::PypiJson),
        Some("pip-inspect.deplock") => Some(DatasourceId::PypiInspectDeplock),
        Some("origin.json") if super::archive::is_pip_cache_origin_json(path) => {
            Some(DatasourceId::PypiPipOriginJson)
        }
        _ if file_name.is_some_and(is_likely_python_sdist_filename) => {
            Some(DatasourceId::PypiSdist)
        }
        _ if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("whl")) =>
        {
            Some(DatasourceId::PypiWheel)
        }
        _ if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("egg")) =>
        {
            Some(DatasourceId::PypiEgg)
        }
        _ => None,
    }
}

pub(crate) struct PypiUrls {
    pub repository_homepage_url: Option<String>,
    pub repository_download_url: Option<String>,
    pub api_data_url: Option<String>,
    pub purl: Option<String>,
}

pub(crate) fn build_pypi_urls(name: Option<&str>, version: Option<&str>) -> PypiUrls {
    let repository_homepage_url = name.map(|value| format!("https://pypi.org/project/{}", value));

    let repository_download_url = name.and_then(|value| {
        version.map(|ver| {
            format!(
                "https://pypi.org/packages/source/{}/{}/{}-{}.tar.gz",
                &value[..1.min(value.len())],
                value,
                value,
                ver
            )
        })
    });

    let api_data_url = name.map(|value| {
        if let Some(ver) = version {
            format!("https://pypi.org/pypi/{}/{}/json", value, ver)
        } else {
            format!("https://pypi.org/pypi/{}/json", value)
        }
    });

    let purl = name.and_then(|value| {
        let mut package_url = PackageUrl::new(PythonParser::PACKAGE_TYPE.as_str(), value).ok()?;
        if let Some(ver) = version {
            package_url.with_version(ver).ok()?;
        }
        Some(package_url.to_string())
    });

    PypiUrls {
        repository_homepage_url,
        repository_download_url,
        api_data_url,
        purl,
    }
}

pub(crate) fn read_toml_file(path: &Path) -> Result<TomlValue, String> {
    let content =
        crate::parsers::utils::read_file_to_string(path, None).map_err(|e| e.to_string())?;
    toml::from_str(&content).map_err(|e| format!("Failed to parse TOML: {}", e))
}

pub(super) fn calculate_file_checksums(path: &Path) -> (Option<u64>, Option<Sha256Digest>) {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return (None, None),
    };

    let metadata = match file.metadata() {
        Ok(m) => m,
        Err(_) => return (None, None),
    };
    let size = metadata.len();

    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 8192];

    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => hasher.update(&buffer[..n]),
            Err(_) => return (Some(size), None),
        }
    }

    let hash = Sha256Digest::from_bytes(hasher.finalize().into());
    (Some(size), Some(hash))
}

pub(super) fn build_python_dependency(
    entry: &str,
    default_scope: &str,
    default_optional: bool,
    marker_override: Option<&str>,
) -> Option<Dependency> {
    let (requirement_part, marker_part) = entry
        .split_once(';')
        .map(|(req, marker)| (req.trim(), Some(marker.trim())))
        .unwrap_or((entry.trim(), None));

    let name = extract_setup_cfg_dependency_name(requirement_part)?;
    let requirement = normalize_rfc822_requirement(requirement_part);
    let parsed = parse_rfc822_marker(
        marker_part.or(marker_override),
        default_scope,
        default_optional,
    );
    let purl = build_python_dependency_purl(&name, None)?;

    let is_pinned = requirement
        .as_deref()
        .is_some_and(|req| req.starts_with("==") || req.starts_with("==="));
    let purl = if is_pinned {
        requirement
            .as_deref()
            .map(|req| req.trim_start_matches('='))
            .and_then(|version| build_python_dependency_purl(&name, Some(version)))
            .unwrap_or(purl)
    } else {
        purl
    };

    let mut extra_data = HashMap::new();
    extra_data.extend(parsed.extra_data);
    if let Some(marker) = parsed.marker {
        extra_data.insert("marker".to_string(), serde_json::Value::String(marker));
    }

    Some(Dependency {
        purl: Some(purl),
        extracted_requirement: requirement,
        scope: Some(parsed.scope),
        is_runtime: Some(true),
        is_optional: Some(parsed.is_optional),
        is_pinned: Some(is_pinned),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: if extra_data.is_empty() {
            None
        } else {
            Some(extra_data)
        },
    })
}

pub(super) fn normalize_python_dependency_name(name: &str) -> String {
    normalize_python_distribution_name(name)
}

pub(super) fn build_python_dependency_purl(name: &str, version: Option<&str>) -> Option<String> {
    let normalized_name = normalize_python_dependency_name(name);

    PackageUrl::new(PythonParser::PACKAGE_TYPE.as_str(), &normalized_name)
        .ok()
        .map(|_| match version {
            Some(version) => {
                format!(
                    "pkg:pypi/{normalized_name}@{}",
                    encode_python_dependency_purl_version(version)
                )
            }
            None => format!("pkg:pypi/{normalized_name}"),
        })
}

fn encode_python_dependency_purl_version(version: &str) -> String {
    version.replace('*', "%2A")
}

fn normalize_rfc822_requirement(requirement_part: &str) -> Option<String> {
    let name = extract_setup_cfg_dependency_name(requirement_part)?;
    let trimmed = requirement_part.trim();
    let mut remainder = trimmed[name.len()..].trim();

    if let Some(stripped) = remainder.strip_prefix('[')
        && let Some(end_idx) = stripped.find(']')
    {
        remainder = stripped[end_idx + 1..].trim();
    }

    let remainder = remainder
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
        .unwrap_or(remainder)
        .trim();

    if remainder.is_empty() {
        return None;
    }

    let mut specifiers: Vec<String> = remainder
        .split(',')
        .map(|specifier| specifier.trim().replace(' ', ""))
        .filter(|specifier| !specifier.is_empty())
        .collect();
    specifiers.sort();
    Some(specifiers.join(","))
}

fn build_rfc822_dependency(entry: &str) -> Option<Dependency> {
    build_python_dependency(entry, "install", false, None)
}

pub(crate) fn extract_requires_dist_dependencies(requires_dist: &[String]) -> Vec<Dependency> {
    requires_dist
        .iter()
        .filter_map(|entry| build_rfc822_dependency(entry))
        .collect()
}

pub(super) struct ParsedMarker {
    pub scope: String,
    pub is_optional: bool,
    pub marker: Option<String>,
    pub extra_data: HashMap<String, serde_json::Value>,
}

fn parse_rfc822_marker(
    marker_part: Option<&str>,
    default_scope: &str,
    default_optional: bool,
) -> ParsedMarker {
    let Some(marker) = marker_part.filter(|marker| !marker.trim().is_empty()) else {
        return ParsedMarker {
            scope: default_scope.to_string(),
            is_optional: default_optional,
            marker: None,
            extra_data: HashMap::new(),
        };
    };

    let mut extra_data = HashMap::new();

    if let Some(python_version) = extract_marker_field(marker, "python_version") {
        extra_data.insert(
            "python_version".to_string(),
            serde_json::Value::String(python_version),
        );
    }
    if let Some(sys_platform) = extract_marker_field(marker, "sys_platform") {
        extra_data.insert(
            "sys_platform".to_string(),
            serde_json::Value::String(sys_platform),
        );
    }

    if let Some(captures) = EXTRA_MARKER_RE.captures(marker)
        && let Some(scope) = captures.get(1)
    {
        return ParsedMarker {
            scope: scope.as_str().to_string(),
            is_optional: true,
            marker: Some(marker.trim().to_string()),
            extra_data,
        };
    }

    ParsedMarker {
        scope: default_scope.to_string(),
        is_optional: default_optional,
        marker: Some(marker.trim().to_string()),
        extra_data,
    }
}

fn extract_marker_field(marker: &str, field: &str) -> Option<String> {
    let captures = MARKER_FIELD_RE.captures(marker)?;
    let matched_field = captures.get(1)?.as_str();
    if matched_field != field {
        return None;
    }
    let operator = captures.get(2)?.as_str();
    let value = captures.get(3)?.as_str();
    Some(format!("{} {}", operator, value))
}

pub(super) fn parse_requires_txt(content: &str) -> Vec<Dependency> {
    let mut dependencies = Vec::new();
    let mut current_scope = "install".to_string();
    let mut current_optional = false;
    let mut current_marker: Option<String> = None;
    let mut line_count = 0usize;

    for line in content.lines() {
        line_count += 1;
        if line_count > MAX_ITERATION_COUNT {
            warn!(
                "Exceeded max line count in requires.txt; stopping at {} lines",
                MAX_ITERATION_COUNT
            );
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let inner = &trimmed[1..trimmed.len() - 1];
            if let Some(rest) = inner.strip_prefix(':') {
                current_scope = "install".to_string();
                current_optional = false;
                current_marker = Some(rest.trim().to_string());
            } else if let Some((scope, marker)) = inner.split_once(':') {
                current_scope = scope.trim().to_string();
                current_optional = true;
                current_marker = Some(marker.trim().to_string());
            } else {
                current_scope = inner.trim().to_string();
                current_optional = true;
                current_marker = None;
            }
            continue;
        }

        if let Some(dependency) = build_python_dependency(
            trimmed,
            &current_scope,
            current_optional,
            current_marker.as_deref(),
        ) {
            dependencies.push(dependency);
        }
    }

    dependencies
}

pub(super) fn has_private_classifier(classifiers: &[String]) -> bool {
    classifiers
        .iter()
        .any(|classifier| classifier.eq_ignore_ascii_case("Private :: Do Not Upload"))
}

pub(super) fn build_setup_py_purl(name: Option<&str>, version: Option<&str>) -> Option<String> {
    let name = name?;
    let mut package_url = PackageUrl::new(PythonParser::PACKAGE_TYPE.as_str(), name).ok()?;
    if let Some(version) = version {
        package_url.with_version(version).ok()?;
    }
    Some(package_url.to_string())
}

pub(super) fn extract_setup_value(content: &str, key: &str) -> Option<String> {
    for captures in SETUP_VALUE_RE.captures_iter(content) {
        if captures.get(1)?.as_str() == key {
            return Some(captures.get(2)?.as_str().to_string());
        }
    }
    None
}

pub(super) fn extract_setup_py_dependencies(content: &str) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    if let Some(tests_deps) = extract_tests_require(content) {
        dependencies.extend(tests_deps);
    }

    if let Some(extras_deps) = extract_extras_require(content) {
        dependencies.extend(extras_deps);
    }

    dependencies
}

fn extract_tests_require(content: &str) -> Option<Vec<Dependency>> {
    let captures = TESTS_REQUIRE_RE.captures(content)?;
    let deps_str = captures.get(1)?.as_str();

    let deps = parse_setup_py_dep_list(deps_str, "test", true);
    if deps.is_empty() { None } else { Some(deps) }
}

fn extract_extras_require(content: &str) -> Option<Vec<Dependency>> {
    let captures = EXTRAS_REQUIRE_RE.captures(content)?;
    let dict_content = captures.get(1)?.as_str();

    let mut all_deps = Vec::new();

    for entry_cap in EXTRAS_ENTRY_RE.captures_iter(dict_content) {
        if let (Some(extra_name), Some(deps_str)) = (entry_cap.get(1), entry_cap.get(2)) {
            let deps = parse_setup_py_dep_list(deps_str.as_str(), extra_name.as_str(), true);
            all_deps.extend(deps);
        }
    }

    if all_deps.is_empty() {
        None
    } else {
        Some(all_deps)
    }
}

fn parse_setup_py_dep_list(deps_str: &str, scope: &str, is_optional: bool) -> Vec<Dependency> {
    DEP_PATTERN_RE
        .captures_iter(deps_str)
        .filter_map(|cap| {
            let dep_str = cap.get(1)?.as_str().trim();
            if dep_str.is_empty() {
                return None;
            }

            let name = extract_setup_cfg_dependency_name(dep_str)?;
            let purl = build_python_dependency_purl(&name, None)?;

            Some(Dependency {
                purl: Some(purl),
                extracted_requirement: Some(dep_str.to_string()),
                scope: Some(scope.to_string()),
                is_runtime: Some(true),
                is_optional: Some(is_optional),
                is_pinned: Some(false),
                is_direct: Some(true),
                resolved_package: None,
                extra_data: None,
            })
        })
        .collect()
}

pub(super) fn strip_python_archive_extension(file_name: &str) -> Option<&str> {
    [".tar.gz", ".tar.bz2", ".tar.xz", ".tgz", ".zip", ".whl"]
        .iter()
        .find_map(|suffix| file_name.strip_suffix(suffix))
}

pub(super) fn normalize_python_package_name(name: &str) -> String {
    normalize_python_distribution_name(name)
}

fn normalize_python_distribution_name(name: &str) -> String {
    let lower = name.trim().to_ascii_lowercase();
    let mut normalized = String::with_capacity(lower.len());
    let mut last_was_separator = false;

    for ch in lower.chars() {
        let is_separator = matches!(ch, '-' | '_' | '.');
        if is_separator {
            if !last_was_separator {
                normalized.push('-');
                last_was_separator = true;
            }
        } else {
            normalized.push(ch);
            last_was_separator = false;
        }
    }

    normalized
}

pub(super) fn detect_pkg_info_datasource_id(path: &Path) -> DatasourceId {
    let path_str = path.to_string_lossy().replace('\\', "/");
    if path_str.contains("/EGG-INFO/PKG-INFO") {
        DatasourceId::PypiEggPkginfo
    } else if path_str.ends_with(".egg-info/PKG-INFO") {
        DatasourceId::PypiEditableEggPkginfo
    } else {
        DatasourceId::PypiSdistPkginfo
    }
}

pub(super) fn extract_setup_cfg_dependency_name(req: &str) -> Option<String> {
    let trimmed = req.trim();
    if trimmed.is_empty() {
        return None;
    }

    let end = trimmed
        .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '=' | '!' | '~' | ';' | '['))
        .unwrap_or(trimmed.len());
    let name = trimmed[..end].trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

pub(super) struct ProjectUrls {
    pub homepage_url: Option<String>,
    pub download_url: Option<String>,
    pub bug_tracking_url: Option<String>,
    pub code_view_url: Option<String>,
    pub vcs_url: Option<String>,
    pub changelog_url: Option<String>,
}

pub(super) fn apply_project_url_mappings(
    parsed_urls: &[(String, String)],
    urls: &mut ProjectUrls,
    extra_data: &mut HashMap<String, serde_json::Value>,
) {
    for (label, url) in parsed_urls {
        let label_lower = label.to_lowercase();

        if urls.bug_tracking_url.is_none()
            && matches!(
                label_lower.as_str(),
                "tracker"
                    | "bug reports"
                    | "bug tracker"
                    | "issues"
                    | "issue tracker"
                    | "github: issues"
            )
        {
            urls.bug_tracking_url = Some(url.clone());
        } else if urls.code_view_url.is_none()
            && matches!(label_lower.as_str(), "source" | "source code" | "code")
        {
            urls.code_view_url = Some(url.clone());
        } else if urls.vcs_url.is_none()
            && matches!(
                label_lower.as_str(),
                "github" | "gitlab" | "github: repo" | "repository"
            )
        {
            urls.vcs_url = Some(url.clone());
        } else if urls.homepage_url.is_none()
            && matches!(label_lower.as_str(), "website" | "homepage" | "home")
        {
            urls.homepage_url = Some(url.clone());
        } else if label_lower == "changelog" {
            urls.changelog_url = Some(url.clone());
            extra_data.insert(
                "changelog_url".to_string(),
                serde_json::Value::String(url.clone()),
            );
        }
    }

    let project_urls_json: serde_json::Map<String, serde_json::Value> = parsed_urls
        .iter()
        .map(|(label, url)| (label.clone(), serde_json::Value::String(url.clone())))
        .collect();

    if !project_urls_json.is_empty() {
        extra_data.insert(
            "project_urls".to_string(),
            serde_json::Value::Object(project_urls_json),
        );
    }
}

pub(super) fn parse_setup_cfg_keywords(value: Option<String>) -> Vec<String> {
    let Some(keywords) = value else {
        return Vec::new();
    };

    keywords
        .split(',')
        .map(str::trim)
        .filter(|keyword| !keyword.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub(super) fn parse_setup_cfg_project_urls(entries: &[String]) -> Vec<(String, String)> {
    entries
        .iter()
        .filter_map(|entry| {
            let (label, url) = entry.split_once('=')?;
            let label = label.trim();
            let url = url.trim();
            if label.is_empty() || url.is_empty() {
                None
            } else {
                Some((label.to_string(), url.to_string()))
            }
        })
        .collect()
}

pub(super) fn extract_rfc822_dependencies(
    headers: &HashMap<String, Vec<String>>,
) -> Vec<Dependency> {
    let requires_dist = super::super::rfc822::get_header_all(headers, "requires-dist");
    extract_requires_dist_dependencies(&requires_dist)
}
