use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::parser_warn as warn;
use serde_json::{Map, Value as JsonValue};

use crate::models::{DatasourceId, Dependency, PackageData, PackageType, ResolvedPackage};
use crate::parsers::utils::{npm_purl, parse_sri};

use super::PackageParser;

pub struct BunLockParser;

#[derive(Clone, Debug)]
struct ManifestDependencyInfo {
    scope: &'static str,
    is_runtime: bool,
    is_optional: bool,
}

struct WorkspaceContext {
    root_name: Option<String>,
    root_version: Option<String>,
    direct_deps: HashMap<String, ManifestDependencyInfo>,
    workspace_versions: HashMap<String, String>,
    workspace_entries: HashMap<String, JsonValue>,
}

impl PackageParser for BunLockParser {
    const PACKAGE_TYPE: PackageType = PackageType::Npm;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "bun.lock")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read bun.lock at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        let root: JsonValue = match json5::from_str(&content) {
            Ok(root) => root,
            Err(e) => {
                warn!("Failed to parse bun.lock at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        vec![parse_bun_lockfile(&root)]
    }
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(BunLockParser::PACKAGE_TYPE),
        primary_language: Some("JavaScript".to_string()),
        datasource_id: Some(DatasourceId::BunLock),
        extra_data: Some(HashMap::new()),
        ..Default::default()
    }
}

fn parse_bun_lockfile(root: &JsonValue) -> PackageData {
    let mut result = default_package_data();

    let workspace_context = extract_workspace_info(root);
    let (namespace, name) = workspace_context
        .root_name
        .as_deref()
        .map(split_namespace_name)
        .unwrap_or((None, None));

    result.namespace = namespace;
    result.name = name;
    result.version = workspace_context.root_version.clone();
    result.purl = result
        .name
        .as_ref()
        .map(|name| qualify_name(&result.namespace, name))
        .and_then(|full_name| npm_purl(&full_name, workspace_context.root_version.as_deref()));

    let extra_data = result.extra_data.get_or_insert_with(HashMap::new);
    if let Some(lockfile_version) = root.get("lockfileVersion").and_then(|value| value.as_i64()) {
        extra_data.insert(
            "lockfileVersion".to_string(),
            JsonValue::from(lockfile_version),
        );
    }
    if let Some(config_version) = root.get("configVersion").and_then(|value| value.as_i64()) {
        extra_data.insert("configVersion".to_string(), JsonValue::from(config_version));
    }
    if let Some(trusted) = root.get("trustedDependencies") {
        extra_data.insert("trustedDependencies".to_string(), trusted.clone());
    }

    let Some(packages) = root.get("packages").and_then(|value| value.as_object()) else {
        warn!("No packages field found in bun.lock");
        if extra_data.is_empty() {
            result.extra_data = None;
        }
        return result;
    };

    let mut dependencies = Vec::new();
    for (key, value) in packages {
        if let Some(dependency) = parse_package_entry(
            key,
            value,
            &workspace_context.direct_deps,
            &workspace_context.workspace_versions,
            &workspace_context.workspace_entries,
        ) {
            dependencies.push(dependency);
        }
    }

    result.dependencies = dependencies;
    if result
        .extra_data
        .as_ref()
        .is_some_and(|data| data.is_empty())
    {
        result.extra_data = None;
    }

    result
}

fn extract_workspace_info(root: &JsonValue) -> WorkspaceContext {
    let mut direct_deps = HashMap::new();
    let mut workspace_versions = HashMap::new();
    let mut workspace_entries = HashMap::new();

    let workspaces = root.get("workspaces").and_then(|value| value.as_object());
    let root_workspace = workspaces.and_then(|workspaces| workspaces.get(""));
    let root_name = root_workspace
        .and_then(|value| value.get("name"))
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned);
    let root_version = root_workspace
        .and_then(|value| value.get("version"))
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned);

    if let Some(workspaces) = workspaces {
        for workspace in workspaces.values() {
            if let Some(name) = workspace.get("name").and_then(|value| value.as_str())
                && let Some(version) = workspace.get("version").and_then(|value| value.as_str())
            {
                workspace_versions.insert(name.to_string(), version.to_string());
            }
            if let Some(name) = workspace.get("name").and_then(|value| value.as_str()) {
                workspace_entries.insert(name.to_string(), workspace.clone());
            }
        }
    }

    if let Some(workspaces) = workspaces {
        for workspace in workspaces.values() {
            insert_manifest_dependency_info(
                workspace.get("dependencies"),
                "dependencies",
                true,
                false,
                &mut direct_deps,
            );
            insert_manifest_dependency_info(
                workspace.get("devDependencies"),
                "devDependencies",
                false,
                true,
                &mut direct_deps,
            );
            insert_manifest_dependency_info(
                workspace.get("optionalDependencies"),
                "optionalDependencies",
                true,
                true,
                &mut direct_deps,
            );
            insert_manifest_dependency_info(
                workspace.get("peerDependencies"),
                "peerDependencies",
                true,
                false,
                &mut direct_deps,
            );
        }
    }

    WorkspaceContext {
        root_name,
        root_version,
        direct_deps,
        workspace_versions,
        workspace_entries,
    }
}

fn insert_manifest_dependency_info(
    value: Option<&JsonValue>,
    scope: &'static str,
    is_runtime: bool,
    is_optional: bool,
    out: &mut HashMap<String, ManifestDependencyInfo>,
) {
    let Some(map) = value.and_then(|value| value.as_object()) else {
        return;
    };

    for name in map.keys() {
        out.insert(
            name.clone(),
            ManifestDependencyInfo {
                scope,
                is_runtime,
                is_optional,
            },
        );
    }
}

fn parse_package_entry(
    key: &str,
    value: &JsonValue,
    direct_deps: &HashMap<String, ManifestDependencyInfo>,
    workspace_versions: &HashMap<String, String>,
    workspace_entries: &HashMap<String, JsonValue>,
) -> Option<Dependency> {
    let tuple = value.as_array()?;
    let resolution = tuple.first()?.as_str()?;
    let (package_name, locator) = split_locator(resolution)?;
    let package_version = resolve_locator_version(&package_name, &locator, workspace_versions);

    let manifest_info = direct_deps
        .get(key)
        .or_else(|| direct_deps.get(&package_name));
    let (scope, is_runtime, is_optional, is_direct) = manifest_info
        .map(|info| {
            (
                info.scope.to_string(),
                info.is_runtime,
                info.is_optional,
                true,
            )
        })
        .unwrap_or_else(|| ("dependencies".to_string(), true, false, false));

    let purl = npm_purl(&package_name, package_version.as_deref());
    let resolved_download_url =
        resolved_download_url(&package_name, &locator, tuple, package_version.as_deref());
    let (sha1, sha256, sha512, md5) = parse_integrity_tuple(tuple);
    let nested_dependencies =
        extract_nested_dependencies(&package_name, tuple, workspace_versions, workspace_entries);

    let (namespace, name) = split_namespace_name(&package_name);
    let resolved_package = ResolvedPackage {
        primary_language: Some("JavaScript".to_string()),
        download_url: resolved_download_url,
        sha1,
        sha256,
        sha512,
        md5,
        is_virtual: true,
        extra_data: None,
        dependencies: nested_dependencies,
        repository_homepage_url: None,
        repository_download_url: None,
        api_data_url: None,
        datasource_id: Some(DatasourceId::BunLock),
        purl: None,
        ..ResolvedPackage::new(
            BunLockParser::PACKAGE_TYPE,
            namespace.unwrap_or_default(),
            name.unwrap_or_else(|| package_name.clone()),
            package_version.clone().unwrap_or_default(),
        )
    };

    Some(Dependency {
        purl,
        extracted_requirement: Some(package_version.clone().unwrap_or(locator.clone())),
        scope: Some(scope),
        is_runtime: Some(is_runtime),
        is_optional: Some(is_optional),
        is_pinned: Some(true),
        is_direct: Some(is_direct),
        resolved_package: Some(Box::new(resolved_package)),
        extra_data: None,
    })
}

fn split_locator(resolution: &str) -> Option<(String, String)> {
    let (name, locator) = resolution.rsplit_once('@')?;
    if name.is_empty() || locator.is_empty() {
        return None;
    }
    Some((name.to_string(), locator.to_string()))
}

fn resolve_locator_version(
    package_name: &str,
    locator: &str,
    workspace_versions: &HashMap<String, String>,
) -> Option<String> {
    if let Some(path) = locator.strip_prefix("workspace:") {
        return workspace_versions
            .get(package_name)
            .cloned()
            .or_else(|| workspace_versions.get(path).cloned());
    }

    if locator.starts_with("file:")
        || locator.starts_with("link:")
        || locator.starts_with("github:")
        || locator.starts_with("git+")
        || locator.starts_with("http://")
        || locator.starts_with("https://")
    {
        return None;
    }

    Some(locator.to_string())
}

fn resolved_download_url(
    package_name: &str,
    locator: &str,
    tuple: &[JsonValue],
    version: Option<&str>,
) -> Option<String> {
    if let Some(url) = tuple.get(1).and_then(|value| value.as_str())
        && !url.is_empty()
    {
        return Some(url.to_string());
    }

    if locator.starts_with("workspace:")
        || locator.starts_with("file:")
        || locator.starts_with("link:")
    {
        return None;
    }

    if locator.starts_with("http://")
        || locator.starts_with("https://")
        || locator.starts_with("git+")
        || locator.starts_with("github:")
    {
        return Some(locator.to_string());
    }

    version.and_then(|version| default_registry_download_url(package_name, version))
}

fn default_registry_download_url(package_name: &str, version: &str) -> Option<String> {
    let (namespace, name) = split_namespace_name(package_name);
    let name = name?;
    let package_path = qualify_name(&namespace, &name);
    Some(format!(
        "https://registry.npmjs.org/{}/-/{}-{}.tgz",
        package_path, name, version
    ))
}

fn parse_integrity_tuple(
    tuple: &[JsonValue],
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    let integrity = tuple.iter().rev().find_map(|value| {
        value.as_str().filter(|value| {
            value.starts_with("sha1-")
                || value.starts_with("sha256-")
                || value.starts_with("sha512-")
                || value.starts_with("md5-")
        })
    });

    let Some(integrity) = integrity else {
        return (None, None, None, None);
    };

    match parse_sri(integrity) {
        Some((algo, hash)) if algo == "sha1" => (Some(hash), None, None, None),
        Some((algo, hash)) if algo == "sha256" => (None, Some(hash), None, None),
        Some((algo, hash)) if algo == "sha512" => (None, None, Some(hash), None),
        Some((algo, hash)) if algo == "md5" => (None, None, None, Some(hash)),
        _ => (None, None, None, None),
    }
}

fn extract_nested_dependencies(
    package_name: &str,
    tuple: &[JsonValue],
    workspace_versions: &HashMap<String, String>,
    workspace_entries: &HashMap<String, JsonValue>,
) -> Vec<Dependency> {
    let info = tuple
        .iter()
        .find_map(|value| value.as_object())
        .or_else(|| {
            workspace_entries
                .get(package_name)
                .and_then(|value| value.as_object())
        });
    let Some(info) = info else {
        return Vec::new();
    };

    let mut dependencies = Vec::new();
    dependencies.extend(build_nested_dependencies(
        info.get("dependencies").and_then(|value| value.as_object()),
        "dependencies",
        true,
        false,
        workspace_versions,
    ));
    dependencies.extend(build_nested_dependencies(
        info.get("optionalDependencies")
            .and_then(|value| value.as_object()),
        "optionalDependencies",
        true,
        true,
        workspace_versions,
    ));
    dependencies.extend(build_nested_dependencies(
        info.get("peerDependencies")
            .and_then(|value| value.as_object()),
        "peerDependencies",
        true,
        false,
        workspace_versions,
    ));
    dependencies
}

fn build_nested_dependencies(
    deps: Option<&Map<String, JsonValue>>,
    scope: &str,
    is_runtime: bool,
    is_optional: bool,
    workspace_versions: &HashMap<String, String>,
) -> Vec<Dependency> {
    let Some(deps) = deps else {
        return Vec::new();
    };

    deps.iter()
        .filter_map(|(name, value)| {
            let requirement = value.as_str()?;
            let version = if requirement.starts_with("workspace:") {
                workspace_versions.get(name).map(String::as_str)
            } else {
                None
            };

            Some(Dependency {
                purl: npm_purl(name, version),
                extracted_requirement: Some(requirement.to_string()),
                scope: Some(scope.to_string()),
                is_runtime: Some(is_runtime),
                is_optional: Some(is_optional),
                is_pinned: Some(false),
                is_direct: Some(false),
                resolved_package: None,
                extra_data: None,
            })
        })
        .collect()
}

fn split_namespace_name(full_name: &str) -> (Option<String>, Option<String>) {
    if full_name.starts_with('@') {
        let mut parts = full_name.splitn(2, '/');
        let namespace = parts.next().map(ToOwned::to_owned);
        let name = parts.next().map(ToOwned::to_owned);
        (namespace, name)
    } else {
        (Some(String::new()), Some(full_name.to_string()))
    }
}

fn qualify_name(namespace: &Option<String>, name: &str) -> String {
    match namespace.as_deref() {
        Some("") | None => name.to_string(),
        Some(namespace) => format!("{}/{}", namespace, name),
    }
}

crate::register_parser!(
    "Bun lockfile",
    &["**/bun.lock"],
    "npm",
    "JavaScript",
    Some("https://bun.sh/docs/pm/lockfile"),
);
