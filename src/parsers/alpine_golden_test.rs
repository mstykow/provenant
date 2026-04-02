#[cfg(test)]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::alpine::*;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use std::path::Path;
    use std::path::PathBuf;

    fn assert_fixture_exists(path: &Path) {
        assert!(path.exists(), "missing fixture: {}", path.display());
    }

    #[test]
    fn test_golden_alpine_installed_db() {
        let test_file = PathBuf::from("testdata/alpine/lib/apk/db/installed");
        let expected_file = PathBuf::from("testdata/alpine/lib/apk/db/installed.expected.json");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = AlpineInstalledParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for Alpine installed DB: {}", e),
        }
    }

    #[test]
    fn test_golden_alpine_apkbuild_icu() {
        let test_file = PathBuf::from("testdata/alpine/apkbuild/icu/APKBUILD");
        let expected_file = PathBuf::from("testdata/alpine/apkbuild/icu/APKBUILD.expected.json");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = AlpineApkbuildParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for Alpine APKBUILD icu: {}", e),
        }
    }

    #[test]
    fn test_golden_alpine_apkbuild_linux_firmware() {
        let test_file = PathBuf::from("testdata/alpine/apkbuild/linux-firmware/APKBUILD");
        let expected_file =
            PathBuf::from("testdata/alpine/apkbuild/linux-firmware/APKBUILD.expected.json");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = AlpineApkbuildParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!(
                "Golden test failed for Alpine APKBUILD linux-firmware: {}",
                e
            ),
        }
    }

    #[test]
    fn test_golden_alpine_apk_archive() {
        let test_file = PathBuf::from("testdata/alpine/apk/basic/test-package-1.0-r0.apk");
        let expected_file =
            PathBuf::from("testdata/alpine/apk/basic/test-package-1.0-r0.apk.expected.json");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = AlpineApkParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for Alpine APK archive: {}", e),
        }
    }
}
