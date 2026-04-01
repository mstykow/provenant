use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Deserializer};

use provenant::golden_maintenance::{find_files_with_extension, run_prettier};

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
        YesNoOrBool::String(s) => {
            let lower = s.to_lowercase();
            if lower == "yes" || lower == "true" || lower == "1" {
                Ok(true)
            } else if lower == "no" || lower == "false" || lower == "0" {
                Ok(false)
            } else {
                Err(D::Error::custom(format!("invalid boolean value: {}", s)))
            }
        }
    }
}

#[derive(Debug, Deserialize, Default)]
struct LicenseTestYaml {
    #[serde(default)]
    license_expressions: Vec<String>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default, deserialize_with = "deserialize_yes_no_bool")]
    #[allow(dead_code)]
    expected_failure: bool,
}

#[derive(Parser, Debug)]
#[command(
    name = "update-license-golden",
    about = "Sync license golden test YAML fixtures from Python reference"
)]
struct Args {
    #[arg(long, help = "Apply file updates (default is dry-run)")]
    write: bool,

    #[arg(long, help = "Print files that differ from reference")]
    list_diffs: bool,

    #[arg(long, help = "Print detailed diff for differing files")]
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

fn yaml_to_string(yaml: &LicenseTestYaml) -> Result<String> {
    let mut lines = Vec::new();

    lines.push("license_expressions:".to_string());
    for expr in &yaml.license_expressions {
        lines.push(format!("  - {}", expr));
    }

    if let Some(ref notes) = yaml.notes {
        lines.push(format!("notes: {}", notes));
    }

    Ok(lines.join("\n") + "\n")
}

fn compare_license_expressions(ours: &[String], theirs: &[String]) -> (Vec<String>, Vec<String>) {
    let ours_set: std::collections::BTreeSet<_> = ours.iter().collect();
    let theirs_set: std::collections::BTreeSet<_> = theirs.iter().collect();

    let missing: Vec<String> = theirs_set
        .difference(&ours_set)
        .map(|s| (*s).clone())
        .collect();
    let extra: Vec<String> = ours_set
        .difference(&theirs_set)
        .map(|s| (*s).clone())
        .collect();

    (missing, extra)
}

fn process_suite(
    suite_name: &str,
    args: &Args,
    repo_root: &Path,
) -> Result<(usize, usize, usize, Vec<PathBuf>)> {
    let ours_root = repo_root.join(GOLDEN_DIR).join(suite_name);
    let ref_root = repo_root.join(REFERENCE_DIR).join(suite_name);

    if !ours_root.exists() || !ref_root.exists() {
        return Ok((0, 0, 0, Vec::new()));
    }

    let yamls = find_files_with_extension(&ours_root, "yml")?;

    let mut updated = 0usize;
    let mut skipped_no_ref = 0usize;
    let mut skipped_mismatch = 0usize;
    let mut updated_files: Vec<PathBuf> = Vec::new();

    for ours_yaml in yamls {
        let rel = ours_yaml.strip_prefix(&ours_root).unwrap_or(&ours_yaml);

        if let Some(ref f) = args.filter
            && !rel.to_string_lossy().contains(f)
        {
            continue;
        }

        let ref_yaml = ref_root.join(rel);
        if !ref_yaml.is_file() {
            skipped_no_ref += 1;
            continue;
        }

        let ours_content = match load_yaml(&ours_yaml) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Warning: Failed to load {}: {}", ours_yaml.display(), e);
                continue;
            }
        };

        let ref_content = match load_yaml(&ref_yaml) {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "Warning: Failed to load reference {}: {}",
                    ref_yaml.display(),
                    e
                );
                continue;
            }
        };

        let expressions_match = ours_content.license_expressions == ref_content.license_expressions;

        if !expressions_match {
            if args.list_diffs || args.show_diff {
                let (missing, extra) = compare_license_expressions(
                    &ours_content.license_expressions,
                    &ref_content.license_expressions,
                );
                eprintln!(
                    "diff: {} ({}/{}): missing={} extra={}",
                    rel.display(),
                    suite_name,
                    ours_yaml.file_name().unwrap_or_default().to_string_lossy(),
                    missing.len(),
                    extra.len()
                );
                if args.show_diff {
                    for m in &missing {
                        eprintln!("  - {}", m);
                    }
                    for e in &extra {
                        eprintln!("  + {}", e);
                    }
                }
            }
            skipped_mismatch += 1;

            if args.write {
                let new_content = yaml_to_string(&ref_content)?;
                fs::write(&ours_yaml, &new_content)
                    .with_context(|| format!("write YAML: {ours_yaml:?}"))?;
                updated_files.push(ours_yaml.clone());
                updated += 1;
            }
            continue;
        }

        let ours_text = yaml_to_string(&ours_content)?;
        let ref_text = yaml_to_string(&ref_content)?;

        if ours_text == ref_text {
            continue;
        }

        if args.write {
            fs::write(&ours_yaml, &ref_text)
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

    let suites = if let Some(ref suite) = args.suite {
        vec![suite.as_str()]
    } else {
        vec!["lic1", "lic2", "lic3", "lic4", "external", "unknown"]
    };

    let mut total_updated = 0usize;
    let mut total_skipped_no_ref = 0usize;
    let mut total_skipped_mismatch = 0usize;
    let mut all_updated_files: Vec<PathBuf> = Vec::new();

    for suite_name in suites {
        let (updated, skipped_no_ref, skipped_mismatch, updated_files) =
            process_suite(suite_name, &args, &repo_root)?;

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
            "updated {} file(s); skipped_no_ref={}; skipped_mismatch={}",
            total_updated, total_skipped_no_ref, total_skipped_mismatch
        );
    } else {
        eprintln!(
            "would update {} file(s); skipped_no_ref={}; skipped_mismatch={} (pass --write to apply)",
            total_updated, total_skipped_no_ref, total_skipped_mismatch
        );
    }

    Ok(())
}
