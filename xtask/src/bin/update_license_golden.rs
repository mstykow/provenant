use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Deserializer};

use provenant::golden_maintenance::{find_files_with_extension, run_prettier};
use provenant::license_detection::LicenseDetectionEngine;
use provenant::license_detection::golden_utils::detect_license_expressions_for_golden;

const GOLDEN_DIR: &str = "testdata/license-golden/datadriven";
const REFERENCE_DIR: &str = "reference/scancode-toolkit/tests/licensedcode/data/datadriven";

fn deserialize_yes_no_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum YesNoOrBool {
        String(String),
        Bool(bool),
    }

    match YesNoOrBool::deserialize(deserializer)? {
        YesNoOrBool::Bool(b) => Ok(b),
        YesNoOrBool::String(s) => match s.to_ascii_lowercase().as_str() {
            "yes" | "true" | "1" => Ok(true),
            "no" | "false" | "0" => Ok(false),
            _ => Err(D::Error::custom(format!("invalid boolean value: {s}"))),
        },
    }
}

#[derive(Debug, Deserialize, Default, Clone)]
struct LicenseTestYaml {
    #[serde(default)]
    license_expressions: Vec<String>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default, deserialize_with = "deserialize_yes_no_bool")]
    expected_failure: bool,
}

#[derive(Debug, Default)]
struct LicenseDiff {
    missing: Vec<String>,
    extra: Vec<String>,
}

#[derive(Parser, Debug)]
#[command(
    name = "update-license-golden",
    about = "Sync and update license golden YAML fixtures"
)]
struct Args {
    #[arg(long, help = "Write expected values from current Rust detector output")]
    sync_actual: bool,

    #[arg(long, help = "Apply file updates (default is dry-run)")]
    write: bool,

    #[arg(
        long = "list-mismatches",
        visible_alias = "list-diffs",
        help = "Print files where Python reference expectations differ from current Rust detector output"
    )]
    list_mismatches: bool,

    #[arg(long, help = "Print detailed diff for mismatches")]
    show_diff: bool,

    #[arg(
        long,
        value_name = "PATTERN",
        help = "Process only paths containing PATTERN"
    )]
    filter: Option<String>,

    #[arg(
        long,
        help = "Suite to process (lic1, lic2, lic3, lic4, external, unknown). Default: all"
    )]
    suite: Option<String>,
}

fn load_yaml(path: &Path) -> Result<LicenseTestYaml> {
    let yaml = fs::read_to_string(path).with_context(|| format!("read YAML: {path:?}"))?;
    yaml_serde::from_str(&yaml).with_context(|| format!("parse YAML: {path:?}"))
}

fn push_yaml_string_field(lines: &mut Vec<String>, key: &str, value: &str) {
    if value.contains('\n') {
        lines.push(format!("{key}: |"));
        for line in value.lines() {
            lines.push(format!("  {line}"));
        }
    } else {
        lines.push(format!("{key}: {value}"));
    }
}

fn yaml_to_string(yaml: &LicenseTestYaml) -> String {
    let mut lines = Vec::new();

    lines.push("license_expressions:".to_string());
    for expr in &yaml.license_expressions {
        lines.push(format!("  - {expr}"));
    }

    if let Some(ref notes) = yaml.notes {
        push_yaml_string_field(&mut lines, "notes", notes);
    }

    if yaml.expected_failure {
        lines.push("expected_failure: yes".to_string());
    }

    lines.join("\n") + "\n"
}

fn compare_license_expressions(actual: &[String], expected: &[String]) -> LicenseDiff {
    let actual_counts = value_counts(actual);
    let expected_counts = value_counts(expected);

    LicenseDiff {
        missing: count_differences(&expected_counts, &actual_counts),
        extra: count_differences(&actual_counts, &expected_counts),
    }
}

fn order_only_mismatch(actual: &[String], expected: &[String]) -> bool {
    actual != expected && value_counts(actual) == value_counts(expected)
}

fn value_counts(values: &[String]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for value in values {
        *counts.entry(value.clone()).or_default() += 1;
    }
    counts
}

fn count_differences(
    left: &BTreeMap<String, usize>,
    right: &BTreeMap<String, usize>,
) -> Vec<String> {
    let mut diffs = Vec::new();

    for (value, left_count) in left {
        let right_count = right.get(value).copied().unwrap_or_default();
        for _ in 0..left_count.saturating_sub(right_count) {
            diffs.push(value.clone());
        }
    }

    diffs
}

fn update_yaml_to_actual(ours_yaml: &Path, actual: Vec<String>, write: bool) -> Result<bool> {
    let mut ours = load_yaml(ours_yaml)?;
    if ours.license_expressions == actual {
        return Ok(false);
    }

    ours.license_expressions = actual;
    let new_text = yaml_to_string(&ours);
    let old_text =
        fs::read_to_string(ours_yaml).with_context(|| format!("read YAML: {ours_yaml:?}"))?;

    if new_text == old_text {
        return Ok(false);
    }

    if write {
        fs::write(ours_yaml, new_text).with_context(|| format!("write YAML: {ours_yaml:?}"))?;
    }

    Ok(true)
}

fn detect_actual_expressions(
    engine: &LicenseDetectionEngine,
    input_path: &Path,
    unknown_licenses: bool,
) -> Result<Vec<String>> {
    detect_license_expressions_for_golden(engine, input_path, unknown_licenses)
}

fn process_suite(
    suite_name: &str,
    args: &Args,
    repo_root: &Path,
    engine: &LicenseDetectionEngine,
) -> Result<(usize, usize, usize, Vec<PathBuf>)> {
    let ours_root = repo_root.join(GOLDEN_DIR).join(suite_name);
    let ref_root = repo_root.join(REFERENCE_DIR).join(suite_name);
    let unknown_licenses = suite_name == "unknown";

    if !ours_root.exists() {
        return Ok((0, 0, 0, Vec::new()));
    }

    let yamls = find_files_with_extension(&ours_root, "yml")?;
    let mut updated = 0usize;
    let mut skipped_no_ref = 0usize;
    let mut skipped_mismatch = 0usize;
    let mut updated_files = Vec::new();

    for ours_yaml in yamls {
        let rel = ours_yaml.strip_prefix(&ours_root).unwrap_or(&ours_yaml);
        if let Some(ref filter) = args.filter
            && !rel.to_string_lossy().contains(filter)
        {
            continue;
        }

        let input_path = ours_yaml.with_extension("");
        if !input_path.is_file() {
            continue;
        }

        let actual = detect_actual_expressions(engine, &input_path, unknown_licenses)
            .with_context(|| format!("detect actual output for {input_path:?}"))?;

        if args.sync_actual {
            if update_yaml_to_actual(&ours_yaml, actual, args.write)? {
                updated += 1;
                if args.write {
                    updated_files.push(ours_yaml.clone());
                }
            }
            continue;
        }

        let ref_yaml = ref_root.join(rel);
        if !ref_yaml.is_file() {
            skipped_no_ref += 1;
            continue;
        }

        let ref_content = load_yaml(&ref_yaml)?;
        let parity_matches =
            ref_content.expected_failure || actual == ref_content.license_expressions;

        if !parity_matches {
            if args.list_mismatches {
                eprintln!("mismatch: {}", rel.display());
            }
            if args.show_diff {
                let diff = compare_license_expressions(&actual, &ref_content.license_expressions);
                eprintln!(
                    "  license_expressions: missing={} extra={} order_only={}",
                    diff.missing.len(),
                    diff.extra.len(),
                    order_only_mismatch(&actual, &ref_content.license_expressions)
                );
                if args.filter.is_some() {
                    for missing in diff.missing.iter().take(10) {
                        eprintln!("    - {missing}");
                    }
                    for extra in diff.extra.iter().take(10) {
                        eprintln!("    + {extra}");
                    }
                    eprintln!("    expected: {:?}", ref_content.license_expressions);
                    eprintln!("    actual:   {:?}", actual);
                }
            }
            skipped_mismatch += 1;
            continue;
        }

        let ref_text = yaml_to_string(&ref_content);
        let ours_text =
            fs::read_to_string(&ours_yaml).with_context(|| format!("read YAML: {ours_yaml:?}"))?;
        if ref_text == ours_text {
            continue;
        }

        if args.write {
            fs::write(&ours_yaml, ref_text)
                .with_context(|| format!("write YAML: {ours_yaml:?}"))?;
            updated_files.push(ours_yaml.clone());
        }
        updated += 1;
    }

    Ok((updated, skipped_no_ref, skipped_mismatch, updated_files))
}

fn main() -> Result<()> {
    let args = Args::parse();
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let engine = LicenseDetectionEngine::from_embedded()
        .context("failed to initialize license detection engine from embedded artifact")?;

    let suites = if let Some(ref suite) = args.suite {
        vec![suite.as_str()]
    } else {
        vec!["lic1", "lic2", "lic3", "lic4", "external", "unknown"]
    };

    let mut total_updated = 0usize;
    let mut total_skipped_no_ref = 0usize;
    let mut total_skipped_mismatch = 0usize;
    let mut all_updated_files = Vec::new();

    for suite_name in suites {
        let (updated, skipped_no_ref, skipped_mismatch, updated_files) =
            process_suite(suite_name, &args, &repo_root, &engine)?;
        total_updated += updated;
        total_skipped_no_ref += skipped_no_ref;
        total_skipped_mismatch += skipped_mismatch;
        all_updated_files.extend(updated_files);
    }

    if args.write && !all_updated_files.is_empty() {
        run_prettier(&all_updated_files)?;
    }

    if args.write {
        eprintln!(
            "updated {total_updated} file(s); skipped_no_ref={total_skipped_no_ref}; skipped_mismatch={total_skipped_mismatch}"
        );
    } else {
        eprintln!(
            "would update {total_updated} file(s); skipped_no_ref={total_skipped_no_ref}; skipped_mismatch={total_skipped_mismatch} (pass --write to apply)"
        );
    }

    Ok(())
}
