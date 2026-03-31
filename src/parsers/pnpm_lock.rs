//! Parser for pnpm-lock.yaml lockfiles.
//!
//! Extracts resolved dependency information from pnpm lockfiles supporting
//! multiple format versions (v5, v6, v9+).
//!
//! # Supported Formats
//! - pnpm-lock.yaml (v5.x, v6.x, v9.x)
//!
//! # Key Features
//! - Multi-version format support (v5, v6, v9)
//! - Direct dependency detection from `importers` section
//! - Development and optional dependency tracking
//! - Integrity hash extraction (sha512, sha256, md5)
//! - Package URL (purl) generation for scoped packages
//! - Nested dependency resolution
//!
//! # Implementation Notes
//! - v9: Uses `@scope+name@version` format in package keys
//! - v6: Uses `/scope/name/version` format
//! - v5: Similar to v6 but with different dependency structure
//! - Direct dependencies tracked via `importers['.'].dependencies`

use crate::models::{DatasourceId, Dependency, PackageData, PackageType, ResolvedPackage};
use crate::parsers::utils::npm_purl;
use serde_yaml::Value;
use std::fs;
use std::path::Path;

use super::PackageParser;
use super::yarn_lock::extract_namespace_and_name;

/// pnpm lockfile parser supporting v5, v6, and v9 formats.
///
/// Extracts pinned dependency versions from pnpm-lock.yaml and shrinkwrap.yaml files.
pub struct PnpmLockParser;

impl PackageParser for PnpmLockParser {
    const PACKAGE_TYPE: PackageType = PackageType::PnpmLock;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| name == "pnpm-lock.yaml" || name == "shrinkwrap.yaml")
            .unwrap_or(false)
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) => {
                crate::parser_warn!("Failed to read pnpm lockfile at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        let lock_data: Value = match serde_yaml::from_str(&content) {
            Ok(data) => data,
            Err(e) => {
                crate::parser_warn!("Failed to parse pnpm lockfile at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        vec![parse_pnpm_lockfile(&lock_data)]
    }
}

/// Returns a default empty PackageData for error cases
fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(PnpmLockParser::PACKAGE_TYPE),
        extra_data: Some(std::collections::HashMap::new()),
        datasource_id: Some(DatasourceId::PnpmLockYaml),
        ..Default::default()
    }
}

/// Compute which packages are dev-only in pnpm v9 lockfiles
///
/// Strategy:
/// 1. Parse importers section to get direct prod and dev dependencies
/// 2. Build dependency graph from snapshots section
/// 3. Traverse graph from prod roots to find all prod-reachable packages
/// 4. Return packages NOT reachable from prod (= dev-only packages)
fn compute_dev_only_packages_v9(lock_data: &Value) -> std::collections::HashSet<String> {
    use std::collections::{HashMap, HashSet, VecDeque};

    let mut prod_roots = HashSet::new();
    let mut dev_roots = HashSet::new();

    // Step 1: Parse importers section to identify direct dependencies
    if let Some(importers) = lock_data.get("importers").and_then(|v| v.as_mapping()) {
        for (_importer_path, importer_data) in importers {
            // Get production dependencies
            if let Some(deps) = importer_data
                .get("dependencies")
                .and_then(|v| v.as_mapping())
            {
                for (name, version_data) in deps {
                    if let Some(version) = version_data.get("version").and_then(|v| v.as_str()) {
                        let pkg_key = format_package_key_v9(name.as_str().unwrap_or(""), version);
                        prod_roots.insert(pkg_key);
                    }
                }
            }

            // Get dev dependencies
            if let Some(dev_deps) = importer_data
                .get("devDependencies")
                .and_then(|v| v.as_mapping())
            {
                for (name, version_data) in dev_deps {
                    if let Some(version) = version_data.get("version").and_then(|v| v.as_str()) {
                        let pkg_key = format_package_key_v9(name.as_str().unwrap_or(""), version);
                        dev_roots.insert(pkg_key);
                    }
                }
            }
        }
    }

    // Step 2: Build dependency graph from snapshots section
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();

    if let Some(snapshots) = lock_data.get("snapshots").and_then(|v| v.as_mapping()) {
        for (pkg_key, pkg_data) in snapshots {
            let pkg_key_str = pkg_key.as_str().unwrap_or("").to_string();
            let mut children = Vec::new();

            if let Some(deps) = pkg_data.get("dependencies").and_then(|v| v.as_mapping()) {
                for (dep_name, dep_version) in deps {
                    let dep_name_str = dep_name.as_str().unwrap_or("");
                    let dep_version_str = dep_version.as_str().unwrap_or("");
                    let child_key = format!("{}@{}", dep_name_str, dep_version_str);
                    children.push(child_key);
                }
            }

            if let Some(opt_deps) = pkg_data
                .get("optionalDependencies")
                .and_then(|v| v.as_mapping())
            {
                for (dep_name, dep_version) in opt_deps {
                    let dep_name_str = dep_name.as_str().unwrap_or("");
                    let dep_version_str = dep_version.as_str().unwrap_or("");
                    let child_key = format!("{}@{}", dep_name_str, dep_version_str);
                    children.push(child_key);
                }
            }

            graph.insert(pkg_key_str, children);
        }
    }

    // Step 3: BFS from prod roots to find all prod-reachable packages
    let mut prod_reachable = HashSet::new();
    let mut queue = VecDeque::new();

    for root in &prod_roots {
        queue.push_back(root.clone());
        prod_reachable.insert(root.clone());
    }

    while let Some(current) = queue.pop_front() {
        if let Some(children) = graph.get(&current) {
            for child in children {
                if prod_reachable.insert(child.clone()) {
                    queue.push_back(child.clone());
                }
            }
        }
    }

    // Step 4: Dev-only packages = all packages NOT reachable from prod
    let mut dev_only = HashSet::new();
    for pkg_key in graph.keys() {
        if !prod_reachable.contains(pkg_key) {
            dev_only.insert(pkg_key.clone());
        }
    }

    dev_only
}

/// Format package key for v9 (name@version format)
fn format_package_key_v9(name: &str, version: &str) -> String {
    // Handle scoped packages and peer dependencies
    // Version might contain peer dep info like "8.28.2(vue@2.7.16)"
    let clean_version = version.split('(').next().unwrap_or(version);
    format!("{}@{}", name, clean_version)
}

/// Parse pnpm lockfile and extract package data
fn parse_pnpm_lockfile(lock_data: &Value) -> PackageData {
    let lockfile_version = detect_pnpm_version(lock_data);

    let mut result = default_package_data();
    result.package_type = Some(PackageType::PnpmLock);

    // For v9: Build dependency graph to determine dev status
    // For v5/v6: Use dev flag from packages section
    let dev_only_packages = if lockfile_version.starts_with('9') {
        compute_dev_only_packages_v9(lock_data)
    } else {
        std::collections::HashSet::new()
    };

    // Extract packages based on version
    if let Some(packages_map) = lock_data.get("packages").and_then(|v| v.as_mapping()) {
        for (purl_fields, data) in packages_map {
            let purl_fields_str = match purl_fields.as_str() {
                Some(s) => s,
                None => continue,
            };

            // Clean purl_fields based on version
            let clean_purl_fields = clean_purl_fields(purl_fields_str, &lockfile_version);

            // For v9, check if package is in dev-only set
            let is_dev_only_v9 = lockfile_version.starts_with('9')
                && dev_only_packages.contains(&clean_purl_fields.to_string());

            // Extract package info and create dependency
            if let Some(dependency) =
                extract_dependency(&clean_purl_fields, data, &lockfile_version, is_dev_only_v9)
            {
                result.dependencies.push(dependency);
            }
        }
    }

    result
}

/// Detect pnpm lockfile version from the lock data
pub fn detect_pnpm_version(lock_data: &Value) -> String {
    if let Some(version) = lock_data.get("lockfileVersion") {
        if let Some(version_str) = version.as_str() {
            return version_str.to_string();
        }
        if let Some(version_num) = version.as_i64() {
            return version_num.to_string();
        }
        if let Some(version_float) = version.as_f64() {
            return version_float.to_string();
        }
    }

    if let Some(version) = lock_data.get("shrinkwrapVersion") {
        if let Some(version_str) = version.as_str() {
            if let Some(minor_str) = lock_data
                .get("shrinkwrapMinorVersion")
                .and_then(|v| v.as_str())
            {
                return format!("{}.{}", version_str, minor_str);
            }
            return version_str.to_string();
        }
        if let Some(version_num) = version.as_i64() {
            if let Some(minor_num) = lock_data
                .get("shrinkwrapMinorVersion")
                .and_then(|v| v.as_i64())
            {
                return format!("{}.{}", version_num, minor_num);
            }
            return version_num.to_string();
        }
    }

    "5.0".to_string()
}

/// Clean purl_fields based on lockfile version
pub fn clean_purl_fields(purl_fields: &str, lockfile_version: &str) -> String {
    let cleaned = if lockfile_version.starts_with('6') {
        purl_fields
            .split('(')
            .next()
            .unwrap_or(purl_fields)
            .to_string()
    } else if lockfile_version.starts_with('5') {
        // v5 format: /<name>/<version>_<peer_hash> or /@scope/name/version_<peer_hash>
        // _<peer_hash> is optional
        let components: Vec<&str> = purl_fields.split('/').collect();

        if let Some(last_component) = components.last() {
            if last_component.contains('_') {
                // Need to determine where version ends and peer hash begins
                // Strategy: Find the first underscore that comes AFTER a valid semver pattern
                // Semver pattern: digits.digits.digits (possibly with -prerelease or +build)

                // Try to find version pattern: look for pattern like "1.2.3" followed by underscore
                // We'll iterate through possible split points and check if the left part looks like a version
                let parts: Vec<&str> = last_component.split('_').collect();
                for i in 1..=parts.len() {
                    let potential_version = parts[..i].join("_");

                    if is_likely_version(&potential_version) {
                        // Found the version, reconstruct path without peer hash
                        let mut result_components = components[..components.len() - 1].to_vec();
                        result_components.push(&potential_version);
                        return result_components
                            .join("/")
                            .strip_prefix('/')
                            .unwrap_or(&result_components.join("/"))
                            .to_string();
                    }
                }

                // Fallback: if no version pattern found, assume no peer hash (keep everything)
                purl_fields.to_string()
            } else {
                purl_fields.to_string()
            }
        } else {
            purl_fields.to_string()
        }
    } else {
        purl_fields.to_string()
    };

    cleaned.strip_prefix('/').unwrap_or(&cleaned).to_string()
}

/// Check if a string looks like a semantic version
///
/// A version typically:
/// - Contains at least one dot (e.g., "1.0", "1.2.3")
/// - Starts with a digit
/// - May contain hyphens for prerelease (e.g., "1.0.0-alpha")
/// - May contain plus for build metadata (e.g., "1.0.0+build")
fn is_likely_version(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    // Must start with a digit
    if !s
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        return false;
    }

    // Must contain at least one dot (for major.minor or major.minor.patch)
    if !s.contains('.') {
        return false;
    }

    // Check if it matches a basic version pattern
    // Split by '-' or '+' to get the core version part
    let core_version = s.split(&['-', '+'][..]).next().unwrap_or(s);

    // Core version should be digits separated by dots
    let parts: Vec<&str> = core_version.split('.').collect();
    if parts.is_empty() {
        return false;
    }

    // Each part should be numeric (allowing leading zeros)
    for part in parts {
        if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
    }

    true
}

fn parse_nested_dependencies(data: &Value) -> Vec<Dependency> {
    let mut all_dependencies = Vec::new();

    if let Some(deps) = data.get("dependencies").and_then(|v| v.as_mapping()) {
        for (name, version) in deps {
            if let Some(dep) = create_simple_dependency(name.as_str(), version.as_str(), None) {
                all_dependencies.push(dep);
            }
        }
    }

    if let Some(dev_deps) = data.get("devDependencies").and_then(|v| v.as_mapping()) {
        for (name, version) in dev_deps {
            if let Some(dep) =
                create_simple_dependency(name.as_str(), version.as_str(), Some("dev".to_string()))
            {
                all_dependencies.push(dep);
            }
        }
    }

    if let Some(peer_deps) = data.get("peerDependencies").and_then(|v| v.as_mapping()) {
        for (name, version) in peer_deps {
            if let Some(dep) =
                create_simple_dependency(name.as_str(), version.as_str(), Some("peer".to_string()))
            {
                all_dependencies.push(dep);
            }
        }
    }

    if let Some(opt_deps) = data
        .get("optionalDependencies")
        .and_then(|v| v.as_mapping())
    {
        for (name, version) in opt_deps {
            if let Some(dep) = create_simple_dependency(
                name.as_str(),
                version.as_str(),
                Some("optional".to_string()),
            ) {
                all_dependencies.push(dep);
            }
        }
    }

    all_dependencies
}

fn create_simple_dependency(
    name: Option<&str>,
    version: Option<&str>,
    scope: Option<String>,
) -> Option<Dependency> {
    let name = name?;
    let version = version?;

    let (namespace_str, pkg_name) = extract_namespace_and_name(name);
    let namespace = if !namespace_str.is_empty() {
        Some(namespace_str)
    } else {
        None
    };
    let purl = create_purl(&namespace, &pkg_name, version);

    let is_runtime = scope.as_deref() != Some("dev");
    let is_optional = scope.as_deref() == Some("optional");

    Some(Dependency {
        purl: Some(purl),
        extracted_requirement: Some(version.to_string()),
        scope,
        is_runtime: Some(is_runtime),
        is_optional: Some(is_optional),
        is_pinned: Some(true),
        is_direct: Some(false),
        resolved_package: None,
        extra_data: None,
    })
}

/// Extract dependency from package data
pub fn extract_dependency(
    clean_purl_fields: &str,
    data: &Value,
    lockfile_version: &str,
    is_dev_only_v9: bool,
) -> Option<Dependency> {
    let (namespace, name, version) = parse_purl_fields(clean_purl_fields, lockfile_version)?;

    // Create PURL
    let purl = create_purl(&namespace, &name, &version);

    // Extract integrity hash from resolution
    let (sha1, sha256, sha512, md5) = if let Some(resolution) = data.get("resolution") {
        if let Some(integrity) = resolution.get("integrity") {
            if let Some(integrity_str) = integrity.as_str() {
                parse_integrity(integrity_str)
            } else {
                (None, None, None, None)
            }
        } else {
            (None, None, None, None)
        }
    } else {
        (None, None, None, None)
    };

    // Extract pnpm-specific fields for extra_data
    let mut extra_data = std::collections::HashMap::new();

    if let (Some(_has_bin), Some(true)) = (
        data.get("hasBin"),
        data.get("hasBin").and_then(|v| v.as_bool()),
    ) {
        extra_data.insert("hasBin".to_string(), serde_json::Value::Bool(true));
    }

    if data.get("requiresBuild").and_then(|v| v.as_bool()) == Some(true) {
        extra_data.insert("requiresBuild".to_string(), serde_json::Value::Bool(true));
    }

    // Check if this is an optional dependency
    let is_optional = data
        .get("optional")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if is_optional {
        extra_data.insert("optional".to_string(), serde_json::Value::Bool(true));
    }

    // Check if this is a dev dependency
    // For v5/v6: Use the dev flag from packages section
    // For v9: Use the is_dev_only_v9 parameter (computed from graph traversal)
    let is_dev = if lockfile_version.starts_with('9') {
        is_dev_only_v9
    } else {
        data.get("dev").and_then(|v| v.as_bool()).unwrap_or(false)
    };

    if is_dev {
        extra_data.insert("dev".to_string(), serde_json::Value::Bool(true));
    }

    // Determine scope based on dev/optional flags
    let scope = if is_dev {
        Some("dev".to_string())
    } else if is_optional {
        Some("optional".to_string())
    } else {
        None
    };

    // Dev dependencies are not runtime dependencies
    let is_runtime = !is_dev;

    let all_dependencies = parse_nested_dependencies(data);

    let resolved_package = ResolvedPackage {
        primary_language: Some("JavaScript".to_string()),
        download_url: None,
        sha1,
        sha256,
        sha512,
        md5,
        is_virtual: true,
        extra_data: None,
        dependencies: all_dependencies,
        repository_homepage_url: None,
        repository_download_url: None,
        api_data_url: None,
        datasource_id: Some(DatasourceId::PnpmLockYaml),
        purl: None,
        ..ResolvedPackage::new(
            PackageType::Npm,
            namespace.clone().unwrap_or_default(),
            name.clone(),
            version.clone(),
        )
    };

    let dependency = Dependency {
        purl: Some(purl),
        extracted_requirement: Some(version),
        scope,
        is_runtime: Some(is_runtime),
        is_optional: Some(is_optional),
        is_pinned: Some(true),
        is_direct: Some(false),
        resolved_package: Some(Box::new(resolved_package)),
        extra_data: if extra_data.is_empty() {
            None
        } else {
            Some(extra_data)
        },
    };

    Some(dependency)
}

/// Parse namespace, name, and version from purl_fields based on lockfile version
pub fn parse_purl_fields(
    clean_purl_fields: &str,
    lockfile_version: &str,
) -> Option<(Option<String>, String, String)> {
    let sections: Vec<&str> = clean_purl_fields.split('/').collect();

    if lockfile_version.starts_with('6') {
        let last_at_pos = clean_purl_fields.rfind('@')?;
        let version = clean_purl_fields[last_at_pos + 1..].to_string();
        let name_part = &clean_purl_fields[..last_at_pos];

        if let Some(stripped) = name_part.strip_prefix('@') {
            let parts: Vec<&str> = stripped.split('/').collect();
            if parts.len() == 2 {
                Some((
                    Some(format!("@{}", parts[0])),
                    parts[1].to_string(),
                    version,
                ))
            } else {
                None
            }
        } else if name_part.contains('/') {
            let parts: Vec<&str> = name_part.split('/').collect();
            if parts.len() == 2 && parts[0].starts_with('@') {
                Some((Some(parts[0].to_string()), parts[1].to_string(), version))
            } else if parts.len() == 2 {
                Some((None, format!("{}/{}", parts[0], parts[1]), version))
            } else {
                Some((None, name_part.to_string(), version))
            }
        } else {
            Some((None, name_part.to_string(), version))
        }
    } else if lockfile_version.starts_with('9') {
        let last_at_pos = clean_purl_fields.rfind('@')?;
        let name_part = &clean_purl_fields[..last_at_pos];
        let version = clean_purl_fields[last_at_pos + 1..].to_string();

        if let Some(stripped) = name_part.strip_prefix('@') {
            let parts: Vec<&str> = stripped.split('/').collect();
            if parts.len() == 2 {
                Some((Some(parts[0].to_string()), parts[1].to_string(), version))
            } else {
                None
            }
        } else {
            Some((None, name_part.to_string(), version))
        }
    } else if lockfile_version.starts_with('5') {
        if sections.len() == 4 && sections[0].is_empty() && sections[1].starts_with('@') {
            let scope = sections[1];
            let name = sections[2];
            let version = sections[3].to_string();
            Some((Some(scope.to_string()), name.to_string(), version))
        } else if sections.len() == 4 && sections[0].is_empty() && !sections[1].starts_with('@') {
            let name = sections[1];
            let version = sections[2].to_string();
            Some((None, name.to_string(), version))
        } else if sections.len() == 3 && sections[0].starts_with('@') {
            let scope = sections[0];
            let name = sections[1];
            let version = sections[2].to_string();
            Some((Some(scope.to_string()), name.to_string(), version))
        } else if sections.len() == 2 {
            let name = sections[0];
            let version = sections[1].to_string();
            Some((None, name.to_string(), version))
        } else {
            None
        }
    } else {
        None
    }
}

pub fn create_purl(namespace: &Option<String>, name: &str, version: &str) -> String {
    let full_name = match namespace {
        Some(ns) if !ns.is_empty() => {
            let ns_with_at = if ns.starts_with('@') {
                ns.clone()
            } else {
                format!("@{}", ns)
            };
            format!("{}/{}", ns_with_at, name)
        }
        _ => name.to_string(),
    };
    npm_purl(&full_name, Some(version)).unwrap_or_else(|| format!("pkg:npm/{}", name))
}

/// Parse integrity field to extract sha1, sha256, sha512, and md5
fn parse_integrity(
    integrity: &str,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    if let Some(dash_pos) = integrity.find('-') {
        let algo = integrity[..dash_pos].to_lowercase();
        let hash = integrity[dash_pos + 1..].to_string();

        if algo.contains("sha1") {
            (Some(hash), None, None, None)
        } else if algo.contains("sha256") {
            (None, Some(hash), None, None)
        } else if algo.contains("sha512") {
            (None, None, Some(hash), None)
        } else if algo.contains("md5") {
            (None, None, None, Some(hash))
        } else {
            (None, None, None, None)
        }
    } else {
        (None, None, None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_pnpm_version_v5() {
        let yaml = "lockfileVersion: 5.4\n";
        let data: Value = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(detect_pnpm_version(&data), "5.4");
    }

    #[test]
    fn test_detect_pnpm_version_v6() {
        let yaml = "lockfileVersion: '6.0'\n";
        let data: Value = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(detect_pnpm_version(&data), "6.0");
    }

    #[test]
    fn test_detect_pnpm_version_v9() {
        let yaml = "lockfileVersion: '9.0'\n";
        let data: Value = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(detect_pnpm_version(&data), "9.0");
    }

    #[test]
    fn test_clean_purl_fields_v6() {
        let purl_fields = "@babel/runtime@7.18.9(react@18.0.0)";
        assert_eq!(
            clean_purl_fields(purl_fields, "6.0"),
            "@babel/runtime@7.18.9"
        );

        let purl_fields = "@babel/runtime@7.18.9(";
        assert_eq!(
            clean_purl_fields(purl_fields, "6.0"),
            "@babel/runtime@7.18.9"
        );
    }

    #[test]
    fn test_clean_purl_fields_v5() {
        let purl_fields = "/_/@headlessui/react/1.6.6_biqbaboplfbrettd7655fr4n2y";
        assert_eq!(
            clean_purl_fields(purl_fields, "5.0"),
            "_/@headlessui/react/1.6.6"
        );
    }

    #[test]
    fn test_clean_purl_fields_v9() {
        let purl_fields = "@babel/helper-string-parser@7.24.8";
        assert_eq!(
            clean_purl_fields(purl_fields, "9.0"),
            "@babel/helper-string-parser@7.24.8"
        );
    }

    #[test]
    fn test_parse_purl_fields_v6_scoped() {
        let (namespace, name, version) = parse_purl_fields("@babel/runtime@7.18.9", "6.0").unwrap();
        assert_eq!(namespace, Some("@babel".to_string()));
        assert_eq!(name, "runtime".to_string());
        assert_eq!(version, "7.18.9".to_string());
    }

    #[test]
    fn test_parse_purl_fields_v9_scoped() {
        let (namespace, name, version) =
            parse_purl_fields("@babel/helper-string-parser@7.24.8", "9.0").unwrap();
        assert_eq!(namespace, Some("babel".to_string()));
        assert_eq!(name, "helper-string-parser".to_string());
        assert_eq!(version, "7.24.8".to_string());
    }

    #[test]
    fn test_parse_purl_fields_v9_non_scoped() {
        let (namespace, name, version) =
            parse_purl_fields("anve-upload-upyun@1.0.8", "9.0").unwrap();
        assert_eq!(namespace, None);
        assert_eq!(name, "anve-upload-upyun".to_string());
        assert_eq!(version, "1.0.8".to_string());
    }

    #[test]
    fn test_parse_purl_fields_v5_scoped() {
        let (namespace, name, version) = parse_purl_fields("@babel/runtime/7.18.9", "5.0").unwrap();
        assert_eq!(namespace, Some("@babel".to_string()));
        assert_eq!(name, "runtime".to_string());
        assert_eq!(version, "7.18.9".to_string());
    }

    #[test]
    fn test_parse_integrity() {
        let (sha1, sha256, sha512, md5) = parse_integrity(
            "sha512-luRj/9OnHgR0f5t4e38q9K9A7l4t8uq4nB/eZ/eZ/e2/e3/e4/e5/e6/e7/e8/e9/e0/eva",
        );
        assert!(sha1.is_none());
        assert!(sha256.is_none());
        assert!(sha512.is_some());
        assert!(md5.is_none());

        let (sha1, sha256, sha512, md5) = parse_integrity("sha1-abc123");
        assert!(sha1.is_some());
        assert!(sha256.is_none());
        assert!(sha512.is_none());
        assert!(md5.is_none());
    }
}

crate::register_parser!(
    "pnpm lockfile",
    &["**/pnpm-lock.yaml", "**/shrinkwrap.yaml"],
    "npm",
    "JavaScript",
    Some("https://pnpm.io/next/git#lockfile-compatibility"),
);
