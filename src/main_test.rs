use super::*;
use clap::Parser;
use serde_json::json;
use std::fs;
use std::path::Path;

use crate::cache::{CacheConfig, DEFAULT_CACHE_DIR_NAME, build_collection_exclude_patterns};
use crate::license_detection::LicenseDetectionEngine;
use crate::post_processing::{
    apply_package_reference_following, collect_top_level_license_detections,
    collect_top_level_license_references,
};
use crate::scan_result_shaping::json_input::{
    JsonScanInput, load_scan_from_json, normalize_loaded_json_scan,
};
use crate::scanner::collect_paths;

#[test]
fn resolve_thread_count_supports_reference_compat_values() {
    assert_eq!(resolve_thread_count(-1), 1);
    assert_eq!(resolve_thread_count(0), default_parallel_threads());
    assert_eq!(resolve_thread_count(4), 4);
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
fn validate_scan_option_compatibility_rejects_cache_flags_with_from_json() {
    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--from-json",
        "--cache",
        "scan-results",
        "sample-scan.json",
    ])
    .unwrap();

    let error = validate_scan_option_compatibility(&cli).unwrap_err();
    assert!(error.to_string().contains("Persistent cache options"));
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

    let preloaded = assembly::AssemblyResult {
        packages: parsed.packages,
        dependencies: parsed.dependencies,
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
fn from_json_skips_final_native_projection_block() {
    let mut loaded = JsonScanInput {
        files: vec![json_file(
            "/tmp/archive/root/src/main.rs",
            crate::models::FileType::File,
        )],
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
        normalize_paths(
            &mut loaded.files,
            cli.dir_path.first().expect("input path exists"),
            cli.strip_root,
            cli.full_root,
        );
    }

    assert_eq!(loaded.files[0].path, "tmp/archive/root/src/main.rs");
}

#[test]
fn from_json_loaded_manifest_detections_can_be_recomputed_into_top_level_uniques() {
    let mut loaded = JsonScanInput {
        files: vec![json_file(
            "project/package.json",
            crate::models::FileType::File,
        )],
        packages: vec![],
        dependencies: vec![],
        license_detections: vec![],
        license_references: vec![],
        license_rule_references: vec![],
        excluded_count: 0,
    };
    loaded.files[0].package_data = vec![crate::models::PackageData {
        package_type: Some(crate::models::PackageType::Npm),
        license_detections: vec![crate::models::LicenseDetection {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            matches: vec![crate::models::Match {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                from_file: None,
                start_line: 1,
                end_line: 1,
                matcher: Some("parser-declared-license".to_string()),
                score: 100.0,
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

    for file in &mut loaded.files {
        file.backfill_license_provenance();
    }

    let top_level = collect_top_level_license_detections(&loaded.files);

    assert_eq!(top_level.len(), 1);
    assert_eq!(top_level[0].license_expression, "mit");
    assert_eq!(
        top_level[0].reference_matches[0].from_file.as_deref(),
        Some("project/package.json")
    );
}

#[test]
fn from_json_recomputes_top_level_uniques_even_without_shaping_flags() {
    let mut loaded = JsonScanInput {
        files: vec![json_file(
            "project/package.json",
            crate::models::FileType::File,
        )],
        packages: vec![],
        dependencies: vec![],
        license_detections: vec![crate::models::TopLevelLicenseDetection {
            identifier: "stale-id".to_string(),
            license_expression: "stale-license".to_string(),
            license_expression_spdx: "LicenseRef-scancode-stale-license".to_string(),
            detection_count: 1,
            detection_log: vec![],
            reference_matches: vec![],
        }],
        license_references: vec![],
        license_rule_references: vec![],
        excluded_count: 0,
    };
    loaded.files[0].package_data = vec![crate::models::PackageData {
        package_type: Some(crate::models::PackageType::Npm),
        other_license_detections: vec![crate::models::LicenseDetection {
            license_expression: "gpl-2.0-only".to_string(),
            license_expression_spdx: "GPL-2.0-only".to_string(),
            matches: vec![crate::models::Match {
                license_expression: "gpl-2.0-only".to_string(),
                license_expression_spdx: "GPL-2.0-only".to_string(),
                from_file: None,
                start_line: 1,
                end_line: 1,
                matcher: Some("parser-declared-license".to_string()),
                score: 100.0,
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

    for file in &mut loaded.files {
        file.backfill_license_provenance();
    }

    let top_level = collect_top_level_license_detections(&loaded.files);

    assert_eq!(top_level.len(), 1);
    assert_eq!(top_level[0].license_expression, "gpl-2.0-only");
    assert_ne!(top_level[0].identifier, "stale-id");
}

#[test]
fn from_json_recomputes_top_level_outputs_after_manifest_reference_following() {
    let mut loaded = JsonScanInput {
        files: vec![
            json_file("project/Cargo.toml", crate::models::FileType::File),
            json_file("project/LICENSE", crate::models::FileType::File),
        ],
        packages: vec![],
        dependencies: vec![],
        license_detections: vec![],
        license_references: vec![],
        license_rule_references: vec![],
        excluded_count: 0,
    };

    loaded.files[0].package_data = vec![crate::models::PackageData {
        package_type: Some(crate::models::PackageType::Cargo),
        datasource_id: Some(crate::models::DatasourceId::CargoToml),
        name: Some("demo".to_string()),
        version: Some("1.0.0".to_string()),
        ..Default::default()
    }];
    let mut package = crate::models::Package::from_package_data(
        &loaded.files[0].package_data[0],
        "project/Cargo.toml".to_string(),
    );
    let package_uid = package.package_uid.clone();
    loaded.files[0].for_packages = vec![package_uid.clone()];
    loaded.files[0].license_detections = vec![crate::models::LicenseDetection {
        license_expression: "unknown-license-reference".to_string(),
        license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
        matches: vec![crate::models::Match {
            license_expression: "unknown-license-reference".to_string(),
            license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
            from_file: Some("project/Cargo.toml".to_string()),
            start_line: 1,
            end_line: 1,
            matcher: Some("2-aho".to_string()),
            score: 100.0,
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
    loaded.files[1].license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![crate::models::Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/LICENSE".to_string()),
            start_line: 1,
            end_line: 10,
            matcher: Some("1-hash".to_string()),
            score: 100.0,
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

    for file in &mut loaded.files {
        file.backfill_license_provenance();
    }
    package.backfill_license_provenance();

    let mut packages = vec![package];
    apply_package_reference_following(&mut loaded.files, &mut packages);

    assert_eq!(
        packages[0].declared_license_expression.as_deref(),
        Some("mit")
    );

    let top_level = collect_top_level_license_detections(&loaded.files);
    assert!(
        top_level
            .iter()
            .any(|detection| detection.license_expression == "mit")
    );

    let engine = LicenseDetectionEngine::from_embedded().expect("embedded engine should load");
    let (license_references, license_rule_references) =
        collect_top_level_license_references(&loaded.files, &packages, engine.index());
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
    let mut loaded = JsonScanInput {
        files: vec![
            json_file(
                "venv/lib/python3.11/site-packages/demo-1.0.dist-info/METADATA",
                crate::models::FileType::File,
            ),
            json_file(
                "venv/lib/python3.11/site-packages/locale/django.po",
                crate::models::FileType::File,
            ),
        ],
        packages: vec![],
        dependencies: vec![],
        license_detections: vec![],
        license_references: vec![],
        license_rule_references: vec![],
        excluded_count: 0,
    };

    loaded.files[0].package_data = vec![crate::models::PackageData {
        package_type: Some(crate::models::PackageType::Pypi),
        datasource_id: Some(crate::models::DatasourceId::PypiWheelMetadata),
        name: Some("demo".to_string()),
        version: Some("1.0.0".to_string()),
        ..Default::default()
    }];
    loaded.files[0].license_detections = vec![crate::models::LicenseDetection {
        license_expression: "bsd-new".to_string(),
        license_expression_spdx: "BSD-3-Clause".to_string(),
        matches: vec![crate::models::Match {
            license_expression: "bsd-new".to_string(),
            license_expression_spdx: "BSD-3-Clause".to_string(),
            from_file: Some(
                "venv/lib/python3.11/site-packages/demo-1.0.dist-info/METADATA".to_string(),
            ),
            start_line: 1,
            end_line: 1,
            matcher: Some("1-hash".to_string()),
            score: 100.0,
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
        &loaded.files[0].package_data[0],
        "venv/lib/python3.11/site-packages/demo-1.0.dist-info/METADATA".to_string(),
    );
    let package_uid = package.package_uid.clone();
    loaded.files[0].for_packages = vec![package_uid.clone()];
    loaded.files[1].for_packages = vec![package_uid.clone()];
    loaded.files[1].license_detections = vec![crate::models::LicenseDetection {
        license_expression: "free-unknown".to_string(),
        license_expression_spdx: "LicenseRef-scancode-free-unknown".to_string(),
        matches: vec![crate::models::Match {
            license_expression: "free-unknown".to_string(),
            license_expression_spdx: "LicenseRef-scancode-free-unknown".to_string(),
            from_file: Some("venv/lib/python3.11/site-packages/locale/django.po".to_string()),
            start_line: 1,
            end_line: 1,
            matcher: Some("2-aho".to_string()),
            score: 100.0,
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

    for file in &mut loaded.files {
        file.backfill_license_provenance();
    }
    package.backfill_license_provenance();

    let mut packages = vec![package];
    apply_package_reference_following(&mut loaded.files, &mut packages);

    assert_eq!(
        packages[0].declared_license_expression.as_deref(),
        Some("bsd-new")
    );
    assert_eq!(
        loaded.files[1].license_detections[0].detection_log,
        vec!["unknown-reference-in-file-to-package"]
    );

    let top_level = collect_top_level_license_detections(&loaded.files);
    assert!(top_level.iter().any(|detection| {
        detection.license_expression == "bsd-new" && detection.detection_count >= 2
    }));

    let engine = LicenseDetectionEngine::from_embedded().expect("embedded engine should load");
    let (license_references, license_rule_references) =
        collect_top_level_license_references(&loaded.files, &packages, engine.index());
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
    let config = prepare_cache_for_scan(scan_root.to_str().unwrap(), &cli).unwrap();

    assert_eq!(config.root_dir(), scan_root.join(DEFAULT_CACHE_DIR_NAME));
    assert!(!config.any_enabled());
    assert!(!config.root_dir().exists());
}

#[test]
fn prepare_cache_for_scan_respects_cache_dir_and_cache_clear() {
    let temp_dir = tempfile::TempDir::new().expect("create temp dir");
    let scan_root = temp_dir.path().join("scan");
    fs::create_dir_all(&scan_root).expect("create scan root");

    let explicit_cache_dir = temp_dir.path().join("explicit-cache");
    fs::create_dir_all(explicit_cache_dir.join("scan-results")).unwrap();
    let stale_file = explicit_cache_dir.join("scan-results").join("stale.txt");
    fs::write(&stale_file, "old").unwrap();

    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--cache",
        "scan-results",
        "--cache-dir",
        explicit_cache_dir.to_str().unwrap(),
        "--cache-clear",
        "sample-dir",
    ])
    .unwrap();
    let config = prepare_cache_for_scan(scan_root.to_str().unwrap(), &cli).unwrap();

    assert_eq!(config.root_dir(), explicit_cache_dir);
    assert!(!stale_file.exists());
    assert!(config.scan_results_dir().exists());
}

#[test]
fn prepare_cache_for_scan_creates_scan_results_dir_when_enabled() {
    let temp_dir = tempfile::TempDir::new().expect("create temp dir");
    let scan_root = temp_dir.path().join("scan");
    fs::create_dir_all(&scan_root).expect("create scan root");

    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--cache",
        "scan-results",
        "sample-dir",
    ])
    .unwrap();
    let config = prepare_cache_for_scan(scan_root.to_str().unwrap(), &cli).unwrap();

    assert!(config.scan_results_enabled());
    assert!(config.scan_results_dir().exists());
}

#[test]
fn build_collection_exclude_patterns_skips_default_cache_dir() {
    let temp_dir = tempfile::TempDir::new().expect("create temp dir");
    let scan_root = temp_dir.path().join("scan");
    fs::create_dir_all(scan_root.join("src")).unwrap();
    fs::create_dir_all(scan_root.join(DEFAULT_CACHE_DIR_NAME).join("scan-results")).unwrap();
    fs::write(scan_root.join("src").join("main.rs"), "fn main() {}").unwrap();
    fs::write(
        scan_root
            .join(DEFAULT_CACHE_DIR_NAME)
            .join("scan-results")
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
    fs::create_dir_all(explicit_cache_dir.join("scan-results")).unwrap();
    fs::write(scan_root.join("docs").join("README.md"), "hello").unwrap();
    fs::write(
        explicit_cache_dir
            .join("scan-results")
            .join("entry.msgpack.zst"),
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
