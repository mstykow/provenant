// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::dart::{PubspecLockParser, PubspecYamlParser};
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use std::path::PathBuf;

    #[test]
    fn test_golden_mini_lock() {
        let test_file = PathBuf::from("testdata/dart-golden/mini-lock/pubspec.lock");
        let expected_file = PathBuf::from("testdata/dart-golden/mini-lock/pubspec.lock.expected");

        let package_data = PubspecLockParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_stock_lock() {
        let test_file = PathBuf::from("testdata/dart-golden/stock-lock/pubspec.lock");
        let expected_file = PathBuf::from("testdata/dart-golden/stock-lock/pubspec.lock.expected");

        let package_data = PubspecLockParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_simple_yaml() {
        let test_file = PathBuf::from("testdata/dart-golden/simple-yaml/pubspec.yaml");
        let expected_file = PathBuf::from("testdata/dart-golden/simple-yaml/pubspec.yaml.expected");

        let package_data = PubspecYamlParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_many_deps_yaml() {
        let test_file = PathBuf::from("testdata/dart-golden/many-deps-yaml/pubspec.yaml");
        let expected_file =
            PathBuf::from("testdata/dart-golden/many-deps-yaml/pubspec.yaml.expected");

        let package_data = PubspecYamlParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_publish_pubspec() {
        let test_file = PathBuf::from("testdata/dart-golden/publish-pubspec/pubspec.yaml");
        let expected_file =
            PathBuf::from("testdata/dart-golden/publish-pubspec/pubspec.yaml.expected");

        let package_data = PubspecYamlParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_deps_versions_dict_pubspec() {
        let test_file =
            PathBuf::from("testdata/dart-golden/deps-versions-dict-pubspec/pubspec.yaml");
        let expected_file =
            PathBuf::from("testdata/dart-golden/deps-versions-dict-pubspec/pubspec.yaml.expected");

        let package_data = PubspecYamlParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_path_lock() {
        let test_file = PathBuf::from("testdata/dart-golden/path-lock/pubspec.lock");
        let expected_file = PathBuf::from("testdata/dart-golden/path-lock/pubspec.lock.expected");

        let package_data = PubspecLockParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_private_archive_pubspec() {
        let test_file = PathBuf::from("testdata/dart-golden/private-archive-pubspec/pubspec.yaml");
        let expected_file =
            PathBuf::from("testdata/dart-golden/private-archive-pubspec/pubspec.yaml.expected");

        let package_data = PubspecYamlParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }
}
