// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::freebsd::FreebsdCompactManifestParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use std::path::PathBuf;

    fn run_golden(test_file: &str, expected_file: &str) {
        let package_data =
            FreebsdCompactManifestParser::extract_first_package(&PathBuf::from(test_file));
        match compare_package_data_parser_only(&package_data, &PathBuf::from(expected_file)) {
            Ok(()) => {}
            Err(error) => panic!("Golden test failed for {}: {}", test_file, error),
        }
    }

    #[test]
    fn test_golden_freebsd_basic() {
        run_golden(
            "testdata/freebsd/basic/+COMPACT_MANIFEST",
            "testdata/freebsd/basic/+COMPACT_MANIFEST.expected.json",
        );
    }

    #[test]
    fn test_golden_freebsd_multi_license() {
        run_golden(
            "testdata/freebsd/multi_license/+COMPACT_MANIFEST",
            "testdata/freebsd/multi_license/+COMPACT_MANIFEST.expected.json",
        );
    }
}
