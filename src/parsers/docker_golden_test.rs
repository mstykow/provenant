// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::docker::DockerfileParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use std::path::PathBuf;

    #[test]
    fn test_golden_dockerfile_jibri() {
        let test_file = PathBuf::from("testdata/docker-golden/jibri/Dockerfile");
        let expected_file = PathBuf::from("testdata/docker-golden/jibri/Dockerfile.expected.json");

        let package_data = DockerfileParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(error) => panic!("Golden test failed: {}", error),
        }
    }

    #[test]
    fn test_golden_containerfile_pulp() {
        let test_file = PathBuf::from("testdata/docker-golden/pulp/Containerfile");
        let expected_file =
            PathBuf::from("testdata/docker-golden/pulp/Containerfile.expected.json");

        let package_data = DockerfileParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(error) => panic!("Golden test failed: {}", error),
        }
    }
}
