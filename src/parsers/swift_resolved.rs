//! Parser for Swift Package.resolved lockfiles (v1, v2, v3).
//!
//! Format differences:
//! - **v1**: Pins under `object.pins[]`, uses `package` and `repositoryURL` fields
//! - **v2/v3**: Pins under `pins[]`, uses `identity`, `location`, and `kind` fields

use std::path::Path;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use serde::Deserialize;
use url::Url;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};
use crate::parsers::PackageParser;
use crate::parsers::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};

/// Parses Swift Package Manager lockfiles (Package.resolved).
///
/// Extracts pinned dependency versions from Swift Package Manager lockfiles.
/// Supports all three format versions (v1, v2, v3).
///
/// # Format Versions
/// - **v1**: Legacy format with `object.pins` array
/// - **v2**: Standard format with `pins` array at root
/// - **v3**: Latest format with `pins` array and enhanced metadata
///
/// # Features
/// - Extracts package identity, repository URL, version/branch/revision
/// - Generates namespace from repository URL (e.g., github.com/apple)
/// - Handles exact versions, branch references, and commit SHAs
///
/// # Example
/// ```no_run
/// use provenant::parsers::{PackageParser, SwiftPackageResolvedParser};
/// use std::path::Path;
///
/// let path = Path::new("Package.resolved");
/// let package_data = SwiftPackageResolvedParser::extract_first_package(path);
/// ```
pub struct SwiftPackageResolvedParser;

impl PackageParser for SwiftPackageResolvedParser {
    const PACKAGE_TYPE: PackageType = PackageType::Swift;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "Package.resolved" || name == ".package.resolved")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        vec![match parse_resolved(path) {
            Ok(data) => data,
            Err(e) => {
                warn!(
                    "Failed to parse Swift Package.resolved at {:?}: {}",
                    path, e
                );
                default_package_data()
            }
        }]
    }
}

#[derive(Deserialize)]
struct ResolvedFile {
    version: u32,
    #[serde(default)]
    pins: Vec<PinV2>,
    #[serde(default)]
    object: Option<ObjectV1>,
}

#[derive(Deserialize)]
struct ObjectV1 {
    #[serde(default)]
    pins: Vec<PinV1>,
}

#[derive(Deserialize)]
struct PinV2 {
    identity: Option<String>,
    kind: Option<String>,
    location: Option<String>,
    #[serde(default)]
    state: PinState,
}

#[derive(Deserialize)]
struct PinV1 {
    package: Option<String>,
    #[serde(rename = "repositoryURL")]
    repository_url: Option<String>,
    #[serde(default)]
    state: PinState,
}

#[derive(Deserialize, Default)]
struct PinState {
    version: Option<String>,
    revision: Option<String>,
}

fn parse_resolved(path: &Path) -> Result<PackageData, String> {
    let content = read_file(path)?;
    let resolved: ResolvedFile =
        serde_json::from_str(&content).map_err(|e| format!("JSON parse error: {}", e))?;

    let dependencies = match resolved.version {
        2 | 3 => parse_v2_v3_pins(&resolved.pins),
        1 => {
            let pins = resolved
                .object
                .as_ref()
                .map(|o| o.pins.as_slice())
                .unwrap_or(&[]);
            parse_v1_pins(pins)
        }
        other => {
            warn!(
                "Unknown Package.resolved version {}, attempting v2/v3 format",
                other
            );
            parse_v2_v3_pins(&resolved.pins)
        }
    };

    Ok(PackageData {
        package_type: Some(SwiftPackageResolvedParser::PACKAGE_TYPE),
        namespace: None,
        name: None,
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
        extra_data: None,
        dependencies,
        repository_homepage_url: None,
        repository_download_url: None,
        api_data_url: None,
        datasource_id: Some(DatasourceId::SwiftPackageResolved),
        purl: None,
    })
}

fn parse_v2_v3_pins(pins: &[PinV2]) -> Vec<Dependency> {
    pins.iter()
        .take(MAX_ITERATION_COUNT)
        .filter_map(pin_v2_to_dependency)
        .collect()
}

fn parse_v1_pins(pins: &[PinV1]) -> Vec<Dependency> {
    pins.iter()
        .take(MAX_ITERATION_COUNT)
        .filter_map(pin_v1_to_dependency)
        .collect()
}

fn pin_v2_to_dependency(pin: &PinV2) -> Option<Dependency> {
    let mut name = pin.identity.clone().map(truncate_field);
    let mut namespace: Option<String> = None;

    if let Some(location) = &pin.location
        && pin.kind.as_deref() == Some("remoteSourceControl")
        && let Some((ns, n)) = get_namespace_and_name(location)
    {
        namespace = Some(ns);
        name = Some(n);
    }

    let name = name?;

    let version = pin
        .state
        .version
        .clone()
        .or_else(|| pin.state.revision.clone())
        .map(truncate_field);

    let purl = build_purl(&name, namespace.as_deref(), version.as_deref());

    Some(Dependency {
        purl: purl.map(truncate_field),
        extracted_requirement: version,
        scope: Some("dependencies".to_string()),
        is_runtime: None,
        is_optional: None,
        is_pinned: Some(true),
        is_direct: None,
        resolved_package: None,
        extra_data: None,
    })
}

fn pin_v1_to_dependency(pin: &PinV1) -> Option<Dependency> {
    let mut name = pin.package.clone().map(truncate_field);
    let mut namespace: Option<String> = None;

    if let Some(url) = &pin.repository_url
        && let Some((ns, n)) = get_namespace_and_name(url)
    {
        namespace = Some(ns);
        name = Some(n);
    }

    let name = name?;

    let version = pin
        .state
        .version
        .clone()
        .or_else(|| pin.state.revision.clone())
        .map(truncate_field);

    let purl = build_purl(&name, namespace.as_deref(), version.as_deref());

    Some(Dependency {
        purl: purl.map(truncate_field),
        extracted_requirement: version,
        scope: Some("dependencies".to_string()),
        is_runtime: None,
        is_optional: None,
        is_pinned: Some(true),
        is_direct: None,
        resolved_package: None,
        extra_data: None,
    })
}

/// Extracts `(namespace, name)` from a repository URL.
///
/// `https://github.com/mapbox/turf-swift.git` -> `("github.com/mapbox", "turf-swift")`
fn get_namespace_and_name(url: &str) -> Option<(String, String)> {
    let parsed = Url::parse(url).ok()?;
    let hostname = parsed.host_str()?;

    let path = parsed.path().trim_start_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);

    let canonical = format!("{}/{}", hostname, path);

    let (ns, name) = canonical.rsplit_once('/')?;

    if name.is_empty() {
        return None;
    }

    Some((
        truncate_field(ns.to_string()),
        truncate_field(name.to_string()),
    ))
}

fn build_purl(name: &str, namespace: Option<&str>, version: Option<&str>) -> Option<String> {
    let mut purl = PackageUrl::new("swift", name).ok()?;
    if let Some(ns) = namespace {
        purl.with_namespace(ns).ok()?;
    }
    if let Some(v) = version {
        purl.with_version(v).ok()?;
    }
    Some(purl.to_string())
}

fn read_file(path: &Path) -> Result<String, String> {
    read_file_to_string(path, None).map_err(|e| format!("Failed to read file: {}", e))
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(SwiftPackageResolvedParser::PACKAGE_TYPE),
        primary_language: Some("Swift".to_string()),
        datasource_id: Some(DatasourceId::SwiftPackageResolved),
        ..Default::default()
    }
}

crate::register_parser!(
    "Swift Package.resolved lockfile",
    &["**/Package.resolved", "**/.package.resolved"],
    "swift",
    "Swift",
    Some(
        "https://docs.swift.org/package-manager/PackageDescription/PackageDescription.html#package-dependency"
    ),
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_namespace_and_name_github_with_git() {
        let (ns, name) =
            get_namespace_and_name("https://github.com/mapbox/turf-swift.git").unwrap();
        assert_eq!(ns, "github.com/mapbox");
        assert_eq!(name, "turf-swift");
    }

    #[test]
    fn test_get_namespace_and_name_github_without_git() {
        let (ns, name) = get_namespace_and_name("https://github.com/vapor/vapor").unwrap();
        assert_eq!(ns, "github.com/vapor");
        assert_eq!(name, "vapor");
    }

    #[test]
    fn test_get_namespace_and_name_deep_path() {
        let (ns, name) =
            get_namespace_and_name("https://github.com/swift-server/async-http-client.git")
                .unwrap();
        assert_eq!(ns, "github.com/swift-server");
        assert_eq!(name, "async-http-client");
    }

    #[test]
    fn test_get_namespace_and_name_invalid_url() {
        assert!(get_namespace_and_name("not-a-url").is_none());
    }

    #[test]
    fn test_build_purl_with_all_fields() {
        let purl = build_purl("turf-swift", Some("github.com/mapbox"), Some("2.8.0"));
        assert_eq!(
            purl.as_deref(),
            Some("pkg:swift/github.com/mapbox/turf-swift@2.8.0")
        );
    }

    #[test]
    fn test_build_purl_without_version() {
        let purl = build_purl("turf-swift", Some("github.com/mapbox"), None);
        assert_eq!(
            purl.as_deref(),
            Some("pkg:swift/github.com/mapbox/turf-swift")
        );
    }

    #[test]
    fn test_build_purl_without_namespace() {
        let purl = build_purl("MyPackage", None, Some("1.0.0"));
        assert_eq!(purl.as_deref(), Some("pkg:swift/MyPackage@1.0.0"));
    }
}
