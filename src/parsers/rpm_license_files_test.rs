//! Tests for RPM license files parser.

use super::PackageParser;
use super::rpm_license_files::RpmLicenseFilesParser;
use std::path::PathBuf;

#[cfg(test)]
mod rpm_license_files_tests {
    use super::*;
    use crate::models::DatasourceId;
    use crate::models::PackageType;

    #[test]
    fn test_is_match_copying_file() {
        assert!(RpmLicenseFilesParser::is_match(&PathBuf::from(
            "/usr/share/licenses/openssl/COPYING"
        )));
        assert!(RpmLicenseFilesParser::is_match(&PathBuf::from(
            "rootfs/usr/share/licenses/glibc/COPYING"
        )));
    }

    #[test]
    fn test_is_match_license_file() {
        assert!(RpmLicenseFilesParser::is_match(&PathBuf::from(
            "/usr/share/licenses/openssl/LICENSE"
        )));
        assert!(RpmLicenseFilesParser::is_match(&PathBuf::from(
            "usr/share/licenses/zlib/LICENSE.md"
        )));
    }

    #[test]
    fn test_is_match_copying_lesser() {
        assert!(RpmLicenseFilesParser::is_match(&PathBuf::from(
            "/usr/share/licenses/glibc/COPYING.LESSER"
        )));
    }

    #[test]
    fn test_is_match_compressed_file() {
        assert!(RpmLicenseFilesParser::is_match(&PathBuf::from(
            "/usr/share/licenses/zlib/COPYING.gz"
        )));
        assert!(RpmLicenseFilesParser::is_match(&PathBuf::from(
            "/usr/share/licenses/zlib/LICENSE.txt"
        )));
    }

    #[test]
    fn test_is_match_negative_readme() {
        assert!(!RpmLicenseFilesParser::is_match(&PathBuf::from(
            "/usr/share/licenses/openssl/README"
        )));
    }

    #[test]
    fn test_is_match_negative_no_license_dir() {
        assert!(!RpmLicenseFilesParser::is_match(&PathBuf::from(
            "/usr/share/doc/openssl/LICENSE"
        )));
        assert!(!RpmLicenseFilesParser::is_match(&PathBuf::from(
            "/opt/licenses/openssl/LICENSE"
        )));
    }

    #[test]
    fn test_is_match_negative_wrong_case() {
        // The Python implementation is case-sensitive
        assert!(!RpmLicenseFilesParser::is_match(&PathBuf::from(
            "/usr/share/licenses/openssl/license"
        )));
        assert!(!RpmLicenseFilesParser::is_match(&PathBuf::from(
            "/usr/share/licenses/openssl/copying"
        )));
    }

    #[test]
    fn test_extract_packages_openssl() {
        let path = PathBuf::from("/usr/share/licenses/openssl/LICENSE");
        let packages = RpmLicenseFilesParser::extract_packages(&path);

        assert_eq!(packages.len(), 1);
        let pkg = &packages[0];

        assert_eq!(pkg.package_type, Some(PackageType::Rpm));
        assert_eq!(pkg.datasource_id, Some(DatasourceId::RpmPackageLicenses));
        assert_eq!(pkg.namespace, Some("mariner".to_string()));
        assert_eq!(pkg.name, Some("openssl".to_string()));
        assert_eq!(pkg.purl, Some("pkg:rpm/mariner/openssl".to_string()));
    }

    #[test]
    fn test_extract_packages_glibc() {
        let path = PathBuf::from("rootfs/usr/share/licenses/glibc/COPYING");
        let packages = RpmLicenseFilesParser::extract_packages(&path);

        assert_eq!(packages.len(), 1);
        let pkg = &packages[0];

        assert_eq!(pkg.package_type, Some(PackageType::Rpm));
        assert_eq!(pkg.datasource_id, Some(DatasourceId::RpmPackageLicenses));
        assert_eq!(pkg.namespace, Some("mariner".to_string()));
        assert_eq!(pkg.name, Some("glibc".to_string()));
        assert_eq!(pkg.purl, Some("pkg:rpm/mariner/glibc".to_string()));
    }

    #[test]
    fn test_extract_packages_with_subdirs() {
        let path = PathBuf::from("/var/rootfs/usr/share/licenses/zlib/LICENSE.md");
        let packages = RpmLicenseFilesParser::extract_packages(&path);

        assert_eq!(packages.len(), 1);
        let pkg = &packages[0];

        assert_eq!(pkg.name, Some("zlib".to_string()));
        assert_eq!(pkg.purl, Some("pkg:rpm/mariner/zlib".to_string()));
    }

    #[test]
    fn test_extract_from_testdata() {
        let test_files = vec![
            "testdata/rpm/licenses/usr/share/licenses/openssl/LICENSE",
            "testdata/rpm/licenses/usr/share/licenses/glibc/COPYING",
        ];

        for test_file in test_files {
            let path = PathBuf::from(test_file);
            assert!(path.exists(), "missing fixture: {}", path.display());

            let packages = RpmLicenseFilesParser::extract_packages(&path);
            assert_eq!(packages.len(), 1);

            let pkg = &packages[0];
            assert_eq!(pkg.package_type, Some(PackageType::Rpm));
            assert_eq!(pkg.datasource_id, Some(DatasourceId::RpmPackageLicenses));
            assert_eq!(pkg.namespace, Some("mariner".to_string()));
            assert!(pkg.name.is_some());
            assert!(pkg.purl.is_some());
        }
    }
}
