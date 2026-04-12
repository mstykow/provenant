//! Golden tests for Swift Package Manager parsers.

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::swift_manifest_json::SwiftManifestJsonParser;
    use crate::parsers::swift_resolved::SwiftPackageResolvedParser;
    use crate::parsers::swift_show_dependencies::SwiftShowDependenciesParser;
    use std::path::PathBuf;

    #[test]
    fn test_golden_swift_fastlane_resolved_v1() {
        let test_file =
            PathBuf::from("testdata/swift-golden/packages/fastlane_resolved_v1/Package.resolved");
        let expected_file =
            PathBuf::from("testdata/swift-golden/swift-fastlane-resolved-v1-parse-expected.json");

        let package_data = SwiftPackageResolvedParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_swift_vercelui_resolved() {
        let test_file = PathBuf::from("testdata/swift-golden/packages/vercelui/Package.resolved");
        let expected_file =
            PathBuf::from("testdata/swift-golden/swift-vercelui-resolved-parse-expected.json");

        let package_data = SwiftPackageResolvedParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_swift_mapboxmaps_resolved() {
        let test_file = PathBuf::from(
            "testdata/swift-golden/packages/mapboxmaps_manifest_and_resolved/Package.resolved",
        );
        let expected_file =
            PathBuf::from("testdata/swift-golden/swift-maboxmaps-resolved-parse-expected.json");

        let package_data = SwiftPackageResolvedParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_swift_mapboxmaps_manifest() {
        let test_file =
            PathBuf::from("testdata/swift-golden/packages/mapboxmaps_manifest/Package.swift.json");
        let expected_file =
            PathBuf::from("testdata/swift-golden/swift-maboxmaps-manifest-parse-expected.json");

        let package_data = SwiftManifestJsonParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_swift_mapboxmaps_manifest_and_resolved() {
        let test_file = PathBuf::from(
            "testdata/swift-golden/packages/mapboxmaps_manifest_and_resolved/Package.swift.json",
        );
        let expected_file =
            PathBuf::from("testdata/swift-golden/swift-maboxmaps-manifest-parse-expected.json");

        let package_data = SwiftManifestJsonParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_swift_vercelui_show_dependencies_parse() {
        let test_file = PathBuf::from(
            "testdata/swift-golden/packages/vercelui_show_dependencies/swift-show-dependencies.deplock",
        );
        let expected_file = PathBuf::from(
            "testdata/swift-golden/swift-vercelui-show-dependencies-parse-expected.json",
        );

        let package_data = SwiftShowDependenciesParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_swift_vercelui_show_dependencies() {
        let test_file = PathBuf::from(
            "testdata/swift-golden/packages/vercelui_show_dependencies/swift-show-dependencies.deplock",
        );
        let expected_file =
            PathBuf::from("testdata/swift-golden/swift-vercelui-show-dependencies-expected.json");

        let package_data = SwiftShowDependenciesParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }
}
