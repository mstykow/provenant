// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use std::path::PathBuf;

    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::{
        HackageCabalParser, HackageCabalProjectParser, HackageStackYamlParser, PackageParser,
    };

    #[test]
    fn test_golden_cabal_basic() {
        let test_file = PathBuf::from("testdata/hackage-golden/cabal-basic/example-hackage.cabal");
        let expected_file = PathBuf::from(
            "testdata/hackage-golden/cabal-basic/example-hackage.cabal.expected.json",
        );

        let package_data = HackageCabalParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(error) => panic!("Golden test failed: {}", error),
        }
    }

    #[test]
    fn test_golden_cabal_project_basic() {
        let test_file = PathBuf::from("testdata/hackage-golden/project-basic/cabal.project");
        let expected_file =
            PathBuf::from("testdata/hackage-golden/project-basic/cabal.project.expected.json");

        let package_data = HackageCabalProjectParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(error) => panic!("Golden test failed: {}", error),
        }
    }

    #[test]
    fn test_golden_stack_yaml_basic() {
        let test_file = PathBuf::from("testdata/hackage-golden/stack-basic/stack.yaml");
        let expected_file =
            PathBuf::from("testdata/hackage-golden/stack-basic/stack.yaml.expected.json");

        let package_data = HackageStackYamlParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(error) => panic!("Golden test failed: {}", error),
        }
    }
}
