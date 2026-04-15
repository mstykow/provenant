//! Parser for Linux OS release metadata files.
//!
//! Extracts distribution information from `/etc/os-release` and `/usr/lib/os-release`
//! files which identify the Linux distribution and version.
//!
//! # Supported Formats
//! - `/etc/os-release` (primary location)
//! - `/usr/lib/os-release` (fallback location)
//!
//! # Key Features
//! - Distribution identification (name, version, ID)
//! - Namespace mapping (debian, fedora, etc.)
//! - Pretty name extraction
//! - Version ID parsing
//!
//! # Implementation Notes
//! - Format: shell-compatible key=value pairs
//! - Values may be quoted with single or double quotes
//! - Comments start with #
//! - Spec: https://www.freedesktop.org/software/systemd/man/os-release.html

use crate::models::{DatasourceId, PackageType};
use std::collections::HashMap;
use std::path::Path;

use crate::parser_warn as warn;

use crate::models::PackageData;

use super::PackageParser;
use super::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};

const PACKAGE_TYPE: PackageType = PackageType::LinuxDistro;

/// Parser for Linux OS release metadata files
pub struct OsReleaseParser;

impl PackageParser for OsReleaseParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.to_str()
            .is_some_and(|p| p.ends_with("/etc/os-release") || p.ends_with("/usr/lib/os-release"))
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read os-release file {:?}: {}", path, e);
                return vec![PackageData {
                    package_type: Some(PACKAGE_TYPE),
                    datasource_id: Some(DatasourceId::EtcOsRelease),
                    ..Default::default()
                }];
            }
        };

        vec![parse_os_release(&content)]
    }
}

pub(crate) fn parse_os_release(content: &str) -> PackageData {
    let fields = parse_key_value_pairs(content);

    let id = fields.get("ID").map(|s| s.as_str()).unwrap_or("");
    let id_like = fields.get("ID_LIKE").map(|s| s.as_str());
    let pretty_name = fields
        .get("PRETTY_NAME")
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    let version_id = fields.get("VERSION_ID").cloned();

    // Namespace and name mapping logic from Python reference
    let (namespace, name) = determine_namespace_and_name(id, id_like, &pretty_name);

    let homepage_url = fields.get("HOME_URL").cloned().map(truncate_field);
    let bug_tracking_url = fields.get("BUG_REPORT_URL").cloned().map(truncate_field);
    let code_view_url = fields.get("SUPPORT_URL").cloned().map(truncate_field);

    PackageData {
        package_type: Some(PACKAGE_TYPE),
        namespace: Some(truncate_field(namespace.to_string())),
        name: Some(truncate_field(name.to_string())),
        version: version_id.map(truncate_field),
        homepage_url,
        bug_tracking_url,
        code_view_url,
        datasource_id: Some(DatasourceId::EtcOsRelease),
        ..Default::default()
    }
}

fn determine_namespace_and_name<'a>(
    id: &'a str,
    id_like: Option<&'a str>,
    pretty_name: &'a str,
) -> (&'a str, &'a str) {
    match id {
        "debian" => {
            let name = if pretty_name.contains("distroless") {
                "distroless"
            } else {
                "debian"
            };
            ("debian", name)
        }
        "ubuntu" if id_like == Some("debian") => ("debian", "ubuntu"),
        id if id.starts_with("fedora") || id_like == Some("fedora") => {
            let name = id_like.unwrap_or(id);
            (id, name)
        }
        _ => {
            let name = id_like.unwrap_or(id);
            (id, name)
        }
    }
}

fn parse_key_value_pairs(content: &str) -> HashMap<String, String> {
    let mut fields = HashMap::new();

    for line in content.lines().take(MAX_ITERATION_COUNT) {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Parse KEY=VALUE format
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim().to_string();
            let value = unquote(value.trim());
            fields.insert(key, value);
        }
    }

    fields
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

crate::register_parser!(
    "Linux OS release metadata file",
    &["*etc/os-release", "*usr/lib/os-release"],
    "linux-distro",
    "",
    Some("https://www.freedesktop.org/software/systemd/man/os-release.html"),
);
