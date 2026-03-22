use anyhow::{Result, anyhow};
use chrono::Utc;
use clap::Parser;
use glob::Pattern;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::cache::{CACHE_DIR_ENV_VAR, CacheConfig};
use crate::cli::Cli;
use crate::license_detection::LicenseDetectionEngine;
use crate::models::{
    ExtraData, FileInfo, FileType, Header, LicenseClarityScore, OUTPUT_FORMAT_VERSION, Output,
    Package, Summary, SystemEnvironment, Tallies, TallyEntry,
};
use crate::output::{OutputWriteConfig, write_output_file};
use crate::progress::{ProgressMode, ScanProgress};
use crate::scanner::{TextDetectionOptions, count_with_size, process, process_with_options};
use crate::utils::spdx::combine_license_expressions;

mod assembly;
mod cache;
mod cli;
mod copyright;
mod finder;
mod license_detection;
mod models;
mod output;
mod parsers;
mod progress;
mod scanner;
mod utils;

#[cfg(test)]
mod test_utils;

fn main() -> std::io::Result<()> {
    if let Err(err) = run() {
        eprintln!("Error: {}", err);
        std::process::exit(1);
    }
    Ok(())
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    if cli.show_attribution {
        print!("{}", include_str!("../NOTICE"));
        return Ok(());
    }

    let start_time = Utc::now();
    let progress = Arc::new(ScanProgress::new(progress_mode_from_cli(&cli)));
    progress.set_processes(resolve_thread_count(cli.processes));
    progress.set_scan_names(configured_scan_names(&cli));
    progress.init_logging_bridge();

    validate_scan_option_compatibility(&cli)?;

    let exclude_patterns = compile_exclude_patterns(&cli.exclude);
    let include_patterns = compile_include_patterns(&cli.include);

    progress.start_discovery();

    let (
        mut scan_result,
        total_dirs,
        preloaded_assembly,
        preloaded_license_references,
        preloaded_license_rule_references,
    ) = if cli.from_json {
        let mut merged: Option<JsonScanInput> = None;
        for input_path in &cli.dir_path {
            let mut loaded = load_scan_from_json(input_path)?;
            if let Some(acc) = &mut merged {
                acc.files.append(&mut loaded.files);
                acc.packages.append(&mut loaded.packages);
                acc.dependencies.append(&mut loaded.dependencies);
                acc.license_references
                    .append(&mut loaded.license_references);
                acc.license_rule_references
                    .append(&mut loaded.license_rule_references);
                acc.excluded_count += loaded.excluded_count;
            } else {
                merged = Some(loaded);
            }
        }

        let loaded = merged.ok_or_else(|| anyhow!("No input paths provided"))?;
        let directories_count = loaded
            .files
            .iter()
            .filter(|f| f.file_type == crate::models::FileType::Directory)
            .count();
        let files_count = loaded
            .files
            .iter()
            .filter(|f| f.file_type == crate::models::FileType::File)
            .count();
        let size_count = loaded
            .files
            .iter()
            .filter(|f| f.file_type == crate::models::FileType::File)
            .map(|f| f.size)
            .sum();
        progress.finish_discovery(
            files_count,
            directories_count,
            size_count,
            loaded.excluded_count,
        );
        (
            scanner::ProcessResult {
                files: loaded.files,
                excluded_count: loaded.excluded_count,
            },
            directories_count,
            assembly::AssemblyResult {
                packages: loaded.packages,
                dependencies: loaded.dependencies,
            },
            loaded.license_references,
            loaded.license_rule_references,
        )
    } else {
        let scan_path = cli
            .dir_path
            .first()
            .ok_or_else(|| anyhow!("No directory input path provided"))?;

        let cache_config = prepare_cache_for_scan(scan_path, &cli)?;

        let (total_files, total_dirs, excluded_count, total_size) =
            count_with_size(scan_path, cli.max_depth, &exclude_patterns)?;
        progress.finish_discovery(total_files, total_dirs, total_size, excluded_count);
        if !cli.quiet {
            progress.output_written(&format!(
                "Found {} files in {} directories ({} items excluded)",
                total_files, total_dirs, excluded_count
            ));
        }

        progress.start_license_detection_engine_creation();
        let license_engine = init_license_engine(&cli.license_rules_path)?;
        progress.finish_license_detection_engine_creation();

        let text_options = TextDetectionOptions {
            detect_copyrights: cli.copyright,
            detect_emails: cli.email,
            detect_urls: cli.url,
            max_emails: cli.max_email,
            max_urls: cli.max_url,
            timeout_seconds: cli.timeout,
            scan_cache_dir: Some(cache_config.scan_results_dir()),
        };
        let default_text_options = TextDetectionOptions::default();

        let thread_count = resolve_thread_count(cli.processes);
        progress.start_scan(total_files);
        let mut result = if !text_options.detect_emails
            && !text_options.detect_urls
            && text_options.detect_copyrights == default_text_options.detect_copyrights
            && text_options.timeout_seconds == default_text_options.timeout_seconds
        {
            run_with_thread_pool(thread_count, || {
                process(
                    scan_path,
                    cli.max_depth,
                    Arc::clone(&progress),
                    &exclude_patterns,
                    Some(license_engine.clone()),
                    cli.include_text,
                )
            })?
        } else {
            run_with_thread_pool(thread_count, || {
                process_with_options(
                    scan_path,
                    cli.max_depth,
                    Arc::clone(&progress),
                    &exclude_patterns,
                    Some(license_engine.clone()),
                    cli.include_text,
                    &text_options,
                )
            })?
        };

        result.excluded_count = excluded_count;
        progress.finish_scan();

        (
            result,
            total_dirs,
            assembly::AssemblyResult {
                packages: Vec::new(),
                dependencies: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        )
    };

    if cli.filter_clues {
        filter_redundant_clues(&mut scan_result.files);
    }

    if !include_patterns.is_empty() {
        apply_include_filter(&mut scan_result.files, &include_patterns);
    }

    if cli.only_findings {
        apply_only_findings_filter(&mut scan_result.files);
    }

    if cli.strip_root || cli.full_root {
        let root_path = cli
            .dir_path
            .first()
            .ok_or_else(|| anyhow!("No input path available for path normalization"))?;
        normalize_paths(
            &mut scan_result.files,
            root_path,
            cli.strip_root,
            cli.full_root,
        );
    }

    if cli.mark_source {
        apply_mark_source(&mut scan_result.files);
    }

    let manifests_seen = scan_result
        .files
        .iter()
        .map(|file| file.package_data.len())
        .sum();
    let assembly_result = if cli.no_assemble {
        assembly::AssemblyResult {
            packages: Vec::new(),
            dependencies: Vec::new(),
        }
    } else if cli.from_json
        && (!preloaded_assembly.packages.is_empty() || !preloaded_assembly.dependencies.is_empty())
    {
        progress.start_assembly();
        progress.finish_assembly(preloaded_assembly.packages.len(), manifests_seen);
        preloaded_assembly
    } else {
        progress.start_assembly();
        let assembled = assembly::assemble(&mut scan_result.files);
        progress.finish_assembly(assembled.packages.len(), manifests_seen);
        assembled
    };

    let end_time = Utc::now();

    let output = create_output(
        start_time,
        end_time,
        scan_result,
        total_dirs,
        assembly_result,
        preloaded_license_references,
        preloaded_license_rule_references,
    );

    progress.start_output();
    for target in cli.output_targets() {
        let output_config = OutputWriteConfig {
            format: target.format,
            custom_template: target.custom_template.clone(),
            scanned_path: if cli.dir_path.len() == 1 {
                cli.dir_path.first().cloned()
            } else {
                None
            },
        };

        write_output_file(&target.file, &output, &output_config)?;
        progress.output_written(&format!(
            "{:?} output written to {}",
            target.format, target.file
        ));
    }
    progress.finish_output();

    progress.record_final_counts(&output.files);
    progress.display_summary(&start_time.to_rfc3339(), &Utc::now().to_rfc3339());

    Ok(())
}

fn validate_scan_option_compatibility(cli: &Cli) -> Result<()> {
    if cli.from_json && (cli.copyright || cli.email || cli.url) {
        return Err(anyhow!(
            "When using --from-json, file scan options like --copyright/--email/--url are not allowed"
        ));
    }

    if !cli.from_json && cli.dir_path.is_empty() {
        return Err(anyhow!("Directory path is required for scan operations"));
    }

    if !cli.from_json && cli.dir_path.len() != 1 {
        return Err(anyhow!(
            "Directory scan mode currently supports exactly one input path"
        ));
    }

    Ok(())
}

fn prepare_cache_for_scan(scan_path: &str, cli: &Cli) -> Result<CacheConfig> {
    let env_cache_dir = env::var_os(CACHE_DIR_ENV_VAR).map(PathBuf::from);
    let config = CacheConfig::from_overrides(
        Path::new(scan_path),
        cli.cache_dir.as_deref().map(Path::new),
        env_cache_dir.as_deref(),
    );

    if cli.cache_clear {
        config.clear()?;
    }

    config.ensure_dirs()?;
    Ok(config)
}

fn compile_exclude_patterns(patterns: &[String]) -> Vec<Pattern> {
    patterns
        .iter()
        .filter_map(|pattern| Pattern::new(pattern).ok())
        .collect()
}

fn compile_include_patterns(patterns: &[String]) -> Vec<Pattern> {
    patterns
        .iter()
        .filter_map(|pattern| Pattern::new(pattern).ok())
        .collect()
}

fn resolve_thread_count(processes: i32) -> usize {
    if processes > 0 {
        processes as usize
    } else if processes == 0 {
        default_parallel_threads()
    } else {
        1
    }
}

fn default_parallel_threads() -> usize {
    let cpus = std::thread::available_parallelism().map_or(1, |n| n.get());
    if cpus > 1 { cpus - 1 } else { 1 }
}

fn progress_mode_from_cli(cli: &Cli) -> ProgressMode {
    if cli.quiet {
        ProgressMode::Quiet
    } else if cli.verbose {
        ProgressMode::Verbose
    } else {
        ProgressMode::Default
    }
}

fn configured_scan_names(cli: &Cli) -> String {
    let mut names = vec!["licenses", "packages"];
    if cli.copyright {
        names.push("copyrights");
    }
    if cli.email {
        names.push("emails");
    }
    if cli.url {
        names.push("urls");
    }
    names.join(", ")
}

fn run_with_thread_pool<T, F>(threads: usize, f: F) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send,
    T: Send,
{
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads.max(1))
        .build()?;
    pool.install(f)
}

fn matches_patterns(path: &str, patterns: &[Pattern]) -> bool {
    if patterns.is_empty() {
        return true;
    }

    let file_name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    patterns
        .iter()
        .any(|pattern| pattern.matches(path) || pattern.matches(file_name))
}

fn apply_include_filter(files: &mut Vec<crate::models::FileInfo>, include_patterns: &[Pattern]) {
    let mut explicitly_included_files = HashSet::new();
    let mut explicitly_included_dirs = Vec::<String>::new();

    for entry in files.iter() {
        if matches_patterns(&entry.path, include_patterns) {
            match entry.file_type {
                crate::models::FileType::File => {
                    explicitly_included_files.insert(entry.path.clone());
                }
                crate::models::FileType::Directory => {
                    explicitly_included_dirs.push(entry.path.clone());
                }
            }
        }
    }

    let mut kept_file_paths = HashSet::new();
    for entry in files.iter() {
        if entry.file_type != crate::models::FileType::File {
            continue;
        }

        let explicitly = explicitly_included_files.contains(&entry.path);
        let under_included_dir = explicitly_included_dirs
            .iter()
            .any(|dir| Path::new(&entry.path).starts_with(Path::new(dir)));

        if explicitly || under_included_dir {
            kept_file_paths.insert(entry.path.clone());
        }
    }

    files.retain(|entry| match entry.file_type {
        crate::models::FileType::File => kept_file_paths.contains(&entry.path),
        crate::models::FileType::Directory => {
            explicitly_included_dirs.contains(&entry.path)
                || kept_file_paths
                    .iter()
                    .any(|path| Path::new(path).starts_with(Path::new(&entry.path)))
        }
    });
}

fn has_findings(file: &crate::models::FileInfo) -> bool {
    file.license_expression.is_some()
        || !file.license_detections.is_empty()
        || !file.copyrights.is_empty()
        || !file.holders.is_empty()
        || !file.authors.is_empty()
        || !file.emails.is_empty()
        || !file.urls.is_empty()
        || !file.package_data.is_empty()
        || !file.scan_errors.is_empty()
}

fn apply_only_findings_filter(files: &mut Vec<crate::models::FileInfo>) {
    let kept_file_paths: HashSet<String> = files
        .iter()
        .filter(|entry| entry.file_type == crate::models::FileType::File && has_findings(entry))
        .map(|entry| entry.path.clone())
        .collect();

    files.retain(|entry| match entry.file_type {
        crate::models::FileType::File => kept_file_paths.contains(&entry.path),
        crate::models::FileType::Directory => kept_file_paths
            .iter()
            .any(|path| Path::new(path).starts_with(Path::new(&entry.path))),
    });
}

fn dedupe_vec_by_key<T, K, F>(items: &mut Vec<T>, mut key_fn: F)
where
    K: std::hash::Hash + Eq,
    F: FnMut(&T) -> K,
{
    let mut seen = HashSet::new();
    items.retain(|item| seen.insert(key_fn(item)));
}

fn filter_redundant_clues(files: &mut [crate::models::FileInfo]) {
    for file in files.iter_mut() {
        dedupe_vec_by_key(&mut file.copyrights, |c| {
            (c.copyright.clone(), c.start_line, c.end_line)
        });
        dedupe_vec_by_key(&mut file.holders, |h| {
            (h.holder.clone(), h.start_line, h.end_line)
        });
        dedupe_vec_by_key(&mut file.authors, |a| {
            (a.author.clone(), a.start_line, a.end_line)
        });
        dedupe_vec_by_key(&mut file.emails, |e| {
            (e.email.clone(), e.start_line, e.end_line)
        });
        dedupe_vec_by_key(&mut file.urls, |u| {
            (u.url.clone(), u.start_line, u.end_line)
        });
    }
}

fn normalize_paths(
    files: &mut [crate::models::FileInfo],
    scan_root: &str,
    strip_root: bool,
    full_root: bool,
) {
    for entry in files.iter_mut() {
        let current_path = PathBuf::from(&entry.path);

        if full_root {
            let absolute_candidate = if current_path.is_absolute() {
                current_path.clone()
            } else {
                env::current_dir()
                    .map(|cwd| cwd.join(&current_path))
                    .unwrap_or(current_path.clone())
            };
            let absolute = absolute_candidate
                .canonicalize()
                .unwrap_or(absolute_candidate);
            entry.path = absolute.to_string_lossy().to_string();
            continue;
        }

        if strip_root && let Some(stripped) = strip_root_prefix(&current_path, Path::new(scan_root))
        {
            entry.path = stripped.to_string_lossy().to_string();
        }
    }
}

fn strip_root_prefix(path: &Path, root: &Path) -> Option<PathBuf> {
    if let Ok(stripped) = path.strip_prefix(root)
        && !stripped.as_os_str().is_empty()
    {
        return Some(stripped.to_path_buf());
    }

    let canonical_path = path.canonicalize().ok()?;
    let canonical_root = root.canonicalize().ok()?;
    let stripped = canonical_path.strip_prefix(canonical_root).ok()?;
    if stripped.as_os_str().is_empty() {
        None
    } else {
        Some(stripped.to_path_buf())
    }
}

#[derive(Deserialize)]
struct JsonScanInput {
    #[serde(default)]
    files: Vec<crate::models::FileInfo>,
    #[serde(default)]
    packages: Vec<crate::models::Package>,
    #[serde(default)]
    dependencies: Vec<crate::models::TopLevelDependency>,
    #[serde(default)]
    license_references: Vec<crate::models::LicenseReference>,
    #[serde(default)]
    license_rule_references: Vec<crate::models::LicenseRuleReference>,
    #[serde(default)]
    excluded_count: usize,
}

fn load_scan_from_json(path: &str) -> Result<JsonScanInput> {
    let input_path = Path::new(path);
    if !input_path.is_file() {
        return Err(anyhow!("--from-json input must be a valid file: {}", path));
    }

    let content = fs::read_to_string(input_path)?;
    let parsed: JsonScanInput = serde_json::from_str(&content)
        .map_err(|e| anyhow!("Input JSON scan file is not valid JSON: {path}: {e}"))?;

    Ok(parsed)
}

fn apply_mark_source(files: &mut [crate::models::FileInfo]) {
    let mut index_by_path = HashMap::<String, usize>::new();
    for (idx, entry) in files.iter().enumerate() {
        index_by_path.insert(entry.path.clone(), idx);
    }

    for entry in files.iter_mut() {
        if entry.file_type == crate::models::FileType::File {
            if entry.is_source != Some(false) {
                entry.is_source = entry.programming_language.as_ref().map(|_| true);
            }
            entry.source_count = None;
        }
    }

    let mut dir_paths = files
        .iter()
        .filter(|entry| entry.file_type == crate::models::FileType::Directory)
        .map(|entry| entry.path.clone())
        .collect::<Vec<_>>();
    dir_paths.sort_by_key(|path| usize::MAX - Path::new(path).components().count());

    let mut direct_file_count = HashMap::<String, usize>::new();
    let mut direct_source_file_count = HashMap::<String, usize>::new();
    let mut child_dirs = HashMap::<String, Vec<String>>::new();

    for entry in files.iter() {
        if let Some(parent) = Path::new(&entry.path).parent().and_then(|p| p.to_str()) {
            let parent_key = parent.to_string();
            if entry.file_type == crate::models::FileType::File {
                if entry.is_source != Some(false) {
                    *direct_file_count.entry(parent_key.clone()).or_insert(0) += 1;
                    if entry.is_source.unwrap_or(false) {
                        *direct_source_file_count.entry(parent_key).or_insert(0) += 1;
                    }
                }
            } else {
                child_dirs
                    .entry(parent_key)
                    .or_default()
                    .push(entry.path.clone());
            }
        }
    }

    let mut descendant_file_count = HashMap::<String, usize>::new();
    let mut descendant_source_count = HashMap::<String, usize>::new();

    for dir_path in dir_paths {
        let mut total_files = *direct_file_count.get(&dir_path).unwrap_or(&0);
        let mut source_files = *direct_source_file_count.get(&dir_path).unwrap_or(&0);

        if let Some(children) = child_dirs.get(&dir_path) {
            for child in children {
                total_files += descendant_file_count.get(child).copied().unwrap_or(0);
                source_files += descendant_source_count.get(child).copied().unwrap_or(0);
            }
        }

        let qualifies = total_files > 0 && (source_files as f64 / total_files as f64) >= 0.9;

        if let Some(idx) = index_by_path.get(&dir_path)
            && let Some(entry) = files.get_mut(*idx)
        {
            if qualifies && source_files > 0 {
                entry.is_source = Some(true);
                entry.source_count = Some(source_files);
            } else {
                entry.is_source = None;
                entry.source_count = None;
            }
        }

        descendant_file_count.insert(dir_path.clone(), total_files);
        descendant_source_count.insert(dir_path, if qualifies { source_files } else { 0 });
    }
}

fn init_license_engine(rules_path: &Option<String>) -> Result<Arc<LicenseDetectionEngine>> {
    match rules_path {
        Some(p) => {
            let path = PathBuf::from(p);
            if !path.exists() {
                return Err(anyhow!("License rules path does not exist: {:?}", path));
            }
            let engine = LicenseDetectionEngine::from_directory(&path)?;
            println!(
                "License detection engine initialized with {} rules from {:?}",
                engine.index().rules_by_rid.len(),
                path
            );
            Ok(Arc::new(engine))
        }
        None => {
            let engine = LicenseDetectionEngine::from_embedded()?;
            println!(
                "License detection engine initialized with {} rules from embedded artifact",
                engine.index().rules_by_rid.len()
            );
            Ok(Arc::new(engine))
        }
    }
}

fn create_output(
    start_time: chrono::DateTime<Utc>,
    end_time: chrono::DateTime<Utc>,
    scan_result: scanner::ProcessResult,
    total_dirs: usize,
    assembly_result: assembly::AssemblyResult,
    license_references: Vec<crate::models::LicenseReference>,
    license_rule_references: Vec<crate::models::LicenseRuleReference>,
) -> Output {
    let duration = (end_time - start_time).num_nanoseconds().unwrap_or(0) as f64 / 1_000_000_000.0;

    let extra_data = ExtraData {
        files_count: scan_result.files.len(),
        directories_count: total_dirs,
        excluded_count: scan_result.excluded_count,
        system_environment: SystemEnvironment {
            operating_system: sys_info::os_type().ok(),
            cpu_architecture: env::consts::ARCH.to_string(),
            platform: format!(
                "{}-{}-{}",
                sys_info::os_type().unwrap_or_else(|_| "unknown".to_string()),
                sys_info::os_release().unwrap_or_else(|_| "unknown".to_string()),
                env::consts::ARCH
            ),
            rust_version: rustc_version_runtime::version().to_string(),
        },
    };

    // Collect all scan errors from individual files
    let errors: Vec<String> = scan_result
        .files
        .iter()
        .filter_map(|file| {
            if file.scan_errors.is_empty() {
                None
            } else {
                Some(
                    file.scan_errors
                        .iter()
                        .map(|error| format!("{}: {}", file.path, error))
                        .collect::<Vec<String>>(),
                )
            }
        })
        .flatten()
        .collect();

    let mut files = scan_result.files;
    let mut packages = assembly_result.packages;
    classify_key_files(&mut files, &packages);
    promote_package_metadata_from_key_files(&files, &mut packages);
    compute_detailed_tallies(&mut files);
    let summary = compute_summary(&files, &packages);
    let tallies = compute_tallies(&files);
    let tallies_of_key_files = compute_key_file_tallies(&files);

    Output {
        summary,
        tallies,
        tallies_of_key_files,
        headers: vec![Header {
            start_timestamp: start_time.to_rfc3339(),
            end_timestamp: end_time.to_rfc3339(),
            duration,
            extra_data,
            errors,
            output_format_version: OUTPUT_FORMAT_VERSION.to_string(),
        }],
        packages,
        dependencies: assembly_result.dependencies,
        files,
        license_references,
        license_rule_references,
    }
}

fn classify_key_files(files: &mut [FileInfo], packages: &[Package]) {
    let package_roots = build_package_roots(packages);
    let package_file_references = build_package_file_reference_map(files);

    for file in files.iter_mut() {
        if file.file_type != FileType::File || file.for_packages.is_empty() {
            continue;
        }

        let basename = file
            .path
            .rsplit('/')
            .next()
            .unwrap_or(file.path.as_str())
            .to_ascii_lowercase();

        file.is_legal = is_legal_filename(&basename);
        file.is_readme = basename.starts_with("readme");
        file.is_manifest = !file.package_data.is_empty() || is_manifest_filename(&basename);

        let path = Path::new(&file.path);
        let is_referenced = file.for_packages.iter().any(|uid| {
            package_file_references
                .get(uid)
                .is_some_and(|refs| refs.contains(file.path.as_str()))
        });
        let is_root_top_level = file.for_packages.iter().any(|uid| {
            package_roots
                .get(uid)
                .and_then(|root| path.strip_prefix(root).ok())
                .is_some_and(|relative| relative.components().count() == 1)
        });

        file.is_top_level = is_referenced || is_root_top_level;
        file.is_key_file =
            file.is_top_level && (file.is_legal || file.is_manifest || file.is_readme);
    }
}

fn build_package_roots(packages: &[Package]) -> HashMap<String, PathBuf> {
    let mut roots = HashMap::new();
    for package in packages {
        if let Some(root) = package_root(package) {
            roots.insert(package.package_uid.clone(), root);
        }
    }
    roots
}

fn package_root(package: &Package) -> Option<PathBuf> {
    for datafile_path in &package.datafile_paths {
        let path = Path::new(datafile_path);

        if path.file_name().and_then(|n| n.to_str()) == Some("metadata.gz-extract") {
            return path.parent().map(|p| p.to_path_buf());
        }

        if path
            .components()
            .any(|c| c.as_os_str() == "data.gz-extract")
        {
            let mut current = path;
            while let Some(parent) = current.parent() {
                if parent.file_name().and_then(|n| n.to_str()) == Some("data.gz-extract") {
                    return parent.parent().map(|p| p.to_path_buf());
                }
                current = parent;
            }
        }

        if let Some(parent) = path.parent() {
            return Some(parent.to_path_buf());
        }
    }
    None
}

fn build_package_file_reference_map(files: &[FileInfo]) -> HashMap<String, HashSet<String>> {
    let mut mapping: HashMap<String, HashSet<String>> = HashMap::new();

    for file in files {
        if file.package_data.is_empty() || file.for_packages.is_empty() {
            continue;
        }

        for package_uid in &file.for_packages {
            let refs = mapping.entry(package_uid.clone()).or_default();
            for pkg_data in &file.package_data {
                for file_ref in &pkg_data.file_references {
                    refs.insert(file_ref.path.clone());
                }
            }
        }
    }

    mapping
}

fn is_legal_filename(basename: &str) -> bool {
    basename == "license"
        || basename.starts_with("license.")
        || basename == "licence"
        || basename.starts_with("licence.")
        || basename == "copyright"
        || basename.starts_with("copyright.")
        || basename == "copying"
        || basename.starts_with("copying.")
        || basename == "notice"
        || basename.starts_with("notice.")
}

fn is_manifest_filename(basename: &str) -> bool {
    basename.ends_with(".gemspec") || basename == "gemfile" || basename == "gemfile.lock"
}

fn promote_package_metadata_from_key_files(files: &[FileInfo], packages: &mut [Package]) {
    for package in packages.iter_mut() {
        let key_files: Vec<&FileInfo> = files
            .iter()
            .filter(|file| file.is_key_file && file.for_packages.contains(&package.package_uid))
            .collect();

        if key_files.is_empty() {
            continue;
        }

        if package.copyright.is_none() {
            package.copyright = key_files
                .iter()
                .flat_map(|file| file.copyrights.iter())
                .map(|copyright| copyright.copyright.clone())
                .next();
        }

        if package.holder.is_none() {
            package.holder = key_files
                .iter()
                .flat_map(|file| file.holders.iter())
                .map(|holder| holder.holder.clone())
                .next();
        }
    }
}

fn compute_summary(files: &[FileInfo], packages: &[Package]) -> Option<Summary> {
    let key_files: Vec<&FileInfo> = files.iter().filter(|file| file.is_key_file).collect();
    let declared_holder = compute_declared_holder(files, packages);
    let primary_language = compute_primary_language(files, packages);
    let other_languages = compute_other_languages(files, packages, primary_language.as_deref());

    if key_files.is_empty()
        && declared_holder.is_none()
        && primary_language.is_none()
        && other_languages.is_empty()
    {
        return None;
    }

    let declared_expressions: Vec<String> = key_files
        .iter()
        .filter_map(|file| file_declared_license_expression(file))
        .collect();
    let declared_license_expression =
        combine_license_expressions(declared_expressions.iter().cloned())
            .map(|expr| expr.to_ascii_lowercase());

    let declared_license = key_files.iter().any(|file| {
        file_declared_license_expression(file).is_some() || !file.license_detections.is_empty()
    });
    let identification_precision = declared_license;
    let has_license_text = key_files
        .iter()
        .any(|file| file.is_legal && !file.license_detections.is_empty());
    let declared_copyrights = key_files.iter().any(|file| !file.copyrights.is_empty());
    let ambiguous_compound_licensing =
        declared_expressions.iter().collect::<HashSet<_>>().len() > 1;

    let mut score: usize = 0;
    if declared_license {
        score += 40;
    }
    if identification_precision {
        score += 40;
    }
    if has_license_text {
        score += 10;
    }
    if declared_copyrights {
        score += 10;
    }
    if ambiguous_compound_licensing {
        score = score.saturating_sub(10);
    }

    let license_clarity_score = if declared_license || has_license_text || declared_copyrights {
        Some(LicenseClarityScore {
            score,
            declared_license,
            identification_precision,
            has_license_text,
            declared_copyrights,
            conflicting_license_categories: false,
            ambiguous_compound_licensing,
        })
    } else {
        None
    };

    Some(Summary {
        declared_license_expression,
        license_clarity_score,
        declared_holder,
        primary_language,
        other_languages,
    })
}

fn compute_tallies(files: &[FileInfo]) -> Option<Tallies> {
    let detected_license_expression = tally_file_values(files, detected_license_values, true);
    let copyrights = tally_file_values(files, copyright_values, true);
    let holders = tally_file_values(files, holder_values, true);
    let authors = tally_file_values(files, author_values, true);
    let programming_language = tally_file_values(files, programming_language_values, false);

    let tallies = Tallies {
        detected_license_expression,
        copyrights,
        holders,
        authors,
        programming_language,
    };

    (!tallies.is_empty()).then_some(tallies)
}

fn compute_key_file_tallies(files: &[FileInfo]) -> Option<Tallies> {
    if !files
        .iter()
        .any(|file| file.file_type == FileType::File && file.is_key_file)
    {
        return None;
    }

    let tallies = Tallies {
        detected_license_expression: tally_file_values_filtered(
            files,
            |file| file.is_key_file,
            detected_license_values,
            false,
        ),
        copyrights: tally_file_values_filtered(
            files,
            |file| file.is_key_file,
            copyright_values,
            false,
        ),
        holders: tally_file_values_filtered(files, |file| file.is_key_file, holder_values, false),
        authors: tally_file_values_filtered(files, |file| file.is_key_file, author_values, false),
        programming_language: tally_file_values_filtered(
            files,
            |file| file.is_key_file,
            programming_language_values,
            false,
        ),
    };

    (!tallies.is_empty()).then_some(tallies)
}

fn compute_detailed_tallies(files: &mut [FileInfo]) {
    let mut children_by_parent: HashMap<String, Vec<usize>> = HashMap::new();
    let known_paths: HashSet<String> = files.iter().map(|file| file.path.clone()).collect();

    for (idx, file) in files.iter().enumerate() {
        let Some(parent) = parent_path(&file.path) else {
            continue;
        };
        if known_paths.contains(parent.as_str()) {
            children_by_parent.entry(parent).or_default().push(idx);
        }
    }

    let mut indices: Vec<usize> = (0..files.len()).collect();
    indices.sort_by_key(|&idx| std::cmp::Reverse(path_depth(&files[idx].path)));

    for idx in indices {
        let tallies = if files[idx].file_type == FileType::File {
            compute_direct_file_tallies(&files[idx])
        } else {
            aggregate_child_tallies(
                children_by_parent
                    .get(files[idx].path.as_str())
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
                files,
            )
        };
        files[idx].tallies = Some(tallies);
    }
}

fn parent_path(path: &str) -> Option<String> {
    Path::new(path)
        .parent()
        .and_then(|parent| parent.to_str())
        .filter(|parent| !parent.is_empty())
        .map(str::to_string)
}

fn path_depth(path: &str) -> usize {
    Path::new(path).components().count()
}

fn compute_direct_file_tallies(file: &FileInfo) -> Tallies {
    Tallies {
        detected_license_expression: build_direct_tally_entries(
            detected_license_values(file),
            true,
        ),
        copyrights: build_direct_tally_entries(copyright_values(file), true),
        holders: build_direct_tally_entries(holder_values(file), true),
        authors: build_direct_tally_entries(author_values(file), true),
        programming_language: build_direct_tally_entries(programming_language_values(file), false),
    }
}

fn aggregate_child_tallies(child_indices: &[usize], files: &[FileInfo]) -> Tallies {
    let mut detected_license_expression = HashMap::new();
    let mut copyrights = HashMap::new();
    let mut holders = HashMap::new();
    let mut authors = HashMap::new();
    let mut programming_language = HashMap::new();

    for &child_idx in child_indices {
        let Some(child_tallies) = files[child_idx].tallies.as_ref() else {
            continue;
        };

        merge_tally_entries(
            &mut detected_license_expression,
            &child_tallies.detected_license_expression,
        );
        merge_tally_entries(&mut copyrights, &child_tallies.copyrights);
        merge_tally_entries(&mut holders, &child_tallies.holders);
        merge_tally_entries(&mut authors, &child_tallies.authors);
        merge_tally_entries(
            &mut programming_language,
            &child_tallies.programming_language,
        );
    }

    Tallies {
        detected_license_expression: build_tally_entries(detected_license_expression),
        copyrights: build_tally_entries(copyrights),
        holders: build_tally_entries(holders),
        authors: build_tally_entries(authors),
        programming_language: build_tally_entries(programming_language),
    }
}

fn build_direct_tally_entries(values: Vec<String>, count_missing: bool) -> Vec<TallyEntry> {
    let mut counts: HashMap<Option<String>, usize> = HashMap::new();

    if values.is_empty() {
        if count_missing {
            counts.insert(None, 1);
        }
    } else {
        for value in values {
            *counts.entry(Some(value)).or_insert(0) += 1;
        }
    }

    build_tally_entries(counts)
}

fn merge_tally_entries(counts: &mut HashMap<Option<String>, usize>, entries: &[TallyEntry]) {
    for entry in entries {
        *counts.entry(entry.value.clone()).or_insert(0) += entry.count;
    }
}

fn tally_file_values<F>(
    files: &[FileInfo],
    values_for_file: F,
    count_missing_files: bool,
) -> Vec<TallyEntry>
where
    F: Fn(&FileInfo) -> Vec<String>,
{
    tally_file_values_filtered(files, |_| true, values_for_file, count_missing_files)
}

fn tally_file_values_filtered<P, F>(
    files: &[FileInfo],
    predicate: P,
    values_for_file: F,
    count_missing_files: bool,
) -> Vec<TallyEntry>
where
    P: Fn(&FileInfo) -> bool,
    F: Fn(&FileInfo) -> Vec<String>,
{
    let mut counts: HashMap<Option<String>, usize> = HashMap::new();

    for file in files
        .iter()
        .filter(|file| file.file_type == FileType::File && predicate(file))
    {
        let values = values_for_file(file);
        if values.is_empty() {
            if count_missing_files {
                *counts.entry(None).or_insert(0) += 1;
            }
            continue;
        }

        for value in values {
            *counts.entry(Some(value)).or_insert(0) += 1;
        }
    }

    build_tally_entries(counts)
}

fn detected_license_values(file: &FileInfo) -> Vec<String> {
    file.license_expression.clone().into_iter().collect()
}

fn copyright_values(file: &FileInfo) -> Vec<String> {
    file.copyrights
        .iter()
        .map(|copyright| copyright.copyright.clone())
        .collect()
}

fn holder_values(file: &FileInfo) -> Vec<String> {
    file.holders
        .iter()
        .map(|holder| holder.holder.clone())
        .collect()
}

fn author_values(file: &FileInfo) -> Vec<String> {
    file.authors
        .iter()
        .map(|author| author.author.clone())
        .collect()
}

fn programming_language_values(file: &FileInfo) -> Vec<String> {
    file.programming_language.clone().into_iter().collect()
}

fn build_tally_entries(counts: HashMap<Option<String>, usize>) -> Vec<TallyEntry> {
    let mut tallies: Vec<TallyEntry> = counts
        .into_iter()
        .map(|(value, count)| TallyEntry { value, count })
        .collect();

    tallies.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.value.cmp(&right.value))
    });
    tallies
}

fn compute_declared_holder(files: &[FileInfo], packages: &[Package]) -> Option<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();

    for holder in packages
        .iter()
        .filter_map(|package| package.holder.as_ref())
    {
        *counts.entry(holder.clone()).or_insert(0) += 1;
    }

    if counts.is_empty() {
        for holder in files
            .iter()
            .filter(|file| file.is_key_file)
            .flat_map(|file| file.holders.iter())
            .map(|holder| holder.holder.clone())
        {
            *counts.entry(holder).or_insert(0) += 1;
        }
    }

    counts
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(&left.0)))
        .map(|(holder, _)| holder)
}

fn compute_primary_language(files: &[FileInfo], packages: &[Package]) -> Option<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();

    for language in packages
        .iter()
        .filter_map(|package| package.primary_language.as_ref())
    {
        *counts.entry(language.clone()).or_insert(0) += 1;
    }

    if counts.is_empty() {
        for language in files
            .iter()
            .filter(|file| file.is_source.unwrap_or(false))
            .filter_map(|file| file.programming_language.as_ref())
        {
            *counts.entry(language.clone()).or_insert(0) += 1;
        }
    }

    counts
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(&left.0)))
        .map(|(language, _)| language)
}

fn compute_other_languages(
    files: &[FileInfo],
    packages: &[Package],
    primary_language: Option<&str>,
) -> Vec<TallyEntry> {
    let mut counts: HashMap<String, usize> = HashMap::new();

    for language in packages
        .iter()
        .filter_map(|package| package.primary_language.as_ref())
    {
        *counts.entry(language.clone()).or_insert(0) += 1;
    }

    if counts.is_empty() {
        for language in files
            .iter()
            .filter(|file| file.is_source.unwrap_or(false))
            .filter_map(|file| file.programming_language.as_ref())
        {
            *counts.entry(language.clone()).or_insert(0) += 1;
        }
    }

    let mut tallies: Vec<TallyEntry> = counts
        .into_iter()
        .filter(|(language, _)| Some(language.as_str()) != primary_language)
        .map(|(language, count)| TallyEntry {
            value: Some(language),
            count,
        })
        .collect();

    tallies.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.value.cmp(&right.value))
    });
    tallies
}

fn file_declared_license_expression(file: &FileInfo) -> Option<String> {
    file.license_expression.clone().or_else(|| {
        file.package_data.iter().find_map(|pkg| {
            pkg.declared_license_expression_spdx
                .clone()
                .or_else(|| pkg.declared_license_expression.clone())
                .or_else(|| pkg.get_license_expression())
        })
    })
}

#[cfg(test)]
mod main_test;
