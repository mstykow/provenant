// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::clojure::{ClojureDepsEdnParser, ClojureProjectCljParser};
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use std::path::PathBuf;

    #[test]
    fn test_golden_deps_edn() {
        let test_file = PathBuf::from("testdata/clojure-golden/basic-deps/deps.edn");
        let expected_file =
            PathBuf::from("testdata/clojure-golden/basic-deps/deps.edn.expected.json");

        let package_data = ClojureDepsEdnParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(error) => panic!("Golden test failed: {}", error),
        }
    }

    #[test]
    fn test_golden_project_clj() {
        let test_file = PathBuf::from("testdata/clojure-golden/basic-project/project.clj");
        let expected_file =
            PathBuf::from("testdata/clojure-golden/basic-project/project.clj.expected.json");

        let package_data = ClojureProjectCljParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(error) => panic!("Golden test failed: {}", error),
        }
    }
}
