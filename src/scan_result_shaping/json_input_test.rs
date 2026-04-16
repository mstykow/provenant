use super::*;
use crate::models::MatchScore;
use crate::output_schema::{OutputMatch, OutputTopLevelLicenseDetection};
use crate::scan_result_shaping::test_fixtures::json_file;
use serde_json::json;
use std::fs;

fn output_json_file(path: &str, file_type: crate::models::FileType) -> OutputFileInfo {
    let internal = json_file(path, file_type);
    OutputFileInfo::from(&internal)
}

#[test]
fn load_scan_from_json_reads_files_and_metadata_sections() {
    let temp_path = std::env::temp_dir().join("provenant-from-json-test.json");
    let content = json!({
        "headers": [
            {
                "errors": ["Path: src/main.rs"],
                "warnings": ["Imported warning"]
            }
        ],
        "files": [
            {
                "name": "main.rs",
                "base_name": "main",
                "extension": ".rs",
                "path": "src/main.rs",
                "type": "file",
                "size": 10,
                "programming_language": "Rust"
            }
        ],
        "packages": [],
        "dependencies": [],
        "license_detections": [
            {
                "identifier": "mit-id",
                "license_expression": "mit",
                "license_expression_spdx": "MIT",
                "detection_count": 1,
                "reference_matches": [
                    {
                        "license_expression": "mit",
                        "license_expression_spdx": "MIT",
                        "from_file": "src/main.rs",
                        "start_line": 1,
                        "end_line": 1,
                        "score": 100.0,
                        "rule_url": null
                    }
                ]
            }
        ],
        "license_references": [
            {"name":"MIT","short_name":"MIT","spdx_license_key":"MIT","text":"..."}
        ],
        "license_rule_references": []
    });
    fs::write(&temp_path, content.to_string()).expect("write json fixture");

    let parsed = load_scan_from_json(temp_path.to_str().expect("utf-8 path"))
        .expect("from-json loading should succeed");

    assert_eq!(parsed.files.len(), 1);
    assert_eq!(parsed.files[0].path, "src/main.rs");
    assert_eq!(parsed.headers.len(), 1);
    assert_eq!(parsed.headers[0].errors, vec!["Path: src/main.rs"]);
    assert_eq!(parsed.headers[0].warnings, vec!["Imported warning"]);
    assert_eq!(parsed.license_detections.len(), 1);
    assert_eq!(parsed.license_references.len(), 1);

    let _ = fs::remove_file(temp_path);
}

#[test]
fn normalize_loaded_json_scan_applies_strip_root_per_loaded_input() {
    let mut loaded = JsonScanInput {
        headers: vec![JsonHeaderInput {
            errors: vec![
                "Failed to read or parse package.json: archive/root/src/main.rs".to_string(),
            ],
            warnings: vec![],
            extra_data: None,
        }],
        files: vec![
            output_json_file("archive/root", crate::models::FileType::Directory),
            output_json_file("archive/root/src/main.rs", crate::models::FileType::File),
        ],
        packages: vec![],
        dependencies: vec![],
        license_detections: vec![OutputTopLevelLicenseDetection {
            identifier: "mit-id".to_string(),
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            detection_count: 1,
            detection_log: vec![],
            reference_matches: vec![OutputMatch {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                from_file: Some("archive/root/src/main.rs".to_string()),
                start_line: 1,
                end_line: 1,
                matcher: None,
                score: MatchScore::MAX,
                matched_length: None,
                match_coverage: None,
                rule_relevance: None,
                rule_identifier: None,
                rule_url: None,
                matched_text: None,
                matched_text_diagnostics: None,
                referenced_filenames: None,
            }],
        }],
        license_references: vec![],
        license_rule_references: vec![],
        excluded_count: 0,
    };

    normalize_loaded_json_scan(&mut loaded, true, false);

    let paths: Vec<_> = loaded.files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(paths, vec!["root", "src/main.rs"]);
    assert_eq!(
        loaded.headers[0].errors,
        vec!["Failed to read or parse package.json: src/main.rs"]
    );
    assert_eq!(
        loaded.license_detections[0].reference_matches[0]
            .from_file
            .as_deref(),
        Some("src/main.rs")
    );
}

#[test]
fn normalize_loaded_json_scan_trims_full_root_display_without_absolutizing() {
    let mut loaded = JsonScanInput {
        headers: vec![JsonHeaderInput {
            errors: vec!["Path: /tmp/archive/root/src/main.rs".to_string()],
            warnings: vec![],
            extra_data: None,
        }],
        files: vec![output_json_file(
            "/tmp/archive/root/src/main.rs",
            crate::models::FileType::File,
        )],
        packages: vec![],
        dependencies: vec![],
        license_detections: vec![OutputTopLevelLicenseDetection {
            identifier: "mit-id".to_string(),
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            detection_count: 1,
            detection_log: vec![],
            reference_matches: vec![OutputMatch {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                from_file: Some("/tmp/archive/root/src/main.rs".to_string()),
                start_line: 1,
                end_line: 1,
                matcher: None,
                score: MatchScore::MAX,
                matched_length: None,
                match_coverage: None,
                rule_relevance: None,
                rule_identifier: None,
                rule_url: None,
                matched_text: None,
                matched_text_diagnostics: None,
                referenced_filenames: None,
            }],
        }],
        license_references: vec![],
        license_rule_references: vec![],
        excluded_count: 0,
    };

    normalize_loaded_json_scan(&mut loaded, false, true);

    assert_eq!(loaded.files[0].path, "tmp/archive/root/src/main.rs");
    assert_eq!(
        loaded.headers[0].errors,
        vec!["Path: tmp/archive/root/src/main.rs"]
    );
    assert_eq!(
        loaded.license_detections[0].reference_matches[0]
            .from_file
            .as_deref(),
        Some("tmp/archive/root/src/main.rs")
    );
}

#[test]
fn into_parts_preserves_imported_header_errors_as_extra_errors() {
    let loaded = JsonScanInput {
        headers: vec![JsonHeaderInput {
            errors: vec!["Failed to read directory: src/main.rs".to_string()],
            warnings: vec!["Imported warning".to_string()],
            extra_data: Some(JsonHeaderExtraDataInput {
                spdx_license_list_version: Some("3.27".to_string()),
                license_index_provenance: Some(crate::models::LicenseIndexProvenance {
                    source: "embedded-artifact".to_string(),
                    policy_path: "resources/license_detection/index_build_policy.toml".to_string(),
                    curation_fingerprint: "abc123".to_string(),
                    ignored_rules: vec!["rule.RULE".to_string()],
                    ignored_licenses: vec![],
                    ignored_rules_due_to_licenses: vec![],
                    added_rules: vec![],
                    replaced_rules: vec![],
                    added_licenses: vec![],
                    replaced_licenses: vec![],
                }),
            }),
        }],
        files: vec![output_json_file(
            "src/main.rs",
            crate::models::FileType::File,
        )],
        packages: vec![],
        dependencies: vec![],
        license_detections: vec![],
        license_references: vec![],
        license_rule_references: vec![],
        excluded_count: 0,
    };

    let (
        _process_result,
        _assembly_result,
        _dets,
        _refs,
        _rule_refs,
        extra_errors,
        imported_spdx_license_list_version,
        imported_license_index_provenance,
    ) = loaded.into_parts().expect("into_parts should succeed");

    assert_eq!(extra_errors, vec!["Failed to read directory: src/main.rs"]);
    assert_eq!(imported_spdx_license_list_version.as_deref(), Some("3.27"));
    assert_eq!(
        imported_license_index_provenance
            .as_ref()
            .map(|provenance| provenance.curation_fingerprint.as_str()),
        Some("abc123")
    );
}

#[test]
fn into_parts_drops_imported_warnings_and_file_summary_errors() {
    let loaded = JsonScanInput {
        headers: vec![JsonHeaderInput {
            errors: vec![
                "Failed to read or parse package.json: src/main.rs".to_string(),
                "Failed to read directory: src/vendor".to_string(),
            ],
            warnings: vec!["Imported warning".to_string()],
            extra_data: None,
        }],
        files: vec![{
            let mut file = output_json_file("src/main.rs", crate::models::FileType::File);
            file.scan_errors = vec!["Imported file failure detail".to_string()];
            file
        }],
        packages: vec![],
        dependencies: vec![],
        license_detections: vec![],
        license_references: vec![],
        license_rule_references: vec![],
        excluded_count: 0,
    };

    let (
        _process_result,
        _assembly_result,
        _dets,
        _refs,
        _rule_refs,
        extra_errors,
        imported_spdx_license_list_version,
        imported_license_index_provenance,
    ) = loaded.into_parts().expect("into_parts should succeed");

    assert_eq!(extra_errors, vec!["Failed to read directory: src/vendor"]);
    assert!(imported_spdx_license_list_version.is_none());
    assert!(imported_license_index_provenance.is_none());
}

#[test]
fn normalize_loaded_json_scan_rewrites_verbose_header_error_path_prefix() {
    let mut loaded = JsonScanInput {
        headers: vec![JsonHeaderInput {
            errors: vec![
                "Failed to parse package.json: /tmp/archive/root/src/main.rs\n  Failed to parse package.json".to_string(),
            ],
            warnings: vec![],
            extra_data: None,
        }],
        files: vec![output_json_file(
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

    normalize_loaded_json_scan(&mut loaded, false, true);

    assert_eq!(
        loaded.headers[0].errors,
        vec![
            "Failed to parse package.json: tmp/archive/root/src/main.rs\n  Failed to parse package.json"
        ]
    );
}

#[test]
fn into_parts_discards_conflicting_imported_header_provenance() {
    let loaded = JsonScanInput {
        headers: vec![
            JsonHeaderInput {
                errors: vec![],
                warnings: vec![],
                extra_data: Some(JsonHeaderExtraDataInput {
                    spdx_license_list_version: Some("3.27".to_string()),
                    license_index_provenance: Some(crate::models::LicenseIndexProvenance {
                        source: "embedded-artifact".to_string(),
                        policy_path: "resources/license_detection/index_build_policy.toml"
                            .to_string(),
                        curation_fingerprint: "one".to_string(),
                        ignored_rules: vec![],
                        ignored_licenses: vec![],
                        ignored_rules_due_to_licenses: vec![],
                        added_rules: vec![],
                        replaced_rules: vec![],
                        added_licenses: vec![],
                        replaced_licenses: vec![],
                    }),
                }),
            },
            JsonHeaderInput {
                errors: vec![],
                warnings: vec![],
                extra_data: Some(JsonHeaderExtraDataInput {
                    spdx_license_list_version: Some("3.28".to_string()),
                    license_index_provenance: Some(crate::models::LicenseIndexProvenance {
                        source: "custom-rules".to_string(),
                        policy_path: "resources/license_detection/index_build_policy.toml"
                            .to_string(),
                        curation_fingerprint: "two".to_string(),
                        ignored_rules: vec![],
                        ignored_licenses: vec![],
                        ignored_rules_due_to_licenses: vec![],
                        added_rules: vec![],
                        replaced_rules: vec![],
                        added_licenses: vec![],
                        replaced_licenses: vec![],
                    }),
                }),
            },
        ],
        files: vec![output_json_file(
            "src/main.rs",
            crate::models::FileType::File,
        )],
        packages: vec![],
        dependencies: vec![],
        license_detections: vec![],
        license_references: vec![],
        license_rule_references: vec![],
        excluded_count: 0,
    };

    let (
        _process_result,
        _assembly_result,
        _dets,
        _refs,
        _rule_refs,
        _extra_errors,
        imported_spdx_license_list_version,
        imported_license_index_provenance,
    ) = loaded.into_parts().expect("into_parts should succeed");

    assert!(imported_spdx_license_list_version.is_none());
    assert!(imported_license_index_provenance.is_none());
}
