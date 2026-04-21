// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Parser for RPM license files in /usr/share/licenses/ directories.
//!
//! Identifies packages from their license files installed in the standard
//! /usr/share/licenses/ location, primarily used in Mariner distroless containers.

use crate::models::{DatasourceId, PackageType};
use std::path::Path;

use crate::models::PackageData;
use crate::parsers::PackageParser;
use crate::parsers::utils::truncate_field;

const PACKAGE_TYPE: PackageType = PackageType::Rpm;

/// Parser for RPM license files in /usr/share/licenses/ directories.
///
/// Identifies packages from their license files installed in the standard
/// /usr/share/licenses/ location, primarily used in Mariner distroless containers.
///
/// # Supported Formats
/// - `/usr/share/licenses/*/COPYING*` - COPYING license files
/// - `/usr/share/licenses/*/LICENSE*` - LICENSE files
///
/// # Key Features
/// - Extracts package name from directory path
/// - Supports Mariner distroless container convention
/// - Package URL generation with mariner namespace
///
/// # Implementation Notes
/// - Package name is extracted from the directory between `licenses/` and the filename
/// - For example: `/usr/share/licenses/openssl/LICENSE` → package name is "openssl"
/// - Does NOT perform license detection (that's handled by the license scanner)
/// - datasource_id: "rpm_package_licenses"
/// - namespace: "mariner"
pub struct RpmLicenseFilesParser;

impl PackageParser for RpmLicenseFilesParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        let path_str = path.to_string_lossy();

        // Check if path contains usr/share/licenses/
        if !path_str.contains("usr/share/licenses/") {
            return false;
        }

        // Get the filename
        if let Some(filename) = path.file_name().and_then(|f| f.to_str()) {
            // Match files starting with COPYING or LICENSE (case-sensitive)
            filename.starts_with("COPYING") || filename.starts_with("LICENSE")
        } else {
            false
        }
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        // Extract package name from path
        let path_str = path.to_string_lossy();

        // Split by usr/share/licenses/ and get the next path component
        let name = if let Some(after_licenses) = path_str.split("usr/share/licenses/").nth(1) {
            // Get the first path component after licenses/ (the package name)
            after_licenses
                .split('/')
                .next()
                .map(|s| truncate_field(s.to_string()))
        } else {
            None
        };

        // Build package data
        let mut pkg = PackageData {
            package_type: Some(PACKAGE_TYPE),
            datasource_id: Some(DatasourceId::RpmPackageLicenses),
            namespace: Some("mariner".to_string()),
            name: name.clone(),
            ..Default::default()
        };

        // Build PURL if we have a name
        if let Some(ref package_name) = name {
            use packageurl::PackageUrl;
            if let Ok(mut purl) = PackageUrl::new(PACKAGE_TYPE.as_str(), package_name)
                && purl.with_namespace("mariner").is_ok()
            {
                pkg.purl = Some(truncate_field(purl.to_string()));
            }
        }

        vec![pkg]
    }
}

crate::register_parser!(
    "RPM mariner distroless package license files",
    &[
        "*usr/share/licenses/*/COPYING*",
        "*usr/share/licenses/*/LICENSE*"
    ],
    "rpm",
    "",
    Some("https://github.com/microsoft/marinara/"),
);
