// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn collect_top_level_license_detections_groups_file_detections_and_preserves_paths() {
    let mut first = file("project/src/lib.rs");
    first.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/src/lib.rs".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(3).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(10),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("mit.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec!["imperfect-match-coverage".to_string()],
        identifier: Some("mit-shared-id".to_string()),
    }];

    let mut second = file("project/src/other.rs");
    second.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/src/other.rs".to_string()),
            start_line: LineNumber::new(4).unwrap(),
            end_line: LineNumber::new(6).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(10),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("mit.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("mit-shared-id".to_string()),
    }];

    let mut third = file("project/src/apache.rs");
    third.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "apache-2.0".to_string(),
        license_expression_spdx: "Apache-2.0".to_string(),
        matches: vec![Match {
            license_expression: "apache-2.0".to_string(),
            license_expression_spdx: "Apache-2.0".to_string(),
            from_file: Some("project/src/apache.rs".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(12).unwrap(),
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(120),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("apache-2.0_2.RULE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("apache-2.0-id".to_string()),
    }];

    let detections = collect_top_level_license_detections(&[first, second, third]);

    assert_eq!(detections.len(), 2);
    assert_eq!(detections[0].license_expression, "apache-2.0");
    assert_eq!(detections[0].detection_count, 1);
    assert_eq!(detections[1].identifier, "mit-shared-id");
    assert_eq!(detections[1].detection_count, 2);
    assert_eq!(
        detections[1].reference_matches[0].from_file.as_deref(),
        Some("project/src/lib.rs")
    );
    assert_eq!(detections[1].reference_matches.len(), 1);
    assert_eq!(
        detections[1].detection_log,
        vec!["imperfect-match-coverage".to_string()]
    );
}

#[test]
fn collect_top_level_license_detections_counts_same_identifier_regions_in_one_file() {
    let mut file = file("project/src/lib.rs");
    file.license_detections = vec![
        crate::models::LicenseDetection {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            matches: vec![Match {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                from_file: Some("project/src/lib.rs".to_string()),
                start_line: LineNumber::ONE,
                end_line: LineNumber::new(3).unwrap(),
                matcher: Some("1-hash".to_string()),
                score: MatchScore::MAX,
                matched_length: Some(10),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: Some("mit.LICENSE".to_string()),
                rule_url: None,
                matched_text: None,
                referenced_filenames: None,
                matched_text_diagnostics: None,
            }],
            detection_log: vec![],
            identifier: Some("mit-shared-id".to_string()),
        },
        crate::models::LicenseDetection {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            matches: vec![Match {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                from_file: Some("project/src/lib.rs".to_string()),
                start_line: LineNumber::new(20).unwrap(),
                end_line: LineNumber::new(25).unwrap(),
                matcher: Some("2-aho".to_string()),
                score: MatchScore::MAX,
                matched_length: Some(12),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: Some("mit_3.RULE".to_string()),
                rule_url: None,
                matched_text: None,
                referenced_filenames: None,
                matched_text_diagnostics: None,
            }],
            detection_log: vec![],
            identifier: Some("mit-shared-id".to_string()),
        },
    ];

    let detections = collect_top_level_license_detections(&[file]);

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].detection_count, 2);
    assert_eq!(detections[0].reference_matches.len(), 1);
}

#[test]
fn collect_top_level_license_detections_deduplicates_identical_regions() {
    let mut file = file("project/src/lib.rs");
    file.license_detections = vec![
        crate::models::LicenseDetection {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            matches: vec![Match {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                from_file: Some("project/src/lib.rs".to_string()),
                start_line: LineNumber::ONE,
                end_line: LineNumber::new(5).unwrap(),
                matcher: Some("1-hash".to_string()),
                score: MatchScore::MAX,
                matched_length: Some(10),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: Some("mit.LICENSE".to_string()),
                rule_url: None,
                matched_text: None,
                referenced_filenames: None,
                matched_text_diagnostics: None,
            }],
            detection_log: vec![],
            identifier: Some("mit-shared-id".to_string()),
        },
        crate::models::LicenseDetection {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            matches: vec![Match {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                from_file: Some("project/src/lib.rs".to_string()),
                start_line: LineNumber::ONE,
                end_line: LineNumber::new(5).unwrap(),
                matcher: Some("2-aho".to_string()),
                score: MatchScore::MAX,
                matched_length: Some(10),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: Some("mit_1.RULE".to_string()),
                rule_url: None,
                matched_text: None,
                referenced_filenames: None,
                matched_text_diagnostics: None,
            }],
            detection_log: vec![],
            identifier: Some("mit-shared-id".to_string()),
        },
    ];

    let detections = collect_top_level_license_detections(&[file]);

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].detection_count, 1);
    assert_eq!(detections[0].reference_matches.len(), 1);
}

#[test]
fn collect_top_level_license_detections_recomputes_empty_expression_from_matches() {
    let mut file = file("project/src/lib.rs");
    file.license_detections = vec![crate::models::LicenseDetection {
        license_expression: String::new(),
        license_expression_spdx: String::new(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/src/lib.rs".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(3).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(10),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("mit.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("mit-shared-id".to_string()),
    }];

    let detections = collect_top_level_license_detections(&[file]);

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].license_expression, "mit");
    assert_eq!(detections[0].license_expression_spdx, "MIT");
}

#[test]
fn collect_top_level_license_detections_includes_package_origin_detections() {
    let mut manifest = file("project/package.json");
    manifest.package_data = vec![PackageData {
        package_type: Some(PackageType::Npm),
        license_detections: vec![crate::models::LicenseDetection {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            matches: vec![Match {
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
        other_license_detections: vec![crate::models::LicenseDetection {
            license_expression: "apache-2.0".to_string(),
            license_expression_spdx: "Apache-2.0".to_string(),
            matches: vec![Match {
                license_expression: "apache-2.0".to_string(),
                license_expression_spdx: "Apache-2.0".to_string(),
                from_file: None,
                start_line: LineNumber::new(2).unwrap(),
                end_line: LineNumber::new(2).unwrap(),
                matcher: Some("parser-declared-license".to_string()),
                score: MatchScore::MAX,
                matched_length: Some(1),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: None,
                rule_url: None,
                matched_text: Some("Apache-2.0".to_string()),
                referenced_filenames: None,
                matched_text_diagnostics: None,
            }],
            detection_log: vec![],
            identifier: None,
        }],
        ..PackageData::default()
    }];
    manifest.backfill_license_provenance();

    let detections = collect_top_level_license_detections(&[manifest]);

    assert_eq!(detections.len(), 2);
    assert_eq!(detections[0].license_expression, "apache-2.0");
    assert_eq!(detections[1].license_expression, "mit");
    assert_eq!(
        detections[1].reference_matches[0].from_file.as_deref(),
        Some("project/package.json")
    );
    assert_eq!(
        detections[1].reference_matches[0]
            .rule_identifier
            .as_deref(),
        Some("parser-declared-license")
    );
}

#[test]
fn collect_top_level_license_detections_prefers_later_logged_representative() {
    let mut first = file("project/src/lib.rs");
    first.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/src/lib.rs".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(3).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(10),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("mit.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("mit-shared-id".to_string()),
    }];

    let mut second = file("project/src/other.rs");
    second.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/src/other.rs".to_string()),
            start_line: LineNumber::new(4).unwrap(),
            end_line: LineNumber::new(6).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(10),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("mit.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec!["imperfect-match-coverage".to_string()],
        identifier: Some("mit-shared-id".to_string()),
    }];

    let detections = collect_top_level_license_detections(&[first, second]);

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].detection_count, 2);
    assert_eq!(
        detections[0].reference_matches[0].from_file.as_deref(),
        Some("project/src/other.rs")
    );
    assert_eq!(
        detections[0].detection_log,
        vec!["imperfect-match-coverage".to_string()]
    );
}

#[test]
fn collect_top_level_license_detections_keeps_identifier_with_zero_match_detection() {
    let mut file = file("project/src/lib.rs");
    file.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![],
        detection_log: vec![],
        identifier: Some("mit-empty".to_string()),
    }];

    let detections = collect_top_level_license_detections(&[file]);

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].identifier, "mit-empty");
    assert_eq!(detections[0].detection_count, 0);
    assert!(detections[0].reference_matches.is_empty());
}
