//! Parser for npm package-lock.json and npm-shrinkwrap.json lockfiles.
//!
//! Extracts resolved dependency information including exact versions, integrity hashes,
//! and dependency trees from npm lockfile formats (v1, v2, v3).
//!
//! # Supported Formats
//! - package-lock.json (lockfile v1, v2, v3)
//! - npm-shrinkwrap.json
//!
//! # Key Features
//! - Lockfile version detection (v1, v2, v3)
//! - Direct vs transitive dependency tracking (`is_direct`)
//! - Integrity hash extraction (sha512, sha256, sha1, md5)
//! - Package URL (purl) generation
//! - Dependency graph traversal with proper nesting
//!
//! # Implementation Notes
//! - v1: Dependencies nested in `dependencies` objects
//! - v2+: Flat dependency structure with `node_modules/` prefix for nesting
//! - Direct dependencies determined by top-level `dependencies` and `devDependencies`

use crate::models::{
    DatasourceId, Dependency, PackageData, PackageType, ResolvedPackage, Sha1Digest, Sha512Digest,
};
use crate::parser_warn as warn;
use crate::parsers::utils::{
    MAX_ITERATION_COUNT, MAX_RECURSION_DEPTH, RecursionGuard, npm_purl, parse_sri,
    read_file_to_string, truncate_field,
};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

use super::PackageParser;

// Field name constants
const FIELD_LOCKFILE_VERSION: &str = "lockfileVersion";
const FIELD_NAME: &str = "name";
const FIELD_VERSION: &str = "version";
const FIELD_DEPENDENCIES: &str = "dependencies";
const FIELD_PACKAGES: &str = "packages";
const FIELD_RESOLVED: &str = "resolved";
const FIELD_INTEGRITY: &str = "integrity";
const FIELD_DEV: &str = "dev";
const FIELD_OPTIONAL: &str = "optional";
const FIELD_DEV_OPTIONAL: &str = "devOptional";
const FIELD_LINK: &str = "link";

/// npm lockfile parser supporting package-lock.json v1, v2, and v3 formats.
///
/// Extracts pinned dependency versions with integrity hashes from lockfiles
/// including npm-shrinkwrap.json variants.
pub struct NpmLockParser;

impl PackageParser for NpmLockParser {
    const PACKAGE_TYPE: PackageType = PackageType::Npm;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| {
                name == "package-lock.json"
                    || name == ".package-lock.json"
                    || name == "npm-shrinkwrap.json"
                    || name == ".npm-shrinkwrap.json"
            })
            .unwrap_or(false)
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read package-lock.json at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        let json: Value = match serde_json::from_str(&content) {
            Ok(json) => json,
            Err(e) => {
                warn!("Failed to parse package-lock.json at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        let lockfile_version = json
            .get(FIELD_LOCKFILE_VERSION)
            .and_then(|v| v.as_i64())
            .unwrap_or(1);

        let root_name = truncate_field(
            json.get(FIELD_NAME)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        );

        let root_version = truncate_field(
            json.get(FIELD_VERSION)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        );

        vec![if lockfile_version == 1 {
            parse_lockfile_v1(&json, root_name, root_version, lockfile_version)
        } else {
            parse_lockfile_v2_plus(&json, root_name, root_version, lockfile_version)
        }]
    }
}

/// Returns a default empty PackageData for error cases
fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(NpmLockParser::PACKAGE_TYPE),
        datasource_id: Some(DatasourceId::NpmPackageLockJson),
        ..Default::default()
    }
}

/// Parse lockfile version 2 or 3 (flat structure with "packages" key)
fn parse_lockfile_v2_plus(
    json: &Value,
    root_name: String,
    root_version: String,
    lockfile_version: i64,
) -> PackageData {
    let packages = match json.get(FIELD_PACKAGES).and_then(|v| v.as_object()) {
        Some(packages) => packages,
        None => {
            warn!("No 'packages' field found in lockfile v2+");
            return default_package_data();
        }
    };

    let (root_name, root_version) = extract_root_package_identity(json, root_name, root_version);
    let (namespace, name, version, purl) =
        normalize_root_package_metadata(&root_name, &root_version);

    // Collect root-level dependencies from top-level sections
    let mut root_deps = std::collections::HashSet::new();

    // Root dependencies are in top-level "dependencies" and "devDependencies"
    if let Some(root_deps_obj) = json.get(FIELD_DEPENDENCIES).and_then(|v| v.as_object()) {
        for key in root_deps_obj.keys().take(MAX_ITERATION_COUNT) {
            root_deps.insert(key.clone());
        }
    }
    if let Some(root_dev_deps_obj) = json.get("devDependencies").and_then(|v| v.as_object()) {
        for key in root_dev_deps_obj.keys().take(MAX_ITERATION_COUNT) {
            root_deps.insert(key.clone());
        }
    }
    if let Some(root_package) = packages.get("").and_then(|value| value.as_object()) {
        collect_root_dependency_names(root_package.get(FIELD_DEPENDENCIES), &mut root_deps);
        collect_root_dependency_names(root_package.get("devDependencies"), &mut root_deps);
        collect_root_dependency_names(root_package.get("optionalDependencies"), &mut root_deps);
    }

    let mut dependencies = Vec::new();

    for (key, value) in packages.iter().take(MAX_ITERATION_COUNT) {
        // Skip the root package (empty string key)
        if key.is_empty() {
            continue;
        }

        // Extract package name from path like "node_modules/@types/node" or "node_modules/express"
        let install_name = extract_package_name_from_path(key);
        if install_name.is_empty() {
            continue;
        }

        let package_name = value
            .get(FIELD_NAME)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| install_name.clone());

        let version = value
            .get(FIELD_VERSION)
            .and_then(|v| v.as_str())
            .map(|v| truncate_field(v.to_string()));

        let is_dev = value
            .get(FIELD_DEV)
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let is_optional = value
            .get(FIELD_OPTIONAL)
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let is_dev_optional = value
            .get(FIELD_DEV_OPTIONAL)
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let resolved = value
            .get(FIELD_RESOLVED)
            .and_then(|v| v.as_str())
            .map(|v| truncate_field(v.to_string()));
        let integrity = value.get(FIELD_INTEGRITY).and_then(|v| v.as_str());
        let from = value.get("from").and_then(|v| v.as_str());
        let in_bundle = value
            .get("inBundle")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let is_link = value
            .get(FIELD_LINK)
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let is_direct = root_deps.contains(&install_name) && is_direct_dependency_path(key);

        let dependency = match version {
            Some(version) => build_npm_dependency(
                &package_name,
                version,
                is_dev,
                is_dev_optional,
                is_optional,
                resolved,
                integrity,
                is_direct,
                from,
                in_bundle,
                Vec::new(),
            ),
            None if is_link => build_link_dependency(
                &package_name,
                is_dev,
                is_dev_optional,
                is_optional,
                resolved,
                is_direct,
            ),
            None => continue,
        };

        dependencies.push(dependency);
    }

    let extra_data = Some(HashMap::from([(
        "lockfileVersion".to_string(),
        Value::from(lockfile_version),
    )]));

    PackageData {
        package_type: Some(NpmLockParser::PACKAGE_TYPE),
        namespace: namespace.clone(),
        name,
        version,
        qualifiers: None,
        subpath: None,
        primary_language: None,
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
        extra_data,
        dependencies,
        repository_homepage_url: None,
        repository_download_url: None,
        api_data_url: None,
        datasource_id: Some(DatasourceId::NpmPackageLockJson),
        purl,
    }
}

/// Parse lockfile version 1 (nested structure with "dependencies" key)
fn parse_lockfile_v1(
    json: &Value,
    root_name: String,
    root_version: String,
    _lockfile_version: i64,
) -> PackageData {
    let dependencies_obj = match json.get(FIELD_DEPENDENCIES).and_then(|v| v.as_object()) {
        Some(deps) => deps,
        None => {
            warn!("No 'dependencies' field found in lockfile v1");
            return default_package_data();
        }
    };

    let (namespace, name, version, purl) =
        normalize_root_package_metadata(&root_name, &root_version);

    let dependencies = parse_dependencies_v1(dependencies_obj);

    PackageData {
        package_type: Some(NpmLockParser::PACKAGE_TYPE),
        namespace: namespace.clone(),
        name,
        version,
        qualifiers: None,
        subpath: None,
        primary_language: None,
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
        datasource_id: Some(DatasourceId::NpmPackageLockJson),
        purl,
    }
}

/// Recursively parse v1 dependencies object
///
/// For v1 lockfiles, root dependencies are at nesting level 0 (direct children of the root
/// "dependencies" object). Transitive dependencies are nested within parent dependencies.
fn parse_dependencies_v1(dependencies_obj: &serde_json::Map<String, Value>) -> Vec<Dependency> {
    let mut guard = RecursionGuard::<()>::depth_only();
    parse_dependencies_v1_with_depth(dependencies_obj, &mut guard)
}

/// Recursively parse v1 dependencies with depth tracking
fn parse_dependencies_v1_with_depth(
    dependencies_obj: &serde_json::Map<String, Value>,
    guard: &mut RecursionGuard<()>,
) -> Vec<Dependency> {
    if guard.descend() {
        warn!(
            "Max recursion depth {} exceeded in v1 dependency parsing",
            MAX_RECURSION_DEPTH
        );
        return Vec::new();
    }

    let mut dependencies = Vec::new();

    for (package_name, dep_data) in dependencies_obj.iter().take(MAX_ITERATION_COUNT) {
        let version = match dep_data.get(FIELD_VERSION).and_then(|v| v.as_str()) {
            Some(v) => truncate_field(v.to_string()),
            None => continue,
        };

        let is_dev = dep_data
            .get(FIELD_DEV)
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let is_optional = dep_data
            .get(FIELD_OPTIONAL)
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let resolved = dep_data
            .get(FIELD_RESOLVED)
            .and_then(|v| v.as_str())
            .map(|v| truncate_field(v.to_string()));
        let integrity = dep_data.get(FIELD_INTEGRITY).and_then(|v| v.as_str());
        let from = dep_data.get("from").and_then(|v| v.as_str());
        let in_bundle = dep_data
            .get("inBundle")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let nested_deps = dep_data
            .get(FIELD_DEPENDENCIES)
            .and_then(|v| v.as_object())
            .map(|nested| parse_dependencies_v1_with_depth(nested, guard))
            .unwrap_or_default();

        let is_direct = guard.depth() == 1;

        let dependency = build_npm_dependency(
            package_name,
            version,
            is_dev,
            false,
            is_optional,
            resolved,
            integrity,
            is_direct,
            from,
            in_bundle,
            nested_deps,
        );

        dependencies.push(dependency);
    }

    guard.ascend();
    dependencies
}

/// Extract namespace and name from a package name like "@types/node" or "express"
/// Returns: (namespace, name) where namespace is empty string "" for non-scoped packages
fn extract_namespace_and_name(package_name: &str) -> (String, String) {
    if package_name.starts_with('@') {
        // Scoped package like "@types/node"
        let parts: Vec<&str> = package_name.splitn(2, '/').collect();
        if parts.len() == 2 {
            (parts[0].to_string(), parts[1].to_string())
        } else {
            // Invalid format, treat as non-scoped
            (String::new(), package_name.to_string())
        }
    } else {
        // Regular package like "express"
        (String::new(), package_name.to_string())
    }
}

/// Extract package name from path like "node_modules/@types/node" or "node_modules/express"
fn extract_package_name_from_path(path: &str) -> String {
    // Find the last occurrence of "node_modules/"
    if let Some(pos) = path.rfind("node_modules/") {
        let after_node_modules = &path[pos + "node_modules/".len()..];

        // Handle scoped packages: "@scope/package"
        if after_node_modules.starts_with('@') {
            // Find the second slash (after @scope/)
            if let Some(slash_pos) = after_node_modules.find('/') {
                let scope_and_package = &after_node_modules[..=slash_pos];
                // Find if there's another segment after the package name
                let remaining = &after_node_modules[slash_pos + 1..];
                if let Some(next_slash) = remaining.find('/') {
                    // Return just @scope/package
                    return format!("{}{}", scope_and_package, &remaining[..next_slash]);
                } else {
                    // Return the full scoped package name
                    return after_node_modules.to_string();
                }
            }
        } else {
            // Regular package: take everything until first slash (or end of string)
            if let Some(slash_pos) = after_node_modules.find('/') {
                return after_node_modules[..slash_pos].to_string();
            } else {
                return after_node_modules.to_string();
            }
        }
    }

    path.to_string()
}

fn create_purl(namespace: &str, name: &str, version: Option<&str>) -> Option<String> {
    let full_name = if namespace.is_empty() {
        name.to_string()
    } else {
        format!("{}/{}", namespace, name)
    };
    npm_purl(&full_name, version.filter(|value| !value.is_empty()))
}

fn normalize_root_package_metadata(
    root_name: &str,
    root_version: &str,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    let (namespace, name) = extract_namespace_and_name(root_name);
    let normalized_name = non_empty_string(&name);
    let normalized_namespace = normalized_name.as_ref().map(|_| namespace);
    let normalized_version = normalized_name
        .as_ref()
        .and_then(|_| non_empty_string(root_version));
    let purl = normalized_name.as_deref().and_then(|name| {
        create_purl(
            normalized_namespace.as_deref().unwrap_or(""),
            name,
            normalized_version.as_deref(),
        )
    });

    (
        normalized_namespace,
        normalized_name,
        normalized_version,
        purl,
    )
}

fn extract_root_package_identity(
    json: &Value,
    root_name: String,
    root_version: String,
) -> (String, String) {
    let root_package = json
        .get(FIELD_PACKAGES)
        .and_then(|value| value.as_object())
        .and_then(|packages| packages.get(""))
        .and_then(|value| value.as_object());

    let name = non_empty_string(&root_name).or_else(|| {
        root_package
            .and_then(|package| package.get(FIELD_NAME))
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .filter(|value| !value.trim().is_empty())
    });
    let version = non_empty_string(&root_version).or_else(|| {
        root_package
            .and_then(|package| package.get(FIELD_VERSION))
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .filter(|value| !value.trim().is_empty())
    });

    (name.unwrap_or_default(), version.unwrap_or_default())
}

fn non_empty_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn collect_root_dependency_names(
    value: Option<&Value>,
    root_deps: &mut std::collections::HashSet<String>,
) {
    if let Some(entries) = value.and_then(|value| value.as_object()) {
        for key in entries.keys().take(MAX_ITERATION_COUNT) {
            root_deps.insert(key.clone());
        }
    }
}

fn is_direct_dependency_path(package_path: &str) -> bool {
    let node_modules_count = package_path.matches("node_modules/").count();

    match node_modules_count {
        0 => true,
        1 => package_path.starts_with("node_modules/") || package_path.starts_with(".pnpm/"),
        _ => false,
    }
}

/// Parse integrity field like "sha512-base64string==" or "sha1-base64string="
/// Returns: (sha1, sha512) as hex strings
fn parse_integrity_field(integrity: Option<&str>) -> (Option<String>, Option<String>) {
    let integrity = match integrity {
        Some(i) => i,
        None => return (None, None),
    };

    match parse_sri(integrity) {
        Some((algo, hex_digest)) => match algo.as_str() {
            "sha1" => (Some(hex_digest), None),
            "sha512" => (None, Some(hex_digest)),
            _ => (None, None),
        },
        None => (None, None),
    }
}

/// Parse resolved URL to extract sha1 checksum if present
/// Example: "https://registry.npmjs.org/package/-/package-1.0.0.tgz#abc123def"
fn parse_resolved_url(url: &str) -> Option<String> {
    // Look for # followed by hex characters
    if let Some(hash_pos) = url.rfind('#') {
        let hash = &url[hash_pos + 1..];
        // Verify it's a hex string (sha1 is 40 characters)
        if hash.len() == 40 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(hash.to_string());
        }
    }
    None
}

/// Determine scope, is_runtime, and is_optional based on dev/optional flags
/// Returns: (scope, is_runtime, is_optional)
fn determine_scope(
    is_dev: bool,
    is_dev_optional: bool,
    is_optional: bool,
) -> (&'static str, bool, bool) {
    if is_dev || is_dev_optional {
        ("devDependencies", false, true)
    } else if is_optional {
        ("dependencies", true, true)
    } else {
        ("dependencies", true, false)
    }
}

fn parse_npm_alias_spec(version_spec: &str) -> Option<(String, String, String)> {
    let aliased_spec = version_spec.strip_prefix("npm:")?;
    let (aliased_name, constraint) = aliased_spec.rsplit_once('@')?;
    let (namespace, name) = extract_namespace_and_name(aliased_name);

    if name.is_empty() || constraint.trim().is_empty() {
        None
    } else {
        Some((namespace, name, constraint.to_string()))
    }
}

fn is_exact_version(version: &str) -> bool {
    let version = version.trim();

    if version.is_empty() {
        return false;
    }

    if version.starts_with('~')
        || version.starts_with('^')
        || version.starts_with('>')
        || version.starts_with('<')
        || version.starts_with('=')
        || version.starts_with('*')
        || version.contains("||")
        || version.contains(" - ")
    {
        return false;
    }

    !is_non_version_dependency(version)
}

fn is_non_version_dependency(version: &str) -> bool {
    let version = version.trim();

    version.starts_with("http://")
        || version.starts_with("https://")
        || version.starts_with("git://")
        || version.starts_with("git+ssh://")
        || version.starts_with("git+http://")
        || version.starts_with("git+https://")
        || version.starts_with("git+file://")
        || version.starts_with("git@")
        || version.starts_with("file:")
        || version.starts_with("link:")
        || version.starts_with("github:")
        || version.starts_with("gitlab:")
        || version.starts_with("bitbucket:")
        || version.starts_with("gist:")
}

fn non_version_download_url(version: &str, resolved: Option<&str>) -> Option<String> {
    resolved
        .map(str::to_string)
        .or_else(|| match version.trim() {
            version if version.starts_with("http://") || version.starts_with("https://") => {
                Some(version.to_string())
            }
            _ => None,
        })
}

#[allow(clippy::too_many_arguments)]
fn build_npm_dependency(
    package_name: &str,
    version: String,
    is_dev: bool,
    is_dev_optional: bool,
    is_optional: bool,
    resolved: Option<String>,
    integrity: Option<&str>,
    is_direct: bool,
    from: Option<&str>,
    in_bundle: bool,
    nested_deps: Vec<Dependency>,
) -> Dependency {
    let (dep_namespace, dep_name) = extract_namespace_and_name(package_name);
    let dep_namespace = truncate_field(dep_namespace);
    let dep_name = truncate_field(dep_name);
    let (scope, is_runtime, is_optional_flag) =
        determine_scope(is_dev, is_dev_optional, is_optional);

    let alias_spec = parse_npm_alias_spec(&version);
    let (purl_namespace, purl_name, resolved_version, is_pinned, dep_purl, download_url) =
        if let Some((alias_namespace, alias_name, alias_constraint)) = alias_spec.clone() {
            let alias_namespace = truncate_field(alias_namespace);
            let alias_name = truncate_field(alias_name);
            let alias_constraint = truncate_field(alias_constraint);
            let is_pinned = is_exact_version(&alias_constraint);
            let dep_purl = create_purl(
                &alias_namespace,
                &alias_name,
                is_pinned.then_some(alias_constraint.as_str()),
            );
            let download_url = non_version_download_url(&alias_constraint, resolved.as_deref());

            (
                alias_namespace,
                alias_name,
                alias_constraint,
                is_pinned,
                dep_purl,
                download_url,
            )
        } else {
            let is_pinned = is_exact_version(&version);
            let dep_purl = create_purl(
                &dep_namespace,
                &dep_name,
                is_pinned.then_some(version.as_str()),
            );
            let download_url = non_version_download_url(&version, resolved.as_deref());

            (
                dep_namespace.clone(),
                dep_name.clone(),
                version.clone(),
                is_pinned,
                dep_purl,
                download_url,
            )
        };

    let (sha1_from_integrity, sha512_from_integrity) = parse_integrity_field(integrity);
    let sha1_from_url = resolved.as_deref().and_then(parse_resolved_url);
    let sha1 = sha1_from_integrity.or(sha1_from_url);

    let mut dep_extra_data = HashMap::new();
    if let Some(from) = from {
        dep_extra_data.insert("from".to_string(), Value::String(from.to_string()));
    }
    if in_bundle {
        dep_extra_data.insert("inBundle".to_string(), Value::Bool(true));
    }

    let resolved_package = ResolvedPackage {
        primary_language: Some("JavaScript".to_string()),
        download_url,
        sha1: sha1.and_then(|h| Sha1Digest::from_hex(&h).ok()),
        sha256: None,
        sha512: sha512_from_integrity.and_then(|h| Sha512Digest::from_hex(&h).ok()),
        md5: None,
        is_virtual: true,
        extra_data: None,
        dependencies: nested_deps,
        repository_homepage_url: None,
        repository_download_url: None,
        api_data_url: None,
        datasource_id: Some(DatasourceId::NpmPackageLockJson),
        purl: None,
        ..ResolvedPackage::new(
            NpmLockParser::PACKAGE_TYPE,
            purl_namespace,
            purl_name,
            resolved_version,
        )
    };

    Dependency {
        purl: dep_purl,
        extracted_requirement: Some(truncate_field(version)),
        scope: Some(scope.to_string()),
        is_runtime: Some(is_runtime),
        is_optional: Some(is_optional_flag),
        is_pinned: Some(is_pinned),
        is_direct: Some(is_direct),
        resolved_package: Some(Box::new(resolved_package)),
        extra_data: (!dep_extra_data.is_empty()).then_some(dep_extra_data),
    }
}

fn build_link_dependency(
    package_name: &str,
    is_dev: bool,
    is_dev_optional: bool,
    is_optional: bool,
    resolved: Option<String>,
    is_direct: bool,
) -> Dependency {
    let (dep_namespace, dep_name) = extract_namespace_and_name(package_name);
    let dep_namespace = truncate_field(dep_namespace);
    let dep_name = truncate_field(dep_name);
    let (scope, is_runtime, is_optional_flag) =
        determine_scope(is_dev, is_dev_optional, is_optional);
    let mut extra_data = HashMap::from([("link".to_string(), Value::Bool(true))]);

    if let Some(resolved) = &resolved {
        extra_data.insert("resolved".to_string(), Value::String(resolved.clone()));
    }

    Dependency {
        purl: create_purl(&dep_namespace, &dep_name, None),
        extracted_requirement: resolved.map(truncate_field),
        scope: Some(scope.to_string()),
        is_runtime: Some(is_runtime),
        is_optional: Some(is_optional_flag),
        is_pinned: Some(false),
        is_direct: Some(is_direct),
        resolved_package: None,
        extra_data: Some(extra_data),
    }
}

crate::register_parser!(
    "npm package-lock.json lockfile",
    &[
        "**/package-lock.json",
        "**/.package-lock.json",
        "**/npm-shrinkwrap.json"
    ],
    "npm",
    "JavaScript",
    Some("https://docs.npmjs.com/cli/v8/configuring-npm/package-lock-json"),
);
