// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use std::path::PathBuf;

    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::{DenoLockParser, DenoParser, PackageParser};

    #[test]
    fn test_parse_deno_json_golden() {
        let test_file = PathBuf::from("testdata/deno/golden/deno_json/deno.json");
        let expected_file = PathBuf::from("testdata/deno/golden/deno_json/deno.json-expected.json");

        let package_data = DenoParser::extract_first_package(&test_file);

        assert_eq!(package_data.namespace.as_deref(), Some("@provenant"));
        assert_eq!(package_data.name.as_deref(), Some("deno-sample"));
        assert_eq!(package_data.dependencies.len(), 3);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_parse_deno_jsonc_golden() {
        let test_file = PathBuf::from("testdata/deno/golden/deno_jsonc/deno.jsonc");
        let expected_file =
            PathBuf::from("testdata/deno/golden/deno_jsonc/deno.jsonc-expected.json");

        let package_data = DenoParser::extract_first_package(&test_file);

        assert_eq!(package_data.namespace.as_deref(), Some("@std"));
        assert_eq!(package_data.name.as_deref(), Some("jsonc"));
        assert_eq!(package_data.dependencies.len(), 1);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_parse_deno_lock_golden() {
        let test_file = PathBuf::from("testdata/deno/golden/deno_lock/deno.lock");
        let expected_file = PathBuf::from("testdata/deno/golden/deno_lock/deno.lock-expected.json");

        let package_data = DenoLockParser::extract_first_package(&test_file);

        assert_eq!(package_data.dependencies.len(), 4);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }
}
