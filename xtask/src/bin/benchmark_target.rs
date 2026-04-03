use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use clap::Parser;
use provenant_xtask::common::{
    ScanProfile, TargetSource, append_tsv_row, derive_repo_name_from_url, ensure_release_binary,
    project_root, realpath, render_tsv_table, resolve_scan_args, run_and_capture,
    write_pretty_json, write_tsv,
};
use provenant_xtask::manifests::{BenchmarkRunManifest, RepoManifest, TargetManifest};
use provenant_xtask::repo_cache::{
    cleanup_repo_worktree, current_git_log_line, current_git_revision, ensure_repo_mirror,
    prepare_repo_worktree, repo_cache_path, resolve_repo_ref_to_sha,
};
use regex::Regex;

#[derive(Parser, Debug)]
#[command(name = "benchmark-target", trailing_var_arg = true)]
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

struct BenchContext {
    project_root: PathBuf,
    workspace_dir: PathBuf,
    output_dir: PathBuf,
    summary_file: PathBuf,
    manifest_path: PathBuf,
    target_dir: PathBuf,
    target_source: TargetSource,
    target_label: String,
    target_revision: String,
    repo_manifest: RepoManifest,
    profile_name: Option<String>,
    scan_args: Vec<String>,
    time_program: String,
    time_args: Vec<String>,
    provenant_bin: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let scan_args = resolve_scan_args(
        args.profile,
        args.scan_args.clone(),
        "pass --profile <common|licenses|packages> or benchmark scan flags after --",
    )?;
    let project_root = project_root();
    let mut context = prepare_context(&project_root, &args, scan_args)?;
    println!("==========================================");
    println!("Provenant Benchmark Command");
    println!("==========================================\n");
    println!("[1/4] Cleaning up previous benchmark directory...");
    if context.workspace_dir.exists() {
        fs::remove_dir_all(&context.workspace_dir)
            .with_context(|| format!("failed to clean {}", context.workspace_dir.display()))?;
    }
    fs::create_dir_all(&context.output_dir)
        .with_context(|| format!("failed to create {}", context.output_dir.display()))?;
    write_tsv(
        &context.summary_file,
        &[
            "scenario",
            "elapsed_seconds",
            "engine_seconds",
            "scan_seconds",
            "total_seconds",
            "peak_memory_kb",
            "files_scanned",
            "packages_detected",
            "incremental_summary",
        ],
        &[],
    )?;

    let checkout = prepare_target_checkout(&context, &args)?;
    context.target_revision = current_target_revision(&context);
    write_manifest(&context)?;

    println!("\nConfiguration:");
    println!(
        "  {}: {}",
        context.target_source.label(),
        context.target_label
    );
    println!("  Revision:   {}", context.target_revision);
    println!("  Work dir:   {}", context.target_dir.display());
    if let Some(cache) = &context.repo_manifest.cache_dir {
        println!("  Repo cache:  {}", cache.display());
        println!(
            "  Repo ref:    {}",
            context.repo_manifest.requested_ref.as_deref().unwrap_or("")
        );
    }
    if let Some(profile) = &context.profile_name {
        println!("  Profile:    {profile}");
    }
    println!("  Scan args:  {}", context.scan_args.join(" "));
    println!(
        "  Time tool:  {} {}\n",
        context.time_program,
        context.time_args.join(" ")
    );

    println!("[3/4] Building provenant (release mode)...");
    ensure_release_binary(&context.project_root, &context.provenant_bin, "provenant")?;
    println!();
    println!("[4/4] Running benchmark matrix...\n");

    run_case(&context, "uncached-cold", None, false, &[])?;
    run_case(&context, "uncached-repeat", None, false, &[])?;
    let cache_dir = context.workspace_dir.join("cache-incremental");
    run_case(
        &context,
        "incremental-cold",
        Some(&cache_dir),
        true,
        &["--cache-dir", cache_dir.to_str().unwrap(), "--incremental"],
    )?;
    run_case(
        &context,
        "incremental-repeat",
        Some(&cache_dir),
        false,
        &["--cache-dir", cache_dir.to_str().unwrap(), "--incremental"],
    )?;

    println!("==========================================");
    println!("Benchmark Results");
    println!("==========================================\n");
    print_summary_table(&context.summary_file)?;
    println!("\nOutput directories:");
    println!(
        "  {}/<scenario>/scan-output.json",
        context.output_dir.display()
    );
    println!(
        "  {}/<scenario>/provenant-stdout.txt",
        context.output_dir.display()
    );
    println!("  {}", context.manifest_path.display());
    println!("  {}", context.summary_file.display());
    println!(
        "\nTo clean up:\n  rm -rf {}",
        context.workspace_dir.display()
    );

    drop(checkout);
    Ok(())
}

fn prepare_context(
    project_root: &Path,
    args: &Args,
    scan_args: Vec<String>,
) -> Result<BenchContext> {
    if args.repo_url.is_some() == args.target_path.is_some() {
        bail!("specify exactly one of --repo-url or --target-path");
    }
    if args.target_path.is_some() && args.repo_ref.is_some() {
        bail!("--repo-ref can only be used with --repo-url");
    }
    if args.repo_url.is_some() && args.repo_ref.is_none() {
        bail!("--repo-url requires --repo-ref (commit SHA, tag, or branch)");
    }

    let workspace_dir = project_root.join(".provenant/benchmarks");
    let output_dir = workspace_dir.join("results");
    let target_dir = if let Some(target_path) = &args.target_path {
        realpath(target_path)?
    } else {
        workspace_dir.join(derive_repo_name_from_url(
            args.repo_url.as_deref().unwrap(),
            "benchmark-target",
        ))
    };
    let target_source = if args.target_path.is_some() {
        TargetSource::TargetPath
    } else {
        TargetSource::RepoUrl
    };
    let target_label = if let Some(path) = &args.target_path {
        realpath(path)?.display().to_string()
    } else {
        args.repo_url.clone().unwrap()
    };
    let repo_manifest = RepoManifest::new(
        args.repo_url.clone(),
        args.repo_ref.clone(),
        None,
        args.repo_url
            .as_ref()
            .map(|url| repo_cache_path(project_root, url)),
    );
    let (time_program, time_args) = detect_time_program();
    Ok(BenchContext {
        project_root: project_root.to_path_buf(),
        workspace_dir: workspace_dir.clone(),
        output_dir,
        summary_file: workspace_dir.join("results/summary.tsv"),
        manifest_path: workspace_dir.join("run-manifest.json"),
        target_dir,
        target_source,
        target_label,
        target_revision: String::new(),
        repo_manifest,
        profile_name: args
            .profile
            .map(|profile| profile.display_name().to_string()),
        scan_args,
        time_program,
        time_args,
        provenant_bin: project_root.join("target/release/provenant"),
    })
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

fn prepare_target_checkout(context: &BenchContext, args: &Args) -> Result<CheckoutGuard> {
    println!("[2/4] Preparing benchmark repository...");
    if let Some(target_path) = &args.target_path {
        if let Some(log_line) = current_git_log_line(&realpath(target_path)?) {
            println!("{log_line}");
        } else {
            println!(
                "  Using local directory without git metadata: {}",
                context.target_dir.display()
            );
        }
        return Ok(CheckoutGuard {
            cache_dir: None,
            target_dir: context.target_dir.clone(),
        });
    }
    let repo_url = args.repo_url.as_deref().unwrap();
    let repo_ref = args.repo_ref.as_deref().unwrap();
    let cache_dir = context.repo_manifest.cache_dir.as_ref().unwrap();
    println!("  Updating repo cache: {}", cache_dir.display());
    ensure_repo_mirror(repo_url, cache_dir)?;
    let resolved_sha = resolve_repo_ref_to_sha(cache_dir, repo_ref)?;
    println!("  Resolved {repo_ref} -> {resolved_sha}");
    println!(
        "  Preparing worktree (detached HEAD {})",
        &resolved_sha[..8]
    );
    prepare_repo_worktree(cache_dir, &resolved_sha, &context.target_dir)?;
    if let Some(log_line) = current_git_log_line(&context.target_dir) {
        println!("{log_line}");
    }
    Ok(CheckoutGuard {
        cache_dir: Some(cache_dir.clone()),
        target_dir: context.target_dir.clone(),
    })
}

fn current_target_revision(context: &BenchContext) -> String {
    if let Some(revision) = current_git_revision(&context.target_dir) {
        revision
    } else if context.target_source.retains_checkout() {
        "current local checkout".to_string()
    } else {
        context
            .repo_manifest
            .requested_ref
            .clone()
            .unwrap_or_default()
    }
}

fn write_manifest(context: &BenchContext) -> Result<()> {
    let target_revision = current_target_revision(context);
    let manifest = BenchmarkRunManifest {
        target: TargetManifest::new(
            context.target_source.label().to_string(),
            context.target_label.clone(),
            target_revision.clone(),
            if context.target_source.retains_checkout() {
                Some(context.target_dir.clone())
            } else {
                None
            },
            context.target_dir.clone(),
            context.target_source.retains_checkout(),
        ),
        repo: RepoManifest::new(
            context.repo_manifest.url.clone(),
            context.repo_manifest.requested_ref.clone(),
            context.repo_manifest.url.as_ref().map(|_| target_revision),
            context.repo_manifest.cache_dir.clone(),
        ),
        scan_profile: context.profile_name.clone(),
        scan_args: context.scan_args.clone(),
    };
    write_pretty_json(&context.manifest_path, &manifest)?;
    Ok(())
}

fn detect_time_program() -> (String, Vec<String>) {
    if Command::new("gtime")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return ("gtime".to_string(), vec!["-v".to_string()]);
    }
    if Command::new("/usr/bin/time")
        .arg("-v")
        .arg("true")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return ("/usr/bin/time".to_string(), vec!["-v".to_string()]);
    }
    ("/usr/bin/time".to_string(), vec!["-l".to_string()])
}

fn run_case(
    context: &BenchContext,
    scenario: &str,
    cache_dir: Option<&Path>,
    clear_cache_dir: bool,
    extra_args: &[&str],
) -> Result<()> {
    let scenario_dir = context.output_dir.join(scenario);
    let output_file = scenario_dir.join("scan-output.json");
    let stdout_file = scenario_dir.join("provenant-stdout.txt");
    fs::create_dir_all(&scenario_dir)?;
    if clear_cache_dir
        && let Some(cache_dir) = cache_dir
        && cache_dir.exists()
    {
        fs::remove_dir_all(cache_dir)?;
    }

    println!("------------------------------------------");
    println!("Scenario: {scenario}");
    if let Some(cache_dir) = cache_dir {
        println!("  Cache dir: {}", cache_dir.display());
    } else {
        println!("  Cache dir: disabled");
    }
    println!("------------------------------------------");

    let start = Instant::now();
    let mut args = Vec::new();
    args.extend(context.time_args.clone());
    args.push(context.provenant_bin.display().to_string());
    args.push("--json".to_string());
    args.push(output_file.display().to_string());
    args.extend(context.scan_args.clone());
    args.extend([
        "--exclude".to_string(),
        "*.git*".to_string(),
        "--exclude".to_string(),
        "target/*".to_string(),
    ]);
    args.extend(extra_args.iter().map(|value| value.to_string()));
    args.push(".".to_string());
    let combined = run_and_capture(
        &context.time_program,
        &args,
        Some(&context.target_dir),
        &stdout_file,
    )
    .with_context(|| format!("failed to execute benchmark scenario {scenario}"))?;
    let elapsed_seconds = start.elapsed().as_secs_f64();
    for line in combined.lines() {
        println!("  {line}");
    }

    let files_scanned = parse_json_count(&output_file, "files");
    let packages_detected = parse_json_count(&output_file, "packages");
    let peak_memory_kb = parse_peak_memory_kb(&combined);
    let engine_seconds = extract_first_available_phase_seconds(
        &combined,
        &[
            "setup_scan:licenses",
            "finalize:license-engine-creation",
            "license_detection_engine_creation",
        ],
    );
    let scan_seconds = extract_phase_seconds(&combined, "scan");
    let total_seconds = extract_phase_seconds(&combined, "total");
    let incremental_summary = extract_summary_line(&combined, "Incremental");
    append_tsv_row(
        &context.summary_file,
        &[
            scenario.to_string(),
            format!("{elapsed_seconds:.3}"),
            engine_seconds.clone(),
            scan_seconds.clone(),
            total_seconds.clone(),
            peak_memory_kb.clone(),
            files_scanned.clone(),
            packages_detected.clone(),
            incremental_summary.clone(),
        ],
    )?;

    println!();
    println!("  Wall clock time: {:.3} seconds", elapsed_seconds);
    if !engine_seconds.is_empty() {
        println!("  Engine time:     {engine_seconds} seconds");
    }
    if !scan_seconds.is_empty() {
        println!("  Scan time:       {scan_seconds} seconds");
    }
    println!("  Files scanned:   {files_scanned}");
    println!("  Packages:        {packages_detected}");
    if peak_memory_kb != "N/A" {
        let peak_mb = peak_memory_kb.parse::<u64>().unwrap_or(0) / 1024;
        println!("  Peak memory:     {peak_mb} MB ({peak_memory_kb} KB)");
    }
    if !incremental_summary.is_empty() {
        println!("  {incremental_summary}");
    }
    println!();
    Ok(())
}

fn parse_json_count(path: &Path, key: &str) -> String {
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        .and_then(|value| {
            value
                .get(key)
                .and_then(|value| value.as_array())
                .map(|array| array.len().to_string())
        })
        .unwrap_or_else(|| "N/A".to_string())
}

fn parse_peak_memory_kb(output: &str) -> String {
    let patterns = [
        (
            Regex::new(r"Maximum resident set size \(kbytes\):\s*(\d+)").unwrap(),
            true,
        ),
        (
            Regex::new(r"^\s*(\d+)\s+maximum resident set size$").unwrap(),
            false,
        ),
    ];
    for line in output.lines() {
        for (pattern, already_kb) in &patterns {
            if let Some(captures) = pattern.captures(line.trim()) {
                let value = captures
                    .get(1)
                    .map(|m| m.as_str())
                    .unwrap_or("0")
                    .parse::<u64>()
                    .unwrap_or(0);
                return if *already_kb {
                    value.to_string()
                } else {
                    (value / 1024).to_string()
                };
            }
        }
    }
    "N/A".to_string()
}

fn extract_phase_seconds(output: &str, phase: &str) -> String {
    let pattern = Regex::new(&format!(
        r"^\s*{}:\s*([0-9]+(?:\.[0-9]+)?)s\s*$",
        regex::escape(phase)
    ))
    .unwrap();
    output
        .lines()
        .find_map(|line| {
            pattern
                .captures(line)
                .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        })
        .unwrap_or_default()
}

fn extract_first_available_phase_seconds(output: &str, phases: &[&str]) -> String {
    phases
        .iter()
        .find_map(|phase| {
            let value = extract_phase_seconds(output, phase);
            if value.is_empty() { None } else { Some(value) }
        })
        .unwrap_or_default()
}

fn extract_summary_line(output: &str, label: &str) -> String {
    let needle = format!("{label}:");
    output
        .lines()
        .rev()
        .filter_map(|line| {
            let trimmed = line.trim();
            trimmed.contains(&needle).then(|| trimmed.to_string())
        })
        .next()
        .unwrap_or_default()
}

fn print_summary_table(summary_file: &Path) -> Result<()> {
    let labels = [
        "Scenario",
        "Seconds",
        "Engine s",
        "Scan s",
        "Total s",
        "Peak KB",
        "Files",
        "Packages",
        "Incremental summary",
    ];
    let rows = render_tsv_table(summary_file, &labels)?;

    let lookup: std::collections::HashMap<String, std::collections::HashMap<&str, String>> = rows
        .iter()
        .filter_map(|row| {
            if row.len() != labels.len() {
                return None;
            }
            Some((
                row[0].clone(),
                [
                    "scenario",
                    "elapsed_seconds",
                    "engine_seconds",
                    "scan_seconds",
                    "total_seconds",
                    "peak_memory_kb",
                    "files_scanned",
                    "packages_detected",
                    "incremental_summary",
                ]
                .iter()
                .copied()
                .zip(row.iter().cloned())
                .collect(),
            ))
        })
        .collect();
    for (baseline, candidate, label) in [
        (
            "uncached-repeat",
            "incremental-repeat",
            "Incremental vs uncached repeat",
        ),
        (
            "incremental-cold",
            "incremental-repeat",
            "Incremental warm vs incremental cold",
        ),
    ] {
        if let Some(wall) = speedup(&lookup, baseline, candidate, "elapsed_seconds") {
            println!("{label} (wall): {wall:.2}x speedup");
        }
        if let Some(scan) = speedup(&lookup, baseline, candidate, "scan_seconds") {
            println!("{label} (scan): {scan:.2}x speedup");
        }
    }
    Ok(())
}

fn speedup(
    lookup: &std::collections::HashMap<String, std::collections::HashMap<&str, String>>,
    baseline: &str,
    candidate: &str,
    metric: &str,
) -> Option<f64> {
    let baseline_value = lookup.get(baseline)?.get(metric)?.parse::<f64>().ok()?;
    let candidate_value = lookup.get(candidate)?.get(metric)?.parse::<f64>().ok()?;
    (candidate_value != 0.0).then_some(baseline_value / candidate_value)
}
