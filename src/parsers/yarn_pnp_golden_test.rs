#[cfg(test)]
mod golden_tests {
    use std::path::PathBuf;

    use crate::parsers::PackageParser;
    use crate::parsers::YarnPnpParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;

    #[test]
    fn test_golden_yarn_pnp_basic() {
        let test_file = PathBuf::from("testdata/yarn-pnp-golden/basic/.pnp.cjs");
        let expected_file = PathBuf::from("testdata/yarn-pnp-golden/basic/.pnp.cjs.expected.json");
        let package_data = YarnPnpParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(error) => panic!("Golden test failed: {}", error),
        }
    }
}
