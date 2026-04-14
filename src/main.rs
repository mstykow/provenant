use anyhow::{Result, anyhow};
use chrono::Utc;
use clap::Parser;
use regex::Regex;
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::cache::{
    CACHE_DIR_ENV_VAR, CacheConfig, IncrementalManifest, IncrementalManifestEntry,
    build_collection_exclude_patterns, incremental_manifest_path, load_incremental_manifest,
    manifest_entry_matches_path, metadata_fingerprint, write_incremental_manifest,
};
use crate::cli::{Cli, ProcessMode};
use crate::license_detection::LicenseDetectionEngine;
use crate::license_detection::license_cache::LicenseCacheConfig;
use crate::models::{FileInfo, FileType, Sha256Digest};
use crate::output::{OutputWriteConfig, write_output_file};
use crate::post_processing::{
    CreateOutputContext, CreateOutputOptions, DEFAULT_LICENSEDB_URL_TEMPLATE,
    apply_license_policy_from_file, apply_package_reference_following, build_facet_rules,
    collect_top_level_license_detections, collect_top_level_license_references, create_output,
};
use crate::progress::{ProgressMode, ScanProgress, format_default_scan_error};
use crate::scan_result_shaping::{
    apply_cli_path_selection_filter, apply_ignore_resource_filter, apply_mark_source,
    apply_only_findings_filter, apply_user_path_filters_to_collected, filter_redundant_clues,
    filter_redundant_clues_with_rules, load_and_merge_json_inputs, normalize_paths,
    normalize_top_level_output_paths, prepare_filter_clue_rule_lookup, resolve_native_scan_inputs,
    trim_preloaded_assembly_to_files,
};
use crate::scanner::{
    LicenseScanOptions, TextDetectionOptions, collect_paths, process_collected_with_memory_limit,
    process_collected_with_memory_limit_sequential, scan_options_fingerprint,
};
use crate::time::format_scancode_timestamp;
use crate::utils::hash::calculate_sha256;

mod assembly;
mod cache;
mod cli;
mod copyright;
mod finder;
mod license_detection;
mod models;
mod output;
mod output_schema;
mod parsers;
mod post_processing;
mod progress;
mod scan_result_shaping;
mod scanner;
mod time;
mod utils;
mod version;

fn main() -> std::io::Result<()> {
    if let Err(err) = run() {
        eprintln!("Error: {}", err);
        std::process::exit(1);
    }
    Ok(())
}

fn run() -> Result<()> {
    #[cfg(feature = "golden-tests")]
    touch_license_golden_symbols();

    let cli = Cli::parse();

    if cli.show_attribution {
        print!("{}", include_str!("../NOTICE"));
        return Ok(());
    }

    let start_time = Utc::now();
    let progress = Arc::new(ScanProgress::new(progress_mode_from_cli(&cli)));
    progress.set_processes(cli.processes);
    progress.set_scan_names(configured_scan_names(&cli));
    progress.init_logging_bridge();

    progress.start_setup();
    validate_scan_option_compatibility(&cli)?;
    let facet_rules = build_facet_rules(&cli.facet)?;

    let ignore_author_patterns = compile_regex_patterns("--ignore-author", &cli.ignore_author)?;
    let ignore_copyright_holder_patterns =
        compile_regex_patterns("--ignore-copyright-holder", &cli.ignore_copyright_holder)?;
    progress.finish_setup();

    progress.start_discovery();

    let (
        mut scan_result,
        total_dirs,
        mut preloaded_assembly,
        preloaded_license_detections,
        preloaded_license_references,
        preloaded_license_rule_references,
        preloaded_extra_errors,
        mut active_license_engine,
    ) = if cli.from_json {
        let loaded = load_and_merge_json_inputs(&cli.dir_path, cli.strip_root, cli.full_root)?;
        let directories_count = loaded.directory_count();
        let files_count = loaded.file_count();
        let size_count = loaded.file_size_count();
        progress.finish_discovery(
            files_count,
            directories_count,
            size_count,
            loaded.excluded_count,
        );
        let (
            process_result,
            assembly_result,
            license_detections,
            license_references,
            license_rule_references,
            extra_errors,
        ) = loaded.into_parts()?;
        (
            process_result,
            directories_count,
            assembly_result,
            license_detections,
            license_references,
            license_rule_references,
            extra_errors,
            None,
        )
    } else {
        let (scan_path, native_input_includes) = resolve_native_scan_inputs(&cli.dir_path)?;
        let mut native_include_patterns = cli.include.clone();
        native_include_patterns.extend(native_input_includes);

        let cache_config = prepare_cache_for_scan(&scan_path, &cli)?;
        let collection_exclude_patterns =
            build_collection_exclude_patterns(Path::new(&scan_path), cache_config.root_dir());

        let mut collected = collect_paths(&scan_path, cli.max_depth, &collection_exclude_patterns);
        let user_excluded_count = apply_user_path_filters_to_collected(
            &mut collected,
            Path::new(&scan_path),
            &native_include_patterns,
            &cli.exclude,
        );
        let total_files = collected.file_count();
        let total_dirs = collected.directory_count();
        let total_size = collected.total_file_bytes;
        let excluded_count = collected.excluded_count + user_excluded_count;
        let all_collected_files = collected.files.clone();
        let ordered_file_paths: Vec<PathBuf> = collected
            .files
            .iter()
            .map(|(path, _)| path.clone())
            .collect();
        let runtime_errors = collected
            .collection_errors
            .iter()
            .map(|(path, err)| format_default_scan_error(path, err))
            .collect();
        for (path, err) in &collected.collection_errors {
            progress.record_runtime_error(path, err);
        }
        progress.finish_discovery(total_files, total_dirs, total_size, excluded_count);
        if !cli.quiet {
            progress.output_written(&format!(
                "Found {} files in {} directories ({} items excluded)",
                total_files, total_dirs, excluded_count
            ));
        }

        let license_engine = if cli.license {
            progress.start_setup();
            progress.start_license_detection_engine_creation();
            let engine = init_license_engine(&cli)?;
            progress.finish_license_detection_engine_creation("setup_scan:licenses");
            progress.finish_setup();
            progress.output_written(&describe_license_engine_source(
                &engine,
                cli.license_rules_path.as_deref(),
            ));
            Some(engine)
        } else {
            None
        };

        let enable_application_packages = cli.package || cli.package_only;
        let enable_system_packages = cli.system_package || cli.package_only;
        let enable_packages =
            enable_application_packages || enable_system_packages || cli.package_in_compiled;
        let (detect_copyrights, detect_emails, detect_urls, detect_generated) = if cli.package_only
        {
            (false, cli.email, cli.url, cli.generated)
        } else {
            (cli.copyright, cli.email, cli.url, cli.generated)
        };
        let process_mode = cli.processes;

        let text_options = TextDetectionOptions {
            collect_info: cli.info,
            detect_packages: enable_packages,
            detect_application_packages: enable_application_packages,
            detect_system_packages: enable_system_packages,
            detect_packages_in_compiled: cli.package_in_compiled,
            detect_copyrights,
            detect_generated,
            detect_emails,
            detect_urls,
            max_emails: cli.max_email,
            max_urls: cli.max_url,
            timeout_seconds: effective_timeout_seconds(process_mode, cli.timeout),
        };

        let license_options = LicenseScanOptions {
            include_text: cli.license_text,
            include_text_diagnostics: cli.license_text_diagnostics,
            include_diagnostics: cli.license_diagnostics,
            unknown_licenses: cli.unknown_licenses,
            min_score: cli.license_score,
        };
        let options_fingerprint =
            scan_options_fingerprint(&text_options, license_options, license_engine.as_deref());

        if cli.incremental {
            let manifest_path = incremental_manifest_path(
                cache_config.root_dir(),
                &incremental_manifest_key(Path::new(&scan_path), &options_fingerprint),
            );
            let previous_manifest =
                load_incremental_manifest(&manifest_path, &options_fingerprint)?;
            let reused_files = partition_incremental_files(
                &mut collected.files,
                Path::new(&scan_path),
                previous_manifest.as_ref(),
            );
            progress.record_incremental_reused(reused_files.len());
        }

        if let Some(message) = process_mode_message(process_mode) {
            progress.output_written(message);
        }
        progress.start_scan(collected.file_count());
        let mut result = match process_mode {
            ProcessMode::Parallel(thread_count) => run_with_thread_pool(thread_count, || {
                Ok(process_collected_with_memory_limit(
                    &collected,
                    Arc::clone(&progress),
                    license_engine.clone(),
                    license_options,
                    &text_options,
                    cli.max_in_memory,
                ))
            })?,
            ProcessMode::SequentialWithTimeouts | ProcessMode::SequentialWithoutTimeouts => {
                process_collected_with_memory_limit_sequential(
                    &collected,
                    Arc::clone(&progress),
                    license_engine.clone(),
                    license_options,
                    &text_options,
                    cli.max_in_memory,
                )
            }
        };

        if cli.incremental {
            let manifest_path = incremental_manifest_path(
                cache_config.root_dir(),
                &incremental_manifest_key(Path::new(&scan_path), &options_fingerprint),
            );
            let reused_files = partition_incremental_files(
                &mut all_collected_files.clone(),
                Path::new(&scan_path),
                load_incremental_manifest(&manifest_path, &options_fingerprint)?.as_ref(),
            );
            result.files =
                merge_incremental_file_results(result.files, reused_files, &ordered_file_paths);

            let manifest = build_incremental_manifest(
                Path::new(&scan_path),
                &all_collected_files,
                &result.files,
                &options_fingerprint,
            );
            write_incremental_manifest(cache_config.root_dir(), &manifest_path, &manifest)?;
        }

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
            Vec::new(),
            runtime_errors,
            license_engine,
        )
    };

    progress.start_post_scan();

    if cli.filter_clues {
        let clue_rule_lookup = record_detail_timing(&progress, "post-scan:filter-clues", || {
            prepare_filter_clue_rule_lookup(
                &scan_result.files,
                active_license_engine.as_deref(),
                cli.license_rules_path.as_deref(),
            )
        })?;
        if let Some(clue_rule_lookup) = clue_rule_lookup.as_ref() {
            filter_redundant_clues_with_rules(&mut scan_result.files, Some(clue_rule_lookup));
        } else {
            filter_redundant_clues(&mut scan_result.files);
        }
    }

    if !ignore_author_patterns.is_empty() || !ignore_copyright_holder_patterns.is_empty() {
        record_detail_timing(&progress, "post-scan:ignore-resource", || {
            apply_ignore_resource_filter(
                &mut scan_result.files,
                &ignore_copyright_holder_patterns,
                &ignore_author_patterns,
            );
        });
    }

    if cli.from_json && (!cli.include.is_empty() || !cli.exclude.is_empty()) {
        record_detail_timing(&progress, "output-filter:path-selection", || {
            apply_cli_path_selection_filter(&mut scan_result.files, &cli.include, &cli.exclude);
        });
    }

    if cli.only_findings {
        record_detail_timing(&progress, "output-filter:only-findings", || {
            apply_only_findings_filter(&mut scan_result.files);
        });
    }

    if cli.info && cli.mark_source {
        record_detail_timing(&progress, "post-scan:mark-source", || {
            apply_mark_source(&mut scan_result.files);
        });
    }

    if should_include_info_surface(&scan_result.files, &cli) {
        record_detail_timing(&progress, "post-scan:info-resource-counts", || {
            populate_info_resource_counts(&mut scan_result.files);
        });
    }

    record_detail_timing(&progress, "post-scan:license-provenance", || {
        for file in &mut scan_result.files {
            file.backfill_license_provenance();
        }
    });

    if cli.from_json {
        for err in &preloaded_extra_errors {
            progress.record_additional_error(err);
        }
    }

    let mut extra_errors = preloaded_extra_errors;
    if let Some(policy_path) = cli.license_policy.as_deref() {
        let license_policy_errors =
            record_detail_timing(&progress, "post-scan:license-policy", || {
                apply_license_policy_from_file(&mut scan_result.files, Path::new(policy_path))
            })?;
        for err in &license_policy_errors {
            progress.record_additional_error(err);
        }
        extra_errors.extend(license_policy_errors);
    }

    if cli.from_json {
        record_detail_timing(&progress, "post-scan:trim-preloaded-assembly", || {
            trim_preloaded_assembly_to_files(
                &scan_result.files,
                &mut preloaded_assembly.packages,
                &mut preloaded_assembly.dependencies,
            );
        });
    }

    progress.finish_post_scan();

    let manifests_seen = scan_result
        .files
        .iter()
        .map(|file| file.package_data.len())
        .sum();
    let skip_assembly = cli.no_assemble || cli.package_only;

    let mut assembly_result = if cli.from_json
        && (!preloaded_assembly.packages.is_empty() || !preloaded_assembly.dependencies.is_empty())
    {
        progress.start_assembly();
        progress.finish_assembly(preloaded_assembly.packages.len(), manifests_seen);
        preloaded_assembly
    } else if skip_assembly {
        assembly::AssemblyResult {
            packages: Vec::new(),
            dependencies: Vec::new(),
        }
    } else {
        progress.start_assembly();
        let assembled = assembly::assemble(&mut scan_result.files);
        progress.finish_assembly(assembled.packages.len(), manifests_seen);
        assembled
    };

    if !cli.from_json && (cli.strip_root || cli.full_root) {
        let root_path = cli
            .dir_path
            .first()
            .ok_or_else(|| anyhow!("No input path available for path normalization"))?;
        progress.start_post_scan();
        record_detail_timing(&progress, "post-scan:path-normalization", || {
            normalize_paths(
                &mut scan_result.files,
                root_path,
                cli.strip_root,
                cli.full_root,
            );
            normalize_top_level_output_paths(
                &mut assembly_result.packages,
                &mut assembly_result.dependencies,
                root_path,
                cli.strip_root,
            );
        });
        progress.finish_post_scan();
    }

    progress.start_post_scan();
    record_detail_timing(&progress, "post-scan:package-license-provenance", || {
        for package in &mut assembly_result.packages {
            package.backfill_license_provenance();
        }
    });

    record_detail_timing(&progress, "post-scan:package-reference-following", || {
        apply_package_reference_following(&mut scan_result.files, &mut assembly_result.packages);
    });
    progress.finish_post_scan();

    progress.start_finalize();

    let license_detections = record_detail_timing(&progress, "finalize:license-detections", || {
        if cli.from_json {
            let _ = preloaded_license_detections;
            collect_top_level_license_detections(&scan_result.files)
        } else {
            collect_top_level_license_detections(&scan_result.files)
        }
    });

    let should_recompute_license_references = cli.from_json
        && (!preloaded_license_references.is_empty()
            || !preloaded_license_rule_references.is_empty()
            || cli.license_references
            || (cli.license_url_template != DEFAULT_LICENSEDB_URL_TEMPLATE
                && !preloaded_license_references.is_empty()));

    if should_recompute_license_references && active_license_engine.is_none() {
        progress.start_license_detection_engine_creation();
        active_license_engine = Some(init_license_engine(&cli)?);
        progress.finish_license_detection_engine_creation("finalize:license-engine-creation");
    }

    let (license_references, license_rule_references) =
        record_detail_timing(&progress, "finalize:license-references", || {
            if cli.from_json && !should_recompute_license_references {
                (
                    preloaded_license_references,
                    preloaded_license_rule_references,
                )
            } else if cli.license_references || should_recompute_license_references {
                if let Some(engine) = active_license_engine.as_deref() {
                    collect_top_level_license_references(
                        &scan_result.files,
                        &assembly_result.packages,
                        engine.index(),
                        &cli.license_url_template,
                    )
                } else {
                    (Vec::new(), Vec::new())
                }
            } else {
                (Vec::new(), Vec::new())
            }
        });

    let end_time = Utc::now();
    let spdx_license_list_version = active_license_engine
        .as_ref()
        .and_then(|engine| engine.spdx_license_list_version().map(ToOwned::to_owned))
        .unwrap_or(LicenseDetectionEngine::embedded_spdx_license_list_version()?);

    let output = record_detail_timing(&progress, "finalize:output-prepare", || {
        create_output(
            start_time,
            end_time,
            scan_result,
            CreateOutputContext {
                total_dirs,
                assembly_result,
                license_detections,
                license_references,
                license_rule_references,
                spdx_license_list_version,
                extra_errors,
                extra_warnings: Vec::new(),
                header_options: cli.output_header_options(),
                options: CreateOutputOptions {
                    facet_rules: &facet_rules,
                    include_classify: cli.classify,
                    include_summary: cli.summary,
                    include_license_clarity_score: cli.license_clarity_score,
                    include_tallies: cli.tallies,
                    include_tallies_of_key_files: cli.tallies_key_files,
                    include_tallies_with_details: cli.tallies_with_details,
                    include_tallies_by_facet: cli.tallies_by_facet,
                    include_generated: cli.generated,
                    verbose: cli.verbose,
                },
            },
        )
    });
    progress.finish_finalize();

    let output_schema_output = crate::output_schema::Output::from(&output);
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

        let timing_name = format!("output:{:?}", target.format).to_lowercase();
        record_detail_timing(&progress, timing_name, || {
            write_output_file(&target.file, &output_schema_output, &output_config)
        })?;
        progress.output_written(&format!(
            "{:?} output written to {}",
            target.format, target.file
        ));
    }
    progress.record_final_counts(&output.files);
    progress.finish_output();

    let summary_end = Utc::now();
    progress.display_summary(
        &format_scancode_timestamp(&start_time),
        &format_scancode_timestamp(&summary_end),
    );

    Ok(())
}

#[cfg(feature = "golden-tests")]
fn touch_license_golden_symbols() {
    let _ = crate::license_detection::golden_utils::read_golden_input_content;
    let _ = crate::license_detection::golden_utils::detect_matches_for_golden;
    let _ = crate::license_detection::golden_utils::detect_license_expressions_for_golden;
    let _ = crate::license_detection::LicenseDetectionEngine::detect_matches_with_kind;
}

fn validate_scan_option_compatibility(cli: &Cli) -> Result<()> {
    if cli.from_json
        && (cli.package
            || cli.system_package
            || cli.package_in_compiled
            || cli.package_only
            || cli.copyright
            || cli.email
            || cli.url
            || cli.generated)
    {
        return Err(anyhow!(
            "When using --from-json, file scan options like --package/--copyright/--email/--url/--generated are not allowed"
        ));
    }

    if cli.from_json && (cli.cache_dir.is_some() || cli.cache_clear) {
        return Err(anyhow!(
            "Persistent cache options are only supported for directory scan mode, not --from-json"
        ));
    }

    if cli.from_json && cli.incremental {
        return Err(anyhow!(
            "--incremental is only supported for directory scan mode, not --from-json"
        ));
    }

    if !cli.from_json && cli.dir_path.is_empty() {
        return Err(anyhow!("Directory path is required for scan operations"));
    }

    if cli.tallies_by_facet && cli.facet.is_empty() {
        return Err(anyhow!(
            "--tallies-by-facet requires at least one --facet <facet>=<pattern> definition"
        ));
    }

    if cli.mark_source && !cli.info {
        return Err(anyhow!("--mark-source requires --info"));
    }

    Ok(())
}

fn prepare_cache_for_scan(scan_path: &str, cli: &Cli) -> Result<CacheConfig> {
    let env_cache_dir = env::var_os(CACHE_DIR_ENV_VAR).map(PathBuf::from);
    let config = CacheConfig::from_overrides(
        Path::new(scan_path),
        cli.cache_dir.as_deref().map(Path::new),
        env_cache_dir.as_deref(),
        cli.incremental,
    );

    if cli.cache_clear {
        crate::cache::locking::with_exclusive_cache_lock(config.root_dir(), || {
            config.clear_contents()
        })?;
    }

    if config.incremental_enabled() {
        config.ensure_dirs()?;
    }

    Ok(config)
}

fn partition_incremental_files(
    collected_files: &mut Vec<(PathBuf, fs::Metadata)>,
    scan_root: &Path,
    manifest: Option<&IncrementalManifest>,
) -> Vec<FileInfo> {
    let Some(manifest) = manifest else {
        return Vec::new();
    };

    let mut files_to_scan = Vec::new();
    let mut reused_files = Vec::new();

    for (path, metadata) in collected_files.drain(..) {
        let relative_path = normalize_relative_scan_path(&path, scan_root);
        let Some(entry) = manifest.entry(&relative_path) else {
            files_to_scan.push((path, metadata));
            continue;
        };

        match manifest_entry_matches_path(entry, &path, &metadata) {
            Ok(true) => reused_files.push(entry.file_info.clone()),
            Ok(false) | Err(_) => files_to_scan.push((path, metadata)),
        }
    }

    *collected_files = files_to_scan;
    reused_files
}

fn merge_incremental_file_results(
    processed_files: Vec<FileInfo>,
    reused_files: Vec<FileInfo>,
    ordered_file_paths: &[PathBuf],
) -> Vec<FileInfo> {
    let mut processed_file_entries = HashMap::new();
    let mut directory_entries = Vec::new();
    for file in processed_files {
        if file.file_type == FileType::File {
            processed_file_entries.insert(file.path.clone(), file);
        } else {
            directory_entries.push(file);
        }
    }

    let mut reused_file_entries: HashMap<_, _> = reused_files
        .into_iter()
        .map(|file| (file.path.clone(), file))
        .collect();

    let mut merged_files = Vec::new();
    for path in ordered_file_paths {
        let path_string = path.to_string_lossy().to_string();
        if let Some(file) = processed_file_entries.remove(&path_string) {
            merged_files.push(file);
            continue;
        }

        if let Some(file) = reused_file_entries.remove(&path_string) {
            merged_files.push(file);
        }
    }

    merged_files.extend(processed_file_entries.into_values());
    merged_files.extend(reused_file_entries.into_values());
    merged_files.extend(directory_entries);
    merged_files
}

fn build_incremental_manifest(
    scan_root: &Path,
    collected_files: &[(PathBuf, fs::Metadata)],
    files: &[FileInfo],
    options_fingerprint: &str,
) -> IncrementalManifest {
    let files_by_relative_path: HashMap<_, _> = files
        .iter()
        .filter(|file| file.file_type == FileType::File)
        .map(|file| {
            (
                normalize_relative_scan_path(Path::new(&file.path), scan_root),
                file.clone(),
            )
        })
        .collect();

    let entries = collected_files
        .iter()
        .filter_map(|(path, metadata)| {
            let relative_path = normalize_relative_scan_path(path, scan_root);
            let state = metadata_fingerprint(metadata)?;
            let file_info = files_by_relative_path.get(&relative_path)?.clone();
            let content_sha256 = file_info.sha256.unwrap_or_else(|| {
                fs::read(path)
                    .map(|bytes| calculate_sha256(&bytes))
                    .unwrap_or_else(|_| {
                        Sha256Digest::from_hex(
                            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                        )
                        .unwrap()
                    })
            });
            Some((
                relative_path,
                IncrementalManifestEntry {
                    state,
                    content_sha256,
                    file_info,
                },
            ))
        })
        .collect::<BTreeMap<_, _>>();

    IncrementalManifest::new(options_fingerprint.to_string(), entries)
}

fn incremental_manifest_key(scan_root: &Path, options_fingerprint: &str) -> String {
    let canonical_root = fs::canonicalize(scan_root).unwrap_or_else(|_| scan_root.to_path_buf());
    calculate_sha256(
        format!(
            "{}\n{options_fingerprint}",
            canonical_root.to_string_lossy()
        )
        .as_bytes(),
    )
    .as_hex()
}

fn normalize_relative_scan_path(path: &Path, scan_root: &Path) -> String {
    path.strip_prefix(scan_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn compile_regex_patterns(option_name: &str, patterns: &[String]) -> Result<Vec<Regex>> {
    patterns
        .iter()
        .map(|pattern| {
            Regex::new(pattern).map_err(|err| {
                anyhow!("Invalid regex for {option_name} pattern \"{pattern}\": {err}")
            })
        })
        .collect()
}

fn effective_timeout_seconds(process_mode: ProcessMode, timeout_seconds: f64) -> f64 {
    match process_mode {
        ProcessMode::SequentialWithoutTimeouts => 0.0,
        ProcessMode::Parallel(_) | ProcessMode::SequentialWithTimeouts => timeout_seconds,
    }
}

fn process_mode_message(process_mode: ProcessMode) -> Option<&'static str> {
    match process_mode {
        ProcessMode::SequentialWithTimeouts => Some("Disabling multi-processing for debugging."),
        ProcessMode::SequentialWithoutTimeouts => {
            Some("Disabling multi-processing and multi-threading for debugging.")
        }
        ProcessMode::Parallel(_) => None,
    }
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
    let mut names = Vec::new();
    if cli.license {
        names.push("licenses");
    }
    if cli.info {
        names.push("info");
    }
    if cli.package {
        names.push("packages");
    }
    if (cli.system_package || cli.package_in_compiled || cli.package_only)
        && !names.contains(&"packages")
    {
        names.push("packages");
    }
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

fn should_include_info_surface(files: &[crate::models::FileInfo], cli: &Cli) -> bool {
    cli.info
        || files.iter().any(|file| {
            file.date.is_some()
                || file.sha1.is_some()
                || file.md5.is_some()
                || file.sha256.is_some()
                || file.sha1_git.is_some()
                || file.mime_type.is_some()
                || file.file_type_label.is_some()
                || file.programming_language.is_some()
                || file.is_binary.is_some()
                || file.is_text.is_some()
                || file.is_archive.is_some()
                || file.is_media.is_some()
                || file.is_source.is_some()
                || file.is_script.is_some()
                || file.files_count.is_some()
                || file.dirs_count.is_some()
                || file.size_count.is_some()
        })
}

fn populate_info_resource_counts(files: &mut [crate::models::FileInfo]) {
    let snapshot: Vec<(String, crate::models::FileType, u64)> = files
        .iter()
        .map(|file| (file.path.clone(), file.file_type.clone(), file.size))
        .collect();

    for file in files {
        match file.file_type {
            crate::models::FileType::Directory => {
                let prefix = format!("{}/", file.path);
                let mut files_count = 0usize;
                let mut dirs_count = 0usize;
                let mut size_count = 0u64;
                for (path, file_type, size) in &snapshot {
                    if !path.starts_with(&prefix) {
                        continue;
                    }
                    match file_type {
                        crate::models::FileType::Directory => dirs_count += 1,
                        crate::models::FileType::File => {
                            files_count += 1;
                            size_count += *size;
                        }
                    }
                }
                file.files_count = Some(files_count);
                file.dirs_count = Some(dirs_count);
                file.size_count = Some(size_count);
            }
            crate::models::FileType::File => {
                file.files_count = Some(0);
                file.dirs_count = Some(0);
                file.size_count = Some(0);
            }
        }
    }
}

fn record_detail_timing<T, F>(progress: &Arc<ScanProgress>, name: impl Into<String>, f: F) -> T
where
    F: FnOnce() -> T,
{
    let started = Instant::now();
    let result = f();
    progress.record_detail_timing(name.into(), started.elapsed().as_secs_f64());
    result
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

fn init_license_engine(cli: &Cli) -> Result<Arc<LicenseDetectionEngine>> {
    let cache_dir = cli
        .license_cache_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(LicenseCacheConfig::default_cache_dir);
    let cache_config = LicenseCacheConfig::new(cache_dir, cli.reindex);

    match &cli.license_rules_path {
        Some(p) => {
            let path = PathBuf::from(p);
            if !path.exists() {
                return Err(anyhow!("License rules path does not exist: {:?}", path));
            }
            let engine = LicenseDetectionEngine::from_directory_with_cache(&path, &cache_config)?;
            Ok(Arc::new(engine))
        }
        None => {
            let engine = LicenseDetectionEngine::from_embedded_with_cache(&cache_config)?;
            Ok(Arc::new(engine))
        }
    }
}

fn describe_license_engine_source(
    engine: &LicenseDetectionEngine,
    rules_path: Option<&str>,
) -> String {
    match rules_path {
        Some(path) => format!(
            "License detection engine initialized with {} rules from {}",
            engine.index().rules_by_rid.len(),
            path
        ),
        None => format!(
            "License detection engine initialized with {} rules from embedded artifact",
            engine.index().rules_by_rid.len()
        ),
    }
}

#[cfg(test)]
mod main_test;
