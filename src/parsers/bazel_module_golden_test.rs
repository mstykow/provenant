// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use std::path::PathBuf;

    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::{BazelModuleParser, PackageParser};

    fn run_golden(test_file: &str, expected_file: &str) {
        let package_data = BazelModuleParser::extract_first_package(&PathBuf::from(test_file));
        match compare_package_data_parser_only(&package_data, &PathBuf::from(expected_file)) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_bazel_module_basic() {
        run_golden(
            "testdata/bazel-golden/module/MODULE.bazel",
            "testdata/bazel-golden/module/MODULE.bazel-expected.json",
        );
    }

    #[test]
    fn test_golden_bazel_module_no_version() {
        run_golden(
            "testdata/bazel-golden/module/MODULE_no_version.bazel",
            "testdata/bazel-golden/module/MODULE_no_version.bazel-expected.json",
        );
    }
}
