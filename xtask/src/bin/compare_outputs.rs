use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Parser;
use serde::Serialize;
use serde_json::{Map, Value, json};
use sha2::Digest;

use provenant_xtask::common::{
    ScanProfile, derive_repo_name_from_url, ensure_release_binary, now_run_id, project_root,
    realpath, render_tsv_table, run_and_capture, sanitize_label, shell_join, write_pretty_json,
    write_tsv,
};
use provenant_xtask::manifests::{
    CommandInvocation, CommandsManifest, CompareArtifactsManifest, CompareRunManifest,
    RepoManifest, ScancodeManifest, TargetManifest,
};
use provenant_xtask::repo_cache::{
    cleanup_repo_worktree, current_git_log_line, current_git_revision, ensure_repo_mirror,
    prepare_repo_worktree, repo_cache_path, resolve_repo_ref_to_sha,
};

#[derive(Parser, Debug)]
#[command(name = "compare-outputs", trailing_var_arg = true)]
struct Args {
    #[arg(long)]
    repo_url: Option<String>,
    #[arg(long)]
    target_path: Option<PathBuf>,
    #[arg(long)]
    repo_ref: Option<String>,
    #[arg(long, value_enum)]
    profile: Option<ScanProfile>,
    scan_args: Vec<String>,
}

struct ContextState {
    project_root: PathBuf,
    scancode_submodule_dir: PathBuf,
    run_id: String,
    run_dir: PathBuf,
    raw_dir: PathBuf,
    comparison_dir: PathBuf,
    samples_dir: PathBuf,
    run_manifest: PathBuf,
    summary_json: PathBuf,
    summary_tsv: PathBuf,
    target_dir: PathBuf,
    target_label: String,
    target_source_label: String,
    target_revision: String,
    repo_manifest: RepoManifest,
    worktree_retained_after_run: bool,
    profile_name: Option<String>,
    scan_args: Vec<String>,
    provenant_bin: PathBuf,
    provenant_json: PathBuf,
    provenant_stdout: PathBuf,
    scancode_json: PathBuf,
    scancode_stdout: PathBuf,
    scancode_image: String,
}

struct CheckoutGuard {
    cache_dir: Option<PathBuf>,
    target_dir: PathBuf,
}

impl Drop for CheckoutGuard {
    fn drop(&mut self) {
        if let Some(cache_dir) = &self.cache_dir {
            let _ = cleanup_repo_worktree(cache_dir, &self.target_dir);
        }
    }
}

#[derive(Debug, Serialize)]
struct ValueCountEntry {
    value: String,
    count: usize,
}

#[derive(Debug, Serialize)]
struct CountDeltaEntry {
    path: String,
    scancode: usize,
    provenant: usize,
    delta: isize,
    scancode_sample_values: Vec<String>,
    provenant_sample_values: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ValueDifferenceEntry {
    path: String,
    scancode: usize,
    provenant: usize,
    missing_in_provenant: Vec<ValueCountEntry>,
    extra_in_provenant: Vec<ValueCountEntry>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let scan_args = resolve_scan_args(args.profile, args.scan_args.clone())?;
    let mut context = prepare_context(&args, scan_args)?;

    println!("==========================================");
    println!("Provenant vs ScanCode Compare Command");
    println!("==========================================\n");

    println!(
        "[1/6] Preparing compare run directory...\n  {}\n",
        context.run_dir.display()
    );
    println!("[2/6] Preparing target...");
    let checkout = prepare_target(&mut context, &args)?;
    println!();

    println!("[3/6] Ensuring Provenant release binary...");
    ensure_release_binary(&context.project_root, &context.provenant_bin, "provenant")?;
    println!();

    println!("[4/6] Ensuring ScanCode Docker runtime...");
    ensure_scancode_runtime(&mut context)?;
    println!();

    println!("Configuration:");
    println!("  Artifact root: {}", context.run_dir.display());
    println!(
        "  {}: {}",
        context.target_source_label, context.target_label
    );
    if let Some(cache_dir) = &context.repo_manifest.cache_dir {
        println!("  Repo cache:    {}", cache_dir.display());
        println!(
            "  Repo ref:      {}",
            context.repo_manifest.requested_ref.as_deref().unwrap_or("")
        );
    }
    if let Some(profile_name) = &context.profile_name {
        println!("  Profile:       {profile_name}");
    }
    println!("  Scan args:     {}\n", context.scan_args.join(" "));

    println!("[5/6] Running both scanners...");
    run_scancode(&context)?;
    run_provenant(&context)?;
    println!("[6/6] Generating reduced comparison artifacts...");
    generate_comparison_artifacts(&context)?;
    write_manifest(&context)?;

    println!("\n==========================================");
    println!("Comparison Summary");
    println!("==========================================\n");
    print_summary_table(&context.summary_tsv)?;
    println!("\nArtifacts:");
    println!("  Run directory:        {}", context.run_dir.display());
    println!("  Run manifest:         {}", context.run_manifest.display());
    println!(
        "  Raw ScanCode JSON:    {}",
        context.scancode_json.display()
    );
    println!(
        "  Raw Provenant JSON:   {}",
        context.provenant_json.display()
    );
    println!(
        "  ScanCode log:         {}",
        context.scancode_stdout.display()
    );
    println!(
        "  Provenant log:        {}",
        context.provenant_stdout.display()
    );
    println!("  Summary JSON:         {}", context.summary_json.display());
    println!("  Summary TSV:          {}", context.summary_tsv.display());
    println!("  Sample artifacts:     {}", context.samples_dir.display());
    println!("\nTo clean up:\n  rm -rf {}", context.run_dir.display());

    drop(checkout);
    Ok(())
}

fn resolve_scan_args(profile: Option<ScanProfile>, scan_args: Vec<String>) -> Result<Vec<String>> {
    if profile.is_some() && !scan_args.is_empty() {
        bail!("use either --profile or explicit scan flags after --, not both");
    }
    if let Some(profile) = profile {
        return Ok(profile
            .args()
            .iter()
            .map(|value| (*value).to_string())
            .collect());
    }
    if scan_args.is_empty() {
        bail!("pass --profile <common|licenses|packages> or explicit shared scan flags after --");
    }
    Ok(scan_args)
}

fn prepare_context(args: &Args, scan_args: Vec<String>) -> Result<ContextState> {
    if args.repo_url.is_some() == args.target_path.is_some() {
        bail!("specify exactly one of --repo-url or --target-path");
    }
    if args.target_path.is_some() && args.repo_ref.is_some() {
        bail!("--repo-ref can only be used with --repo-url");
    }
    if args.repo_url.is_some() && args.repo_ref.is_none() {
        bail!("--repo-url requires --repo-ref (commit SHA, tag, or branch)");
    }

    let project_root = project_root();
    let artifact_root = project_root.join(".provenant/compare-runs");
    let scancode_submodule_dir = project_root.join("reference/scancode-toolkit");
    if !scancode_submodule_dir.exists() {
        bail!(
            "ScanCode submodule not available at {}. Run ./setup.sh or git submodule update --init first.",
            scancode_submodule_dir.display()
        );
    }
    let slug = if let Some(target_path) = &args.target_path {
        sanitize_label(
            target_path
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or("compare-target"),
            "compare-target",
        )
    } else {
        sanitize_label(
            &derive_repo_name_from_url(args.repo_url.as_deref().unwrap(), "compare-target"),
            "compare-target",
        )
    };
    let run_id = now_run_id(&slug);
    let run_dir = artifact_root.join(&run_id);
    let raw_dir = run_dir.join("raw");
    let comparison_dir = run_dir.join("comparison");
    let samples_dir = comparison_dir.join("samples");
    fs::create_dir_all(&raw_dir)?;
    fs::create_dir_all(&samples_dir)?;
    let target_dir = if let Some(target_path) = &args.target_path {
        realpath(target_path)?
    } else {
        run_dir.join("target")
    };
    let target_source_label = if args.target_path.is_some() {
        "Target path"
    } else {
        "Repo URL"
    }
    .to_string();
    let target_label = if let Some(target_path) = &args.target_path {
        realpath(target_path)?.display().to_string()
    } else {
        args.repo_url.clone().unwrap()
    };
    let repo_manifest = RepoManifest::new(
        args.repo_url.clone(),
        args.repo_ref.clone(),
        None,
        args.repo_url
            .as_ref()
            .map(|url| repo_cache_path(&project_root, url)),
    );
    Ok(ContextState {
        project_root: project_root.clone(),
        scancode_submodule_dir,
        run_id,
        run_dir: run_dir.clone(),
        raw_dir: raw_dir.clone(),
        comparison_dir: comparison_dir.clone(),
        samples_dir: samples_dir.clone(),
        run_manifest: run_dir.join("run-manifest.json"),
        summary_json: comparison_dir.join("summary.json"),
        summary_tsv: comparison_dir.join("summary.tsv"),
        target_dir,
        target_label,
        target_source_label,
        target_revision: String::new(),
        repo_manifest,
        worktree_retained_after_run: args.target_path.is_some(),
        profile_name: args
            .profile
            .map(|profile| profile.display_name().to_string()),
        scan_args,
        provenant_bin: project_root.join("target/release/provenant"),
        provenant_json: raw_dir.join("provenant.json"),
        provenant_stdout: raw_dir.join("provenant-stdout.txt"),
        scancode_json: raw_dir.join("scancode.json"),
        scancode_stdout: raw_dir.join("scancode-stdout.txt"),
        scancode_image: String::new(),
    })
}

fn prepare_target(context: &mut ContextState, args: &Args) -> Result<CheckoutGuard> {
    if let Some(target_path) = &args.target_path {
        if let Some(log_line) = current_git_log_line(&realpath(target_path)?) {
            println!("{log_line}");
        } else {
            println!(
                "  Using local directory without git metadata: {}",
                context.target_dir.display()
            );
        }
        context.target_revision = current_git_revision(&context.target_dir)
            .unwrap_or_else(|| "current local checkout".to_string());
        return Ok(CheckoutGuard {
            cache_dir: None,
            target_dir: context.target_dir.clone(),
        });
    }

    let repo_url = args.repo_url.as_deref().unwrap();
    let repo_ref = args.repo_ref.as_deref().unwrap();
    let cache_dir = context.repo_manifest.cache_dir.clone().unwrap();
    println!("  Updating repo cache: {}", cache_dir.display());
    ensure_repo_mirror(repo_url, &cache_dir)?;
    let resolved_sha = resolve_repo_ref_to_sha(&cache_dir, repo_ref)?;
    println!("  Resolved {repo_ref} -> {resolved_sha}");
    println!(
        "  Preparing worktree (detached HEAD {})",
        &resolved_sha[..8]
    );
    prepare_repo_worktree(&cache_dir, &resolved_sha, &context.target_dir)?;
    if let Some(log_line) = current_git_log_line(&context.target_dir) {
        println!("{log_line}");
    }
    context.target_revision = resolved_sha.clone();
    context.repo_manifest.resolved_sha = Some(resolved_sha);
    Ok(CheckoutGuard {
        cache_dir: Some(cache_dir),
        target_dir: context.target_dir.clone(),
    })
}

fn ensure_scancode_runtime(context: &mut ContextState) -> Result<()> {
    if Command::new("docker")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
    } else {
        bail!("docker is required for compare-outputs");
    }
    let commit = current_git_revision(&context.scancode_submodule_dir)
        .context("failed to resolve ScanCode submodule revision")?;
    let short_commit: String = commit.chars().take(10).collect();
    let status = Command::new("git")
        .current_dir(&context.scancode_submodule_dir)
        .args(["status", "--short", "--untracked-files=no"])
        .output()
        .context("failed to inspect ScanCode worktree")?;
    let dirty = !String::from_utf8_lossy(&status.stdout).trim().is_empty();
    let mut image = format!("provenant-scancode-local:{short_commit}");
    if dirty {
        let diff = Command::new("git")
            .current_dir(&context.scancode_submodule_dir)
            .args(["diff", "--no-ext-diff", "--binary", "HEAD"])
            .output()?;
        let mut hasher = sha2::Sha256::new();
        hasher.update(&diff.stdout);
        let dirty_hash: String = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{:02x}", byte))
            .collect();
        image = format!("provenant-scancode-local:{short_commit}-dirty-{dirty_hash}",);
        image.truncate(128);
    }
    context.scancode_image = image.clone();
    let inspect = Command::new("docker")
        .args(["image", "inspect", &image])
        .output()?;
    if !inspect.status.success() {
        println!("Building ScanCode Docker image: {image}");
        let status = Command::new("docker")
            .current_dir(&context.scancode_submodule_dir)
            .args(["build", "--platform", "linux/amd64", "-t", &image, "."])
            .status()
            .context("failed to build ScanCode Docker image")?;
        if !status.success() {
            bail!("docker build failed for ScanCode image");
        }
    } else {
        println!("Reusing ScanCode Docker image: {image}");
    }
    Ok(())
}

fn run_scancode(context: &ContextState) -> Result<()> {
    println!("------------------------------------------");
    println!("Running ScanCode");
    println!("------------------------------------------");
    let args = build_scancode_args(context);
    println!(
        "  {}",
        shell_join(
            &std::iter::once("docker".to_string())
                .chain(args.iter().cloned())
                .collect::<Vec<_>>()
        )
    );
    let combined = run_and_capture("docker", &args, None, &context.scancode_stdout)?;
    for line in combined.lines() {
        println!("  {line}");
    }
    println!();
    Ok(())
}

fn run_provenant(context: &ContextState) -> Result<()> {
    println!("------------------------------------------");
    println!("Running Provenant");
    println!("------------------------------------------");
    let args = build_provenant_args(context);
    println!(
        "  {}",
        shell_join(
            &std::iter::once(context.provenant_bin.display().to_string())
                .chain(args.iter().cloned())
                .collect::<Vec<_>>()
        )
    );
    let combined = run_and_capture(
        context.provenant_bin.to_str().unwrap(),
        &args,
        Some(&context.target_dir),
        &context.provenant_stdout,
    )?;
    for line in combined.lines() {
        println!("  {line}");
    }
    println!();
    Ok(())
}

fn generate_comparison_artifacts(context: &ContextState) -> Result<()> {
    let scancode: Value = serde_json::from_str(&fs::read_to_string(&context.scancode_json)?)?;
    let provenant: Value = serde_json::from_str(&fs::read_to_string(&context.provenant_json)?)?;
    let scancode_files = files_by_path(&scancode);
    let provenant_files = files_by_path(&provenant);
    let scancode_paths: BTreeSet<String> = scancode_files.keys().cloned().collect();
    let provenant_paths: BTreeSet<String> = provenant_files.keys().cloned().collect();
    let common_paths: Vec<String> = scancode_paths
        .intersection(&provenant_paths)
        .cloned()
        .collect();
    let only_scancode_paths: Vec<String> = scancode_paths
        .difference(&provenant_paths)
        .cloned()
        .collect();
    let only_provenant_paths: Vec<String> = provenant_paths
        .difference(&scancode_paths)
        .cloned()
        .collect();
    let metrics = [
        "license_detections",
        "package_data",
        "copyrights",
        "holders",
        "authors",
        "emails",
        "urls",
        "scan_errors",
    ];
    let mut lower_counts: BTreeMap<String, Vec<CountDeltaEntry>> = metrics
        .iter()
        .map(|m| ((*m).to_string(), Vec::new()))
        .collect();
    let mut higher_counts: BTreeMap<String, Vec<CountDeltaEntry>> = metrics
        .iter()
        .map(|m| ((*m).to_string(), Vec::new()))
        .collect();
    let mut value_differences: BTreeMap<String, Vec<ValueDifferenceEntry>> = metrics
        .iter()
        .map(|m| ((*m).to_string(), Vec::new()))
        .collect();

    for path in &common_paths {
        let scancode_file = scancode_files.get(path).unwrap();
        let provenant_file = provenant_files.get(path).unwrap();
        for metric in metrics {
            let sc_count = metric_count(scancode_file, metric);
            let pr_count = metric_count(provenant_file, metric);
            let sc_values = metric_values(scancode_file, metric);
            let pr_values = metric_values(provenant_file, metric);
            if pr_count < sc_count {
                lower_counts.get_mut(metric).unwrap().push(CountDeltaEntry {
                    path: path.clone(),
                    scancode: sc_count,
                    provenant: pr_count,
                    delta: pr_count as isize - sc_count as isize,
                    scancode_sample_values: sample_values(&sc_values),
                    provenant_sample_values: sample_values(&pr_values),
                });
            } else if pr_count > sc_count {
                higher_counts
                    .get_mut(metric)
                    .unwrap()
                    .push(CountDeltaEntry {
                        path: path.clone(),
                        scancode: sc_count,
                        provenant: pr_count,
                        delta: pr_count as isize - sc_count as isize,
                        scancode_sample_values: sample_values(&sc_values),
                        provenant_sample_values: sample_values(&pr_values),
                    });
            }
            let sc_counter = value_counter(&sc_values);
            let pr_counter = value_counter(&pr_values);
            let missing = subtract_counters(&sc_counter, &pr_counter);
            let extra = subtract_counters(&pr_counter, &sc_counter);
            if !missing.is_empty() || !extra.is_empty() {
                value_differences
                    .get_mut(metric)
                    .unwrap()
                    .push(ValueDifferenceEntry {
                        path: path.clone(),
                        scancode: sc_count,
                        provenant: pr_count,
                        missing_in_provenant: counter_entries(&missing),
                        extra_in_provenant: counter_entries(&extra),
                    });
            }
        }
    }

    let sc_top = top_level_counts(&scancode);
    let pr_top = top_level_counts(&provenant);
    let license_deltas = top_level_license_deltas(&scancode, &provenant);
    let top_level_regressions_map = top_level_regressions(&sc_top, &pr_top, true);
    let top_level_higher_counts = top_level_regressions(&pr_top, &sc_top, false);

    let mut file_metric_summary = Map::new();
    let mut rows = vec![];
    for key in [
        "files",
        "packages",
        "dependencies",
        "license_detections",
        "license_references",
        "license_rule_references",
    ] {
        rows.push(tsv_row(
            key,
            sc_top[key],
            pr_top[key],
            pr_top[key] - sc_top[key],
            "top-level count",
        ));
    }
    rows.push(tsv_row(
        "common_file_paths",
        common_paths.len() as i64,
        common_paths.len() as i64,
        0,
        "paths present in both outputs",
    ));
    rows.push(tsv_row(
        "only_scancode_file_paths",
        only_scancode_paths.len() as i64,
        0,
        -(only_scancode_paths.len() as i64),
        "paths seen only in ScanCode output",
    ));
    rows.push(tsv_row(
        "only_provenant_file_paths",
        0,
        only_provenant_paths.len() as i64,
        only_provenant_paths.len() as i64,
        "paths seen only in Provenant output",
    ));

    let mut potential_regressions = only_scancode_paths.len() + top_level_regressions_map.len();
    let mut potential_higher = only_provenant_paths.len() + top_level_higher_counts.len();
    for metric in metrics {
        let missing = value_differences[metric]
            .iter()
            .filter(|entry| !entry.missing_in_provenant.is_empty())
            .count();
        let extra = value_differences[metric]
            .iter()
            .filter(|entry| !entry.extra_in_provenant.is_empty())
            .count();
        file_metric_summary.insert(
            metric.to_string(),
            json!({
                "lower_counts": lower_counts[metric].len(),
                "higher_counts": higher_counts[metric].len(),
                "missing_in_provenant": missing,
                "extra_in_provenant": extra,
            }),
        );
        if metric == "scan_errors" {
            potential_regressions += higher_counts[metric].len();
            potential_regressions += extra;
            potential_higher += missing;
        } else {
            potential_regressions += lower_counts[metric].len();
            potential_higher += higher_counts[metric].len();
            potential_regressions += missing;
            potential_higher += extra;
        }
        rows.push(tsv_row(
            &format!("{metric}_lower_counts"),
            lower_counts[metric].len() as i64,
            0,
            -(lower_counts[metric].len() as i64),
            "common-path files where Provenant count is lower",
        ));
        rows.push(tsv_row(
            &format!("{metric}_higher_counts"),
            0,
            higher_counts[metric].len() as i64,
            higher_counts[metric].len() as i64,
            "common-path files where Provenant count is higher",
        ));
        rows.push(tsv_row(
            &format!("{metric}_missing_in_provenant"),
            missing as i64,
            0,
            -(missing as i64),
            "paths where normalized values exist only in ScanCode output",
        ));
        rows.push(tsv_row(
            &format!("{metric}_extra_in_provenant"),
            0,
            extra as i64,
            extra as i64,
            "paths where normalized values exist only in Provenant output",
        ));
    }
    let dependency_differences = dependency_differences(&scancode, &provenant);
    let dependency_missing = dependency_differences
        .iter()
        .filter(|entry| !entry.missing_in_provenant.is_empty())
        .count();
    let dependency_extra = dependency_differences
        .iter()
        .filter(|entry| !entry.extra_in_provenant.is_empty())
        .count();
    file_metric_summary.insert(
        "dependencies".to_string(),
        json!({
            "missing_in_provenant": dependency_missing,
            "extra_in_provenant": dependency_extra,
        }),
    );
    potential_regressions += dependency_missing;
    potential_higher += dependency_extra;
    rows.push(tsv_row(
        "dependencies_missing_in_provenant",
        dependency_missing as i64,
        0,
        -(dependency_missing as i64),
        "dependency identities present only in ScanCode output",
    ));
    rows.push(tsv_row(
        "dependencies_extra_in_provenant",
        0,
        dependency_extra as i64,
        dependency_extra as i64,
        "dependency identities present only in Provenant output",
    ));
    rows.push(tsv_row(
        "top_level_license_expression_deltas",
        license_deltas.len() as i64,
        license_deltas.len() as i64,
        0,
        "expressions with different top-level detection counts",
    ));

    let comparison_status = if potential_regressions > 0 {
        "potential_regressions_detected"
    } else if potential_higher > 0 || !license_deltas.is_empty() {
        "differences_detected"
    } else {
        "no_detected_differences"
    };

    let sample_paths = [
        (
            "only_scancode_paths",
            context.samples_dir.join("only_scancode_paths.json"),
        ),
        (
            "only_provenant_paths",
            context.samples_dir.join("only_provenant_paths.json"),
        ),
        (
            "file_metric_lower_counts",
            context.samples_dir.join("file_metric_lower_counts.json"),
        ),
        (
            "file_metric_higher_counts",
            context.samples_dir.join("file_metric_higher_counts.json"),
        ),
        (
            "file_metric_value_differences",
            context
                .samples_dir
                .join("file_metric_value_differences.json"),
        ),
        (
            "top_level_license_expression_deltas",
            context
                .samples_dir
                .join("top_level_license_expression_deltas.json"),
        ),
        (
            "dependency_value_differences",
            context
                .samples_dir
                .join("dependency_value_differences.json"),
        ),
    ];
    write_pretty_json(&sample_paths[0].1, &only_scancode_paths)?;
    write_pretty_json(&sample_paths[1].1, &only_provenant_paths)?;
    write_pretty_json(&sample_paths[2].1, &lower_counts)?;
    write_pretty_json(&sample_paths[3].1, &higher_counts)?;
    write_pretty_json(&sample_paths[4].1, &value_differences)?;
    write_pretty_json(&sample_paths[5].1, &license_deltas)?;
    write_pretty_json(&sample_paths[6].1, &dependency_differences)?;

    let summary = json!({
        "comparison_status": comparison_status,
        "top_level_counts": {
            "scancode": sc_top,
            "provenant": pr_top,
            "delta": {
                "files": pr_top["files"] - sc_top["files"],
                "packages": pr_top["packages"] - sc_top["packages"],
                "dependencies": pr_top["dependencies"] - sc_top["dependencies"],
                "license_detections": pr_top["license_detections"] - sc_top["license_detections"],
                "license_references": pr_top["license_references"] - sc_top["license_references"],
                "license_rule_references": pr_top["license_rule_references"] - sc_top["license_rule_references"],
            }
        },
        "file_path_comparison": {
            "common_paths": common_paths.len(),
            "only_scancode_paths": only_scancode_paths.len(),
            "only_provenant_paths": only_provenant_paths.len(),
        },
        "file_metric_summary": file_metric_summary,
        "top_level_regressions": top_level_regressions_map,
        "top_level_higher_counts": top_level_higher_counts,
        "top_level_license_expression_delta_count": license_deltas.len(),
        "sample_artifacts": BTreeMap::from(sample_paths.map(|(name, path)| (name.to_string(), path.display().to_string()))),
    });
    write_pretty_json(&context.summary_json, &summary)?;
    write_tsv(
        &context.summary_tsv,
        &["metric", "scancode", "provenant", "delta", "notes"],
        &rows,
    )?;
    Ok(())
}

fn files_by_path(value: &Value) -> BTreeMap<String, Value> {
    value
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            if entry.get("type").and_then(Value::as_str) != Some("file") {
                return None;
            }
            entry
                .get("path")
                .and_then(Value::as_str)
                .map(|path| (normalize_compare_path(path), entry.clone()))
        })
        .collect()
}

fn metric_count(entry: &Value, key: &str) -> usize {
    entry
        .get(key)
        .and_then(Value::as_array)
        .map(|values| values.len())
        .unwrap_or(0)
}

fn normalize_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn metric_values(entry: &Value, metric: &str) -> Vec<String> {
    let Some(values) = entry.get(metric).and_then(Value::as_array) else {
        return Vec::new();
    };
    values
        .iter()
        .filter_map(|item| {
            let value = match metric {
                "license_detections" => item
                    .get("license_expression_spdx")
                    .or_else(|| item.get("license_expression"))
                    .or_else(|| item.get("identifier"))
                    .and_then(Value::as_str)
                    .map(normalize_license_expression),
                "package_data" => package_identity(item)
                    .map(str::to_string)
                    .or_else(|| package_fallback_identity(item)),
                "copyrights" => item
                    .get("copyright")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                "holders" => item
                    .get("holder")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                "authors" => item
                    .get("author")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                "emails" => item
                    .get("email")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                "urls" => item.get("url").and_then(Value::as_str).map(str::to_string),
                "scan_errors" => scan_error_identity(item).map(str::to_string),
                _ => None,
            }?;
            let normalized = normalize_text(&value);
            (!normalized.is_empty()).then_some(normalized)
        })
        .collect()
}

fn package_identity(item: &Value) -> Option<&str> {
    item.get("purl")
        .and_then(Value::as_str)
        .or_else(|| item.get("package_url").and_then(Value::as_str))
}

fn package_fallback_identity(item: &Value) -> Option<String> {
    let mut parts = Vec::new();
    for key in [
        "type",
        "package_type",
        "scope",
        "namespace",
        "name",
        "version",
        "datasource_id",
    ] {
        if let Some(value) = item.get(key).and_then(Value::as_str) {
            let normalized = normalize_text(value);
            if !normalized.is_empty() {
                parts.push(format!("{key}={normalized}"));
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("|"))
    }
}

fn scan_error_identity(item: &Value) -> Option<&str> {
    item.as_str()
        .or_else(|| item.get("error").and_then(Value::as_str))
        .or_else(|| item.get("message").and_then(Value::as_str))
        .or_else(|| item.get("scan_error").and_then(Value::as_str))
        .or_else(|| item.get("details").and_then(Value::as_str))
}

fn normalize_compare_path(path: &str) -> String {
    let trimmed = path.trim();
    if matches!(trimmed, "" | "." | "input" | "/input") {
        "<root>".to_string()
    } else {
        trimmed
            .trim_start_matches("./")
            .trim_start_matches("/input/")
            .trim_start_matches("input/")
            .to_string()
    }
}

fn normalize_license_expression(value: &str) -> String {
    let normalized = normalize_text(value);
    if normalized.contains(" OR ")
        || normalized.contains(" or ")
        || normalized.contains(" WITH ")
        || normalized.contains(" with ")
    {
        normalized
    } else {
        normalized.replace(['(', ')'], "")
    }
}

fn sample_values(values: &[String]) -> Vec<String> {
    let mut set = BTreeSet::new();
    for value in values {
        set.insert(value.clone());
    }
    set.into_iter().take(10).collect()
}

fn value_counter(values: &[String]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for value in values {
        *counts.entry(value.clone()).or_insert(0) += 1;
    }
    counts
}

fn subtract_counters(
    left: &BTreeMap<String, usize>,
    right: &BTreeMap<String, usize>,
) -> BTreeMap<String, usize> {
    let mut result = BTreeMap::new();
    for (key, left_count) in left {
        let right_count = right.get(key).copied().unwrap_or(0);
        if left_count > &right_count {
            result.insert(key.clone(), left_count - right_count);
        }
    }
    result
}

fn counter_entries(counter: &BTreeMap<String, usize>) -> Vec<ValueCountEntry> {
    counter
        .iter()
        .map(|(value, count)| ValueCountEntry {
            value: value.clone(),
            count: *count,
        })
        .collect()
}

fn top_level_counts(value: &Value) -> HashMap<&'static str, i64> {
    HashMap::from([
        ("files", file_entry_count(value) as i64),
        ("packages", array_len(value, "packages") as i64),
        ("dependencies", array_len(value, "dependencies") as i64),
        (
            "license_detections",
            array_len(value, "license_detections") as i64,
        ),
        (
            "license_references",
            array_len(value, "license_references") as i64,
        ),
        (
            "license_rule_references",
            array_len(value, "license_rule_references") as i64,
        ),
    ])
}

fn file_entry_count(value: &Value) -> usize {
    value
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|entry| entry.get("type").and_then(Value::as_str) == Some("file"))
        .count()
}

fn array_len(value: &Value, key: &str) -> usize {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|values| values.len())
        .unwrap_or(0)
}

fn top_level_license_deltas(scancode: &Value, provenant: &Value) -> Vec<Value> {
    let mut counter = BTreeMap::new();
    for (label, value) in [("scancode", scancode), ("provenant", provenant)] {
        for item in value
            .get("license_detections")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let key = item
                .get("license_expression_spdx")
                .or_else(|| item.get("license_expression"))
                .or_else(|| item.get("identifier"))
                .and_then(Value::as_str)
                .map(normalize_license_expression)
                .unwrap_or_else(|| "<unknown>".to_string());
            let count = item
                .get("detection_count")
                .and_then(Value::as_i64)
                .unwrap_or(1);
            let entry = counter.entry(key).or_insert((0_i64, 0_i64));
            if label == "scancode" {
                entry.0 += count;
            } else {
                entry.1 += count;
            }
        }
    }
    counter.into_iter().filter_map(|(key, (sc, pr))| (sc != pr).then_some(json!({"license_expression": key, "scancode": sc, "provenant": pr, "delta": pr - sc}))).collect()
}

fn dependency_differences(scancode: &Value, provenant: &Value) -> Vec<ValueDifferenceEntry> {
    let sc_by_path = dependency_counter_by_path(scancode);
    let pr_by_path = dependency_counter_by_path(provenant);
    let mut paths = BTreeSet::new();
    paths.extend(sc_by_path.keys().cloned());
    paths.extend(pr_by_path.keys().cloned());
    let mut differences = Vec::new();
    for path in paths {
        let sc_counter = sc_by_path.get(&path).cloned().unwrap_or_default();
        let pr_counter = pr_by_path.get(&path).cloned().unwrap_or_default();
        let missing = subtract_counters(&sc_counter, &pr_counter);
        let extra = subtract_counters(&pr_counter, &sc_counter);
        if !missing.is_empty() || !extra.is_empty() {
            differences.push(ValueDifferenceEntry {
                path,
                scancode: sc_counter.values().sum(),
                provenant: pr_counter.values().sum(),
                missing_in_provenant: counter_entries(&missing),
                extra_in_provenant: counter_entries(&extra),
            });
        }
    }
    differences
}

fn dependency_counter_by_path(value: &Value) -> BTreeMap<String, BTreeMap<String, usize>> {
    let mut output: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for item in value
        .get("dependencies")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let path = item
            .get("datafile_path")
            .or_else(|| item.get("path"))
            .and_then(Value::as_str)
            .map(normalize_compare_path)
            .unwrap_or_else(|| "<unknown>".to_string());
        let identity = dependency_identity(item).unwrap_or_else(|| "<unknown>".to_string());
        *output.entry(path).or_default().entry(identity).or_insert(0) += 1;
    }
    output
}

fn dependency_identity(item: &Value) -> Option<String> {
    for key in ["purl", "package_url", "dependency_uid"] {
        if let Some(value) = item.get(key).and_then(Value::as_str) {
            let normalized = normalize_text(value);
            if !normalized.is_empty() {
                return Some(normalized);
            }
        }
    }
    let mut parts = Vec::new();
    for key in [
        "datafile_path",
        "scope",
        "namespace",
        "name",
        "version",
        "version_requirement",
        "is_runtime",
        "is_optional",
    ] {
        if let Some(value) = item.get(key) {
            let normalized = if let Some(text) = value.as_str() {
                normalize_text(text)
            } else {
                value.to_string()
            };
            if !normalized.is_empty() {
                parts.push(format!("{key}={normalized}"));
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("|"))
    }
}

fn top_level_regressions(
    left: &HashMap<&'static str, i64>,
    right: &HashMap<&'static str, i64>,
    left_is_scancode: bool,
) -> BTreeMap<String, i64> {
    let mut output = BTreeMap::new();
    for key in [
        "packages",
        "dependencies",
        "license_detections",
        "license_references",
        "license_rule_references",
    ] {
        let left_value = left[key];
        let right_value = right[key];
        if left_is_scancode {
            if right_value < left_value {
                output.insert(key.to_string(), left_value - right_value);
            }
        } else if left_value > right_value {
            output.insert(key.to_string(), left_value - right_value);
        }
    }
    output
}

fn tsv_row(metric: &str, scancode: i64, provenant: i64, delta: i64, notes: &str) -> Vec<String> {
    vec![
        metric.to_string(),
        scancode.to_string(),
        provenant.to_string(),
        delta.to_string(),
        notes.to_string(),
    ]
}

fn write_manifest(context: &ContextState) -> Result<()> {
    let scancode_args = build_scancode_args(context);
    let provenant_args = build_provenant_args(context);
    let manifest = CompareRunManifest {
        run_id: context.run_id.clone(),
        target: TargetManifest::new(
            context.target_source_label.clone(),
            context.target_label.clone(),
            context.target_revision.clone(),
            if context.target_source_label == "Target path" {
                Some(context.target_dir.clone())
            } else {
                None
            },
            context.target_dir.clone(),
            context.worktree_retained_after_run,
        ),
        repo: context.repo_manifest.clone(),
        scan_profile: context.profile_name.clone(),
        artifacts: CompareArtifactsManifest {
            raw_dir: context.raw_dir.clone(),
            comparison_dir: context.comparison_dir.clone(),
        },
        commands: CommandsManifest {
            scancode: CommandInvocation {
                command: shell_join(
                    &std::iter::once("docker".to_string())
                        .chain(scancode_args.iter().cloned())
                        .collect::<Vec<_>>(),
                ),
                working_directory: None,
            },
            provenant: CommandInvocation {
                command: shell_join(
                    &std::iter::once(context.provenant_bin.display().to_string())
                        .chain(provenant_args.iter().cloned())
                        .collect::<Vec<_>>(),
                ),
                working_directory: Some(context.target_dir.clone()),
            },
        },
        scancode: ScancodeManifest {
            image: context.scancode_image.clone(),
            submodule_path: context.scancode_submodule_dir.clone(),
        },
    };
    write_pretty_json(&context.run_manifest, &manifest)?;
    Ok(())
}

fn build_scancode_args(context: &ContextState) -> Vec<String> {
    let mut args = vec![
        "run".to_string(),
        "--rm".to_string(),
        "--platform".to_string(),
        "linux/amd64".to_string(),
        "-v".to_string(),
        format!("{}:/input:ro", context.target_dir.display()),
        "-v".to_string(),
        format!("{}:/out", context.raw_dir.display()),
        context.scancode_image.clone(),
        "--json-pp".to_string(),
        "/out/scancode.json".to_string(),
    ];
    args.extend(context.scan_args.clone());
    args.extend([
        "--ignore".to_string(),
        "*.git*".to_string(),
        "--ignore".to_string(),
        "target/*".to_string(),
        "/input".to_string(),
    ]);
    args
}

fn build_provenant_args(context: &ContextState) -> Vec<String> {
    let mut args = vec![
        "--json-pp".to_string(),
        context.provenant_json.display().to_string(),
    ];
    args.extend(context.scan_args.clone());
    args.extend([
        "--ignore".to_string(),
        "*.git*".to_string(),
        "--ignore".to_string(),
        "target/*".to_string(),
        ".".to_string(),
    ]);
    args
}

fn print_summary_table(path: &Path) -> Result<()> {
    let labels = ["Metric", "ScanCode", "Provenant", "Delta", "Notes"];
    let _ = render_tsv_table(path, &labels)?;
    Ok(())
}
