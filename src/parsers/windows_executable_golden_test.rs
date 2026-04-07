#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use std::fs;
    use std::path::PathBuf;

    use crate::parsers::golden_test_utils::compare_package_data_collection_parser_only;
    use crate::parsers::windows_executable::try_parse_windows_executable_bytes;

    const WIN_PE_FIXTURE: &str = "testdata/compiled-binary-golden/win_pe/libiconv2.dll";
    const WIN_PE_FALLBACK_FIXTURE: &str =
        "testdata/compiled-binary-golden/win_pe/no_version_info.dll";

    #[test]
    fn test_golden_windows_executable_binary() {
        let test_file = PathBuf::from(WIN_PE_FIXTURE);
        let expected_file =
            PathBuf::from("testdata/compiled-binary-golden/win_pe/libiconv2.dll.expected.json");

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

        let bytes = fs::read(&test_file).expect("read PE fixture");
        let parse_result = try_parse_windows_executable_bytes(&test_file, &bytes)
            .expect("Windows executable packages");

        match compare_package_data_collection_parser_only(&parse_result.packages, &expected_file) {
            Ok(_) => (),
            Err(error) => panic!(
                "Golden test failed for Windows executable: {error}\nactual={}",
                serde_json::to_string_pretty(&parse_result.packages).unwrap_or_default()
            ),
        }
    }

    #[test]
    fn test_golden_windows_executable_binary_without_version_info() {
        let test_file = PathBuf::from(WIN_PE_FALLBACK_FIXTURE);
        let expected_file = PathBuf::from(
            "testdata/compiled-binary-golden/win_pe/no_version_info.dll.expected.json",
        );

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

        let bytes = fs::read(&test_file).expect("read PE fallback fixture");
        let parse_result = try_parse_windows_executable_bytes(&test_file, &bytes)
            .expect("Windows executable packages");

        match compare_package_data_collection_parser_only(&parse_result.packages, &expected_file) {
            Ok(_) => (),
            Err(error) => panic!(
                "Golden test failed for Windows executable fallback: {error}\nactual={}",
                serde_json::to_string_pretty(&parse_result.packages).unwrap_or_default()
            ),
        }
    }
}
