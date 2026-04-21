// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::meson::MesonParser;
    use std::path::PathBuf;

    fn run_golden(test_file: &str, expected_file: &str) {
        let package_data = MesonParser::extract_first_package(&PathBuf::from(test_file));
        match compare_package_data_parser_only(&package_data, &PathBuf::from(expected_file)) {
            Ok(_) => (),
            Err(error) => panic!("Golden test failed: {}", error),
        }
    }

    #[test]
    fn test_golden_literal_root_build() {
        run_golden(
            "testdata/meson-golden/literal-root/meson.build",
            "testdata/meson-golden/literal-root/meson.build-expected.json",
        );
    }

    #[test]
    fn test_golden_guardrails_and_skips() {
        run_golden(
            "testdata/meson-golden/guardrails-and-skips/meson.build",
            "testdata/meson-golden/guardrails-and-skips/meson.build-expected.json",
        );
    }
}
