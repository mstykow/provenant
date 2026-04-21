// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Golden tests for CocoaPods parsers.

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::podfile::PodfileParser;
    use crate::parsers::podfile_lock::PodfileLockParser;
    use crate::parsers::podspec::PodspecParser;
    use crate::parsers::podspec_json::PodspecJsonParser;
    use std::path::PathBuf;

    #[test]
    fn test_golden_podspec_json_firebase_analytics() {
        let test_file =
            PathBuf::from("testdata/cocoapods-golden/podspec.json/FirebaseAnalytics.podspec.json");
        let expected_file = PathBuf::from(
            "testdata/cocoapods-golden/podspec.json/FirebaseAnalytics.podspec.json.expected.json",
        );

        let package_data = PodspecJsonParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_podfile_lock_braintree() {
        let test_file =
            PathBuf::from("testdata/cocoapods-golden/podfile.lock/braintree_ios_Podfile.lock");
        let expected_file = PathBuf::from(
            "testdata/cocoapods-golden/podfile.lock/braintree_ios_Podfile.lock.expected.json",
        );

        let package_data = PodfileLockParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_podfile_lock_artsy() {
        let test_file =
            PathBuf::from("testdata/cocoapods-golden/podfile.lock/artsy_eigen_Podfile.lock");
        let expected_file = PathBuf::from(
            "testdata/cocoapods-golden/podfile.lock/artsy_eigen_Podfile.lock.expected.json",
        );

        let package_data = PodfileLockParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_podfile_lock_solo() {
        let test_file = PathBuf::from("testdata/cocoapods-golden/assemble/solo/Podfile.lock");
        let expected_file =
            PathBuf::from("testdata/cocoapods-golden/assemble/solo/Podfile.lock-expected.json");

        let package_data = PodfileLockParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_podspec_rxdatasources_solo() {
        let test_file =
            PathBuf::from("testdata/cocoapods-golden/assemble/solo/RxDataSources.podspec");
        let expected_file = PathBuf::from(
            "testdata/cocoapods-golden/assemble/solo/RxDataSources-package-only.podspec-expected.json",
        );

        let package_data = PodspecParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_podspec_rxdatasources() {
        let test_file = PathBuf::from("testdata/cocoapods-golden/podspec/RxDataSources.podspec");
        let expected_file =
            PathBuf::from("testdata/cocoapods-golden/podspec/RxDataSources.podspec.expected.json");

        let package_data = PodspecParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_rxdatasources_podspec_has_no_duplicate_dependencies() {
        let test_file = PathBuf::from("testdata/cocoapods-golden/podspec/RxDataSources.podspec");
        let package_data = PodspecParser::extract_first_package(&test_file);

        assert_eq!(package_data.name.as_deref(), Some("RxDataSources"));
        assert_eq!(package_data.dependencies.len(), 3);

        let mut unique_purls = std::collections::BTreeSet::new();
        for dep in &package_data.dependencies {
            unique_purls.insert(dep.purl.clone());
        }
        assert_eq!(unique_purls.len(), package_data.dependencies.len());
    }

    #[test]
    fn test_golden_podspec_starscream() {
        let test_file = PathBuf::from("testdata/cocoapods-golden/podspec/Starscream.podspec");
        let expected_file =
            PathBuf::from("testdata/cocoapods-golden/podspec/Starscream.podspec.expected.json");

        let package_data = PodspecParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_podspec_nanopb() {
        let test_file = PathBuf::from("testdata/cocoapods-golden/podspec/nanopb.podspec");
        let expected_file =
            PathBuf::from("testdata/cocoapods-golden/podspec/nanopb.podspec.expected.json");

        let package_data = PodspecParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_podspec_badgehub() {
        let test_file = PathBuf::from("testdata/cocoapods-golden/podspec/BadgeHub.podspec");
        let expected_file =
            PathBuf::from("testdata/cocoapods-golden/podspec/BadgeHub.podspec.expected.json");

        let package_data = PodspecParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_podfile_solo() {
        let test_file = PathBuf::from("testdata/cocoapods-golden/assemble/solo/Podfile");
        let expected_file = PathBuf::from(
            "testdata/cocoapods-golden/assemble/solo/Podfile-package-only-expected.json",
        );

        let package_data = PodfileParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }
}
