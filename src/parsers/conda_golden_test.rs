#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::conda::{CondaEnvironmentYmlParser, CondaMetaYamlParser};
    use crate::parsers::conda_meta_json::CondaMetaJsonParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use std::path::PathBuf;

    /// Helper function to run golden tests.
    ///
    /// Compares parsed output against expected JSON files.
    fn run_golden(test_file: &str, expected_file: &str, parser_type: &str) {
        let test_path = PathBuf::from(test_file);

        let package_data = match parser_type {
            "meta" => CondaMetaYamlParser::extract_first_package(&test_path),
            "env" => CondaEnvironmentYmlParser::extract_first_package(&test_path),
            "meta-json" => CondaMetaJsonParser::extract_first_package(&test_path),
            _ => panic!("Unknown parser type: {}", parser_type),
        };

        match compare_package_data_parser_only(&package_data, &PathBuf::from(expected_file)) {
            Ok(()) => {}
            Err(e) => panic!("Golden test failed for {}: {}", test_file, e),
        }
    }

    #[test]
    fn test_golden_meta_yaml_abeona() {
        run_golden(
            "testdata/conda/meta-yaml/abeona/meta.yaml",
            "testdata/conda/meta-yaml/abeona/meta.yaml-expected.json",
            "meta",
        );
    }

    #[test]
    fn test_golden_meta_yaml_gcnvkernel() {
        run_golden(
            "testdata/conda/meta-yaml/gcnvkernel/meta.yaml",
            "testdata/conda/meta-yaml/gcnvkernel/meta.yaml-expected.json",
            "meta",
        );
    }

    #[test]
    fn test_golden_meta_yaml_pippy() {
        run_golden(
            "testdata/conda/meta-yaml/pippy/meta.yaml",
            "testdata/conda/meta-yaml/pippy/meta.yaml-expected.json",
            "meta",
        );
    }

    #[test]
    fn test_golden_environment_ringer() {
        run_golden(
            "testdata/conda/conda-yaml/ringer/environment.yaml",
            "testdata/conda/conda-yaml/ringer/environment.yaml-expected.json",
            "env",
        );
    }

    #[test]
    fn test_golden_environment_test() {
        run_golden(
            "testdata/conda/conda-yaml/test/environment_host_port.yml",
            "testdata/conda/conda-yaml/test/environment_host_port.yml-expected.json",
            "env",
        );
    }

    #[test]
    fn test_golden_environment_phc_gnn() {
        run_golden(
            "testdata/conda/conda-yaml/phc-gnn/environment_gpu.yml",
            "testdata/conda/conda-yaml/phc-gnn/environment_gpu.yml-expected.json",
            "env",
        );
    }

    #[test]
    fn test_golden_conda_meta_json_tzdata() {
        run_golden(
            "testdata/conda/conda-meta/tzdata-2024b-h04d1e81_0.json",
            "testdata/conda/conda-meta/tzdata-2024b-h04d1e81_0.json-expected.json",
            "meta-json",
        );
    }
}
