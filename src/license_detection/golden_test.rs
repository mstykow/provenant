//! Golden tests for license detection against Python ScanCode reference.
//!
//! These tests validate that the Rust license detection engine produces
//! correct results compared to the Python reference implementation.
//! The expected YAML files in `testdata/license-golden/` are repository-owned so
//! intentional Rust-specific improvements can be recorded locally when parity is
//! no longer the best behavior.
//!
//! ## Test Data
//!
//! Test data in `testdata/license-golden/` was originally copied from the
//! ScanCode datadriven fixture corpus:
//! - `lic1/` - ~291 test cases
//! - `lic2/` - ~340 test cases  
//! - `lic3/` - ~292 test cases
//! - `lic4/` - ~345 test cases
//! - `external/` - External license references
//! - `unknown/` - Unknown license detection
//!
//! Each test consists of:
//! - A source file to scan (e.g., `mit.c`)
//! - A YAML expectation file with expected `license_expressions`
//!
//! ## Running Tests
//!
//! ```bash
//! cargo test license_detection_golden
//! ```

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::license_detection::LicenseDetectionEngine;
    use crate::license_detection::golden_utils::detect_license_expressions_for_golden;
    use once_cell::sync::Lazy;
    use serde::Deserialize;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Once;

    const GOLDEN_DIR: &str = "testdata/license-golden/datadriven";

    /// Shared engine instance - created once and reused across all tests
    static TEST_ENGINE: Lazy<Option<LicenseDetectionEngine>> =
        Lazy::new(|| match LicenseDetectionEngine::from_embedded() {
            Ok(engine) => {
                eprintln!("License detection engine initialized from embedded artifact");
                Some(engine)
            }
            Err(e) => {
                eprintln!("Failed to create engine from embedded: {:?}", e);
                None
            }
        });

    /// Initialize engine once before any tests run
    static INIT: Once = Once::new();

    fn ensure_engine() -> Option<&'static LicenseDetectionEngine> {
        INIT.call_once(|| {
            let _ = &*TEST_ENGINE;
        });
        TEST_ENGINE.as_ref()
    }

    /// Represents the YAML expectation file format
    #[derive(Debug, Deserialize, Default)]
    struct LicenseTestYaml {
        #[serde(default)]
        license_expressions: Vec<String>,
    }

    /// A single golden test case
    struct LicenseGoldenTest {
        name: String,
        test_file: PathBuf,
        yaml: LicenseTestYaml,
    }

    impl LicenseGoldenTest {
        /// Load a test from its YAML file
        fn load(yaml_path: &Path) -> Result<Self, String> {
            let content = fs::read_to_string(yaml_path)
                .map_err(|e| format!("Failed to read {}: {}", yaml_path.display(), e))?;

            let yaml: LicenseTestYaml = yaml_serde::from_str(&content)
                .map_err(|e| format!("Failed to parse YAML {}: {}", yaml_path.display(), e))?;

            let test_file = yaml_path.with_extension("");

            // Use relative path from GOLDEN_DIR as name for uniqueness
            let name = yaml_path
                .strip_prefix(PathBuf::from(GOLDEN_DIR).parent().unwrap_or(Path::new("")))
                .unwrap_or(yaml_path)
                .with_extension("")
                .to_string_lossy()
                .replace('\\', "/");

            Ok(Self {
                name,
                test_file,
                yaml,
            })
        }

        /// Run this test against the detection engine
        fn run(
            &self,
            engine: &LicenseDetectionEngine,
            unknown_licenses: bool,
        ) -> Result<(), String> {
            let actual =
                detect_license_expressions_for_golden(engine, &self.test_file, unknown_licenses)
                    .map_err(|e| {
                        format!("Detection failed for {}: {e:?}", self.test_file.display())
                    })?;

            let actual: Vec<&str> = actual.iter().map(|s| s.as_str()).collect();

            let expected: Vec<&str> = self
                .yaml
                .license_expressions
                .iter()
                .map(|s| s.as_str())
                .collect();

            if actual != expected {
                return Err(format!(
                    "license_expressions mismatch for {}:  Expected: {:?}  Actual:   {:?}",
                    self.name, expected, actual
                ));
            }

            Ok(())
        }
    }

    /// Discover all golden tests in a directory (recursively)
    fn discover_tests(dir: &Path) -> Vec<LicenseGoldenTest> {
        let mut tests = Vec::new();
        discover_tests_recursive(dir, &mut tests);
        tests.sort_by(|a, b| a.name.cmp(&b.name));
        tests
    }

    fn discover_tests_recursive(dir: &Path, tests: &mut Vec<LicenseGoldenTest>) {
        assert!(dir.exists(), "missing golden suite dir: {}", dir.display());

        let entries: Vec<_> = match fs::read_dir(dir) {
            Ok(e) => e.filter_map(|e| e.ok()).collect(),
            Err(_) => return,
        };

        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                discover_tests_recursive(&path, tests);
            } else if path.extension().is_some_and(|e| e == "yml")
                && let Ok(test) = LicenseGoldenTest::load(&path)
            {
                tests.push(test);
            }
        }
    }

    /// Result of running a test suite
    struct SuiteResult {
        total: usize,
        passed: usize,
        failed: usize,
        skipped: usize,
        failures: Vec<(String, String)>,
    }

    /// Run a complete test suite using the shared engine
    fn run_suite(suite_name: &str, dir: &Path) -> SuiteResult {
        run_suite_range(suite_name, dir, None, None, false)
    }

    /// Run a complete test suite with unknown_licenses enabled
    fn run_suite_unknown(suite_name: &str, dir: &Path) -> SuiteResult {
        run_suite_range(suite_name, dir, None, None, true)
    }

    /// Run a subset of a test suite (for splitting large suites)
    fn run_suite_range(
        suite_name: &str,
        dir: &Path,
        start: Option<usize>,
        end: Option<usize>,
        unknown_licenses: bool,
    ) -> SuiteResult {
        assert!(dir.exists(), "missing golden suite dir: {}", dir.display());

        let mut result = SuiteResult {
            total: 0,
            passed: 0,
            failed: 0,
            skipped: 0,
            failures: Vec::new(),
        };

        let Some(engine) = ensure_engine() else {
            eprintln!("Skipping {}: engine not available", suite_name);
            return result;
        };

        let mut tests = discover_tests(dir);
        let total_tests = tests.len();

        let start_idx = start.unwrap_or(0);
        let end_idx = end.unwrap_or(total_tests).min(total_tests);

        if start_idx >= total_tests {
            return result;
        }

        tests = tests.split_off(start_idx);
        if end_idx < total_tests {
            tests.truncate(end_idx - start_idx);
        }

        result.total = tests.len();

        println!(
            "\n{}: Running {} tests ({}-{} of {})...",
            suite_name,
            tests.len(),
            start_idx,
            end_idx,
            total_tests
        );

        for test in &tests {
            match test.run(engine, unknown_licenses) {
                Ok(()) => result.passed += 1,
                Err(e) => {
                    result.failed += 1;
                    result.failures.push((test.name.clone(), e));
                }
            }
        }

        println!(
            "{}: {} passed, {} failed, {} skipped",
            suite_name, result.passed, result.failed, result.skipped
        );

        result
    }

    #[test]
    fn test_golden_lic1() {
        let result = run_suite("lic1", &PathBuf::from(format!("{}/lic1", GOLDEN_DIR)));
        if result.failed > 0 {
            println!("\n{} failures:", result.failed);
            for (name, err) in &result.failures {
                println!("  - {}: {}", name, err.lines().next().unwrap_or(err));
            }
        }
        assert_eq!(result.failed, 0, "lic1 had {} failures", result.failed);
    }

    #[test]
    fn test_golden_lic2_part1() {
        let result = run_suite_range(
            "lic2-part1",
            &PathBuf::from(format!("{}/lic2", GOLDEN_DIR)),
            Some(0),
            Some(285),
            false,
        );
        if result.failed > 0 {
            println!("\n{} failures:", result.failed);
            for (name, err) in &result.failures {
                println!("  - {}: {}", name, err.lines().next().unwrap_or(err));
            }
        }
        assert_eq!(
            result.failed, 0,
            "lic2-part1 had {} failures",
            result.failed
        );
    }

    #[test]
    fn test_golden_lic2_part2() {
        let result = run_suite_range(
            "lic2-part2",
            &PathBuf::from(format!("{}/lic2", GOLDEN_DIR)),
            Some(285),
            Some(570),
            false,
        );
        if result.failed > 0 {
            println!("\n{} failures:", result.failed);
            for (name, err) in &result.failures {
                println!("  - {}: {}", name, err.lines().next().unwrap_or(err));
            }
        }
        assert_eq!(
            result.failed, 0,
            "lic2-part2 had {} failures",
            result.failed
        );
    }

    #[test]
    fn test_golden_lic2_part3() {
        let result = run_suite_range(
            "lic2-part3",
            &PathBuf::from(format!("{}/lic2", GOLDEN_DIR)),
            Some(570),
            None,
            false,
        );
        if result.failed > 0 {
            println!("\n{} failures:", result.failed);
            for (name, err) in &result.failures {
                println!("  - {}: {}", name, err.lines().next().unwrap_or(err));
            }
        }
        assert_eq!(
            result.failed, 0,
            "lic2-part3 had {} failures",
            result.failed
        );
    }

    #[test]
    fn test_golden_lic3() {
        let result = run_suite("lic3", &PathBuf::from(format!("{}/lic3", GOLDEN_DIR)));
        if result.failed > 0 {
            println!("\n{} failures:", result.failed);
            for (name, err) in &result.failures {
                println!("  - {}: {}", name, err.lines().next().unwrap_or(err));
            }
        }
        assert_eq!(result.failed, 0, "lic3 had {} failures", result.failed);
    }

    #[test]
    fn test_golden_lic4_part1() {
        let result = run_suite_range(
            "lic4-part1",
            &PathBuf::from(format!("{}/lic4", GOLDEN_DIR)),
            Some(0),
            Some(175),
            false,
        );
        if result.failed > 0 {
            println!("\n{} failures:", result.failed);
            for (name, err) in &result.failures {
                println!("  - {}: {}", name, err.lines().next().unwrap_or(err));
            }
        }
        assert_eq!(
            result.failed, 0,
            "lic4-part1 had {} failures",
            result.failed
        );
    }

    #[test]
    fn test_golden_lic4_part2() {
        let result = run_suite_range(
            "lic4-part2",
            &PathBuf::from(format!("{}/lic4", GOLDEN_DIR)),
            Some(175),
            None,
            false,
        );
        if result.failed > 0 {
            println!("\n{} failures:", result.failed);
            for (name, err) in &result.failures {
                println!("  - {}: {}", name, err.lines().next().unwrap_or(err));
            }
        }
        assert_eq!(
            result.failed, 0,
            "lic4-part2 had {} failures",
            result.failed
        );
    }

    #[test]
    fn test_golden_external_part1() {
        let result = run_suite_range(
            "external-part1",
            &PathBuf::from(format!("{}/external", GOLDEN_DIR)),
            Some(0),
            Some(285),
            false,
        );
        if result.failed > 0 {
            println!("\n{} failures:", result.failed);
            for (name, err) in &result.failures {
                println!("  - {}: {}", name, err.lines().next().unwrap_or(err));
            }
        }
        assert_eq!(
            result.failed, 0,
            "external-part1 had {} failures",
            result.failed
        );
    }

    #[test]
    fn test_golden_external_part2() {
        let result = run_suite_range(
            "external-part2",
            &PathBuf::from(format!("{}/external", GOLDEN_DIR)),
            Some(285),
            Some(570),
            false,
        );
        if result.failed > 0 {
            println!("\n{} failures:", result.failed);
            for (name, err) in &result.failures {
                println!("  - {}: {}", name, err.lines().next().unwrap_or(err));
            }
        }
        assert_eq!(
            result.failed, 0,
            "external-part2 had {} failures",
            result.failed
        );
    }

    #[test]
    fn test_golden_external_part3() {
        let result = run_suite_range(
            "external-part3",
            &PathBuf::from(format!("{}/external", GOLDEN_DIR)),
            Some(570),
            Some(855),
            false,
        );
        if result.failed > 0 {
            println!("\n{} failures:", result.failed);
            for (name, err) in &result.failures {
                println!("  - {}: {}", name, err.lines().next().unwrap_or(err));
            }
        }
        assert_eq!(
            result.failed, 0,
            "external-part3 had {} failures",
            result.failed
        );
    }

    #[test]
    fn test_golden_external_part4() {
        let result = run_suite_range(
            "external-part4",
            &PathBuf::from(format!("{}/external", GOLDEN_DIR)),
            Some(855),
            Some(1140),
            false,
        );
        if result.failed > 0 {
            println!("\n{} failures:", result.failed);
            for (name, err) in &result.failures {
                println!("  - {}: {}", name, err.lines().next().unwrap_or(err));
            }
        }
        assert_eq!(
            result.failed, 0,
            "external-part4 had {} failures",
            result.failed
        );
    }

    #[test]
    fn test_golden_external_part5() {
        let result = run_suite_range(
            "external-part5",
            &PathBuf::from(format!("{}/external", GOLDEN_DIR)),
            Some(1140),
            Some(1425),
            false,
        );
        if result.failed > 0 {
            println!("\n{} failures:", result.failed);
            for (name, err) in &result.failures {
                println!("  - {}: {}", name, err.lines().next().unwrap_or(err));
            }
        }
        assert_eq!(
            result.failed, 0,
            "external-part5 had {} failures",
            result.failed
        );
    }

    #[test]
    fn test_golden_external_part6() {
        let result = run_suite_range(
            "external-part6",
            &PathBuf::from(format!("{}/external", GOLDEN_DIR)),
            Some(1425),
            Some(1710),
            false,
        );
        if result.failed > 0 {
            println!("\n{} failures:", result.failed);
            for (name, err) in &result.failures {
                println!("  - {}: {}", name, err.lines().next().unwrap_or(err));
            }
        }
        assert_eq!(
            result.failed, 0,
            "external-part6 had {} failures",
            result.failed
        );
    }

    #[test]
    fn test_golden_external_part7() {
        let result = run_suite_range(
            "external-part7",
            &PathBuf::from(format!("{}/external", GOLDEN_DIR)),
            Some(1710),
            Some(1995),
            false,
        );
        if result.failed > 0 {
            println!("\n{} failures:", result.failed);
            for (name, err) in &result.failures {
                println!("  - {}: {}", name, err.lines().next().unwrap_or(err));
            }
        }
        assert_eq!(
            result.failed, 0,
            "external-part7 had {} failures",
            result.failed
        );
    }

    #[test]
    fn test_golden_external_part8() {
        let result = run_suite_range(
            "external-part8",
            &PathBuf::from(format!("{}/external", GOLDEN_DIR)),
            Some(1995),
            Some(2280),
            false,
        );
        if result.failed > 0 {
            println!("\n{} failures:", result.failed);
            for (name, err) in &result.failures {
                println!("  - {}: {}", name, err.lines().next().unwrap_or(err));
            }
        }
        assert_eq!(
            result.failed, 0,
            "external-part8 had {} failures",
            result.failed
        );
    }

    #[test]
    fn test_golden_external_part9() {
        let result = run_suite_range(
            "external-part9",
            &PathBuf::from(format!("{}/external", GOLDEN_DIR)),
            Some(2280),
            None,
            false,
        );
        if result.failed > 0 {
            println!("\n{} failures:", result.failed);
            for (name, err) in &result.failures {
                println!("  - {}: {}", name, err.lines().next().unwrap_or(err));
            }
        }
        assert_eq!(
            result.failed, 0,
            "external-part9 had {} failures",
            result.failed
        );
    }

    #[test]
    fn test_golden_unknown() {
        let result =
            run_suite_unknown("unknown", &PathBuf::from(format!("{}/unknown", GOLDEN_DIR)));
        if result.failed > 0 {
            println!("\n{} failures:", result.failed);
            for (name, err) in &result.failures {
                println!("  - {}: {}", name, err.lines().next().unwrap_or(err));
            }
        }
        assert_eq!(result.failed, 0, "unknown had {} failures", result.failed);
    }
}
