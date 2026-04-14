//! Parser for FreeBSD package manifest files.
//!
//! Extracts package metadata from FreeBSD compact manifest files (+COMPACT_MANIFEST)
//! which are JSON/YAML format files containing package information.
//!
//! # Supported Formats
//! - `+COMPACT_MANIFEST` (JSON/YAML format)
//!
//! # Key Features
//! - Package metadata extraction (name, version, description, etc.)
//! - Complex license logic handling (single/and/or/dual)
//! - URL construction from origin and architecture fields
//! - Qualifier extraction (arch, origin)
//! - Maintainer information parsing
//!
//! # Implementation Notes
//! - Uses `yaml_serde` for parsing (handles both JSON and YAML)
//! - Implements FreeBSD-specific license logic combining
//! - Graceful error handling with `warn!()` logs

use std::collections::HashMap;
use std::path::Path;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use serde::Deserialize;

use crate::models::{DatasourceId, PackageData, PackageType, Party};
use crate::parsers::utils::{read_file_to_string, truncate_field};
use crate::utils::spdx::{ExpressionRelation, combine_license_expressions_with_relation};

use super::PackageParser;
use super::license_normalization::{
    DeclaredLicenseMatchMetadata, NormalizedDeclaredLicense, build_declared_license_data,
    combine_normalized_licenses, empty_declared_license_data, normalize_declared_license_key,
};

const PACKAGE_TYPE: PackageType = PackageType::Freebsd;

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(PACKAGE_TYPE),
        datasource_id: Some(DatasourceId::FreebsdCompactManifest),
        ..Default::default()
    }
}

/// Parser for FreeBSD +COMPACT_MANIFEST files
pub struct FreebsdCompactManifestParser;

impl PackageParser for FreebsdCompactManifestParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| name == "+COMPACT_MANIFEST")
            .unwrap_or(false)
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read FreeBSD manifest {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        vec![parse_freebsd_manifest(&content)]
    }
}

#[derive(Debug, Deserialize)]
struct FreebsdManifest {
    name: Option<String>,
    version: Option<String>,
    #[serde(rename = "desc")]
    description: Option<String>,
    categories: Option<Vec<String>>,
    www: Option<String>,
    maintainer: Option<String>,
    origin: Option<String>,
    arch: Option<String>,
    licenses: Option<Vec<String>>,
    licenselogic: Option<String>,
}

pub(crate) fn parse_freebsd_manifest(content: &str) -> PackageData {
    let manifest: FreebsdManifest = match yaml_serde::from_str(content) {
        Ok(m) => m,
        Err(e) => {
            warn!("Failed to parse FreeBSD manifest: {}", e);
            return default_package_data();
        }
    };

    let name = manifest.name.map(truncate_field);
    let version = manifest.version.map(truncate_field);
    let description = manifest.description.map(truncate_field);
    let homepage_url = manifest.www.map(truncate_field);
    let keywords = manifest
        .categories
        .unwrap_or_default()
        .into_iter()
        .map(truncate_field)
        .collect();

    // Build qualifiers from arch and origin
    let mut qualifiers = HashMap::new();
    if let Some(ref arch) = manifest.arch {
        qualifiers.insert("arch".to_string(), truncate_field(arch.clone()));
    }
    if let Some(ref origin) = manifest.origin {
        qualifiers.insert("origin".to_string(), truncate_field(origin.clone()));
    }

    // Build parties from maintainer (just an email address)
    let mut parties = Vec::new();
    if let Some(maintainer_email) = manifest.maintainer {
        parties.push(Party {
            r#type: Some("person".to_string()),
            role: Some("maintainer".to_string()),
            name: None,
            email: Some(truncate_field(maintainer_email)),
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }

    // Build extracted_license_statement from licenses and licenselogic
    let licenses = manifest
        .licenses
        .map(|lics| lics.into_iter().map(truncate_field).collect());
    let licenselogic = manifest.licenselogic.map(truncate_field);
    let extracted_license_statement = build_license_statement(&licenses, &licenselogic);
    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        build_freebsd_license_data(
            licenses.as_deref(),
            licenselogic.as_deref(),
            extracted_license_statement.as_deref(),
        );

    // Build code_view_url from origin
    let code_view_url = manifest
        .origin
        .as_ref()
        .map(|origin| truncate_field(format!("https://svnweb.freebsd.org/ports/head/{}", origin)));

    // Build download_url from arch, name, and version
    let download_url = if let (Some(arch), Some(pkg_name), Some(pkg_version)) =
        (&manifest.arch, &name, &version)
    {
        Some(truncate_field(format!(
            "https://pkg.freebsd.org/{}/latest/All/{}-{}.txz",
            arch, pkg_name, pkg_version
        )))
    } else {
        None
    };

    let purl = name.as_ref().and_then(|pkg_name| {
        build_freebsd_purl(
            pkg_name,
            version.as_deref(),
            manifest.arch.as_deref(),
            manifest.origin.as_deref(),
        )
    });
    let purl = purl.map(truncate_field);

    PackageData {
        datasource_id: Some(DatasourceId::FreebsdCompactManifest),
        package_type: Some(PACKAGE_TYPE),
        name,
        version,
        description,
        homepage_url,
        keywords,
        parties,
        qualifiers: if qualifiers.is_empty() {
            None
        } else {
            Some(qualifiers)
        },
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        extracted_license_statement: extracted_license_statement.map(truncate_field),
        code_view_url,
        download_url,
        purl,
        ..Default::default()
    }
}

pub(crate) fn build_freebsd_purl(
    name: &str,
    version: Option<&str>,
    arch: Option<&str>,
    origin: Option<&str>,
) -> Option<String> {
    let mut purl = PackageUrl::new(PACKAGE_TYPE.as_str(), name).ok()?;

    if let Some(version) = version {
        purl.with_version(version).ok()?;
    }

    if let Some(arch) = arch {
        purl.add_qualifier("arch", arch).ok()?;
    }

    if let Some(origin) = origin {
        purl.add_qualifier("origin", origin).ok()?;
    }

    Some(purl.to_string())
}

pub(crate) fn build_freebsd_license_data(
    licenses: Option<&[String]>,
    licenselogic: Option<&str>,
    matched_text: Option<&str>,
) -> (
    Option<String>,
    Option<String>,
    Vec<crate::models::LicenseDetection>,
) {
    let Some(licenses) = licenses else {
        return empty_declared_license_data();
    };

    let normalized: Vec<_> = licenses
        .iter()
        .filter_map(|license| normalize_freebsd_license_name(license))
        .collect();

    if normalized.is_empty() {
        return empty_declared_license_data();
    }

    let combined = match licenselogic.unwrap_or("and") {
        "single" => normalized.into_iter().next(),
        "or" | "dual" => combine_normalized_licenses(normalized, " OR "),
        _ => combine_normalized_licenses(normalized, " AND "),
    };

    let Some(combined) = combined else {
        return empty_declared_license_data();
    };

    build_declared_license_data(
        combined,
        DeclaredLicenseMatchMetadata::single_line(matched_text.unwrap_or_default()),
    )
}

fn normalize_freebsd_license_name(license: &str) -> Option<NormalizedDeclaredLicense> {
    match license.trim() {
        "GPLv2" => Some(NormalizedDeclaredLicense::new("gpl-2.0", "GPL-2.0-only")),
        "GPLv3" => Some(NormalizedDeclaredLicense::new("gpl-3.0", "GPL-3.0-only")),
        "BSD3CLAUSE" => Some(NormalizedDeclaredLicense::new("bsd-new", "BSD-3-Clause")),
        "PSFL" => Some(NormalizedDeclaredLicense::new("psf-2.0", "PSF-2.0")),
        "RUBY" => Some(NormalizedDeclaredLicense::new("ruby", "Ruby")),
        other => normalize_declared_license_key(other),
    }
}

/// Builds the extracted_license_statement string from licenses and licenselogic.
///
/// # Logic:
/// - `licenselogic: "single"` → single license string (just the first license)
/// - `licenselogic: "and"` → join licenses with " AND "
/// - `licenselogic: "or"` or `"dual"` → join licenses with " OR "
/// - If `licenselogic` is missing or unknown → join with " AND " (default)
pub(crate) fn build_license_statement(
    licenses: &Option<Vec<String>>,
    licenselogic: &Option<String>,
) -> Option<String> {
    let license_list = licenses.as_ref()?;

    if license_list.is_empty() {
        return None;
    }

    // Filter out empty licenses and trim whitespace
    let filtered_licenses: Vec<String> = license_list
        .iter()
        .filter_map(|lic| {
            let trimmed = lic.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect();

    if filtered_licenses.is_empty() {
        return None;
    }

    let logic = licenselogic.as_deref().unwrap_or("and");

    match logic {
        "single" => Some(filtered_licenses[0].clone()),
        "or" | "dual" => {
            combine_license_expressions_with_relation(filtered_licenses, ExpressionRelation::Or)
        }
        _ => combine_license_expressions_with_relation(filtered_licenses, ExpressionRelation::And),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_is_match() {
        assert!(FreebsdCompactManifestParser::is_match(&PathBuf::from(
            "/path/to/+COMPACT_MANIFEST"
        )));
        assert!(FreebsdCompactManifestParser::is_match(&PathBuf::from(
            "+COMPACT_MANIFEST"
        )));
        assert!(!FreebsdCompactManifestParser::is_match(&PathBuf::from(
            "+MANIFEST"
        )));
        assert!(!FreebsdCompactManifestParser::is_match(&PathBuf::from(
            "COMPACT_MANIFEST"
        )));
        assert!(!FreebsdCompactManifestParser::is_match(&PathBuf::from(
            "package.json"
        )));
    }

    #[test]
    fn test_build_license_statement_single() {
        let licenses = Some(vec!["GPLv2".to_string()]);
        let logic = Some("single".to_string());
        let result = build_license_statement(&licenses, &logic);
        assert_eq!(result, Some("GPLv2".to_string()));
    }

    #[test]
    fn test_build_license_statement_and() {
        let licenses = Some(vec!["MIT".to_string(), "BSD-2-Clause".to_string()]);
        let logic = Some("and".to_string());
        let result = build_license_statement(&licenses, &logic);
        assert_eq!(result, Some("BSD-2-Clause AND MIT".to_string()));
    }

    #[test]
    fn test_build_license_statement_or() {
        let licenses = Some(vec!["MIT".to_string(), "Apache-2.0".to_string()]);
        let logic = Some("or".to_string());
        let result = build_license_statement(&licenses, &logic);
        assert_eq!(result, Some("Apache-2.0 OR MIT".to_string()));
    }

    #[test]
    fn test_build_license_statement_dual() {
        let licenses = Some(vec!["MIT".to_string(), "Apache-2.0".to_string()]);
        let logic = Some("dual".to_string());
        let result = build_license_statement(&licenses, &logic);
        assert_eq!(result, Some("Apache-2.0 OR MIT".to_string()));
    }

    #[test]
    fn test_build_license_statement_default_and() {
        let licenses = Some(vec!["MIT".to_string(), "BSD".to_string()]);
        let logic = None;
        let result = build_license_statement(&licenses, &logic);
        assert_eq!(result, Some("BSD AND MIT".to_string()));
    }

    #[test]
    fn test_build_license_statement_unknown_defaults_to_and() {
        let licenses = Some(vec!["MIT".to_string(), "BSD".to_string()]);
        let logic = Some("unknown".to_string());
        let result = build_license_statement(&licenses, &logic);
        assert_eq!(result, Some("BSD AND MIT".to_string()));
    }

    #[test]
    fn test_build_license_statement_empty_licenses() {
        let licenses = Some(vec![]);
        let logic = Some("and".to_string());
        let result = build_license_statement(&licenses, &logic);
        assert_eq!(result, None);
    }

    #[test]
    fn test_build_license_statement_no_licenses() {
        let licenses = None;
        let logic = Some("and".to_string());
        let result = build_license_statement(&licenses, &logic);
        assert_eq!(result, None);
    }

    #[test]
    fn test_build_license_statement_filters_empty() {
        let licenses = Some(vec!["MIT".to_string(), "".to_string(), "  ".to_string()]);
        let logic = Some("and".to_string());
        let result = build_license_statement(&licenses, &logic);
        assert_eq!(result, Some("MIT".to_string()));
    }

    #[test]
    fn test_build_license_statement_trims_whitespace() {
        let licenses = Some(vec!["  MIT  ".to_string(), " Apache-2.0 ".to_string()]);
        let logic = Some("or".to_string());
        let result = build_license_statement(&licenses, &logic);
        assert_eq!(result, Some("Apache-2.0 OR MIT".to_string()));
    }
}

crate::register_parser!(
    "FreeBSD +COMPACT_MANIFEST package manifest",
    &["**/*COMPACT_MANIFEST"],
    "freebsd",
    "",
    Some("https://man.freebsd.org/cgi/man.cgi?query=pkg-create"),
);
