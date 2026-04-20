#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::carthage::{CarthageCartfileParser, CarthageCartfileResolvedParser};
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use std::path::Path;
    use std::path::PathBuf;

    fn assert_fixture_exists(path: &Path) {
        assert!(path.exists(), "missing fixture: {}", path.display());
    }

    #[test]
    fn test_golden_cartfile_basic() {
        let test_file = PathBuf::from("testdata/carthage-golden/basic/Cartfile");
        let expected_file = PathBuf::from("testdata/carthage-golden/basic/Cartfile.expected");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = CarthageCartfileParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for Cartfile basic: {}", e),
        }
    }

    #[test]
    fn test_golden_cartfile_resolved_basic() {
        let test_file = PathBuf::from("testdata/carthage-golden/basic/Cartfile.resolved");
        let expected_file =
            PathBuf::from("testdata/carthage-golden/basic/Cartfile.resolved.expected");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = CarthageCartfileResolvedParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for Cartfile.resolved basic: {}", e),
        }
    }
}
