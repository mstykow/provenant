// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use std::path::PathBuf;

    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::{NixDefaultParser, NixFlakeLockParser, NixFlakeParser, PackageParser};

    fn run_golden<P: PackageParser>(test_file: &str, expected_file: &str) {
        let package_data = P::extract_first_package(&PathBuf::from(test_file));
        match compare_package_data_parser_only(&package_data, &PathBuf::from(expected_file)) {
            Ok(()) => {}
            Err(error) => panic!("Golden test failed for {}: {}", test_file, error),
        }
    }

    #[test]
    fn test_golden_flake_manifest() {
        run_golden::<NixFlakeParser>(
            "testdata/nix-golden/flake-demo/flake.nix",
            "testdata/nix-golden/flake-demo/flake.nix.expected.json",
        );
    }

    #[test]
    fn test_golden_flake_manifest_with_relative_paths() {
        run_golden::<NixFlakeParser>(
            "testdata/nix-golden/rnp-flake/flake.nix",
            "testdata/nix-golden/rnp-flake/flake.nix.expected.json",
        );
    }

    #[test]
    fn test_golden_flake_lock() {
        run_golden::<NixFlakeLockParser>(
            "testdata/nix-golden/lock-demo/flake.lock",
            "testdata/nix-golden/lock-demo/flake.lock.expected.json",
        );
    }

    #[test]
    fn test_golden_default_nix_derivation() {
        run_golden::<NixDefaultParser>(
            "testdata/nix-golden/default-demo/default.nix",
            "testdata/nix-golden/default-demo/default.nix.expected.json",
        );
    }
}
