#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::opam::OpamParser;
    use std::path::PathBuf;

    /// Helper function to run golden tests.
    ///
    /// Compares parsed output against expected JSON files.
    fn run_golden(test_file: &str, expected_file: &str) {
        let test_path = PathBuf::from(test_file);
        let package_data = OpamParser::extract_first_package(&test_path);

        match compare_package_data_parser_only(&package_data, &PathBuf::from(expected_file)) {
            Ok(()) => {}
            Err(e) => panic!("Golden test failed for {}: {}", test_file, e),
        }
    }

    #[test]
    fn test_golden_sample1() {
        run_golden(
            "testdata/opam/sample1/sample1.opam",
            "testdata/opam/sample1/output.opam.expected",
        );
    }

    #[test]
    fn test_golden_sample2() {
        run_golden(
            "testdata/opam/sample2/sample2.opam",
            "testdata/opam/sample2/output.opam.expected",
        );
    }

    #[test]
    fn test_golden_sample3() {
        run_golden(
            "testdata/opam/sample3/sample3.opam",
            "testdata/opam/sample3/output.opam.expected",
        );
    }

    #[test]
    fn test_golden_sample4() {
        run_golden(
            "testdata/opam/sample4/opam",
            "testdata/opam/sample4/output.opam.expected",
        );
    }

    #[test]
    fn test_golden_sample5() {
        run_golden(
            "testdata/opam/sample5/opam",
            "testdata/opam/sample5/output.opam.expected",
        );
    }

    #[test]
    fn test_golden_sample6() {
        run_golden(
            "testdata/opam/sample6/sample6.opam",
            "testdata/opam/sample6/output.opam.expected",
        );
    }

    #[test]
    fn test_golden_sample7() {
        run_golden(
            "testdata/opam/sample7/sample7.opam",
            "testdata/opam/sample7/output.opam.expected",
        );
    }

    #[test]
    fn test_golden_sample8() {
        run_golden(
            "testdata/opam/sample8/opam",
            "testdata/opam/sample8/output.opam.expected",
        );
    }
}
