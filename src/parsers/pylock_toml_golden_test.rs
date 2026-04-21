// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use std::path::PathBuf;

    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::{PackageParser, PylockTomlParser};

    #[test]
    fn test_parse_pylock_toml_golden() {
        let test_file = PathBuf::from("testdata/python/golden/pylock_toml/pylock.toml");
        let expected_file =
            PathBuf::from("testdata/python/golden/pylock_toml/pylock.toml-expected.json");

        let package_data = PylockTomlParser::extract_first_package(&test_file);

        assert_eq!(package_data.dependencies.len(), 7);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }
}
