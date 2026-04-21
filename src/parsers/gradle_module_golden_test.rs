// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::{GradleModuleParser, PackageParser};
    use std::path::PathBuf;

    fn run_golden(test_file: &str, expected_file: &str) {
        let package_data = GradleModuleParser::extract_first_package(&PathBuf::from(test_file));
        match compare_package_data_parser_only(&package_data, &PathBuf::from(expected_file)) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_gradle_module_simple() {
        run_golden(
            "testdata/gradle-golden/module/simple.module",
            "testdata/gradle-golden/module/simple.module-expected.json",
        );
    }

    #[test]
    fn test_golden_gradle_module_material() {
        run_golden(
            "testdata/gradle-golden/module/material-1.9.0.module",
            "testdata/gradle-golden/module/material-1.9.0.module-expected.json",
        );
    }

    #[test]
    fn test_golden_gradle_module_converter_moshi() {
        run_golden(
            "testdata/gradle-golden/module/converter-moshi-2.11.0.module",
            "testdata/gradle-golden/module/converter-moshi-2.11.0.module-expected.json",
        );
    }
}
