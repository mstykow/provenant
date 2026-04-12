#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::npm::NpmParser;
    use crate::parsers::npm_lock::NpmLockParser;
    use crate::parsers::npm_workspace::NpmWorkspaceParser;
    use crate::parsers::yarn_lock::YarnLockParser;
    use serde_json::Value;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn test_golden_basic() {
        let test_file = PathBuf::from("testdata/npm-golden/basic/package.json");
        let expected_file = PathBuf::from("testdata/npm-golden/basic/package.json.expected");

        let package_data = NpmParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_authors_list_dicts() {
        let test_file = PathBuf::from("testdata/npm-golden/authors_list_dicts/package.json");
        let expected_file =
            PathBuf::from("testdata/npm-golden/authors_list_dicts/package.json.expected");

        let package_data = NpmParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_authors_list_strings() {
        let test_file = PathBuf::from("testdata/npm-golden/authors_list_strings/package.json");
        let expected_file =
            PathBuf::from("testdata/npm-golden/authors_list_strings/package.json.expected");

        let package_data = NpmParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_double_license() {
        let test_file = PathBuf::from("testdata/npm-golden/double_license/package.json");
        let expected_file =
            PathBuf::from("testdata/npm-golden/double_license/package.json.expected");

        let package_data = NpmParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_express_jwt() {
        let test_file = PathBuf::from("testdata/npm-golden/express-jwt-3.4.0/package.json");
        let expected_file =
            PathBuf::from("testdata/npm-golden/express-jwt-3.4.0/package.json.expected");

        let package_data = NpmParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_from_npmjs() {
        let test_file = PathBuf::from("testdata/npm-golden/from_npmjs/package.json");
        let expected_file = PathBuf::from("testdata/npm-golden/from_npmjs/package.json.expected");

        let package_data = NpmParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_bundled_deps() {
        let test_file = PathBuf::from("testdata/npm-golden/bundledDeps/package.json");
        let expected_file = PathBuf::from("testdata/npm-golden/bundledDeps/package.json.expected");

        let package_data = NpmParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_casepath() {
        let test_file = PathBuf::from("testdata/npm-golden/casepath/package.json");
        let expected_file = PathBuf::from("testdata/npm-golden/casepath/package.json.expected");

        let package_data = NpmParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_chartist() {
        let test_file = PathBuf::from("testdata/npm-golden/chartist/package.json");
        let expected_file = PathBuf::from("testdata/npm-golden/chartist/package.json.expected");

        let package_data = NpmParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_dist() {
        let test_file = PathBuf::from("testdata/npm-golden/dist/package.json");
        let expected_file = PathBuf::from("testdata/npm-golden/dist/package.json.expected");

        let package_data = NpmParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_electron() {
        let test_file = PathBuf::from("testdata/npm-golden/electron/package.json");
        let expected_file = PathBuf::from("testdata/npm-golden/electron/package.json.expected");

        let package_data = NpmParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_npm_lock_v2() {
        let test_file = PathBuf::from("testdata/npm/package-lock-v2.json");
        let expected_file = PathBuf::from("testdata/npm/package-lock-v2.json.expected.json");

        let package_data = NpmLockParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_npm_lock_v1_unnamed_root() {
        let test_file = PathBuf::from("testdata/npm/package-lock-v1-unnamed-root.json");
        let expected_file =
            PathBuf::from("testdata/npm/package-lock-v1-unnamed-root.json.expected.json");

        let package_data = NpmLockParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_npm_workspace() {
        let test_file = PathBuf::from("testdata/npm-workspace/basic.yaml");
        let expected_file = PathBuf::from("testdata/npm-workspace/basic.yaml.expected.json");

        let package_data = NpmWorkspaceParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_yarn_lock_v1() {
        let test_file = PathBuf::from("testdata/npm/yarn-v1.lock");
        let expected_file = PathBuf::from("testdata/npm/yarn-v1.lock.expected.json");

        let package_data = YarnLockParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_yarn_lock_v2() {
        let test_file = PathBuf::from("testdata/npm/yarn-v2.lock");
        let expected_file = PathBuf::from("testdata/npm/yarn-v2.lock.expected.json");

        let package_data = YarnLockParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_yarn_lock_v2_protocol_resolution() {
        let test_file = PathBuf::from("testdata/npm/yarn-v2-protocol.lock");
        let expected_file = PathBuf::from("testdata/npm/yarn-v2-protocol.lock.expected.json");

        let package_data = YarnLockParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }

        let metadata_expected_file =
            PathBuf::from("testdata/npm/yarn-v2-protocol.lock.extra.expected.json");
        let metadata_expected: Value = serde_json::from_str(
            &fs::read_to_string(metadata_expected_file).expect("expected metadata fixture"),
        )
        .expect("valid expected metadata json");

        let package_extra =
            serde_json::to_value(package_data.extra_data).expect("serialize package extra_data");
        assert_eq!(
            package_extra,
            metadata_expected
                .get("package_extra_data")
                .cloned()
                .unwrap()
        );

        let dep = package_data
            .dependencies
            .first()
            .expect("expected one dependency");
        let dep_extra =
            serde_json::to_value(dep.extra_data.clone()).expect("serialize dep extra_data");
        assert_eq!(
            dep_extra,
            metadata_expected
                .get("dependency_extra_data")
                .cloned()
                .unwrap()
        );

        let resolved_extra = serde_json::to_value(
            dep.resolved_package
                .as_ref()
                .expect("expected resolved package")
                .extra_data
                .clone(),
        )
        .expect("serialize resolved extra_data");
        assert_eq!(
            resolved_extra,
            metadata_expected
                .get("resolved_package_extra_data")
                .cloned()
                .unwrap()
        );
    }

    #[test]
    fn test_golden_platform_metadata_extra_data() {
        let test_file = PathBuf::from("testdata/npm-golden/platform_metadata/package.json");
        let expected_file = PathBuf::from(
            "testdata/npm-golden/platform_metadata/package.json.extra_data.expected.json",
        );

        let package_data = NpmParser::extract_first_package(&test_file);
        let actual_extra_data =
            serde_json::to_value(package_data.extra_data).expect("serialize npm extra_data");
        let expected_extra_data: Value = serde_json::from_str(
            &fs::read_to_string(expected_file).expect("expected npm metadata fixture"),
        )
        .expect("valid expected npm metadata json");

        assert_eq!(actual_extra_data, expected_extra_data);
    }
}
