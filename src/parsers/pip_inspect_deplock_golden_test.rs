#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::pip_inspect_deplock::PipInspectDeplockParser;
    use std::path::PathBuf;

    #[test]
    fn test_golden_pip_inspect_deplock() {
        let test_file = PathBuf::from("testdata/pip-inspect-deplock/basic/pip-inspect.deplock");
        let expected_file =
            PathBuf::from("testdata/pip-inspect-deplock/basic/pip-inspect.deplock.expected.json");
        let package_data = PipInspectDeplockParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(()) => {}
            Err(error) => panic!("Golden test failed: {}", error),
        }
    }
}
