#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::cargo::CargoParser;
    use crate::parsers::cargo_lock::CargoLockParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use std::path::Path;
    use std::path::PathBuf;

    fn assert_fixture_exists(path: &Path) {
        assert!(path.exists(), "missing fixture: {}", path.display());
    }

    #[test]
    fn test_golden_clap() {
        let test_file = PathBuf::from("testdata/cargo-golden/clap/Cargo.toml");
        let expected_file = PathBuf::from("testdata/cargo-golden/clap/Cargo.toml.expected");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = CargoParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for clap: {}", e),
        }
    }

    #[test]
    fn test_golden_package() {
        let test_file = PathBuf::from("testdata/cargo-golden/package/Cargo.toml");
        let expected_file = PathBuf::from("testdata/cargo-golden/package/Cargo.toml.expected");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = CargoParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for package: {}", e),
        }
    }

    #[test]
    fn test_golden_rustup() {
        let test_file = PathBuf::from("testdata/cargo-golden/rustup/Cargo.toml");
        let expected_file = PathBuf::from("testdata/cargo-golden/rustup/Cargo.toml.expected");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = CargoParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for rustup: {}", e),
        }
    }

    #[test]
    fn test_golden_scan() {
        let test_file = PathBuf::from("testdata/cargo-golden/scan/Cargo.toml");
        let expected_file = PathBuf::from("testdata/cargo-golden/scan/Cargo.toml.expected");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = CargoParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for scan: {}", e),
        }
    }

    #[test]
    fn test_golden_single_file_scan() {
        let test_file = PathBuf::from("testdata/cargo-golden/single-file-scan/Cargo.toml");
        let expected_file =
            PathBuf::from("testdata/cargo-golden/single-file-scan/Cargo.toml.expected");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = CargoParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for single-file-scan: {}", e),
        }
    }

    #[test]
    fn test_golden_tauri() {
        let test_file = PathBuf::from("testdata/cargo-golden/tauri/Cargo.toml");
        let expected_file = PathBuf::from("testdata/cargo-golden/tauri/Cargo.toml.expected");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = CargoParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for tauri: {}", e),
        }
    }

    #[test]
    fn test_golden_publish_false() {
        let test_file = PathBuf::from("testdata/cargo-golden/publish-false/Cargo.toml");
        let expected_file =
            PathBuf::from("testdata/cargo-golden/publish-false/Cargo.toml.expected");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = CargoParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for publish-false: {}", e),
        }
    }

    #[test]
    fn test_golden_cargo_lock_basic() {
        let test_file = PathBuf::from("testdata/cargo-golden/lock-basic/Cargo.lock");
        let expected_file = PathBuf::from("testdata/cargo-golden/lock-basic/Cargo.lock.expected");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = CargoLockParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for lock-basic: {}", e),
        }
    }
}
