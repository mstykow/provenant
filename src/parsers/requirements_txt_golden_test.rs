// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::{PackageParser, RequirementsTxtParser};
    use std::path::PathBuf;

    #[test]
    fn test_parse_requirements_txt_basic_golden() {
        let test_file =
            PathBuf::from("testdata/python/golden/requirements_txt/basic-requirements.txt");
        let expected_file =
            PathBuf::from("testdata/python/golden/requirements_txt/basic-expected.json");

        let package_data = RequirementsTxtParser::extract_first_package(&test_file);

        assert_eq!(package_data.dependencies.len(), 3);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_parse_requirements_txt_complex_golden() {
        let test_file =
            PathBuf::from("testdata/python/golden/requirements_txt/complex-requirements.txt");
        let expected_file =
            PathBuf::from("testdata/python/golden/requirements_txt/complex-expected.json");

        let package_data = RequirementsTxtParser::extract_first_package(&test_file);

        assert_eq!(package_data.dependencies.len(), 4);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_parse_requirements_txt_lockfile_style_golden() {
        let test_file =
            PathBuf::from("testdata/python/golden/requirements_txt/requirements_lock_3_11.txt");
        let expected_file = PathBuf::from(
            "testdata/python/golden/requirements_txt/requirements_lock_3_11-expected.json",
        );

        let package_data = RequirementsTxtParser::extract_first_package(&test_file);

        assert_eq!(package_data.dependencies.len(), 5);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }
}
