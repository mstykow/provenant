#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::microsoft_update_manifest::MicrosoftUpdateManifestParser;
    use std::path::PathBuf;

    #[test]
    fn test_golden_microsoft_update_manifest() {
        let test_file = PathBuf::from("testdata/microsoft-update-manifest/basic/update.mum");
        let expected_file =
            PathBuf::from("testdata/microsoft-update-manifest/basic/update.mum.expected.json");
        let package_data = MicrosoftUpdateManifestParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(()) => {}
            Err(error) => panic!("Golden test failed: {}", error),
        }
    }
}
