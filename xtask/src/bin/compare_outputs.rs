// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::Digest;

use provenant_xtask::common::{
    ScanProfile, derive_repo_name_from_url, ensure_release_binary, now_run_id, project_root,
    read_binary_version, realpath, render_tsv_table, resolve_git_worktree_identity,
    resolve_scan_args, sanitize_label, shell_join, write_pretty_json, write_tsv,
};
use provenant_xtask::manifests::{
    CommandInvocation, CommandsManifest, CompareArtifactsManifest, CompareRunManifest,
    ProvenantManifest, RepoManifest, ScancodeManifest, TargetManifest,
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
    target_path: Vec<PathBuf>,
    #[arg(long, requires = "target_path")]
    scancode_cache_identity: Option<String>,
    #[arg(long)]
    repo_ref: Option<String>,
    #[arg(long, value_enum)]
    profile: Option<ScanProfile>,
    scan_args: Vec<String>,
}

#[derive(Debug)]
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
    auxiliary_dir: PathBuf,
    target_resolved_paths: Vec<PathBuf>,
    target_input_args: Vec<String>,
    target_uses_staged_inputs: bool,
    auxiliary_scan_inputs: Vec<AuxiliaryScanInput>,
    target_label: String,
    target_source_label: String,
    target_revision: String,
    target_scancode_cache_identity: Option<String>,
    repo_manifest: RepoManifest,
    worktree_retained_after_run: bool,
    profile_name: Option<String>,
    scan_args: Vec<String>,
    provenant_bin: PathBuf,
    provenant_json: PathBuf,
    provenant_stdout: PathBuf,
    scancode_json: PathBuf,
    scancode_stdout: PathBuf,
    provenant_version: String,
    provenant_runtime_revision: Option<String>,
    provenant_runtime_dirty: bool,
    provenant_runtime_diff_hash: Option<String>,
    scancode_image: String,
    scancode_platform: String,
    scancode_runtime_revision: String,
    scancode_runtime_dirty: bool,
    scancode_runtime_diff_hash: Option<String>,
    scancode_docker_memory_limit: Option<String>,
    scancode_docker_memory_swap_limit: Option<String>,
    scancode_cache_root: PathBuf,
    scancode_cache_dir: Option<PathBuf>,
    scancode_cache_key: Option<String>,
    scancode_cache_hit: bool,
}

#[derive(Debug, Clone)]
struct AuxiliaryScanInput {
    original_arg: String,
    resolved_path: PathBuf,
    staged_name: String,
}

struct CommandRunOutput {
    combined: String,
    log_warning: Option<String>,
    success: bool,
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

#[derive(Debug, Serialize)]
struct ScalarDifferenceEntry {
    path: String,
    scancode: Option<String>,
    provenant: Option<String>,
}

#[derive(Debug, Serialize)]
struct TopLevelSectionDifferenceEntry {
    section: String,
    scancode: Option<Value>,
    provenant: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ScancodeCacheEntryManifest {
    cache_key: String,
    cache_identity: Option<String>,
    target_label: String,
    target_revision: String,
    repo_url: Option<String>,
    scan_args: Vec<String>,
    scancode_image: String,
    scancode_runtime_revision: String,
    scancode_runtime_dirty: bool,
    scancode_runtime_diff_hash: Option<String>,
    docker_memory_limit: Option<String>,
    docker_memory_swap_limit: Option<String>,
    scancode_json: PathBuf,
    scancode_stdout: Option<PathBuf>,
}

const SCANCODE_PLACEHOLDER_LOG_MESSAGE: &str = "ScanCode stdout was not captured for this cache entry. Reused cached scancode.json without a corresponding log file.\n";
const COMMON_PROFILE_SCANCODE_MEMORY_LIMIT: &str = "12g";
const COMMON_PROFILE_SCANCODE_MEMORY_LIMIT_BYTES: u64 = 12 * 1024 * 1024 * 1024;

fn main() -> Result<()> {
    let args = Args::parse();
    let profile = args.profile;
    let explicit_scan_args = args.scan_args.clone();
    let scan_args = resolve_scan_args(
        profile,
        explicit_scan_args,
        "pass --profile <common|common-with-compiled|licenses|packages> or explicit shared scan flags after --",
    )?;
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

    resolve_provenant_runtime_identity(&mut context)?;
    println!("[4/6] Preparing ScanCode runtime/cache...");
    resolve_scancode_runtime_identity(&mut context)?;
    prepare_scancode_cache(&mut context)?;
    if context.scancode_cache_hit {
        println!(
            "Reusing cached ScanCode result: {}",
            context.scancode_cache_dir.as_ref().unwrap().display()
        );
        println!("Skipping Docker runtime preparation on cache hit");
    } else {
        ensure_scancode_runtime(&context)?;
    }
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
    println!("  Provenant:     {}", context.provenant_version);
    if let Some(revision) = &context.provenant_runtime_revision {
        println!("  Provenant rev: {revision}");
    }
    println!("  ScanCode image: {}", context.scancode_image);
    println!("  ScanCode platform: {}", context.scancode_platform);
    println!("  Scan args:     {}\n", context.scan_args.join(" "));
    if let Some(cache_dir) = &context.scancode_cache_dir {
        println!(
            "  ScanCode cache: {} ({})",
            cache_dir.display(),
            if context.scancode_cache_hit {
                "hit"
            } else {
                "miss"
            }
        );
    } else {
        println!(
            "  ScanCode cache: disabled (repo-url runs require --repo-ref; target-path runs require --scancode-cache-identity)\n"
        );
    }

    println!("[5/6] Running both scanners...");
    run_scancode(&mut context)?;
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
        optional_artifact_display(&context.scancode_stdout)
    );
    println!(
        "  Provenant log:        {}",
        optional_artifact_display(&context.provenant_stdout)
    );
    println!("  Summary JSON:         {}", context.summary_json.display());
    println!("  Summary TSV:          {}", context.summary_tsv.display());
    println!("  Sample artifacts:     {}", context.samples_dir.display());
    println!("\nTo clean up:\n  rm -rf {}", context.run_dir.display());

    drop(checkout);
    Ok(())
}

fn prepare_context(args: &Args, scan_args: Vec<String>) -> Result<ContextState> {
    let has_target_paths = !args.target_path.is_empty();
    if args.repo_url.is_some() == has_target_paths {
        bail!("specify exactly one of --repo-url or --target-path");
    }
    if has_target_paths && args.repo_ref.is_some() {
        bail!("--repo-ref can only be used with --repo-url");
    }
    if args.repo_url.is_some() && args.repo_ref.is_none() {
        bail!("--repo-url requires --repo-ref (commit SHA, tag, or branch)");
    }
    if args.scancode_cache_identity.is_some() && !has_target_paths {
        bail!("--scancode-cache-identity can only be used with --target-path");
    }
    let target_scancode_cache_identity = args
        .scancode_cache_identity
        .as_deref()
        .map(str::trim)
        .map(str::to_string);
    if target_scancode_cache_identity
        .as_deref()
        .is_some_and(str::is_empty)
    {
        bail!("--scancode-cache-identity must not be blank");
    }

    let target_resolved_paths = if has_target_paths {
        args.target_path
            .iter()
            .map(|path| realpath(path))
            .collect::<Result<Vec<_>>>()?
    } else {
        Vec::new()
    };
    if target_resolved_paths.len() > 1 && target_resolved_paths.iter().any(|path| path.is_dir()) {
        bail!("multiple --target-path values currently support files only");
    }
    let target_uses_staged_inputs = !target_resolved_paths.is_empty()
        && target_resolved_paths.iter().all(|path| path.is_file());
    let target_input_args = if target_uses_staged_inputs {
        staged_input_names(&target_resolved_paths)
    } else {
        vec![".".to_string()]
    };
    let auxiliary_scan_inputs = auxiliary_scan_inputs(&scan_args)?;

    let project_root = project_root();
    let artifact_root = project_root.join(".provenant/compare-runs");
    let scancode_cache_root = project_root.join(".provenant/scancode-cache");
    let scancode_submodule_dir = project_root.join("reference/scancode-toolkit");
    if !scancode_submodule_dir.exists() {
        bail!(
            "ScanCode submodule not available at {}. Run ./setup.sh or git submodule update --init first.",
            scancode_submodule_dir.display()
        );
    }
    let slug = if has_target_paths {
        if target_resolved_paths.len() == 1 {
            sanitize_label(
                target_resolved_paths[0]
                    .file_name()
                    .and_then(|v| v.to_str())
                    .unwrap_or("compare-target"),
                "compare-target",
            )
        } else {
            "multi-target".to_string()
        }
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
    let target_dir = if has_target_paths {
        if target_uses_staged_inputs {
            run_dir.join("input")
        } else {
            target_resolved_paths
                .first()
                .cloned()
                .unwrap_or_else(|| run_dir.join("input"))
        }
    } else {
        run_dir.join(&slug)
    };
    let auxiliary_dir = run_dir.join("auxiliary-inputs");
    let target_source_label = if has_target_paths {
        if target_resolved_paths.len() > 1 {
            "Target paths"
        } else {
            "Target path"
        }
    } else {
        "Repo URL"
    }
    .to_string();
    let target_label = if has_target_paths {
        target_resolved_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
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
        auxiliary_dir,
        target_resolved_paths,
        target_input_args,
        target_uses_staged_inputs,
        auxiliary_scan_inputs,
        target_label,
        target_source_label,
        target_revision: String::new(),
        target_scancode_cache_identity,
        repo_manifest,
        worktree_retained_after_run: has_target_paths,
        profile_name: args
            .profile
            .map(|profile| profile.display_name().to_string()),
        scan_args,
        provenant_bin: project_root.join("target/release/provenant"),
        provenant_json: raw_dir.join("provenant.json"),
        provenant_stdout: raw_dir.join("provenant-stdout.txt"),
        provenant_version: String::new(),
        provenant_runtime_revision: None,
        provenant_runtime_dirty: false,
        provenant_runtime_diff_hash: None,
        scancode_json: raw_dir.join("scancode.json"),
        scancode_stdout: raw_dir.join("scancode-stdout.txt"),
        scancode_image: String::new(),
        scancode_platform: String::new(),
        scancode_runtime_revision: String::new(),
        scancode_runtime_dirty: false,
        scancode_runtime_diff_hash: None,
        scancode_docker_memory_limit: None,
        scancode_docker_memory_swap_limit: None,
        scancode_cache_root,
        scancode_cache_dir: None,
        scancode_cache_key: None,
        scancode_cache_hit: false,
    })
}

fn prepare_target(context: &mut ContextState, args: &Args) -> Result<CheckoutGuard> {
    if !args.target_path.is_empty() {
        for resolved_target in &context.target_resolved_paths {
            if let Some(log_line) = current_git_log_line(resolved_target) {
                println!("{log_line}");
            } else {
                println!(
                    "  Using local path without git metadata: {}",
                    resolved_target.display()
                );
            }
        }
        context.target_revision = local_target_revision(&context.target_resolved_paths);
        if context.target_uses_staged_inputs {
            fs::create_dir_all(&context.target_dir).with_context(|| {
                format!(
                    "failed to create staged input directory {}",
                    context.target_dir.display()
                )
            })?;
            for (resolved_target, staged_name) in context
                .target_resolved_paths
                .iter()
                .zip(context.target_input_args.iter())
            {
                materialize_file(resolved_target, &context.target_dir.join(staged_name))?;
            }
        }
        materialize_auxiliary_scan_inputs(context)?;
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
    materialize_auxiliary_scan_inputs(context)?;
    Ok(CheckoutGuard {
        cache_dir: Some(cache_dir),
        target_dir: context.target_dir.clone(),
    })
}

fn resolve_scancode_runtime_identity(context: &mut ContextState) -> Result<()> {
    let identity = resolve_git_worktree_identity(&context.scancode_submodule_dir)?;
    let commit = identity
        .revision
        .context("failed to resolve ScanCode submodule revision")?;
    let docker_info = resolve_docker_server_info();
    let platform = effective_scancode_docker_platform(docker_info.as_ref());
    let platform_label = sanitize_docker_platform_for_tag(&platform);
    let docker_memory_limit = effective_scancode_docker_memory_limit(
        context.profile_name.as_deref(),
        docker_info.as_ref().and_then(|info| info.mem_total_bytes),
    );
    let short_commit: String = commit.chars().take(10).collect();
    let dirty = identity.dirty;
    let mut image = format!("provenant-scancode-local:{short_commit}-{platform_label}");
    let diff_hash = identity.diff_hash;
    if dirty {
        let digest = diff_hash.clone().unwrap_or_default();
        image = format!("provenant-scancode-local:{short_commit}-{platform_label}-dirty-{digest}");
        image.truncate(128);
    }
    context.scancode_platform = platform;
    context.scancode_runtime_revision = commit;
    context.scancode_runtime_dirty = dirty;
    context.scancode_runtime_diff_hash = diff_hash;
    context.scancode_image = image;
    context.scancode_docker_memory_limit = docker_memory_limit.clone();
    context.scancode_docker_memory_swap_limit = docker_memory_limit;
    Ok(())
}

#[derive(Debug, Clone)]
struct DockerServerInfo {
    architecture: String,
    mem_total_bytes: Option<u64>,
}

fn resolve_docker_server_info() -> Option<DockerServerInfo> {
    let output = Command::new("docker")
        .args(["info", "--format", "{{.Architecture}}\t{{.MemTotal}}"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let mut parts = stdout.trim().split('\t');
    let architecture = parts.next()?.trim().to_string();
    let mem_total_bytes = parts
        .next()
        .and_then(|value| value.trim().parse::<u64>().ok());
    Some(DockerServerInfo {
        architecture,
        mem_total_bytes,
    })
}

fn effective_scancode_docker_platform(docker_info: Option<&DockerServerInfo>) -> String {
    if let Some(platform) = std::env::var("PROVENANT_SCANCODE_DOCKER_PLATFORM")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return platform;
    }

    match docker_info
        .map(|info| info.architecture.as_str())
        .unwrap_or(std::env::consts::ARCH)
    {
        "arm64" | "aarch64" => "linux/arm64/v8".to_string(),
        _ => "linux/amd64".to_string(),
    }
}

fn sanitize_docker_platform_for_tag(platform: &str) -> String {
    platform
        .chars()
        .map(|ch| match ch {
            '/' | ':' | '.' => '-',
            _ => ch,
        })
        .collect()
}

fn effective_scancode_docker_memory_limit(
    profile_name: Option<&str>,
    docker_mem_total_bytes: Option<u64>,
) -> Option<String> {
    if !matches!(profile_name, Some("common")) {
        return None;
    }

    if docker_mem_total_bytes
        .is_some_and(|mem_total_bytes| mem_total_bytes < COMMON_PROFILE_SCANCODE_MEMORY_LIMIT_BYTES)
    {
        return None;
    }

    Some(COMMON_PROFILE_SCANCODE_MEMORY_LIMIT.to_string())
}

fn resolve_provenant_runtime_identity(context: &mut ContextState) -> Result<()> {
    context.provenant_version = read_binary_version(&context.provenant_bin)?;
    let identity = resolve_git_worktree_identity(&context.project_root)?;
    context.provenant_runtime_revision = identity.revision;
    context.provenant_runtime_dirty = identity.dirty;
    context.provenant_runtime_diff_hash = identity.diff_hash;
    Ok(())
}

fn prepare_scancode_cache(context: &mut ContextState) -> Result<()> {
    if effective_scancode_cache_identity(context).is_none() {
        return Ok(());
    }
    fs::create_dir_all(&context.scancode_cache_root).with_context(|| {
        format!(
            "failed to create ScanCode cache root {}",
            context.scancode_cache_root.display()
        )
    })?;
    let key = build_scancode_cache_key(context)?;
    let cache_dir = context.scancode_cache_root.join(&key);
    context.scancode_cache_key = Some(key);
    context.scancode_cache_hit = scancode_cache_complete(&cache_dir);
    context.scancode_cache_dir = Some(cache_dir);
    Ok(())
}

fn run_and_capture_optional_log(
    program: &str,
    args: &[String],
    cwd: Option<&Path>,
    log_path: &Path,
) -> Result<(String, Option<String>)> {
    let output = run_and_capture_optional_log_with_status(program, args, cwd, log_path)?;
    if !output.success {
        bail!(build_command_failure_message(
            program,
            args,
            &output.combined,
            output.log_warning.as_deref()
        ));
    }
    Ok((output.combined, output.log_warning))
}

fn run_and_capture_optional_log_with_status(
    program: &str,
    args: &[String],
    cwd: Option<&Path>,
    log_path: &Path,
) -> Result<CommandRunOutput> {
    let mut command = Command::new(program);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command
        .args(args)
        .output()
        .with_context(|| format!("failed to execute {program}"))?;
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let log_warning = write_optional_command_log(log_path, &combined);
    Ok(CommandRunOutput {
        combined,
        log_warning,
        success: output.status.success(),
    })
}

fn build_command_failure_message(
    program: &str,
    args: &[String],
    combined: &str,
    log_warning: Option<&str>,
) -> String {
    let command = shell_join(
        &std::iter::once(program.to_string())
            .chain(args.iter().cloned())
            .collect::<Vec<_>>(),
    );
    let mut message = format!("command failed: {command}");
    if let Some(log_warning) = log_warning {
        message.push('\n');
        message.push_str(log_warning);
    }
    let combined = combined.trim();
    if !combined.is_empty() {
        message.push_str("\n--- command output ---\n");
        message.push_str(combined);
    }
    message
}

fn write_optional_command_log(log_path: &Path, content: &str) -> Option<String> {
    if let Some(parent) = log_path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        return Some(format!(
            "failed to create log directory {}: {error}",
            parent.display()
        ));
    }
    if let Err(error) = fs::write(log_path, content) {
        return Some(format!(
            "failed to write optional command log {}: {error}",
            log_path.display()
        ));
    }
    None
}

fn ensure_scancode_runtime(context: &ContextState) -> Result<()> {
    if Command::new("docker")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
    } else {
        bail!("docker is required for compare-outputs");
    }
    let inspect = Command::new("docker")
        .args(["image", "inspect", &context.scancode_image])
        .output()?;
    if !inspect.status.success() {
        println!("Building ScanCode Docker image: {}", context.scancode_image);
        let status = Command::new("docker")
            .current_dir(&context.scancode_submodule_dir)
            .args([
                "build",
                "--platform",
                &context.scancode_platform,
                "-t",
                &context.scancode_image,
                ".",
            ])
            .status()
            .context("failed to build ScanCode Docker image")?;
        if !status.success() {
            bail!("docker build failed for ScanCode image");
        }
    } else {
        println!("Reusing ScanCode Docker image: {}", context.scancode_image);
    }
    Ok(())
}

fn run_scancode(context: &mut ContextState) -> Result<()> {
    println!("------------------------------------------");
    println!("Running ScanCode");
    println!("------------------------------------------");
    if context.scancode_cache_hit {
        match validate_and_materialize_scancode_cache_hit(context) {
            Ok(log_warning) => {
                if let Some(log_warning) = log_warning {
                    println!("  Warning: {log_warning}");
                }
                println!(
                    "  Reusing cached ScanCode artifacts from {}",
                    context.scancode_cache_dir.as_ref().unwrap().display()
                );
                println!();
                return Ok(());
            }
            Err(error) => {
                println!("  Cached ScanCode result unusable, rerunning: {error}");
                context.scancode_cache_hit = false;
            }
        }
    }
    let args = build_scancode_docker_args(context);
    println!(
        "  {}",
        shell_join(
            &std::iter::once("docker".to_string())
                .chain(args.iter().cloned())
                .collect::<Vec<_>>()
        )
    );
    let output =
        run_and_capture_optional_log_with_status("docker", &args, None, &context.scancode_stdout)?;
    if let Some(log_warning) = &output.log_warning {
        println!("  Warning: {log_warning}");
    }
    for line in output.combined.lines() {
        println!("  {line}");
    }
    if !output.success {
        let scan_error_count = validate_scancode_output_on_failure(context).map_err(|error| {
            anyhow::anyhow!(
                "{}\n--- cached-output validation ---\n{error}",
                build_command_failure_message(
                    "docker",
                    &args,
                    &output.combined,
                    output.log_warning.as_deref(),
                )
            )
        })?;
        println!(
            "  Warning: ScanCode exited non-zero but wrote valid JSON with {scan_error_count} scan error(s); continuing with captured output."
        );
    }
    persist_scancode_cache_entry(context)?;
    println!();
    Ok(())
}

fn validate_scancode_output_on_failure(context: &ContextState) -> Result<usize> {
    let scancode: Value = serde_json::from_reader(BufReader::new(
        File::open(&context.scancode_json).with_context(|| {
            format!(
                "failed to open ScanCode JSON after non-zero exit {}",
                context.scancode_json.display()
            )
        })?,
    ))
    .with_context(|| {
        format!(
            "failed to parse ScanCode JSON after non-zero exit {}",
            context.scancode_json.display()
        )
    })?;

    let header = scancode
        .get("headers")
        .and_then(Value::as_array)
        .and_then(|headers| headers.first())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "ScanCode output {} is missing headers[0]",
                context.scancode_json.display()
            )
        })?;

    if header
        .get("message")
        .and_then(Value::as_str)
        .is_some_and(|message| !message.trim().is_empty())
    {
        bail!(
            "ScanCode exited non-zero with a non-empty header message in {}",
            context.scancode_json.display()
        );
    }

    let errors = header
        .get("errors")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "ScanCode exited non-zero but output {} is missing headers[0].errors",
                context.scancode_json.display()
            )
        })?;

    if errors.is_empty() {
        bail!(
            "ScanCode exited non-zero but produced no header-level scan errors in {}",
            context.scancode_json.display()
        );
    }

    let error_paths = errors
        .iter()
        .map(|error| {
            let message = error
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("ScanCode error entries must be strings"))?;
            message
                .strip_prefix("Path: ")
                .map(normalize_scancode_error_path)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "ScanCode exited non-zero with a non-scan-error header entry in {}: {}",
                        context.scancode_json.display(),
                        message
                    )
                })
        })
        .collect::<Result<Vec<_>>>()?;

    let files = scancode
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "ScanCode exited non-zero but output {} is missing a files array",
                context.scancode_json.display()
            )
        })?;

    if let Some(unmatched_path) = error_paths.iter().find(|path| {
        !files.iter().any(|file| {
            file.get("path")
                .and_then(Value::as_str)
                .is_some_and(|file_path| normalize_scancode_error_path(file_path) == **path)
                && file
                    .get("scan_errors")
                    .and_then(Value::as_array)
                    .is_some_and(|scan_errors| !scan_errors.is_empty())
        })
    }) {
        bail!(
            "ScanCode exited non-zero but header error path {} had no matching file scan_errors in {}",
            unmatched_path,
            context.scancode_json.display()
        );
    }

    Ok(errors.len())
}

fn normalize_scancode_error_path(path: &str) -> String {
    path.trim()
        .trim_start_matches("/input/")
        .trim_start_matches("input/")
        .to_string()
}

fn build_provenant_invocation(context: &ContextState) -> (PathBuf, Vec<String>) {
    if context.target_uses_staged_inputs {
        (
            context.target_dir.clone(),
            context.target_input_args.clone(),
        )
    } else {
        (context.target_dir.clone(), vec![".".to_string()])
    }
}

fn run_provenant(context: &ContextState) -> Result<()> {
    println!("------------------------------------------");
    println!("Running Provenant");
    println!("------------------------------------------");
    let (working_dir, _input_args) = build_provenant_invocation(context);
    let args = build_provenant_args(context);
    println!(
        "  {}",
        shell_join(
            &std::iter::once(context.provenant_bin.display().to_string())
                .chain(args.iter().cloned())
                .collect::<Vec<_>>()
        )
    );
    let (combined, log_warning) = run_and_capture_optional_log(
        context.provenant_bin.to_str().unwrap(),
        &args,
        Some(&working_dir),
        &context.provenant_stdout,
    )?;
    if let Some(log_warning) = log_warning {
        println!("  Warning: {log_warning}");
    }
    for line in combined.lines() {
        println!("  {line}");
    }
    println!();
    Ok(())
}

fn generate_comparison_artifacts(context: &ContextState) -> Result<()> {
    let scancode: Value = serde_json::from_str(&fs::read_to_string(&context.scancode_json)?)?;
    let provenant: Value = serde_json::from_str(&fs::read_to_string(&context.provenant_json)?)?;
    let info_mode = context
        .scan_args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--info" | "--mark-source"));
    let row2_mode = context.scan_args.iter().any(|arg| {
        matches!(
            arg.as_str(),
            "--classify"
                | "--summary"
                | "--license-clarity-score"
                | "--tallies"
                | "--tallies-key-files"
                | "--tallies-with-details"
                | "--tallies-by-facet"
                | "--facet"
        )
    });
    let scancode_files = files_by_path(&scancode);
    let provenant_files = files_by_path(&provenant);
    let scancode_resources = resources_by_path(&scancode);
    let provenant_resources = resources_by_path(&provenant);
    let scancode_paths: BTreeSet<String> = scancode_files.keys().cloned().collect();
    let provenant_paths: BTreeSet<String> = provenant_files.keys().cloned().collect();
    let scancode_resource_paths: BTreeSet<String> = scancode_resources.keys().cloned().collect();
    let provenant_resource_paths: BTreeSet<String> = provenant_resources.keys().cloned().collect();
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
    let common_resource_paths: Vec<String> = scancode_resource_paths
        .intersection(&provenant_resource_paths)
        .cloned()
        .collect();
    let only_scancode_resource_paths: Vec<String> = scancode_resource_paths
        .difference(&provenant_resource_paths)
        .cloned()
        .collect();
    let only_provenant_resource_paths: Vec<String> = provenant_resource_paths
        .difference(&scancode_resource_paths)
        .cloned()
        .collect();
    let metrics = [
        "license_detections",
        "license_clues",
        "license_policy",
        "package_data",
        "copyrights",
        "holders",
        "authors",
        "emails",
        "urls",
        "scan_errors",
    ];
    let info_metrics = [
        "mime_type",
        "file_type",
        "programming_language",
        "sha1",
        "md5",
        "sha256",
        "sha1_git",
        "is_binary",
        "is_text",
        "is_archive",
        "is_media",
        "is_source",
        "is_script",
        "files_count",
        "dirs_count",
        "size_count",
        "source_count",
    ];
    let classify_metrics = [
        "is_legal",
        "is_manifest",
        "is_readme",
        "is_top_level",
        "is_key_file",
        "is_community",
    ];
    let row2_value_metrics = ["facets", "tallies"];
    let row2_top_level_sections = [
        "summary",
        "tallies",
        "tallies_of_key_files",
        "tallies_by_facet",
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
    let mut info_value_differences: BTreeMap<String, Vec<ScalarDifferenceEntry>> = info_metrics
        .iter()
        .map(|m| ((*m).to_string(), Vec::new()))
        .collect();
    let mut classify_value_differences: BTreeMap<String, Vec<ScalarDifferenceEntry>> =
        classify_metrics
            .iter()
            .map(|m| ((*m).to_string(), Vec::new()))
            .collect();
    let mut row2_value_differences: BTreeMap<String, Vec<ScalarDifferenceEntry>> =
        row2_value_metrics
            .iter()
            .map(|m| ((*m).to_string(), Vec::new()))
            .collect();
    let mut row2_top_level_differences = Vec::new();

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

    for path in &common_resource_paths {
        let scancode_resource = scancode_resources.get(path).unwrap();
        let provenant_resource = provenant_resources.get(path).unwrap();
        for metric in info_metrics {
            let scancode_value = scalar_field_value(scancode_resource, metric);
            let provenant_value = scalar_field_value(provenant_resource, metric);
            if scancode_value != provenant_value {
                info_value_differences
                    .get_mut(metric)
                    .unwrap()
                    .push(ScalarDifferenceEntry {
                        path: path.clone(),
                        scancode: scancode_value,
                        provenant: provenant_value,
                    });
            }
        }
        for metric in classify_metrics {
            let scancode_value = classify_scalar_value(scancode_resource, metric);
            let provenant_value = classify_scalar_value(provenant_resource, metric);
            if scancode_value != provenant_value {
                classify_value_differences
                    .get_mut(metric)
                    .unwrap()
                    .push(ScalarDifferenceEntry {
                        path: path.clone(),
                        scancode: scancode_value,
                        provenant: provenant_value,
                    });
            }
        }
        for metric in row2_value_metrics {
            let scancode_value = structured_field_value(scancode_resource, metric);
            let provenant_value = structured_field_value(provenant_resource, metric);
            if scancode_value != provenant_value {
                row2_value_differences
                    .get_mut(metric)
                    .unwrap()
                    .push(ScalarDifferenceEntry {
                        path: path.clone(),
                        scancode: scancode_value,
                        provenant: provenant_value,
                    });
            }
        }
    }

    for section in row2_top_level_sections {
        let scancode_value = canonical_section_value(&scancode, section);
        let provenant_value = canonical_section_value(&provenant, section);
        if scancode_value != provenant_value {
            row2_top_level_differences.push(TopLevelSectionDifferenceEntry {
                section: section.to_string(),
                scancode: scancode_value,
                provenant: provenant_value,
            });
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
    rows.push(tsv_row(
        "common_resource_paths",
        common_resource_paths.len() as i64,
        common_resource_paths.len() as i64,
        0,
        "resource paths present in both outputs",
    ));
    rows.push(tsv_row(
        "only_scancode_resource_paths",
        only_scancode_resource_paths.len() as i64,
        0,
        -(only_scancode_resource_paths.len() as i64),
        "resource paths seen only in ScanCode output",
    ));
    rows.push(tsv_row(
        "only_provenant_resource_paths",
        0,
        only_provenant_resource_paths.len() as i64,
        only_provenant_resource_paths.len() as i64,
        "resource paths seen only in Provenant output",
    ));

    let mut potential_regressions = only_scancode_paths.len() + top_level_regressions_map.len();
    let mut potential_higher = only_provenant_paths.len() + top_level_higher_counts.len();
    if info_mode {
        potential_regressions += only_scancode_resource_paths.len();
        potential_higher += only_provenant_resource_paths.len();
    }
    if row2_mode {
        potential_regressions += row2_top_level_differences.len();
    }
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
    let mut info_metric_summary = Map::new();
    for metric in info_metrics {
        let differences = info_value_differences[metric].len();
        info_metric_summary.insert(
            metric.to_string(),
            json!({
                "value_differences": differences,
            }),
        );
        if info_mode {
            potential_regressions += differences;
        }
        rows.push(tsv_row(
            &format!("info_{metric}_value_differences"),
            differences as i64,
            differences as i64,
            0,
            "common-path resources where info values differ",
        ));
    }
    let mut classify_metric_summary = Map::new();
    for metric in classify_metrics {
        let differences = classify_value_differences[metric].len();
        classify_metric_summary.insert(
            metric.to_string(),
            json!({
                "value_differences": differences,
            }),
        );
        if row2_mode {
            potential_regressions += differences;
        }
        rows.push(tsv_row(
            &format!("classify_{metric}_value_differences"),
            differences as i64,
            differences as i64,
            0,
            "common-path resources where classify values differ",
        ));
    }
    let mut row2_metric_summary = Map::new();
    for metric in row2_value_metrics {
        let differences = row2_value_differences[metric].len();
        row2_metric_summary.insert(
            metric.to_string(),
            json!({
                "value_differences": differences,
            }),
        );
        if row2_mode {
            potential_regressions += differences;
        }
        rows.push(tsv_row(
            &format!("row2_{metric}_value_differences"),
            differences as i64,
            differences as i64,
            0,
            "common-path resources where row-2 workflow values differ",
        ));
    }
    rows.push(tsv_row(
        "row2_top_level_section_differences",
        row2_top_level_differences.len() as i64,
        row2_top_level_differences.len() as i64,
        0,
        "top-level row-2 workflow sections with normalized JSON differences",
    ));
    let dependency_value_differences = dependency_differences(&scancode, &provenant);
    let dependency_missing = dependency_value_differences
        .iter()
        .filter(|entry| !entry.missing_in_provenant.is_empty())
        .count();
    let dependency_extra = dependency_value_differences
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
        (
            "info_value_differences",
            context.samples_dir.join("info_value_differences.json"),
        ),
        (
            "classify_value_differences",
            context.samples_dir.join("classify_value_differences.json"),
        ),
        (
            "row2_value_differences",
            context.samples_dir.join("row2_value_differences.json"),
        ),
        (
            "row2_top_level_differences",
            context.samples_dir.join("row2_top_level_differences.json"),
        ),
    ];
    write_pretty_json(&sample_paths[0].1, &only_scancode_paths)?;
    write_pretty_json(&sample_paths[1].1, &only_provenant_paths)?;
    write_pretty_json(&sample_paths[2].1, &lower_counts)?;
    write_pretty_json(&sample_paths[3].1, &higher_counts)?;
    write_pretty_json(&sample_paths[4].1, &value_differences)?;
    write_pretty_json(&sample_paths[5].1, &license_deltas)?;
    write_pretty_json(&sample_paths[6].1, &dependency_value_differences)?;
    write_pretty_json(&sample_paths[7].1, &info_value_differences)?;
    write_pretty_json(&sample_paths[8].1, &classify_value_differences)?;
    write_pretty_json(&sample_paths[9].1, &row2_value_differences)?;
    write_pretty_json(&sample_paths[10].1, &row2_top_level_differences)?;

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
        "resource_path_comparison": {
            "common_paths": common_resource_paths.len(),
            "only_scancode_paths": only_scancode_resource_paths.len(),
            "only_provenant_paths": only_provenant_resource_paths.len(),
        },
        "file_metric_summary": file_metric_summary,
        "info_metric_summary": info_metric_summary,
        "classify_metric_summary": classify_metric_summary,
        "row2_metric_summary": row2_metric_summary,
        "row2_top_level_section_difference_count": row2_top_level_differences.len(),
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

fn resources_by_path(value: &Value) -> BTreeMap<String, Value> {
    value
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
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
                "license_clues" | "license_policy" => Some(canonical_value_string(item)),
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
    } else if normalized.contains(" AND ") {
        let stripped = normalized.replace(['(', ')'], "");
        let mut parts: Vec<_> = stripped
            .split(" AND ")
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect();
        parts.sort_unstable();
        parts.join(" AND ")
    } else {
        normalized.replace(['(', ')'], "")
    }
}

fn scalar_field_value(entry: &Value, key: &str) -> Option<String> {
    let value = entry.get(key)?;
    let normalized = match value {
        Value::Null => return None,
        Value::String(text) => normalize_text(text),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        _ => normalize_text(&value.to_string()),
    };
    (!normalized.is_empty()).then_some(normalized)
}

fn structured_field_value(entry: &Value, key: &str) -> Option<String> {
    let value = entry.get(key)?;
    if value.is_null() {
        return None;
    }
    match key {
        "facets" if value.as_array().is_some_and(|items| items.is_empty()) => None,
        "tallies" => canonical_tallies_field_string(value),
        _ => Some(canonical_value_string(value)),
    }
}

fn classify_scalar_value(entry: &Value, key: &str) -> Option<String> {
    match entry.get(key) {
        Some(Value::Bool(flag)) => Some(flag.to_string()),
        Some(Value::Null) | None => Some("false".to_string()),
        Some(other) => scalar_field_value(&json!({ key: other }), key),
    }
}

fn canonical_section_value(value: &Value, key: &str) -> Option<Value> {
    let section = value.get(key)?;
    match key {
        "summary" => Some(canonicalize_summary_section(section)),
        "tallies" | "tallies_of_key_files" => canonical_tallies_section(section),
        "tallies_by_facet" => canonical_tallies_by_facet_section(section),
        _ => Some(canonicalize_json_value(section)),
    }
}

fn canonical_value_string(value: &Value) -> String {
    serde_json::to_string(&canonicalize_json_value(value)).unwrap_or_else(|_| value.to_string())
}

fn canonicalize_json_value(value: &Value) -> Value {
    match value {
        Value::Array(values) => {
            let mut normalized: Vec<Value> = values.iter().map(canonicalize_json_value).collect();
            normalized.sort_by_cached_key(canonical_value_string);
            Value::Array(normalized)
        }
        Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by(|(left, _), (right, _)| left.cmp(right));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key.clone(), canonicalize_json_value(value)))
                    .collect(),
            )
        }
        _ => value.clone(),
    }
}

fn is_empty_tallies_value(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object
        .values()
        .all(|entry| entry.as_array().is_some_and(|items| items.is_empty()))
}

fn canonical_tallies_field_string(value: &Value) -> Option<String> {
    canonical_tallies_section(value).map(|value| canonical_value_string(&value))
}

fn canonicalize_summary_section(value: &Value) -> Value {
    let Some(object) = value.as_object() else {
        return canonicalize_json_value(value);
    };

    let mut normalized = serde_json::Map::new();
    for (key, section_value) in object {
        let normalized_value = match key.as_str() {
            "other_license_expressions" => {
                canonicalize_tally_entry_array(section_value, "detected_license_expression")
            }
            "other_holders" => canonicalize_tally_entry_array(section_value, "holders"),
            "other_languages" => {
                canonicalize_tally_entry_array(section_value, "programming_language")
            }
            _ => canonicalize_json_value(section_value),
        };
        normalized.insert(key.clone(), normalized_value);
    }

    for key in [
        "other_license_expressions",
        "other_holders",
        "other_languages",
    ] {
        normalized
            .entry(key.to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
    }

    Value::Object(normalized)
}

fn canonical_tallies_section(value: &Value) -> Option<Value> {
    let Some(object) = value.as_object() else {
        return Some(canonicalize_json_value(value));
    };

    let mut normalized = serde_json::Map::new();
    for key in [
        "detected_license_expression",
        "copyrights",
        "holders",
        "authors",
        "programming_language",
    ] {
        let normalized_entries = object
            .get(key)
            .map(|entries| canonicalize_tally_entry_array(entries, key))
            .unwrap_or_else(|| Value::Array(Vec::new()));
        normalized.insert(key.to_string(), normalized_entries);
    }

    let normalized_value = Value::Object(normalized);
    (!is_empty_tallies_value(&normalized_value)).then_some(normalized_value)
}

fn canonical_tallies_by_facet_section(value: &Value) -> Option<Value> {
    let Some(array) = value.as_array() else {
        return Some(canonicalize_json_value(value));
    };

    let mut normalized: Vec<Value> = array
        .iter()
        .map(|entry| {
            let facet = entry
                .get("facet")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let tallies = canonical_tallies_section(entry.get("tallies").unwrap_or(&Value::Null))
                .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
            json!({
                "facet": facet,
                "tallies": tallies,
            })
        })
        .collect();
    normalized.sort_by_cached_key(canonical_value_string);
    Some(Value::Array(normalized))
}

fn canonicalize_tally_entry_array(value: &Value, kind: &str) -> Value {
    let Some(array) = value.as_array() else {
        return Value::Array(Vec::new());
    };

    let mut normalized: Vec<Value> = array
        .iter()
        .map(|entry| {
            let count = entry.get("count").and_then(Value::as_u64).unwrap_or(0);
            let normalized_value = entry
                .get("value")
                .and_then(Value::as_str)
                .map(|text| normalize_tally_value(kind, text));
            json!({
                "count": count,
                "value": normalized_value,
            })
        })
        .collect();
    normalized.sort_by_cached_key(canonical_value_string);
    Value::Array(normalized)
}

fn normalize_tally_value(kind: &str, value: &str) -> String {
    match kind {
        "detected_license_expression" => normalize_license_expression(value),
        "copyrights" => normalize_tally_copyright_value(value),
        "holders" => normalize_text(value),
        "authors" => normalize_text(value),
        "programming_language" => normalize_text(value),
        _ => normalize_text(value),
    }
}

fn normalize_tally_copyright_value(value: &str) -> String {
    let trimmed = value
        .trim()
        .trim_end_matches(" as indicated by the @authors tag");

    if let Some(rest) = trimmed.strip_prefix("(c) ") {
        let normalized_rest = rest.trim_start_matches(|ch: char| {
            ch.is_ascii_digit() || ch == ' ' || ch == ',' || ch == '-'
        });

        if !normalized_rest.is_empty() && normalized_rest != rest {
            return format!("(c) {}", normalized_rest.trim());
        }
    }

    if let Some(rest) = trimmed.strip_prefix("Copyright (c) ") {
        let normalized_rest = rest.trim_start_matches(|ch: char| {
            ch.is_ascii_digit() || ch == ' ' || ch == ',' || ch == '-'
        });

        if !normalized_rest.is_empty() && normalized_rest != rest {
            return format!("Copyright (c) {}", normalized_rest.trim());
        }
    }

    if let Some(rest) = trimmed.strip_prefix("Copyright ")
        && let Some((yearish, remainder)) = rest.split_once(',')
        && !yearish.is_empty()
        && yearish
            .chars()
            .all(|ch| ch.is_ascii_digit() || ch == ' ' || ch == ',' || ch == '-')
    {
        return format!("Copyright {}", remainder.trim());
    }

    if let Some(rest) = trimmed.strip_prefix("Copyright ") {
        let mut parts = rest.rsplitn(2, ' ');
        let trailing = parts.next().unwrap_or_default();
        let leading = parts.next().unwrap_or_default();
        if !leading.is_empty()
            && trailing
                .chars()
                .all(|ch| ch.is_ascii_digit() || ch == ',' || ch == '-')
        {
            return format!("Copyright {}", leading.trim());
        }
    }

    trimmed.to_string()
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
    let scancode_args = build_scancode_docker_args(context);
    let provenant_args = build_provenant_args(context);
    let (provenant_working_dir, _provenant_input_args) = build_provenant_invocation(context);
    let manifest = CompareRunManifest {
        run_id: context.run_id.clone(),
        target: TargetManifest::new(
            context.target_source_label.clone(),
            context.target_label.clone(),
            context.target_revision.clone(),
            if context.target_source_label == "Target path" {
                Some(PathBuf::from(&context.target_label))
            } else {
                None
            },
            context.target_dir.clone(),
            context.worktree_retained_after_run,
        ),
        repo: context.repo_manifest.clone(),
        scan_profile: context.profile_name.clone(),
        scan_args: context.scan_args.clone(),
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
                working_directory: Some(provenant_working_dir),
            },
        },
        provenant: ProvenantManifest {
            version: context.provenant_version.clone(),
            runtime_revision: context.provenant_runtime_revision.clone(),
            runtime_dirty: context.provenant_runtime_dirty,
            runtime_diff_hash: context.provenant_runtime_diff_hash.clone(),
        },
        scancode: ScancodeManifest {
            image: context.scancode_image.clone(),
            docker_platform: context.scancode_platform.clone(),
            submodule_path: context.scancode_submodule_dir.clone(),
            runtime_revision: context.scancode_runtime_revision.clone(),
            runtime_dirty: context.scancode_runtime_dirty,
            runtime_diff_hash: context.scancode_runtime_diff_hash.clone(),
            docker_memory_limit: scancode_docker_memory_limit(context).map(str::to_string),
            docker_memory_swap_limit: scancode_docker_memory_swap_limit(context)
                .map(str::to_string),
            cache_identity: effective_scancode_cache_identity(context).map(str::to_string),
            cache_key: context.scancode_cache_key.clone(),
            cache_dir: context.scancode_cache_dir.clone(),
            cache_hit: context.scancode_cache_hit,
        },
    };
    write_pretty_json(&context.run_manifest, &manifest)?;
    Ok(())
}

fn build_scancode_cli_args(context: &ContextState) -> Vec<String> {
    let mut args = vec!["--json-pp".to_string(), "/out/scancode.json".to_string()];
    args.extend(rewrite_scan_args(context, AuxiliaryPathFlavor::Scancode));
    args.extend(scancode_ignore_args());
    if context.target_uses_staged_inputs {
        args.extend(
            context
                .target_input_args
                .iter()
                .map(|input| format!("/input/{input}")),
        );
    } else {
        args.push("/input".to_string());
    }
    args
}

fn build_scancode_docker_args(context: &ContextState) -> Vec<String> {
    let mut args = vec![
        "run".to_string(),
        "--rm".to_string(),
        "--platform".to_string(),
        context.scancode_platform.clone(),
    ];
    if let Some(limit) = scancode_docker_memory_limit(context) {
        args.push("--memory".to_string());
        args.push(limit.to_string());
    }
    if let Some(limit) = scancode_docker_memory_swap_limit(context) {
        args.push("--memory-swap".to_string());
        args.push(limit.to_string());
    }
    args.extend([
        "-e".to_string(),
        "SCANCODE_CACHE=/tmp/scancode-cache".to_string(),
        "-e".to_string(),
        "SCANCODE_LICENSE_INDEX_CACHE=/tmp/scancode-license-index-cache".to_string(),
        "-e".to_string(),
        "SCANCODE_PACKAGE_INDEX_CACHE=/tmp/scancode-package-index-cache".to_string(),
        "-e".to_string(),
        "SCANCODE_TEMP=/tmp/scancode-temp".to_string(),
        "-v".to_string(),
        format!("{}:/input:ro", context.target_dir.display()),
        "-v".to_string(),
        format!("{}:/out", context.raw_dir.display()),
    ]);
    if !context.auxiliary_scan_inputs.is_empty() {
        args.push("-v".to_string());
        args.push(format!("{}:/aux:ro", context.auxiliary_dir.display()));
    }
    args.push(context.scancode_image.clone());
    args.extend(build_scancode_cli_args(context));
    args
}

fn build_provenant_args(context: &ContextState) -> Vec<String> {
    let (_working_dir, input_args) = build_provenant_invocation(context);
    let mut args = vec![
        "--json-pp".to_string(),
        context.provenant_json.display().to_string(),
        "--no-license-index-cache".to_string(),
    ];
    args.extend(rewrite_scan_args(context, AuxiliaryPathFlavor::Provenant));
    args.extend(provenant_ignore_args());
    args.extend(input_args);
    args
}

#[derive(Debug, Clone, Copy)]
enum AuxiliaryPathFlavor {
    Provenant,
    Scancode,
}

fn materialize_auxiliary_scan_inputs(context: &ContextState) -> Result<()> {
    if context.auxiliary_scan_inputs.is_empty() {
        return Ok(());
    }
    fs::create_dir_all(&context.auxiliary_dir).with_context(|| {
        format!(
            "failed to create staged auxiliary input directory {}",
            context.auxiliary_dir.display()
        )
    })?;
    for input in &context.auxiliary_scan_inputs {
        materialize_file(
            &input.resolved_path,
            &context.auxiliary_dir.join(&input.staged_name),
        )?;
    }
    Ok(())
}

fn rewrite_scan_args(context: &ContextState, flavor: AuxiliaryPathFlavor) -> Vec<String> {
    context
        .scan_args
        .iter()
        .map(|arg| rewrite_auxiliary_scan_arg(arg, context, flavor))
        .collect()
}

fn rewrite_auxiliary_scan_arg(
    arg: &str,
    context: &ContextState,
    flavor: AuxiliaryPathFlavor,
) -> String {
    let Some(aux) = context.auxiliary_scan_inputs.iter().find(|input| {
        input.original_arg == arg || input.resolved_path.display().to_string() == arg
    }) else {
        return arg.to_string();
    };

    match flavor {
        AuxiliaryPathFlavor::Provenant => context
            .auxiliary_dir
            .join(&aux.staged_name)
            .display()
            .to_string(),
        AuxiliaryPathFlavor::Scancode => format!("/aux/{}", aux.staged_name),
    }
}

fn auxiliary_scan_inputs(scan_args: &[String]) -> Result<Vec<AuxiliaryScanInput>> {
    let mut path_args = Vec::new();
    let mut index = 0;
    while index < scan_args.len() {
        let arg = &scan_args[index];
        if scan_arg_uses_local_path(arg) {
            let Some(raw_value) = scan_args.get(index + 1) else {
                bail!("scan flag {arg} requires a path argument");
            };
            let candidate_path = PathBuf::from(raw_value);
            if candidate_path.exists() {
                path_args.push((raw_value.clone(), realpath(&candidate_path)?));
            }
            index += 2;
            continue;
        }
        index += 1;
    }

    let resolved_paths: Vec<PathBuf> = path_args.iter().map(|(_, path)| path.clone()).collect();
    let staged_names = staged_input_names(&resolved_paths);
    Ok(path_args
        .into_iter()
        .zip(staged_names)
        .map(
            |((original_arg, resolved_path), staged_name)| AuxiliaryScanInput {
                original_arg,
                resolved_path,
                staged_name,
            },
        )
        .collect())
}

fn scan_arg_uses_local_path(arg: &str) -> bool {
    matches!(
        arg,
        "--license-policy" | "--license-rules-path" | "--custom-template"
    )
}

fn staged_input_names(paths: &[PathBuf]) -> Vec<String> {
    let total = paths.len();
    paths
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("input.json");
            if total == 1 {
                file_name.to_string()
            } else {
                format!("{index:02}-{file_name}")
            }
        })
        .collect()
}

fn local_target_revision(paths: &[PathBuf]) -> String {
    if paths.len() == 1 {
        return current_git_revision(&paths[0])
            .unwrap_or_else(|| "current local checkout".to_string());
    }

    let revisions: BTreeSet<String> = paths
        .iter()
        .filter_map(|path| current_git_revision(path))
        .collect();
    if revisions.len() == 1 {
        revisions.into_iter().next().unwrap()
    } else {
        "multiple local inputs".to_string()
    }
}

fn scancode_ignore_args() -> Vec<String> {
    [".git", ".git/**", "**/.git", "**/.git/**"]
        .into_iter()
        .flat_map(|pattern| ["--ignore".to_string(), pattern.to_string()])
        .collect()
}

fn provenant_ignore_args() -> Vec<String> {
    Vec::new()
}

fn effective_scancode_cache_identity(context: &ContextState) -> Option<&str> {
    context
        .repo_manifest
        .resolved_sha
        .as_deref()
        .or(context.target_scancode_cache_identity.as_deref())
}

fn scancode_docker_memory_limit(context: &ContextState) -> Option<&str> {
    context.scancode_docker_memory_limit.as_deref()
}

fn scancode_docker_memory_swap_limit(context: &ContextState) -> Option<&str> {
    context.scancode_docker_memory_swap_limit.as_deref()
}

fn build_scancode_cache_key(context: &ContextState) -> Result<String> {
    let cache_identity = effective_scancode_cache_identity(context)
        .context("ScanCode cache identity missing while building cache key")?;
    let key_input = json!({
        "repo_url": context.repo_manifest.url,
        "cache_identity": cache_identity,
        "scancode_image": context.scancode_image,
        "scancode_runtime_revision": context.scancode_runtime_revision,
        "scancode_runtime_dirty": context.scancode_runtime_dirty,
        "scancode_runtime_diff_hash": context.scancode_runtime_diff_hash,
        "scancode_platform": context.scancode_platform,
        "docker_memory_limit": scancode_docker_memory_limit(context),
        "docker_memory_swap_limit": scancode_docker_memory_swap_limit(context),
        "scancode_cli_args": build_scancode_cli_args(context),
    });
    let mut hasher = sha2::Sha256::default();
    hasher.update(serde_json::to_vec(&key_input)?);
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect();
    let label = context
        .repo_manifest
        .url
        .as_deref()
        .map(|repo_url| derive_repo_name_from_url(repo_url, "scancode"))
        .unwrap_or_else(|| cache_identity.to_string());
    Ok(format!(
        "{}-{}",
        sanitize_label(&label, "scancode"),
        &digest[..16]
    ))
}

fn scancode_cache_complete(cache_dir: &Path) -> bool {
    cache_json_path(cache_dir).is_file() && cache_manifest_path(cache_dir).is_file()
}

fn cache_json_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join("scancode.json")
}

fn cache_stdout_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join("scancode-stdout.txt")
}

fn cache_manifest_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join("manifest.json")
}

fn validate_and_materialize_scancode_cache_hit(context: &ContextState) -> Result<Option<String>> {
    validate_scancode_cache_hit(context)?;
    materialize_scancode_cache_hit(context)
}

fn validate_scancode_cache_hit(context: &ContextState) -> Result<()> {
    let cache_dir = context
        .scancode_cache_dir
        .as_ref()
        .context("ScanCode cache dir missing on cache hit")?;
    let expected_cache_key = context
        .scancode_cache_key
        .as_deref()
        .context("ScanCode cache key missing on cache hit")?;
    let expected_cache_identity = effective_scancode_cache_identity(context)
        .context("ScanCode cache identity missing on cache hit")?;
    let manifest: ScancodeCacheEntryManifest = serde_json::from_reader(BufReader::new(
        File::open(cache_manifest_path(cache_dir)).with_context(|| {
            format!(
                "failed to open ScanCode cache manifest {}",
                cache_manifest_path(cache_dir).display()
            )
        })?,
    ))
    .with_context(|| {
        format!(
            "failed to parse ScanCode cache manifest {}",
            cache_manifest_path(cache_dir).display()
        )
    })?;
    if manifest.cache_key != expected_cache_key {
        bail!(
            "ScanCode cache key mismatch: expected {}, found {}",
            expected_cache_key,
            manifest.cache_key
        );
    }
    if manifest.cache_identity.as_deref() != Some(expected_cache_identity) {
        bail!(
            "ScanCode cache identity mismatch: expected {}, found {:?}",
            expected_cache_identity,
            manifest.cache_identity
        );
    }
    if manifest.repo_url != context.repo_manifest.url {
        bail!(
            "ScanCode cache repo URL mismatch: expected {:?}, found {:?}",
            context.repo_manifest.url,
            manifest.repo_url
        );
    }
    if manifest.scan_args != build_scancode_cli_args(context) {
        bail!("ScanCode cache args mismatch");
    }
    if manifest.scancode_image != context.scancode_image {
        bail!(
            "ScanCode cache image mismatch: expected {}, found {}",
            context.scancode_image,
            manifest.scancode_image
        );
    }
    if manifest.scancode_runtime_revision != context.scancode_runtime_revision {
        bail!(
            "ScanCode runtime revision mismatch: expected {}, found {}",
            context.scancode_runtime_revision,
            manifest.scancode_runtime_revision
        );
    }
    if manifest.scancode_runtime_dirty != context.scancode_runtime_dirty {
        bail!(
            "ScanCode runtime dirty-state mismatch: expected {}, found {}",
            context.scancode_runtime_dirty,
            manifest.scancode_runtime_dirty
        );
    }
    if manifest.scancode_runtime_diff_hash != context.scancode_runtime_diff_hash {
        bail!("ScanCode runtime diff hash mismatch");
    }
    if manifest.docker_memory_limit != scancode_docker_memory_limit(context).map(str::to_string) {
        bail!("ScanCode docker memory limit mismatch");
    }
    if manifest.docker_memory_swap_limit
        != scancode_docker_memory_swap_limit(context).map(str::to_string)
    {
        bail!("ScanCode docker memory swap limit mismatch");
    }
    serde_json::from_reader::<_, Value>(BufReader::new(
        File::open(cache_json_path(cache_dir)).with_context(|| {
            format!(
                "failed to open cached ScanCode JSON {}",
                cache_json_path(cache_dir).display()
            )
        })?,
    ))
    .with_context(|| {
        format!(
            "failed to parse cached ScanCode JSON {}",
            cache_json_path(cache_dir).display()
        )
    })?;
    Ok(())
}

fn materialize_scancode_cache_hit(context: &ContextState) -> Result<Option<String>> {
    let cache_dir = context
        .scancode_cache_dir
        .as_ref()
        .context("ScanCode cache dir missing on cache hit")?;
    materialize_file(&cache_json_path(cache_dir), &context.scancode_json)?;
    if cache_stdout_path(cache_dir).is_file() {
        materialize_file(&cache_stdout_path(cache_dir), &context.scancode_stdout)?;
        Ok(None)
    } else {
        Ok(write_placeholder_scancode_stdout(&context.scancode_stdout))
    }
}

fn persist_scancode_cache_entry(context: &ContextState) -> Result<()> {
    let Some(cache_dir) = &context.scancode_cache_dir else {
        return Ok(());
    };
    fs::create_dir_all(cache_dir).with_context(|| {
        format!(
            "failed to create ScanCode cache dir {}",
            cache_dir.display()
        )
    })?;
    materialize_file(&context.scancode_json, &cache_json_path(cache_dir))?;
    let cache_stdout = if context.scancode_stdout.is_file() {
        let cache_stdout = cache_stdout_path(cache_dir);
        materialize_file(&context.scancode_stdout, &cache_stdout)?;
        Some(cache_stdout)
    } else {
        None
    };
    let manifest = ScancodeCacheEntryManifest {
        cache_key: context.scancode_cache_key.clone().unwrap_or_default(),
        cache_identity: effective_scancode_cache_identity(context).map(str::to_string),
        target_label: context.target_label.clone(),
        target_revision: context.target_revision.clone(),
        repo_url: context.repo_manifest.url.clone(),
        scan_args: build_scancode_cli_args(context),
        scancode_image: context.scancode_image.clone(),
        scancode_runtime_revision: context.scancode_runtime_revision.clone(),
        scancode_runtime_dirty: context.scancode_runtime_dirty,
        scancode_runtime_diff_hash: context.scancode_runtime_diff_hash.clone(),
        docker_memory_limit: scancode_docker_memory_limit(context).map(str::to_string),
        docker_memory_swap_limit: scancode_docker_memory_swap_limit(context).map(str::to_string),
        scancode_json: cache_json_path(cache_dir),
        scancode_stdout: cache_stdout,
    };
    write_pretty_json(&cache_manifest_path(cache_dir), &manifest)?;
    Ok(())
}

fn write_placeholder_scancode_stdout(path: &Path) -> Option<String> {
    write_optional_command_log(path, SCANCODE_PLACEHOLDER_LOG_MESSAGE)
}

fn materialize_file(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parent directory {}", parent.display()))?;
    }
    if dst.exists() {
        fs::remove_file(dst)
            .with_context(|| format!("failed to remove existing file {}", dst.display()))?;
    }
    match fs::hard_link(src, dst) {
        Ok(()) => Ok(()),
        Err(_) => {
            fs::copy(src, dst).with_context(|| {
                format!(
                    "failed to copy cached artifact {} -> {}",
                    src.display(),
                    dst.display()
                )
            })?;
            Ok(())
        }
    }
}

fn print_summary_table(path: &Path) -> Result<()> {
    let labels = ["Metric", "ScanCode", "Provenant", "Delta", "Notes"];
    let _ = render_tsv_table(path, &labels)?;
    Ok(())
}

fn optional_artifact_display(path: &Path) -> String {
    if path.is_file() {
        match fs::read_to_string(path) {
            Ok(content) if content == SCANCODE_PLACEHOLDER_LOG_MESSAGE => {
                format!("placeholder diagnostic log: {}", path.display())
            }
            _ => path.display().to_string(),
        }
    } else {
        format!(
            "not written (optional diagnostic; intended path: {})",
            path.display()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_context() -> ContextState {
        ContextState {
            project_root: PathBuf::from("/tmp/project"),
            scancode_submodule_dir: PathBuf::from("/tmp/project/reference/scancode-toolkit"),
            run_id: "run-id".to_string(),
            run_dir: PathBuf::from("/tmp/project/.provenant/compare-runs/run-id"),
            raw_dir: PathBuf::from("/tmp/project/.provenant/compare-runs/run-id/raw"),
            comparison_dir: PathBuf::from("/tmp/project/.provenant/compare-runs/run-id/comparison"),
            samples_dir: PathBuf::from(
                "/tmp/project/.provenant/compare-runs/run-id/comparison/samples",
            ),
            run_manifest: PathBuf::from(
                "/tmp/project/.provenant/compare-runs/run-id/run-manifest.json",
            ),
            summary_json: PathBuf::from(
                "/tmp/project/.provenant/compare-runs/run-id/comparison/summary.json",
            ),
            summary_tsv: PathBuf::from(
                "/tmp/project/.provenant/compare-runs/run-id/comparison/summary.tsv",
            ),
            target_dir: PathBuf::from("/tmp/target"),
            auxiliary_dir: PathBuf::from(
                "/tmp/project/.provenant/compare-runs/run-id/auxiliary-inputs",
            ),
            target_resolved_paths: Vec::new(),
            target_input_args: vec![".".to_string()],
            target_uses_staged_inputs: false,
            auxiliary_scan_inputs: Vec::new(),
            target_label: "/tmp/target".to_string(),
            target_source_label: "Target path".to_string(),
            target_revision: "current local checkout".to_string(),
            target_scancode_cache_identity: None,
            repo_manifest: RepoManifest::new(None, None, None, None),
            worktree_retained_after_run: true,
            profile_name: Some("common".to_string()),
            scan_args: vec![
                "-clupe".to_string(),
                "--system-package".to_string(),
                "--strip-root".to_string(),
                "--processes".to_string(),
                "4".to_string(),
            ],
            provenant_bin: PathBuf::from("/tmp/project/target/release/provenant"),
            provenant_json: PathBuf::from(
                "/tmp/project/.provenant/compare-runs/run-id/raw/provenant.json",
            ),
            provenant_stdout: PathBuf::from(
                "/tmp/project/.provenant/compare-runs/run-id/raw/provenant-stdout.txt",
            ),
            provenant_version: "0.0.13".to_string(),
            provenant_runtime_revision: Some("prov-rev".to_string()),
            provenant_runtime_dirty: false,
            provenant_runtime_diff_hash: None,
            scancode_json: PathBuf::from(
                "/tmp/project/.provenant/compare-runs/run-id/raw/scancode.json",
            ),
            scancode_stdout: PathBuf::from(
                "/tmp/project/.provenant/compare-runs/run-id/raw/scancode-stdout.txt",
            ),
            scancode_image: "provenant-scancode-local:test".to_string(),
            scancode_platform: "linux/amd64".to_string(),
            scancode_runtime_revision: "runtime-rev".to_string(),
            scancode_runtime_dirty: false,
            scancode_runtime_diff_hash: None,
            scancode_docker_memory_limit: Some("12g".to_string()),
            scancode_docker_memory_swap_limit: Some("12g".to_string()),
            scancode_cache_root: PathBuf::from("/tmp/project/.provenant/scancode-cache"),
            scancode_cache_dir: None,
            scancode_cache_key: None,
            scancode_cache_hit: false,
        }
    }

    fn write_valid_cache_manifest(cache_dir: &Path, context: &ContextState) {
        let manifest = ScancodeCacheEntryManifest {
            cache_key: build_scancode_cache_key(context).unwrap(),
            cache_identity: effective_scancode_cache_identity(context).map(str::to_string),
            target_label: context.target_label.clone(),
            target_revision: context.target_revision.clone(),
            repo_url: context.repo_manifest.url.clone(),
            scan_args: build_scancode_cli_args(context),
            scancode_image: context.scancode_image.clone(),
            scancode_runtime_revision: context.scancode_runtime_revision.clone(),
            scancode_runtime_dirty: context.scancode_runtime_dirty,
            scancode_runtime_diff_hash: context.scancode_runtime_diff_hash.clone(),
            docker_memory_limit: scancode_docker_memory_limit(context).map(str::to_string),
            docker_memory_swap_limit: scancode_docker_memory_swap_limit(context)
                .map(str::to_string),
            scancode_json: cache_json_path(cache_dir),
            scancode_stdout: None,
        };
        write_pretty_json(&cache_manifest_path(cache_dir), &manifest).unwrap();
    }

    #[test]
    fn target_path_cache_key_uses_explicit_identity_not_path() {
        let mut first = test_context();
        first.target_label = "/tmp/chromium-a".to_string();
        first.target_scancode_cache_identity = Some("chromium@2befda78".to_string());

        let mut second = test_context();
        second.target_label = "/different/path/chromium-b".to_string();
        second.target_scancode_cache_identity = Some("chromium@2befda78".to_string());

        assert_eq!(
            build_scancode_cache_key(&first).unwrap(),
            build_scancode_cache_key(&second).unwrap()
        );
    }

    #[test]
    fn cache_complete_accepts_json_and_manifest_without_stdout() {
        let cache_dir = unique_temp_dir("cache-complete");
        fs::create_dir_all(&cache_dir).unwrap();
        fs::write(cache_json_path(&cache_dir), "{}\n").unwrap();
        fs::write(cache_manifest_path(&cache_dir), "{}\n").unwrap();

        assert!(scancode_cache_complete(&cache_dir));

        let _ = fs::remove_dir_all(&cache_dir);
    }

    #[test]
    fn placeholder_scancode_stdout_is_materialized_when_cache_log_is_missing() {
        let temp_root = unique_temp_dir("placeholder-log");
        let cache_dir = temp_root.join("cache");
        let raw_dir = temp_root.join("raw");
        fs::create_dir_all(&cache_dir).unwrap();
        fs::create_dir_all(&raw_dir).unwrap();
        fs::write(cache_json_path(&cache_dir), "{}\n").unwrap();

        let mut context = test_context();
        context.target_scancode_cache_identity = Some("chromium@2befda78".to_string());
        context.scancode_cache_dir = Some(cache_dir.clone());
        context.scancode_cache_key = Some(build_scancode_cache_key(&context).unwrap());
        context.scancode_json = raw_dir.join("scancode.json");
        context.scancode_stdout = raw_dir.join("scancode-stdout.txt");

        write_valid_cache_manifest(&cache_dir, &context);

        let log_warning = materialize_scancode_cache_hit(&context).unwrap();

        assert!(context.scancode_json.is_file());
        let placeholder = fs::read_to_string(&context.scancode_stdout).unwrap();
        assert!(placeholder.contains("stdout was not captured"));
        assert!(log_warning.is_none());

        let _ = fs::remove_dir_all(&temp_root);
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("compare-outputs-{name}-{nanos}"))
    }

    #[test]
    fn arm64_docker_hosts_use_native_scancode_platform() {
        let docker_info = DockerServerInfo {
            architecture: "aarch64".to_string(),
            mem_total_bytes: Some(8 * 1024 * 1024 * 1024),
        };

        assert_eq!(
            effective_scancode_docker_platform(Some(&docker_info)),
            "linux/arm64/v8"
        );
        assert_eq!(
            sanitize_docker_platform_for_tag("linux/arm64/v8"),
            "linux-arm64-v8"
        );
    }

    #[test]
    fn common_profile_skips_memory_cap_when_docker_engine_is_smaller() {
        assert_eq!(
            effective_scancode_docker_memory_limit(Some("common"), Some(8 * 1024 * 1024 * 1024)),
            None
        );
        assert_eq!(
            effective_scancode_docker_memory_limit(
                Some("common"),
                Some(COMMON_PROFILE_SCANCODE_MEMORY_LIMIT_BYTES),
            ),
            Some("12g".to_string())
        );
    }

    #[test]
    fn optional_command_log_returns_warning_when_log_path_is_unwritable() {
        let temp_root = unique_temp_dir("optional-log-warning");
        fs::create_dir_all(&temp_root).unwrap();
        let blocking_file = temp_root.join("not-a-directory");
        fs::write(&blocking_file, "block").unwrap();

        let warning = write_optional_command_log(&blocking_file.join("command.log"), "hello");

        assert!(warning.is_some());
        assert!(warning.unwrap().contains("failed to create log directory"));

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn optional_artifact_display_marks_missing_logs_as_optional() {
        let missing = PathBuf::from("/tmp/missing-command-log.txt");

        let display = optional_artifact_display(&missing);

        assert!(display.contains("not written (optional diagnostic"));
        assert!(display.contains(missing.to_str().unwrap()));
    }

    #[test]
    fn optional_artifact_display_marks_placeholder_logs() {
        let temp_root = unique_temp_dir("placeholder-display");
        fs::create_dir_all(&temp_root).unwrap();
        let placeholder = temp_root.join("scancode-stdout.txt");
        fs::write(&placeholder, SCANCODE_PLACEHOLDER_LOG_MESSAGE).unwrap();

        let display = optional_artifact_display(&placeholder);

        assert!(display.contains("placeholder diagnostic log"));
        assert!(display.contains(placeholder.to_str().unwrap()));

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn command_failure_message_includes_output_and_log_warning() {
        let message = build_command_failure_message(
            "docker",
            &["run".to_string(), "example".to_string()],
            "scanner failed\nmore detail",
            Some("failed to write optional command log /tmp/log.txt: permission denied"),
        );

        assert!(message.contains("command failed: docker run example"));
        assert!(message.contains("failed to write optional command log /tmp/log.txt"));
        assert!(message.contains("--- command output ---"));
        assert!(message.contains("scanner failed"));
        assert!(message.contains("more detail"));
    }

    #[test]
    fn cache_validation_rejects_empty_manifest() {
        let temp_root = unique_temp_dir("invalid-manifest");
        let cache_dir = temp_root.join("cache");
        fs::create_dir_all(&cache_dir).unwrap();
        fs::write(cache_json_path(&cache_dir), "{}\n").unwrap();
        fs::write(cache_manifest_path(&cache_dir), "{}\n").unwrap();

        let mut context = test_context();
        context.target_scancode_cache_identity = Some("chromium@2befda78".to_string());
        context.scancode_cache_dir = Some(cache_dir.clone());
        context.scancode_cache_key = Some(build_scancode_cache_key(&context).unwrap());

        let error = validate_scancode_cache_hit(&context)
            .unwrap_err()
            .to_string();
        assert!(error.contains("failed to parse ScanCode cache manifest"));

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn validate_scancode_output_on_failure_accepts_header_errors() {
        let temp_root = unique_temp_dir("scancode-failure-json-ok");
        let raw_dir = temp_root.join("raw");
        fs::create_dir_all(&raw_dir).unwrap();

        let mut context = test_context();
        context.scancode_json = raw_dir.join("scancode.json");
        fs::write(
            &context.scancode_json,
            r#"{"headers":[{"errors":["Path: input/package.json","Path: input/package-lock.json"]}],"files":[{"path":"package.json","scan_errors":["workspace assembly failed"]},{"path":"package-lock.json","scan_errors":["workspace assembly failed"]}],"packages":[]}"#,
        )
        .unwrap();

        let scan_error_count = validate_scancode_output_on_failure(&context).unwrap();

        assert_eq!(scan_error_count, 2);

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn validate_scancode_output_on_failure_rejects_missing_header_errors() {
        let temp_root = unique_temp_dir("scancode-failure-json-no-errors");
        let raw_dir = temp_root.join("raw");
        fs::create_dir_all(&raw_dir).unwrap();

        let mut context = test_context();
        context.scancode_json = raw_dir.join("scancode.json");
        fs::write(
            &context.scancode_json,
            r#"{"headers":[{"errors":[]}],"files":[],"packages":[]}"#,
        )
        .unwrap();

        let error = validate_scancode_output_on_failure(&context)
            .unwrap_err()
            .to_string();

        assert!(error.contains("produced no header-level scan errors"));

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn validate_scancode_output_on_failure_rejects_non_path_header_errors() {
        let temp_root = unique_temp_dir("scancode-failure-json-bad-error-kind");
        let raw_dir = temp_root.join("raw");
        fs::create_dir_all(&raw_dir).unwrap();

        let mut context = test_context();
        context.scancode_json = raw_dir.join("scancode.json");
        fs::write(
            &context.scancode_json,
            r#"{"headers":[{"errors":["Path: input/package.json","fatal docker failure"]}],"files":[],"packages":[]}"#,
        )
        .unwrap();

        let error = validate_scancode_output_on_failure(&context)
            .unwrap_err()
            .to_string();

        assert!(error.contains("non-scan-error header entry"));

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn validate_scancode_output_on_failure_rejects_non_empty_header_message() {
        let temp_root = unique_temp_dir("scancode-failure-json-header-message");
        let raw_dir = temp_root.join("raw");
        fs::create_dir_all(&raw_dir).unwrap();

        let mut context = test_context();
        context.scancode_json = raw_dir.join("scancode.json");
        fs::write(
            &context.scancode_json,
            r#"{"headers":[{"message":"fatal runtime error","errors":["Path: input/package.json"]}],"files":[],"packages":[]}"#,
        )
        .unwrap();

        let error = validate_scancode_output_on_failure(&context)
            .unwrap_err()
            .to_string();

        assert!(error.contains("non-empty header message"));

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn validate_scancode_output_on_failure_rejects_unmatched_header_error_path() {
        let temp_root = unique_temp_dir("scancode-failure-json-unmatched-path");
        let raw_dir = temp_root.join("raw");
        fs::create_dir_all(&raw_dir).unwrap();

        let mut context = test_context();
        context.scancode_json = raw_dir.join("scancode.json");
        fs::write(
            &context.scancode_json,
            r#"{"headers":[{"errors":["Path: input/package.json"]}],"files":[{"path":"package.json","scan_errors":[]}],"packages":[]}"#,
        )
        .unwrap();

        let error = validate_scancode_output_on_failure(&context)
            .unwrap_err()
            .to_string();

        assert!(error.contains("had no matching file scan_errors"));

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn build_scancode_docker_args_uses_ephemeral_cache_envs() {
        let context = test_context();

        let args = build_scancode_docker_args(&context);

        assert!(args.windows(2).any(|pair| pair == ["--memory", "12g"]));
        assert!(args.windows(2).any(|pair| pair == ["--memory-swap", "12g"]));

        assert!(
            args.windows(2)
                .any(|pair| pair == ["-e", "SCANCODE_CACHE=/tmp/scancode-cache"])
        );
        assert!(args.windows(2).any(|pair| {
            pair == [
                "-e",
                "SCANCODE_LICENSE_INDEX_CACHE=/tmp/scancode-license-index-cache",
            ]
        }));
        assert!(args.windows(2).any(|pair| {
            pair == [
                "-e",
                "SCANCODE_PACKAGE_INDEX_CACHE=/tmp/scancode-package-index-cache",
            ]
        }));
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-e", "SCANCODE_TEMP=/tmp/scancode-temp"])
        );
    }

    #[test]
    fn build_provenant_args_disables_persistent_license_cache() {
        let context = test_context();

        let args = build_provenant_args(&context);

        assert!(args.iter().any(|arg| arg == "--no-license-index-cache"));
        assert!(args.windows(2).any(|pair| pair == ["--processes", "4"]));
    }

    #[test]
    fn cache_validation_rejects_docker_memory_mismatch() {
        let temp_root = unique_temp_dir("cache-memory-mismatch");
        let cache_dir = temp_root.join("cache");
        fs::create_dir_all(&cache_dir).unwrap();
        fs::write(cache_json_path(&cache_dir), "{}\n").unwrap();

        let mut context = test_context();
        context.target_scancode_cache_identity = Some("defectdojo@rev".to_string());
        context.scancode_cache_dir = Some(cache_dir.clone());
        context.scancode_cache_key = Some(build_scancode_cache_key(&context).unwrap());

        write_valid_cache_manifest(&cache_dir, &context);

        let manifest_path = cache_manifest_path(&cache_dir);
        let mut manifest: ScancodeCacheEntryManifest =
            serde_json::from_reader(BufReader::new(File::open(&manifest_path).unwrap())).unwrap();
        manifest.docker_memory_limit = Some("8g".to_string());
        write_pretty_json(&manifest_path, &manifest).unwrap();

        let error = validate_scancode_cache_hit(&context)
            .unwrap_err()
            .to_string();
        assert!(error.contains("docker memory limit mismatch"));

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn build_provenant_args_uses_staged_input_file_for_single_file_targets() {
        let temp_root = unique_temp_dir("single-file-provenant-args");
        fs::create_dir_all(&temp_root).unwrap();
        let staged_input = temp_root.join("input");
        fs::create_dir_all(&staged_input).unwrap();
        fs::write(staged_input.join("fixture.txt"), "fixture").unwrap();

        let mut context = test_context();
        context.target_dir = staged_input;
        context.target_input_args = vec!["fixture.txt".to_string()];
        context.target_uses_staged_inputs = true;

        let args = build_provenant_args(&context);

        assert_eq!(args.last().map(String::as_str), Some("fixture.txt"));

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn build_provenant_invocation_uses_parent_dir_for_single_file_targets() {
        let temp_root = unique_temp_dir("single-file-provenant-invocation");
        fs::create_dir_all(&temp_root).unwrap();
        let staged_input = temp_root.join("input");
        fs::create_dir_all(&staged_input).unwrap();
        fs::write(staged_input.join("fixture.txt"), "fixture").unwrap();

        let mut context = test_context();
        context.target_dir = staged_input.clone();
        context.target_input_args = vec!["fixture.txt".to_string()];
        context.target_uses_staged_inputs = true;

        let (working_dir, input_args) = build_provenant_invocation(&context);

        assert_eq!(working_dir, staged_input);
        assert_eq!(input_args, vec!["fixture.txt"]);

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn prepare_context_stages_single_file_targets_as_input() {
        let temp_root = unique_temp_dir("single-file-target-context");
        fs::create_dir_all(&temp_root).unwrap();
        let fixture = temp_root.join("fixture.txt");
        fs::write(&fixture, "fixture").unwrap();

        let args = Args {
            repo_url: None,
            target_path: vec![fixture.clone()],
            scancode_cache_identity: Some("fixture@rev".to_string()),
            repo_ref: None,
            profile: None,
            scan_args: Vec::new(),
        };

        let context = prepare_context(&args, vec!["--copyright".to_string()]).unwrap();

        assert_eq!(
            context.target_label,
            realpath(&fixture).unwrap().display().to_string()
        );
        assert_eq!(
            context
                .target_dir
                .file_name()
                .and_then(|name| name.to_str()),
            Some("input")
        );
        assert_eq!(context.target_input_args, vec!["fixture.txt"]);
        assert_ne!(context.target_dir, fixture);

        let _ = fs::remove_dir_all(&temp_root);
        let _ = fs::remove_dir_all(&context.run_dir);
    }

    #[test]
    fn prepare_target_materializes_single_file_target_into_staged_input() {
        let temp_root = unique_temp_dir("single-file-target-prepare");
        fs::create_dir_all(&temp_root).unwrap();
        let fixture = temp_root.join("fixture.txt");
        fs::write(&fixture, "fixture contents").unwrap();

        let mut context = test_context();
        context.target_dir = temp_root.join("run/input");
        context.target_resolved_paths = vec![fixture.clone()];
        context.target_input_args = vec!["fixture.txt".to_string()];
        context.target_uses_staged_inputs = true;
        let args = Args {
            repo_url: None,
            target_path: vec![fixture.clone()],
            scancode_cache_identity: Some("fixture@rev".to_string()),
            repo_ref: None,
            profile: None,
            scan_args: Vec::new(),
        };

        let _guard = prepare_target(&mut context, &args).unwrap();

        assert!(context.target_dir.is_dir());
        assert_eq!(
            fs::read_to_string(context.target_dir.join("fixture.txt")).unwrap(),
            "fixture contents"
        );

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn scancode_ignore_args_keep_git_control_paths_without_hiding_gitmodules() {
        let args = scancode_ignore_args();

        assert!(args.windows(2).any(|pair| pair == ["--ignore", ".git"]));
        assert!(args.windows(2).any(|pair| pair == ["--ignore", ".git/**"]));
        assert!(args.windows(2).any(|pair| pair == ["--ignore", "**/.git"]));
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--ignore", "**/.git/**"])
        );
        assert!(!args.iter().any(|arg| arg == "*.git*"));
        assert!(!args.iter().any(|arg| arg == ".gitmodules"));
    }

    #[test]
    fn provenant_ignore_args_do_not_hide_legitimate_source_trees() {
        let args = provenant_ignore_args();

        assert!(!args.windows(2).any(|pair| pair == ["--ignore", "target/*"]));
        assert!(!args.iter().any(|arg| arg == ".git"));
        assert!(!args.iter().any(|arg| arg == ".git/**"));
        assert!(!args.iter().any(|arg| arg == "**/.git"));
        assert!(!args.iter().any(|arg| arg == "**/.git/**"));
    }

    #[test]
    fn prepare_context_rejects_blank_scancode_cache_identity() {
        let args = Args {
            repo_url: None,
            target_path: vec![PathBuf::from("/tmp/chromium")],
            scancode_cache_identity: Some("   ".to_string()),
            repo_ref: None,
            profile: None,
            scan_args: Vec::new(),
        };

        let error = prepare_context(&args, vec!["-p".to_string()])
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("must not be blank"));
    }

    #[test]
    fn prepare_context_stages_multi_file_targets_with_numbered_inputs() {
        let temp_root = unique_temp_dir("multi-file-target-context");
        fs::create_dir_all(&temp_root).unwrap();
        let first = temp_root.join("first.json");
        let second = temp_root.join("second.json");
        fs::write(&first, "first").unwrap();
        fs::write(&second, "second").unwrap();

        let args = Args {
            repo_url: None,
            target_path: vec![first.clone(), second.clone()],
            scancode_cache_identity: Some("pair@rev".to_string()),
            repo_ref: None,
            profile: None,
            scan_args: Vec::new(),
        };

        let context = prepare_context(&args, vec!["--from-json".to_string()]).unwrap();

        assert!(context.target_uses_staged_inputs);
        assert_eq!(
            context.target_input_args,
            vec!["00-first.json", "01-second.json"]
        );
        assert_eq!(
            context
                .target_dir
                .file_name()
                .and_then(|name| name.to_str()),
            Some("input")
        );

        let _ = fs::remove_dir_all(&temp_root);
        let _ = fs::remove_dir_all(&context.run_dir);
    }

    #[test]
    fn prepare_target_materializes_multi_file_targets_into_staged_input_dir() {
        let temp_root = unique_temp_dir("multi-file-target-prepare");
        fs::create_dir_all(&temp_root).unwrap();
        let first = temp_root.join("first.json");
        let second = temp_root.join("second.json");
        fs::write(&first, "first contents").unwrap();
        fs::write(&second, "second contents").unwrap();

        let mut context = test_context();
        context.target_dir = temp_root.join("run/input");
        context.target_resolved_paths = vec![first.clone(), second.clone()];
        context.target_input_args = vec!["00-first.json".to_string(), "01-second.json".to_string()];
        context.target_uses_staged_inputs = true;
        let args = Args {
            repo_url: None,
            target_path: vec![first.clone(), second.clone()],
            scancode_cache_identity: Some("pair@rev".to_string()),
            repo_ref: None,
            profile: None,
            scan_args: Vec::new(),
        };

        let _guard = prepare_target(&mut context, &args).unwrap();

        assert!(context.target_dir.is_dir());
        assert_eq!(
            fs::read_to_string(context.target_dir.join("00-first.json")).unwrap(),
            "first contents"
        );
        assert_eq!(
            fs::read_to_string(context.target_dir.join("01-second.json")).unwrap(),
            "second contents"
        );

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn build_scancode_and_provenant_args_include_all_staged_multi_inputs() {
        let mut context = test_context();
        context.target_dir = PathBuf::from("/tmp/staged-inputs");
        context.target_input_args = vec!["00-first.json".to_string(), "01-second.json".to_string()];
        context.target_uses_staged_inputs = true;

        let scancode_args = build_scancode_cli_args(&context);
        let provenant_args = build_provenant_args(&context);

        assert!(scancode_args.ends_with(&[
            "/input/00-first.json".to_string(),
            "/input/01-second.json".to_string(),
        ]));
        assert!(
            provenant_args.ends_with(&["00-first.json".to_string(), "01-second.json".to_string(),])
        );
    }

    #[test]
    fn build_scancode_and_provenant_args_rewrite_auxiliary_policy_path() {
        let mut context = test_context();
        context.scan_args = vec![
            "--from-json".to_string(),
            "--license-policy".to_string(),
            "/tmp/original/policy.yml".to_string(),
            "--filter-clues".to_string(),
        ];
        context.auxiliary_dir = PathBuf::from("/tmp/run/auxiliary-inputs");
        context.auxiliary_scan_inputs = vec![AuxiliaryScanInput {
            original_arg: "/tmp/original/policy.yml".to_string(),
            resolved_path: PathBuf::from("/tmp/original/policy.yml"),
            staged_name: "policy.yml".to_string(),
        }];

        let scancode_args = build_scancode_cli_args(&context);
        let provenant_args = build_provenant_args(&context);

        assert!(scancode_args.contains(&"/aux/policy.yml".to_string()));
        assert!(provenant_args.contains(&"/tmp/run/auxiliary-inputs/policy.yml".to_string()));
    }

    #[test]
    fn prepare_target_materializes_auxiliary_scan_inputs() {
        let temp_root = unique_temp_dir("auxiliary-scan-inputs");
        fs::create_dir_all(&temp_root).unwrap();
        let fixture = temp_root.join("fixture.json");
        let policy = temp_root.join("policy.yml");
        fs::write(&fixture, "fixture contents").unwrap();
        fs::write(&policy, "license_policies: []\n").unwrap();

        let mut context = test_context();
        context.target_dir = temp_root.join("run/input");
        context.auxiliary_dir = temp_root.join("run/auxiliary-inputs");
        context.target_resolved_paths = vec![fixture.clone()];
        context.target_input_args = vec!["fixture.json".to_string()];
        context.target_uses_staged_inputs = true;
        context.auxiliary_scan_inputs = vec![AuxiliaryScanInput {
            original_arg: policy.display().to_string(),
            resolved_path: policy.clone(),
            staged_name: "policy.yml".to_string(),
        }];
        let args = Args {
            repo_url: None,
            target_path: vec![fixture.clone()],
            scancode_cache_identity: Some("fixture@rev".to_string()),
            repo_ref: None,
            profile: None,
            scan_args: vec![
                "--from-json".to_string(),
                "--license-policy".to_string(),
                policy.display().to_string(),
            ],
        };

        let _guard = prepare_target(&mut context, &args).unwrap();

        assert_eq!(
            fs::read_to_string(context.auxiliary_dir.join("policy.yml")).unwrap(),
            "license_policies: []\n"
        );

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn metric_values_support_license_policy_and_license_clues() {
        let entry = json!({
            "license_policy": [
                {"icon": "ok", "license_key": "boost-1.0", "label": "Approved"}
            ],
            "license_clues": [
                {"license_expression": "boost-1.0", "start_line": 1, "end_line": 2}
            ]
        });

        let policy_values = metric_values(&entry, "license_policy");
        let clue_values = metric_values(&entry, "license_clues");

        assert_eq!(policy_values.len(), 1);
        assert!(policy_values[0].contains("boost-1.0"));
        assert_eq!(clue_values.len(), 1);
        assert!(clue_values[0].contains("license_expression"));
    }

    #[test]
    fn prepare_context_rejects_multiple_directory_targets() {
        let temp_root = unique_temp_dir("multi-directory-target-context");
        fs::create_dir_all(temp_root.join("a")).unwrap();
        fs::create_dir_all(temp_root.join("b")).unwrap();

        let args = Args {
            repo_url: None,
            target_path: vec![temp_root.join("a"), temp_root.join("b")],
            scancode_cache_identity: Some("dirs@rev".to_string()),
            repo_ref: None,
            profile: None,
            scan_args: Vec::new(),
        };

        let error = prepare_context(&args, vec!["--from-json".to_string()])
            .unwrap_err()
            .to_string();
        assert!(error.contains("multiple --target-path values currently support files only"));

        let _ = fs::remove_dir_all(&temp_root);
    }
}
