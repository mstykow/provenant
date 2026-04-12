#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::chef::{ChefMetadataJsonParser, ChefMetadataRbParser};
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use std::path::PathBuf;

    fn run_json_golden(test_file: &str, expected_file: &str) {
        let package_data = ChefMetadataJsonParser::extract_first_package(&PathBuf::from(test_file));
        match compare_package_data_parser_only(&package_data, &PathBuf::from(expected_file)) {
            Ok(()) => {}
            Err(error) => panic!("Chef JSON golden test failed for {}: {}", test_file, error),
        }
    }

    fn run_rb_golden(test_file: &str, expected_file: &str) {
        let package_data = ChefMetadataRbParser::extract_first_package(&PathBuf::from(test_file));
        match compare_package_data_parser_only(&package_data, &PathBuf::from(expected_file)) {
            Ok(()) => {}
            Err(error) => panic!("Chef Ruby golden test failed for {}: {}", test_file, error),
        }
    }

    #[test]
    fn test_golden_chef_metadata_json() {
        run_json_golden(
            "testdata/chef/basic/metadata.json",
            "testdata/chef/basic/metadata.json.expected.json",
        );
    }

    #[test]
    fn test_golden_chef_metadata_rb_basic() {
        run_rb_golden(
            "testdata/chef/basic/metadata.rb",
            "testdata/chef/basic/metadata.rb.expected.json",
        );
    }

    #[test]
    fn test_golden_chef_metadata_rb_dependencies() {
        run_rb_golden(
            "testdata/chef/dependencies/metadata.rb",
            "testdata/chef/dependencies/metadata.rb.expected.json",
        );
    }
}
