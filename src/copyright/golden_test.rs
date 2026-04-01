//! Golden tests for copyright detection.
//!
//! These tests load YAML expected outputs (copied from the Python ScanCode test
//! suite into `testdata/copyright-golden/`), run our Rust copyright detection on
//! the corresponding input files, and compare the results.
//!
//! The expected output files are owned by this repo so we can adjust them for
//! intentional differences (e.g., unicode name preservation, bug fixes).

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use rayon::prelude::*;
    use serde::Deserialize;

    use super::super::detect_copyrights;
    use super::super::golden_utils::{canonicalize_golden_value, read_input_content};

    /// Expected output structure matching Python ScanCode's YAML test format.
    #[derive(Debug, Deserialize, Default)]
    struct ExpectedOutput {
        what: Option<Vec<String>>,
        copyrights: Option<Vec<String>>,
        holders: Option<Vec<String>>,
        authors: Option<Vec<String>>,
    }

    struct FieldDiff {
        field: String,
        missing: Vec<String>,
        extra: Vec<String>,
    }

    struct GoldenTestCase {
        yaml_path: PathBuf,
        expected: ExpectedOutput,
        check_copyrights: bool,
        check_holders: bool,
        check_authors: bool,
    }

    impl FieldDiff {
        fn is_match(&self) -> bool {
            self.missing.is_empty() && self.extra.is_empty()
        }
    }

    impl std::fmt::Display for FieldDiff {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            writeln!(f, "  Field: {}", self.field)?;
            if !self.missing.is_empty() {
                writeln!(f, "    Missing (expected but not found):")?;
                for m in &self.missing {
                    writeln!(f, "      - {:?}", m)?;
                }
            }
            if !self.extra.is_empty() {
                writeln!(f, "    Extra (found but not expected):")?;
                for e in &self.extra {
                    writeln!(f, "      + {:?}", e)?;
                }
            }
            Ok(())
        }
    }

    fn compare_field_iter<'a, I>(field: &str, expected: &[String], actual: I) -> FieldDiff
    where
        I: IntoIterator<Item = &'a str>,
    {
        let expected_set: BTreeSet<String> = expected
            .iter()
            .map(|s| canonicalize_golden_value(s.as_str()))
            .collect();
        let actual_set: BTreeSet<String> =
            actual.into_iter().map(canonicalize_golden_value).collect();

        let missing: Vec<String> = expected_set
            .difference(&actual_set)
            .map(|s| s.to_string())
            .collect();
        let extra: Vec<String> = actual_set
            .difference(&expected_set)
            .map(|s| s.to_string())
            .collect();

        FieldDiff {
            field: field.to_string(),
            missing,
            extra,
        }
    }

    fn compare_field(field: &str, expected: &[String], actual: &[String]) -> FieldDiff {
        compare_field_iter(field, expected, actual.iter().map(String::as_str))
    }

    /// Discover all YAML test files in a directory (recursively).
    fn find_yaml_files(dir: &Path) -> Vec<PathBuf> {
        let mut yamls = Vec::new();
        if !dir.is_dir() {
            return yamls;
        }
        collect_yaml_files_recursive(dir, &mut yamls);
        yamls.sort();
        yamls
    }

    fn collect_yaml_files_recursive(dir: &Path, yamls: &mut Vec<PathBuf>) {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_yaml_files_recursive(&path, yamls);
            } else if path.extension().is_some_and(|ext| ext == "yml") {
                yamls.push(path);
            }
        }
    }

    /// Derive the input file path from a YAML expected output path.
    /// The convention is: input file = YAML path with `.yml` extension removed.
    fn input_path_from_yaml(yaml_path: &Path) -> PathBuf {
        let stem = yaml_path.to_string_lossy();
        let input = stem
            .strip_suffix(".yml")
            .expect("YAML path must end in .yml");
        PathBuf::from(input)
    }

    /// Run golden tests for all YAML files in the given test directory.
    fn run_golden_test(test_dir: &str) {
        let dir = PathBuf::from(test_dir);
        if !dir.is_dir() {
            eprintln!("Skipping golden test: directory not found: {}", test_dir);
            return;
        }

        let yaml_files = find_yaml_files(&dir);
        if yaml_files.is_empty() {
            eprintln!("Skipping golden test: no YAML files in {}", test_dir);
            return;
        }

        // Pre-filter to testable files and count skipped
        let mut test_cases: Vec<GoldenTestCase> = Vec::new();
        let mut skipped = 0usize;
        let mut setup_failures: Vec<(String, String)> = Vec::new();

        for yaml_path in &yaml_files {
            let input_path = input_path_from_yaml(yaml_path);
            if !input_path.is_file() {
                skipped += 1;
                continue;
            }

            let relative_path = yaml_path
                .strip_prefix(&dir)
                .unwrap_or(yaml_path)
                .to_string_lossy()
                .to_string();

            let yaml_content = match fs::read_to_string(yaml_path) {
                Ok(c) => c,
                Err(e) => {
                    setup_failures.push((
                        relative_path,
                        format!("YAML read error: {:?}\n  Error: {}", yaml_path, e),
                    ));
                    continue;
                }
            };

            let expected: ExpectedOutput = match yaml_serde::from_str(&yaml_content) {
                Ok(e) => e,
                Err(e) => {
                    setup_failures.push((
                        relative_path,
                        format!("YAML parse error: {:?}\n  Error: {}", yaml_path, e),
                    ));
                    continue;
                }
            };

            let what_fields = expected.what.as_deref().unwrap_or(&[]);
            let check_copyrights = what_fields.iter().any(|w| w == "copyrights");
            let check_holders = what_fields.iter().any(|w| w == "holders");
            let check_authors = what_fields.iter().any(|w| w == "authors");
            let has_check = check_copyrights || check_holders || check_authors;

            if !has_check {
                skipped += 1;
                continue;
            }

            test_cases.push(GoldenTestCase {
                yaml_path: yaml_path.clone(),
                expected,
                check_copyrights,
                check_holders,
                check_authors,
            });
        }

        let setup_failure_count = setup_failures.len();
        let total_test_cases = test_cases.len();
        let passed_count = AtomicUsize::new(0);
        let failures: Mutex<Vec<(String, String)>> = Mutex::new(setup_failures);

        // Run detection in parallel across all test cases
        test_cases.par_iter().for_each(|case| {
            let yaml_path = &case.yaml_path;
            let expected = &case.expected;
            let input_path = input_path_from_yaml(yaml_path);
            let relative_path = yaml_path
                .strip_prefix(&dir)
                .unwrap_or(yaml_path)
                .to_string_lossy()
                .to_string();

            let check_copyrights = case.check_copyrights;
            let check_holders = case.check_holders;
            let check_authors = case.check_authors;

            let content = match read_input_content(&input_path) {
                Ok(content) => content,
                Err(e) => {
                    failures.lock().unwrap().push((
                        relative_path,
                        format!("Input read error: {:?}\n  Error: {}", input_path, e),
                    ));
                    return;
                }
            };

            // Run detection
            let (copyrights, holders, authors) = detect_copyrights(&content);

            // Compare requested fields
            let mut field_diffs: Vec<FieldDiff> = Vec::new();

            if check_copyrights {
                let expected_copyrights = expected.copyrights.as_deref().unwrap_or(&[]);
                let diff = compare_field_iter(
                    "copyrights",
                    expected_copyrights,
                    copyrights.iter().map(|c| c.copyright.as_str()),
                );
                if !diff.is_match() {
                    field_diffs.push(diff);
                }
            }

            if check_holders {
                let expected_holders = expected.holders.as_deref().unwrap_or(&[]);
                let diff = compare_field_iter(
                    "holders",
                    expected_holders,
                    holders.iter().map(|h| h.holder.as_str()),
                );
                if !diff.is_match() {
                    field_diffs.push(diff);
                }
            }

            if check_authors {
                let expected_authors = expected.authors.as_deref().unwrap_or(&[]);
                let diff = compare_field_iter(
                    "authors",
                    expected_authors,
                    authors.iter().map(|a| a.author.as_str()),
                );
                if !diff.is_match() {
                    field_diffs.push(diff);
                }
            }

            if field_diffs.is_empty() {
                passed_count.fetch_add(1, Ordering::Relaxed);
            } else {
                let mut failure_msg = format!("FAIL: {}", relative_path);
                for diff in &field_diffs {
                    failure_msg.push_str(&format!("\n{}", diff));
                }
                failures.lock().unwrap().push((relative_path, failure_msg));
            }
        });

        let passed = passed_count.load(Ordering::Relaxed);
        let mut failures = failures.into_inner().unwrap();
        failures.sort_by(|a, b| a.0.cmp(&b.0));

        let failure_count = failures.len();

        let total = total_test_cases + setup_failure_count;

        let pass_rate = if total > 0 {
            (passed as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        // Print summary report
        eprintln!("\n{}", "=".repeat(60));
        eprintln!("Copyright Golden Test Report");
        eprintln!("{}", "=".repeat(60));
        eprintln!(
            "Total: {} | Passed: {} | Failed: {} | Skipped: {}",
            total, passed, failure_count, skipped
        );

        if !failures.is_empty() {
            eprintln!("\n--- Failures ({}) ---\n", failure_count);
            for (i, (_rel_path, failure)) in failures.iter().enumerate() {
                eprintln!("[{}/{}] {}\n", i + 1, failure_count, failure);
            }
        }

        eprintln!("\n{}\n", "=".repeat(60));

        assert!(
            failures.is_empty(),
            "{}/{} golden tests failed ({:.1}% pass rate). See failure details above.",
            failure_count,
            total,
            pass_rate,
        );
    }

    #[test]
    fn test_golden_copyrights() {
        run_golden_test("testdata/copyright-golden/copyrights");
    }

    #[test]
    fn test_golden_collective_authors_and_contributors() {
        run_golden_test("testdata/copyright-golden/collective-authors-and-contributors");
    }

    #[test]
    fn test_golden_portions_held_by_others() {
        run_golden_test("testdata/copyright-golden/portions-held-by-others");
    }

    #[test]
    fn test_golden_bare_inline_holders() {
        run_golden_test("testdata/copyright-golden/bare-inline-holders");
    }

    #[test]
    fn test_golden_assembly_year_only_copyrights() {
        run_golden_test("testdata/copyright-golden/assembly-year-only-copyrights");
    }

    #[test]
    fn test_fixture_transfig_with_parts() {
        let yaml_path = PathBuf::from(
            "testdata/copyright-golden/copyrights/transfig_with_parts-transfig.copyright.yml",
        );
        assert!(yaml_path.is_file(), "Missing fixture YAML: {yaml_path:?}");

        let yaml_content = fs::read_to_string(&yaml_path).expect("read YAML");
        let expected: ExpectedOutput = yaml_serde::from_str(&yaml_content).expect("parse YAML");

        let input_path = input_path_from_yaml(&yaml_path);
        assert!(
            input_path.is_file(),
            "Missing fixture input: {input_path:?}"
        );
        let content = read_input_content(&input_path).expect("read input");

        let (copyrights, holders, _authors) = detect_copyrights(&content);
        let actual_copyrights: Vec<String> =
            copyrights.iter().map(|c| c.copyright.clone()).collect();
        let actual_holders: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();

        let expected_copyrights = expected.copyrights.as_deref().unwrap_or(&[]);
        let expected_holders = expected.holders.as_deref().unwrap_or(&[]);

        let cr_diff = compare_field("copyrights", expected_copyrights, &actual_copyrights);
        let h_diff = compare_field("holders", expected_holders, &actual_holders);

        assert!(
            cr_diff.is_match() && h_diff.is_match(),
            "Fixture mismatch: transfig_with_parts\n{cr_diff}\n{h_diff}"
        );
    }

    #[test]
    fn test_fixture_copyrights_to_fix() {
        let yaml_path =
            PathBuf::from("testdata/copyright-golden/copyrights/copyrights-to-fix.txt.yml");
        assert!(yaml_path.is_file(), "Missing fixture YAML: {yaml_path:?}");

        let yaml_content = fs::read_to_string(&yaml_path).expect("read YAML");
        let expected: ExpectedOutput = yaml_serde::from_str(&yaml_content).expect("parse YAML");

        let input_path = input_path_from_yaml(&yaml_path);
        assert!(
            input_path.is_file(),
            "Missing fixture input: {input_path:?}"
        );
        let content = read_input_content(&input_path).expect("read input");

        let (copyrights, holders, authors) = detect_copyrights(&content);
        let actual_copyrights: Vec<String> =
            copyrights.iter().map(|c| c.copyright.clone()).collect();
        let actual_holders: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();
        let actual_authors: Vec<String> = authors.iter().map(|a| a.author.clone()).collect();

        let what_fields: Vec<String> = expected.what.clone().unwrap_or_default();
        let check_copyrights = what_fields.iter().any(|w| w == "copyrights");
        let check_holders = what_fields.iter().any(|w| w == "holders");
        let check_authors = what_fields.iter().any(|w| w == "authors");

        let mut diffs = Vec::new();
        if check_copyrights {
            diffs.push(compare_field(
                "copyrights",
                expected.copyrights.as_deref().unwrap_or(&[]),
                &actual_copyrights,
            ));
        }
        if check_holders {
            diffs.push(compare_field(
                "holders",
                expected.holders.as_deref().unwrap_or(&[]),
                &actual_holders,
            ));
        }
        if check_authors {
            diffs.push(compare_field(
                "authors",
                expected.authors.as_deref().unwrap_or(&[]),
                &actual_authors,
            ));
        }

        let all_match = diffs.iter().all(|d| d.is_match());
        assert!(
            all_match,
            "Fixture mismatch: copyrights-to-fix\n{}\n\nActual copyrights: {actual_copyrights:#?}\nActual holders: {actual_holders:#?}\nActual authors: {actual_authors:#?}",
            diffs
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn test_fixture_essential_smoke_ibm_c() {
        let yaml_path =
            PathBuf::from("testdata/copyright-golden/copyrights/essential_smoke-ibm_c.c.yml");
        assert!(yaml_path.is_file(), "Missing fixture YAML: {yaml_path:?}");

        let yaml_content = fs::read_to_string(&yaml_path).expect("read YAML");
        let expected: ExpectedOutput = yaml_serde::from_str(&yaml_content).expect("parse YAML");

        let input_path = input_path_from_yaml(&yaml_path);
        assert!(
            input_path.is_file(),
            "Missing fixture input: {input_path:?}"
        );
        let content = read_input_content(&input_path).expect("read input");

        let (copyrights, holders, authors) = detect_copyrights(&content);
        let actual_copyrights: Vec<String> =
            copyrights.iter().map(|c| c.copyright.clone()).collect();
        let actual_holders: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();
        let actual_authors: Vec<String> = authors.iter().map(|a| a.author.clone()).collect();

        let what_fields: Vec<String> = expected.what.clone().unwrap_or_default();
        let check_copyrights = what_fields.iter().any(|w| w == "copyrights");
        let check_holders = what_fields.iter().any(|w| w == "holders");
        let check_authors = what_fields.iter().any(|w| w == "authors");

        let mut diffs = Vec::new();
        if check_copyrights {
            diffs.push(compare_field(
                "copyrights",
                expected.copyrights.as_deref().unwrap_or(&[]),
                &actual_copyrights,
            ));
        }
        if check_holders {
            diffs.push(compare_field(
                "holders",
                expected.holders.as_deref().unwrap_or(&[]),
                &actual_holders,
            ));
        }
        if check_authors {
            diffs.push(compare_field(
                "authors",
                expected.authors.as_deref().unwrap_or(&[]),
                &actual_authors,
            ));
        }

        let all_match = diffs.iter().all(|d| d.is_match());
        assert!(
            all_match,
            "Fixture mismatch: essential_smoke-ibm_c\n{}\n\nActual copyrights: {actual_copyrights:#?}\nActual holders: {actual_holders:#?}\nActual authors: {actual_authors:#?}",
            diffs
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn test_fixture_co_cust_java() {
        let yaml_path = PathBuf::from("testdata/copyright-golden/copyrights/co_cust-java.java.yml");
        assert!(yaml_path.is_file(), "Missing fixture YAML: {yaml_path:?}");

        let yaml_content = fs::read_to_string(&yaml_path).expect("read YAML");
        let expected: ExpectedOutput = yaml_serde::from_str(&yaml_content).expect("parse YAML");

        let input_path = input_path_from_yaml(&yaml_path);
        assert!(
            input_path.is_file(),
            "Missing fixture input: {input_path:?}"
        );
        let content = read_input_content(&input_path).expect("read input");

        let (copyrights, holders, _authors) = detect_copyrights(&content);
        let actual_copyrights: Vec<String> =
            copyrights.iter().map(|c| c.copyright.clone()).collect();
        let actual_holders: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();

        let diffs = [
            compare_field(
                "copyrights",
                expected.copyrights.as_deref().unwrap_or(&[]),
                &actual_copyrights,
            ),
            compare_field(
                "holders",
                expected.holders.as_deref().unwrap_or(&[]),
                &actual_holders,
            ),
        ];

        let all_match = diffs.iter().all(|d| d.is_match());
        assert!(
            all_match,
            "Fixture mismatch: co_cust-java\n{}\n\nActual copyrights: {actual_copyrights:#?}\nActual holders: {actual_holders:#?}",
            diffs
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn test_fixture_no_holder_java() {
        let yaml_path =
            PathBuf::from("testdata/copyright-golden/copyrights/no_holder_java-java.java.yml");
        assert!(yaml_path.is_file(), "Missing fixture YAML: {yaml_path:?}");

        let yaml_content = fs::read_to_string(&yaml_path).expect("read YAML");
        let expected: ExpectedOutput = yaml_serde::from_str(&yaml_content).expect("parse YAML");

        let input_path = input_path_from_yaml(&yaml_path);
        assert!(
            input_path.is_file(),
            "Missing fixture input: {input_path:?}"
        );
        let content = read_input_content(&input_path).expect("read input");

        let (copyrights, holders, _authors) = detect_copyrights(&content);
        let actual_copyrights: Vec<String> =
            copyrights.iter().map(|c| c.copyright.clone()).collect();
        let actual_holders: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();

        let diffs = [
            compare_field(
                "copyrights",
                expected.copyrights.as_deref().unwrap_or(&[]),
                &actual_copyrights,
            ),
            compare_field(
                "holders",
                expected.holders.as_deref().unwrap_or(&[]),
                &actual_holders,
            ),
        ];

        let all_match = diffs.iter().all(|d| d.is_match());
        assert!(
            all_match,
            "Fixture mismatch: no_holder_java\n{}\n\nActual copyrights: {actual_copyrights:#?}\nActual holders: {actual_holders:#?}",
            diffs
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn test_fixture_eupl_copyrights_11() {
        let yaml_path =
            PathBuf::from("testdata/copyright-golden/copyrights/eupl-copyrights_11.txt.yml");
        assert!(yaml_path.is_file(), "Missing fixture YAML: {yaml_path:?}");

        let yaml_content = fs::read_to_string(&yaml_path).expect("read YAML");
        let expected: ExpectedOutput = yaml_serde::from_str(&yaml_content).expect("parse YAML");

        let input_path = input_path_from_yaml(&yaml_path);
        assert!(
            input_path.is_file(),
            "Missing fixture input: {input_path:?}"
        );
        let content = read_input_content(&input_path).expect("read input");

        let (copyrights, holders, authors) = detect_copyrights(&content);
        let actual_copyrights: Vec<String> =
            copyrights.iter().map(|c| c.copyright.clone()).collect();
        let actual_holders: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();
        let actual_authors: Vec<String> = authors.iter().map(|a| a.author.clone()).collect();

        let what_fields: Vec<String> = expected.what.clone().unwrap_or_default();
        let check_copyrights = what_fields.iter().any(|w| w == "copyrights");
        let check_holders = what_fields.iter().any(|w| w == "holders");
        let check_authors = what_fields.iter().any(|w| w == "authors");

        let mut diffs = Vec::new();
        if check_copyrights {
            diffs.push(compare_field(
                "copyrights",
                expected.copyrights.as_deref().unwrap_or(&[]),
                &actual_copyrights,
            ));
        }
        if check_holders {
            diffs.push(compare_field(
                "holders",
                expected.holders.as_deref().unwrap_or(&[]),
                &actual_holders,
            ));
        }
        if check_authors {
            diffs.push(compare_field(
                "authors",
                expected.authors.as_deref().unwrap_or(&[]),
                &actual_authors,
            ));
        }

        let all_match = diffs.iter().all(|d| d.is_match());
        assert!(
            all_match,
            "Fixture mismatch: eupl-copyrights_11\n{}\n\nActual copyrights: {actual_copyrights:#?}\nActual holders: {actual_holders:#?}\nActual authors: {actual_authors:#?}",
            diffs
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn test_fixture_eupl_copyrights_12() {
        let yaml_path =
            PathBuf::from("testdata/copyright-golden/copyrights/eupl-copyrights_12.txt.yml");
        assert!(yaml_path.is_file(), "Missing fixture YAML: {yaml_path:?}");

        let yaml_content = fs::read_to_string(&yaml_path).expect("read YAML");
        let expected: ExpectedOutput = yaml_serde::from_str(&yaml_content).expect("parse YAML");

        let input_path = input_path_from_yaml(&yaml_path);
        assert!(
            input_path.is_file(),
            "Missing fixture input: {input_path:?}"
        );
        let content = read_input_content(&input_path).expect("read input");

        let (copyrights, holders, authors) = detect_copyrights(&content);
        let actual_copyrights: Vec<String> =
            copyrights.iter().map(|c| c.copyright.clone()).collect();
        let actual_holders: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();
        let actual_authors: Vec<String> = authors.iter().map(|a| a.author.clone()).collect();

        let what_fields: Vec<String> = expected.what.clone().unwrap_or_default();
        let check_copyrights = what_fields.iter().any(|w| w == "copyrights");
        let check_holders = what_fields.iter().any(|w| w == "holders");
        let check_authors = what_fields.iter().any(|w| w == "authors");

        let mut diffs = Vec::new();
        if check_copyrights {
            diffs.push(compare_field(
                "copyrights",
                expected.copyrights.as_deref().unwrap_or(&[]),
                &actual_copyrights,
            ));
        }
        if check_holders {
            diffs.push(compare_field(
                "holders",
                expected.holders.as_deref().unwrap_or(&[]),
                &actual_holders,
            ));
        }
        if check_authors {
            diffs.push(compare_field(
                "authors",
                expected.authors.as_deref().unwrap_or(&[]),
                &actual_authors,
            ));
        }

        let all_match = diffs.iter().all(|d| d.is_match());
        assert!(
            all_match,
            "Fixture mismatch: eupl-copyrights_12\n{}\n\nActual copyrights: {actual_copyrights:#?}\nActual holders: {actual_holders:#?}\nActual authors: {actual_authors:#?}",
            diffs
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn test_fixture_misco2_eupl() {
        let yaml_path = PathBuf::from("testdata/copyright-golden/copyrights/misco2/eupl.txt.yml");
        assert!(yaml_path.is_file(), "Missing fixture YAML: {yaml_path:?}");

        let yaml_content = fs::read_to_string(&yaml_path).expect("read YAML");
        let expected: ExpectedOutput = yaml_serde::from_str(&yaml_content).expect("parse YAML");

        let input_path = input_path_from_yaml(&yaml_path);
        assert!(
            input_path.is_file(),
            "Missing fixture input: {input_path:?}"
        );
        let content = read_input_content(&input_path).expect("read input");

        let (copyrights, holders, authors) = detect_copyrights(&content);
        let actual_copyrights: Vec<String> =
            copyrights.iter().map(|c| c.copyright.clone()).collect();
        let actual_holders: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();
        let actual_authors: Vec<String> = authors.iter().map(|a| a.author.clone()).collect();

        let what_fields: Vec<String> = expected.what.clone().unwrap_or_default();
        let check_copyrights = what_fields.iter().any(|w| w == "copyrights");
        let check_holders = what_fields.iter().any(|w| w == "holders");
        let check_authors = what_fields.iter().any(|w| w == "authors");

        let mut diffs = Vec::new();
        if check_copyrights {
            diffs.push(compare_field(
                "copyrights",
                expected.copyrights.as_deref().unwrap_or(&[]),
                &actual_copyrights,
            ));
        }
        if check_holders {
            diffs.push(compare_field(
                "holders",
                expected.holders.as_deref().unwrap_or(&[]),
                &actual_holders,
            ));
        }
        if check_authors {
            diffs.push(compare_field(
                "authors",
                expected.authors.as_deref().unwrap_or(&[]),
                &actual_authors,
            ));
        }

        let all_match = diffs.iter().all(|d| d.is_match());
        assert!(
            all_match,
            "Fixture mismatch: misco2/eupl\n{}\n\nActual copyrights: {actual_copyrights:#?}\nActual holders: {actual_holders:#?}\nActual authors: {actual_authors:#?}",
            diffs
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn test_fixture_frameworxv1_0() {
        let yaml_path =
            PathBuf::from("testdata/copyright-golden/copyrights/frameworxv1_0-Frameworxv.0.yml");
        assert!(yaml_path.is_file(), "Missing fixture YAML: {yaml_path:?}");

        let yaml_content = fs::read_to_string(&yaml_path).expect("read YAML");
        let expected: ExpectedOutput = yaml_serde::from_str(&yaml_content).expect("parse YAML");

        let input_path = input_path_from_yaml(&yaml_path);
        assert!(
            input_path.is_file(),
            "Missing fixture input: {input_path:?}"
        );
        let content = read_input_content(&input_path).expect("read input");

        let (copyrights, holders, _authors) = detect_copyrights(&content);
        let actual_copyrights: Vec<String> =
            copyrights.iter().map(|c| c.copyright.clone()).collect();
        let actual_holders: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();

        let diffs = [
            compare_field(
                "copyrights",
                expected.copyrights.as_deref().unwrap_or(&[]),
                &actual_copyrights,
            ),
            compare_field(
                "holders",
                expected.holders.as_deref().unwrap_or(&[]),
                &actual_holders,
            ),
        ];

        let all_match = diffs.iter().all(|d| d.is_match());
        assert!(
            all_match,
            "Fixture mismatch: frameworxv1_0\n{}\n\nActual copyrights: {actual_copyrights:#?}\nActual holders: {actual_holders:#?}",
            diffs
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn test_fixture_misco2_distributed_5() {
        let yaml_path =
            PathBuf::from("testdata/copyright-golden/copyrights/misco2/distributed_5.txt.yml");
        assert!(yaml_path.is_file(), "Missing fixture YAML: {yaml_path:?}");

        let yaml_content = fs::read_to_string(&yaml_path).expect("read YAML");
        let expected: ExpectedOutput = yaml_serde::from_str(&yaml_content).expect("parse YAML");

        let input_path = input_path_from_yaml(&yaml_path);
        assert!(
            input_path.is_file(),
            "Missing fixture input: {input_path:?}"
        );
        let content = read_input_content(&input_path).expect("read input");

        let (copyrights, holders, _authors) = detect_copyrights(&content);
        let actual_copyrights: Vec<String> =
            copyrights.iter().map(|c| c.copyright.clone()).collect();
        let actual_holders: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();

        let diffs = [
            compare_field(
                "copyrights",
                expected.copyrights.as_deref().unwrap_or(&[]),
                &actual_copyrights,
            ),
            compare_field(
                "holders",
                expected.holders.as_deref().unwrap_or(&[]),
                &actual_holders,
            ),
        ];

        let all_match = diffs.iter().all(|d| d.is_match());
        assert!(
            all_match,
            "Fixture mismatch: misco2/distributed_5\n{}\n\nActual copyrights: {actual_copyrights:#?}\nActual holders: {actual_holders:#?}",
            diffs
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn test_fixture_misco2_distributed_8() {
        let yaml_path =
            PathBuf::from("testdata/copyright-golden/copyrights/misco2/distributed_8.txt.yml");
        assert!(yaml_path.is_file(), "Missing fixture YAML: {yaml_path:?}");

        let yaml_content = fs::read_to_string(&yaml_path).expect("read YAML");
        let expected: ExpectedOutput = yaml_serde::from_str(&yaml_content).expect("parse YAML");

        let input_path = input_path_from_yaml(&yaml_path);
        assert!(
            input_path.is_file(),
            "Missing fixture input: {input_path:?}"
        );
        let content = read_input_content(&input_path).expect("read input");

        let (copyrights, holders, _authors) = detect_copyrights(&content);
        let actual_copyrights: Vec<String> =
            copyrights.iter().map(|c| c.copyright.clone()).collect();
        let actual_holders: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();

        let diffs = [
            compare_field(
                "copyrights",
                expected.copyrights.as_deref().unwrap_or(&[]),
                &actual_copyrights,
            ),
            compare_field(
                "holders",
                expected.holders.as_deref().unwrap_or(&[]),
                &actual_holders,
            ),
        ];

        let all_match = diffs.iter().all(|d| d.is_match());
        assert!(
            all_match,
            "Fixture mismatch: misco2/distributed_8\n{}\n\nActual copyrights: {actual_copyrights:#?}\nActual holders: {actual_holders:#?}",
            diffs
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn test_fixture_misco2_its_authors() {
        let yaml_path =
            PathBuf::from("testdata/copyright-golden/copyrights/misco2/its-authors.txt.yml");
        assert!(yaml_path.is_file(), "Missing fixture YAML: {yaml_path:?}");

        let yaml_content = fs::read_to_string(&yaml_path).expect("read YAML");
        let expected: ExpectedOutput = yaml_serde::from_str(&yaml_content).expect("parse YAML");

        let input_path = input_path_from_yaml(&yaml_path);
        assert!(
            input_path.is_file(),
            "Missing fixture input: {input_path:?}"
        );
        let content = read_input_content(&input_path).expect("read input");

        let (copyrights, holders, _authors) = detect_copyrights(&content);
        let actual_copyrights: Vec<String> =
            copyrights.iter().map(|c| c.copyright.clone()).collect();
        let actual_holders: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();

        let diffs = [
            compare_field(
                "copyrights",
                expected.copyrights.as_deref().unwrap_or(&[]),
                &actual_copyrights,
            ),
            compare_field(
                "holders",
                expected.holders.as_deref().unwrap_or(&[]),
                &actual_holders,
            ),
        ];

        let all_match = diffs.iter().all(|d| d.is_match());
        assert!(
            all_match,
            "Fixture mismatch: misco2/its-authors\n{}\n\nActual copyrights: {actual_copyrights:#?}\nActual holders: {actual_holders:#?}",
            diffs
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn test_fixture_misco2_xz64() {
        let yaml_path = PathBuf::from("testdata/copyright-golden/copyrights/misco2/xz64.txt.yml");
        assert!(yaml_path.is_file(), "Missing fixture YAML: {yaml_path:?}");

        let yaml_content = fs::read_to_string(&yaml_path).expect("read YAML");
        let expected: ExpectedOutput = yaml_serde::from_str(&yaml_content).expect("parse YAML");

        let input_path = input_path_from_yaml(&yaml_path);
        assert!(
            input_path.is_file(),
            "Missing fixture input: {input_path:?}"
        );
        let content = read_input_content(&input_path).expect("read input");

        let (copyrights, holders, _authors) = detect_copyrights(&content);
        let actual_copyrights: Vec<String> =
            copyrights.iter().map(|c| c.copyright.clone()).collect();
        let actual_holders: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();

        let diffs = [
            compare_field(
                "copyrights",
                expected.copyrights.as_deref().unwrap_or(&[]),
                &actual_copyrights,
            ),
            compare_field(
                "holders",
                expected.holders.as_deref().unwrap_or(&[]),
                &actual_holders,
            ),
        ];

        let all_match = diffs.iter().all(|d| d.is_match());
        assert!(
            all_match,
            "Fixture mismatch: misco2/xz64\n{}\n\nActual copyrights: {actual_copyrights:#?}\nActual holders: {actual_holders:#?}",
            diffs
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn test_fixture_misco2_regexhq_036() {
        let yaml_path = PathBuf::from(
            "testdata/copyright-golden/copyrights/misco2/regexhq/regexhq-036.txt.yml",
        );
        assert!(yaml_path.is_file(), "Missing fixture YAML: {yaml_path:?}");

        let yaml_content = fs::read_to_string(&yaml_path).expect("read YAML");
        let expected: ExpectedOutput = yaml_serde::from_str(&yaml_content).expect("parse YAML");

        let input_path = input_path_from_yaml(&yaml_path);
        assert!(
            input_path.is_file(),
            "Missing fixture input: {input_path:?}"
        );
        let content = read_input_content(&input_path).expect("read input");

        let (copyrights, holders, _authors) = detect_copyrights(&content);
        let actual_copyrights: Vec<String> =
            copyrights.iter().map(|c| c.copyright.clone()).collect();
        let actual_holders: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();

        let diffs = [
            compare_field(
                "copyrights",
                expected.copyrights.as_deref().unwrap_or(&[]),
                &actual_copyrights,
            ),
            compare_field(
                "holders",
                expected.holders.as_deref().unwrap_or(&[]),
                &actual_holders,
            ),
        ];

        let all_match = diffs.iter().all(|d| d.is_match());
        assert!(
            all_match,
            "Fixture mismatch: misco2/regexhq-036\n{}\n\nActual copyrights: {actual_copyrights:#?}\nActual holders: {actual_holders:#?}",
            diffs
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn test_fixture_misco2_regexhq_194() {
        let yaml_path = PathBuf::from(
            "testdata/copyright-golden/copyrights/misco2/regexhq/regexhq-194.txt.yml",
        );
        assert!(yaml_path.is_file(), "Missing fixture YAML: {yaml_path:?}");

        let yaml_content = fs::read_to_string(&yaml_path).expect("read YAML");
        let expected: ExpectedOutput = yaml_serde::from_str(&yaml_content).expect("parse YAML");

        let input_path = input_path_from_yaml(&yaml_path);
        assert!(
            input_path.is_file(),
            "Missing fixture input: {input_path:?}"
        );
        let content = read_input_content(&input_path).expect("read input");

        let (copyrights, holders, _authors) = detect_copyrights(&content);
        let actual_copyrights: Vec<String> =
            copyrights.iter().map(|c| c.copyright.clone()).collect();
        let actual_holders: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();

        let diffs = [
            compare_field(
                "copyrights",
                expected.copyrights.as_deref().unwrap_or(&[]),
                &actual_copyrights,
            ),
            compare_field(
                "holders",
                expected.holders.as_deref().unwrap_or(&[]),
                &actual_holders,
            ),
        ];

        let all_match = diffs.iter().all(|d| d.is_match());
        assert!(
            all_match,
            "Fixture mismatch: misco2/regexhq-194\n{}\n\nActual copyrights: {actual_copyrights:#?}\nActual holders: {actual_holders:#?}",
            diffs
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn test_fixture_misco2_font_awesome() {
        let yaml_path =
            PathBuf::from("testdata/copyright-golden/copyrights/misco2/font-awesome.txt.yml");
        assert!(yaml_path.is_file(), "Missing fixture YAML: {yaml_path:?}");

        let yaml_content = fs::read_to_string(&yaml_path).expect("read YAML");
        let expected: ExpectedOutput = yaml_serde::from_str(&yaml_content).expect("parse YAML");

        let input_path = input_path_from_yaml(&yaml_path);
        assert!(
            input_path.is_file(),
            "Missing fixture input: {input_path:?}"
        );
        let content = read_input_content(&input_path).expect("read input");

        let (copyrights, holders, _authors) = detect_copyrights(&content);
        let actual_copyrights: Vec<String> =
            copyrights.iter().map(|c| c.copyright.clone()).collect();
        let actual_holders: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();

        let diffs = [
            compare_field(
                "copyrights",
                expected.copyrights.as_deref().unwrap_or(&[]),
                &actual_copyrights,
            ),
            compare_field(
                "holders",
                expected.holders.as_deref().unwrap_or(&[]),
                &actual_holders,
            ),
        ];

        let all_match = diffs.iter().all(|d| d.is_match());
        assert!(
            all_match,
            "Fixture mismatch: misco2/font-awesome\n{}\n\nActual copyrights: {actual_copyrights:#?}\nActual holders: {actual_holders:#?}",
            diffs
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn test_fixture_misco4_freien() {
        let yaml_path = PathBuf::from("testdata/copyright-golden/copyrights/misco4/freien.txt.yml");
        assert!(yaml_path.is_file(), "Missing fixture YAML: {yaml_path:?}");

        let yaml_content = fs::read_to_string(&yaml_path).expect("read YAML");
        let expected: ExpectedOutput = yaml_serde::from_str(&yaml_content).expect("parse YAML");

        let input_path = input_path_from_yaml(&yaml_path);
        assert!(
            input_path.is_file(),
            "Missing fixture input: {input_path:?}"
        );
        let content = read_input_content(&input_path).expect("read input");

        let (_copyrights, _holders, authors) = detect_copyrights(&content);
        let actual_authors: Vec<String> = authors.iter().map(|a| a.author.clone()).collect();

        let diff = compare_field(
            "authors",
            expected.authors.as_deref().unwrap_or(&[]),
            &actual_authors,
        );
        assert!(
            diff.is_match(),
            "Fixture mismatch: misco4/freien\n{diff}\n\nActual authors: {actual_authors:#?}"
        );
    }

    #[test]
    fn test_fixture_misco3_not_real_copyrights() {
        let yaml_path =
            PathBuf::from("testdata/copyright-golden/copyrights/misco3/not-real-copyrights.yml");
        assert!(yaml_path.is_file(), "Missing fixture YAML: {yaml_path:?}");

        let yaml_content = fs::read_to_string(&yaml_path).expect("read YAML");
        let expected: ExpectedOutput = yaml_serde::from_str(&yaml_content).expect("parse YAML");

        let input_path = input_path_from_yaml(&yaml_path);
        assert!(
            input_path.is_file(),
            "Missing fixture input: {input_path:?}"
        );
        let content = read_input_content(&input_path).expect("read input");

        let (copyrights, holders, _authors) = detect_copyrights(&content);
        let actual_copyrights: Vec<String> =
            copyrights.iter().map(|c| c.copyright.clone()).collect();
        let actual_holders: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();

        let diffs = [
            compare_field(
                "copyrights",
                expected.copyrights.as_deref().unwrap_or(&[]),
                &actual_copyrights,
            ),
            compare_field(
                "holders",
                expected.holders.as_deref().unwrap_or(&[]),
                &actual_holders,
            ),
        ];

        let all_match = diffs.iter().all(|d| d.is_match());
        assert!(
            all_match,
            "Fixture mismatch: misco3/not-real-copyrights\n{}\n\nActual copyrights: {actual_copyrights:#?}\nActual holders: {actual_holders:#?}",
            diffs
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn test_fixture_junk_copyright_64() {
        let yaml_path = PathBuf::from(
            "testdata/copyright-golden/copyrights/misco4/to_improve/junk-copyright-64.txt.yml",
        );
        assert!(yaml_path.is_file(), "Missing fixture YAML: {yaml_path:?}");

        let yaml_content = fs::read_to_string(&yaml_path).expect("read YAML");
        let expected: ExpectedOutput = yaml_serde::from_str(&yaml_content).expect("parse YAML");

        let input_path = input_path_from_yaml(&yaml_path);
        assert!(
            input_path.is_file(),
            "Missing fixture input: {input_path:?}"
        );
        let content = read_input_content(&input_path).expect("read input");

        let (copyrights, holders, _authors) = detect_copyrights(&content);
        let actual_copyrights: Vec<String> =
            copyrights.iter().map(|c| c.copyright.clone()).collect();
        let actual_holders: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();

        let diffs = [
            compare_field(
                "copyrights",
                expected.copyrights.as_deref().unwrap_or(&[]),
                &actual_copyrights,
            ),
            compare_field(
                "holders",
                expected.holders.as_deref().unwrap_or(&[]),
                &actual_holders,
            ),
        ];

        let all_match = diffs.iter().all(|d| d.is_match());
        assert!(
            all_match,
            "Fixture mismatch: junk-copyright-64\n{}\n\nActual copyrights: {actual_copyrights:#?}\nActual holders: {actual_holders:#?}",
            diffs
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn test_fixture_junk_copyright_290() {
        let yaml_path = PathBuf::from(
            "testdata/copyright-golden/copyrights/misco4/to_improve/junk-copyright-290.txt.yml",
        );
        assert!(yaml_path.is_file(), "Missing fixture YAML: {yaml_path:?}");

        let yaml_content = fs::read_to_string(&yaml_path).expect("read YAML");
        let expected: ExpectedOutput = yaml_serde::from_str(&yaml_content).expect("parse YAML");

        let input_path = input_path_from_yaml(&yaml_path);
        assert!(
            input_path.is_file(),
            "Missing fixture input: {input_path:?}"
        );
        let content = read_input_content(&input_path).expect("read input");

        let (copyrights, holders, _authors) = detect_copyrights(&content);
        let actual_copyrights: Vec<String> =
            copyrights.iter().map(|c| c.copyright.clone()).collect();
        let actual_holders: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();

        let diffs = [
            compare_field(
                "copyrights",
                expected.copyrights.as_deref().unwrap_or(&[]),
                &actual_copyrights,
            ),
            compare_field(
                "holders",
                expected.holders.as_deref().unwrap_or(&[]),
                &actual_holders,
            ),
        ];

        let all_match = diffs.iter().all(|d| d.is_match());
        assert!(
            all_match,
            "Fixture mismatch: junk-copyright-290\n{}\n\nActual copyrights: {actual_copyrights:#?}\nActual holders: {actual_holders:#?}",
            diffs
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn test_fixture_misco3_correct_copyright_minpack() {
        let yaml_path = PathBuf::from(
            "testdata/copyright-golden/copyrights/misco3/correct-copyright-minpack.txt.yml",
        );
        assert!(yaml_path.is_file(), "Missing fixture YAML: {yaml_path:?}");

        let yaml_content = fs::read_to_string(&yaml_path).expect("read YAML");
        let expected: ExpectedOutput = yaml_serde::from_str(&yaml_content).expect("parse YAML");

        let input_path = input_path_from_yaml(&yaml_path);
        assert!(
            input_path.is_file(),
            "Missing fixture input: {input_path:?}"
        );
        let content = read_input_content(&input_path).expect("read input");

        let (copyrights, holders, _authors) = detect_copyrights(&content);
        let actual_copyrights: Vec<String> =
            copyrights.iter().map(|c| c.copyright.clone()).collect();
        let actual_holders: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();

        let diffs = [
            compare_field(
                "copyrights",
                expected.copyrights.as_deref().unwrap_or(&[]),
                &actual_copyrights,
            ),
            compare_field(
                "holders",
                expected.holders.as_deref().unwrap_or(&[]),
                &actual_holders,
            ),
        ];

        let all_match = diffs.iter().all(|d| d.is_match());
        assert!(
            all_match,
            "Fixture mismatch: misco3/correct-copyright-minpack\n{}\n\nActual copyrights: {actual_copyrights:#?}\nActual holders: {actual_holders:#?}",
            diffs
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn test_fixture_linux_with_add() {
        let yaml_path =
            PathBuf::from("testdata/copyright-golden/copyrights/misco4/linux/with-add.txt.yml");
        assert!(yaml_path.is_file(), "Missing fixture YAML: {yaml_path:?}");

        let yaml_content = fs::read_to_string(&yaml_path).expect("read YAML");
        let expected: ExpectedOutput = yaml_serde::from_str(&yaml_content).expect("parse YAML");

        let input_path = input_path_from_yaml(&yaml_path);
        assert!(
            input_path.is_file(),
            "Missing fixture input: {input_path:?}"
        );
        let content = read_input_content(&input_path).expect("read input");

        let (copyrights, holders, authors) = detect_copyrights(&content);
        let actual_copyrights: Vec<String> =
            copyrights.iter().map(|c| c.copyright.clone()).collect();
        let actual_holders: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();
        let actual_authors: Vec<String> = authors.iter().map(|a| a.author.clone()).collect();

        let diffs = [
            compare_field(
                "copyrights",
                expected.copyrights.as_deref().unwrap_or(&[]),
                &actual_copyrights,
            ),
            compare_field(
                "holders",
                expected.holders.as_deref().unwrap_or(&[]),
                &actual_holders,
            ),
            compare_field(
                "authors",
                expected.authors.as_deref().unwrap_or(&[]),
                &actual_authors,
            ),
        ];

        let all_match = diffs.iter().all(|d| d.is_match());
        assert!(
            all_match,
            "Fixture mismatch: misco4/linux/with-add\n{}\n\nActual copyrights: {actual_copyrights:#?}\nActual holders: {actual_holders:#?}\nActual authors: {actual_authors:#?}",
            diffs
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn test_fixture_linux5_susecaps() {
        let yaml_path =
            PathBuf::from("testdata/copyright-golden/copyrights/misco4/linux5/susecaps.txt.yml");
        assert!(yaml_path.is_file(), "Missing fixture YAML: {yaml_path:?}");

        let yaml_content = fs::read_to_string(&yaml_path).expect("read YAML");
        let expected: ExpectedOutput = yaml_serde::from_str(&yaml_content).expect("parse YAML");

        let input_path = input_path_from_yaml(&yaml_path);
        assert!(
            input_path.is_file(),
            "Missing fixture input: {input_path:?}"
        );
        let content = read_input_content(&input_path).expect("read input");

        let numbered_lines: Vec<(usize, String)> = content
            .lines()
            .enumerate()
            .map(|(i, line)| (i + 1, line.to_string()))
            .collect();
        let groups = crate::copyright::candidates::collect_candidate_lines(numbered_lines);
        let tokens: Vec<crate::copyright::types::Token> = groups
            .first()
            .map(|g| crate::copyright::lexer::get_tokens(g))
            .unwrap_or_default();
        let tree = if tokens.is_empty() {
            Vec::new()
        } else {
            crate::copyright::parser::parse(tokens.clone())
        };

        let (copyrights, holders, _authors) = detect_copyrights(&content);
        let actual_copyrights: Vec<String> =
            copyrights.iter().map(|c| c.copyright.clone()).collect();
        let actual_holders: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();

        let diffs = [
            compare_field(
                "copyrights",
                expected.copyrights.as_deref().unwrap_or(&[]),
                &actual_copyrights,
            ),
            compare_field(
                "holders",
                expected.holders.as_deref().unwrap_or(&[]),
                &actual_holders,
            ),
        ];

        let all_match = diffs.iter().all(|d| d.is_match());
        assert!(
            all_match,
            "Fixture mismatch: misco4/linux5/susecaps\n{}\n\nActual copyrights: {actual_copyrights:#?}\nActual holders: {actual_holders:#?}\n\nGroups: {groups:#?}\n\nTokens: {tokens:#?}\n\nTree: {tree:#?}",
            diffs
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn test_fixture_more_linux_following() {
        let yaml_path = PathBuf::from(
            "testdata/copyright-golden/copyrights/misco4/more-linux/following.txt.yml",
        );
        assert!(yaml_path.is_file(), "Missing fixture YAML: {yaml_path:?}");

        let yaml_content = fs::read_to_string(&yaml_path).expect("read YAML");
        let expected: ExpectedOutput = yaml_serde::from_str(&yaml_content).expect("parse YAML");

        let input_path = input_path_from_yaml(&yaml_path);
        assert!(
            input_path.is_file(),
            "Missing fixture input: {input_path:?}"
        );
        let content = read_input_content(&input_path).expect("read input");

        let (copyrights, holders, authors) = detect_copyrights(&content);
        let actual_copyrights: Vec<String> =
            copyrights.iter().map(|c| c.copyright.clone()).collect();
        let actual_holders: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();
        let actual_authors: Vec<String> = authors.iter().map(|a| a.author.clone()).collect();

        let what_fields: Vec<String> = expected.what.clone().unwrap_or_default();
        let check_copyrights = what_fields.iter().any(|w| w == "copyrights");
        let check_holders = what_fields.iter().any(|w| w == "holders");
        let check_authors = what_fields.iter().any(|w| w == "authors");

        let mut diffs = Vec::new();
        if check_copyrights {
            diffs.push(compare_field(
                "copyrights",
                expected.copyrights.as_deref().unwrap_or(&[]),
                &actual_copyrights,
            ));
        }
        if check_holders {
            diffs.push(compare_field(
                "holders",
                expected.holders.as_deref().unwrap_or(&[]),
                &actual_holders,
            ));
        }
        if check_authors {
            diffs.push(compare_field(
                "authors",
                expected.authors.as_deref().unwrap_or(&[]),
                &actual_authors,
            ));
        }

        let all_match = diffs.iter().all(|d| d.is_match());
        assert!(
            all_match,
            "Fixture mismatch: misco4/more-linux/following\n{}\n\nActual copyrights: {actual_copyrights:#?}\nActual holders: {actual_holders:#?}\nActual authors: {actual_authors:#?}",
            diffs
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn test_fixture_more_linux_ilia() {
        let yaml_path =
            PathBuf::from("testdata/copyright-golden/copyrights/misco4/more-linux/ilia.txt.yml");
        assert!(yaml_path.is_file(), "Missing fixture YAML: {yaml_path:?}");

        let yaml_content = fs::read_to_string(&yaml_path).expect("read YAML");
        let expected: ExpectedOutput = yaml_serde::from_str(&yaml_content).expect("parse YAML");

        let input_path = input_path_from_yaml(&yaml_path);
        assert!(
            input_path.is_file(),
            "Missing fixture input: {input_path:?}"
        );
        let content = read_input_content(&input_path).expect("read input");

        let (copyrights, holders, authors) = detect_copyrights(&content);
        let actual_copyrights: Vec<String> =
            copyrights.iter().map(|c| c.copyright.clone()).collect();
        let actual_holders: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();
        let actual_authors: Vec<String> = authors.iter().map(|a| a.author.clone()).collect();

        let what_fields: Vec<String> = expected.what.clone().unwrap_or_default();
        let check_copyrights = what_fields.iter().any(|w| w == "copyrights");
        let check_holders = what_fields.iter().any(|w| w == "holders");
        let check_authors = what_fields.iter().any(|w| w == "authors");

        let mut diffs = Vec::new();
        if check_copyrights {
            diffs.push(compare_field(
                "copyrights",
                expected.copyrights.as_deref().unwrap_or(&[]),
                &actual_copyrights,
            ));
        }
        if check_holders {
            diffs.push(compare_field(
                "holders",
                expected.holders.as_deref().unwrap_or(&[]),
                &actual_holders,
            ));
        }
        if check_authors {
            diffs.push(compare_field(
                "authors",
                expected.authors.as_deref().unwrap_or(&[]),
                &actual_authors,
            ));
        }

        let all_match = diffs.iter().all(|d| d.is_match());
        assert!(
            all_match,
            "Fixture mismatch: misco4/more-linux/ilia\n{}\n\nActual copyrights: {actual_copyrights:#?}\nActual holders: {actual_holders:#?}\nActual authors: {actual_authors:#?}",
            diffs
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn test_golden_holders() {
        run_golden_test("testdata/copyright-golden/holders");
    }

    #[test]
    fn test_golden_authors() {
        run_golden_test("testdata/copyright-golden/authors");
    }

    #[test]
    fn test_golden_ics() {
        run_golden_test("testdata/copyright-golden/ics");
    }

    #[test]
    fn test_golden_non_detection_boilerplate() {
        run_golden_test("testdata/copyright-golden/non-detection-boilerplate");
    }

    #[test]
    fn test_golden_all_rights_reserved() {
        run_golden_test("testdata/copyright-golden/all-rights-reserved");
    }
}
