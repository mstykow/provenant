// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::models::Sha256Digest;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::{PackageParser, PipfileLockParser};
    use std::path::PathBuf;

    #[test]
    fn test_parse_pipfile_lock_golden() {
        let test_file = PathBuf::from("testdata/python/golden/pipfile_lock/Pipfile.lock");
        let expected_file =
            PathBuf::from("testdata/python/golden/pipfile_lock/Pipfile.lock-expected.json");

        let package_data = PipfileLockParser::extract_first_package(&test_file);

        assert_eq!(package_data.dependencies.len(), 9);
        assert_eq!(
            package_data.sha256,
            Some(
                Sha256Digest::from_hex(
                    "813f8e1b624fd42eee7d681228d7aca1fce209e1d60bf21c3eb33a73f7268d57"
                )
                .unwrap()
            )
        );
        assert!(
            package_data
                .dependencies
                .iter()
                .all(|dep| dep.scope.as_deref() != Some("develop"))
        );

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }
}
