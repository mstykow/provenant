// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::os_release::OsReleaseParser;
    use std::path::PathBuf;

    #[test]
    fn test_golden_os_release() {
        let test_file = PathBuf::from("testdata/os-release/etc/os-release");
        let expected_file = PathBuf::from("testdata/os-release/etc/os-release.expected.json");
        let package_data = OsReleaseParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(()) => {}
            Err(error) => panic!("Golden test failed: {}", error),
        }
    }
}
