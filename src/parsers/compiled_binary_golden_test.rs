#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use std::fs;
    use std::path::PathBuf;

    use crate::parsers::golden_test_utils::compare_package_data_collection_parser_only;
    use crate::parsers::try_parse_compiled_bytes;

    const RUST_COMPILED_BINARY_FIXTURE: &str =
        "testdata/compiled-binary-golden/rust/cargo_dependencies";
    const GO_COMPILED_BINARY_FIXTURE: &str = "testdata/compiled-binary-golden/go-basic/demo";

    #[test]
    fn test_golden_rust_compiled_binary() {
        let test_file = PathBuf::from(RUST_COMPILED_BINARY_FIXTURE);
        let expected_file =
            PathBuf::from("testdata/compiled-binary-golden/rust/cargo_dependencies.expected.json");

        assert!(
            test_file.exists(),
            "missing fixture: {}",
            test_file.display()
        );
        assert!(
            expected_file.exists(),
            "missing fixture: {}",
            expected_file.display()
        );

        let bytes = fs::read(&test_file).expect("read rust compiled binary fixture");
        let parse_result = try_parse_compiled_bytes(&bytes).expect("compiled Rust binary packages");

        match compare_package_data_collection_parser_only(&parse_result.packages, &expected_file) {
            Ok(_) => (),
            Err(error) => panic!(
                "Golden test failed for rust compiled binary: {error}\nactual={}",
                serde_json::to_string_pretty(&parse_result.packages).unwrap_or_default()
            ),
        }
    }

    #[test]
    fn test_golden_go_compiled_binary() {
        let test_file = PathBuf::from(GO_COMPILED_BINARY_FIXTURE);
        let expected_file =
            PathBuf::from("testdata/compiled-binary-golden/go-basic/demo.expected.json");

        assert!(
            test_file.exists(),
            "missing fixture: {}",
            test_file.display()
        );
        assert!(
            expected_file.exists(),
            "missing fixture: {}",
            expected_file.display()
        );

        let bytes = fs::read(&test_file).expect("read go compiled binary fixture");
        let parse_result = try_parse_compiled_bytes(&bytes).expect("compiled Go binary packages");

        match compare_package_data_collection_parser_only(&parse_result.packages, &expected_file) {
            Ok(_) => (),
            Err(error) => panic!(
                "Golden test failed for go compiled binary: {error}\nactual={}",
                serde_json::to_string_pretty(&parse_result.packages).unwrap_or_default()
            ),
        }
    }
}
