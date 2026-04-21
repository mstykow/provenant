// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::hex_lock::HexLockParser;
    use std::path::PathBuf;

    #[test]
    fn test_golden_hex_mix_lock_basic() {
        let test_file = PathBuf::from("testdata/hex/basic/mix.lock");
        let expected_file = PathBuf::from("testdata/hex/golden/mix.lock.expected.json");

        let package_data = HexLockParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }
}
