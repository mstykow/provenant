// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use super::super::deb::extract_package_name_from_deb_path;
    use crate::models::DatasourceId;
    use crate::models::Md5Digest;
    use crate::models::PackageType;
    use crate::parsers::PackageParser;
    use crate::parsers::debian::*;
    use std::path::PathBuf;

    #[test]
    fn test_debian_md5sum_in_package_is_match_control_tar_xz() {
        let path = PathBuf::from(
            "testdata/debian/extracted-md5sums/testpkg_2.5-3_i386.deb-extract/control.tar.xz-extract/md5sums",
        );
        assert!(DebianMd5sumInPackageParser::is_match(&path));
    }

    #[test]
    fn test_debian_md5sum_in_package_is_not_match_wrong_path() {
        let path = PathBuf::from("testdata/debian/var/lib/dpkg/info/bash.md5sums");
        assert!(!DebianMd5sumInPackageParser::is_match(&path));
    }

    #[test]
    fn test_debian_md5sum_in_package_is_not_match_wrong_filename() {
        let path = PathBuf::from(
            "testdata/debian/extracted-md5sums/example_1.0-1_amd64.deb-extract/control.tar.gz-extract/control",
        );
        assert!(!DebianMd5sumInPackageParser::is_match(&path));
    }

    #[test]
    fn test_debian_md5sum_in_package_extract_from_control_tar_gz() {
        let path = PathBuf::from(
            "testdata/debian/extracted-md5sums/example_1.0-1_amd64.deb-extract/control.tar.gz-extract/md5sums",
        );
        assert!(path.exists(), "missing fixture: {}", path.display());

        let packages = DebianMd5sumInPackageParser::extract_packages(&path);
        assert_eq!(packages.len(), 1);

        let package = &packages[0];
        assert_eq!(package.package_type, Some(PackageType::Deb));
        assert_eq!(
            package.datasource_id,
            Some(DatasourceId::DebianMd5SumsInExtractedDeb)
        );
        assert_eq!(package.name, Some("example".to_string()));
        assert_eq!(package.namespace, Some("debian".to_string()));
        assert_eq!(package.purl, Some("pkg:deb/debian/example".to_string()));

        assert_eq!(package.file_references.len(), 4);

        let first_ref = &package.file_references[0];
        assert_eq!(first_ref.path, "usr/bin/example");
        assert_eq!(
            first_ref.md5,
            Some(Md5Digest::from_hex("d41d8cd98f00b204e9800998ecf8427e").unwrap())
        );
        assert!(first_ref.sha1.is_none());
        assert!(first_ref.sha256.is_none());

        let last_ref = &package.file_references[3];
        assert_eq!(last_ref.path, "usr/share/man/man1/example.1.gz");
        assert_eq!(
            last_ref.md5,
            Some(Md5Digest::from_hex("9e107d9d372bb6826bd81d3542a419d6").unwrap())
        );
    }

    #[test]
    fn test_debian_md5sum_in_package_extract_from_control_tar_xz() {
        let path = PathBuf::from(
            "testdata/debian/extracted-md5sums/testpkg_2.5-3_i386.deb-extract/control.tar.xz-extract/md5sums",
        );
        assert!(path.exists(), "missing fixture: {}", path.display());

        let packages = DebianMd5sumInPackageParser::extract_packages(&path);
        assert_eq!(packages.len(), 1);

        let package = &packages[0];
        assert_eq!(package.package_type, Some(PackageType::Deb));
        assert_eq!(
            package.datasource_id,
            Some(DatasourceId::DebianMd5SumsInExtractedDeb)
        );
        assert_eq!(package.name, Some("testpkg".to_string()));
        assert_eq!(package.namespace, Some("debian".to_string()));
        assert_eq!(package.purl, Some("pkg:deb/debian/testpkg".to_string()));

        assert_eq!(package.file_references.len(), 3);

        let first_ref = &package.file_references[0];
        assert_eq!(first_ref.path, "usr/bin/testapp");
        assert_eq!(
            first_ref.md5,
            Some(Md5Digest::from_hex("5f4dcc3b5aa765d61d8327deb882cf99").unwrap())
        );

        let last_ref = &package.file_references[2];
        assert_eq!(last_ref.path, "usr/lib/testpkg/libtest.so");
        assert_eq!(
            last_ref.md5,
            Some(Md5Digest::from_hex("ad0234829205b9033196ba818f7a872b").unwrap())
        );
    }

    // ====== DebianControlInExtractedDebParser tests ======

    #[test]
    fn test_debian_control_in_extracted_deb_is_match_gz() {
        let path = PathBuf::from(
            "testdata/debian/extracted-md5sums/example_1.0-1_amd64.deb-extract/control.tar.gz-extract/control",
        );
        assert!(DebianControlInExtractedDebParser::is_match(&path));
    }

    #[test]
    fn test_debian_control_in_extracted_deb_is_match_xz() {
        let path = PathBuf::from(
            "testdata/debian/extracted-md5sums/testpkg_2.5-3_i386.deb-extract/control.tar.xz-extract/control",
        );
        assert!(DebianControlInExtractedDebParser::is_match(&path));
    }

    #[test]
    fn test_debian_control_in_extracted_deb_not_match_debian_control() {
        let path = PathBuf::from("debian/control");
        assert!(!DebianControlInExtractedDebParser::is_match(&path));
    }

    #[test]
    fn test_debian_control_in_extracted_deb_not_match_md5sums() {
        let path = PathBuf::from("example_1.0-1_amd64.deb-extract/control.tar.gz-extract/md5sums");
        assert!(!DebianControlInExtractedDebParser::is_match(&path));
    }

    #[test]
    fn test_debian_control_in_extracted_deb_not_match_plain_control() {
        let path = PathBuf::from("/some/path/control");
        assert!(!DebianControlInExtractedDebParser::is_match(&path));
    }

    #[test]
    fn test_debian_control_in_extracted_deb_extract_gz() {
        let path = PathBuf::from(
            "testdata/debian/extracted-md5sums/example_1.0-1_amd64.deb-extract/control.tar.gz-extract/control",
        );
        assert!(path.exists(), "missing fixture: {}", path.display());

        let packages = DebianControlInExtractedDebParser::extract_packages(&path);
        assert_eq!(packages.len(), 1);

        let pkg = &packages[0];
        assert_eq!(pkg.package_type, Some(PackageType::Deb));
        assert_eq!(
            pkg.datasource_id,
            Some(DatasourceId::DebianControlExtractedDeb)
        );
        assert_eq!(pkg.name, Some("example".to_string()));
        assert_eq!(pkg.version, Some("1.0-1".to_string()));
        assert_eq!(pkg.namespace, Some("debian".to_string()));
        assert_eq!(pkg.homepage_url, Some("https://example.com".to_string()));
        assert!(pkg.purl.is_some());
        assert!(pkg.purl.as_ref().unwrap().contains("example"));
        assert!(pkg.purl.as_ref().unwrap().contains("1.0-1"));

        assert_eq!(pkg.parties.len(), 1);
        assert_eq!(pkg.parties[0].role, Some("maintainer".to_string()));
        assert_eq!(pkg.parties[0].name, Some("Example Developer".to_string()));

        assert!(!pkg.dependencies.is_empty());
    }

    #[test]
    fn test_debian_control_in_extracted_deb_extract_xz() {
        let path = PathBuf::from(
            "testdata/debian/extracted-md5sums/testpkg_2.5-3_i386.deb-extract/control.tar.xz-extract/control",
        );
        assert!(path.exists(), "missing fixture: {}", path.display());

        let packages = DebianControlInExtractedDebParser::extract_packages(&path);
        assert_eq!(packages.len(), 1);

        let pkg = &packages[0];
        assert_eq!(pkg.package_type, Some(PackageType::Deb));
        assert_eq!(
            pkg.datasource_id,
            Some(DatasourceId::DebianControlExtractedDeb)
        );
        assert_eq!(pkg.name, Some("testpkg".to_string()));
        assert_eq!(pkg.version, Some("2.5-3".to_string()));
        assert_eq!(pkg.namespace, Some("debian".to_string()));

        assert_eq!(pkg.parties.len(), 1);
        assert_eq!(pkg.parties[0].name, Some("Test Maintainer".to_string()));

        assert!(!pkg.dependencies.is_empty());
        let dep_purls: Vec<&str> = pkg
            .dependencies
            .iter()
            .filter_map(|d| d.purl.as_deref())
            .collect();
        assert!(dep_purls.iter().any(|p| p.contains("libc6")));
    }

    #[test]
    fn test_extract_package_name_from_deb_path() {
        let path1 = PathBuf::from("example_1.0-1_amd64.deb-extract/control.tar.gz-extract/md5sums");
        let name1 = extract_package_name_from_deb_path(&path1);
        assert_eq!(name1, Some("example".to_string()));

        let path2 = PathBuf::from("testpkg_2.5-3_i386.deb-extract/control.tar.xz-extract/md5sums");
        let name2 = extract_package_name_from_deb_path(&path2);
        assert_eq!(name2, Some("testpkg".to_string()));

        let path3 = PathBuf::from(
            "complex-name_1.2.3-4ubuntu1_amd64.deb-extract/control.tar.gz-extract/md5sums",
        );
        let name3 = extract_package_name_from_deb_path(&path3);
        assert_eq!(name3, Some("complex-name".to_string()));
    }

    #[test]
    fn test_debian_control_missing_file_preserves_datasource() {
        let path = PathBuf::from("/nonexistent/debian/control");
        let package = DebianControlParser::extract_first_package(&path);

        assert_eq!(package.package_type, Some(PackageType::Deb));
        assert_eq!(
            package.datasource_id,
            Some(DatasourceId::DebianControlInSource)
        );
    }

    #[test]
    fn test_debian_installed_missing_file_preserves_datasource() {
        let path = PathBuf::from("/nonexistent/var/lib/dpkg/status");
        let package = DebianInstalledParser::extract_first_package(&path);

        assert_eq!(package.package_type, Some(PackageType::Deb));
        assert_eq!(
            package.datasource_id,
            Some(DatasourceId::DebianInstalledStatusDb)
        );
    }
}
