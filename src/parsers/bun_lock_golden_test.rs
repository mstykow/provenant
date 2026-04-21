// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use std::path::PathBuf;

    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::{BunLockParser, PackageParser};

    #[test]
    fn test_golden_bun_lock_basic() {
        let test_file = PathBuf::from("testdata/bun/basic/bun.lock");
        let expected_file = PathBuf::from("testdata/bun/golden/basic-bun-lock-expected.json");

        let package_data = BunLockParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }
}
