#[cfg(all(test, feature = "golden-tests"))]
mod tests {
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::{HaxeParser, PackageParser};
    use std::path::PathBuf;

    #[test]
    fn test_haxe_golden_basic() {
        let haxelib_path = PathBuf::from("testdata/haxe/basic/haxelib.json");
        let expected_path = PathBuf::from("testdata/haxe/basic/haxelib.json.expected");

        let package_data = HaxeParser::extract_first_package(&haxelib_path);
        compare_package_data_parser_only(&package_data, &expected_path)
            .expect("Golden test failed for basic");
    }

    #[test]
    fn test_haxe_golden_basic2() {
        let haxelib_path = PathBuf::from("testdata/haxe/basic2/haxelib.json");
        let expected_path = PathBuf::from("testdata/haxe/basic2/haxelib.json.expected");

        let package_data = HaxeParser::extract_first_package(&haxelib_path);
        compare_package_data_parser_only(&package_data, &expected_path)
            .expect("Golden test failed for basic2");
    }

    #[test]
    fn test_haxe_golden_deps() {
        let haxelib_path = PathBuf::from("testdata/haxe/deps/haxelib.json");
        let expected_path = PathBuf::from("testdata/haxe/deps/haxelib.json.expected");

        let package_data = HaxeParser::extract_first_package(&haxelib_path);
        compare_package_data_parser_only(&package_data, &expected_path)
            .expect("Golden test failed for deps");
    }

    #[test]
    fn test_haxe_golden_tags() {
        let haxelib_path = PathBuf::from("testdata/haxe/tags/haxelib.json");
        let expected_path = PathBuf::from("testdata/haxe/tags/haxelib.json.expected");

        let package_data = HaxeParser::extract_first_package(&haxelib_path);
        compare_package_data_parser_only(&package_data, &expected_path)
            .expect("Golden test failed for tags");
    }
}
