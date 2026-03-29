use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use serde_json::Value;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};

use super::PackageParser;

pub struct SwiftManifestJsonParser;

impl PackageParser for SwiftManifestJsonParser {
    const PACKAGE_TYPE: PackageType = PackageType::Swift;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let filename = path.file_name().and_then(|n| n.to_str());

        let is_json_file = filename
            .map(|n| n.ends_with(".swift.json") || n.ends_with(".swift.deplock"))
            .unwrap_or(false);
        let is_raw_swift = filename.map(|n| n == "Package.swift").unwrap_or(false);

        vec![if is_json_file {
            let json_content = match read_swift_manifest_json(path) {
                Ok(content) => content,
                Err(e) => {
                    warn!(
                        "Failed to read or parse Swift manifest JSON at {:?}: {}",
                        path, e
                    );
                    return vec![default_package_data(path)];
                }
            };
            parse_swift_manifest(&json_content)
        } else if is_raw_swift {
            match dump_package_json(path) {
                Ok(json_str) => match serde_json::from_str::<Value>(&json_str) {
                    Ok(json) => parse_swift_manifest(&json),
                    Err(e) => {
                        warn!(
                            "Swift toolchain generated invalid JSON for {:?}: {}",
                            path, e
                        );
                        default_package_data(path)
                    }
                },
                Err(e) => {
                    warn!(
                        "Cannot auto-generate Package.swift.json for {:?}: {}. \
                             Swift toolchain may not be installed. \
                             To scan this file, manually run: swift package dump-package > Package.swift.json",
                        path, e
                    );
                    default_package_data(path)
                }
            }
        } else {
            default_package_data(path)
        }]
    }

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                name.ends_with(".swift.json")
                    || name.ends_with(".swift.deplock")
                    || name == "Package.swift"
            })
    }
}

fn read_swift_manifest_json(path: &Path) -> Result<Value, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;

    serde_json::from_str(&content).map_err(|e| format!("Failed to parse JSON: {}", e))
}

fn parse_swift_manifest(manifest: &Value) -> PackageData {
    let name = manifest
        .get("name")
        .and_then(|v| v.as_str())
        .map(String::from);

    let dependencies = get_dependencies(manifest.get("dependencies"));
    let platforms = manifest.get("platforms").cloned();

    let tools_version = manifest
        .get("toolsVersion")
        .and_then(|tv| tv.get("_version"))
        .and_then(|v| v.as_str())
        .map(String::from);

    let mut extra_data = HashMap::new();
    if let Some(platforms_val) = platforms {
        extra_data.insert("platforms".to_string(), platforms_val);
    }
    if let Some(ref tv) = tools_version {
        extra_data.insert(
            "swift_tools_version".to_string(),
            serde_json::Value::String(tv.clone()),
        );
    }

    let purl = create_package_url(&name, &None);

    PackageData {
        package_type: Some(SwiftManifestJsonParser::PACKAGE_TYPE),
        namespace: None,
        name,
        version: None,
        qualifiers: None,
        subpath: None,
        primary_language: Some("Swift".to_string()),
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
        extra_data: if extra_data.is_empty() {
            None
        } else {
            Some(extra_data)
        },
        dependencies,
        repository_homepage_url: None,
        repository_download_url: None,
        api_data_url: None,
        datasource_id: Some(DatasourceId::SwiftPackageManifestJson),
        purl,
    }
}

fn get_dependencies(dependencies: Option<&Value>) -> Vec<Dependency> {
    let Some(deps_array) = dependencies.and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    let mut dependent_packages = Vec::new();

    for dependency in deps_array {
        if let Some(dep) = parse_manifest_dependency(dependency) {
            dependent_packages.push(dep);
        }
    }

    dependent_packages
}

fn parse_manifest_dependency(dependency: &Value) -> Option<Dependency> {
    if let Some(source_control) = dependency.get("sourceControl").and_then(|v| v.as_array())
        && let Some(source) = source_control.first()
    {
        let identity = source
            .get("identity")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        let (namespace, dep_name) = extract_namespace_and_name(source, identity);
        let (version, is_pinned, requirement_kind) = extract_version_requirement(source);
        let purl = create_dependency_purl(&namespace, &dep_name, &version, is_pinned);
        let mut extra_data = HashMap::from([
            (
                "dependency_kind".to_string(),
                serde_json::Value::String("sourceControl".to_string()),
            ),
            (
                "requirement_kind".to_string(),
                serde_json::Value::String(requirement_kind.to_string()),
            ),
        ]);
        if let Some(remote) = source
            .get("location")
            .and_then(|loc| loc.get("remote"))
            .and_then(|remote| remote.as_array())
            .and_then(|arr| arr.first())
            .and_then(|first| first.get("urlString"))
            .and_then(|v| v.as_str())
        {
            extra_data.insert(
                "location".to_string(),
                serde_json::Value::String(remote.to_string()),
            );
        }

        return Some(Dependency {
            purl: Some(purl),
            extracted_requirement: version,
            scope: Some("dependencies".to_string()),
            is_runtime: None,
            is_optional: Some(false),
            is_pinned: Some(is_pinned),
            is_direct: Some(true),
            resolved_package: None,
            extra_data: Some(extra_data),
        });
    }

    if let Some(file_system) = dependency.get("fileSystem").and_then(|v| v.as_array())
        && let Some(source) = file_system.first()
    {
        let identity = source
            .get("identity")
            .and_then(|v| v.as_str())
            .or_else(|| source.get("name").and_then(|v| v.as_str()))
            .unwrap_or_default();
        if identity.is_empty() {
            return None;
        }

        let dep_name = identity.to_string();
        let purl = create_dependency_purl(&None, &dep_name, &None, false);
        let mut extra_data = HashMap::from([(
            "dependency_kind".to_string(),
            serde_json::Value::String("fileSystem".to_string()),
        )]);
        if let Some(path) = source.get("path").and_then(|v| v.as_str()) {
            extra_data.insert(
                "path".to_string(),
                serde_json::Value::String(path.to_string()),
            );
        }

        return Some(Dependency {
            purl: Some(purl),
            extracted_requirement: None,
            scope: Some("dependencies".to_string()),
            is_runtime: None,
            is_optional: Some(false),
            is_pinned: Some(false),
            is_direct: Some(true),
            resolved_package: None,
            extra_data: Some(extra_data),
        });
    }

    None
}

fn extract_namespace_and_name(source: &Value, identity: &str) -> (Option<String>, String) {
    let url = source
        .get("location")
        .and_then(|loc| loc.get("remote"))
        .and_then(|remote| remote.as_array())
        .and_then(|arr| arr.first())
        .and_then(|first| first.get("urlString"))
        .and_then(|v| v.as_str());

    match url {
        Some(url_str) => get_namespace_and_name(url_str),
        None => (None, identity.to_string()),
    }
}

/// Parses a repository URL into (namespace, name).
///
/// Example: `https://github.com/apple/swift-argument-parser.git`
/// yields namespace=`"github.com/apple"`, name=`"swift-argument-parser"`
pub fn get_namespace_and_name(url: &str) -> (Option<String>, String) {
    let (hostname, path) = if let Some(stripped) = url.strip_prefix("https://") {
        let rest = stripped.trim_end_matches('/');
        match rest.find('/') {
            Some(idx) => (Some(&rest[..idx]), &rest[idx + 1..]),
            None => (Some(rest), ""),
        }
    } else if let Some(stripped) = url.strip_prefix("http://") {
        let rest = stripped.trim_end_matches('/');
        match rest.find('/') {
            Some(idx) => (Some(&rest[..idx]), &rest[idx + 1..]),
            None => (Some(rest), ""),
        }
    } else {
        (None, url)
    };

    let clean_path = path
        .strip_suffix(".git")
        .unwrap_or(path)
        .trim_end_matches('/');

    if let Some(host) = hostname {
        let canonical = format!("{}/{}", host, clean_path);
        match canonical.rsplit_once('/') {
            Some((ns, name)) => (Some(ns.to_string()), name.to_string()),
            None => (None, canonical),
        }
    } else {
        match clean_path.rsplit_once('/') {
            Some((ns, name)) => (Some(ns.to_string()), name.to_string()),
            None => (None, clean_path.to_string()),
        }
    }
}

/// Handles four requirement types:
/// - `exact`: `["1.0.0"]` -> version="1.0.0", is_pinned=true
/// - `range`: `[{"lowerBound": "1.0.0", "upperBound": "2.0.0"}]` -> version="vers:swift/>=1.0.0|<2.0.0", is_pinned=false
/// - `branch`: `["main"]` -> version="main", is_pinned=false
/// - `revision`: `["abc123"]` -> version="abc123", is_pinned=true
fn extract_version_requirement(source: &Value) -> (Option<String>, bool, &'static str) {
    let Some(requirement) = source.get("requirement") else {
        return (None, false, "unknown");
    };

    if let Some(exact) = requirement.get("exact").and_then(|v| v.as_array())
        && let Some(version) = exact.first().and_then(|v| v.as_str())
    {
        return (Some(version.to_string()), true, "exact");
    }

    if let Some(range) = requirement.get("range").and_then(|v| v.as_array())
        && let Some(bound) = range.first()
    {
        let lower = bound.get("lowerBound").and_then(|v| v.as_str());
        let upper = bound.get("upperBound").and_then(|v| v.as_str());
        if let (Some(lb), Some(ub)) = (lower, upper) {
            let vers = format!("vers:swift/>={lb}|<{ub}");
            return (Some(vers), false, "range");
        }
    }

    if let Some(branch) = requirement.get("branch").and_then(|v| v.as_array())
        && let Some(branch_name) = branch.first().and_then(|v| v.as_str())
    {
        return (Some(branch_name.to_string()), false, "branch");
    }

    if let Some(revision) = requirement.get("revision").and_then(|v| v.as_array())
        && let Some(rev) = revision.first().and_then(|v| v.as_str())
    {
        return (Some(rev.to_string()), true, "revision");
    }

    (None, false, "unknown")
}

fn create_dependency_purl(
    namespace: &Option<String>,
    name: &str,
    version: &Option<String>,
    is_pinned: bool,
) -> String {
    let mut purl = match PackageUrl::new(SwiftManifestJsonParser::PACKAGE_TYPE.as_str(), name) {
        Ok(p) => p,
        Err(e) => {
            warn!(
                "Failed to create PackageUrl for swift dependency '{}': {}",
                name, e
            );
            return match (namespace, is_pinned.then_some(version.as_deref()).flatten()) {
                (Some(ns), Some(v)) => format!("pkg:swift/{}/{}@{}", ns, name, v),
                (Some(ns), None) => format!("pkg:swift/{}/{}", ns, name),
                (None, Some(v)) => format!("pkg:swift/{}@{}", name, v),
                (None, None) => format!("pkg:swift/{}", name),
            };
        }
    };

    if let Some(ns) = namespace
        && let Err(e) = purl.with_namespace(ns)
    {
        warn!(
            "Failed to set namespace '{}' for swift dependency '{}': {}",
            ns, name, e
        );
    }

    if is_pinned
        && let Some(v) = version
        && let Err(e) = purl.with_version(v)
    {
        warn!(
            "Failed to set version '{}' for swift dependency '{}': {}",
            v, name, e
        );
    }

    purl.to_string()
}

fn create_package_url(name: &Option<String>, version: &Option<String>) -> Option<String> {
    name.as_ref().and_then(|name| {
        let mut package_url =
            match PackageUrl::new(SwiftManifestJsonParser::PACKAGE_TYPE.as_str(), name) {
                Ok(p) => p,
                Err(e) => {
                    warn!(
                        "Failed to create PackageUrl for swift package '{}': {}",
                        name, e
                    );
                    return None;
                }
            };

        if let Some(v) = version
            && let Err(e) = package_url.with_version(v)
        {
            warn!(
                "Failed to set version '{}' for swift package '{}': {}",
                v, name, e
            );
            return None;
        }

        Some(package_url.to_string())
    })
}

pub fn invoke_swift_dump_package(package_dir: &Path) -> Result<String, String> {
    let output = Command::new("swift")
        .args(["package", "dump-package"])
        .current_dir(package_dir)
        .output()
        .map_err(|e| {
            format!(
                "Failed to execute 'swift package dump-package' in {:?}: {}. \
                 Is the Swift toolchain installed and available on PATH?",
                package_dir, e
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "'swift package dump-package' failed in {:?} (exit code: {:?}): {}",
            package_dir,
            output.status.code(),
            stderr.trim()
        ));
    }

    String::from_utf8(output.stdout)
        .map_err(|e| format!("swift dump-package output is not valid UTF-8: {}", e))
}

pub fn dump_package_json(package_swift_path: &Path) -> Result<String, String> {
    fs::read_to_string(package_swift_path).map_err(|e| {
        format!(
            "Failed to read Package.swift at {:?}: {}",
            package_swift_path, e
        )
    })?;

    let parent_dir = package_swift_path.parent().ok_or_else(|| {
        format!(
            "Cannot determine parent directory of {:?}",
            package_swift_path
        )
    })?;

    let json_output = invoke_swift_dump_package(parent_dir)?;

    serde_json::from_str::<Value>(&json_output)
        .map_err(|e| format!("swift dump-package produced invalid JSON: {}", e))?;

    Ok(json_output)
}

fn default_package_data(path: &Path) -> PackageData {
    let _ = path;

    PackageData {
        package_type: Some(SwiftManifestJsonParser::PACKAGE_TYPE),
        primary_language: Some("Swift".to_string()),
        datasource_id: Some(DatasourceId::SwiftPackageManifestJson),
        ..Default::default()
    }
}

crate::register_parser!(
    "Swift Package Manager manifest (Package.swift, Package.swift.json, Package.swift.deplock)",
    &[
        "**/Package.swift",
        "**/Package.swift.json",
        "**/Package.swift.deplock"
    ],
    "swift",
    "Swift",
    Some("https://docs.swift.org/package-manager/PackageDescription/PackageDescription.html"),
);
