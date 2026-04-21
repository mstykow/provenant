// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use base64::Engine;
    use std::path::PathBuf;

    use crate::parsers::PackageParser;
    use crate::parsers::bun_lockb::BunLockbParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use tempfile::TempDir;

    fn decode_legacy_no_scripts_fixture() -> Vec<u8> {
        let fixture = PathBuf::from("testdata/bun/legacy/bun.lockb.v2-no-scripts.base64");
        base64::engine::general_purpose::STANDARD
            .decode(
                std::fs::read_to_string(&fixture)
                    .expect("fixture should be readable")
                    .trim(),
            )
            .expect("fixture should decode")
    }

    #[test]
    fn test_golden_bun_lockb_v2() {
        let test_file = PathBuf::from("testdata/bun/legacy/bun.lockb.v2");
        let expected_file = PathBuf::from("testdata/bun/golden/bun-lockb-v2-expected.json");

        let package_data = BunLockbParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_bun_lockb_v2_without_scripts_field() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let test_file = temp_dir.path().join("bun.lockb");
        std::fs::write(&test_file, decode_legacy_no_scripts_fixture())
            .expect("Failed to write decoded bun.lockb fixture");
        let expected_file =
            PathBuf::from("testdata/bun/golden/bun-lockb-v2-no-scripts-expected.json");

        let package_data = BunLockbParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }
}
