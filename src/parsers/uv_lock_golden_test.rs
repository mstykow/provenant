// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use std::path::PathBuf;

    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::{PackageParser, UvLockParser};

    #[test]
    fn test_parse_uv_lock_golden() {
        let test_file = PathBuf::from("testdata/python/golden/uv_lock/uv.lock");
        let expected_file = PathBuf::from("testdata/python/golden/uv_lock/uv.lock-expected.json");

        let package_data = UvLockParser::extract_first_package(&test_file);

        assert_eq!(package_data.name.as_deref(), Some("uv-demo"));
        assert_eq!(package_data.dependencies.len(), 4);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }
}
