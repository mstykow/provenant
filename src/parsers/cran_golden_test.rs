#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::cran::CranParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;

    use std::path::PathBuf;

    /// Helper function to run golden tests.
    ///
    /// Compares parsed output against expected JSON files.
    fn run_golden(test_file: &str, expected_file: &str) {
        let test_path = PathBuf::from(test_file);
        let package_data = CranParser::extract_first_package(&test_path);
        match compare_package_data_parser_only(&package_data, &PathBuf::from(expected_file)) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_geometry() {
        run_golden(
            "testdata/cran/geometry/DESCRIPTION",
            "testdata/cran/geometry/expected.json",
        );
    }

    #[test]
    fn test_golden_codetools() {
        run_golden(
            "testdata/cran/codetools/DESCRIPTION",
            "testdata/cran/codetools/expected.json",
        );
    }

    #[test]
    fn test_golden_package() {
        run_golden(
            "testdata/cran/package/DESCRIPTION",
            "testdata/cran/package/expected.json",
        );
    }
}
