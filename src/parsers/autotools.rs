//! Parser for Autotools configure scripts.
//!
//! Extracts basic package metadata from Autotools configure files by using
//! the parent directory name as the package name.
//!
//! # Supported Formats
//! - configure (Autotools configure script)
//! - configure.ac (Autoconf input file)
//!
//! # Key Features
//! - Lightweight detection based on parent directory name
//! - No file content parsing required
//!
//! # Implementation Notes
//! - This parser does NOT read file contents, only extracts parent directory name
//! - configure.in is NOT supported (deprecated legacy format)
//! - Returns minimal PackageData with only package_type and name fields

use crate::models::PackageData;
use crate::models::{DatasourceId, PackageType};
use packageurl::PackageUrl;
use std::path::Path;

use super::PackageParser;

/// Parser for Autotools configure scripts.
///
/// Extracts the parent directory name as the package name without parsing file contents.
pub struct AutotoolsConfigureParser;

impl PackageParser for AutotoolsConfigureParser {
    const PACKAGE_TYPE: PackageType = PackageType::Autotools;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "configure" || name == "configure.ac")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let name = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .or_else(|| {
                path.file_name()
                    .is_some_and(|name| name == "configure" || name == "configure.ac")
                    .then_some("input".to_string())
            });

        let purl = name.as_deref().and_then(|name| {
            PackageUrl::new(Self::PACKAGE_TYPE.as_str(), name)
                .ok()
                .map(|purl| purl.to_string())
        });

        vec![PackageData {
            package_type: Some(Self::PACKAGE_TYPE),
            name,
            datasource_id: Some(DatasourceId::AutotoolsConfigure),
            purl,
            ..Default::default()
        }]
    }
}

crate::register_parser!(
    "Autotools configure script",
    &["**/configure", "**/configure.ac"],
    "autotools",
    "C",
    Some("https://www.gnu.org/software/autoconf/"),
);
