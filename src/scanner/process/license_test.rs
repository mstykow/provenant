use super::{
    LicenseExtractionInput, compute_percentage_of_license_text, convert_detection_to_model,
    extract_license_information,
};
use crate::license_detection::LicenseDetection as InternalLicenseDetection;
use crate::license_detection::LicenseDetectionEngine;
use crate::license_detection::index::LicenseIndex;
use crate::license_detection::index::dictionary::TokenDictionary;
use crate::license_detection::models::position_span::PositionSpan;
use crate::license_detection::models::{LicenseMatch, MatchCoordinates, MatcherKind, RuleKind};
use crate::license_detection::query::Query;
use crate::models::{FileInfoBuilder, LineNumber, MatchScore};
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
    let dictionary = TokenDictionary::new_with_legalese(entries);
    let mut index = LicenseIndex::new(dictionary);
    index.len_legalese = len_legalese;
    index
}

#[test]
fn test_convert_detection_to_model_preserves_rule_url() {
    let detection = make_detection(
        "https://github.com/aboutcode-org/scancode-toolkit/tree/develop/src/licensedcode/data/licenses/mit.LICENSE",
    );

    let (converted, clues) =
        convert_detection_to_model(&detection, LicenseScanOptions::default(), "", None);
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
        convert_detection_to_model(&detection, LicenseScanOptions::default(), "", None);
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
        convert_detection_to_model(&detection, LicenseScanOptions::default(), "", None);
    let converted = converted.expect("detection should convert");

    assert_eq!(
        converted.matches[0].score,
        MatchScore::from_percentage(81.82)
    );
    assert_eq!(converted.matches[0].match_coverage, Some(33.33));
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
    let mut scan_errors = Vec::new();

    let error = extract_license_information(
        &mut file_info_builder,
        &mut scan_errors,
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

    assert!(scan_errors.is_empty());
    assert_eq!(error.to_string(), "Timeout during license scan (> 1.00s)");
}
