//! Parser for Ruby/RubyGems package manifests.
//!
//! Extracts package metadata, dependencies, and platform information from
//! Gemfile and Gemfile.lock files used by Ruby/Bundler projects.
//!
//! # Supported Formats
//! - Gemfile (manifest with Ruby DSL)
//! - Gemfile.lock (lockfile with state machine sections)
//! - *.gemspec (gem specification files)
//! - *.gem (gem archive packages)
//! - metadata.gz-extract (pre-extracted gem metadata)
//!
//! # Key Features
//! - State machine parsing for Gemfile.lock sections (GEM, GIT, PATH, SVN, PLATFORMS, BUNDLED WITH, DEPENDENCIES)
//! - Regex-based Ruby DSL parsing for Gemfile
//! - Dependency group handling (:development, :test, etc.)
//! - Platform-specific gem support
//! - Pessimistic version operator (~>) support
//! - Bug Fix #1: Strip .freeze suffix from strings
//! - Bug Fix #4: Correct dependency scope mapping (:runtime → None, :development → "development")
//!
//! # Implementation Notes
//! - Uses regex for pattern matching (not full Ruby AST)
//! - Graceful error handling: logs warnings and returns default on parse failure
//! - PURL type: "gem"

use crate::models::{DatasourceId, Dependency, PackageData, PackageType, Party};
use crate::parser_warn as warn;
use crate::parsers::utils::{
    MAX_ITERATION_COUNT, read_file_to_string, split_name_email, truncate_field,
};
use flate2::read::GzDecoder;
use packageurl::PackageUrl;
use regex::Regex;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use tar::Archive;

use super::PackageParser;
use super::license_normalization::normalize_spdx_declared_license;

const PACKAGE_TYPE: PackageType = PackageType::Gem;

// =============================================================================
// Bug Fix #1: Strip .freeze suffix from strings
// =============================================================================

/// Strips the `.freeze` suffix from Ruby frozen string literals.
///
/// In Ruby, `.freeze` makes a string immutable. We need to remove this suffix
/// when parsing gem names and versions from Gemfile.
///
/// # Examples
/// ```ignore
/// assert_eq!(strip_freeze_suffix("\"name\".freeze"), "\"name\"");
/// assert_eq!(strip_freeze_suffix("'1.0.0'.freeze"), "'1.0.0'");
/// ```
pub fn strip_freeze_suffix(s: &str) -> &str {
    s.trim_end_matches(".freeze")
}

enum GemfileBlock {
    Group(Vec<String>),
    Source(String),
}

// =============================================================================
// Gemfile Parser (Ruby DSL)
// =============================================================================

/// Ruby Gemfile parser for manifest files.
///
/// Parses Ruby DSL syntax to extract gem declarations, dependency groups,
/// platform-specific gems, and version constraints.
pub struct GemfileParser;

impl PackageParser for GemfileParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let datasource_id = gemfile_datasource_id(path);
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read Gemfile at {:?}: {}", path, e);
                return vec![default_package_data_with_datasource(datasource_id)];
            }
        };

        let mut package_data = parse_gemfile(&content);
        package_data.datasource_id = Some(datasource_id);
        vec![package_data]
    }

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|name| name == "Gemfile")
            || path
                .to_str()
                .is_some_and(|p| p.contains("data.gz-extract/") && p.ends_with("/Gemfile"))
    }
}

/// Parses Gemfile content and extracts dependencies with groups.
fn parse_gemfile(content: &str) -> PackageData {
    let mut dependencies = Vec::new();
    let mut block_stack = Vec::new();
    let mut default_source = None;
    let mut sources = Vec::new();

    // Regex patterns for Gemfile parsing
    // gem "name", "version", options...
    let gem_regex = match Regex::new(
        r#"^\s*gem\s+["']([^"']+)["'](?:\.freeze)?(?:\s*,\s*["']([^"']+)["'](?:\.freeze)?)?(?:\s*,\s*["']([^"']+)["'](?:\.freeze)?)?(?:\s*,\s*(.+))?"#,
    ) {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to compile gem regex: {}", e);
            return default_package_data_with_datasource(DatasourceId::Gemfile);
        }
    };

    // group :name do ... end
    let group_start_regex = match Regex::new(r"^\s*group\s+(.+?)\s+do\s*$") {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to compile group regex: {}", e);
            return default_package_data_with_datasource(DatasourceId::Gemfile);
        }
    };

    let group_end_regex = match Regex::new(r"^\s*end\s*$") {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to compile end regex: {}", e);
            return default_package_data_with_datasource(DatasourceId::Gemfile);
        }
    };

    let source_block_start_regex = match Regex::new(r#"^\s*source\s+["']([^"']+)["']\s+do\s*$"#) {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to compile source block regex: {}", e);
            return default_package_data_with_datasource(DatasourceId::Gemfile);
        }
    };

    let source_regex = match Regex::new(r#"^\s*source\s+["']([^"']+)["']\s*$"#) {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to compile source regex: {}", e);
            return default_package_data_with_datasource(DatasourceId::Gemfile);
        }
    };

    // Parse symbols like :development, :test
    let symbol_regex = match Regex::new(r":(\w+)") {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to compile symbol regex: {}", e);
            return default_package_data_with_datasource(DatasourceId::Gemfile);
        }
    };

    for line in content.lines().take(MAX_ITERATION_COUNT) {
        let trimmed = line.trim();

        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Check for group start
        if let Some(caps) = group_start_regex.captures(trimmed) {
            let groups_str = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let mut current_groups = Vec::new();
            for cap in symbol_regex.captures_iter(groups_str) {
                if let Some(group_name) = cap.get(1) {
                    current_groups.push(group_name.as_str().to_string());
                }
            }
            block_stack.push(GemfileBlock::Group(current_groups));
            continue;
        }

        if let Some(caps) = source_block_start_regex.captures(trimmed) {
            let source = caps
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            if !source.is_empty() {
                push_unique_string(&mut sources, source.clone());
                block_stack.push(GemfileBlock::Source(source));
            }
            continue;
        }

        if let Some(caps) = source_regex.captures(trimmed) {
            if let Some(source) = caps.get(1).map(|m| m.as_str().to_string()) {
                push_unique_string(&mut sources, source.clone());
                default_source = Some(source);
            }
            continue;
        }

        // Check for group end
        if group_end_regex.is_match(trimmed) {
            block_stack.pop();
            continue;
        }

        // Parse gem declaration
        if let Some(caps) = gem_regex.captures(trimmed) {
            let name = strip_freeze_suffix(caps.get(1).map(|m| m.as_str()).unwrap_or(""));
            if name.is_empty() {
                continue;
            }

            // Collect version constraints
            let mut version_parts = Vec::new();
            if let Some(v) = caps.get(2) {
                version_parts.push(strip_freeze_suffix(v.as_str()).to_string());
            }
            if let Some(v) = caps.get(3) {
                let v_str = strip_freeze_suffix(v.as_str());
                // Check if it looks like a version constraint
                if looks_like_version_constraint(v_str) {
                    version_parts.push(v_str.to_string());
                }
            }

            let extracted_requirement = if version_parts.is_empty() {
                None
            } else {
                Some(version_parts.join(", "))
            };

            let current_groups = current_group_names(&block_stack);

            // Determine scope based on current group
            // Bug Fix #4: :runtime → None, :development → "development"
            let (scope, is_runtime, is_optional) = if current_groups.is_empty() {
                // No group = runtime dependency
                (None, true, false)
            } else if current_groups.iter().any(|g| g == "development") {
                (Some("development".to_string()), false, true)
            } else if current_groups.iter().any(|g| g == "test") {
                (Some("test".to_string()), false, true)
            } else {
                // Other groups (e.g., :production)
                let group = current_groups.first().cloned();
                (group, true, false)
            };

            // Create PURL
            let purl = create_gem_purl(name, None);
            let inherited_source = current_source(&block_stack, default_source.as_deref());
            let extra_data = build_gemfile_dependency_extra_data(
                caps.get(4).map(|m| m.as_str()),
                inherited_source.as_deref(),
            );

            dependencies.push(Dependency {
                purl,
                extracted_requirement,
                scope,
                is_runtime: Some(is_runtime),
                is_optional: Some(is_optional),
                is_pinned: None,
                is_direct: Some(true),
                resolved_package: None,
                extra_data,
            });
        }
    }

    let extra_data = if sources.is_empty() {
        None
    } else {
        Some(HashMap::from([(
            "sources".to_string(),
            serde_json::Value::Array(sources.into_iter().map(serde_json::Value::String).collect()),
        )]))
    };

    PackageData {
        package_type: Some(PACKAGE_TYPE),
        primary_language: Some("Ruby".to_string()),
        dependencies,
        extra_data,
        datasource_id: Some(DatasourceId::Gemfile),
        ..default_package_data()
    }
}

fn current_group_names(block_stack: &[GemfileBlock]) -> Vec<String> {
    block_stack
        .iter()
        .rev()
        .find_map(|block| match block {
            GemfileBlock::Group(groups) => Some(groups.clone()),
            GemfileBlock::Source(_) => None,
        })
        .unwrap_or_default()
}

fn current_source(block_stack: &[GemfileBlock], default_source: Option<&str>) -> Option<String> {
    block_stack
        .iter()
        .rev()
        .find_map(|block| match block {
            GemfileBlock::Source(source) => Some(source.clone()),
            GemfileBlock::Group(_) => None,
        })
        .or_else(|| default_source.map(str::to_string))
}

fn push_unique_string(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn build_gemfile_dependency_extra_data(
    options: Option<&str>,
    inherited_source: Option<&str>,
) -> Option<HashMap<String, serde_json::Value>> {
    let mut extra = HashMap::new();
    let options = options.unwrap_or("");

    if let Some(git) = extract_gemfile_quoted_option(options, "git") {
        extra.insert(
            "source_type".to_string(),
            serde_json::Value::String("GIT".to_string()),
        );
        extra.insert("git".to_string(), serde_json::Value::String(git.clone()));
        extra.insert("remote".to_string(), serde_json::Value::String(git));
    }

    if let Some(path) = extract_gemfile_quoted_option(options, "path") {
        extra.insert(
            "source_type".to_string(),
            serde_json::Value::String("PATH".to_string()),
        );
        extra.insert("path".to_string(), serde_json::Value::String(path));
    }

    for key in ["branch", "ref", "tag"] {
        if let Some(value) = extract_gemfile_quoted_option(options, key) {
            extra.insert(key.to_string(), serde_json::Value::String(value));
        }
    }

    let direct_source = extract_gemfile_quoted_option(options, "source");
    if let Some(source) = direct_source {
        extra.insert("source".to_string(), serde_json::Value::String(source));
    } else if !extra.contains_key("source_type")
        && let Some(source) = inherited_source
    {
        extra.insert(
            "source".to_string(),
            serde_json::Value::String(source.to_string()),
        );
    }

    (!extra.is_empty()).then_some(extra)
}

fn extract_gemfile_quoted_option(options: &str, key: &str) -> Option<String> {
    if options.is_empty() {
        return None;
    }

    let pattern = format!(r#"(?:^|,\s*){}\s*:\s*["']([^"']+)["']"#, regex::escape(key));
    Regex::new(&pattern)
        .ok()
        .and_then(|regex| regex.captures(options))
        .and_then(|captures| captures.get(1).map(|m| m.as_str().to_string()))
}

/// Checks if a string looks like a version constraint.
fn looks_like_version_constraint(s: &str) -> bool {
    s.starts_with('~')
        || s.starts_with('>')
        || s.starts_with('<')
        || s.starts_with('=')
        || s.starts_with('!')
        || s.chars().next().is_some_and(|c| c.is_ascii_digit())
}

// =============================================================================
// Gemfile.lock Parser (State Machine)
// =============================================================================

/// Ruby Gemfile.lock parser for lockfiles.
///
/// Uses a state machine to parse sections: GEM, GIT, PATH, SVN,
/// PLATFORMS, BUNDLED WITH, DEPENDENCIES.
pub struct GemfileLockParser;

impl PackageParser for GemfileLockParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let datasource_id = gemfile_lock_datasource_id(path);
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read Gemfile.lock at {:?}: {}", path, e);
                return vec![default_package_data_with_datasource(datasource_id)];
            }
        };

        let mut package_data = parse_gemfile_lock(&content);
        package_data.datasource_id = Some(datasource_id);
        vec![package_data]
    }

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|name| name == "Gemfile.lock")
            || path
                .to_str()
                .is_some_and(|p| p.contains("data.gz-extract/") && p.ends_with("/Gemfile.lock"))
    }
}

/// Parse state for Gemfile.lock state machine.
#[derive(Debug, Clone, PartialEq)]
enum ParseState {
    None,
    Gem,
    Git,
    Path,
    Svn,
    Specs,
    Platforms,
    BundledWith,
    Dependencies,
}

/// Parsed gem information from Gemfile.lock.
///
/// All fields are actively used:
/// - `gem_type`, `remote`, `revision`, `ref_field`, `branch`, `tag`: Stored in extra_data for GIT/PATH/SVN sources
/// - `name`, `version`, `platform`, `pinned`: Used for dependency PURL and metadata generation
/// - `requirements`: Stored as extracted_requirement for version constraints
#[derive(Debug, Clone, Default)]
struct GemInfo {
    name: String,
    version: Option<String>,
    platform: Option<String>,
    gem_type: String,
    remote: Option<String>,
    revision: Option<String>,
    ref_field: Option<String>,
    branch: Option<String>,
    tag: Option<String>,
    pinned: bool,
    requirements: Vec<String>,
}

fn select_primary_path_gem(gems: &HashMap<String, GemInfo>) -> Option<GemInfo> {
    let mut path_gems: Vec<&GemInfo> = gems.values().filter(|gem| gem.gem_type == "PATH").collect();
    path_gems.sort_by(|left, right| {
        left.remote
            .as_deref()
            .cmp(&right.remote.as_deref())
            .then_with(|| left.name.cmp(&right.name))
    });

    path_gems
        .iter()
        .copied()
        .find(|gem| gem.pinned && gem.remote.as_deref() == Some("."))
        .or_else(|| path_gems.iter().copied().find(|gem| gem.pinned))
        .or_else(|| {
            path_gems
                .iter()
                .copied()
                .find(|gem| gem.remote.as_deref() == Some("."))
        })
        .or_else(|| path_gems.first().copied())
        .cloned()
}

/// Parses Gemfile.lock content using a state machine.
fn parse_gemfile_lock(content: &str) -> PackageData {
    let mut state = ParseState::None;
    let mut dependencies = Vec::new();
    let mut gems: HashMap<String, GemInfo> = HashMap::new();
    let mut platforms: Vec<String> = Vec::new();
    let mut bundler_version: Option<String> = None;
    let mut current_gem_type = String::new();
    let mut current_remote: Option<String> = None;
    let mut current_options: HashMap<String, String> = HashMap::new();

    // DEPS pattern: 2 spaces at line start
    let deps_regex = match Regex::new(r"^ {2}([^ \)\(,!:]+)(?: \(([^)]+)\))?(!)?$") {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to compile deps regex: {}", e);
            return default_package_data_with_datasource(DatasourceId::GemfileLock);
        }
    };

    // SPEC_DEPS pattern: 4 spaces at line start
    let spec_deps_regex = match Regex::new(r"^ {4}([^ \)\(,!:]+)(?: \(([^)]+)\))?$") {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to compile spec_deps regex: {}", e);
            return default_package_data_with_datasource(DatasourceId::GemfileLock);
        }
    };

    // OPTIONS pattern: key: value
    let options_regex = match Regex::new(r"^ {2}([a-z]+): (.+)$") {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to compile options regex: {}", e);
            return default_package_data_with_datasource(DatasourceId::GemfileLock);
        }
    };

    // VERSION pattern for BUNDLED WITH
    let version_regex = match Regex::new(r"^\s+(\d+(?:\.\d+)+)\s*$") {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to compile version regex: {}", e);
            return default_package_data_with_datasource(DatasourceId::GemfileLock);
        }
    };

    for line in content.lines().take(MAX_ITERATION_COUNT) {
        let trimmed = line.trim_end();

        // Empty line resets state
        if trimmed.is_empty() {
            current_options.clear();
            continue;
        }

        // Section headers (no leading whitespace) and sub-section headers
        match trimmed {
            "GEM" => {
                state = ParseState::Gem;
                current_gem_type = "GEM".to_string();
                current_remote = None;
                current_options.clear();
                continue;
            }
            "GIT" => {
                state = ParseState::Git;
                current_gem_type = "GIT".to_string();
                current_remote = None;
                current_options.clear();
                continue;
            }
            "PATH" => {
                state = ParseState::Path;
                current_gem_type = "PATH".to_string();
                current_remote = None;
                current_options.clear();
                continue;
            }
            "SVN" => {
                state = ParseState::Svn;
                current_gem_type = "SVN".to_string();
                current_remote = None;
                current_options.clear();
                continue;
            }
            "PLATFORMS" => {
                state = ParseState::Platforms;
                continue;
            }
            "BUNDLED WITH" => {
                state = ParseState::BundledWith;
                continue;
            }
            "DEPENDENCIES" => {
                state = ParseState::Dependencies;
                continue;
            }
            _ => {}
        }

        // Check for "  specs:" sub-section header (2-space indent) within
        // GEM/GIT/PATH/SVN sections. This must be checked separately because
        // the leading whitespace is preserved by trim_end().
        if trimmed.trim() == "specs:" {
            state = match state {
                ParseState::Gem | ParseState::Git | ParseState::Path | ParseState::Svn => {
                    ParseState::Specs
                }
                _ => state,
            };
            continue;
        }

        // Process based on current state
        match state {
            ParseState::Gem | ParseState::Git | ParseState::Path | ParseState::Svn => {
                // Parse options (remote:, revision:, ref:, branch:, tag:)
                if let Some(caps) = options_regex.captures(line) {
                    let key = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                    let value = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                    current_options.insert(key.to_string(), value.to_string());
                    if key == "remote" {
                        current_remote = Some(value.to_string());
                    }
                }
            }
            ParseState::Specs => {
                // Parse gem specs (4 spaces indent)
                if let Some(caps) = spec_deps_regex.captures(line) {
                    let name = caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
                    let version_str = caps.get(2).map(|m| m.as_str()).unwrap_or("");

                    // Parse version and platform
                    let (version, platform) = parse_version_platform(version_str);

                    if !name.is_empty() {
                        let gem_info = GemInfo {
                            name: name.clone(),
                            version,
                            platform,
                            gem_type: current_gem_type.clone(),
                            remote: current_remote.clone(),
                            revision: current_options.get("revision").cloned(),
                            ref_field: current_options.get("ref").cloned(),
                            branch: current_options.get("branch").cloned(),
                            tag: current_options.get("tag").cloned(),
                            pinned: false,
                            requirements: Vec::new(),
                        };
                        gems.insert(name, gem_info);
                    }
                }
            }
            ParseState::Platforms => {
                // Parse platform entries (2 spaces indent)
                let platform = trimmed.trim();
                if !platform.is_empty() {
                    platforms.push(platform.to_string());
                }
            }
            ParseState::BundledWith => {
                // Parse bundler version
                if let Some(caps) = version_regex.captures(line) {
                    bundler_version = caps.get(1).map(|m| m.as_str().to_string());
                }
            }
            ParseState::Dependencies => {
                // Parse direct dependencies (2 spaces indent)
                if let Some(caps) = deps_regex.captures(line) {
                    let name = caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
                    let version_constraint = caps.get(2).map(|m| m.as_str().to_string());
                    let pinned = caps.get(3).is_some();

                    if !name.is_empty() {
                        // Update gem info if exists, or create new
                        if let Some(gem) = gems.get_mut(&name) {
                            gem.pinned = pinned;
                            if let Some(vc) = &version_constraint {
                                gem.requirements.push(vc.clone());
                            }
                        } else {
                            let gem_info = GemInfo {
                                name: name.clone(),
                                version: None,
                                platform: None,
                                gem_type: "GEM".to_string(),
                                remote: None,
                                revision: None,
                                ref_field: None,
                                branch: None,
                                tag: None,
                                pinned,
                                requirements: version_constraint.into_iter().collect(),
                            };
                            gems.insert(name, gem_info);
                        }
                    }
                }
            }
            ParseState::None => {}
        }
    }

    let primary_gem = select_primary_path_gem(&gems);

    let (
        package_name,
        package_version,
        repository_homepage_url,
        repository_download_url,
        api_data_url,
        download_url,
    ) = if let Some(ref pg) = primary_gem {
        let urls = get_rubygems_urls(&pg.name, pg.version.as_deref(), pg.platform.as_deref());
        (
            Some(pg.name.clone()),
            pg.version.clone(),
            urls.0,
            urls.1,
            urls.2,
            urls.3,
        )
    } else {
        (None, None, None, None, None, None)
    };

    for (_, gem) in gems {
        if let Some(ref pg) = primary_gem
            && gem.name == pg.name
        {
            continue;
        }

        let version_for_purl = gem.version.as_deref();
        let purl = create_gem_purl(&gem.name, version_for_purl);

        let extracted_requirement = if !gem.requirements.is_empty() {
            Some(gem.requirements.join(", "))
        } else {
            gem.version.clone()
        };

        let extra_data = build_gem_source_extra_data(&gem);

        dependencies.push(Dependency {
            purl,
            extracted_requirement,
            scope: Some("dependencies".to_string()),
            is_runtime: Some(true),
            is_optional: Some(false),
            is_pinned: Some(gem.pinned),
            is_direct: Some(true),
            resolved_package: None,
            extra_data,
        });
    }

    dependencies.sort_by(|left, right| {
        left.purl
            .as_deref()
            .cmp(&right.purl.as_deref())
            .then_with(|| {
                left.extracted_requirement
                    .as_deref()
                    .cmp(&right.extracted_requirement.as_deref())
            })
    });

    // Build extra_data
    let mut extra_data = HashMap::new();
    if !platforms.is_empty() {
        extra_data.insert(
            "platforms".to_string(),
            serde_json::Value::Array(
                platforms
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
    }
    if let Some(bv) = bundler_version {
        extra_data.insert("bundler_version".to_string(), serde_json::Value::String(bv));
    }

    let purl = package_name
        .as_deref()
        .map(|n| create_gem_purl(n, package_version.as_deref()))
        .unwrap_or(None);

    PackageData {
        package_type: Some(PACKAGE_TYPE),
        name: package_name,
        version: package_version,
        primary_language: Some("Ruby".to_string()),
        download_url,
        dependencies,
        repository_homepage_url,
        repository_download_url,
        api_data_url,
        extra_data: if extra_data.is_empty() {
            None
        } else {
            Some(extra_data)
        },
        datasource_id: Some(DatasourceId::GemfileLock),
        purl,
        ..default_package_data()
    }
}

fn build_gem_source_extra_data(gem: &GemInfo) -> Option<HashMap<String, serde_json::Value>> {
    if gem.gem_type != "GIT" && gem.gem_type != "PATH" && gem.gem_type != "SVN" {
        return None;
    }

    let mut extra = HashMap::new();
    extra.insert(
        "source_type".to_string(),
        serde_json::Value::String(gem.gem_type.clone()),
    );

    if let Some(ref remote) = gem.remote {
        extra.insert(
            "remote".to_string(),
            serde_json::Value::String(remote.clone()),
        );
    }
    if let Some(ref revision) = gem.revision {
        extra.insert(
            "revision".to_string(),
            serde_json::Value::String(revision.clone()),
        );
    }
    if let Some(ref ref_field) = gem.ref_field {
        extra.insert(
            "ref".to_string(),
            serde_json::Value::String(ref_field.clone()),
        );
    }
    if let Some(ref branch) = gem.branch {
        extra.insert(
            "branch".to_string(),
            serde_json::Value::String(branch.clone()),
        );
    }
    if let Some(ref tag) = gem.tag {
        extra.insert("tag".to_string(), serde_json::Value::String(tag.clone()));
    }

    Some(extra)
}

/// Parses version and platform from a combined string.
/// Examples: "2.6.3" -> ("2.6.3", None), "2.6.3-java" -> ("2.6.3", Some("java"))
fn parse_version_platform(s: &str) -> (Option<String>, Option<String>) {
    if s.is_empty() {
        return (None, None);
    }
    if let Some(idx) = s.find('-') {
        let version = &s[..idx];
        let platform = &s[idx + 1..];
        (Some(version.to_string()), Some(platform.to_string()))
    } else {
        (Some(s.to_string()), None)
    }
}

/// Creates a gem PURL.
fn create_gem_purl(name: &str, version: Option<&str>) -> Option<String> {
    let mut purl = match PackageUrl::new(PACKAGE_TYPE.as_str(), name) {
        Ok(p) => p,
        Err(e) => {
            warn!("Failed to create PURL for gem '{}': {}", name, e);
            return None;
        }
    };

    if let Some(v) = version
        && let Err(e) = purl.with_version(v)
    {
        warn!("Failed to set version '{}' for gem '{}': {}", v, name, e);
    }

    Some(purl.to_string())
}

fn rubygems_homepage_url(name: &str, version: Option<&str>) -> Option<String> {
    if name.is_empty() {
        return None;
    }

    if let Some(v) = version {
        let v = v.trim().trim_matches('/');
        Some(format!("https://rubygems.org/gems/{}/versions/{}", name, v))
    } else {
        Some(format!("https://rubygems.org/gems/{}", name))
    }
}

fn rubygems_download_url(
    name: &str,
    version: Option<&str>,
    platform: Option<&str>,
) -> Option<String> {
    if name.is_empty() || version.is_none() {
        return None;
    }

    let name = name.trim().trim_matches('/');
    let version = version?.trim().trim_matches('/');

    let version_plat = if let Some(p) = platform {
        if p != "ruby" {
            format!("{}-{}", version, p)
        } else {
            version.to_string()
        }
    } else {
        version.to_string()
    };

    Some(format!(
        "https://rubygems.org/downloads/{}-{}.gem",
        name, version_plat
    ))
}

fn rubygems_api_url(name: &str, version: Option<&str>) -> Option<String> {
    if name.is_empty() {
        return None;
    }

    if let Some(v) = version {
        Some(format!(
            "https://rubygems.org/api/v2/rubygems/{}/versions/{}.json",
            name, v
        ))
    } else {
        Some(format!(
            "https://rubygems.org/api/v1/versions/{}.json",
            name
        ))
    }
}

fn get_rubygems_urls(
    name: &str,
    version: Option<&str>,
    platform: Option<&str>,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    let repository_homepage_url = rubygems_homepage_url(name, version);
    let repository_download_url = rubygems_download_url(name, version, platform);
    let api_data_url = rubygems_api_url(name, version);
    let download_url = repository_download_url.clone();

    (
        repository_homepage_url,
        repository_download_url,
        api_data_url,
        download_url,
    )
}

/// Returns a default PackageData with gem-specific settings.
fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(PACKAGE_TYPE),
        primary_language: Some("Ruby".to_string()),
        ..Default::default()
    }
}

fn default_package_data_with_datasource(datasource_id: DatasourceId) -> PackageData {
    PackageData {
        datasource_id: Some(datasource_id),
        ..default_package_data()
    }
}

// =============================================================================
// Gemspec Parser (Ruby DSL)
// =============================================================================

/// Ruby .gemspec file parser.
///
/// Parses `Gem::Specification.new` blocks using regex-based extraction.
/// Handles frozen strings (Bug #1), variable version resolution (Bug #2),
/// and RFC 5322 email parsing (Bug #6).
pub struct GemspecParser;

impl PackageParser for GemspecParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let datasource_id = gemspec_datasource_id(path);
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read .gemspec at {:?}: {}", path, e);
                return vec![default_package_data_with_datasource(datasource_id)];
            }
        };

        let mut package_data = parse_gemspec_with_context(&content, path.parent());
        package_data.datasource_id = Some(datasource_id);
        vec![package_data]
    }

    fn is_match(path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext == "gemspec")
    }
}

fn normalized_ruby_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn gemfile_datasource_id(path: &Path) -> DatasourceId {
    if normalized_ruby_path(path).contains("/data.gz-extract/") {
        DatasourceId::GemfileExtracted
    } else {
        DatasourceId::Gemfile
    }
}

fn gemfile_lock_datasource_id(path: &Path) -> DatasourceId {
    if normalized_ruby_path(path).contains("/data.gz-extract/") {
        DatasourceId::GemfileLockExtracted
    } else {
        DatasourceId::GemfileLock
    }
}

fn gemspec_datasource_id(path: &Path) -> DatasourceId {
    let normalized = normalized_ruby_path(path);
    if normalized.contains("/data.gz-extract/") {
        DatasourceId::GemspecExtracted
    } else if normalized.contains("/specifications/") {
        DatasourceId::GemGemspecInstalledSpecifications
    } else {
        DatasourceId::Gemspec
    }
}

/// Cleans a value extracted from gemspec by stripping quotes, .freeze, %q{}, and brackets.
fn clean_gemspec_value(s: &str) -> String {
    let s = strip_freeze_suffix(s).trim();

    let s = if let Some(pos) = s.find(" #") {
        s[..pos].trim()
    } else {
        s
    };

    let s = if let Some(stripped) = s.strip_prefix("%q{") {
        stripped.strip_suffix('}').unwrap_or(stripped)
    } else if let Some(stripped) = s.strip_prefix("%q<") {
        stripped.strip_suffix('>').unwrap_or(stripped)
    } else if let Some(stripped) = s.strip_prefix("%q[") {
        stripped.strip_suffix(']').unwrap_or(stripped)
    } else if let Some(stripped) = s.strip_prefix("%q(") {
        stripped.strip_suffix(')').unwrap_or(stripped)
    } else {
        s
    };

    let s = s
        .trim_start_matches('"')
        .trim_end_matches('"')
        .trim_start_matches('\'')
        .trim_end_matches('\'');
    let s = strip_freeze_suffix(s).trim();
    s.to_string()
}

/// Extracts items from a Ruby array literal like `["a", "b", "c"]`.
fn extract_ruby_array(s: &str) -> Vec<String> {
    let s = strip_freeze_suffix(s.trim());
    let s = s.trim_start_matches('[').trim_end_matches(']');
    let item_re = match Regex::new(r#"["']([^"']*?)["'](?:\.freeze)?"#) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    item_re
        .captures_iter(s)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

fn extract_all_ruby_values(s: &str) -> Vec<String> {
    let value_re = match Regex::new(r#"%q[\{<\[(]([^\}>\])]+)[\}>\])]|["']([^"']+)["']"#) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    value_re
        .captures_iter(s)
        .filter_map(|caps| caps.get(1).or_else(|| caps.get(2)))
        .map(|m| clean_gemspec_value(m.as_str()))
        .collect()
}

fn extract_first_ruby_value(s: &str) -> Option<String> {
    extract_all_ruby_values(s).into_iter().next()
}

fn after_first_argument(args: &str) -> &str {
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut in_quote: Option<char> = None;
    let chars: Vec<(usize, char)> = args.char_indices().collect();
    let mut i = 0;

    while i < chars.len() {
        let (idx, ch) = chars[i];

        if let Some(quote) = in_quote {
            if ch == '\\' {
                i += 2;
                continue;
            }
            if ch == quote {
                in_quote = None;
            }
            i += 1;
            continue;
        }

        match ch {
            '\'' | '"' => in_quote = Some(ch),
            '[' | '{' | '<' => bracket_depth += 1,
            ']' | '}' | '>' => bracket_depth = bracket_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            ',' if bracket_depth == 0 && paren_depth == 0 => return args[idx + 1..].trim(),
            _ => {}
        }

        i += 1;
    }

    ""
}

/// Bug #2: Resolves variable version references like `CSV::VERSION` or `RAILS_VERSION`.
///
/// Scans the file content for constant definitions matching the variable name
/// and returns the resolved string value.
fn resolve_variable_version(var_name: &str, contexts: &[String]) -> Option<String> {
    let var_name = var_name.trim();
    if var_name.is_empty() {
        return None;
    }

    for candidate in candidate_constant_names(var_name) {
        let escaped = regex::escape(&candidate);
        let pattern = format!(r#"(?m)^\s*{}\s*=\s*["']([^"']+)["']"#, escaped);
        let Ok(re) = Regex::new(&pattern) else {
            continue;
        };

        for context in contexts {
            if let Some(caps) = re.captures(context) {
                return caps.get(1).map(|m| m.as_str().to_string());
            }
        }
    }

    None
}

fn resolve_variable_array(var_name: &str, contexts: &[String]) -> Option<Vec<String>> {
    let var_name = var_name.trim();
    if var_name.is_empty() {
        return None;
    }

    for candidate in candidate_constant_names(var_name) {
        let escaped = regex::escape(&candidate);
        let pattern = format!(r#"(?m)^\s*{}\s*=\s*(\[[^\n]+\])"#, escaped);
        let Ok(re) = Regex::new(&pattern) else {
            continue;
        };

        for context in contexts {
            if let Some(caps) = re.captures(context)
                && let Some(raw) = caps.get(1)
            {
                let values = extract_ruby_array(raw.as_str());
                if !values.is_empty() {
                    return Some(values);
                }
            }
        }
    }

    None
}

fn candidate_constant_names(var_name: &str) -> Vec<String> {
    let mut names = vec![var_name.to_string()];
    if let Some(last) = var_name.split("::").last()
        && last != var_name
    {
        names.push(last.to_string());
    }
    names
}

fn looks_like_local_variable_reference(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some('_' | 'a'..='z'))
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn resolve_ruby_read_root(base_dir: Option<&Path>) -> Option<PathBuf> {
    let base_dir = base_dir?;
    let current_dir = std::env::current_dir().ok();

    current_dir
        .and_then(|cwd| {
            let canonical_cwd = cwd.canonicalize().ok()?;
            let canonical_base = base_dir.canonicalize().ok()?;
            canonical_base
                .starts_with(&canonical_cwd)
                .then_some(canonical_cwd)
        })
        .or_else(|| base_dir.canonicalize().ok())
}

fn resolve_ruby_read_path(path: PathBuf, allowed_root: &Path) -> Option<PathBuf> {
    let canonical_path = path.canonicalize().ok()?;
    canonical_path
        .starts_with(allowed_root)
        .then_some(canonical_path)
}

fn resolve_file_read_argument(args: &str, base_dir: Option<&Path>) -> Option<String> {
    let base_dir = base_dir?;
    let allowed_root = resolve_ruby_read_root(base_dir.into())?;
    let relative_path = extract_first_ruby_value(args)?;
    if relative_path.is_empty() {
        return None;
    }

    let candidate = Path::new(&relative_path);
    let path = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        base_dir.join(candidate)
    };

    let safe_path = resolve_ruby_read_path(path, &allowed_root)?;

    fs::read_to_string(safe_path)
        .ok()
        .map(|content| content.trim().to_string())
        .filter(|content| !content.is_empty())
}

fn resolve_scalar_expression(
    expression: &str,
    base_dir: Option<&Path>,
    contexts: &[String],
) -> Option<String> {
    let expression = if let Some(pos) = expression.find(" #") {
        expression[..pos].trim()
    } else {
        expression.trim()
    };

    let file_read_re = Regex::new(r#"^File\.read\((.+)\)(?:\.strip)?(?:\.freeze)?$"#).ok()?;
    if let Some(caps) = file_read_re.captures(expression) {
        return caps
            .get(1)
            .and_then(|m| resolve_file_read_argument(m.as_str(), base_dir));
    }

    if let Some(value) = extract_first_ruby_value(expression) {
        return Some(value);
    }

    let cleaned = clean_gemspec_value(expression);
    if looks_like_constant_reference(&cleaned) {
        return resolve_variable_version(&cleaned, contexts).or(Some(cleaned));
    }

    None
}

fn resolve_local_variable_value(
    var_name: &str,
    content: &str,
    base_dir: Option<&Path>,
    contexts: &[String],
) -> Option<String> {
    let escaped = regex::escape(var_name.trim());
    let pattern = format!(r#"(?m)^\s*{}\s*=\s*(.+)$"#, escaped);
    let re = Regex::new(&pattern).ok()?;

    re.captures_iter(content).find_map(|caps| {
        caps.get(1)
            .and_then(|m| resolve_scalar_expression(m.as_str(), base_dir, contexts))
    })
}

fn resolve_gemspec_scalar_value(
    raw_value: &str,
    content: &str,
    base_dir: Option<&Path>,
    contexts: &[String],
) -> Option<String> {
    let cleaned = truncate_field(clean_gemspec_value(raw_value));
    if cleaned.is_empty() {
        return None;
    }

    if looks_like_constant_reference(&cleaned) {
        return resolve_variable_version(&cleaned, contexts)
            .map(truncate_field)
            .or(Some(cleaned));
    }

    if looks_like_local_variable_reference(&cleaned) {
        return resolve_local_variable_value(&cleaned, content, base_dir, contexts)
            .map(truncate_field)
            .or(Some(cleaned));
    }

    Some(cleaned)
}

fn load_required_ruby_contexts(content: &str, base_dir: Option<&Path>) -> Vec<String> {
    let mut contexts = vec![content.to_string()];
    let Some(base_dir) = base_dir else {
        return contexts;
    };
    let allowed_root = resolve_ruby_read_root(Some(base_dir));

    let require_re = match Regex::new(r#"(?m)^\s*require(?:_relative)?\s+["']([^"']+)["']"#) {
        Ok(re) => re,
        Err(_) => return contexts,
    };

    for caps in require_re.captures_iter(content) {
        let Some(required) = caps.get(1).map(|m| m.as_str()) else {
            continue;
        };
        for candidate in candidate_require_paths(base_dir, required) {
            let Some(safe_candidate) = allowed_root
                .as_deref()
                .and_then(|root| resolve_ruby_read_path(candidate, root))
            else {
                continue;
            };
            if let Ok(required_content) = read_file_to_string(&safe_candidate, None) {
                contexts.push(required_content);
                break;
            }
        }
    }

    contexts
}

fn candidate_require_paths(base_dir: &Path, required: &str) -> Vec<PathBuf> {
    let relative = required.replace("::", "/");
    let filename = if relative.ends_with(".rb") {
        relative
    } else {
        format!("{}.rb", relative)
    };

    vec![
        base_dir.join(&filename),
        base_dir.join("lib").join(&filename),
    ]
}

fn looks_like_constant_reference(s: &str) -> bool {
    s.contains("::") || s.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// Parses a .gemspec file content and returns PackageData.
#[cfg(test)]
fn parse_gemspec(content: &str) -> PackageData {
    parse_gemspec_with_context(content, None)
}

fn parse_gemspec_with_context(content: &str, base_dir: Option<&Path>) -> PackageData {
    let contexts = load_required_ruby_contexts(content, base_dir);

    // Regex for spec.name = "value" or s.name = "value"
    // The spec variable name varies: spec, s, gem, etc.
    let field_re = match Regex::new(
        r#"(?m)^\s*\w+\.(name|version|summary|description|homepage|license)\s*=\s*(.+)$"#,
    ) {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to compile gemspec field regex: {}", e);
            return default_package_data_with_datasource(DatasourceId::Gemspec);
        }
    };

    let licenses_re = match Regex::new(r#"(?m)^\s*\w+\.licenses\s*=\s*(.+)$"#) {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to compile licenses regex: {}", e);
            return default_package_data_with_datasource(DatasourceId::Gemspec);
        }
    };

    let authors_re = match Regex::new(r#"(?m)^\s*\w+\.(?:authors|author)\s*=\s*(.+)$"#) {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to compile authors regex: {}", e);
            return default_package_data_with_datasource(DatasourceId::Gemspec);
        }
    };

    let email_re = match Regex::new(r#"(?m)^\s*\w+\.email\s*=\s*(.+)$"#) {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to compile email regex: {}", e);
            return default_package_data_with_datasource(DatasourceId::Gemspec);
        }
    };

    let dependency_call_re = match Regex::new(
        r#"(?m)^\s*\w+\.(add_(?:development_|runtime_)?dependency)\s*\(?(.+?)\)?\s*$"#,
    ) {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to compile gemspec dependency regex: {}", e);
            return default_package_data_with_datasource(DatasourceId::Gemspec);
        }
    };

    let mut name: Option<String> = None;
    let mut version: Option<String> = None;
    let mut summary: Option<String> = None;
    let mut description: Option<String> = None;
    let mut homepage: Option<String> = None;
    let mut license: Option<String> = None;
    let mut licenses: Vec<String> = Vec::new();
    let mut authors: Vec<String> = Vec::new();
    let mut emails: Vec<String> = Vec::new();
    let mut dependencies: Vec<Dependency> = Vec::new();

    // Extract basic fields
    for caps in field_re.captures_iter(content).take(MAX_ITERATION_COUNT) {
        let field_name = match caps.get(1) {
            Some(m) => m.as_str(),
            None => continue,
        };
        let raw_value = match caps.get(2) {
            Some(m) => m.as_str().trim(),
            None => continue,
        };

        match field_name {
            "name" => name = resolve_gemspec_scalar_value(raw_value, content, base_dir, &contexts),
            "version" => {
                version = resolve_gemspec_scalar_value(raw_value, content, base_dir, &contexts);
            }
            "summary" => {
                summary = resolve_gemspec_scalar_value(raw_value, content, base_dir, &contexts)
            }
            "description" => description = Some(truncate_field(clean_gemspec_value(raw_value))),
            "homepage" => {
                homepage = resolve_gemspec_scalar_value(raw_value, content, base_dir, &contexts)
            }
            "license" => license = Some(truncate_field(clean_gemspec_value(raw_value))),
            _ => {}
        }
    }

    // Extract licenses (plural)
    for caps in licenses_re.captures_iter(content).take(MAX_ITERATION_COUNT) {
        if let Some(raw) = caps.get(1) {
            licenses = extract_ruby_array(raw.as_str());
        }
    }

    // Extract authors
    for caps in authors_re.captures_iter(content).take(MAX_ITERATION_COUNT) {
        if let Some(raw) = caps.get(1) {
            let raw_str = raw.as_str().trim();
            if raw_str.starts_with('[') {
                authors = extract_ruby_array(raw_str);
            } else if looks_like_constant_reference(raw_str) {
                authors = resolve_variable_array(raw_str, &contexts)
                    .unwrap_or_else(|| vec![clean_gemspec_value(raw_str)]);
            } else {
                authors.push(clean_gemspec_value(raw_str));
            }
        }
    }

    // Extract emails
    for caps in email_re.captures_iter(content).take(MAX_ITERATION_COUNT) {
        if let Some(raw) = caps.get(1) {
            let raw_str = raw.as_str().trim();
            if raw_str.starts_with('[') {
                emails = extract_ruby_array(raw_str);
            } else if looks_like_constant_reference(raw_str) {
                emails = resolve_variable_array(raw_str, &contexts)
                    .unwrap_or_else(|| vec![clean_gemspec_value(raw_str)]);
            } else {
                emails.push(clean_gemspec_value(raw_str));
            }
        }
    }

    // Build parties from authors and emails
    let mut parties: Vec<Party> = Vec::new();

    if authors.len() == 1 && emails.len() == 1 {
        let email_str = emails.first().map(String::as_str);
        let (parsed_email_name, parsed_email) = match email_str {
            Some(e) => split_name_email(e),
            None => (None, None),
        };

        parties.push(Party {
            r#type: Some("person".to_string()),
            role: Some("author".to_string()),
            name: authors.first().cloned().or(parsed_email_name),
            email: parsed_email.or_else(|| {
                email_str
                    .filter(|e| e.contains('@') && !e.contains('<'))
                    .map(|e| e.to_string())
            }),
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    } else {
        for author_name in authors {
            parties.push(Party {
                r#type: Some("person".to_string()),
                role: Some("author".to_string()),
                name: Some(author_name),
                email: None,
                url: None,
                organization: None,
                organization_url: None,
                timezone: None,
            });
        }

        for email_str in emails {
            let (parsed_email_name, parsed_email) = if email_str.contains('<') {
                split_name_email(&email_str)
            } else {
                (None, None)
            };
            parties.push(Party {
                r#type: Some("person".to_string()),
                role: Some("author".to_string()),
                name: parsed_email_name,
                email: parsed_email.or_else(|| email_str.contains('@').then_some(email_str)),
                url: None,
                organization: None,
                organization_url: None,
                timezone: None,
            });
        }
    }

    for caps in dependency_call_re
        .captures_iter(content)
        .take(MAX_ITERATION_COUNT)
    {
        let method = match caps.get(1) {
            Some(m) => m.as_str(),
            None => continue,
        };
        let args = match caps.get(2) {
            Some(m) => m.as_str(),
            None => continue,
        };

        let Some(dep_name) = extract_first_ruby_value(args).map(truncate_field) else {
            continue;
        };
        let version_parts = extract_all_ruby_values(after_first_argument(args));
        let extracted_requirement = if version_parts.is_empty() {
            None
        } else {
            Some(version_parts.join(", "))
        };
        let purl = create_gem_purl(&dep_name, None);
        let is_development = method == "add_development_dependency";
        let scope = if is_development {
            "development"
        } else {
            "runtime"
        };

        dependencies.push(Dependency {
            purl,
            extracted_requirement,
            scope: Some(scope.to_string()),
            is_runtime: Some(!is_development),
            is_optional: Some(is_development),
            is_pinned: None,
            is_direct: Some(true),
            resolved_package: None,
            extra_data: None,
        });
    }

    // Extract license statement only - detection happens in separate engine
    let extracted_license_statement = if !licenses.is_empty() {
        Some(licenses.join(" AND "))
    } else {
        license
    };

    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        normalize_spdx_declared_license(extracted_license_statement.as_deref());

    // Prefer description over summary
    let final_description = description.or(summary);

    // Build PURL
    let purl = name
        .as_deref()
        .map(|n| create_gem_purl(n, version.as_deref()))
        .unwrap_or(None);

    let (repository_homepage_url, repository_download_url, api_data_url, download_url) =
        if let Some(n) = name.as_deref() {
            get_rubygems_urls(n, version.as_deref(), None)
        } else {
            (None, None, None, None)
        };

    PackageData {
        package_type: Some(PACKAGE_TYPE),
        name,
        version,
        primary_language: Some("Ruby".to_string()),
        description: final_description,
        homepage_url: homepage,
        download_url,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        extracted_license_statement,
        parties,
        dependencies,
        repository_homepage_url,
        repository_download_url,
        api_data_url,
        datasource_id: Some(DatasourceId::Gemspec),
        purl,
        ..default_package_data()
    }
}

// =============================================================================
// .gem Archive Parser (Wave 3)
// =============================================================================

const MAX_ARCHIVE_SIZE: u64 = 100 * 1024 * 1024; // 100MB
const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024; // 50MB per file
const MAX_COMPRESSION_RATIO: f64 = 100.0; // 100:1 ratio

/// Parser for .gem archive files.
///
/// Extracts metadata from Ruby .gem packages, which are tar archives
/// containing a gzip-compressed YAML metadata file (`metadata.gz`).
///
/// Includes safety checks against zip bombs and oversized archives.
pub struct GemArchiveParser;

impl PackageParser for GemArchiveParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        vec![match extract_gem_archive(path) {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to extract .gem archive at {:?}: {}", path, e);
                default_package_data_with_datasource(DatasourceId::GemArchive)
            }
        }]
    }

    fn is_match(path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext == "gem")
    }
}

fn extract_gem_archive(path: &Path) -> Result<PackageData, String> {
    let file_metadata =
        fs::metadata(path).map_err(|e| format!("Failed to read file metadata: {}", e))?;
    let archive_size = file_metadata.len();

    if archive_size > MAX_ARCHIVE_SIZE {
        return Err(format!(
            "Archive too large: {} bytes (limit: {} bytes)",
            archive_size, MAX_ARCHIVE_SIZE
        ));
    }

    let file = File::open(path).map_err(|e| format!("Failed to open archive: {}", e))?;
    let mut archive = Archive::new(file);

    let mut entry_count: usize = 0;
    for entry_result in archive
        .entries()
        .map_err(|e| format!("Failed to read tar entries: {}", e))?
    {
        entry_count += 1;
        if entry_count > MAX_ITERATION_COUNT {
            warn!(
                "Exceeded max tar entry count ({}) in .gem archive, stopping iteration",
                MAX_ITERATION_COUNT
            );
            break;
        }

        let entry = entry_result.map_err(|e| format!("Failed to read tar entry: {}", e))?;
        let entry_path = entry
            .path()
            .map_err(|e| format!("Failed to get entry path: {}", e))?;
        let entry_str = entry_path.to_string_lossy();
        if entry_str.contains("..") {
            warn!("Skipping tar entry with path traversal: {}", entry_str);
            continue;
        }

        if entry_path.to_str() == Some("metadata.gz") {
            let entry_size = entry.size();
            if entry_size > MAX_FILE_SIZE {
                return Err(format!(
                    "metadata.gz too large: {} bytes (limit: {} bytes)",
                    entry_size, MAX_FILE_SIZE
                ));
            }

            let mut decoder = GzDecoder::new(entry);
            let mut content = Vec::new();
            let mut limited = std::io::Read::take(&mut decoder, MAX_FILE_SIZE + 1);
            limited
                .read_to_end(&mut content)
                .map_err(|e| format!("Failed to decompress metadata.gz: {}", e))?;

            if content.len() > MAX_FILE_SIZE as usize {
                return Err(format!(
                    "Decompressed metadata too large: exceeds {} byte limit",
                    MAX_FILE_SIZE
                ));
            }

            let content = match String::from_utf8(content) {
                Ok(s) => s,
                Err(err) => {
                    let bytes = err.into_bytes();
                    warn!("Invalid UTF-8 in gem metadata; using lossy conversion");
                    String::from_utf8_lossy(&bytes).into_owned()
                }
            };

            let uncompressed_size = content.len() as u64;
            if entry_size > 0 {
                let ratio = uncompressed_size as f64 / entry_size as f64;
                if ratio > MAX_COMPRESSION_RATIO {
                    return Err(format!(
                        "Suspicious compression ratio: {:.2}:1 (limit: {:.0}:1)",
                        ratio, MAX_COMPRESSION_RATIO
                    ));
                }
            }

            return parse_gem_metadata_yaml(&content, DatasourceId::GemArchive);
        }
    }

    Err("metadata.gz not found in .gem archive".to_string())
}

fn parse_gem_metadata_yaml(
    content: &str,
    datasource_id: DatasourceId,
) -> Result<PackageData, String> {
    // Ruby YAML tagged types need to be handled:
    // --- !ruby/object:Gem::Specification
    // We strip Ruby-specific YAML tags since yaml_serde can't handle them
    let cleaned = clean_ruby_yaml_tags(content);

    let yaml: yaml_serde::Value =
        yaml_serde::from_str(&cleaned).map_err(|e| format!("Failed to parse YAML: {}", e))?;

    let name = yaml_string(&yaml, "name").map(truncate_field);
    let version = yaml.get("version").and_then(|v| {
        if v.is_string() {
            v.as_str().map(|s| truncate_field(s.to_string()))
        } else {
            yaml_string(v, "version").map(truncate_field)
        }
    });
    let description = yaml_string(&yaml, "description")
        .or_else(|| yaml_string(&yaml, "summary"))
        .map(truncate_field);
    let homepage = yaml_string(&yaml, "homepage").map(truncate_field);
    let summary = yaml_string(&yaml, "summary").map(truncate_field);

    // Licenses
    let licenses: Vec<String> = yaml
        .get("licenses")
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|item| item.as_str().map(|s| truncate_field(s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    // Extract license statement only - detection happens in separate engine
    let extracted_license_statement = if !licenses.is_empty() {
        Some(licenses.join(" AND "))
    } else {
        None
    };

    let (license_expression, license_expression_spdx, license_detections) =
        normalize_spdx_declared_license(extracted_license_statement.as_deref());

    // Authors
    let authors: Vec<String> = yaml
        .get("authors")
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|item| item.as_str().map(|s| truncate_field(s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    let emails: Vec<String> = yaml
        .get("email")
        .map(|v| {
            if let Some(seq) = v.as_sequence() {
                seq.iter()
                    .filter_map(|item| item.as_str().map(|s| truncate_field(s.to_string())))
                    .collect()
            } else if let Some(s) = v.as_str() {
                vec![truncate_field(s.to_string())]
            } else {
                Vec::new()
            }
        })
        .unwrap_or_default();

    // Build parties
    let mut parties: Vec<Party> = Vec::new();
    let max_len = authors.len().max(emails.len());
    for i in 0..max_len {
        let author_name = authors.get(i).map(|s| s.as_str());
        let email_str = emails.get(i).map(|s| s.as_str());

        let (parsed_email_name, parsed_email) = match email_str {
            Some(e) if e.contains('<') => split_name_email(e),
            None => (None, None),
            _ => (None, None),
        };

        let party_name = author_name.map(|s| s.to_string()).or(parsed_email_name);

        parties.push(Party {
            r#type: Some("person".to_string()),
            role: Some("author".to_string()),
            name: party_name,
            email: parsed_email.or_else(|| {
                email_str
                    .filter(|e| e.contains('@') && !e.contains('<'))
                    .map(|e| e.to_string())
            }),
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }

    // Dependencies
    let dependencies = parse_gem_yaml_dependencies(&yaml);

    let metadata = yaml.get("metadata");

    let bug_tracking_url = metadata
        .and_then(|m| yaml_string(m, "bug_tracking_uri"))
        .map(truncate_field);

    let code_view_url = metadata
        .and_then(|m| yaml_string(m, "source_code_uri"))
        .map(truncate_field);

    let vcs_url = code_view_url.clone().or_else(|| {
        metadata
            .and_then(|m| yaml_string(m, "homepage_uri"))
            .map(truncate_field)
    });

    let file_references = metadata
        .and_then(|m| m.get("files"))
        .and_then(|f| f.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str())
                .map(|s| crate::models::FileReference {
                    path: s.to_string(),
                    size: None,
                    sha1: None,
                    md5: None,
                    sha256: None,
                    sha512: None,
                    extra_data: None,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let release_date = yaml_string(&yaml, "date").and_then(|d| {
        if d.len() >= 10 {
            Some(d[..10].to_string())
        } else {
            None
        }
    });

    let purl = name
        .as_deref()
        .map(|n| create_gem_purl(n, version.as_deref()))
        .unwrap_or(None);

    let platform = yaml_string(&yaml, "platform").map(truncate_field);
    let (repository_homepage_url, repository_download_url, api_data_url, download_url) =
        if let Some(n) = name.as_deref() {
            get_rubygems_urls(n, version.as_deref(), platform.as_deref())
        } else {
            (None, None, None, None)
        };

    let qualifiers = if let Some(ref p) = platform {
        if p != "ruby" {
            let mut q = HashMap::new();
            q.insert("platform".to_string(), p.clone());
            Some(q)
        } else {
            None
        }
    } else {
        None
    };

    Ok(PackageData {
        package_type: Some(PACKAGE_TYPE),
        name,
        version,
        qualifiers,
        primary_language: Some("Ruby".to_string()),
        description: description.or(summary),
        release_date,
        homepage_url: homepage,
        download_url,
        bug_tracking_url,
        code_view_url,
        declared_license_expression: license_expression,
        declared_license_expression_spdx: license_expression_spdx,
        license_detections,
        extracted_license_statement,
        file_references,
        parties,
        dependencies,
        repository_homepage_url,
        repository_download_url,
        api_data_url,
        datasource_id: Some(datasource_id),
        purl,
        vcs_url,
        ..default_package_data()
    })
}

/// Strips Ruby-specific YAML tags that yaml_serde cannot handle.
fn clean_ruby_yaml_tags(content: &str) -> String {
    let tag_re = match Regex::new(r"!ruby/\S+") {
        Ok(r) => r,
        Err(_) => return content.to_string(),
    };
    tag_re.replace_all(content, "").to_string()
}

fn yaml_string(yaml: &yaml_serde::Value, key: &str) -> Option<String> {
    yaml.get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn parse_gem_yaml_dependencies(yaml: &yaml_serde::Value) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    let deps_seq = match yaml.get("dependencies").and_then(|v| v.as_sequence()) {
        Some(seq) => seq,
        None => return dependencies,
    };

    for dep_value in deps_seq.iter().take(MAX_ITERATION_COUNT) {
        let dep_name = match yaml_string(dep_value, "name").map(truncate_field) {
            Some(n) => n,
            None => continue,
        };

        let dep_type = yaml_string(dep_value, "type");
        let is_development = dep_type.as_deref() == Some(":development");

        // Extract version requirements from the nested structure
        let requirements = dep_value
            .get("requirement")
            .or_else(|| dep_value.get("version_requirements"))
            .and_then(|req| req.get("requirements"))
            .and_then(|reqs| reqs.as_sequence());

        let extracted_requirement = requirements.map(|reqs| {
            let parts: Vec<String> = reqs
                .iter()
                .filter_map(|req| {
                    let seq = req.as_sequence()?;
                    if seq.len() >= 2 {
                        let op = seq[0].as_str().unwrap_or("");
                        let ver = seq[1].get("version").and_then(|v| v.as_str()).unwrap_or("");
                        if op == ">=" && ver == "0" {
                            // ">= 0" means "any version" - skip
                            None
                        } else if op.is_empty() || ver.is_empty() {
                            None
                        } else {
                            Some(format!("{} {}", op, ver))
                        }
                    } else {
                        None
                    }
                })
                .collect();
            parts.join(", ")
        });

        let extracted_requirement = extracted_requirement
            .filter(|s| !s.is_empty())
            .or_else(|| Some(String::new()));

        let (scope, is_runtime, is_optional) = if is_development {
            (Some("development".to_string()), false, true)
        } else {
            (Some("runtime".to_string()), true, false)
        };

        let purl = create_gem_purl(&dep_name, None);

        dependencies.push(Dependency {
            purl,
            extracted_requirement,
            scope,
            is_runtime: Some(is_runtime),
            is_optional: Some(is_optional),
            is_pinned: None,
            is_direct: Some(true),
            resolved_package: None,
            extra_data: None,
        });
    }

    dependencies
}

// =============================================================================
// Gem Metadata Extracted Parser (metadata.gz-extract files)
// =============================================================================

pub struct GemMetadataExtractedParser;

impl PackageParser for GemMetadataExtractedParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        vec![match extract_gem_metadata_extracted(path) {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to extract gem metadata from {:?}: {}", path, e);
                default_package_data_with_datasource(DatasourceId::GemArchiveExtracted)
            }
        }]
    }

    fn is_match(path: &Path) -> bool {
        path.to_str()
            .is_some_and(|p| p.contains("metadata.gz-extract"))
    }
}

fn extract_gem_metadata_extracted(path: &Path) -> Result<PackageData, String> {
    let content = read_file_to_string(path, None)
        .map_err(|e| format!("Failed to read metadata.gz-extract file: {}", e))?;

    parse_gem_metadata_yaml(&content, DatasourceId::GemArchiveExtracted)
}

// Register parser with metadata
crate::register_parser!(
    "Ruby Gemfile manifest",
    &["**/Gemfile", "**/data.gz-extract/Gemfile"],
    "gem",
    "Ruby",
    Some("https://bundler.io/man/gemfile.5.html"),
);

crate::register_parser!(
    "Ruby Gemfile.lock lockfile",
    &["**/Gemfile.lock", "**/data.gz-extract/Gemfile.lock"],
    "gem",
    "Ruby",
    Some("https://bundler.io/man/gemfile.5.html"),
);

crate::register_parser!(
    "Ruby .gemspec manifest",
    &[
        "**/*.gemspec",
        "**/data.gz-extract/*.gemspec",
        "**/specifications/*.gemspec"
    ],
    "gem",
    "Ruby",
    Some("https://guides.rubygems.org/specification-reference/"),
);

crate::register_parser!(
    "Ruby .gem archive",
    &["**/*.gem"],
    "gem",
    "Ruby",
    Some("https://guides.rubygems.org/specification-reference/"),
);

crate::register_parser!(
    "Ruby gem metadata (extracted)",
    &["**/metadata.gz-extract"],
    "gem",
    "Ruby",
    Some("https://guides.rubygems.org/specification-reference/"),
);

#[cfg(test)]
mod tests {
    use super::parse_gemspec;

    #[test]
    fn test_clean_gemspec_value_handles_unterminated_percent_q() {
        assert_eq!(
            super::clean_gemspec_value("%q{Arel is a SQL AST manager for Ruby. It"),
            "Arel is a SQL AST manager for Ruby. It"
        );
    }

    #[test]
    fn test_parse_gemspec_runtime_dependency_scope() {
        let content = r#"
Gem::Specification.new do |spec|
  spec.name = "demo"
  spec.version = "1.0.0"
  spec.add_runtime_dependency "rack", "~> 3.0"
  spec.add_dependency "thor", ">= 1.0"
end
"#;

        let package_data = parse_gemspec(content);
        assert_eq!(package_data.dependencies.len(), 2);
        assert_eq!(
            package_data.dependencies[0].scope,
            Some("runtime".to_string())
        );
        assert_eq!(
            package_data.dependencies[0].extracted_requirement,
            Some("~> 3.0".to_string())
        );
        assert_eq!(
            package_data.dependencies[1].scope,
            Some("runtime".to_string())
        );
        assert_eq!(
            package_data.dependencies[1].extracted_requirement,
            Some(">= 1.0".to_string())
        );
    }
}
