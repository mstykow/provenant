// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::bazel::BazelBuildParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use std::path::PathBuf;

    fn run_golden(test_file: &str, expected_file: &str) {
        let package_data = BazelBuildParser::extract_first_package(&PathBuf::from(test_file));
        match compare_package_data_parser_only(&package_data, &PathBuf::from(expected_file)) {
            Ok(()) => {}
            Err(error) => panic!("Golden test failed for {}: {}", test_file, error),
        }
    }

    #[test]
    fn test_golden_bazel_build_parse() {
        run_golden(
            "testdata/bazel/parse/BUILD",
            "testdata/bazel/parse/BUILD.expected.json",
        );
    }

    #[test]
    fn test_golden_bazel_build_fallback() {
        run_golden(
            "testdata/bazel/end2end/subdir2/BUILD",
            "testdata/bazel/end2end/subdir2/BUILD.expected.json",
        );
    }
}
