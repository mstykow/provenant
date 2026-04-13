#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::julia::JuliaManifestTomlParser;
    use crate::parsers::julia::JuliaProjectTomlParser;
    use std::path::Path;
    use std::path::PathBuf;

    fn assert_fixture_exists(path: &Path) {
        assert!(path.exists(), "missing fixture: {}", path.display());
    }

    #[test]
    fn test_golden_project_basic() {
        let test_file = PathBuf::from("testdata/julia-golden/basic/Project.toml");
        let expected_file = PathBuf::from("testdata/julia-golden/basic/Project.toml.expected");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = JuliaProjectTomlParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for basic: {}", e),
        }
    }

    #[test]
    fn test_golden_manifest_basic() {
        let test_file = PathBuf::from("testdata/julia-golden/basic/Manifest.toml");
        let expected_file = PathBuf::from("testdata/julia-golden/basic/Manifest.toml.expected");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = JuliaManifestTomlParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for manifest basic: {}", e),
        }
    }
}
