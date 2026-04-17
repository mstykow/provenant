use std::path::Path;

use crate::models::{DatasourceId, PackageData, PackageType};
use crate::parsers::utils::truncate_field;

use super::utils::build_debian_purl;
use super::{PACKAGE_TYPE, default_package_data};
use crate::parsers::PackageParser;

/// Parser for Debian original source tarballs (*.orig.tar.*)
pub struct DebianOrigTarParser;

impl PackageParser for DebianOrigTarParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|name| name.contains(".orig.tar."))
            .unwrap_or(false)
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(f) => f,
            None => {
                return vec![default_package_data(
                    DatasourceId::DebianOriginalSourceTarball,
                )];
            }
        };

        vec![parse_source_tarball_filename(
            filename,
            DatasourceId::DebianOriginalSourceTarball,
        )]
    }
}

crate::register_parser!(
    "Debian original source tarball",
    &["**/*.orig.tar.*"],
    "deb",
    "",
    Some("https://www.debian.org/doc/debian-policy/ch-source.html"),
);

/// Parser for Debian source package metadata tarballs (*.debian.tar.*)
pub struct DebianDebianTarParser;

impl PackageParser for DebianDebianTarParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|name| name.contains(".debian.tar."))
            .unwrap_or(false)
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(f) => f,
            None => {
                return vec![default_package_data(
                    DatasourceId::DebianSourceMetadataTarball,
                )];
            }
        };

        vec![parse_source_tarball_filename(
            filename,
            DatasourceId::DebianSourceMetadataTarball,
        )]
    }
}

crate::register_parser!(
    "Debian source metadata tarball",
    &["**/*.debian.tar.*"],
    "deb",
    "",
    Some("https://www.debian.org/doc/debian-policy/ch-source.html"),
);

fn parse_source_tarball_filename(filename: &str, datasource_id: DatasourceId) -> PackageData {
    let without_tar_ext = filename
        .trim_end_matches(".gz")
        .trim_end_matches(".xz")
        .trim_end_matches(".bz2")
        .trim_end_matches(".tar");

    let parts: Vec<&str> = without_tar_ext.splitn(2, '_').collect();
    if parts.len() < 2 {
        return default_package_data(datasource_id);
    }

    let name = truncate_field(parts[0].to_string());
    let version_with_suffix = parts[1];

    let version = version_with_suffix
        .trim_end_matches(".orig")
        .trim_end_matches(".debian")
        .to_string();
    let version = truncate_field(version);

    let namespace = Some("debian".to_string());

    PackageData {
        datasource_id: Some(datasource_id),
        package_type: Some(PACKAGE_TYPE),
        namespace: namespace.clone(),
        name: Some(name.clone()),
        version: Some(version.clone()),
        purl: build_debian_purl(&name, Some(&version), namespace.as_deref(), None),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DatasourceId;
    use std::path::PathBuf;

    #[test]
    fn test_orig_tar_parser_is_match() {
        assert!(DebianOrigTarParser::is_match(&PathBuf::from(
            "package_1.0.orig.tar.gz"
        )));
        assert!(DebianOrigTarParser::is_match(&PathBuf::from(
            "abseil_0~20200923.3.orig.tar.xz"
        )));
        assert!(!DebianOrigTarParser::is_match(&PathBuf::from(
            "package.debian.tar.gz"
        )));
        assert!(!DebianOrigTarParser::is_match(&PathBuf::from("control")));
    }

    #[test]
    fn test_debian_tar_parser_is_match() {
        assert!(DebianDebianTarParser::is_match(&PathBuf::from(
            "package_1.0-1.debian.tar.xz"
        )));
        assert!(DebianDebianTarParser::is_match(&PathBuf::from(
            "abseil_20220623.1-1.debian.tar.gz"
        )));
        assert!(!DebianDebianTarParser::is_match(&PathBuf::from(
            "package.orig.tar.gz"
        )));
        assert!(!DebianDebianTarParser::is_match(&PathBuf::from("control")));
    }

    #[test]
    fn test_parse_orig_tar_filename() {
        let pkg = parse_source_tarball_filename(
            "abseil_0~20200923.3.orig.tar.gz",
            DatasourceId::DebianOriginalSourceTarball,
        );
        assert_eq!(pkg.name, Some("abseil".to_string()));
        assert_eq!(pkg.version, Some("0~20200923.3".to_string()));
        assert_eq!(pkg.namespace, Some("debian".to_string()));
        assert_eq!(
            pkg.purl,
            Some("pkg:deb/debian/abseil@0~20200923.3".to_string())
        );
        assert_eq!(
            pkg.datasource_id,
            Some(DatasourceId::DebianOriginalSourceTarball)
        );
    }

    #[test]
    fn test_parse_debian_tar_filename() {
        let pkg = parse_source_tarball_filename(
            "abseil_20220623.1-1.debian.tar.xz",
            DatasourceId::DebianSourceMetadataTarball,
        );
        assert_eq!(pkg.name, Some("abseil".to_string()));
        assert_eq!(pkg.version, Some("20220623.1-1".to_string()));
        assert_eq!(pkg.namespace, Some("debian".to_string()));
        assert_eq!(
            pkg.purl,
            Some("pkg:deb/debian/abseil@20220623.1-1".to_string())
        );
    }

    #[test]
    fn test_parse_source_tarball_various_compressions() {
        let pkg_gz = parse_source_tarball_filename(
            "test_1.0.orig.tar.gz",
            DatasourceId::DebianOriginalSourceTarball,
        );
        let pkg_xz = parse_source_tarball_filename(
            "test_1.0.orig.tar.xz",
            DatasourceId::DebianOriginalSourceTarball,
        );
        let pkg_bz2 = parse_source_tarball_filename(
            "test_1.0.orig.tar.bz2",
            DatasourceId::DebianOriginalSourceTarball,
        );

        assert_eq!(pkg_gz.version, Some("1.0".to_string()));
        assert_eq!(pkg_xz.version, Some("1.0".to_string()));
        assert_eq!(pkg_bz2.version, Some("1.0".to_string()));
    }

    #[test]
    fn test_parse_source_tarball_invalid_format() {
        let pkg = parse_source_tarball_filename(
            "invalid-no-underscore.tar.gz",
            DatasourceId::DebianOriginalSourceTarball,
        );
        assert!(pkg.name.is_none());
        assert!(pkg.version.is_none());
    }
}
