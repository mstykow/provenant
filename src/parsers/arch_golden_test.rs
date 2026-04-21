// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::arch::{ArchPkginfoParser, ArchSrcinfoParser};
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use std::path::PathBuf;

    #[test]
    fn test_golden_arch_srcinfo_basic() {
        let test_file = PathBuf::from("testdata/arch/srcinfo/basic/.SRCINFO");
        let expected_file = PathBuf::from("testdata/arch/golden/srcinfo-basic-expected.json");

        let package_data = ArchSrcinfoParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for Arch .SRCINFO: {}", e),
        }
    }

    #[test]
    fn test_golden_arch_pkginfo_basic() {
        let test_file = PathBuf::from("testdata/arch/pkginfo/basic/.PKGINFO");
        let expected_file = PathBuf::from("testdata/arch/golden/pkginfo-basic-expected.json");

        let package_data = ArchPkginfoParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for Arch .PKGINFO: {}", e),
        }
    }
}
