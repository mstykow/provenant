#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::{PackageParser, PythonParser};
    use std::path::PathBuf;

    #[test]
    fn test_golden_metadata() {
        let test_file =
            PathBuf::from("testdata/python/golden/metadata/six-1.10.0.dist-info/METADATA");
        let expected_file = PathBuf::from("testdata/python/golden/metadata/METADATA-expected.json");

        let package_data = PythonParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_setup_cfg() {
        let test_file = PathBuf::from("testdata/python/golden/setup_cfg_wheel/setup.cfg");
        let expected_file =
            PathBuf::from("testdata/python/setup_cfg_wheel/setup.cfg-expected-corrected.json");

        let package_data = PythonParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_setup_py_function_call() {
        let test_file = PathBuf::from("testdata/python/golden/setup_py_function_call/setup.py");
        let expected_file =
            PathBuf::from("testdata/python/golden/setup_py_function_call/setup.py-expected.json");

        let package_data = PythonParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_setup_py_empty_call() {
        let test_file = PathBuf::from("testdata/python/golden/setup_py_empty_call/setup.py");
        let expected_file =
            PathBuf::from("testdata/python/golden/setup_py_empty_call/setup.py-expected.json");

        let package_data = PythonParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_pyproject_toml() {
        let test_file = PathBuf::from("testdata/python/pyproject.toml");
        let expected_file = PathBuf::from("testdata/python/golden/pyproject.toml-expected.json");

        let package_data = PythonParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_pypi_json() {
        let test_file = PathBuf::from("testdata/python/pypi.json");
        let expected_file = PathBuf::from("testdata/python/golden/pypi.json-expected.json");

        let package_data = PythonParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_anonapi_wheel_metadata() {
        let test_file = PathBuf::from(
            "testdata/python/golden/metadata-fixtures/unpacked_wheel/metadata-2.1/with_sources/anonapi-0.0.19.dist-info/METADATA",
        );
        let expected_file =
            PathBuf::from("testdata/python/golden/anonapi-wheel-metadata-expected.json");

        let package_data = PythonParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_anonapi_sdist_pkginfo() {
        let test_file = PathBuf::from(
            "testdata/python/golden/metadata-fixtures/unpacked_sdist/metadata-1.2/anonapi-0.0.19/PKG-INFO",
        );
        let expected_file =
            PathBuf::from("testdata/python/golden/anonapi-sdist-pkginfo-expected.json");

        let package_data = PythonParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_demo_sdist_archive() {
        let test_file = PathBuf::from("testdata/python/golden/archives/demo-1.0.0.tar.gz");
        let expected_file =
            PathBuf::from("testdata/python/golden/archives/demo-1.0.0.tar.gz-expected.json");

        let package_data = PythonParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_pip_cache_origin_json() {
        let test_file =
            PathBuf::from("testdata/python/golden/pip_cache/wheels/construct/origin.json");
        let expected_file = PathBuf::from(
            "testdata/python/golden/pip_cache/wheels/construct/origin.json-expected.json",
        );

        let package_data = PythonParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }
}
