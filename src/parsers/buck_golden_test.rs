// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::buck::{BuckBuildParser, BuckMetadataBzlParser};
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use std::path::PathBuf;

    fn run_build_golden(test_file: &str, expected_file: &str) {
        let package_data = BuckBuildParser::extract_first_package(&PathBuf::from(test_file));
        match compare_package_data_parser_only(&package_data, &PathBuf::from(expected_file)) {
            Ok(()) => {}
            Err(error) => panic!("Golden build test failed for {}: {}", test_file, error),
        }
    }

    fn run_metadata_golden(test_file: &str, expected_file: &str) {
        let package_data = BuckMetadataBzlParser::extract_first_package(&PathBuf::from(test_file));
        match compare_package_data_parser_only(&package_data, &PathBuf::from(expected_file)) {
            Ok(()) => {}
            Err(error) => panic!("Golden metadata test failed for {}: {}", test_file, error),
        }
    }

    #[test]
    fn test_golden_buck_build_parse() {
        run_build_golden(
            "testdata/buck/parse/BUCK",
            "testdata/buck/parse/BUCK.expected.json",
        );
    }

    #[test]
    fn test_golden_buck_build_fallback() {
        run_build_golden(
            "testdata/buck/end2end/subdir2/BUCK",
            "testdata/buck/end2end/subdir2/BUCK.expected.json",
        );
    }

    #[test]
    fn test_golden_buck_build_oss_guarded() {
        run_build_golden(
            "testdata/buck/oss-guarded/BUCK",
            "testdata/buck/oss-guarded/BUCK.expected.json",
        );
    }

    #[test]
    fn test_golden_buck_metadata_basic() {
        run_metadata_golden(
            "testdata/buck/metadata/METADATA.bzl",
            "testdata/buck/metadata/METADATA.bzl.expected.json",
        );
    }

    #[test]
    fn test_golden_buck_metadata_package_url() {
        run_metadata_golden(
            "testdata/buck/metadata/with-package-url-METADATA.bzl",
            "testdata/buck/metadata/with-package-url-METADATA.bzl.expected.json",
        );
    }
}
