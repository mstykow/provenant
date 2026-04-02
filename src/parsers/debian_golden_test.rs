#[cfg(test)]
mod golden_tests {
    use crate::models::PackageData;
    use crate::parsers::PackageParser;
    use crate::parsers::debian::*;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use std::path::Path;
    use std::path::PathBuf;

    fn assert_fixture_exists(path: &Path) {
        assert!(path.exists(), "missing fixture: {}", path.display());
    }

    fn compare_debian_package_data(actual: &PackageData, expected_file: &Path) {
        match compare_package_data_parser_only(actual, expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Debian golden mismatch: {}", e),
        }
    }

    #[test]
    fn test_golden_deb_archive_extraction() {
        let test_file = PathBuf::from("testdata/debian/deb/adduser_3.112ubuntu1_all.deb");
        let expected_file =
            PathBuf::from("testdata/debian/deb/adduser_3.112ubuntu1_all.deb.expected.json");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = DebianDebParser::extract_first_package(&test_file);

        compare_debian_package_data(&package_data, &expected_file);
    }

    #[test]
    fn test_golden_dsc_file() {
        let test_file = PathBuf::from("testdata/debian/dsc_files/zsh_5.7.1-1+deb10u1.dsc");
        let expected_file =
            PathBuf::from("testdata/debian/dsc_files/zsh_5.7.1-1+deb10u1.dsc.expected.json");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = DebianDscParser::extract_first_package(&test_file);

        compare_debian_package_data(&package_data, &expected_file);
    }

    #[test]
    fn test_golden_copyright_file() {
        let test_file = PathBuf::from("testdata/debian/copyright/copyright");
        let expected_file = PathBuf::from("testdata/debian/copyright/copyright.expected.json");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = DebianCopyrightParser::extract_first_package(&test_file);
        compare_debian_package_data(&package_data, &expected_file);
    }

    #[test]
    fn test_golden_debian_control() {
        let test_file = PathBuf::from("testdata/debian/project/debian/control");
        let expected_file = PathBuf::from("testdata/debian/project/debian/control.expected.json");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = DebianControlParser::extract_first_package(&test_file);
        compare_debian_package_data(&package_data, &expected_file);
    }

    #[test]
    fn test_golden_debian_installed_status() {
        let test_file = PathBuf::from("testdata/debian/var/lib/dpkg/status");
        let expected_file = PathBuf::from("testdata/debian/var/lib/dpkg/status.expected.json");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = DebianInstalledParser::extract_first_package(&test_file);
        compare_debian_package_data(&package_data, &expected_file);
    }

    #[test]
    fn test_golden_debian_distroless_installed() {
        let test_file = PathBuf::from("testdata/debian/var/lib/dpkg/status.d/base-files");
        let expected_file =
            PathBuf::from("testdata/debian/var/lib/dpkg/status.d/base-files.expected.json");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = DebianDistrolessInstalledParser::extract_first_package(&test_file);
        compare_debian_package_data(&package_data, &expected_file);
    }

    #[test]
    fn test_golden_debian_orig_tar() {
        let test_file = PathBuf::from("testdata/debian/example_1.0.orig.tar.gz");
        let expected_file = PathBuf::from("testdata/debian/example_1.0.orig.tar.gz.expected.json");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = DebianOrigTarParser::extract_first_package(&test_file);
        compare_debian_package_data(&package_data, &expected_file);
    }

    #[test]
    fn test_golden_debian_debian_tar() {
        let test_file = PathBuf::from("testdata/debian/example_1.0.debian.tar.xz");
        let expected_file =
            PathBuf::from("testdata/debian/example_1.0.debian.tar.xz.expected.json");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = DebianDebianTarParser::extract_first_package(&test_file);
        compare_debian_package_data(&package_data, &expected_file);
    }

    #[test]
    fn test_golden_debian_installed_list() {
        let test_file = PathBuf::from("testdata/debian/var/lib/dpkg/info/bash.list");
        let expected_file =
            PathBuf::from("testdata/debian/var/lib/dpkg/info/bash.list.expected.json");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = DebianInstalledListParser::extract_first_package(&test_file);
        compare_debian_package_data(&package_data, &expected_file);
    }

    #[test]
    fn test_golden_debian_installed_md5sums() {
        let test_file = PathBuf::from("testdata/debian/var/lib/dpkg/info/bash.md5sums");
        let expected_file =
            PathBuf::from("testdata/debian/var/lib/dpkg/info/bash.md5sums.expected.json");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = DebianInstalledMd5sumsParser::extract_first_package(&test_file);
        compare_debian_package_data(&package_data, &expected_file);
    }

    #[test]
    fn test_golden_debian_control_in_extracted_deb() {
        let test_file = PathBuf::from(
            "testdata/debian/extracted-md5sums/example_1.0-1_amd64.deb-extract/control.tar.gz-extract/control",
        );
        let expected_file = PathBuf::from(
            "testdata/debian/extracted-md5sums/example_1.0-1_amd64.deb-extract/control.tar.gz-extract/control.expected.json",
        );

        let package_data = DebianControlInExtractedDebParser::extract_first_package(&test_file);
        compare_debian_package_data(&package_data, &expected_file);
    }

    #[test]
    fn test_golden_debian_md5sum_in_package() {
        let test_file = PathBuf::from(
            "testdata/debian/extracted-md5sums/example_1.0-1_amd64.deb-extract/control.tar.gz-extract/md5sums",
        );
        let expected_file = PathBuf::from(
            "testdata/debian/extracted-md5sums/example_1.0-1_amd64.deb-extract/control.tar.gz-extract/md5sums.expected.json",
        );

        let package_data = DebianMd5sumInPackageParser::extract_first_package(&test_file);
        compare_debian_package_data(&package_data, &expected_file);
    }
}
