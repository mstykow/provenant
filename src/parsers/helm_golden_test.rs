#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::helm::{HelmChartLockParser, HelmChartYamlParser};
    use std::path::PathBuf;

    #[test]
    fn test_golden_chart_yaml() {
        let test_file = PathBuf::from("testdata/helm-golden/chart-basic/Chart.yaml");
        let expected_file =
            PathBuf::from("testdata/helm-golden/chart-basic/Chart.yaml.expected.json");

        let package_data = HelmChartYamlParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(error) => panic!("Golden test failed: {}", error),
        }
    }

    #[test]
    fn test_golden_chart_lock() {
        let test_file = PathBuf::from("testdata/helm-golden/lock-basic/Chart.lock");
        let expected_file =
            PathBuf::from("testdata/helm-golden/lock-basic/Chart.lock.expected.json");

        let package_data = HelmChartLockParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(error) => panic!("Golden test failed: {}", error),
        }
    }
}
