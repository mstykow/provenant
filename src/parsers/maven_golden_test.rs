#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::maven::MavenParser;
    use std::path::Path;
    use std::path::PathBuf;

    fn assert_fixture_exists(path: &Path) {
        assert!(path.exists(), "missing fixture: {}", path.display());
    }

    #[test]
    fn test_golden_basic() {
        let test_file = PathBuf::from("testdata/maven-golden/basic/pom.xml");
        let expected_file = PathBuf::from("testdata/maven-golden/basic/pom.xml.expected");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = MavenParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for basic: {}", e),
        }
    }

    #[test]
    fn test_golden_logback_access() {
        let test_file = PathBuf::from("testdata/maven-golden/logback-access/pom.xml");
        let expected_file = PathBuf::from("testdata/maven-golden/logback-access/pom.xml.expected");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = MavenParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for logback-access: {}", e),
        }
    }

    #[test]
    fn test_golden_spring() {
        let test_file = PathBuf::from("testdata/maven-golden/spring/pom.xml");
        let expected_file = PathBuf::from("testdata/maven-golden/spring/pom.xml.expected");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = MavenParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for spring: {}", e),
        }
    }

    #[test]
    fn test_golden_commons_fileupload() {
        let test_file = PathBuf::from("testdata/maven-golden/commons-fileupload/pom.xml");
        let expected_file =
            PathBuf::from("testdata/maven-golden/commons-fileupload/pom.xml.expected");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = MavenParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for commons-fileupload: {}", e),
        }
    }

    #[test]
    fn test_golden_jrecordbind() {
        let test_file = PathBuf::from("testdata/maven-golden/jrecordbind/pom.xml");
        let expected_file = PathBuf::from("testdata/maven-golden/jrecordbind/pom.xml.expected");

        assert_fixture_exists(&test_file);
        assert_fixture_exists(&expected_file);

        let package_data = MavenParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for jrecordbind: {}", e),
        }
    }
}
