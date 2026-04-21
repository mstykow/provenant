// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Parser for Go ecosystem dependency files.
//!
//! Extracts package metadata and dependencies from Go module management files
//! and legacy dependency tracking formats.
//!
//! # Supported Formats
//! - go.mod (Go module manifest with dependencies and version constraints)
//! - go.sum (Go module checksum database for verification)
//! - Godeps.json (Legacy dependency format from godep tool)
//!
//! # Key Features
//! - go.mod dependency extraction with version constraint parsing
//! - Direct vs transitive dependency tracking from require/indirect fields
//! - Checksum extraction from go.sum for integrity verification
//! - Legacy Godeps.json support for older projects
//! - Package URL (purl) generation for golang packages
//! - Module path parsing and namespace detection
//!
//! # Implementation Notes
//! - PURL type: "golang"
//! - All dependencies are pinned in go.mod/go.sum (`is_pinned: Some(true)`)
//! - Graceful error handling with `warn!()` logs
//! - Supports Go 1.11+ module syntax

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};
use crate::parser_warn as warn;
use crate::parsers::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};
use packageurl::PackageUrl;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::PackageParser;

const PACKAGE_TYPE: PackageType = PackageType::Golang;

/// Go go.mod manifest parser.
///
/// Extracts module declaration, require dependencies (with indirect marker
/// preservation), and exclude directives from go.mod files.
pub struct GoModParser;

impl PackageParser for GoModParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read go.mod at {:?}: {}", path, e);
                return vec![default_go_mod_package_data()];
            }
        };

        vec![parse_go_mod(&content)]
    }

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "go.mod")
    }
}

#[derive(Debug, Clone, PartialEq)]
enum BlockState {
    None,
    Require,
    Exclude,
    Replace,
    Retract,
}

pub fn parse_go_mod(content: &str) -> PackageData {
    let mut namespace: Option<String> = None;
    let mut name: Option<String> = None;
    let mut go_version: Option<String> = None;
    let mut toolchain: Option<String> = None;
    let mut require_deps: Vec<Dependency> = Vec::new();
    let mut exclude_deps: Vec<Dependency> = Vec::new();
    let mut replace_deps: Vec<Dependency> = Vec::new();
    let mut retracted_versions: Vec<String> = Vec::new();
    let mut block_state = BlockState::None;

    for line in content.lines().take(MAX_ITERATION_COUNT) {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }

        // Bug #5: Reset block state on closing paren
        if trimmed == ")" {
            block_state = BlockState::None;
            continue;
        }

        // Inside a block: dispatch by block type
        if block_state != BlockState::None {
            match block_state {
                BlockState::Require => {
                    if let Some(dep) = parse_dependency_line(trimmed, "require") {
                        require_deps.push(dep);
                    }
                }
                BlockState::Exclude => {
                    if let Some(dep) = parse_dependency_line(trimmed, "exclude") {
                        exclude_deps.push(dep);
                    }
                }
                BlockState::Replace => {
                    if let Some(dep) = parse_replace_line(trimmed) {
                        replace_deps.push(dep);
                    }
                }
                BlockState::Retract => {
                    retracted_versions.extend(parse_retract_value(trimmed));
                }
                BlockState::None => {}
            }
            continue;
        }

        // Block openings
        if trimmed.starts_with("require") && trimmed.contains('(') {
            block_state = BlockState::Require;
            continue;
        }
        if trimmed.starts_with("exclude") && trimmed.contains('(') {
            block_state = BlockState::Exclude;
            continue;
        }
        if trimmed.starts_with("replace") && trimmed.contains('(') {
            block_state = BlockState::Replace;
            continue;
        }
        if trimmed.starts_with("retract") && trimmed.contains('(') {
            block_state = BlockState::Retract;
            continue;
        }

        // Module declaration
        if let Some(module_path) = trimmed.strip_prefix("module ") {
            let module_path = strip_comment(module_path).trim();
            if !module_path.is_empty() {
                let (ns, n) = split_module_path(module_path);
                namespace = ns.map(truncate_field);
                name = Some(truncate_field(n));
            }
            continue;
        }

        // Go version directive
        if let Some(version) = trimmed.strip_prefix("go ") {
            let version = strip_comment(version).trim();
            if !version.is_empty() {
                go_version = Some(truncate_field(version.to_string()));
            }
            continue;
        }

        // Toolchain directive
        if let Some(tc) = trimmed.strip_prefix("toolchain ") {
            let tc = strip_comment(tc).trim();
            if !tc.is_empty() {
                toolchain = Some(truncate_field(tc.to_string()));
            }
            continue;
        }

        // Single-line require
        if let Some(rest) = trimmed.strip_prefix("require ") {
            if let Some(dep) = parse_dependency_line(rest, "require") {
                require_deps.push(dep);
            }
            continue;
        }

        // Single-line exclude
        if let Some(rest) = trimmed.strip_prefix("exclude ") {
            if let Some(dep) = parse_dependency_line(rest, "exclude") {
                exclude_deps.push(dep);
            }
            continue;
        }

        // Single-line replace (without opening paren)
        if let Some(rest) = trimmed.strip_prefix("replace ") {
            let rest = strip_comment(rest).trim();
            if !rest.contains('(')
                && let Some(dep) = parse_replace_line(rest)
            {
                replace_deps.push(dep);
            }
            continue;
        }

        // Single-line retract
        if let Some(rest) = trimmed.strip_prefix("retract ") {
            let rest = strip_comment(rest).trim();
            if !rest.contains('(') {
                retracted_versions.extend(parse_retract_value(rest));
            }
            continue;
        }
    }

    let full_module = match (&namespace, &name) {
        (Some(ns), Some(n)) => Some(format!("{}/{}", ns, n)),
        (None, Some(n)) => Some(n.clone()),
        _ => None,
    };

    let homepage_url = full_module
        .as_ref()
        .map(|m| truncate_field(format!("https://pkg.go.dev/{}", m)));

    let vcs_url = full_module
        .as_ref()
        .map(|m| truncate_field(format!("https://{}.git", m)));

    let repository_homepage_url = homepage_url.clone();

    let purl = full_module
        .as_ref()
        .and_then(|m| create_golang_purl(m, None));

    let mut dependencies =
        Vec::with_capacity(require_deps.len() + exclude_deps.len() + replace_deps.len());
    dependencies.append(&mut require_deps);
    dependencies.append(&mut exclude_deps);
    dependencies.append(&mut replace_deps);

    let mut extra_data_map = std::collections::HashMap::new();
    if let Some(v) = go_version {
        extra_data_map.insert("go_version".to_string(), serde_json::Value::String(v));
    }
    if let Some(tc) = toolchain {
        extra_data_map.insert("toolchain".to_string(), serde_json::Value::String(tc));
    }
    if !retracted_versions.is_empty() {
        extra_data_map.insert(
            "retracted_versions".to_string(),
            serde_json::json!(retracted_versions),
        );
    }
    let extra_data = if extra_data_map.is_empty() {
        None
    } else {
        Some(extra_data_map)
    };

    PackageData {
        package_type: Some(PACKAGE_TYPE),
        namespace,
        name,
        version: None,
        qualifiers: None,
        subpath: None,
        primary_language: Some("Go".to_string()),
        description: None,
        release_date: None,
        parties: Vec::new(),
        keywords: Vec::new(),
        homepage_url,
        download_url: None,
        size: None,
        sha1: None,
        md5: None,
        sha256: None,
        sha512: None,
        bug_tracking_url: None,
        code_view_url: None,
        vcs_url,
        copyright: None,
        holder: None,
        declared_license_expression: None,
        declared_license_expression_spdx: None,
        license_detections: Vec::new(),
        other_license_expression: None,
        other_license_expression_spdx: None,
        other_license_detections: Vec::new(),
        extracted_license_statement: None,
        notice_text: None,
        source_packages: Vec::new(),
        file_references: Vec::new(),
        is_private: false,
        is_virtual: false,
        extra_data,
        dependencies,
        repository_homepage_url,
        repository_download_url: None,
        api_data_url: None,
        datasource_id: Some(DatasourceId::GoMod),
        purl,
    }
}

/// Parses a single dependency line from a require or exclude block/directive.
///
/// Handles:
/// - Bug #2: Preserves `// indirect` marker as `is_direct = false`
/// - Bug #8: `+incompatible` suffix in versions
/// - Bug #10: Pseudo-versions (v0.0.0-YYYYMMDDHHMMSS-hash)
///
/// Format: `github.com/foo/bar v1.2.3 // indirect`
fn parse_dependency_line(line: &str, scope: &str) -> Option<Dependency> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("//") {
        return None;
    }

    // Bug #2: Check for // indirect BEFORE stripping comments
    let is_indirect = trimmed.contains("// indirect");
    let is_direct = !is_indirect;

    // Strip comment for parsing the module path and version
    let without_comment = strip_comment(trimmed);
    let without_comment = without_comment.trim();

    // Split into module path and version
    let parts: Vec<&str> = without_comment.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    let module_path = parts[0];
    let version = truncate_field(parts[1].to_string());

    let purl = create_golang_purl(module_path, Some(&version));

    Some(Dependency {
        purl,
        extracted_requirement: Some(version),
        scope: Some(scope.to_string()),
        is_runtime: Some(true),
        is_optional: Some(false),
        is_pinned: Some(false),
        is_direct: Some(is_direct),
        resolved_package: None,
        extra_data: None,
    })
}

/// Parses a replace line: `old-module [version] => new-module [version]`
///
/// Returns a `Dependency` with scope "replace" and extra_data containing
/// replace_old, replace_new, replace_version, and optionally replace_old_version.
fn parse_replace_line(line: &str) -> Option<Dependency> {
    let line = strip_comment(line).trim();

    let parts: Vec<&str> = line.splitn(2, "=>").collect();
    if parts.len() != 2 {
        return None;
    }

    let old_parts: Vec<&str> = parts[0].split_whitespace().collect();
    let new_parts: Vec<&str> = parts[1].split_whitespace().collect();

    if old_parts.is_empty() || new_parts.is_empty() {
        return None;
    }

    let old_module = old_parts[0];
    let old_version = old_parts.get(1).copied();
    let new_module = new_parts[0];
    let new_version = new_parts.get(1).map(|s| truncate_field(s.to_string()));

    let purl = create_golang_purl(new_module, new_version.as_deref());

    let mut extra = std::collections::HashMap::new();
    extra.insert(
        "replace_old".to_string(),
        serde_json::Value::String(truncate_field(old_module.to_string())),
    );
    extra.insert(
        "replace_new".to_string(),
        serde_json::Value::String(truncate_field(new_module.to_string())),
    );
    if let Some(ref v) = new_version {
        extra.insert(
            "replace_version".to_string(),
            serde_json::Value::String(v.clone()),
        );
    }
    if let Some(ov) = old_version {
        extra.insert(
            "replace_old_version".to_string(),
            serde_json::Value::String(truncate_field(ov.to_string())),
        );
    }

    Some(Dependency {
        purl,
        extracted_requirement: new_version,
        scope: Some("replace".to_string()),
        is_runtime: Some(true),
        is_optional: Some(false),
        is_pinned: Some(false),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: Some(extra),
    })
}

/// Parses a retract value which can be a single version or a range `[v1, v2]`.
fn parse_retract_value(value: &str) -> Vec<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = &trimmed[1..trimmed.len() - 1];
        inner
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        vec![trimmed.to_string()]
    }
}

pub(crate) fn split_module_path(path: &str) -> (Option<String>, String) {
    match path.rfind('/') {
        Some(idx) => {
            let namespace = &path[..idx];
            let name = &path[idx + 1..];
            (
                Some(truncate_field(namespace.to_string())),
                truncate_field(name.to_string()),
            )
        }
        None => (None, truncate_field(path.to_string())),
    }
}

/// Strips inline comments (everything after `//`) from a line.
///
/// Preserves the content before the comment marker.
fn strip_comment(line: &str) -> &str {
    match line.find("//") {
        Some(idx) => &line[..idx],
        None => line,
    }
}

/// Creates a PURL for a Go module.
///
/// Format: `pkg:golang/namespace/name@version`
/// The module path is split into namespace and name for PURL construction.
pub(crate) fn create_golang_purl(module_path: &str, version: Option<&str>) -> Option<String> {
    let (namespace, name) = split_module_path(module_path);

    let mut purl = match PackageUrl::new(PACKAGE_TYPE.as_str(), &name) {
        Ok(p) => p,
        Err(e) => {
            warn!(
                "Failed to create PURL for golang module '{}': {}",
                module_path, e
            );
            return None;
        }
    };

    if let Some(ns) = &namespace
        && let Err(e) = purl.with_namespace(ns)
    {
        warn!(
            "Failed to set namespace '{}' for golang module '{}': {}",
            ns, module_path, e
        );
        return None;
    }

    if let Some(v) = version
        && let Err(e) = purl.with_version(v)
    {
        warn!(
            "Failed to set version '{}' for golang module '{}': {}",
            v, module_path, e
        );
        return None;
    }

    Some(purl.to_string())
}

/// Returns a default empty PackageData for Go modules.
fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(PACKAGE_TYPE),
        primary_language: Some("Go".to_string()),
        ..Default::default()
    }
}

fn default_go_mod_package_data() -> PackageData {
    PackageData {
        datasource_id: Some(DatasourceId::GoMod),
        ..default_package_data()
    }
}

fn default_go_sum_package_data() -> PackageData {
    PackageData {
        datasource_id: Some(DatasourceId::GoSum),
        ..default_package_data()
    }
}

fn default_go_work_package_data() -> PackageData {
    PackageData {
        datasource_id: Some(DatasourceId::GoWork),
        ..default_package_data()
    }
}

fn default_godeps_package_data() -> PackageData {
    PackageData {
        datasource_id: Some(DatasourceId::Godeps),
        ..default_package_data()
    }
}

crate::register_parser!(
    "Go go.mod module manifest",
    &["**/go.mod"],
    "golang",
    "Go",
    Some("https://go.dev/ref/mod#go-mod-file"),
);

// ============================================================================
// GoSumParser
// ============================================================================

pub struct GoSumParser;

impl PackageParser for GoSumParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read go.sum at {:?}: {}", path, e);
                return vec![default_go_sum_package_data()];
            }
        };

        vec![parse_go_sum(&content)]
    }

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "go.sum")
    }
}

pub fn parse_go_sum(content: &str) -> PackageData {
    let mut dependencies = Vec::new();
    let mut seen = HashSet::new();

    for line in content.lines().take(MAX_ITERATION_COUNT) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 3 || !parts[2].starts_with("h1:") {
            continue;
        }

        let module = parts[0];
        let raw_version = parts[1];

        let version = raw_version.strip_suffix("/go.mod").unwrap_or(raw_version);

        let key = format!("{}@{}", module, version);
        if seen.contains(&key) {
            continue;
        }
        seen.insert(key);

        let purl = create_golang_purl(module, Some(version));

        dependencies.push(Dependency {
            purl,
            extracted_requirement: Some(truncate_field(version.to_string())),
            scope: Some("dependency".to_string()),
            is_runtime: Some(true),
            is_optional: Some(false),
            is_pinned: Some(true),
            is_direct: None,
            resolved_package: None,
            extra_data: None,
        });
    }

    PackageData {
        package_type: Some(PACKAGE_TYPE),
        namespace: None,
        name: None,
        version: None,
        qualifiers: None,
        subpath: None,
        primary_language: Some("Go".to_string()),
        description: None,
        release_date: None,
        parties: Vec::new(),
        keywords: Vec::new(),
        homepage_url: None,
        download_url: None,
        size: None,
        sha1: None,
        md5: None,
        sha256: None,
        sha512: None,
        bug_tracking_url: None,
        code_view_url: None,
        vcs_url: None,
        copyright: None,
        holder: None,
        declared_license_expression: None,
        declared_license_expression_spdx: None,
        license_detections: Vec::new(),
        other_license_expression: None,
        other_license_expression_spdx: None,
        other_license_detections: Vec::new(),
        extracted_license_statement: None,
        notice_text: None,
        source_packages: Vec::new(),
        file_references: Vec::new(),
        is_private: false,
        is_virtual: false,
        extra_data: None,
        dependencies,
        repository_homepage_url: None,
        repository_download_url: None,
        api_data_url: None,
        datasource_id: Some(DatasourceId::GoSum),
        purl: None,
    }
}

crate::register_parser!(
    "Go go.sum checksum database",
    &["**/go.sum"],
    "golang",
    "Go",
    Some("https://go.dev/ref/mod#go-sum-files"),
);

pub struct GoWorkParser;

impl PackageParser for GoWorkParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read go.work at {:?}: {}", path, e);
                return vec![default_go_work_package_data()];
            }
        };

        vec![parse_go_work(&content, path)]
    }

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "go.work")
    }
}

pub fn parse_go_work(content: &str, work_path: &Path) -> PackageData {
    let mut go_version: Option<String> = None;
    let mut toolchain: Option<String> = None;
    let mut use_paths: Vec<String> = Vec::new();
    let mut replace_deps: Vec<Dependency> = Vec::new();
    let mut unresolved_use_paths: Vec<String> = Vec::new();
    let mut block_state = BlockState::None;

    for line in content.lines().take(MAX_ITERATION_COUNT) {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }

        if trimmed == ")" {
            block_state = BlockState::None;
            continue;
        }

        if block_state != BlockState::None {
            match block_state {
                BlockState::Require => {
                    let use_path = extract_single_go_token(trimmed);
                    if let Some(use_path) = use_path.filter(|path| !path.is_empty()) {
                        use_paths.push(truncate_field(use_path));
                    }
                }
                BlockState::Replace => {
                    if let Some(dep) = parse_workspace_replace_line(trimmed) {
                        replace_deps.push(dep);
                    }
                }
                _ => {}
            }
            continue;
        }

        if trimmed.starts_with("use") && trimmed.contains('(') {
            block_state = BlockState::Require;
            continue;
        }
        if trimmed.starts_with("replace") && trimmed.contains('(') {
            block_state = BlockState::Replace;
            continue;
        }

        if let Some(version) = trimmed.strip_prefix("go ") {
            let version = strip_comment(version).trim();
            if !version.is_empty() {
                go_version = Some(truncate_field(version.to_string()));
            }
            continue;
        }

        if let Some(tc) = trimmed.strip_prefix("toolchain ") {
            let tc = strip_comment(tc).trim();
            if !tc.is_empty() {
                toolchain = Some(truncate_field(tc.to_string()));
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("use ") {
            let use_path = extract_single_go_token(rest);
            if let Some(use_path) = use_path.filter(|path| !path.is_empty()) {
                use_paths.push(truncate_field(use_path));
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("replace ") {
            if let Some(dep) = parse_workspace_replace_line(rest) {
                replace_deps.push(dep);
            }
            continue;
        }
    }

    if go_version.is_none() || use_paths.is_empty() {
        warn!("Invalid go.work: missing go directive or use directive");
        return default_go_work_package_data();
    }

    let (mut dependencies, unresolved) = resolve_workspace_use_dependencies(work_path, &use_paths);
    dependencies.extend(replace_deps);
    unresolved_use_paths.extend(unresolved);

    let mut extra_data = HashMap::new();
    if let Some(v) = go_version {
        extra_data.insert("go_version".to_string(), serde_json::Value::String(v));
    }
    if let Some(tc) = toolchain {
        extra_data.insert("toolchain".to_string(), serde_json::Value::String(tc));
    }
    extra_data.insert(
        "use_paths".to_string(),
        serde_json::Value::Array(
            use_paths
                .iter()
                .cloned()
                .map(serde_json::Value::String)
                .collect(),
        ),
    );
    if !unresolved_use_paths.is_empty() {
        extra_data.insert(
            "unresolved_use_paths".to_string(),
            serde_json::Value::Array(
                unresolved_use_paths
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
    }

    PackageData {
        package_type: Some(PACKAGE_TYPE),
        namespace: None,
        name: None,
        version: None,
        qualifiers: None,
        subpath: None,
        primary_language: Some("Go".to_string()),
        description: None,
        release_date: None,
        parties: Vec::new(),
        keywords: Vec::new(),
        homepage_url: None,
        download_url: None,
        size: None,
        sha1: None,
        md5: None,
        sha256: None,
        sha512: None,
        bug_tracking_url: None,
        code_view_url: None,
        vcs_url: None,
        copyright: None,
        holder: None,
        declared_license_expression: None,
        declared_license_expression_spdx: None,
        license_detections: Vec::new(),
        other_license_expression: None,
        other_license_expression_spdx: None,
        other_license_detections: Vec::new(),
        extracted_license_statement: None,
        notice_text: None,
        source_packages: Vec::new(),
        file_references: Vec::new(),
        is_private: false,
        is_virtual: false,
        extra_data: Some(extra_data),
        dependencies,
        repository_homepage_url: None,
        repository_download_url: None,
        api_data_url: None,
        datasource_id: Some(DatasourceId::GoWork),
        purl: None,
    }
}

fn resolve_workspace_use_dependencies(
    work_path: &Path,
    use_paths: &[String],
) -> (Vec<Dependency>, Vec<String>) {
    let Some(base_dir) = work_path.parent() else {
        return (Vec::new(), use_paths.to_vec());
    };

    let mut dependencies = Vec::new();
    let mut unresolved = Vec::new();

    for use_path in use_paths.iter().take(MAX_ITERATION_COUNT) {
        let go_mod_path = base_dir.join(use_path).join("go.mod");
        let module_path = read_file_to_string(&go_mod_path, None)
            .ok()
            .and_then(|content| extract_module_path_from_go_mod(&content));

        let purl = module_path
            .as_deref()
            .and_then(|module_path| create_golang_purl(module_path, None));

        if purl.is_none() {
            unresolved.push(use_path.clone());
            continue;
        }

        let mut extra_data = HashMap::new();
        extra_data.insert(
            "workspace_path".to_string(),
            serde_json::Value::String(truncate_field(use_path.clone())),
        );
        if let Some(module_path) = module_path {
            extra_data.insert(
                "workspace_module_path".to_string(),
                serde_json::Value::String(truncate_field(module_path)),
            );
        }

        dependencies.push(Dependency {
            purl,
            extracted_requirement: Some(truncate_field(use_path.clone())),
            scope: Some("use".to_string()),
            is_runtime: Some(true),
            is_optional: Some(false),
            is_pinned: Some(false),
            is_direct: Some(true),
            resolved_package: None,
            extra_data: Some(extra_data),
        });
    }

    (dependencies, unresolved)
}

fn extract_module_path_from_go_mod(content: &str) -> Option<String> {
    for line in content.lines().take(MAX_ITERATION_COUNT) {
        let trimmed = line.trim();
        if let Some(module_path) = trimmed.strip_prefix("module ") {
            let module_path = strip_comment(module_path).trim();
            if !module_path.is_empty() {
                return Some(truncate_field(module_path.to_string()));
            }
        }
    }
    None
}

fn parse_workspace_replace_line(line: &str) -> Option<Dependency> {
    let line = strip_comment(line).trim();
    let parts: Vec<&str> = line.splitn(2, "=>").collect();
    if parts.len() != 2 {
        return None;
    }

    let old_parts = parse_go_tokens(parts[0]);
    let new_parts = parse_go_tokens(parts[1]);
    if old_parts.is_empty() || new_parts.is_empty() {
        return None;
    }

    let old_module = old_parts[0].as_str();
    let old_version = old_parts.get(1).map(|s| s.as_str());
    let new_module = new_parts[0].as_str();
    let new_version = new_parts.get(1).map(|s| truncate_field(s.clone()));
    let is_local_path = new_module.starts_with("./")
        || new_module.starts_with("../")
        || new_module.starts_with('/')
        || new_module.starts_with('~');

    let purl = if is_local_path {
        None
    } else {
        create_golang_purl(new_module, new_version.as_deref())
    };

    let mut extra = std::collections::HashMap::new();
    extra.insert(
        "replace_old".to_string(),
        serde_json::Value::String(truncate_field(old_module.to_string())),
    );
    extra.insert(
        "replace_new".to_string(),
        serde_json::Value::String(truncate_field(new_module.to_string())),
    );
    if let Some(ref v) = new_version {
        extra.insert(
            "replace_version".to_string(),
            serde_json::Value::String(v.clone()),
        );
    }
    if let Some(ov) = old_version {
        extra.insert(
            "replace_old_version".to_string(),
            serde_json::Value::String(truncate_field(ov.to_string())),
        );
    }
    if is_local_path {
        extra.insert(
            "replace_local_path".to_string(),
            serde_json::Value::Bool(true),
        );
    }

    Some(Dependency {
        purl,
        extracted_requirement: new_version,
        scope: Some("replace".to_string()),
        is_runtime: Some(true),
        is_optional: Some(false),
        is_pinned: Some(false),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: Some(extra),
    })
}

fn extract_single_go_token(value: &str) -> Option<String> {
    parse_go_tokens(value).into_iter().next()
}

fn parse_go_tokens(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut chars = value.chars().peekable();

    while let Some(ch) = chars.next() {
        if let Some(active_quote) = quote {
            if ch == active_quote {
                quote = None;
                continue;
            }

            if active_quote == '"' && ch == '\\' {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
                continue;
            }

            current.push(ch);
            continue;
        }

        match ch {
            '"' | '`' => {
                quote = Some(ch);
            }
            c if c.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

crate::register_parser!(
    "Go go.work workspace file",
    &["**/go.work"],
    "golang",
    "Go",
    Some("https://go.dev/ref/mod#go-work-files"),
);

// ============================================================================
// GodepsParser
// ============================================================================

pub struct GodepsParser;

impl PackageParser for GodepsParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read Godeps.json at {:?}: {}", path, e);
                return vec![default_godeps_package_data()];
            }
        };

        vec![parse_godeps_json(&content)]
    }

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "Godeps.json")
    }
}

pub fn parse_godeps_json(content: &str) -> PackageData {
    let json: serde_json::Value = match serde_json::from_str(content) {
        Ok(j) => j,
        Err(e) => {
            warn!("Failed to parse Godeps.json: {}", e);
            return default_godeps_package_data();
        }
    };

    let import_path = json
        .get("ImportPath")
        .and_then(|v| v.as_str())
        .map(|s| truncate_field(s.to_string()));

    let go_version = json
        .get("GoVersion")
        .and_then(|v| v.as_str())
        .map(|s| truncate_field(s.to_string()));

    let (namespace, name) = match &import_path {
        Some(ip) => {
            let (ns, n) = split_module_path(ip);
            (ns, Some(n))
        }
        None => (None, None),
    };

    let purl = import_path
        .as_deref()
        .and_then(|ip| create_golang_purl(ip, None));

    let mut dependencies = Vec::new();

    if let Some(deps) = json.get("Deps").and_then(|v| v.as_array()) {
        for dep in deps.iter().take(MAX_ITERATION_COUNT) {
            let dep_import_path = dep.get("ImportPath").and_then(|v| v.as_str());
            let rev = dep.get("Rev").and_then(|v| v.as_str());

            if let Some(path) = dep_import_path {
                let dep_purl = create_golang_purl(path, None);

                dependencies.push(Dependency {
                    purl: dep_purl,
                    extracted_requirement: rev.map(|s| truncate_field(s.to_string())),
                    scope: Some("Deps".to_string()),
                    is_runtime: Some(true),
                    is_optional: Some(false),
                    is_pinned: Some(false),
                    is_direct: None,
                    resolved_package: None,
                    extra_data: None,
                });
            }
        }
    }

    let extra_data = go_version.map(|v| {
        let mut map = HashMap::new();
        map.insert("go_version".to_string(), serde_json::Value::String(v));
        map
    });

    let homepage_url = import_path
        .as_ref()
        .map(|m| truncate_field(format!("https://pkg.go.dev/{}", m)));

    let vcs_url = import_path
        .as_ref()
        .map(|m| truncate_field(format!("https://{}.git", m)));

    PackageData {
        package_type: Some(PACKAGE_TYPE),
        namespace,
        name,
        version: None,
        qualifiers: None,
        subpath: None,
        primary_language: Some("Go".to_string()),
        description: None,
        release_date: None,
        parties: Vec::new(),
        keywords: Vec::new(),
        homepage_url,
        download_url: None,
        size: None,
        sha1: None,
        md5: None,
        sha256: None,
        sha512: None,
        bug_tracking_url: None,
        code_view_url: None,
        vcs_url,
        copyright: None,
        holder: None,
        declared_license_expression: None,
        declared_license_expression_spdx: None,
        license_detections: Vec::new(),
        other_license_expression: None,
        other_license_expression_spdx: None,
        other_license_detections: Vec::new(),
        extracted_license_statement: None,
        notice_text: None,
        source_packages: Vec::new(),
        file_references: Vec::new(),
        is_private: false,
        is_virtual: false,
        extra_data,
        dependencies,
        repository_homepage_url: None,
        repository_download_url: None,
        api_data_url: None,
        datasource_id: Some(DatasourceId::Godeps),
        purl,
    }
}

crate::register_parser!(
    "Go Godeps.json legacy dependency file",
    &["**/Godeps.json"],
    "golang",
    "Go",
    None,
);
