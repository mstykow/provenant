#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::bower::BowerJsonParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use std::path::PathBuf;

    fn run_golden(test_file: &str, expected_file: &str) {
        let package_data = BowerJsonParser::extract_first_package(&PathBuf::from(test_file));
        match compare_package_data_parser_only(&package_data, &PathBuf::from(expected_file)) {
            Ok(()) => {}
            Err(error) => panic!("Golden test failed for {}: {}", test_file, error),
        }
    }

    #[test]
    fn test_golden_bower_basic() {
        run_golden(
            "testdata/bower/basic/bower.json",
            "testdata/bower/basic/bower.json.expected.json",
        );
    }

    #[test]
    fn test_golden_bower_author_objects() {
        run_golden(
            "testdata/bower/author-objects/bower.json",
            "testdata/bower/author-objects/bower.json.expected.json",
        );
    }

    #[test]
    fn test_golden_bower_license_list() {
        run_golden(
            "testdata/bower/list-of-licenses/bower.json",
            "testdata/bower/list-of-licenses/bower.json.expected.json",
        );
    }
}
