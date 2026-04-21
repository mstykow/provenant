// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::about::AboutFileParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use std::path::PathBuf;

    fn run_golden(test_file: &str, expected_file: &str) {
        let test_path = PathBuf::from(test_file);
        let expected_path = PathBuf::from(expected_file);

        let package_data = AboutFileParser::extract_first_package(&test_path);

        match compare_package_data_parser_only(&package_data, &expected_path) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_apipkg_about() {
        run_golden(
            "testdata/about/apipkg.ABOUT",
            "testdata/about/apipkg.ABOUT.expected.json",
        );
    }

    #[test]
    fn test_golden_appdirs_about() {
        run_golden(
            "testdata/about/appdirs.ABOUT",
            "testdata/about/appdirs.ABOUT.expected.json",
        );
    }
}
