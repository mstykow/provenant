// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::erlang_otp::{ErlangAppSrcParser, RebarConfigParser, RebarLockParser};
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use std::path::Path;
    use std::path::PathBuf;

    fn assert_fixture_exists(path: &Path) {
        assert!(path.exists(), "missing fixture: {}", path.display());
    }

    #[test]
    fn test_golden_app_src() {
        let test_file = PathBuf::from("testdata/erlang-otp-golden/lager.app.src");
        let expected_file = PathBuf::from("testdata/erlang-otp-golden/lager.app.src.expected");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = ErlangAppSrcParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for app.src: {}", e),
        }
    }

    #[test]
    fn test_golden_rebar_config() {
        let test_file = PathBuf::from("testdata/erlang-otp-golden/rebar.config");
        let expected_file = PathBuf::from("testdata/erlang-otp-golden/rebar.config.expected");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = RebarConfigParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for rebar.config: {}", e),
        }
    }

    #[test]
    fn test_golden_rebar_lock() {
        let test_file = PathBuf::from("testdata/erlang-otp-golden/rebar.lock");
        let expected_file = PathBuf::from("testdata/erlang-otp-golden/rebar.lock.expected");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = RebarLockParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for rebar.lock: {}", e),
        }
    }
}
