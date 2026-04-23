// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use crate::assembly;
use crate::cache::{
    CACHE_DIR_ENV_VAR, CacheConfig, IncrementalManifest, IncrementalManifestEntry,
    build_collection_exclude_patterns, incremental_manifest_path, load_incremental_manifest,
    manifest_entry_matches_path, metadata_fingerprint, write_incremental_manifest,
};
use crate::cli::{Cli, ProcessMode};
use crate::license_detection::LicenseDetectionEngine;
use crate::license_detection::dataset::export_embedded_license_dataset;
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
    SelectedPath, apply_cli_path_selection_filter, apply_ignore_resource_filter, apply_mark_source,
    apply_only_findings_filter, apply_user_path_filters_to_collected, filter_redundant_clues,
    filter_redundant_clues_with_rules, load_and_merge_json_inputs, normalize_paths,
    normalize_top_level_output_paths, populate_info_resource_counts,
    prepare_filter_clue_rule_lookup, resolve_native_scan_inputs, resolve_paths_file_entries,
    trim_preloaded_assembly_to_files,
};
use crate::scanner::{
    CollectionFrontier, LicenseScanOptions, TextDetectionOptions, collect_paths,
    collect_selected_paths, process_collected_with_memory_limit,
    process_collected_with_memory_limit_sequential, scan_options_fingerprint,
};
use crate::time::format_scancode_timestamp;
use crate::utils::hash::calculate_sha256;
use anyhow::{Result, anyhow};
use chrono::Utc;
use clap::Parser;
use regex::Regex;
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

pub fn run() -> Result<()> {
    #[cfg(feature = "golden-tests")]
    touch_license_golden_symbols();

    let cli = Cli::parse();

    validate_scan_option_compatibility(&cli)?;

    if cli.show_attribution {
        print!("{}", include_str!("../../../NOTICE"));
        return Ok(());
    }

    if let Some(export_dir) = cli.export_license_dataset.as_deref() {
        export_embedded_license_dataset(Path::new(export_dir))?;
        return Ok(());
    }

    let start_time = Utc::now();
    let progress = Arc::new(ScanProgress::new(progress_mode_from_cli(&cli)));
    progress.set_processes(cli.processes);
    progress.set_scan_names(configured_scan_names(&cli));
    progress.init_logging_bridge();
    let mut shared_license_cache_config: Option<LicenseCacheConfig> = None;

    progress.start_setup();
    let facet_rules = build_facet_rules(&cli.facet)?;

    let ignore_author_patterns = compile_regex_patterns("--ignore-author", &cli.ignore_author)?;
    let ignore_copyright_holder_patterns =
        compile_regex_patterns("--ignore-copyright-holder", &cli.ignore_copyright_holder)?;
    progress.finish_setup();

    progress.start_discovery();

    let mut shared_cache_config = if cli.from_json {
        let cache_config = prepare_cache_config(None, &cli)?;
        shared_license_cache_config = Some(build_license_cache_config(&cache_config, &cli));
        Some(cache_config)
    } else {
        None
    };

    let (
        mut scan_result,
        total_dirs,
        mut preloaded_assembly,
        preloaded_license_detections,
        preloaded_license_references,
        preloaded_license_rule_references,
        preloaded_extra_errors,
        extra_warnings,
        imported_spdx_license_list_version,
        imported_license_index_provenance,
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
            imported_spdx_license_list_version,
            imported_license_index_provenance,
        ) = loaded.into_parts()?;
        (
            process_result,
            directories_count,
            assembly_result,
            license_detections,
            license_references,
            license_rule_references,
            extra_errors,
            Vec::new(),
            imported_spdx_license_list_version,
            imported_license_index_provenance,
            None,
        )
    } else {
        let NativeScanSelection {
            scan_path,
            selected_paths,
            collection_frontier,
            missing_entries: missing_paths_file_entries,
        } = resolve_native_scan_selection(&cli)?;
        let paths_file_warnings = build_paths_file_warning_messages(&missing_paths_file_entries);
        for warning in &paths_file_warnings {
            progress.output_written(warning);
        }

        let cache_config = prepare_cache_config(Some(Path::new(&scan_path)), &cli)?;
        shared_license_cache_config = Some(build_license_cache_config(&cache_config, &cli));
        shared_cache_config = Some(cache_config.clone());
        let collection_exclude_patterns =
            build_collection_exclude_patterns(Path::new(&scan_path), cache_config.root_dir());

        let mut collected = if cli.paths_file.is_empty() {
            collect_paths(&scan_path, cli.max_depth, &collection_exclude_patterns)
        } else {
            collect_selected_paths(
                Path::new(&scan_path),
                &collection_frontier,
                cli.max_depth,
                &collection_exclude_patterns,
            )
        };
        let user_excluded_count = apply_user_path_filters_to_collected(
            &mut collected,
            Path::new(&scan_path),
            &selected_paths,
            &cli.include,
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
            let engine = init_license_engine(
                shared_cache_config
                    .as_ref()
                    .expect("cache config should be prepared before license engine init"),
                &cli,
            )?;
            progress.finish_license_detection_engine_creation("setup_scan:licenses");
            progress.finish_setup();
            progress.output_written(&describe_license_engine_source(
                &engine,
                cli.license_dataset_path.as_deref(),
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
            paths_file_warnings,
            None,
            None,
            license_engine,
        )
    };

    progress.start_post_scan();

    if cli.filter_clues {
        progress.post_scan_step("Filtering redundant clues...");
        let clue_rule_lookup = record_detail_timing(&progress, "post-scan:filter-clues", || {
            prepare_filter_clue_rule_lookup(
                &scan_result.files,
                active_license_engine.as_deref(),
                cli.license_dataset_path.as_deref(),
                shared_license_cache_config.as_ref(),
            )
        })?;
        if let Some(clue_rule_lookup) = clue_rule_lookup.as_ref() {
            filter_redundant_clues_with_rules(&mut scan_result.files, Some(clue_rule_lookup));
        } else {
            filter_redundant_clues(&mut scan_result.files);
        }
    }

    if !ignore_author_patterns.is_empty() || !ignore_copyright_holder_patterns.is_empty() {
        progress.post_scan_step("Applying ignore-resource filters...");
        record_detail_timing(&progress, "post-scan:ignore-resource", || {
            apply_ignore_resource_filter(
                &mut scan_result.files,
                &ignore_copyright_holder_patterns,
                &ignore_author_patterns,
            );
        });
    }

    if cli.from_json && (!cli.include.is_empty() || !cli.exclude.is_empty()) {
        progress.post_scan_step("Applying path selection filters...");
        record_detail_timing(&progress, "output-filter:path-selection", || {
            apply_cli_path_selection_filter(&mut scan_result.files, &cli.include, &cli.exclude);
        });
    }

    if cli.only_findings {
        progress.post_scan_step("Filtering to resources with findings...");
        record_detail_timing(&progress, "output-filter:only-findings", || {
            apply_only_findings_filter(&mut scan_result.files);
        });
    }

    if cli.info && cli.mark_source {
        progress.post_scan_step("Marking source files...");
        record_detail_timing(&progress, "post-scan:mark-source", || {
            apply_mark_source(&mut scan_result.files);
        });
    }

    if should_include_info_surface(&scan_result.files, &cli) {
        progress.post_scan_step("Populating info resource counts...");
        record_detail_timing(&progress, "post-scan:info-resource-counts", || {
            populate_info_resource_counts(&mut scan_result.files);
        });
    }

    progress.post_scan_step("Backfilling license provenance...");
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
        progress.post_scan_step("Applying license policy...");
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
        progress.post_scan_step("Trimming preloaded assembly to filtered files...");
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

    let mut assembly_result = if skip_assembly {
        assembly::AssemblyResult {
            packages: Vec::new(),
            dependencies: Vec::new(),
        }
    } else {
        progress.start_assembly();

        let mut result = if cli.from_json
            && (!preloaded_assembly.packages.is_empty()
                || !preloaded_assembly.dependencies.is_empty())
        {
            progress.assembly_step("Using preloaded assembly...");
            preloaded_assembly
        } else {
            assembly::assemble(&mut scan_result.files)
        };

        progress.assembly_step("Backfilling package license provenance...");
        record_detail_timing(&progress, "assembly:package-license-provenance", || {
            for package in &mut result.packages {
                package.backfill_license_provenance();
            }
        });

        progress.assembly_step("Applying package reference following...");
        record_detail_timing(&progress, "assembly:package-reference-following", || {
            apply_package_reference_following(&mut scan_result.files, &mut result.packages);
        });

        progress.finish_assembly(result.packages.len(), manifests_seen);
        result
    };

    progress.start_finalize();

    if !cli.from_json && (cli.strip_root || cli.full_root) {
        let root_path = cli
            .dir_path
            .first()
            .ok_or_else(|| anyhow!("No input path available for path normalization"))?;
        progress.finalize_step("Normalizing paths...");
        record_detail_timing(&progress, "finalize:path-normalization", || {
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
    }

    progress.finalize_step("Collecting license detections...");
    let license_detections = record_detail_timing(&progress, "finalize:license-detections", || {
        let preserve_preloaded_top_level_detections = cli.from_json
            && (cli.only_findings || !cli.include.is_empty() || !cli.exclude.is_empty());
        collect_top_level_license_detections_for_mode(
            &scan_result.files,
            preloaded_license_detections,
            preserve_preloaded_top_level_detections,
            cli.from_json && cli.dir_path.len() > 1,
        )
    });

    let should_recompute_license_references = cli.from_json
        && (!preloaded_license_references.is_empty()
            || !preloaded_license_rule_references.is_empty()
            || cli.license_references
            || (cli.license_url_template != DEFAULT_LICENSEDB_URL_TEMPLATE
                && !preloaded_license_references.is_empty()));

    if should_recompute_license_references && active_license_engine.is_none() {
        progress.start_license_detection_engine_creation();
        active_license_engine = Some(init_license_engine(
            shared_cache_config
                .as_ref()
                .expect("cache config should be prepared before license engine init"),
            &cli,
        )?);
        progress.finish_license_detection_engine_creation("finalize:license-engine-creation");
    }

    progress.finalize_step("Collecting license references...");
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
        .or(imported_spdx_license_list_version)
        .unwrap_or(LicenseDetectionEngine::embedded_spdx_license_list_version()?);
    let license_index_provenance = active_license_engine
        .as_ref()
        .and_then(|engine| engine.license_index_provenance().cloned())
        .or(imported_license_index_provenance);

    progress.finalize_step("Preparing output...");
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
                license_index_provenance,
                extra_errors,
                extra_warnings,
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
    progress.record_final_header_counts(&output.headers);
    progress.finish_output();

    let summary_end = Utc::now();
    progress.display_summary(
        &format_scancode_timestamp(&start_time),
        &format_scancode_timestamp(&summary_end),
    );

    Ok(())
}

fn collect_top_level_license_detections_for_mode(
    files: &[FileInfo],
    preloaded: Vec<crate::models::TopLevelLicenseDetection>,
    preserve_preloaded: bool,
    clear_for_multi_input_replay: bool,
) -> Vec<crate::models::TopLevelLicenseDetection> {
    if clear_for_multi_input_replay {
        Vec::new()
    } else if preserve_preloaded {
        preloaded
    } else {
        collect_top_level_license_detections(files)
    }
}

#[cfg(feature = "golden-tests")]
fn touch_license_golden_symbols() {
    let _ = crate::license_detection::golden_utils::read_golden_input_content;
    let _ = crate::license_detection::golden_utils::detect_matches_for_golden;
    let _ = crate::license_detection::golden_utils::detect_license_expressions_for_golden;
    let _ = crate::license_detection::LicenseDetectionEngine::detect_matches_with_kind;
}

#[derive(Debug)]
struct NativeScanSelection {
    scan_path: String,
    selected_paths: Vec<SelectedPath>,
    collection_frontier: Vec<CollectionFrontier>,
    missing_entries: Vec<String>,
}

fn resolve_native_scan_selection(cli: &Cli) -> Result<NativeScanSelection> {
    if cli.paths_file.is_empty() {
        let (scan_path, selected_paths) = resolve_native_scan_inputs(&cli.dir_path)?;
        return Ok(NativeScanSelection {
            scan_path,
            selected_paths,
            collection_frontier: Vec::new(),
            missing_entries: Vec::new(),
        });
    }

    let scan_path = cli
        .dir_path
        .first()
        .cloned()
        .ok_or_else(|| anyhow!("--paths-file requires one positional scan root"))?;
    let path_file_entries = load_paths_file_entries(&cli.paths_file)?;
    let resolved = resolve_paths_file_entries(Path::new(&scan_path), &path_file_entries)?;
    if resolved.selections.is_empty() {
        return Err(anyhow!(
            "--paths-file did not resolve to any existing files or directories under {:?}",
            Path::new(&scan_path)
        ));
    }

    Ok(NativeScanSelection {
        scan_path,
        selected_paths: resolved.selections,
        collection_frontier: resolved.frontier,
        missing_entries: resolved.missing_entries,
    })
}

fn load_paths_file_entries(paths_files: &[String]) -> Result<Vec<String>> {
    let mut entries = Vec::new();
    for paths_file in paths_files {
        let content = read_paths_file_content(paths_file)?;
        entries.extend(content.lines().map(ToOwned::to_owned));
    }
    Ok(entries)
}

fn read_paths_file_content(paths_file: &str) -> Result<String> {
    if paths_file == "-" {
        let mut content = String::new();
        std::io::stdin()
            .read_to_string(&mut content)
            .map_err(|err| anyhow!("Failed to read --paths-file from stdin: {err}"))?;
        return Ok(content);
    }

    fs::read_to_string(paths_file)
        .map_err(|err| anyhow!("Failed to read --paths-file {:?}: {err}", paths_file))
}

fn build_paths_file_warning_messages(missing_entries: &[String]) -> Vec<String> {
    missing_entries
        .iter()
        .map(|entry| format!("Skipping missing --paths-file entry: {entry}"))
        .collect()
}

fn validate_scan_option_compatibility(cli: &Cli) -> Result<()> {
    if cli.show_attribution {
        return Ok(());
    }

    if cli.export_license_dataset.is_some() {
        if !cli.dir_path.is_empty() || !cli.paths_file.is_empty() {
            return Err(anyhow!(
                "--export-license-dataset does not accept scan input paths or --paths-file"
            ));
        }

        if cli.from_json
            || cli.license
            || cli.package
            || cli.system_package
            || cli.package_in_compiled
            || cli.package_only
            || cli.copyright
            || cli.email
            || cli.url
            || cli.generated
            || cli.info
            || cli.incremental
            || cli.reindex
            || cli.no_license_index_cache
            || cli.license_dataset_path.is_some()
        {
            return Err(anyhow!(
                "--export-license-dataset is a standalone mode and cannot be combined with scan or license-index flags"
            ));
        }

        return Ok(());
    }

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

    if cli.from_json && !cli.paths_file.is_empty() {
        return Err(anyhow!(
            "--paths-file is only supported for native scan mode, not --from-json"
        ));
    }

    if cli.from_json && cli.incremental {
        return Err(anyhow!(
            "--incremental is only supported for directory scan mode, not --from-json"
        ));
    }

    if !cli.paths_file.is_empty() && cli.dir_path.len() != 1 {
        return Err(anyhow!(
            "--paths-file requires exactly one positional scan root"
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

fn prepare_cache_config(scan_root: Option<&Path>, cli: &Cli) -> Result<CacheConfig> {
    let env_cache_dir = env::var_os(CACHE_DIR_ENV_VAR).map(PathBuf::from);
    let config = CacheConfig::from_overrides(
        scan_root,
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

fn build_license_cache_config(cache_root: &CacheConfig, cli: &Cli) -> LicenseCacheConfig {
    LicenseCacheConfig::new(
        cache_root.root_dir().to_path_buf(),
        cli.reindex,
        !cli.no_license_index_cache,
    )
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

fn init_license_engine(cache_root: &CacheConfig, cli: &Cli) -> Result<Arc<LicenseDetectionEngine>> {
    let cache_config = build_license_cache_config(cache_root, cli);

    match &cli.license_dataset_path {
        Some(p) => {
            let path = PathBuf::from(p);
            if !path.exists() {
                return Err(anyhow!("License dataset path does not exist: {:?}", path));
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
            "License detection engine initialized with {} rules from custom dataset {}",
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
mod tests;
