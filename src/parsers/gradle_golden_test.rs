#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::gradle::GradleParser;
    use crate::parsers::gradle_lock::GradleLockfileParser;
    use std::path::PathBuf;

    fn run_golden(test_file: &str, expected_file: &str) {
        let package_data = GradleParser::extract_first_package(&PathBuf::from(test_file));
        match compare_package_data_parser_only(&package_data, &PathBuf::from(expected_file)) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    fn run_lock_golden(test_file: &str, expected_file: &str) {
        let package_data = GradleLockfileParser::extract_first_package(&PathBuf::from(test_file));
        match compare_package_data_parser_only(&package_data, &PathBuf::from(expected_file)) {
            Ok(_) => (),
            Err(e) => panic!("Golden lockfile test failed: {}", e),
        }
    }

    // =========================================================================
    // Groovy Tests
    // =========================================================================

    #[test]
    fn test_golden_groovy1() {
        run_golden(
            "testdata/gradle-golden/groovy/groovy1/build.gradle",
            "testdata/gradle-golden/groovy/groovy1/build.gradle-expected.json",
        );
    }

    #[test]
    fn test_golden_groovy_compile_only() {
        run_golden(
            "testdata/gradle-golden/groovy/compile-only/build.gradle",
            "testdata/gradle-golden/groovy/compile-only/build.gradle-expected.json",
        );
    }

    #[test]
    fn test_golden_groovy2() {
        run_golden(
            "testdata/gradle-golden/groovy/groovy2/build.gradle",
            "testdata/gradle-golden/groovy/groovy2/build.gradle-expected.json",
        );
    }

    #[test]
    fn test_golden_groovy3() {
        run_golden(
            "testdata/gradle-golden/groovy/groovy3/build.gradle",
            "testdata/gradle-golden/groovy/groovy3/build.gradle-expected.json",
        );
    }

    #[test]
    fn test_golden_groovy4() {
        run_golden(
            "testdata/gradle-golden/groovy/groovy4/build.gradle",
            "testdata/gradle-golden/groovy/groovy4/build.gradle-expected.json",
        );
    }

    #[test]
    fn test_golden_groovy4_singlequotes() {
        run_golden(
            "testdata/gradle-golden/groovy/groovy4-singlequotes/build.gradle",
            "testdata/gradle-golden/groovy/groovy4-singlequotes/build.gradle-expected.json",
        );
    }

    #[test]
    fn test_golden_groovy5() {
        run_golden(
            "testdata/gradle-golden/groovy/groovy5/build.gradle",
            "testdata/gradle-golden/groovy/groovy5/build.gradle-expected.json",
        );
    }

    #[test]
    fn test_golden_groovy5_parens_singlequotes() {
        run_golden(
            "testdata/gradle-golden/groovy/groovy5-parens+singlequotes/build.gradle",
            "testdata/gradle-golden/groovy/groovy5-parens+singlequotes/build.gradle-expected.json",
        );
    }

    #[test]
    fn test_golden_groovy6_braces() {
        run_golden(
            "testdata/gradle-golden/groovy/groovy6-braces/build.gradle",
            "testdata/gradle-golden/groovy/groovy6-braces/build.gradle-expected.json",
        );
    }

    #[test]
    fn test_golden_groovy6_with_props() {
        run_golden(
            "testdata/gradle-golden/groovy/groovy6-with-props/build.gradle",
            "testdata/gradle-golden/groovy/groovy6-with-props/build.gradle-expected.json",
        );
    }

    #[test]
    fn test_golden_groovy_basic() {
        run_golden(
            "testdata/gradle-golden/groovy/groovy-basic/build.gradle",
            "testdata/gradle-golden/groovy/groovy-basic/build.gradle-expected.json",
        );
    }

    #[test]
    fn test_golden_groovy_version_catalog() {
        run_golden(
            "testdata/gradle-golden/groovy/version-catalog/build.gradle",
            "testdata/gradle-golden/groovy/version-catalog/build.gradle-expected.json",
        );
    }

    #[test]
    fn test_golden_groovy_no_parens() {
        run_golden(
            "testdata/gradle-golden/groovy/groovy-no-parens/build.gradle",
            "testdata/gradle-golden/groovy/groovy-no-parens/build.gradle-expected.json",
        );
    }

    #[test]
    fn test_golden_groovy_and_kotlin1() {
        run_golden(
            "testdata/gradle-golden/groovy/groovy-and-kotlin1/build.gradle",
            "testdata/gradle-golden/groovy/groovy-and-kotlin1/build.gradle-expected.json",
        );
    }

    // =========================================================================
    // Kotlin Tests
    // =========================================================================

    #[test]
    fn test_golden_kotlin1() {
        run_golden(
            "testdata/gradle-golden/kotlin/kotlin1/build.gradle.kts",
            "testdata/gradle-golden/kotlin/kotlin1/build.gradle.kts-expected.json",
        );
    }

    #[test]
    fn test_golden_kotlin2() {
        run_golden(
            "testdata/gradle-golden/kotlin/kotlin2/build.gradle.kts",
            "testdata/gradle-golden/kotlin/kotlin2/build.gradle.kts-expected.json",
        );
    }

    #[test]
    fn test_golden_kotlin3() {
        run_golden(
            "testdata/gradle-golden/kotlin/kotlin3/build.gradle.kts",
            "testdata/gradle-golden/kotlin/kotlin3/build.gradle.kts-expected.json",
        );
    }

    #[test]
    fn test_golden_kotlin4() {
        run_golden(
            "testdata/gradle-golden/kotlin/kotlin4/build.gradle.kts",
            "testdata/gradle-golden/kotlin/kotlin4/build.gradle.kts-expected.json",
        );
    }

    #[test]
    fn test_golden_kotlin5() {
        run_golden(
            "testdata/gradle-golden/kotlin/kotlin5/build.gradle.kts",
            "testdata/gradle-golden/kotlin/kotlin5/build.gradle.kts-expected.json",
        );
    }

    #[test]
    fn test_golden_groovy_and_kotlin2() {
        run_golden(
            "testdata/gradle-golden/kotlin/groovy-and-kotlin2/build.gradle.kts",
            "testdata/gradle-golden/kotlin/groovy-and-kotlin2/build.gradle.kts-expected.json",
        );
    }

    // =========================================================================
    // End-to-end (different JSON format - uses ScanCode output, not parser array)
    // =========================================================================

    #[test]
    fn test_golden_end2end() {
        run_golden(
            "testdata/gradle-golden/end2end/build.gradle",
            "testdata/gradle-golden/end2end/build.gradle-package-only-expected.json",
        );
    }

    #[test]
    fn test_golden_gradle_lockfile() {
        run_lock_golden(
            "testdata/gradle-lock/basic/gradle.lockfile",
            "testdata/gradle-lock/basic/gradle.lockfile.expected.json",
        );
    }
}
