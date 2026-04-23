// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::cli::ProcessMode;
use crate::models::{LineNumber, MatchScore};
use serde_json::json;
use std::fs;
use std::path::Path;

use crate::cache::{CacheConfig, DEFAULT_CACHE_DIR_NAME, build_collection_exclude_patterns};
use crate::license_detection::LicenseDetectionEngine;
use crate::post_processing::{
    DEFAULT_LICENSEDB_URL_TEMPLATE, apply_package_reference_following,
    collect_top_level_license_detections, collect_top_level_license_references,
};
use crate::scan_result_shaping::json_input::{
    JsonScanInput, load_scan_from_json, normalize_loaded_json_scan,
};
use crate::scanner::collect_paths;

#[test]
fn process_mode_to_i32_supports_reference_compat_values() {
    assert_eq!(ProcessMode::SequentialWithoutTimeouts.to_i32(), -1);
    assert_eq!(ProcessMode::SequentialWithTimeouts.to_i32(), 0);
    assert_eq!(ProcessMode::Parallel(4).to_i32(), 4);
    assert_eq!(
        effective_timeout_seconds(ProcessMode::SequentialWithoutTimeouts, 30.0),
        0.0
    );
    assert_eq!(
        effective_timeout_seconds(ProcessMode::SequentialWithTimeouts, 30.0),
        30.0
    );
}

#[test]
fn configured_scan_names_only_lists_enabled_non_license_scans() {
    let package_cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--package",
        "README.md",
    ])
    .unwrap();
    assert_eq!(configured_scan_names(&package_cli), "packages");

    let package_only_cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--package-only",
        "README.md",
    ])
    .unwrap();
    assert_eq!(configured_scan_names(&package_only_cli), "packages");

    let mixed_cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--info",
        "--email",
        "README.md",
    ])
    .unwrap();
    assert_eq!(configured_scan_names(&mixed_cli), "info, emails");
}

#[test]
fn configured_scan_names_keeps_license_first_when_enabled() {
    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--license",
        "--package",
        "README.md",
    ])
    .unwrap();

    assert_eq!(configured_scan_names(&cli), "licenses, packages");
}

#[test]
fn validate_scan_option_compatibility_rejects_scan_flags_with_from_json() {
    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--from-json",
        "--copyright",
        "sample-scan.json",
    ])
    .unwrap();
    assert!(validate_scan_option_compatibility(&cli).is_err());
}

#[test]
fn validate_scan_option_compatibility_allows_cache_root_flags_with_from_json() {
    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--from-json",
        "--cache-dir",
        "/tmp/cache",
        "sample-scan.json",
    ])
    .unwrap();

    assert!(validate_scan_option_compatibility(&cli).is_ok());
}

#[test]
fn validate_scan_option_compatibility_allows_license_cache_opt_out_with_from_json() {
    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--from-json",
        "--no-license-index-cache",
        "sample-scan.json",
    ])
    .unwrap();

    assert!(validate_scan_option_compatibility(&cli).is_ok());
}

#[test]
fn validate_scan_option_compatibility_rejects_incremental_with_from_json() {
    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--from-json",
        "--incremental",
        "sample-scan.json",
    ])
    .unwrap();

    let error = validate_scan_option_compatibility(&cli).unwrap_err();
    assert!(error.to_string().contains("--incremental"));
}

#[test]
fn validate_scan_option_compatibility_rejects_package_with_from_json() {
    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--from-json",
        "--package",
        "sample-scan.json",
    ])
    .unwrap();
    assert!(validate_scan_option_compatibility(&cli).is_err());
}

#[test]
fn validate_scan_option_compatibility_rejects_generated_with_from_json() {
    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--from-json",
        "--generated",
        "sample-scan.json",
    ])
    .unwrap();
    assert!(validate_scan_option_compatibility(&cli).is_err());
}

#[test]
fn validate_scan_option_compatibility_allows_strip_root_with_from_json() {
    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--from-json",
        "--strip-root",
        "sample-scan.json",
    ])
    .unwrap();
    assert!(validate_scan_option_compatibility(&cli).is_ok());
}

#[test]
fn validate_scan_option_compatibility_allows_full_root_with_from_json() {
    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--from-json",
        "--full-root",
        "sample-scan.json",
    ])
    .unwrap();
    assert!(validate_scan_option_compatibility(&cli).is_ok());
}

#[test]
fn validate_scan_option_compatibility_allows_scan_flags_without_from_json() {
    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--copyright",
        "sample-dir",
    ])
    .unwrap();
    assert!(validate_scan_option_compatibility(&cli).is_ok());
}

#[test]
fn validate_scan_option_compatibility_allows_multiple_inputs_with_from_json() {
    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--from-json",
        "scan-a.json",
        "scan-b.json",
    ])
    .unwrap();
    assert!(validate_scan_option_compatibility(&cli).is_ok());
}

#[test]
fn compile_regex_patterns_rejects_invalid_regex() {
    let result = compile_regex_patterns("--ignore-author", &["[".to_string()]);

    assert!(result.is_err());
    let error = result.err().unwrap().to_string();
    assert!(error.contains("--ignore-author"));
    assert!(error.contains("Invalid regex"));
}

#[test]
fn from_json_with_no_assemble_preserves_preloaded_package_sections() {
    let temp_path = std::env::temp_dir().join("provenant-from-json-with-packages-test.json");
    let content = json!({
        "files": [],
        "packages": [
            {
                "package_uid": "pkg:npm/demo@1.0.0",
                "type": "npm",
                "name": "demo",
                "version": "1.0.0",
                "parties": [],
                "datafile_paths": ["package.json"],
                "datasource_ids": ["npm_package_json"]
            }
        ],
        "dependencies": [
            {
                "purl": "pkg:npm/dep@2.0.0",
                "scope": "dependencies",
                "is_runtime": true,
                "is_optional": false,
                "is_pinned": true,
                "dependency_uid": "pkg:npm/dep@2.0.0?uuid=test",
                "for_package_uid": "pkg:npm/demo@1.0.0",
                "datafile_path": "package.json",
                "datasource_id": "npm_package_json"
            }
        ],
        "license_detections": [],
        "license_references": [],
        "license_rule_references": []
    });
    fs::write(&temp_path, content.to_string()).expect("write json fixture");

    let parsed = load_scan_from_json(temp_path.to_str().expect("utf-8 path"))
        .expect("from-json loading should succeed");

    let packages: Vec<crate::models::Package> = parsed
        .packages
        .iter()
        .map(crate::models::Package::try_from)
        .collect::<Result<Vec<_>, _>>()
        .expect("package conversion should succeed");
    let dependencies: Vec<crate::models::TopLevelDependency> = parsed
        .dependencies
        .iter()
        .map(crate::models::TopLevelDependency::try_from)
        .collect::<Result<Vec<_>, _>>()
        .expect("dependency conversion should succeed");

    let preloaded = assembly::AssemblyResult {
        packages,
        dependencies,
    };

    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--from-json",
        "--no-assemble",
        temp_path.to_str().expect("utf-8 path"),
    ])
    .expect("cli parse should succeed");

    let assembly_result = if cli.from_json
        && (!preloaded.packages.is_empty() || !preloaded.dependencies.is_empty())
    {
        preloaded
    } else if cli.no_assemble {
        assembly::AssemblyResult {
            packages: Vec::new(),
            dependencies: Vec::new(),
        }
    } else {
        unreachable!("test only covers from-json preload precedence")
    };

    assert_eq!(assembly_result.packages.len(), 1);
    assert_eq!(assembly_result.dependencies.len(), 1);

    let _ = fs::remove_file(temp_path);
}

#[test]
fn validate_scan_option_compatibility_allows_multiple_paths_without_from_json() {
    let cli =
        crate::cli::Cli::try_parse_from(["provenant", "--json-pp", "scan.json", "dir-a", "dir-b"])
            .unwrap();
    assert!(validate_scan_option_compatibility(&cli).is_ok());
}

#[test]
fn validate_scan_option_compatibility_rejects_paths_file_with_from_json() {
    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--from-json",
        "--paths-file",
        "changed-files.txt",
        "sample-scan.json",
    ])
    .unwrap();

    let error = validate_scan_option_compatibility(&cli).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("--paths-file is only supported for native scan mode")
    );
}

#[test]
fn validate_scan_option_compatibility_rejects_paths_file_without_single_root() {
    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--paths-file",
        "changed-files.txt",
    ])
    .unwrap();

    let error = validate_scan_option_compatibility(&cli).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("--paths-file requires exactly one positional scan root")
    );
}

#[test]
fn validate_scan_option_compatibility_rejects_mark_source_without_info() {
    let mut cli =
        crate::cli::Cli::try_parse_from(["provenant", "--json-pp", "scan.json", "sample-dir"])
            .unwrap();
    cli.mark_source = true;

    let error = validate_scan_option_compatibility(&cli).unwrap_err();
    assert!(error.to_string().contains("--mark-source requires --info"));
}

#[test]
fn validate_scan_option_compatibility_allows_export_license_dataset_mode() {
    let cli =
        crate::cli::Cli::try_parse_from(["provenant", "--export-license-dataset", "dataset-out"])
            .unwrap();

    assert!(validate_scan_option_compatibility(&cli).is_ok());
}

#[test]
fn validate_scan_option_compatibility_rejects_scan_flags_with_export_license_dataset() {
    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--export-license-dataset",
        "dataset-out",
        "--license",
    ])
    .unwrap();

    let error = validate_scan_option_compatibility(&cli).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("--export-license-dataset is a standalone mode")
    );
}

#[test]
fn from_json_skips_final_native_projection_block() {
    let mut loaded = JsonScanInput {
        headers: vec![],
        files: vec![crate::output_schema::OutputFileInfo::from(&json_file(
            "/tmp/archive/root/src/main.rs",
            crate::models::FileType::File,
        ))],
        packages: vec![],
        dependencies: vec![],
        license_detections: vec![],
        license_references: vec![],
        license_rule_references: vec![],
        excluded_count: 0,
    };

    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--from-json",
        "--full-root",
        "sample-scan.json",
    ])
    .expect("cli parse should succeed");

    normalize_loaded_json_scan(&mut loaded, false, true);

    if !cli.from_json && (cli.strip_root || cli.full_root) {
        let mut files: Vec<crate::models::FileInfo> = loaded
            .files
            .iter()
            .map(crate::models::FileInfo::try_from)
            .collect::<Result<Vec<_>, _>>()
            .expect("file conversion should succeed");
        normalize_paths(
            &mut files,
            cli.dir_path.first().expect("input path exists"),
            cli.strip_root,
            cli.full_root,
        );
    }

    assert_eq!(loaded.files[0].path, "tmp/archive/root/src/main.rs");
}

#[test]
fn from_json_loaded_manifest_detections_can_be_recomputed_into_top_level_uniques() {
    let mut file0 = json_file("project/package.json", crate::models::FileType::File);
    file0.package_data = vec![crate::models::PackageData {
        package_type: Some(crate::models::PackageType::Npm),
        license_detections: vec![crate::models::LicenseDetection {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            matches: vec![crate::models::Match {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                from_file: None,
                start_line: LineNumber::ONE,
                end_line: LineNumber::ONE,
                matcher: Some("parser-declared-license".to_string()),
                score: MatchScore::MAX,
                matched_length: Some(1),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: None,
                rule_url: None,
                matched_text: Some("MIT".to_string()),
                referenced_filenames: None,
                matched_text_diagnostics: None,
            }],
            detection_log: vec![],
            identifier: None,
        }],
        ..Default::default()
    }];
    let mut files = vec![file0];

    for file in &mut files {
        file.backfill_license_provenance();
    }

    let top_level = collect_top_level_license_detections(&files);

    assert_eq!(top_level.len(), 1);
    assert_eq!(top_level[0].license_expression, "mit");
    assert_eq!(
        top_level[0].reference_matches[0].from_file.as_deref(),
        Some("project/package.json")
    );
    assert_eq!(
        top_level[0].reference_matches[0].rule_identifier.as_deref(),
        Some("parser-declared-license")
    );
}

#[test]
fn from_json_recomputes_top_level_uniques_even_without_shaping_flags() {
    let mut file0 = json_file("project/package.json", crate::models::FileType::File);
    file0.package_data = vec![crate::models::PackageData {
        package_type: Some(crate::models::PackageType::Npm),
        other_license_detections: vec![crate::models::LicenseDetection {
            license_expression: "gpl-2.0-only".to_string(),
            license_expression_spdx: "GPL-2.0-only".to_string(),
            matches: vec![crate::models::Match {
                license_expression: "gpl-2.0-only".to_string(),
                license_expression_spdx: "GPL-2.0-only".to_string(),
                from_file: None,
                start_line: LineNumber::ONE,
                end_line: LineNumber::ONE,
                matcher: Some("parser-declared-license".to_string()),
                score: MatchScore::MAX,
                matched_length: Some(1),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: None,
                rule_url: None,
                matched_text: Some("GPL-2.0-only".to_string()),
                referenced_filenames: None,
                matched_text_diagnostics: None,
            }],
            detection_log: vec![],
            identifier: None,
        }],
        ..Default::default()
    }];
    let mut files = vec![file0];

    for file in &mut files {
        file.backfill_license_provenance();
    }

    let top_level = collect_top_level_license_detections(&files);

    assert_eq!(top_level.len(), 1);
    assert_eq!(top_level[0].license_expression, "gpl-2.0-only");
    assert_ne!(top_level[0].identifier, "stale-id");
    assert_eq!(
        top_level[0].reference_matches[0].rule_identifier.as_deref(),
        Some("parser-declared-license")
    );
}

#[test]
fn from_json_only_findings_keeps_files_with_findings() {
    let mut file = json_file("project/package.json", crate::models::FileType::File);
    file.license_expression = Some("mit".to_string());
    let mut files = vec![file];

    apply_only_findings_filter(&mut files);

    assert_eq!(files.len(), 1);
}

#[test]
fn native_only_findings_still_keeps_files_with_findings() {
    let mut file = json_file("project/package.json", crate::models::FileType::File);
    file.license_expression = Some("mit".to_string());
    let mut files = vec![file];

    apply_only_findings_filter(&mut files);

    assert_eq!(files.len(), 1);
}

#[test]
fn from_json_only_findings_preserves_preloaded_top_level_detections() {
    let files = vec![json_file(
        "project/package.json",
        crate::models::FileType::File,
    )];
    let preloaded = vec![crate::models::TopLevelLicenseDetection {
        identifier: "mit-id".to_string(),
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        detection_count: 1,
        detection_log: vec![],
        reference_matches: vec![],
    }];

    let detections = collect_top_level_license_detections_for_mode(&files, preloaded, true, false);

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].license_expression, "mit");
}

#[test]
fn from_json_filtered_replay_preserves_preloaded_top_level_detections() {
    let files = vec![json_file(
        "project/package.json",
        crate::models::FileType::File,
    )];
    let preloaded = vec![crate::models::TopLevelLicenseDetection {
        identifier: "mit-id".to_string(),
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        detection_count: 1,
        detection_log: vec![],
        reference_matches: vec![],
    }];

    let detections = collect_top_level_license_detections_for_mode(&files, preloaded, true, false);

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].license_expression, "mit");
}

#[test]
fn from_json_multi_input_replay_clears_top_level_detections() {
    let files = vec![json_file(
        "project/package.json",
        crate::models::FileType::File,
    )];
    let preloaded = vec![crate::models::TopLevelLicenseDetection {
        identifier: "mit-id".to_string(),
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        detection_count: 1,
        detection_log: vec![],
        reference_matches: vec![],
    }];

    let detections = collect_top_level_license_detections_for_mode(&files, preloaded, false, true);

    assert!(detections.is_empty());
}

#[test]
fn from_json_recomputes_top_level_outputs_after_manifest_reference_following() {
    let file0 = json_file("project/Cargo.toml", crate::models::FileType::File);
    let file1 = json_file("project/LICENSE", crate::models::FileType::File);
    let mut files = vec![file0, file1];

    files[0].package_data = vec![crate::models::PackageData {
        package_type: Some(crate::models::PackageType::Cargo),
        datasource_id: Some(crate::models::DatasourceId::CargoToml),
        name: Some("demo".to_string()),
        version: Some("1.0.0".to_string()),
        ..Default::default()
    }];
    let mut package = crate::models::Package::from_package_data(
        &files[0].package_data[0],
        "project/Cargo.toml".to_string(),
    );
    let package_uid = package.package_uid.clone();
    files[0].for_packages = vec![package_uid.clone()];
    files[0].license_detections = vec![crate::models::LicenseDetection {
        license_expression: "unknown-license-reference".to_string(),
        license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
        matches: vec![crate::models::Match {
            license_expression: "unknown-license-reference".to_string(),
            license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
            from_file: Some("project/Cargo.toml".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(2),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some(
                "unknown-license-reference_see_license_at_manifest_1.RULE".to_string(),
            ),
            rule_url: None,
            matched_text: Some("See LICENSE".to_string()),
            referenced_filenames: Some(vec!["LICENSE".to_string()]),
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: None,
    }];
    files[1].license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![crate::models::Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/LICENSE".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(10).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(50),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("mit.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("mit-license".to_string()),
    }];

    for file in &mut files {
        file.backfill_license_provenance();
    }
    package.backfill_license_provenance();

    let mut packages = vec![package];
    apply_package_reference_following(&mut files, &mut packages);

    assert_eq!(
        packages[0].declared_license_expression.as_deref(),
        Some("mit")
    );

    let top_level = collect_top_level_license_detections(&files);
    assert!(
        top_level
            .iter()
            .any(|detection| detection.license_expression == "mit")
    );

    let engine = LicenseDetectionEngine::from_embedded().expect("embedded engine should load");
    let (license_references, license_rule_references) = collect_top_level_license_references(
        &files,
        &packages,
        engine.index(),
        DEFAULT_LICENSEDB_URL_TEMPLATE,
    );
    assert!(
        license_references
            .iter()
            .any(|reference| reference.key.as_deref() == Some("mit"))
    );
    assert!(license_rule_references.iter().any(|rule| {
        rule.identifier == "unknown-license-reference_see_license_at_manifest_1.RULE"
    }));
}

#[test]
fn from_json_recomputes_top_level_outputs_after_package_inheritance_following() {
    let file0 = json_file(
        "venv/lib/python3.11/site-packages/demo-1.0.dist-info/METADATA",
        crate::models::FileType::File,
    );
    let file1 = json_file(
        "venv/lib/python3.11/site-packages/locale/django.po",
        crate::models::FileType::File,
    );
    let mut files = vec![file0, file1];

    files[0].package_data = vec![crate::models::PackageData {
        package_type: Some(crate::models::PackageType::Pypi),
        datasource_id: Some(crate::models::DatasourceId::PypiWheelMetadata),
        name: Some("demo".to_string()),
        version: Some("1.0.0".to_string()),
        ..Default::default()
    }];
    files[0].license_detections = vec![crate::models::LicenseDetection {
        license_expression: "bsd-new".to_string(),
        license_expression_spdx: "BSD-3-Clause".to_string(),
        matches: vec![crate::models::Match {
            license_expression: "bsd-new".to_string(),
            license_expression_spdx: "BSD-3-Clause".to_string(),
            from_file: Some(
                "venv/lib/python3.11/site-packages/demo-1.0.dist-info/METADATA".to_string(),
            ),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(1),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("bsd-new_195.RULE".to_string()),
            rule_url: None,
            matched_text: Some("BSD-3-Clause".to_string()),
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: None,
    }];
    let mut package = crate::models::Package::from_package_data(
        &files[0].package_data[0],
        "venv/lib/python3.11/site-packages/demo-1.0.dist-info/METADATA".to_string(),
    );
    let package_uid = package.package_uid.clone();
    files[0].for_packages = vec![package_uid.clone()];
    files[1].for_packages = vec![package_uid.clone()];
    files[1].license_detections = vec![crate::models::LicenseDetection {
        license_expression: "free-unknown".to_string(),
        license_expression_spdx: "LicenseRef-scancode-free-unknown".to_string(),
        matches: vec![crate::models::Match {
            license_expression: "free-unknown".to_string(),
            license_expression_spdx: "LicenseRef-scancode-free-unknown".to_string(),
            from_file: Some("venv/lib/python3.11/site-packages/locale/django.po".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(11),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("free-unknown-package_1.RULE".to_string()),
            rule_url: None,
            matched_text: Some("same license as package".to_string()),
            referenced_filenames: Some(vec!["INHERIT_LICENSE_FROM_PACKAGE".to_string()]),
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: None,
    }];

    for file in &mut files {
        file.backfill_license_provenance();
    }
    package.backfill_license_provenance();

    let mut packages = vec![package];
    apply_package_reference_following(&mut files, &mut packages);

    assert_eq!(
        packages[0].declared_license_expression.as_deref(),
        Some("bsd-new")
    );
    assert_eq!(
        files[1].license_detections[0].detection_log,
        vec!["unknown-reference-in-file-to-package"]
    );

    let top_level = collect_top_level_license_detections(&files);
    let bsd_new_detections = top_level
        .iter()
        .filter(|detection| detection.license_expression == "bsd-new")
        .collect::<Vec<_>>();
    assert_eq!(bsd_new_detections.len(), 2);
    assert!(
        bsd_new_detections
            .iter()
            .all(|detection| detection.detection_count == 1)
    );
    assert!(
        bsd_new_detections
            .iter()
            .any(|detection| detection.detection_log.is_empty())
    );
    assert!(
        bsd_new_detections.iter().any(|detection| {
            detection.detection_log == ["unknown-reference-in-file-to-package"]
        })
    );

    let engine = LicenseDetectionEngine::from_embedded().expect("embedded engine should load");
    let (license_references, license_rule_references) = collect_top_level_license_references(
        &files,
        &packages,
        engine.index(),
        DEFAULT_LICENSEDB_URL_TEMPLATE,
    );
    assert!(
        license_references
            .iter()
            .any(|reference| { reference.key.as_deref() == Some("bsd-new") })
    );
    assert!(
        license_rule_references
            .iter()
            .any(|rule| rule.identifier == "free-unknown-package_1.RULE")
    );
}

#[test]
fn from_json_keeps_multi_datafile_package_license_provenance_on_manifest_package() {
    let file0 = json_file("project/package-lock.json", crate::models::FileType::File);
    let file1 = json_file("project/package.json", crate::models::FileType::File);
    let mut files = vec![file0, file1];

    files[0].package_data = vec![crate::models::PackageData {
        package_type: Some(crate::models::PackageType::Npm),
        datasource_id: Some(crate::models::DatasourceId::NpmPackageLockJson),
        name: Some("phoenix".to_string()),
        version: Some("1.8.5".to_string()),
        ..Default::default()
    }];
    files[0].license_detections = vec![crate::models::LicenseDetection {
        license_expression: "apache-2.0".to_string(),
        license_expression_spdx: "Apache-2.0".to_string(),
        matches: vec![crate::models::Match {
            license_expression: "apache-2.0".to_string(),
            license_expression_spdx: "Apache-2.0".to_string(),
            from_file: Some("project/package-lock.json".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(10),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("apache-2.0_65.RULE".to_string()),
            rule_url: None,
            matched_text: Some("Apache-2.0".to_string()),
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: None,
    }];

    files[1].package_data = vec![crate::models::PackageData {
        package_type: Some(crate::models::PackageType::Npm),
        datasource_id: Some(crate::models::DatasourceId::NpmPackageJson),
        name: Some("phoenix".to_string()),
        version: Some("1.8.5".to_string()),
        declared_license_expression: Some("mit".to_string()),
        declared_license_expression_spdx: Some("MIT".to_string()),
        license_detections: vec![crate::models::LicenseDetection {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            matches: vec![crate::models::Match {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                from_file: Some("project/package.json".to_string()),
                start_line: LineNumber::ONE,
                end_line: LineNumber::ONE,
                matcher: Some("2-aho".to_string()),
                score: MatchScore::MAX,
                matched_length: Some(3),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: Some("mit_30.RULE".to_string()),
                rule_url: None,
                matched_text: Some("MIT".to_string()),
                referenced_filenames: None,
                matched_text_diagnostics: None,
            }],
            detection_log: vec![],
            identifier: None,
        }],
        ..Default::default()
    }];

    let mut package = crate::models::Package::from_package_data(
        &files[1].package_data[0],
        "project/package.json".to_string(),
    );
    package.datafile_paths = vec![
        "project/package-lock.json".to_string(),
        "project/package.json".to_string(),
    ];
    let package_uid = package.package_uid.clone();
    files[0].for_packages = vec![package_uid.clone()];
    files[1].for_packages = vec![package_uid];

    for file in &mut files {
        file.backfill_license_provenance();
    }
    package.backfill_license_provenance();

    let mut packages = vec![package];
    apply_package_reference_following(&mut files, &mut packages);

    assert_eq!(
        packages[0].declared_license_expression.as_deref(),
        Some("mit")
    );
    assert_eq!(
        packages[0].declared_license_expression_spdx.as_deref(),
        Some("MIT")
    );
    assert_eq!(packages[0].license_detections.len(), 1);
    assert_eq!(
        packages[0].license_detections[0].license_expression_spdx,
        "MIT"
    );
}

fn json_file(path: &str, file_type: crate::models::FileType) -> crate::models::FileInfo {
    crate::models::FileInfo::new(
        Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string(),
        Path::new(path)
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string(),
        Path::new(path)
            .extension()
            .and_then(|name| name.to_str())
            .map(|ext| format!(".{ext}"))
            .unwrap_or_default(),
        path.to_string(),
        file_type,
        None,
        None,
        0,
        None,
        None,
        None,
        None,
        None,
        Vec::new(),
        None,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
}

#[test]
fn progress_mode_from_cli_maps_quiet_verbose_default() {
    let default_cli =
        crate::cli::Cli::try_parse_from(["provenant", "--json-pp", "scan.json", "sample-dir"])
            .unwrap();
    assert_eq!(
        progress_mode_from_cli(&default_cli),
        crate::progress::ProgressMode::Default
    );

    let quiet_cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--quiet",
        "sample-dir",
    ])
    .unwrap();
    assert_eq!(
        progress_mode_from_cli(&quiet_cli),
        crate::progress::ProgressMode::Quiet
    );

    let verbose_cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--verbose",
        "sample-dir",
    ])
    .unwrap();
    assert_eq!(
        progress_mode_from_cli(&verbose_cli),
        crate::progress::ProgressMode::Verbose
    );
}

#[test]
fn prepare_cache_for_scan_defaults_to_scan_root_cache_directory_without_creating_dirs() {
    let temp_dir = tempfile::TempDir::new().expect("create temp dir");
    let scan_root = temp_dir.path().join("scan");
    fs::create_dir_all(&scan_root).expect("create scan root");

    let cli =
        crate::cli::Cli::try_parse_from(["provenant", "--json-pp", "scan.json", "sample-dir"])
            .unwrap();
    let config = prepare_cache_config(Some(&scan_root), &cli).unwrap();

    assert_eq!(config.root_dir(), CacheConfig::default_root_dir(&scan_root));
    assert!(!config.incremental_enabled());
}

#[test]
fn prepare_cache_for_scan_respects_cache_dir_and_cache_clear() {
    let temp_dir = tempfile::TempDir::new().expect("create temp dir");
    let scan_root = temp_dir.path().join("scan");
    fs::create_dir_all(&scan_root).expect("create scan root");

    let explicit_cache_dir = temp_dir.path().join("explicit-cache");
    fs::create_dir_all(explicit_cache_dir.join("incremental")).unwrap();
    let stale_file = explicit_cache_dir.join("incremental").join("stale.txt");
    fs::write(&stale_file, "old").unwrap();

    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--cache-dir",
        explicit_cache_dir.to_str().unwrap(),
        "--cache-clear",
        "sample-dir",
    ])
    .unwrap();
    let config = prepare_cache_config(Some(&scan_root), &cli).unwrap();

    assert_eq!(config.root_dir(), explicit_cache_dir);
    assert!(!stale_file.exists());
}

#[test]
fn prepare_cache_for_scan_creates_incremental_dir_when_enabled() {
    let temp_dir = tempfile::TempDir::new().expect("create temp dir");
    let scan_root = temp_dir.path().join("scan");
    fs::create_dir_all(&scan_root).expect("create scan root");

    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--incremental",
        "sample-dir",
    ])
    .unwrap();
    let config = prepare_cache_config(Some(&scan_root), &cli).unwrap();

    assert!(config.incremental_enabled());
    assert!(config.incremental_dir().exists());
}

#[test]
fn prepare_cache_config_without_scan_root_uses_non_scan_default() {
    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--from-json",
        "sample-scan.json",
    ])
    .unwrap();

    let config = prepare_cache_config(None, &cli).unwrap();

    assert_eq!(
        config.root_dir(),
        CacheConfig::default_root_dir_without_scan_root()
    );
    assert!(!config.incremental_enabled());
}

#[test]
fn build_collection_exclude_patterns_skips_default_cache_dir() {
    let temp_dir = tempfile::TempDir::new().expect("create temp dir");
    let scan_root = temp_dir.path().join("scan");
    fs::create_dir_all(scan_root.join("src")).unwrap();
    fs::create_dir_all(scan_root.join(DEFAULT_CACHE_DIR_NAME).join("incremental")).unwrap();
    fs::write(scan_root.join("src").join("main.rs"), "fn main() {}").unwrap();
    fs::write(
        scan_root
            .join(DEFAULT_CACHE_DIR_NAME)
            .join("incremental")
            .join("stale.txt"),
        "cached",
    )
    .unwrap();

    let config = CacheConfig::from_scan_root(&scan_root);
    let exclude_patterns = build_collection_exclude_patterns(&scan_root, config.root_dir());
    let collected = collect_paths(&scan_root, 0, &exclude_patterns);

    assert!(
        collected
            .files
            .iter()
            .all(|(path, _)| !path.starts_with(config.root_dir()))
    );
    assert!(collected.excluded_count >= 1);
}

#[test]
fn build_collection_exclude_patterns_skips_explicit_in_tree_cache_dir() {
    let temp_dir = tempfile::TempDir::new().expect("create temp dir");
    let scan_root = temp_dir.path().join("scan");
    let explicit_cache_dir = scan_root.join("custom-cache");
    fs::create_dir_all(scan_root.join("docs")).unwrap();
    fs::create_dir_all(explicit_cache_dir.join("incremental")).unwrap();
    fs::write(scan_root.join("docs").join("README.md"), "hello").unwrap();
    fs::write(
        explicit_cache_dir.join("incremental").join("manifest.json"),
        "cached",
    )
    .unwrap();

    let config = CacheConfig::new(explicit_cache_dir.clone());
    let exclude_patterns = build_collection_exclude_patterns(&scan_root, config.root_dir());
    let collected = collect_paths(&scan_root, 0, &exclude_patterns);

    assert!(
        collected
            .files
            .iter()
            .all(|(path, _)| !path.starts_with(&explicit_cache_dir))
    );
    assert!(collected.excluded_count >= 1);
}

#[test]
fn build_collection_exclude_patterns_skips_license_index_files_under_cache_root() {
    let temp_dir = tempfile::TempDir::new().expect("create temp dir");
    let scan_root = temp_dir.path().join("scan");
    let explicit_cache_dir = scan_root.join("custom-cache");
    fs::create_dir_all(scan_root.join("docs")).unwrap();
    fs::create_dir_all(explicit_cache_dir.join("license-index").join("embedded")).unwrap();
    fs::write(scan_root.join("docs").join("README.md"), "hello").unwrap();
    fs::write(
        explicit_cache_dir
            .join("license-index")
            .join("embedded")
            .join("cache.rkyv"),
        "cached",
    )
    .unwrap();

    let config = CacheConfig::new(explicit_cache_dir.clone());
    let exclude_patterns = build_collection_exclude_patterns(&scan_root, config.root_dir());
    let collected = collect_paths(&scan_root, 0, &exclude_patterns);

    assert!(
        collected
            .files
            .iter()
            .all(|(path, _)| !path.starts_with(&explicit_cache_dir))
    );
    assert!(collected.excluded_count >= 1);
}

#[test]
fn build_collection_exclude_patterns_does_not_exclude_scan_root_when_cache_root_matches_it() {
    let temp_dir = tempfile::TempDir::new().expect("create temp dir");
    let scan_root = temp_dir.path().join("scan");
    fs::create_dir_all(scan_root.join("src")).unwrap();
    fs::write(scan_root.join("src").join("main.rs"), "fn main() {}").unwrap();

    let config = CacheConfig::new(scan_root.clone());
    let exclude_patterns = build_collection_exclude_patterns(&scan_root, config.root_dir());
    let collected = collect_paths(&scan_root, 0, &exclude_patterns);

    assert_eq!(collected.file_count(), 1);
    assert_eq!(collected.excluded_count, 0);
}

#[test]
fn build_collection_exclude_patterns_skips_vcs_metadata_directories() {
    let temp_dir = tempfile::TempDir::new().expect("create temp dir");
    let scan_root = temp_dir.path().join("scan");
    fs::create_dir_all(scan_root.join("src")).unwrap();
    fs::create_dir_all(scan_root.join(".git")).unwrap();
    fs::write(scan_root.join("src").join("main.rs"), "fn main() {}\n").unwrap();
    fs::write(scan_root.join(".git").join("index"), b"git index contents").unwrap();
    fs::write(scan_root.join(".gitignore"), "target/\n").unwrap();
    fs::create_dir_all(scan_root.join("nested")).unwrap();
    fs::write(scan_root.join("nested").join(".gitignore"), "*.log\n").unwrap();

    let config = CacheConfig::from_scan_root(&scan_root);
    let exclude_patterns = build_collection_exclude_patterns(&scan_root, config.root_dir());
    let collected = collect_paths(&scan_root, 0, &exclude_patterns);

    assert!(
        collected
            .files
            .iter()
            .all(|(path, _)| !path.starts_with(scan_root.join(".git")))
    );
    assert!(
        collected
            .files
            .iter()
            .all(|(path, _)| path.file_name().and_then(|name| name.to_str()) != Some(".gitignore"))
    );
    assert_eq!(collected.file_count(), 1);
    assert!(collected.excluded_count >= 3);
}

#[test]
fn resolve_native_scan_selection_uses_paths_file_under_explicit_root() {
    let temp_dir = tempfile::TempDir::new().expect("create temp dir");
    let scan_root = temp_dir.path().join("repo");
    fs::create_dir_all(scan_root.join("src")).expect("create src dir");
    fs::create_dir_all(scan_root.join("docs")).expect("create docs dir");
    fs::write(scan_root.join("src/lib.rs"), "pub fn demo() {}\n").expect("write lib");
    fs::write(scan_root.join("docs/guide.md"), "# guide\n").expect("write guide");

    let paths_file_a = temp_dir.path().join("changed-a.txt");
    let paths_file_b = temp_dir.path().join("changed-b.txt");
    fs::write(&paths_file_a, "src/lib.rs\r\nmissing.rs\n").expect("write first paths file");
    fs::write(&paths_file_b, "docs\nsrc/lib.rs\n").expect("write second paths file");

    let other_cwd = tempfile::TempDir::new().expect("create alternate cwd");
    let old_cwd = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(other_cwd.path()).expect("set cwd");

    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--paths-file",
        paths_file_a.to_str().expect("utf-8 path"),
        "--paths-file",
        paths_file_b.to_str().expect("utf-8 path"),
        scan_root.to_str().expect("utf-8 path"),
    ])
    .expect("cli parse should succeed");

    let result = resolve_native_scan_selection(&cli);

    std::env::set_current_dir(old_cwd).expect("restore cwd");

    let NativeScanSelection {
        scan_path: resolved_root,
        selected_paths: includes,
        collection_frontier: frontier,
        missing_entries,
    } = result.expect("paths file selection should resolve");
    assert_eq!(resolved_root, scan_root.to_str().expect("utf-8 path"));
    assert_eq!(
        includes,
        vec![
            crate::scan_result_shaping::SelectedPath::Exact("src/lib.rs".to_string()),
            crate::scan_result_shaping::SelectedPath::Subtree("docs".to_string())
        ]
    );
    assert_eq!(
        frontier,
        vec![
            crate::scanner::CollectionFrontier {
                path: std::path::PathBuf::from("src/lib.rs"),
                recurse: false,
            },
            crate::scanner::CollectionFrontier {
                path: std::path::PathBuf::from("docs"),
                recurse: true,
            }
        ]
    );
    assert_eq!(missing_entries, vec!["missing.rs"]);
}

#[test]
fn resolve_native_scan_selection_errors_when_paths_file_keeps_no_existing_entries() {
    let temp_dir = tempfile::TempDir::new().expect("create temp dir");
    let scan_root = temp_dir.path().join("repo");
    fs::create_dir_all(&scan_root).expect("create scan root");
    let paths_file = temp_dir.path().join("changed.txt");
    fs::write(&paths_file, "missing.rs\n").expect("write paths file");

    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--paths-file",
        paths_file.to_str().expect("utf-8 path"),
        scan_root.to_str().expect("utf-8 path"),
    ])
    .expect("cli parse should succeed");

    let error = resolve_native_scan_selection(&cli).expect_err("selection should fail");
    assert!(
        error
            .to_string()
            .contains("did not resolve to any existing files or directories")
    );
}

#[test]
fn build_paths_file_warning_messages_formats_missing_entries_for_headers() {
    let warnings =
        build_paths_file_warning_messages(&["missing.rs".to_string(), "docs/guide.md".to_string()]);

    assert_eq!(
        warnings,
        vec![
            "Skipping missing --paths-file entry: missing.rs".to_string(),
            "Skipping missing --paths-file entry: docs/guide.md".to_string(),
        ]
    );
}
