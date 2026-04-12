#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::autotools::AutotoolsConfigureParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use std::path::PathBuf;

    fn run_golden(test_file: &str, expected_file: &str) {
        let package_data =
            AutotoolsConfigureParser::extract_first_package(&PathBuf::from(test_file));
        match compare_package_data_parser_only(&package_data, &PathBuf::from(expected_file)) {
            Ok(()) => {}
            Err(error) => panic!("Golden test failed for {}: {}", test_file, error),
        }
    }

    #[test]
    fn test_golden_autotools_configure() {
        run_golden(
            "testdata/autotools/myproject/configure",
            "testdata/autotools/myproject/configure.expected.json",
        );
    }

    #[test]
    fn test_golden_autotools_configure_ac() {
        run_golden(
            "testdata/autotools/another-project/configure.ac",
            "testdata/autotools/another-project/configure.ac.expected.json",
        );
    }

    #[test]
    fn test_golden_autotools_root_configure() {
        run_golden(
            "configure",
            "testdata/autotools/root-configure.expected.json",
        );
    }
}
