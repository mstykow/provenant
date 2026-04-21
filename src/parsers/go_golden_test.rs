// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::go::{GoModParser, GoSumParser, GoWorkParser, GodepsParser};
    use crate::parsers::go_mod_graph::GoModGraphParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use std::path::PathBuf;

    #[test]
    fn test_golden_kingpin_mod() {
        let test_file = PathBuf::from("testdata/go-golden/kingpin-mod/go.mod");
        let expected_file = PathBuf::from("testdata/go-golden/kingpin-mod/go.mod.expected");

        let package_data = GoModParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_sample_mod() {
        let test_file = PathBuf::from("testdata/go-golden/sample-mod/go.mod");
        let expected_file = PathBuf::from("testdata/go-golden/sample-mod/go.mod.expected");

        let package_data = GoModParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_opencensus_service_mod() {
        let test_file = PathBuf::from("testdata/go-golden/opencensus-service/go.mod");
        let expected_file = PathBuf::from("testdata/go-golden/opencensus-service/go.mod.expected");

        let package_data = GoModParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_sample2_sum() {
        let test_file = PathBuf::from("testdata/go-golden/sample2-sum/go.sum");
        let expected_file = PathBuf::from("testdata/go-golden/sample2-sum/go.sum.expected");

        let package_data = GoSumParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_sample3_sum() {
        let test_file = PathBuf::from("testdata/go-golden/sample3-sum/go.sum");
        let expected_file = PathBuf::from("testdata/go-golden/sample3-sum/go.sum.expected");

        let package_data = GoSumParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_sample_graph() {
        let test_file = PathBuf::from("testdata/go-golden/sample-graph/go.mod.graph");
        let expected_file = PathBuf::from("testdata/go-golden/sample-graph/go.mod.graph.expected");

        let package_data = GoModGraphParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_gowork_sample1() {
        let test_file = PathBuf::from("testdata/go-golden/gowork-sample1/go.work");
        let expected_file =
            PathBuf::from("testdata/go-golden/gowork-sample1/go.work.expected.json");

        let package_data = GoWorkParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_gowork_sample2() {
        let test_file = PathBuf::from("testdata/go-golden/gowork-sample2/go.work");
        let expected_file =
            PathBuf::from("testdata/go-golden/gowork-sample2/go.work.expected.json");

        let package_data = GoWorkParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_godeps() {
        let test_file = PathBuf::from("testdata/go/full-godeps.json");
        let expected_file = PathBuf::from("testdata/go/full-godeps.json.expected.json");

        let package_data = GodepsParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_godeps_mini() {
        let test_file = PathBuf::from("testdata/go-golden/godeps-mini/Godeps.json");
        let expected_file =
            PathBuf::from("testdata/go-golden/godeps-mini/Godeps.json.expected.json");

        let package_data = GodepsParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_godeps_comments() {
        let test_file = PathBuf::from("testdata/go-golden/godeps-comments/Godeps.json");
        let expected_file =
            PathBuf::from("testdata/go-golden/godeps-comments/Godeps.json.expected.json");

        let package_data = GodepsParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }
}
