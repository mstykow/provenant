// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::bitbake::BitbakeRecipeParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use std::path::Path;
    use std::path::PathBuf;

    fn assert_fixture_exists(path: &Path) {
        assert!(path.exists(), "missing fixture: {}", path.display());
    }

    #[test]
    fn test_golden_bitbake_basic() {
        let test_file = PathBuf::from("testdata/bitbake-golden/basic/busybox_1.36.1.bb");
        let expected_file =
            PathBuf::from("testdata/bitbake-golden/basic/busybox_1.36.1.bb.expected");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = BitbakeRecipeParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for BitBake basic: {}", e),
        }
    }
}
