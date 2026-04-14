//! Parser for RPM Mariner container manifest files.
//!
//! Extracts package metadata from `container-manifest-2` files which contain
//! installed RPM package information in Mariner distroless containers.
//!
//! # Supported Formats
//! - `container-manifest-2` - RPM Mariner distroless package manifest
//!
//! # Key Features
//! - Installed package identification
//! - Version and architecture metadata
//! - Checksum information
//!
//! # Implementation Notes
//! - Format: Tab-separated text with package metadata
//! - One package per line
//! - Spec: https://github.com/microsoft/marinara/

use crate::models::{DatasourceId, PackageType};
use std::path::Path;

use crate::parser_warn as warn;
use crate::parsers::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};

use crate::models::PackageData;

use super::PackageParser;

const PACKAGE_TYPE: PackageType = PackageType::Rpm;

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(PACKAGE_TYPE),
        namespace: Some("mariner".to_string()),
        datasource_id: Some(DatasourceId::RpmMarinerManifest),
        ..Default::default()
    }
}

/// Parser for RPM Mariner container manifest files
pub struct RpmMarinerManifestParser;

impl PackageParser for RpmMarinerManifestParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.to_str()
            .is_some_and(|p| p.ends_with("/var/lib/rpmmanifest/container-manifest-2"))
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read RPM Mariner manifest {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        parse_rpm_mariner_manifest(&content)
    }
}

pub(crate) fn parse_rpm_mariner_manifest(content: &str) -> Vec<PackageData> {
    let mut packages = Vec::new();

    for line in content.lines().take(MAX_ITERATION_COUNT) {
        // Only trim whitespace, not tabs
        let line = line.trim_matches(|c: char| c.is_whitespace() && c != '\t');
        if line.is_empty() {
            continue;
        }

        // Split by tabs
        let parts: Vec<&str> = line.split('\t').collect();

        // According to Python reference, manifest_attributes are:
        // ["name", "version", "n1", "n2", "party", "n3", "n4", "arch", "checksum_algo", "filename"]
        // We only care about name, version, arch, and filename

        if parts.len() < 10 {
            warn!(
                "Invalid RPM Mariner manifest line (expected 10 fields): {}",
                line
            );
            continue;
        }

        let name = truncate_field(parts[0].to_string());
        let version = truncate_field(parts[1].to_string());
        let arch = truncate_field(parts[7].to_string());
        let filename = truncate_field(parts[9].to_string());

        let qualifiers = if arch.is_empty() {
            None
        } else {
            let mut quals = std::collections::HashMap::new();
            quals.insert("arch".to_string(), arch.clone());
            Some(quals)
        };

        let extra_data = if filename.is_empty() {
            None
        } else {
            let mut extra = std::collections::HashMap::new();
            extra.insert(
                "filename".to_string(),
                serde_json::Value::String(filename.clone()),
            );
            Some(extra)
        };

        packages.push(PackageData {
            package_type: Some(PACKAGE_TYPE),
            namespace: Some(truncate_field("mariner".to_string())),
            name: if name.is_empty() { None } else { Some(name) },
            version: if version.is_empty() {
                None
            } else {
                Some(version)
            },
            qualifiers,
            datasource_id: Some(DatasourceId::RpmMarinerManifest),
            extra_data,
            ..Default::default()
        });
    }

    if packages.is_empty() {
        packages.push(default_package_data());
    }

    packages
}

crate::register_parser!(
    "RPM Mariner distroless package manifest",
    &["*var/lib/rpmmanifest/container-manifest-2"],
    "rpm",
    "",
    Some("https://github.com/microsoft/marinara/"),
);
