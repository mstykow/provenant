// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::pnpm_lock::PnpmLockParser;
    use std::path::PathBuf;

    fn run_golden(test_file: &str, expected_file: &str) {
        let package_data = PnpmLockParser::extract_first_package(&PathBuf::from(test_file));
        match compare_package_data_parser_only(&package_data, &PathBuf::from(expected_file)) {
            Ok(()) => {}
            Err(error) => panic!("Golden test failed for {}: {}", test_file, error),
        }
    }

    #[test]
    fn test_golden_pnpm_v5() {
        run_golden(
            "testdata/pnpm/pnpm-v5.yaml",
            "testdata/pnpm/pnpm-v5.yaml.expected.json",
        );
    }

    #[test]
    fn test_golden_pnpm_v6() {
        run_golden(
            "testdata/pnpm/pnpm-v6.yaml",
            "testdata/pnpm/pnpm-v6.yaml.expected.json",
        );
    }

    #[test]
    fn test_golden_pnpm_v9() {
        run_golden(
            "testdata/pnpm/pnpm-v9.yaml",
            "testdata/pnpm/pnpm-v9.yaml.expected.json",
        );
    }
}
