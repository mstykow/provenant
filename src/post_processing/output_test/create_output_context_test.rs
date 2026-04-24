// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn create_output_preserves_top_level_license_references_from_context() {
    let start = Utc::now();
    let end = start;
    let output = create_output(
        start,
        end,
        crate::scanner::ProcessResult {
            files: vec![dir("project")],
            excluded_count: 0,
        },
        CreateOutputContext {
            total_dirs: 1,
            assembly_result: assembly::AssemblyResult {
                packages: vec![],
                dependencies: vec![],
            },
            license_detections: vec![],
            license_references: vec![crate::models::LicenseReference {
                key: Some("mit".to_string()),
                language: Some("en".to_string()),
                name: "MIT License".to_string(),
                short_name: "MIT".to_string(),
                owner: Some("Example Owner".to_string()),
                homepage_url: Some("https://example.com/license".to_string()),
                spdx_license_key: "MIT".to_string(),
                other_spdx_license_keys: vec![],
                osi_license_key: Some("MIT".to_string()),
                text_urls: vec!["https://example.com/license.txt".to_string()],
                osi_url: Some("https://opensource.org/licenses/MIT".to_string()),
                faq_url: Some("https://example.com/faq".to_string()),
                other_urls: vec!["https://example.com/other".to_string()],
                category: None,
                is_exception: false,
                is_unknown: false,
                is_generic: false,
                notes: None,
                minimum_coverage: None,
                standard_notice: None,
                ignorable_copyrights: vec![],
                ignorable_holders: vec![],
                ignorable_authors: vec![],
                ignorable_urls: vec![],
                ignorable_emails: vec![],
                scancode_url: None,
                licensedb_url: None,
                spdx_url: None,
                text: "MIT text".to_string(),
            }],
            license_rule_references: vec![crate::models::LicenseRuleReference {
                identifier: "mit_1.RULE".to_string(),
                license_expression: "mit".to_string(),
                is_license_text: true,
                is_license_notice: false,
                is_license_reference: false,
                is_license_tag: false,
                is_license_clue: false,
                is_license_intro: false,
                language: None,
                rule_url: None,
                is_required_phrase: false,
                skip_for_required_phrase_generation: false,
                replaced_by: vec![],
                is_continuous: false,
                is_synthetic: false,
                is_from_license: false,
                length: 0,
                relevance: None,
                minimum_coverage: None,
                referenced_filenames: vec![],
                notes: None,
                ignorable_copyrights: vec![],
                ignorable_holders: vec![],
                ignorable_authors: vec![],
                ignorable_urls: vec![],
                ignorable_emails: vec![],
                text: None,
            }],
            spdx_license_list_version: "3.27".to_string(),
            license_index_provenance: None,
            extra_errors: vec![],
            extra_warnings: vec![],
            header_options: serde_json::Map::new(),
            options: CreateOutputOptions {
                facet_rules: &[],
                include_classify: false,
                include_tallies_by_facet: false,
                include_summary: false,
                include_license_clarity_score: false,
                include_tallies: false,
                include_tallies_with_details: false,
                include_tallies_of_key_files: false,
                include_generated: false,
                verbose: false,
            },
        },
    );

    assert_eq!(output.license_references.len(), 1);
    assert_eq!(output.license_rule_references.len(), 1);
    assert_eq!(output.license_references[0].spdx_license_key, "MIT");
    assert_eq!(output.license_rule_references[0].identifier, "mit_1.RULE");
}

#[test]
fn create_output_projects_file_scan_errors_into_headers_and_serialized_files() {
    let start = Utc::now();
    let end = start;
    let parse_error =
        "Failed to read or parse package.json at \"project/package.json\": expected value";

    let mut manifest = file("project/package.json");
    manifest.scan_errors = vec![parse_error.to_string()];

    let output = create_output(
        start,
        end,
        crate::scanner::ProcessResult {
            files: vec![dir("project"), manifest],
            excluded_count: 0,
        },
        CreateOutputContext {
            total_dirs: 1,
            assembly_result: assembly::AssemblyResult {
                packages: vec![],
                dependencies: vec![],
            },
            license_detections: vec![],
            license_references: vec![],
            license_rule_references: vec![],
            spdx_license_list_version: "3.27".to_string(),
            license_index_provenance: None,
            extra_errors: vec![],
            extra_warnings: vec![],
            header_options: serde_json::Map::new(),
            options: CreateOutputOptions {
                facet_rules: &[],
                include_classify: false,
                include_tallies_by_facet: false,
                include_summary: false,
                include_license_clarity_score: false,
                include_tallies: false,
                include_tallies_with_details: false,
                include_tallies_of_key_files: false,
                include_generated: false,
                verbose: false,
            },
        },
    );

    assert_eq!(
        output.headers[0].errors,
        vec!["Failed to read or parse package.json: project/package.json".to_string()]
    );

    let serialized = serde_json::to_value(crate::output_schema::Output::from(&output))
        .expect("serialize output with scan errors");
    let serialized_manifest = serialized["files"]
        .as_array()
        .expect("files should serialize as an array")
        .iter()
        .find(|entry| entry["path"] == "project/package.json")
        .expect("serialized package.json entry should exist");

    assert_eq!(serialized_manifest["scan_errors"], json!([parse_error]));
}

#[test]
fn create_output_embeds_license_index_provenance_in_header_extra_data() {
    let start = Utc::now();
    let end = start;
    let output = create_output(
        start,
        end,
        crate::scanner::ProcessResult {
            files: vec![dir("project")],
            excluded_count: 0,
        },
        CreateOutputContext {
            total_dirs: 1,
            assembly_result: assembly::AssemblyResult {
                packages: vec![],
                dependencies: vec![],
            },
            license_detections: vec![],
            license_references: vec![],
            license_rule_references: vec![],
            spdx_license_list_version: "3.27".to_string(),
            license_index_provenance: Some(crate::models::LicenseIndexProvenance {
                source: "embedded-artifact".to_string(),
                dataset_fingerprint: "abc123".to_string(),
                ignored_rules: vec!["gpl-2.0_and-unknown-license-reference_1.RULE".to_string()],
                ignored_licenses: vec![],
                ignored_rules_due_to_licenses: vec![],
                added_rules: vec!["false-positive-example_1.RULE".to_string()],
                replaced_rules: vec![],
                added_licenses: vec![],
                replaced_licenses: vec![],
            }),
            extra_errors: vec![],
            extra_warnings: vec![],
            header_options: serde_json::Map::new(),
            options: CreateOutputOptions {
                facet_rules: &[],
                include_classify: false,
                include_tallies_by_facet: false,
                include_summary: false,
                include_license_clarity_score: false,
                include_tallies: false,
                include_tallies_with_details: false,
                include_tallies_of_key_files: false,
                include_generated: false,
                verbose: false,
            },
        },
    );

    let provenance = output.headers[0]
        .extra_data
        .license_index_provenance
        .as_ref()
        .expect("header should carry license index provenance");
    assert_eq!(provenance.source, "embedded-artifact");
    assert_eq!(provenance.dataset_fingerprint, "abc123");
    assert_eq!(
        provenance.ignored_rules,
        vec!["gpl-2.0_and-unknown-license-reference_1.RULE".to_string()]
    );
    assert_eq!(
        provenance.added_rules,
        vec!["false-positive-example_1.RULE".to_string()]
    );
}

#[test]
fn create_output_header_errors_summarize_errored_paths_by_default() {
    let start = Utc::now();
    let end = start;
    let first_error = "Failed to parse package.json at \"project/package.json\": expected value";
    let second_error = "Timeout before license scan (> 120.00s)";

    let mut manifest = file("project/package.json");
    manifest.scan_errors = vec![first_error.to_string(), second_error.to_string()];

    let output = create_output(
        start,
        end,
        crate::scanner::ProcessResult {
            files: vec![dir("project"), manifest],
            excluded_count: 0,
        },
        CreateOutputContext {
            total_dirs: 1,
            assembly_result: assembly::AssemblyResult {
                packages: vec![],
                dependencies: vec![],
            },
            license_detections: vec![],
            license_references: vec![],
            license_rule_references: vec![],
            spdx_license_list_version: "3.27".to_string(),
            license_index_provenance: None,
            extra_errors: vec![],
            extra_warnings: vec![],
            header_options: serde_json::Map::new(),
            options: CreateOutputOptions {
                facet_rules: &[],
                include_classify: false,
                include_tallies_by_facet: false,
                include_summary: false,
                include_license_clarity_score: false,
                include_tallies: false,
                include_tallies_with_details: false,
                include_tallies_of_key_files: false,
                include_generated: false,
                verbose: false,
            },
        },
    );

    assert_eq!(
        output.headers[0].errors,
        vec!["Timeout before license scan (> 120.00s): project/package.json".to_string()]
    );
}

#[test]
fn create_output_header_errors_expand_scan_error_details_in_verbose_mode() {
    let start = Utc::now();
    let end = start;
    let first_error = "Failed to parse package.json at \"project/package.json\": expected value";
    let second_error = "Timeout before license scan (> 120.00s)";

    let mut manifest = file("project/package.json");
    manifest.scan_errors = vec![first_error.to_string(), second_error.to_string()];

    let output = create_output(
        start,
        end,
        crate::scanner::ProcessResult {
            files: vec![dir("project"), manifest],
            excluded_count: 0,
        },
        CreateOutputContext {
            total_dirs: 1,
            assembly_result: assembly::AssemblyResult {
                packages: vec![],
                dependencies: vec![],
            },
            license_detections: vec![],
            license_references: vec![],
            license_rule_references: vec![],
            spdx_license_list_version: "3.27".to_string(),
            license_index_provenance: None,
            extra_errors: vec![],
            extra_warnings: vec![],
            header_options: serde_json::Map::new(),
            options: CreateOutputOptions {
                facet_rules: &[],
                include_classify: false,
                include_tallies_by_facet: false,
                include_summary: false,
                include_license_clarity_score: false,
                include_tallies: false,
                include_tallies_with_details: false,
                include_tallies_of_key_files: false,
                include_generated: false,
                verbose: true,
            },
        },
    );

    assert_eq!(
        output.headers[0].errors,
        vec![format!(
            "Timeout before license scan (> 120.00s): project/package.json\n  {first_error}\n  {second_error}"
        )]
    );
}

#[test]
fn create_output_preserves_extra_errors_in_header_summary() {
    let start = Utc::now();
    let end = start;

    let output = create_output(
        start,
        end,
        crate::scanner::ProcessResult {
            files: vec![dir("project")],
            excluded_count: 0,
        },
        CreateOutputContext {
            total_dirs: 1,
            assembly_result: assembly::AssemblyResult {
                packages: vec![],
                dependencies: vec![],
            },
            license_detections: vec![],
            license_references: vec![],
            license_rule_references: vec![],
            spdx_license_list_version: "3.27".to_string(),
            license_index_provenance: None,
            extra_errors: vec!["Failed to read directory: project/vendor".to_string()],
            extra_warnings: vec![],
            header_options: serde_json::Map::new(),
            options: CreateOutputOptions {
                facet_rules: &[],
                include_classify: false,
                include_tallies_by_facet: false,
                include_summary: false,
                include_license_clarity_score: false,
                include_tallies: false,
                include_tallies_with_details: false,
                include_tallies_of_key_files: false,
                include_generated: false,
                verbose: false,
            },
        },
    );

    assert_eq!(
        output.headers[0].errors,
        vec!["Failed to read directory: project/vendor".to_string()]
    );
}

#[test]
fn create_output_preserves_extra_warnings_in_header() {
    let start = Utc::now();
    let end = start;

    let output = create_output(
        start,
        end,
        crate::scanner::ProcessResult {
            files: vec![dir("project")],
            excluded_count: 0,
        },
        CreateOutputContext {
            total_dirs: 1,
            assembly_result: assembly::AssemblyResult {
                packages: vec![],
                dependencies: vec![],
            },
            license_detections: vec![],
            license_references: vec![],
            license_rule_references: vec![],
            spdx_license_list_version: "3.27".to_string(),
            license_index_provenance: None,
            extra_errors: vec![],
            extra_warnings: vec!["Imported warning".to_string()],
            header_options: serde_json::Map::new(),
            options: CreateOutputOptions {
                facet_rules: &[],
                include_classify: false,
                include_tallies_by_facet: false,
                include_summary: false,
                include_license_clarity_score: false,
                include_tallies: false,
                include_tallies_with_details: false,
                include_tallies_of_key_files: false,
                include_generated: false,
                verbose: false,
            },
        },
    );

    assert_eq!(
        output.headers[0].warnings,
        vec!["Imported warning".to_string()]
    );
}

#[test]
fn create_output_routes_warning_like_scan_errors_into_header_warnings() {
    let start = Utc::now();
    let end = start;

    let mut manifest = file("project/pom.xml");
    manifest.scan_errors = vec![
        "Maven property missing key compiler.version".to_string(),
        "Circular include detected: requirements.txt".to_string(),
    ];

    let output = create_output(
        start,
        end,
        crate::scanner::ProcessResult {
            files: vec![dir("project"), manifest],
            excluded_count: 0,
        },
        CreateOutputContext {
            total_dirs: 1,
            assembly_result: assembly::AssemblyResult {
                packages: vec![],
                dependencies: vec![],
            },
            license_detections: vec![],
            license_references: vec![],
            license_rule_references: vec![],
            spdx_license_list_version: "3.27".to_string(),
            license_index_provenance: None,
            extra_errors: vec![],
            extra_warnings: vec![],
            header_options: serde_json::Map::new(),
            options: CreateOutputOptions {
                facet_rules: &[],
                include_classify: false,
                include_tallies_by_facet: false,
                include_summary: false,
                include_license_clarity_score: false,
                include_tallies: false,
                include_tallies_with_details: false,
                include_tallies_of_key_files: false,
                include_generated: false,
                verbose: false,
            },
        },
    );

    assert!(output.headers[0].errors.is_empty());
    assert_eq!(
        output.headers[0].warnings,
        vec!["Maven property missing key compiler.version: project/pom.xml".to_string()]
    );
}

#[test]
fn create_output_routes_structured_warning_diagnostics_into_header_warnings() {
    let start = Utc::now();
    let end = start;

    let mut manifest = file("project/custom.txt");
    manifest.scan_errors = vec!["custom recoverable warning".to_string()];
    manifest.scan_diagnostics = vec![crate::models::ScanDiagnostic::warning(
        "custom recoverable warning",
    )];

    let output = create_output(
        start,
        end,
        crate::scanner::ProcessResult {
            files: vec![dir("project"), manifest],
            excluded_count: 0,
        },
        CreateOutputContext {
            total_dirs: 1,
            assembly_result: assembly::AssemblyResult {
                packages: vec![],
                dependencies: vec![],
            },
            license_detections: vec![],
            license_references: vec![],
            license_rule_references: vec![],
            spdx_license_list_version: "3.27".to_string(),
            license_index_provenance: None,
            extra_errors: vec![],
            extra_warnings: vec![],
            header_options: serde_json::Map::new(),
            options: CreateOutputOptions {
                facet_rules: &[],
                include_classify: false,
                include_tallies_by_facet: false,
                include_summary: false,
                include_license_clarity_score: false,
                include_tallies: false,
                include_tallies_with_details: false,
                include_tallies_of_key_files: false,
                include_generated: false,
                verbose: false,
            },
        },
    );

    assert!(output.headers[0].errors.is_empty());
    assert_eq!(
        output.headers[0].warnings,
        vec!["custom recoverable warning: project/custom.txt".to_string()]
    );
}

#[test]
fn create_output_deduplicates_header_summary_errors() {
    let start = Utc::now();
    let end = start;
    let parse_error =
        "Failed to read or parse package.json at \"project/package.json\": expected value";

    let mut manifest = file("project/package.json");
    manifest.scan_errors = vec![parse_error.to_string()];

    let output = create_output(
        start,
        end,
        crate::scanner::ProcessResult {
            files: vec![dir("project"), manifest],
            excluded_count: 0,
        },
        CreateOutputContext {
            total_dirs: 1,
            assembly_result: assembly::AssemblyResult {
                packages: vec![],
                dependencies: vec![],
            },
            license_detections: vec![],
            license_references: vec![],
            license_rule_references: vec![],
            spdx_license_list_version: "3.27".to_string(),
            license_index_provenance: None,
            extra_errors: vec![
                "Failed to read or parse package.json: project/package.json".to_string(),
            ],
            extra_warnings: vec![],
            header_options: serde_json::Map::new(),
            options: CreateOutputOptions {
                facet_rules: &[],
                include_classify: false,
                include_tallies_by_facet: false,
                include_summary: false,
                include_license_clarity_score: false,
                include_tallies: false,
                include_tallies_with_details: false,
                include_tallies_of_key_files: false,
                include_generated: false,
                verbose: false,
            },
        },
    );

    assert_eq!(
        output.headers[0].errors,
        vec!["Failed to read or parse package.json: project/package.json".to_string()]
    );
}

#[test]
fn create_output_preserves_top_level_license_detections_from_context() {
    let start = Utc::now();
    let end = start;
    let output = create_output(
        start,
        end,
        crate::scanner::ProcessResult {
            files: vec![dir("project")],
            excluded_count: 0,
        },
        CreateOutputContext {
            total_dirs: 1,
            assembly_result: assembly::AssemblyResult {
                packages: vec![],
                dependencies: vec![],
            },
            license_detections: vec![crate::models::TopLevelLicenseDetection {
                identifier: "mit-id".to_string(),
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                detection_count: 2,
                detection_log: vec![],
                reference_matches: vec![Match {
                    license_expression: "mit".to_string(),
                    license_expression_spdx: "MIT".to_string(),
                    from_file: Some("project/LICENSE".to_string()),
                    start_line: LineNumber::ONE,
                    end_line: LineNumber::new(20).unwrap(),
                    matcher: Some("1-hash".to_string()),
                    score: MatchScore::MAX,
                    matched_length: Some(20),
                    match_coverage: Some(100.0),
                    rule_relevance: Some(100),
                    rule_identifier: Some("mit.LICENSE".to_string()),
                    rule_url: None,
                    matched_text: None,
                    referenced_filenames: None,
                    matched_text_diagnostics: None,
                }],
            }],
            license_references: vec![],
            license_rule_references: vec![],
            spdx_license_list_version: "3.27".to_string(),
            license_index_provenance: None,
            extra_errors: vec![],
            extra_warnings: vec![],
            header_options: serde_json::Map::new(),
            options: CreateOutputOptions {
                facet_rules: &[],
                include_classify: false,
                include_tallies_by_facet: false,
                include_summary: false,
                include_license_clarity_score: false,
                include_tallies: false,
                include_tallies_with_details: false,
                include_tallies_of_key_files: false,
                include_generated: false,
                verbose: false,
            },
        },
    );

    assert_eq!(output.license_detections.len(), 1);
    assert_eq!(output.license_detections[0].identifier, "mit-id");
    assert_eq!(output.license_detections[0].detection_count, 2);
}
