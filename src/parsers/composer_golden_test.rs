// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::composer::{ComposerJsonParser, ComposerLockParser};
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use std::path::PathBuf;

    #[test]
    fn test_golden_composer_lock() {
        let test_file = PathBuf::from("testdata/composer-golden/composer-lock/composer.lock");
        let expected_file =
            PathBuf::from("testdata/composer-golden/composer-lock/composer.lock.expected");

        let package_data = ComposerLockParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_composer_json_a_timer() {
        let test_file = PathBuf::from("testdata/composer-golden/a-timer/composer.json");
        let expected_file =
            PathBuf::from("testdata/composer-golden/a-timer/composer.json.expected");

        let package_data = ComposerJsonParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }
}
