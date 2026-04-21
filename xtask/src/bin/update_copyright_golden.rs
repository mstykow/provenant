// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use serde::Deserialize;
use yaml_serde::Value;

use provenant::copyright::golden_utils::{canonicalize_golden_value, read_input_content};
use provenant::golden_maintenance::{find_files_with_extension, run_prettier};

const EXPECTED_FAILURES_KEY: &str = "expected_failures";

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum Suite {
    Authors,
    Ics,
    Copyrights,
}

impl Suite {
    fn as_str(self) -> &'static str {
        match self {
            Self::Authors => "authors",
            Self::Ics => "ics",
            Self::Copyrights => "copyrights",
        }
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "update-copyright-golden",
    about = "Sync and update copyright golden YAML fixtures"
)]
struct Args {
    #[arg(value_enum, help = "Fixture suite to process")]
    suite: Suite,

    #[arg(
        long,
        help = "Update expected values from current Rust detector output"
    )]
    sync_actual: bool,

    #[arg(long, help = "Apply file updates (default is dry-run)")]
    write: bool,

    #[arg(long, help = "Print mismatching fixture files")]
    list_mismatches: bool,

    #[arg(long, help = "Print missing/extra summary for mismatches")]
    show_diff: bool,

    #[arg(
        long,
        value_name = "PATTERN",
        help = "Process only paths containing PATTERN"
    )]
    filter: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ExpectedOutput {
    what: Option<Vec<String>>,
    copyrights: Option<Vec<String>>,
    holders: Option<Vec<String>>,
    authors: Option<Vec<String>>,
}

fn canonical_set(values: &[String]) -> BTreeSet<String> {
    values
        .iter()
        .map(|s| canonicalize_golden_value(s.as_str()))
        .collect()
}

fn stable_unique_sorted(values: Vec<String>) -> Vec<String> {
    let set: BTreeSet<String> = values.into_iter().collect();
    set.into_iter().collect()
}

fn strip_expected_failures_from_yaml_text(yaml_text: &str, yaml_path: &Path) -> Result<String> {
    if !yaml_text.contains(EXPECTED_FAILURES_KEY) {
        return Ok(yaml_text.to_string());
    }

    let mut root: Value =
        yaml_serde::from_str(yaml_text).with_context(|| format!("parse YAML: {yaml_path:?}"))?;

    let mapping = root
        .as_mapping_mut()
        .context("YAML root must be a mapping")?;

    let removed = mapping
        .remove(Value::String(EXPECTED_FAILURES_KEY.to_string()))
        .is_some();

    if !removed {
        return Ok(yaml_text.to_string());
    }

    yaml_serde::to_string(&root).with_context(|| format!("serialize YAML: {yaml_path:?}"))
}

fn update_yaml_to_actual(ours_yaml: &Path, content: &str, write: bool) -> Result<bool> {
    let yaml_text =
        fs::read_to_string(ours_yaml).with_context(|| format!("read YAML: {ours_yaml:?}"))?;
    let mut root: Value =
        yaml_serde::from_str(&yaml_text).with_context(|| format!("parse YAML: {ours_yaml:?}"))?;

    let mapping = root
        .as_mapping_mut()
        .context("YAML root must be a mapping")?;

    let removed_expected_failures = mapping
        .remove(Value::String(EXPECTED_FAILURES_KEY.to_string()))
        .is_some();

    let what = mapping
        .get("what")
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();

    let check_c = what.iter().any(|w| w == "copyrights") || mapping.contains_key("copyrights");
    let check_h = what.iter().any(|w| w == "holders") || mapping.contains_key("holders");
    let check_a = what.iter().any(|w| w == "authors") || mapping.contains_key("authors");

    if !(check_c || check_h || check_a) {
        if !removed_expected_failures {
            return Ok(false);
        }

        let new_text = yaml_serde::to_string(&root)
            .with_context(|| format!("serialize YAML: {ours_yaml:?}"))?;
        if new_text == yaml_text {
            return Ok(false);
        }
        if write {
            fs::write(ours_yaml, new_text).with_context(|| format!("write YAML: {ours_yaml:?}"))?;
        }
        return Ok(true);
    }

    let (c, h, a) = provenant::copyright::detect_copyrights(content, None);
    let actual_c = stable_unique_sorted(c.into_iter().map(|d| d.copyright).collect());
    let actual_h = stable_unique_sorted(h.into_iter().map(|d| d.holder).collect());
    let actual_a = stable_unique_sorted(a.into_iter().map(|d| d.author).collect());

    if check_c {
        mapping.insert(
            Value::String("copyrights".to_string()),
            Value::Sequence(actual_c.into_iter().map(Value::String).collect()),
        );
    }
    if check_h {
        mapping.insert(
            Value::String("holders".to_string()),
            Value::Sequence(actual_h.into_iter().map(Value::String).collect()),
        );
    }
    if check_a {
        mapping.insert(
            Value::String("authors".to_string()),
            Value::Sequence(actual_a.into_iter().map(Value::String).collect()),
        );
    }

    let new_text =
        yaml_serde::to_string(&root).with_context(|| format!("serialize YAML: {ours_yaml:?}"))?;
    if new_text == yaml_text {
        return Ok(false);
    }
    if write {
        fs::write(ours_yaml, new_text).with_context(|| format!("write YAML: {ours_yaml:?}"))?;
    }
    Ok(true)
}

fn load_expected(path: &Path) -> Result<ExpectedOutput> {
    let yaml = fs::read_to_string(path).with_context(|| format!("read YAML: {path:?}"))?;
    yaml_serde::from_str(&yaml).with_context(|| format!("parse YAML: {path:?}"))
}

fn main() -> Result<()> {
    let args = Args::parse();
    let suite = args.suite.as_str();
    let write = args.write;
    let sync_actual = args.sync_actual;
    let list_mismatches = args.list_mismatches;
    let show_diff = args.show_diff;
    let filter = args.filter;

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let ours_root = repo_root.join("testdata/copyright-golden").join(suite);
    let ref_root = repo_root
        .join("reference/scancode-toolkit/tests/cluecode/data")
        .join(suite);

    let yamls = find_files_with_extension(&ours_root, "yml")?;
    if yamls.is_empty() {
        anyhow::bail!("no YAML files found under {ours_root:?}");
    }

    let mut updated = 0usize;
    let mut skipped_no_ref = 0usize;
    let mut skipped_mismatch = 0usize;
    let mut updated_files: Vec<PathBuf> = Vec::new();

    for ours_yaml in yamls {
        let rel = ours_yaml.strip_prefix(&ours_root).unwrap_or(&ours_yaml);
        if let Some(ref f) = filter
            && !rel.to_string_lossy().contains(f)
        {
            continue;
        }
        let ref_yaml = ref_root.join(rel);
        if !sync_actual && !ref_yaml.is_file() {
            skipped_no_ref += 1;
            continue;
        }

        let input_path = ours_yaml.with_extension("");
        if !input_path.is_file() {
            continue;
        }

        let content = read_input_content(&input_path)
            .with_context(|| format!("read input content: {input_path:?}"))?;

        if sync_actual {
            if update_yaml_to_actual(&ours_yaml, &content, write)? {
                updated += 1;
                if write {
                    updated_files.push(ours_yaml.clone());
                }
            }
            continue;
        }

        let expected_ref = load_expected(&ref_yaml)?;
        let what = expected_ref.what.unwrap_or_default();
        let check_c = what.iter().any(|w| w == "copyrights");
        let check_h = what.iter().any(|w| w == "holders");
        let check_a = what.iter().any(|w| w == "authors");
        if !(check_c || check_h || check_a) {
            continue;
        }

        let expected_c = expected_ref.copyrights.unwrap_or_default();
        let expected_h = expected_ref.holders.unwrap_or_default();
        let expected_a = expected_ref.authors.unwrap_or_default();

        let (c, h, a) = provenant::copyright::detect_copyrights(&content, None);
        let actual_c: Vec<String> = c.into_iter().map(|d| d.copyright).collect();
        let actual_h: Vec<String> = h.into_iter().map(|d| d.holder).collect();
        let actual_a: Vec<String> = a.into_iter().map(|d| d.author).collect();

        let match_c = !check_c || canonical_set(&expected_c) == canonical_set(&actual_c);
        let match_h = !check_h || canonical_set(&expected_h) == canonical_set(&actual_h);
        let match_a = !check_a || canonical_set(&expected_a) == canonical_set(&actual_a);

        if !(match_c && match_h && match_a) {
            if list_mismatches {
                eprintln!(
                    "mismatch: {} (c={} h={} a={})",
                    rel.display(),
                    match_c,
                    match_h,
                    match_a
                );
            }
            if show_diff {
                if !match_c {
                    let exp = canonical_set(&expected_c);
                    let act = canonical_set(&actual_c);
                    let missing: Vec<_> = exp.difference(&act).cloned().collect();
                    let extra: Vec<_> = act.difference(&exp).cloned().collect();
                    eprintln!(
                        "  copyrights: missing={} extra={}",
                        missing.len(),
                        extra.len()
                    );
                    if filter.is_some() {
                        for m in missing.iter().take(10) {
                            eprintln!("    - {m}");
                        }
                        for e in extra.iter().take(10) {
                            eprintln!("    + {e}");
                        }
                    }
                }
                if !match_h {
                    let exp = canonical_set(&expected_h);
                    let act = canonical_set(&actual_h);
                    let missing: Vec<_> = exp.difference(&act).cloned().collect();
                    let extra: Vec<_> = act.difference(&exp).cloned().collect();
                    eprintln!(
                        "  holders:    missing={} extra={}",
                        missing.len(),
                        extra.len()
                    );
                    if filter.is_some() {
                        for m in missing.iter().take(10) {
                            eprintln!("    - {m}");
                        }
                        for e in extra.iter().take(10) {
                            eprintln!("    + {e}");
                        }
                    }
                }
                if !match_a {
                    let exp = canonical_set(&expected_a);
                    let act = canonical_set(&actual_a);
                    let missing: Vec<_> = exp.difference(&act).cloned().collect();
                    let extra: Vec<_> = act.difference(&exp).cloned().collect();
                    eprintln!(
                        "  authors:    missing={} extra={}",
                        missing.len(),
                        extra.len()
                    );
                    if filter.is_some() {
                        for m in missing.iter().take(10) {
                            eprintln!("    - {m}");
                        }
                        for e in extra.iter().take(10) {
                            eprintln!("    + {e}");
                        }
                    }
                }
            }
            skipped_mismatch += 1;
            continue;
        }

        let ref_text_raw = fs::read_to_string(&ref_yaml)
            .with_context(|| format!("read reference YAML: {ref_yaml:?}"))?;
        let ref_text = strip_expected_failures_from_yaml_text(&ref_text_raw, &ref_yaml)?;

        let ours_text_raw = fs::read_to_string(&ours_yaml)
            .with_context(|| format!("read our YAML: {ours_yaml:?}"))?;
        let ours_text = strip_expected_failures_from_yaml_text(&ours_text_raw, &ours_yaml)?;

        let ours_requires_strip_write = ours_text != ours_text_raw;
        if ours_text == ref_text && !ours_requires_strip_write {
            continue;
        }

        let text_to_write = if ours_text == ref_text {
            ours_text
        } else {
            ref_text
        };

        if write {
            fs::write(&ours_yaml, text_to_write)
                .with_context(|| format!("write YAML: {ours_yaml:?}"))?;
            updated_files.push(ours_yaml.clone());
        }
        updated += 1;
    }

    if write {
        run_prettier(&updated_files)?;
    }

    if write {
        eprintln!(
            "updated {updated} file(s); skipped_no_ref={skipped_no_ref}; skipped_mismatch={skipped_mismatch}"
        );
    } else {
        eprintln!(
            "would update {updated} file(s); skipped_no_ref={skipped_no_ref}; skipped_mismatch={skipped_mismatch} (pass --write to apply)"
        );
    }

    Ok(())
}
