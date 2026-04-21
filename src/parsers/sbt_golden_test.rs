// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::sbt::SbtParser;
    use std::path::PathBuf;

    fn run_golden(test_file: &str, expected_file: &str) {
        let package_data = SbtParser::extract_first_package(&PathBuf::from(test_file));
        match compare_package_data_parser_only(&package_data, &PathBuf::from(expected_file)) {
            Ok(_) => (),
            Err(error) => panic!("Golden test failed: {}", error),
        }
    }

    #[test]
    fn test_golden_literal_root_build() {
        run_golden(
            "testdata/sbt-golden/literal-root/build.sbt",
            "testdata/sbt-golden/literal-root/build.sbt-expected.json",
        );
    }

    #[test]
    fn test_golden_fallback_and_skips() {
        run_golden(
            "testdata/sbt-golden/fallback-and-skips/build.sbt",
            "testdata/sbt-golden/fallback-and-skips/build.sbt-expected.json",
        );
    }

    #[test]
    fn test_golden_config_prefixed_dependencies() {
        run_golden(
            "testdata/sbt-golden/config-prefixed-deps/build.sbt",
            "testdata/sbt-golden/config-prefixed-deps/build.sbt-expected.json",
        );
    }

    #[test]
    fn test_golden_settings_and_shared_bundles() {
        run_golden(
            "testdata/sbt-golden/settings-and-shared-bundles/build.sbt",
            "testdata/sbt-golden/settings-and-shared-bundles/build.sbt-expected.json",
        );
    }
}
