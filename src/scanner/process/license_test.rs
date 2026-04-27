// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::{
    LicenseExtractionInput, MAX_OUTPUT_MATCHED_TEXT_BYTES, MAX_OUTPUT_MATCHED_TEXT_LINE_LENGTH,
    compute_percentage_of_license_text, convert_detection_to_model, extract_license_information,
    promote_legal_notice_low_quality_detections,
};
use crate::license_detection::LicenseDetection as InternalLicenseDetection;
use crate::license_detection::LicenseDetectionEngine;
use crate::license_detection::index::LicenseIndex;
use crate::license_detection::index::dictionary::TokenDictionary;
use crate::license_detection::models::License;
use crate::license_detection::models::position_span::PositionSpan;
use crate::license_detection::models::{LicenseMatch, MatchCoordinates, MatcherKind, RuleKind};
use crate::license_detection::query::Query;
use crate::models::{FileInfoBuilder, LineNumber, MatchScore, ScanDiagnostic};
use crate::scanner::LicenseScanOptions;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

static TEST_ENGINE: LazyLock<Arc<LicenseDetectionEngine>> = LazyLock::new(|| {
    Arc::new(LicenseDetectionEngine::from_test_index(create_test_index(
        &[
            ("mit", 0),
            ("license", 1),
            ("permission", 2),
            ("granted", 3),
        ],
        4,
    )))
});

fn make_internal_match(rule_url: &str) -> LicenseMatch {
    LicenseMatch {
        rid: 0,
        license_expression: "mit".to_string(),
        license_expression_spdx: Some("MIT".to_string()),
        from_file: None,
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
        start_token: 0,
        end_token: 1,
        matcher: MatcherKind::Hash,
        score: MatchScore::from_percentage(1.0),
        matched_length: 3,
        rule_length: 3,
        match_coverage: 100.0,
        rule_relevance: 100,
        rule_identifier: "mit.LICENSE".to_string(),
        rule_url: rule_url.to_string(),
        matched_text: Some("MIT".to_string()),
        referenced_filenames: None,
        rule_kind: RuleKind::Text,
        is_from_license: true,
        rule_start_token: 0,
        coordinates: MatchCoordinates::query_region(PositionSpan::empty()),
        candidate_resemblance: 0.0,
        candidate_containment: 0.0,
    }
}

fn make_detection(rule_url: &str) -> InternalLicenseDetection {
    InternalLicenseDetection {
        license_expression: Some("mit".to_string()),
        license_expression_spdx: Some("MIT".to_string()),
        matches: vec![make_internal_match(rule_url)],
        detection_log: vec![],
        identifier: Some("mit-test".to_string()),
        file_regions: Vec::new(),
    }
}

fn create_test_index(entries: &[(&str, u16)], len_legalese: usize) -> LicenseIndex {
    let dictionary = TokenDictionary::new_with_legalese_pairs(entries);
    let mut index = LicenseIndex::new(dictionary);
    index.len_legalese = len_legalese;
    index
}

fn make_internal_notice_match(
    expr: &str,
    expr_spdx: &str,
    start_line: usize,
    end_line: usize,
) -> LicenseMatch {
    LicenseMatch {
        rid: 0,
        license_expression: expr.to_string(),
        license_expression_spdx: Some(expr_spdx.to_string()),
        from_file: Some("NOTICE".to_string()),
        start_line: LineNumber::new(start_line).expect("valid start line"),
        end_line: LineNumber::new(end_line).expect("valid end line"),
        start_token: start_line,
        end_token: end_line + 1,
        matcher: MatcherKind::Seq,
        score: MatchScore::from_percentage(50.0),
        matched_length: 11,
        rule_length: 22,
        match_coverage: 50.0,
        rule_relevance: 100,
        rule_identifier: "apache-2.0_559.RULE".to_string(),
        rule_url: String::new(),
        matched_text: None,
        referenced_filenames: None,
        rule_kind: RuleKind::Text,
        is_from_license: false,
        rule_start_token: 0,
        coordinates: MatchCoordinates::query_region(PositionSpan::empty()),
        candidate_resemblance: 0.0,
        candidate_containment: 0.0,
    }
}

#[test]
fn test_convert_detection_to_model_preserves_rule_url() {
    let detection = make_detection(
        "https://github.com/aboutcode-org/scancode-toolkit/tree/develop/src/licensedcode/data/licenses/mit.LICENSE",
    );

    let (converted, clues) =
        convert_detection_to_model(&detection, LicenseScanOptions::default(), "", None, None);
    let converted = converted.expect("detection should convert");

    assert_eq!(
        converted.matches[0].rule_url.as_deref(),
        Some(
            "https://github.com/aboutcode-org/scancode-toolkit/tree/develop/src/licensedcode/data/licenses/mit.LICENSE"
        )
    );
    assert!(clues.is_empty());
}

#[test]
fn test_convert_detection_to_model_emits_null_for_empty_rule_url() {
    let detection = make_detection("");

    let (converted, clues) =
        convert_detection_to_model(&detection, LicenseScanOptions::default(), "", None, None);
    let converted = converted.expect("detection should convert");

    assert_eq!(converted.matches[0].rule_url, None);
    assert!(clues.is_empty());
}

#[test]
fn test_convert_detection_to_model_rounds_match_coverage() {
    let mut detection = make_detection("");
    detection.matches[0].score = MatchScore::from_percentage(81.82);
    detection.matches[0].match_coverage = 33.334;

    let (converted, clues) =
        convert_detection_to_model(&detection, LicenseScanOptions::default(), "", None, None);
    let converted = converted.expect("detection should convert");

    assert_eq!(
        converted.matches[0].score,
        MatchScore::from_percentage(81.82)
    );
    assert_eq!(converted.matches[0].match_coverage, Some(33.33));
    assert!(clues.is_empty());
}

#[test]
fn test_convert_detection_to_model_normalizes_redundant_outer_spdx_parentheses() {
    let mut detection = make_detection("");
    detection.license_expression = Some("mit OR cc0-1.0".to_string());
    detection.license_expression_spdx = Some("(MIT OR CC0-1.0)".to_string());
    detection.matches[0].license_expression = "mit OR cc0-1.0".to_string();
    detection.matches[0].license_expression_spdx = Some("(MIT OR CC0-1.0)".to_string());

    let (converted, clues) =
        convert_detection_to_model(&detection, LicenseScanOptions::default(), "", None, None);
    let converted = converted.expect("detection should convert");

    assert_eq!(converted.license_expression_spdx, "MIT OR CC0-1.0");
    assert_eq!(
        converted.matches[0].license_expression_spdx,
        "MIT OR CC0-1.0"
    );
    assert!(clues.is_empty());
}

#[test]
fn test_convert_detection_to_model_routes_expressionless_detection_to_license_clues() {
    let mut detection = make_detection(
        "https://github.com/aboutcode-org/scancode-toolkit/tree/develop/src/licensedcode/data/rules/license-clue_1.RULE",
    );
    detection.license_expression = None;
    detection.license_expression_spdx = None;
    detection.identifier = None;
    detection.matches[0].license_expression = "unknown-license-reference".to_string();
    detection.matches[0].license_expression_spdx =
        Some("LicenseRef-scancode-unknown-license-reference".to_string());
    detection.matches[0].rule_identifier = "license-clue_1.RULE".to_string();
    detection.matches[0].rule_kind = RuleKind::Clue;

    let (converted, clues) = convert_detection_to_model(
        &detection,
        LicenseScanOptions {
            include_text: true,
            min_score: 0,
            ..LicenseScanOptions::default()
        },
        "clue text",
        None,
        None,
    );

    assert!(converted.is_none());
    assert_eq!(clues.len(), 1);
    assert_eq!(clues[0].license_expression, "unknown-license-reference");
    assert_eq!(
        clues[0].license_expression_spdx,
        "LicenseRef-scancode-unknown-license-reference"
    );
    assert_eq!(
        clues[0].rule_identifier.as_deref(),
        Some("license-clue_1.RULE")
    );
    assert_eq!(clues[0].matched_text.as_deref(), Some("MIT"));
    assert_eq!(clues[0].matched_text_diagnostics, None);
}

#[test]
fn test_convert_detection_to_model_promotes_exact_reference_url_clue() {
    let mut index = create_test_index(&[], 0);
    index.licenses_by_key.insert(
        "cc-by-3.0".to_string(),
        License {
            key: "cc-by-3.0".to_string(),
            name: "CC BY 3.0".to_string(),
            reference_urls: vec!["http://creativecommons.org/licenses/by/3.0/".to_string()],
            ..License::default()
        },
    );
    index.licenses_by_key.insert(
        "cc-by-sa-3.0".to_string(),
        License {
            key: "cc-by-sa-3.0".to_string(),
            name: "CC BY-SA 3.0".to_string(),
            reference_urls: vec!["http://creativecommons.org/licenses/by-sa/3.0/".to_string()],
            ..License::default()
        },
    );

    let text = concat!(
        "<rights license=\"http://creativecommons.org/licenses/by-sa/3.0/\">",
        "This work is licensed under a Creative Commons Attribution-ShareAlike 3.0 License",
        "</rights>",
    );
    let query = Query::from_extracted_text(text, &index, false).expect("query should build");

    let mut weaker_match = make_internal_match("");
    weaker_match.license_expression = "cc-by-3.0".to_string();
    weaker_match.license_expression_spdx = Some("CC-BY-3.0".to_string());
    weaker_match.matcher = MatcherKind::Seq;
    weaker_match.score = MatchScore::from_percentage(52.94);
    weaker_match.matched_length = 9;
    weaker_match.rule_length = 9;
    weaker_match.match_coverage = 52.94;
    weaker_match.rule_identifier = "cc-by-3.0_7.RULE".to_string();
    weaker_match.matched_text = None;

    let mut stronger_match = make_internal_match("");
    stronger_match.license_expression = "cc-by-sa-3.0".to_string();
    stronger_match.license_expression_spdx = Some("CC-BY-SA-3.0".to_string());
    stronger_match.matcher = MatcherKind::Seq;
    stronger_match.score = MatchScore::from_percentage(50.0);
    stronger_match.matched_length = 8;
    stronger_match.rule_length = 8;
    stronger_match.match_coverage = 50.0;
    stronger_match.rule_identifier = "cc-by-sa-3.0_10.RULE".to_string();
    stronger_match.matched_text = None;

    let detection = InternalLicenseDetection {
        license_expression: None,
        license_expression_spdx: None,
        matches: vec![weaker_match, stronger_match],
        detection_log: vec![],
        identifier: None,
        file_regions: Vec::new(),
    };

    let (converted, clues) = convert_detection_to_model(
        &detection,
        LicenseScanOptions::default(),
        text,
        Some(&query),
        Some(&index),
    );

    let converted = converted.expect("detection should promote from exact reference URL");
    assert_eq!(converted.license_expression, "cc-by-sa-3.0");
    assert_eq!(converted.license_expression_spdx, "CC-BY-SA-3.0");
    assert_eq!(converted.matches.len(), 1);
    assert_eq!(converted.matches[0].license_expression, "cc-by-sa-3.0");
    assert!(clues.is_empty());
}

#[test]
fn test_supplement_nix_manifest_license_detections_adds_missing_singleton_symbol() {
    let detections = super::supplement_nix_manifest_license_detections(
        std::path::Path::new("package.nix"),
        "meta = {\n  license = lib.licenses.asl20;\n};\n",
        &[],
    );

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].license_expression_spdx, "Apache-2.0");
    assert_eq!(
        detections[0].matches[0].start_line,
        LineNumber::new(2).unwrap()
    );
}

#[test]
fn test_supplement_nix_manifest_license_detections_adds_only_missing_browser_symbol() {
    let existing = vec![crate::models::LicenseDetection {
        license_expression: "lgpl-2.1-plus AND lgpl-3.0-plus".to_string(),
        license_expression_spdx: "LGPL-2.1-or-later AND LGPL-3.0-or-later".to_string(),
        matches: vec![],
        detection_log: vec![],
        identifier: None,
    }];

    let detections = super::supplement_nix_manifest_license_detections(
        std::path::Path::new("package.nix"),
        "meta = {\n  license = with lib.licenses; [\n    mpl20\n    lgpl21Plus\n    lgpl3Plus\n    free\n  ];\n};\n",
        &existing,
    );

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].license_expression_spdx, "MPL-2.0");
    assert_eq!(
        detections[0].matches[0].start_line,
        LineNumber::new(3).unwrap()
    );
}

#[test]
fn test_promote_legal_notice_low_quality_detections_promotes_apache_notice_fragment() {
    let concrete = InternalLicenseDetection {
        license_expression: Some("cve-tou".to_string()),
        license_expression_spdx: Some("cve-tou".to_string()),
        matches: vec![make_internal_notice_match("cve-tou", "cve-tou", 39, 42)],
        detection_log: Vec::new(),
        identifier: None,
        file_regions: Vec::new(),
    };
    let low_quality = InternalLicenseDetection {
        license_expression: None,
        license_expression_spdx: None,
        matches: vec![make_internal_notice_match("apache-2.0", "Apache-2.0", 7, 8)],
        detection_log: vec!["low-quality-match-fragments".to_string()],
        identifier: None,
        file_regions: Vec::new(),
    };
    let mut detections = vec![concrete, low_quality];

    promote_legal_notice_low_quality_detections(&mut detections, std::path::Path::new("NOTICE"));

    assert_eq!(
        detections[1].license_expression.as_deref(),
        Some("apache-2.0")
    );
    assert_eq!(
        detections[1].license_expression_spdx.as_deref(),
        Some("Apache-2.0")
    );
    assert!(
        detections[1]
            .detection_log
            .contains(&"promoted-low-quality-legal-notice".to_string())
    );
}

#[test]
fn test_promote_legal_notice_low_quality_detections_ignores_non_legal_path() {
    let concrete = InternalLicenseDetection {
        license_expression: Some("cve-tou".to_string()),
        license_expression_spdx: Some("cve-tou".to_string()),
        matches: vec![make_internal_notice_match("cve-tou", "cve-tou", 39, 42)],
        detection_log: Vec::new(),
        identifier: None,
        file_regions: Vec::new(),
    };
    let low_quality = InternalLicenseDetection {
        license_expression: None,
        license_expression_spdx: None,
        matches: vec![make_internal_notice_match("apache-2.0", "Apache-2.0", 7, 8)],
        detection_log: vec!["low-quality-match-fragments".to_string()],
        identifier: None,
        file_regions: Vec::new(),
    };
    let mut detections = vec![concrete, low_quality];

    promote_legal_notice_low_quality_detections(&mut detections, std::path::Path::new("README.md"));

    assert!(detections[1].license_expression.is_none());
}

#[test]
fn test_promote_legal_notice_low_quality_detections_ignores_true_clue_rules() {
    let concrete = InternalLicenseDetection {
        license_expression: Some("cve-tou".to_string()),
        license_expression_spdx: Some("cve-tou".to_string()),
        matches: vec![make_internal_notice_match("cve-tou", "cve-tou", 39, 42)],
        detection_log: Vec::new(),
        identifier: None,
        file_regions: Vec::new(),
    };
    let mut clue_match = make_internal_notice_match("apache-2.0", "Apache-2.0", 7, 8);
    clue_match.rule_kind = RuleKind::Clue;
    let low_quality = InternalLicenseDetection {
        license_expression: None,
        license_expression_spdx: None,
        matches: vec![clue_match],
        detection_log: vec!["low-quality-match-fragments".to_string()],
        identifier: None,
        file_regions: Vec::new(),
    };
    let mut detections = vec![concrete, low_quality];

    promote_legal_notice_low_quality_detections(&mut detections, std::path::Path::new("NOTICE"));

    assert!(detections[1].license_expression.is_none());
}

#[test]
fn test_convert_detection_to_model_includes_diagnostics_when_enabled() {
    let text = concat!(
        "Reproduction and distribution of this file, with or without modification, are\n",
        "permitted in any medium without royalties provided the copyright notice\n",
        "and this notice are preserved. This file is offered as-is, without any warranties.\n",
    );
    let index = create_test_index(
        &[
            ("reproduction", 0),
            ("distribution", 1),
            ("file", 2),
            ("without", 3),
            ("modification", 4),
            ("permitted", 5),
            ("medium", 6),
            ("royalties", 7),
            ("provided", 8),
            ("copyright", 9),
            ("notice", 10),
            ("preserved", 11),
            ("offered", 12),
            ("warranties", 13),
        ],
        14,
    );
    let query = Query::from_extracted_text(text, &index, false).expect("query should build");
    let mut detection = make_detection(
        "https://github.com/aboutcode-org/scancode-toolkit/tree/develop/src/licensedcode/data/licenses/fsf-ap.LICENSE",
    );
    detection.detection_log = vec!["imperfect-match-coverage".to_string()];
    detection.matches[0].license_expression = "fsf-ap".to_string();
    detection.matches[0].license_expression_spdx = Some("FSFAP".to_string());
    detection.matches[0].rule_identifier = "fsf-ap.LICENSE".to_string();
    detection.matches[0].matched_text = None;
    detection.matches[0].start_line = LineNumber::ONE;
    detection.matches[0].end_line = LineNumber::new(3).unwrap();
    detection.matches[0].start_token = 0;
    detection.matches[0].end_token = query.tokens.len();
    detection.matches[0].coordinates =
        MatchCoordinates::query_region(PositionSpan::from_positions(
            query
                .tokens
                .iter()
                .enumerate()
                .filter_map(|(idx, _)| (idx != 9).then_some(idx))
                .collect::<Vec<_>>(),
        ));
    detection.identifier = Some("fsf_ap-test".to_string());

    let (converted, clues) = convert_detection_to_model(
        &detection,
        LicenseScanOptions {
            include_text: true,
            include_text_diagnostics: true,
            include_diagnostics: true,
            unknown_licenses: false,
            min_score: 0,
        },
        text,
        Some(&query),
        None,
    );
    let converted = converted.expect("detection should convert");

    assert!(clues.is_empty());
    assert_eq!(converted.detection_log, vec!["imperfect-match-coverage"]);
    assert_eq!(
        converted.matches[0].matched_text.as_deref(),
        Some(text.trim_end())
    );
    let diagnostics = converted.matches[0]
        .matched_text_diagnostics
        .as_deref()
        .expect("diagnostics should be present");
    assert!(diagnostics.contains('['));
    assert!(diagnostics.contains(']'));
    assert_ne!(diagnostics, text.trim_end());
}

#[test]
fn test_convert_detection_to_model_preserves_whole_line_matched_text_for_normal_files() {
    let text = "Header\nMIT License\nFooter";
    let index = create_test_index(&[("mit", 0), ("license", 1)], 2);
    let query = Query::from_extracted_text(text, &index, false).expect("query should build");
    let mut detection = make_detection("");
    detection.matches[0].matched_text = None;
    detection.matches[0].start_line = LineNumber::new(2).unwrap();
    detection.matches[0].end_line = LineNumber::new(2).unwrap();
    detection.matches[0].coordinates =
        MatchCoordinates::query_region(PositionSpan::from_positions(vec![0, 1]));

    let (converted, clues) = convert_detection_to_model(
        &detection,
        LicenseScanOptions {
            include_text: true,
            ..LicenseScanOptions::default()
        },
        text,
        Some(&query),
        None,
    );

    let converted = converted.expect("detection should convert");
    assert!(clues.is_empty());
    assert_eq!(
        converted.matches[0].matched_text.as_deref(),
        Some("MIT License")
    );
}

#[test]
fn test_convert_detection_to_model_compacts_oversized_long_line_matched_text() {
    let padding = "a".repeat(MAX_OUTPUT_MATCHED_TEXT_LINE_LENGTH + 128);
    let text = format!("{padding} MIT License {padding}");
    let index = create_test_index(&[("mit", 0), ("license", 1)], 2);
    let query = Query::from_extracted_text(&text, &index, false).expect("query should build");
    let mut detection = make_detection("");
    detection.matches[0].matched_text = None;
    detection.matches[0].coordinates =
        MatchCoordinates::query_region(PositionSpan::from_positions(vec![0, 1]));
    detection.matches[0].matched_length = 2;
    detection.matches[0].rule_length = 2;
    detection.matches[0].start_token = 0;
    detection.matches[0].end_token = 2;

    let (converted, clues) = convert_detection_to_model(
        &detection,
        LicenseScanOptions {
            include_text: true,
            ..LicenseScanOptions::default()
        },
        &text,
        Some(&query),
        None,
    );

    let converted = converted.expect("detection should convert");
    let matched_text = converted.matches[0]
        .matched_text
        .as_deref()
        .expect("matched_text should be present");

    assert!(clues.is_empty());
    assert!(matched_text.contains("MIT"));
    assert!(matched_text.contains("License"));
    assert!(matched_text.len() < text.len());
    assert!(matched_text.len() < MAX_OUTPUT_MATCHED_TEXT_LINE_LENGTH);
}

#[test]
fn test_convert_detection_to_model_truncates_output_only_when_query_missing() {
    let text = "ß".repeat(MAX_OUTPUT_MATCHED_TEXT_BYTES) + " MIT";
    let mut detection = make_detection("");
    detection.matches[0].matched_text = None;

    let (converted, clues) = convert_detection_to_model(
        &detection,
        LicenseScanOptions {
            include_text: true,
            ..LicenseScanOptions::default()
        },
        &text,
        None,
        None,
    );

    let converted = converted.expect("detection should convert");
    let matched_text = converted.matches[0]
        .matched_text
        .as_deref()
        .expect("matched_text should be present");

    assert!(clues.is_empty());
    assert!(matched_text.ends_with("… [truncated]"));
    assert!(matched_text.len() <= MAX_OUTPUT_MATCHED_TEXT_BYTES);
    assert!(matched_text.len() < text.len());
}

#[test]
fn test_compute_percentage_of_license_text_counts_unknown_tokens() {
    let index = create_test_index(&[("alpha", 0), ("mit", 1)], 2);
    let text = "alpha MIT omega";
    let query = Query::from_extracted_text(text, &index, false).expect("query should build");
    let mut detection = make_detection("");
    detection.matches[0].coordinates =
        MatchCoordinates::query_region(PositionSpan::from_positions(vec![1]));
    detection.matches[0].start_token = 1;
    detection.matches[0].end_token = 2;

    let percentage = compute_percentage_of_license_text(&query, &[detection]);

    assert_eq!(percentage, 33.33);
}

#[test]
fn test_extract_license_information_maps_timeout_to_stage_error() {
    let mut file_info_builder = FileInfoBuilder::default();
    let mut scan_diagnostics: Vec<ScanDiagnostic> = Vec::new();

    let error = extract_license_information(
        &mut file_info_builder,
        &mut scan_diagnostics,
        LicenseExtractionInput {
            path: std::path::Path::new("timeout.txt"),
            text_content:
                "Permission is hereby granted, free of charge, to any person obtaining a copy"
                    .to_string(),
            license_engine: Some(TEST_ENGINE.clone()),
            license_options: LicenseScanOptions::default(),
            from_binary_strings: false,
            timeout_seconds: 1.0,
            deadline: Some(Instant::now() - Duration::from_millis(1)),
        },
    )
    .expect_err("expired deadline should map to stage-specific timeout");

    assert!(scan_diagnostics.is_empty());
    assert_eq!(error.to_string(), "Timeout during license scan (> 1.00s)");
}
