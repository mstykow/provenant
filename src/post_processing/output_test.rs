use chrono::{TimeZone, Timelike, Utc};
use std::collections::HashMap;

use super::test_utils::{dir, file};
use super::*;
use crate::assembly;
use crate::license_detection::index::{IndexedRuleMetadata, LicenseIndex};
use crate::license_detection::models::{License as RuntimeLicense, Rule, RuleKind};
use crate::models::{
    Copyright, Holder, LineNumber, Match, MatchScore, Package, PackageData, PackageType,
    PackageUid, Tallies,
};
use crate::scan_result_shaping::normalize_paths;
use serde_json::json;

fn sample_runtime_license(key: &str, name: &str, spdx_license_key: Option<&str>) -> RuntimeLicense {
    RuntimeLicense {
        key: key.to_string(),
        short_name: Some(name.to_string()),
        name: name.to_string(),
        language: Some("en".to_string()),
        spdx_license_key: spdx_license_key.map(str::to_string),
        other_spdx_license_keys: vec![],
        category: Some("Permissive".to_string()),
        owner: Some("Example Owner".to_string()),
        homepage_url: Some("https://example.com/license".to_string()),
        text: format!("{name} text"),
        reference_urls: vec!["https://example.com/license".to_string()],
        osi_license_key: spdx_license_key.map(str::to_string),
        text_urls: vec!["https://example.com/license.txt".to_string()],
        osi_url: Some("https://opensource.org/licenses/example".to_string()),
        faq_url: Some("https://example.com/faq".to_string()),
        other_urls: vec!["https://example.com/other".to_string()],
        notes: None,
        is_deprecated: false,
        is_exception: false,
        is_unknown: false,
        is_generic: false,
        replaced_by: vec![],
        minimum_coverage: None,
        standard_notice: Some("Standard notice".to_string()),
        ignorable_copyrights: None,
        ignorable_holders: None,
        ignorable_authors: None,
        ignorable_urls: None,
        ignorable_emails: None,
    }
}

fn sample_rule(identifier: &str, expression: &str, rule_kind: RuleKind) -> Rule {
    Rule {
        identifier: identifier.to_string(),
        license_expression: expression.to_string(),
        text: format!("{identifier} text"),
        tokens: vec![],
        rule_kind,
        is_false_positive: false,
        is_required_phrase: false,
        is_from_license: false,
        relevance: 100,
        minimum_coverage: None,
        has_stored_minimum_coverage: false,
        is_continuous: true,
        required_phrase_spans: vec![],
        stopwords_by_pos: HashMap::new(),
        referenced_filenames: None,
        ignorable_urls: None,
        ignorable_emails: None,
        ignorable_copyrights: None,
        ignorable_holders: None,
        ignorable_authors: None,
        language: None,
        notes: None,
        length_unique: 0,
        high_length_unique: 0,
        high_length: 0,
        min_matched_length: 0,
        min_high_matched_length: 0,
        min_matched_length_unique: 0,
        min_high_matched_length_unique: 0,
        is_small: false,
        is_tiny: false,
        starts_with_license: false,
        ends_with_license: false,
        is_deprecated: false,
        spdx_license_key: None,
        other_spdx_license_keys: vec![],
    }
}

#[test]
fn collect_top_level_license_references_includes_clues_packages_and_sorted_deduped_refs() {
    let licenses = vec![
        sample_runtime_license("apache-2.0", "Apache License 2.0", Some("Apache-2.0")),
        sample_runtime_license("bsd-simplified", "BSD 2-Clause", Some("BSD-2-Clause")),
        sample_runtime_license("mit", "MIT License", Some("MIT")),
        sample_runtime_license(
            "unknown-license-reference",
            "Unknown License Reference",
            None,
        ),
    ];
    let mut license_index = LicenseIndex::default();
    for license in &licenses {
        license_index
            .licenses_by_key
            .insert(license.key.clone(), license.clone());
    }
    license_index.rules_by_rid = vec![
        sample_rule("apache-2.0_1.RULE", "apache-2.0", RuleKind::Text),
        sample_rule(
            "license-clue_1.RULE",
            "unknown-license-reference",
            RuleKind::Clue,
        ),
    ];
    let mut source = file("project/src/lib.rs");
    source.license_expression = Some("mit".to_string());
    source.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/src/lib.rs".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(2).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(10),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("apache-2.0_1.RULE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        identifier: None,
        detection_log: vec![],
    }];
    source.license_clues = vec![Match {
        license_expression: "unknown-license-reference".to_string(),
        license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
        from_file: Some("project/NOTICE".to_string()),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
        matcher: Some("2-aho".to_string()),
        score: MatchScore::MAX,
        matched_length: Some(4),
        match_coverage: Some(100.0),
        rule_relevance: Some(100),
        rule_identifier: Some("license-clue_1.RULE".to_string()),
        rule_url: None,
        matched_text: None,
        referenced_filenames: None,
        matched_text_diagnostics: None,
    }];
    source.package_data = vec![PackageData {
        package_type: Some(PackageType::Npm),
        declared_license_expression: Some("bsd-simplified".to_string()),
        ..PackageData::default()
    }];

    let mut package = super::test_utils::package("pkg:npm/demo?uuid=test", "project/package.json");
    package.package_type = Some(PackageType::Npm);
    package.declared_license_expression = Some("apache-2.0".to_string());

    let (license_references, license_rule_references) = collect_top_level_license_references(
        &[dir("project"), source],
        &[package],
        &license_index,
        DEFAULT_LICENSEDB_URL_TEMPLATE,
    );

    assert_eq!(
        license_references
            .iter()
            .map(|reference| reference.spdx_license_key.as_str())
            .collect::<Vec<_>>(),
        vec![
            "Apache-2.0",
            "BSD-2-Clause",
            "MIT",
            "LicenseRef-scancode-unknown-license-reference",
        ]
    );
    assert_eq!(
        license_rule_references
            .iter()
            .map(|reference| reference.identifier.as_str())
            .collect::<Vec<_>>(),
        vec!["apache-2.0_1.RULE", "license-clue_1.RULE"]
    );
    assert!(license_rule_references[1].is_license_clue);
    assert_eq!(license_references[0].key.as_deref(), Some("apache-2.0"));
    assert_eq!(
        license_references[0].category.as_deref(),
        Some("Permissive")
    );
    assert_eq!(license_references[0].language.as_deref(), Some("en"));
    assert_eq!(
        license_references[0].owner.as_deref(),
        Some("Example Owner")
    );
    assert_eq!(
        license_references[0].homepage_url.as_deref(),
        Some("https://example.com/license")
    );
    assert_eq!(
        license_references[0].osi_license_key.as_deref(),
        Some("Apache-2.0")
    );
    assert_eq!(
        license_references[0].text_urls,
        vec!["https://example.com/license.txt".to_string()]
    );
    assert_eq!(
        license_references[0].osi_url.as_deref(),
        Some("https://opensource.org/licenses/example")
    );
    assert!(!license_references[0].is_exception);
    assert_eq!(
        license_references[0].standard_notice.as_deref(),
        Some("Standard notice")
    );
    assert!(license_references[0].scancode_url.is_some());
    assert_eq!(
        license_references[0].licensedb_url.as_deref(),
        Some("https://scancode-licensedb.aboutcode.org/apache-2.0")
    );
    assert_eq!(license_rule_references[0].relevance, Some(100));
}

#[test]
fn collect_top_level_license_references_returns_empty_for_empty_inputs() {
    let license_index = LicenseIndex::default();

    let (license_references, license_rule_references) = collect_top_level_license_references(
        &[],
        &[],
        &license_index,
        DEFAULT_LICENSEDB_URL_TEMPLATE,
    );

    assert!(license_references.is_empty());
    assert!(license_rule_references.is_empty());
}

#[test]
fn collect_top_level_license_references_marks_synthetic_spdx_rules() {
    let license_index = LicenseIndex {
        rules_by_rid: vec![Rule {
            identifier: "spdx_license_id_mit_for_mit.RULE".to_string(),
            license_expression: "mit".to_string(),
            text: "MIT".to_string(),
            tokens: vec![],
            rule_kind: RuleKind::Tag,
            is_false_positive: false,
            is_required_phrase: false,
            is_from_license: false,
            relevance: 100,
            minimum_coverage: Some(0),
            has_stored_minimum_coverage: false,
            is_continuous: false,
            required_phrase_spans: vec![],
            stopwords_by_pos: HashMap::new(),
            referenced_filenames: None,
            ignorable_urls: None,
            ignorable_emails: None,
            ignorable_copyrights: None,
            ignorable_holders: None,
            ignorable_authors: None,
            language: Some("en".to_string()),
            notes: None,
            length_unique: 0,
            high_length_unique: 0,
            high_length: 0,
            min_matched_length: 0,
            min_high_matched_length: 0,
            min_matched_length_unique: 0,
            min_high_matched_length_unique: 0,
            is_small: false,
            is_tiny: false,
            starts_with_license: false,
            ends_with_license: false,
            is_deprecated: false,
            spdx_license_key: Some("MIT".to_string()),
            other_spdx_license_keys: vec![],
        }],
        ..LicenseIndex::default()
    };

    let mut source = file("project/Cargo.toml");
    source.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/Cargo.toml".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("1-spdx-id".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(1),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("spdx_license_id_mit_for_mit.RULE".to_string()),
            rule_url: None,
            matched_text: Some("MIT".to_string()),
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        identifier: Some("mit-id".to_string()),
        detection_log: vec![],
    }];

    let (_, license_rule_references) = collect_top_level_license_references(
        &[source],
        &[],
        &license_index,
        DEFAULT_LICENSEDB_URL_TEMPLATE,
    );

    assert_eq!(license_rule_references.len(), 1);
    assert!(license_rule_references[0].is_synthetic);
    assert!(license_rule_references[0].rule_url.is_none());
    assert_eq!(license_rule_references[0].length, 0);
    assert!(!license_rule_references[0].skip_for_required_phrase_generation);
}

#[test]
fn collect_top_level_license_references_applies_custom_license_url_template() {
    let licenses = vec![sample_runtime_license("mit", "MIT License", Some("MIT"))];
    let mut license_index = LicenseIndex::default();
    for license in &licenses {
        license_index
            .licenses_by_key
            .insert(license.key.clone(), license.clone());
    }

    let mut source = file("project/src/lib.rs");
    source.license_expression = Some("mit".to_string());
    source.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/src/lib.rs".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("1-hash".to_string()),
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
        identifier: None,
        detection_log: vec![],
    }];

    let (license_references, _) = collect_top_level_license_references(
        &[dir("project"), source],
        &[],
        &license_index,
        "https://licenses.example.test/{}/details",
    );

    assert_eq!(license_references.len(), 1);
    assert_eq!(
        license_references[0].licensedb_url.as_deref(),
        Some("https://licenses.example.test/mit/details")
    );
    assert_eq!(
        license_references[0].scancode_url.as_deref(),
        Some(
            "https://github.com/aboutcode-org/scancode-toolkit/tree/develop/src/licensedcode/data/licenses/mit.LICENSE"
        )
    );
}

#[test]
fn collect_top_level_license_references_preserves_rule_metadata() {
    let licenses = vec![sample_runtime_license("mit", "MIT License", Some("MIT"))];
    let mut license_index = LicenseIndex::default();
    for license in &licenses {
        license_index
            .licenses_by_key
            .insert(license.key.clone(), license.clone());
    }

    license_index.rules_by_rid = vec![sample_rule("mit_1.RULE", "mit", RuleKind::Text)];
    license_index.rule_metadata_by_identifier.insert(
        "mit_1.RULE".to_string(),
        IndexedRuleMetadata {
            license_expression_spdx: Some("MIT".to_string()),
            skip_for_required_phrase_generation: true,
            replaced_by: vec!["apache-2.0".to_string()],
        },
    );

    let mut source = file("project/src/lib.rs");
    source.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: None,
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(2),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("mit_1.RULE".to_string()),
            rule_url: None,
            matched_text: Some("MIT License".to_string()),
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        identifier: Some("mit-id".to_string()),
        detection_log: vec![],
    }];

    let (_, license_rule_references) = collect_top_level_license_references(
        &[source],
        &[],
        &license_index,
        DEFAULT_LICENSEDB_URL_TEMPLATE,
    );

    assert_eq!(license_rule_references.len(), 1);
    assert!(license_rule_references[0].skip_for_required_phrase_generation);
    assert_eq!(
        license_rule_references[0].replaced_by,
        vec!["apache-2.0".to_string()]
    );
}

#[test]
fn apply_local_file_reference_following_resolves_root_license_file() {
    let mut license = file("project/LICENSE");
    license.license_expression = Some("mit".to_string());
    license.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/LICENSE".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(20).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(100),
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

    let mut notice = file("project/src/notice.js");
    notice.license_expression = Some("unknown-license-reference".to_string());
    notice.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "unknown-license-reference".to_string(),
        license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
        matches: vec![Match {
            license_expression: "unknown-license-reference".to_string(),
            license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
            from_file: Some("project/src/notice.js".to_string()),
            start_line: LineNumber::new(2).unwrap(),
            end_line: LineNumber::new(2).unwrap(),
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(2),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("unknown-license-reference_see-license_1.RULE".to_string()),
            rule_url: None,
            matched_text: Some("See LICENSE".to_string()),
            referenced_filenames: Some(vec!["LICENSE".to_string()]),
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("unknown-ref".to_string()),
    }];

    let mut files = vec![dir("project"), license, notice];
    let mut packages = Vec::new();
    apply_package_reference_following(&mut files, &mut packages);

    let notice = files
        .iter()
        .find(|file| file.path == "project/src/notice.js")
        .expect("notice file should exist");
    assert_eq!(notice.license_expression.as_deref(), Some("mit"));
    assert_eq!(
        notice.license_detections[0].detection_log,
        vec!["unknown-reference-to-local-file"]
    );
    assert_eq!(notice.license_detections[0].matches.len(), 2);
    assert_eq!(
        notice.license_detections[0].matches[1].from_file.as_deref(),
        Some("project/LICENSE")
    );
}

#[test]
fn apply_local_file_reference_following_prefers_root_license_for_imperfect_subdir_reference() {
    let mut root_license = file("LICENSE");
    root_license.license_expression = Some("npsl-exception-0.95".to_string());
    root_license.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "npsl-exception-0.95".to_string(),
        license_expression_spdx: "LicenseRef-scancode-npsl-exception-0.95".to_string(),
        matches: vec![Match {
            license_expression: "npsl-exception-0.95".to_string(),
            license_expression_spdx: "LicenseRef-scancode-npsl-exception-0.95".to_string(),
            from_file: Some("LICENSE".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(582).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(4720),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("npsl-exception-0.95.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("npsl-license".to_string()),
    }];

    let mut sibling_license = file("third_party/LICENSE");
    sibling_license.license_expression = Some("bsd-new".to_string());
    sibling_license.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "bsd-new".to_string(),
        license_expression_spdx: "BSD-3-Clause".to_string(),
        matches: vec![Match {
            license_expression: "bsd-new".to_string(),
            license_expression_spdx: "BSD-3-Clause".to_string(),
            from_file: Some("third_party/LICENSE".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(30).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(150),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("bsd-new.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("bsd-license".to_string()),
    }];

    let mut header = file("src/FPEngine.h");
    header.license_expression = Some("gpl-1.0-plus OR mit".to_string());
    header.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "gpl-1.0-plus OR mit".to_string(),
        license_expression_spdx: "GPL-1.0-or-later OR MIT".to_string(),
        matches: vec![Match {
            license_expression: "gpl-1.0-plus OR mit".to_string(),
            license_expression_spdx: "GPL-1.0-or-later OR MIT".to_string(),
            from_file: Some("src/FPEngine.h".to_string()),
            start_line: LineNumber::new(49).unwrap(),
            end_line: LineNumber::new(57).unwrap(),
            matcher: Some("3-seq".to_string()),
            score: MatchScore::from_percentage(41.79),
            matched_length: Some(28),
            match_coverage: Some(41.79),
            rule_relevance: Some(100),
            rule_identifier: Some("gpl-1.0-plus_or_mit_2.RULE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: Some(vec!["LICENSE".to_string()]),
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("nmap-header-ref".to_string()),
    }];

    let mut files = vec![
        dir("src"),
        dir("third_party"),
        root_license,
        sibling_license,
        header,
    ];
    let mut packages = Vec::new();
    apply_package_reference_following(&mut files, &mut packages);

    let header = files
        .iter()
        .find(|file| file.path == "src/FPEngine.h")
        .expect("header file should exist");
    assert_eq!(
        header.license_expression.as_deref(),
        Some("npsl-exception-0.95")
    );
    assert_eq!(
        header.license_detections[0].license_expression_spdx,
        "LicenseRef-scancode-npsl-exception-0.95"
    );
    assert_eq!(
        header.license_detections[0].detection_log,
        vec!["unknown-reference-to-local-file"]
    );
    assert_eq!(header.license_detections[0].matches.len(), 2);
    assert_eq!(
        header.license_detections[0].matches[1].from_file.as_deref(),
        Some("LICENSE")
    );
}

#[test]
fn apply_local_file_reference_following_does_not_reuse_followed_license_as_second_hop_source() {
    let mut root_license = file("project/LICENSE");
    root_license.license_expression = Some("mit".to_string());
    root_license.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/LICENSE".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(20).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(100),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("mit.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("root-license".to_string()),
    }];

    let mut followed_license = file("project/ncat/LICENSE");
    followed_license.license_expression = Some("mit".to_string());
    followed_license.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![
            Match {
                license_expression: "unknown-license-reference".to_string(),
                license_expression_spdx: "LicenseRef-scancode-unknown-license-reference"
                    .to_string(),
                from_file: Some("project/ncat/LICENSE".to_string()),
                start_line: LineNumber::ONE,
                end_line: LineNumber::ONE,
                matcher: Some("2-aho".to_string()),
                score: MatchScore::MAX,
                matched_length: Some(2),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: Some("unknown-license-reference_see-license_1.RULE".to_string()),
                rule_url: None,
                matched_text: Some("See LICENSE".to_string()),
                referenced_filenames: Some(vec!["LICENSE".to_string()]),
                matched_text_diagnostics: None,
            },
            Match {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                from_file: Some("project/LICENSE".to_string()),
                start_line: LineNumber::ONE,
                end_line: LineNumber::new(20).unwrap(),
                matcher: Some("1-hash".to_string()),
                score: MatchScore::MAX,
                matched_length: Some(100),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: Some("mit.LICENSE".to_string()),
                rule_url: None,
                matched_text: None,
                referenced_filenames: None,
                matched_text_diagnostics: None,
            },
        ],
        detection_log: vec!["unknown-reference-to-local-file".to_string()],
        identifier: Some("followed-license".to_string()),
    }];

    let mut source = file("project/ncat/ncat_core.h");
    source.license_expression = Some("unknown-license-reference".to_string());
    source.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "unknown-license-reference".to_string(),
        license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
        matches: vec![Match {
            license_expression: "unknown-license-reference".to_string(),
            license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
            from_file: Some("project/ncat/ncat_core.h".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(2),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("unknown-license-reference_see-license_1.RULE".to_string()),
            rule_url: None,
            matched_text: Some("See LICENSE".to_string()),
            referenced_filenames: Some(vec!["LICENSE".to_string()]),
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("second-hop-source".to_string()),
    }];

    let mut files = vec![
        dir("project"),
        dir("project/ncat"),
        root_license,
        followed_license,
        source,
    ];
    let mut packages = Vec::new();
    apply_package_reference_following(&mut files, &mut packages);

    let source = files
        .iter()
        .find(|file| file.path == "project/ncat/ncat_core.h")
        .expect("source file should exist");
    assert_eq!(
        source.license_expression.as_deref(),
        Some("unknown-license-reference")
    );
    assert_eq!(
        source.license_detections[0].detection_log,
        Vec::<String>::new()
    );
    assert_eq!(source.license_detections[0].matches.len(), 1);
}

#[test]
fn apply_local_file_reference_following_requires_exact_filename_match() {
    let mut license = file("project/LICENSE");
    license.license_expression = Some("mit".to_string());
    license.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/LICENSE".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(20).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(100),
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

    let mut notice = file("project/src/notice.js");
    notice.license_expression = Some("unknown-license-reference".to_string());
    notice.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "unknown-license-reference".to_string(),
        license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
        matches: vec![Match {
            license_expression: "unknown-license-reference".to_string(),
            license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
            from_file: Some("project/src/notice.js".to_string()),
            start_line: LineNumber::new(2).unwrap(),
            end_line: LineNumber::new(2).unwrap(),
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(2),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("unknown-license-reference_see-license_1.RULE".to_string()),
            rule_url: None,
            matched_text: Some("See LICENSE.txt".to_string()),
            referenced_filenames: Some(vec!["LICENSE.txt".to_string()]),
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("unknown-ref".to_string()),
    }];

    let mut files = vec![dir("project"), license, notice];
    let mut packages = Vec::new();
    apply_package_reference_following(&mut files, &mut packages);

    let notice = files
        .iter()
        .find(|file| file.path == "project/src/notice.js")
        .expect("notice file should exist");
    assert_eq!(
        notice.license_expression.as_deref(),
        Some("unknown-license-reference")
    );
    assert_eq!(notice.license_detections[0].matches.len(), 1);
}

#[test]
fn apply_local_file_reference_following_does_not_search_unrelated_top_level_directories() {
    let mut nested_copying = file("libssh2/COPYING");
    nested_copying.license_expression = Some("bsd-new".to_string());
    nested_copying.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "bsd-new".to_string(),
        license_expression_spdx: "BSD-3-Clause".to_string(),
        matches: vec![Match {
            license_expression: "bsd-new".to_string(),
            license_expression_spdx: "BSD-3-Clause".to_string(),
            from_file: Some("libssh2/COPYING".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(20).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(100),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("bsd-new.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("nested-copying".to_string()),
    }];

    let mut notice = file("docs/3rd-party-licenses.txt");
    notice.license_expression = Some("unknown-license-reference".to_string());
    notice.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "unknown-license-reference".to_string(),
        license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
        matches: vec![Match {
            license_expression: "unknown-license-reference".to_string(),
            license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
            from_file: Some("docs/3rd-party-licenses.txt".to_string()),
            start_line: LineNumber::new(10).unwrap(),
            end_line: LineNumber::new(10).unwrap(),
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(2),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("unknown-license-reference_see-license_1.RULE".to_string()),
            rule_url: None,
            matched_text: Some("See COPYING".to_string()),
            referenced_filenames: Some(vec!["COPYING".to_string()]),
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("docs-copying-ref".to_string()),
    }];

    let mut files = vec![dir("docs"), dir("libssh2"), nested_copying, notice];
    let mut packages = Vec::new();
    apply_package_reference_following(&mut files, &mut packages);

    let notice = files
        .iter()
        .find(|file| file.path == "docs/3rd-party-licenses.txt")
        .expect("notice file should exist");
    assert_eq!(
        notice.license_expression.as_deref(),
        Some("unknown-license-reference")
    );
    assert_eq!(notice.license_detections[0].matches.len(), 1);
    assert!(notice.license_detections[0].detection_log.is_empty());
}

#[test]
fn apply_local_file_reference_following_drops_unknown_intro_from_resolved_target() {
    let mut license = file("project/LICENSE");
    license.license_expression = Some("apache-2.0".to_string());
    license.license_detections = vec![
        crate::models::LicenseDetection {
            license_expression: "unknown-license-reference".to_string(),
            license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
            matches: vec![Match {
                license_expression: "unknown-license-reference".to_string(),
                license_expression_spdx: "LicenseRef-scancode-unknown-license-reference"
                    .to_string(),
                from_file: Some("project/LICENSE".to_string()),
                start_line: LineNumber::new(2).unwrap(),
                end_line: LineNumber::new(2).unwrap(),
                matcher: Some("2-aho".to_string()),
                score: MatchScore::from_percentage(50.0),
                matched_length: Some(2),
                match_coverage: Some(100.0),
                rule_relevance: Some(50),
                rule_identifier: Some("license-intro_2.RULE".to_string()),
                rule_url: None,
                matched_text: Some("Apache License".to_string()),
                referenced_filenames: None,
                matched_text_diagnostics: None,
            }],
            detection_log: vec![],
            identifier: Some("license-intro".to_string()),
        },
        crate::models::LicenseDetection {
            license_expression: "apache-2.0".to_string(),
            license_expression_spdx: "Apache-2.0".to_string(),
            matches: vec![Match {
                license_expression: "apache-2.0".to_string(),
                license_expression_spdx: "Apache-2.0".to_string(),
                from_file: Some("project/LICENSE".to_string()),
                start_line: LineNumber::new(5).unwrap(),
                end_line: LineNumber::new(205).unwrap(),
                matcher: Some("1-hash".to_string()),
                score: MatchScore::MAX,
                matched_length: Some(1584),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: Some("apache-2.0.LICENSE".to_string()),
                rule_url: None,
                matched_text: None,
                referenced_filenames: None,
                matched_text_diagnostics: None,
            }],
            detection_log: vec![],
            identifier: Some("apache-license".to_string()),
        },
    ];

    let mut notice = file("project/src/notice.js");
    notice.license_expression = Some("unknown-license-reference".to_string());
    notice.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "unknown-license-reference".to_string(),
        license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
        matches: vec![Match {
            license_expression: "unknown-license-reference".to_string(),
            license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
            from_file: Some("project/src/notice.js".to_string()),
            start_line: LineNumber::new(2).unwrap(),
            end_line: LineNumber::new(2).unwrap(),
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(2),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("unknown-license-reference_see-license_1.RULE".to_string()),
            rule_url: None,
            matched_text: Some("See LICENSE".to_string()),
            referenced_filenames: Some(vec!["LICENSE".to_string()]),
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("unknown-ref".to_string()),
    }];

    let mut files = vec![dir("project"), license, notice];
    let mut packages = Vec::new();
    apply_package_reference_following(&mut files, &mut packages);

    let notice = files
        .iter()
        .find(|file| file.path == "project/src/notice.js")
        .expect("notice file should exist");
    assert_eq!(notice.license_expression.as_deref(), Some("apache-2.0"));
    assert_eq!(
        notice.license_detections[0].detection_log,
        vec!["unknown-reference-to-local-file"]
    );
    assert_eq!(notice.license_detections[0].matches.len(), 2);
    assert!(notice.license_detections[0].matches.iter().all(|m| {
        m.license_expression != "unknown-license-reference"
            || m.from_file.as_deref() != Some("project/LICENSE")
    }));
}

#[test]
fn apply_local_file_reference_following_resolves_files_beside_manifest() {
    let package_uid = "pkg:pypi/demo?uuid=test".to_string();
    let mut package = super::test_utils::package(&package_uid, "project/demo.dist-info/METADATA");
    package.datafile_paths = vec!["project/demo.dist-info/METADATA".to_string()];

    let mut license = file("project/demo.dist-info/LICENSE");
    license.license_expression = Some("mit".to_string());
    license.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/demo.dist-info/LICENSE".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(20).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(100),
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

    let mut source = file("project/demo/__init__.py");
    source.for_packages = vec![PackageUid::from_raw(package_uid.clone())];
    source.license_expression = Some("unknown-license-reference".to_string());
    source.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "unknown-license-reference".to_string(),
        license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
        matches: vec![Match {
            license_expression: "unknown-license-reference".to_string(),
            license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
            from_file: Some("project/demo/__init__.py".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(2),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("unknown-license-reference_see-license_1.RULE".to_string()),
            rule_url: None,
            matched_text: Some("See LICENSE".to_string()),
            referenced_filenames: Some(vec!["LICENSE".to_string()]),
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("unknown-ref".to_string()),
    }];

    let mut files = vec![dir("project"), license, source];
    let mut packages = vec![package];
    apply_package_reference_following(&mut files, &mut packages);

    let source = files
        .iter()
        .find(|file| file.path == "project/demo/__init__.py")
        .expect("source file should exist");
    assert_eq!(source.license_expression.as_deref(), Some("mit"));
    assert_eq!(
        source.license_detections[0].matches[1].from_file.as_deref(),
        Some("project/demo.dist-info/LICENSE")
    );
}

#[test]
fn apply_package_reference_following_resolves_manifest_origin_local_file() {
    let package_uid = "pkg:cargo/demo?uuid=test".to_string();
    let mut package = super::test_utils::package(&package_uid, "project/Cargo.toml");
    package.datafile_paths = vec!["project/Cargo.toml".to_string()];
    package.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "unknown-license-reference".to_string(),
        license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
        matches: vec![Match {
            license_expression: "unknown-license-reference".to_string(),
            license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
            from_file: Some("project/Cargo.toml".to_string()),
            start_line: LineNumber::new(5).unwrap(),
            end_line: LineNumber::new(5).unwrap(),
            matcher: Some("parser-declared-license".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(1),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: None,
            rule_url: None,
            matched_text: Some("MIT".to_string()),
            referenced_filenames: Some(vec!["LICENSE".to_string()]),
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("unknown-ref".to_string()),
    }];

    let mut manifest = file("project/Cargo.toml");
    manifest.for_packages = vec![PackageUid::from_raw(package_uid.clone())];
    manifest.package_data = vec![PackageData {
        package_type: Some(PackageType::Cargo),
        license_detections: package.license_detections.clone(),
        ..Default::default()
    }];

    let mut license = file("project/LICENSE");
    license.license_expression = Some("mit".to_string());
    license.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/LICENSE".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(20).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(100),
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

    let mut files = vec![dir("project"), manifest, license];
    let mut packages = vec![package];
    apply_package_reference_following(&mut files, &mut packages);

    assert_eq!(
        packages[0].declared_license_expression.as_deref(),
        Some("mit")
    );
    assert_eq!(packages[0].license_detections[0].matches.len(), 2);
    assert_eq!(
        packages[0].license_detections[0].matches[1]
            .from_file
            .as_deref(),
        Some("project/LICENSE")
    );
    assert_eq!(
        files[1].package_data[0]
            .declared_license_expression
            .as_deref(),
        Some("mit")
    );
}

#[test]
fn apply_package_reference_following_resolves_absolute_rootfs_license_reference() {
    let mut common_license = file("usr/share/common-licenses/GPL-2");
    common_license.license_expression = Some("gpl-2.0".to_string());
    common_license.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "gpl-2.0".to_string(),
        license_expression_spdx: "GPL-2.0-only".to_string(),
        matches: vec![Match {
            license_expression: "gpl-2.0".to_string(),
            license_expression_spdx: "GPL-2.0-only".to_string(),
            from_file: Some("usr/share/common-licenses/GPL-2".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(339).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(2931),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("gpl-2.0.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("gpl-root".to_string()),
    }];

    let mut service = file("usr/sbin/service");
    service.license_expression = Some("gpl-2.0-plus".to_string());
    service.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "gpl-2.0-plus".to_string(),
        license_expression_spdx: "GPL-2.0-or-later".to_string(),
        matches: vec![Match {
            license_expression: "gpl-2.0-plus".to_string(),
            license_expression_spdx: "GPL-2.0-or-later".to_string(),
            from_file: Some("usr/sbin/service".to_string()),
            start_line: LineNumber::new(16).unwrap(),
            end_line: LineNumber::new(31).unwrap(),
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(139),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("gpl-2.0-plus_233.RULE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: Some(vec!["/usr/share/common-licenses/GPL-2".to_string()]),
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("service-gpl".to_string()),
    }];

    let mut files = vec![
        dir("usr"),
        dir("usr/sbin"),
        dir("usr/share"),
        dir("usr/share/common-licenses"),
        common_license,
        service,
    ];
    let mut packages = Vec::new();
    let snapshot = super::build_reference_follow_snapshot(&files, &packages);
    let resolved = super::resolve_referenced_resource(
        "/usr/share/common-licenses/GPL-2",
        "usr/sbin/service",
        &[],
        &snapshot,
    )
    .expect("absolute rootfs reference should resolve");
    assert_eq!(resolved.path, "usr/share/common-licenses/GPL-2");
    assert!(super::use_referenced_license_expression(
        Some("gpl-2.0"),
        &files[5].license_detections[0],
    ));

    apply_package_reference_following(&mut files, &mut packages);

    let service = files
        .iter()
        .find(|file| file.path == "usr/sbin/service")
        .expect("service file should exist");
    assert_eq!(
        service.license_expression.as_deref(),
        Some("gpl-2.0-plus AND gpl-2.0")
    );
    assert_eq!(
        service.license_detections[0].license_expression_spdx,
        "GPL-2.0-or-later AND GPL-2.0-only"
    );
    assert_eq!(service.license_detections[0].matches.len(), 2);
    assert_eq!(
        service.license_detections[0].matches[1]
            .from_file
            .as_deref(),
        Some("usr/share/common-licenses/GPL-2")
    );
    assert_eq!(
        service.license_detections[0].matches[1].license_expression_spdx,
        "GPL-2.0-only"
    );
}

#[test]
fn apply_package_reference_following_falls_back_to_root_when_package_missing() {
    let mut root_copying = file("project/COPYING");
    root_copying.license_expression = Some("gpl-3.0".to_string());
    root_copying.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "gpl-3.0".to_string(),
        license_expression_spdx: "GPL-3.0-only".to_string(),
        matches: vec![Match {
            license_expression: "gpl-3.0".to_string(),
            license_expression_spdx: "GPL-3.0-only".to_string(),
            from_file: Some("project/COPYING".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(10).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(50),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("gpl-3.0.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("gpl-root".to_string()),
    }];

    let mut po = file("project/po/en_US.po");
    po.license_expression = Some("unknown-license-reference".to_string());
    po.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "unknown-license-reference".to_string(),
        license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
        matches: vec![Match {
            license_expression: "unknown-license-reference".to_string(),
            license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
            from_file: Some("project/po/en_US.po".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(5),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("unknown-license-reference_see-license_1.RULE".to_string()),
            rule_url: None,
            matched_text: Some("same license as package".to_string()),
            referenced_filenames: Some(vec!["COPYING".to_string()]),
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("unknown-ref".to_string()),
    }];

    let mut files = vec![dir("project"), root_copying, po];
    let mut packages = Vec::new();
    apply_package_reference_following(&mut files, &mut packages);

    let po = files
        .iter()
        .find(|file| file.path == "project/po/en_US.po")
        .expect("po file should exist");
    assert_eq!(po.license_expression.as_deref(), Some("gpl-3.0"));
    assert_eq!(
        po.license_detections[0].detection_log,
        vec!["unknown-reference-to-local-file"]
    );
}

#[test]
fn apply_package_reference_following_prefers_nearest_ancestor_license_file() {
    let mut repo_root_license = file("project/LICENSE");
    repo_root_license.license_expression = Some("mit".to_string());
    repo_root_license.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
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
        identifier: Some("mit-root".to_string()),
    }];

    let mut nested_license = file("project/java/LICENSE");
    nested_license.license_expression = Some("apache-2.0".to_string());
    nested_license.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "apache-2.0".to_string(),
        license_expression_spdx: "Apache-2.0".to_string(),
        matches: vec![Match {
            license_expression: "apache-2.0".to_string(),
            license_expression_spdx: "Apache-2.0".to_string(),
            from_file: Some("project/java/LICENSE".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(17).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(120),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("apache-2.0.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("apache-java".to_string()),
    }];

    let mut source = file("project/java/src/com/example/Callback.java");
    source.license_expression = Some("mit".to_string());
    source.license_detections = vec![
        crate::models::LicenseDetection {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            matches: vec![Match {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                from_file: Some("project/java/src/com/example/Callback.java".to_string()),
                start_line: LineNumber::new(4).unwrap(),
                end_line: LineNumber::new(5).unwrap(),
                matcher: Some("2-aho".to_string()),
                score: MatchScore::MAX,
                matched_length: Some(22),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: Some("mit_101.RULE".to_string()),
                rule_url: None,
                matched_text: Some("licensed under the MIT license".to_string()),
                referenced_filenames: Some(vec!["LICENSE".to_string()]),
                matched_text_diagnostics: None,
            }],
            detection_log: vec![],
            identifier: Some("source-mit".to_string()),
        },
        crate::models::LicenseDetection {
            license_expression: "apache-2.0".to_string(),
            license_expression_spdx: "Apache-2.0".to_string(),
            matches: vec![Match {
                license_expression: "apache-2.0".to_string(),
                license_expression_spdx: "Apache-2.0".to_string(),
                from_file: Some("project/java/src/com/example/Callback.java".to_string()),
                start_line: LineNumber::new(12).unwrap(),
                end_line: LineNumber::new(22).unwrap(),
                matcher: Some("2-aho".to_string()),
                score: MatchScore::MAX,
                matched_length: Some(85),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: Some("apache-2.0_7.RULE".to_string()),
                rule_url: None,
                matched_text: None,
                referenced_filenames: None,
                matched_text_diagnostics: None,
            }],
            detection_log: vec![],
            identifier: Some("source-apache".to_string()),
        },
    ];

    let mut files = vec![
        dir("project"),
        dir("project/java"),
        dir("project/java/src"),
        dir("project/java/src/com"),
        dir("project/java/src/com/example"),
        repo_root_license,
        nested_license,
        source,
    ];
    let mut packages = Vec::new();

    let snapshot = super::build_reference_follow_snapshot(&files, &packages);
    let resolved = super::resolve_referenced_resource(
        "LICENSE",
        "project/java/src/com/example/Callback.java",
        &[],
        &snapshot,
    )
    .expect("nearest ancestor LICENSE should resolve");
    assert_eq!(resolved.path, "project/java/LICENSE");

    apply_package_reference_following(&mut files, &mut packages);

    let source = files
        .iter()
        .find(|file| file.path == "project/java/src/com/example/Callback.java")
        .expect("source file should exist");
    assert_eq!(
        source.license_expression.as_deref(),
        Some("apache-2.0 AND mit")
    );
    assert_eq!(source.license_detections.len(), 2);
    let combined = source
        .license_detections
        .iter()
        .find(|detection| detection.license_expression_spdx == "Apache-2.0 AND MIT")
        .expect("combined followed detection should exist");
    assert_eq!(combined.detection_log, ["unknown-reference-to-local-file"]);
    assert!(
        source
            .license_detections
            .iter()
            .any(|detection| detection.license_expression_spdx == "Apache-2.0")
    );
    assert!(combined.matches.iter().any(|detection_match| {
        detection_match.from_file.as_deref() == Some("project/java/LICENSE")
    }));
}

#[test]
fn apply_package_reference_following_falls_back_past_nested_root_to_repo_root() {
    let mut root_license = file("LICENSE");
    root_license.license_expression = Some("mit".to_string());
    root_license.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("LICENSE".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(20).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(100),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("mit.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("mit-root".to_string()),
    }];

    let mut manpage = file("docs/man-xlate/nmap-id.1");
    manpage.license_expression = Some("unknown-license-reference".to_string());
    manpage.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "unknown-license-reference".to_string(),
        license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
        matches: vec![Match {
            license_expression: "unknown-license-reference".to_string(),
            license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
            from_file: Some("docs/man-xlate/nmap-id.1".to_string()),
            start_line: LineNumber::new(100).unwrap(),
            end_line: LineNumber::new(100).unwrap(),
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(2),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("unknown-license-reference_see-license_1.RULE".to_string()),
            rule_url: None,
            matched_text: Some("See LICENSE".to_string()),
            referenced_filenames: Some(vec!["LICENSE".to_string()]),
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("manpage-ref".to_string()),
    }];

    let mut files = vec![dir("docs"), dir("docs/man-xlate"), root_license, manpage];
    let mut packages = Vec::new();
    apply_package_reference_following(&mut files, &mut packages);

    let manpage = files
        .iter()
        .find(|file| file.path == "docs/man-xlate/nmap-id.1")
        .expect("manpage file should exist");
    assert_eq!(manpage.license_expression.as_deref(), Some("mit"));
    assert_eq!(
        manpage.license_detections[0].detection_log,
        vec!["unknown-reference-to-local-file"]
    );
    assert_eq!(
        manpage.license_detections[0].matches[1]
            .from_file
            .as_deref(),
        Some("LICENSE")
    );
}

#[test]
fn apply_package_reference_following_inherits_license_from_package_context() {
    let package_uid = "pkg:pypi/demo?uuid=test".to_string();
    let mut package = super::test_utils::package(&package_uid, "project/PKG-INFO");
    package.datafile_paths = vec!["project/PKG-INFO".to_string()];
    package.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "bsd-new".to_string(),
        license_expression_spdx: "BSD-3-Clause".to_string(),
        matches: vec![Match {
            license_expression: "bsd-new".to_string(),
            license_expression_spdx: "BSD-3-Clause".to_string(),
            from_file: Some("project/PKG-INFO".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("1-hash".to_string()),
            score: MatchScore::from_percentage(99.0),
            matched_length: Some(5),
            match_coverage: Some(100.0),
            rule_relevance: Some(99),
            rule_identifier: Some("pypi_bsd_license.RULE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("package-license".to_string()),
    }];

    let mut source = file("project/locale/django.po");
    source.for_packages = vec![PackageUid::from_raw(package_uid.clone())];
    source.license_expression = Some("free-unknown".to_string());
    source.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "free-unknown".to_string(),
        license_expression_spdx: "LicenseRef-scancode-free-unknown".to_string(),
        matches: vec![Match {
            license_expression: "free-unknown".to_string(),
            license_expression_spdx: "LicenseRef-scancode-free-unknown".to_string(),
            from_file: Some("project/locale/django.po".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(11),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("free-unknown-package_1.RULE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: Some(vec!["INHERIT_LICENSE_FROM_PACKAGE".to_string()]),
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("unknown-package-ref".to_string()),
    }];

    let mut files = vec![dir("project"), source];
    let mut packages = vec![package];
    apply_package_reference_following(&mut files, &mut packages);

    let source = files
        .iter()
        .find(|file| file.path == "project/locale/django.po")
        .expect("source file should exist");
    assert_eq!(source.license_expression.as_deref(), Some("bsd-new"));
    assert_eq!(
        source.license_detections[0].detection_log,
        vec!["unknown-reference-in-file-to-package"]
    );
    assert_eq!(source.license_detections[0].matches.len(), 2);
    assert_eq!(
        source.license_detections[0].matches[1].from_file.as_deref(),
        Some("project/PKG-INFO")
    );
}

#[test]
fn apply_package_reference_following_falls_back_to_root_for_missing_package_reference() {
    let mut root_copying = file("project/COPYING");
    root_copying.license_expression = Some("gpl-3.0".to_string());
    root_copying.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "gpl-3.0".to_string(),
        license_expression_spdx: "GPL-3.0-only".to_string(),
        matches: vec![Match {
            license_expression: "gpl-3.0".to_string(),
            license_expression_spdx: "GPL-3.0-only".to_string(),
            from_file: Some("project/COPYING".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(10).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(50),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("gpl-3.0.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("gpl-root".to_string()),
    }];

    let mut po = file("project/po/en_US.po");
    po.license_expression = Some("free-unknown".to_string());
    po.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "free-unknown".to_string(),
        license_expression_spdx: "LicenseRef-scancode-free-unknown".to_string(),
        matches: vec![Match {
            license_expression: "free-unknown".to_string(),
            license_expression_spdx: "LicenseRef-scancode-free-unknown".to_string(),
            from_file: Some("project/po/en_US.po".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(5),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("free-unknown-package_2.RULE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: Some(vec!["INHERIT_LICENSE_FROM_PACKAGE".to_string()]),
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("unknown-package-ref".to_string()),
    }];

    let mut files = vec![dir("project"), root_copying, po];
    let mut packages = Vec::new();
    apply_package_reference_following(&mut files, &mut packages);

    let po = files
        .iter()
        .find(|file| file.path == "project/po/en_US.po")
        .expect("po file should exist");
    assert_eq!(po.license_expression.as_deref(), Some("gpl-3.0"));
    assert_eq!(
        po.license_detections[0].detection_log,
        vec!["unknown-reference-in-file-to-nonexistent-package"]
    );
    assert_eq!(
        po.license_detections[0].matches[1].from_file.as_deref(),
        Some("project/COPYING")
    );
}

#[test]
fn apply_package_reference_following_leaves_ambiguous_multi_package_file_unresolved() {
    let first_uid = "pkg:pypi/demo-a?uuid=test".to_string();
    let second_uid = "pkg:pypi/demo-b?uuid=test".to_string();

    let mut first_package = super::test_utils::package(&first_uid, "project/a/PKG-INFO");
    first_package.datafile_paths = vec!["project/a/PKG-INFO".to_string()];
    first_package.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/a/PKG-INFO".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(5),
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

    let mut second_package = super::test_utils::package(&second_uid, "project/b/PKG-INFO");
    second_package.datafile_paths = vec!["project/b/PKG-INFO".to_string()];
    second_package.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "apache-2.0".to_string(),
        license_expression_spdx: "Apache-2.0".to_string(),
        matches: vec![Match {
            license_expression: "apache-2.0".to_string(),
            license_expression_spdx: "Apache-2.0".to_string(),
            from_file: Some("project/b/PKG-INFO".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(5),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("apache-2.0.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("apache-license".to_string()),
    }];

    let mut shared_file = file("project/shared/locale.po");
    shared_file.for_packages = vec![
        PackageUid::from_raw(first_uid),
        PackageUid::from_raw(second_uid),
    ];
    shared_file.license_expression = Some("free-unknown".to_string());
    shared_file.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "free-unknown".to_string(),
        license_expression_spdx: "LicenseRef-scancode-free-unknown".to_string(),
        matches: vec![Match {
            license_expression: "free-unknown".to_string(),
            license_expression_spdx: "LicenseRef-scancode-free-unknown".to_string(),
            from_file: Some("project/shared/locale.po".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(11),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("free-unknown-package_1.RULE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: Some(vec!["INHERIT_LICENSE_FROM_PACKAGE".to_string()]),
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("unknown-package-ref".to_string()),
    }];

    let mut files = vec![dir("project"), shared_file];
    let mut packages = vec![first_package, second_package];
    apply_package_reference_following(&mut files, &mut packages);

    let shared_file = files
        .iter()
        .find(|file| file.path == "project/shared/locale.po")
        .expect("shared file should exist");
    assert_eq!(
        shared_file.license_expression.as_deref(),
        Some("free-unknown")
    );
    assert_eq!(shared_file.license_detections[0].matches.len(), 1);
    assert!(shared_file.license_detections[0].detection_log.is_empty());
}

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

#[test]
fn create_output_gates_summary_tallies_and_generated_sections() {
    let license_rel = "project/LICENSE".to_string();
    let mut disabled_license = file(&license_rel);
    disabled_license.is_generated = Some(true);
    disabled_license.tallies = Some(Tallies::default());

    let start = Utc::now();
    let end = start;
    let output_without_flags = create_output(
        start,
        end,
        crate::scanner::ProcessResult {
            files: vec![dir("project"), disabled_license],
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
    assert!(output_without_flags.summary.is_none());
    assert!(output_without_flags.tallies.is_none());
    assert!(output_without_flags.tallies_of_key_files.is_none());
    assert!(
        output_without_flags
            .files
            .iter()
            .all(|file| file.is_generated.is_none())
    );

    let mut enabled_license = file(&license_rel);
    enabled_license.is_generated = Some(true);
    enabled_license.license_expression = Some("mit".to_string());
    enabled_license.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some(license_rel.clone()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(10),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: None,
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        identifier: None,
        detection_log: vec![],
    }];

    let output_with_flags = create_output(
        start,
        end,
        crate::scanner::ProcessResult {
            files: vec![dir("project"), enabled_license],
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
            extra_errors: vec![],
            extra_warnings: vec![],
            header_options: serde_json::Map::new(),
            options: CreateOutputOptions {
                facet_rules: &[],
                include_classify: false,
                include_tallies_by_facet: false,
                include_summary: true,
                include_license_clarity_score: true,
                include_tallies: true,
                include_tallies_with_details: true,
                include_tallies_of_key_files: true,
                include_generated: true,
                verbose: false,
            },
        },
    );
    assert!(output_with_flags.summary.is_some());
    assert!(output_with_flags.tallies.is_some());
    assert!(output_with_flags.tallies_of_key_files.is_some());
    assert!(
        output_with_flags
            .files
            .iter()
            .find(|file| file.path == license_rel)
            .is_some_and(|file| file.is_generated == Some(true) && file.tallies.is_some())
    );
}

#[test]
fn create_output_preserves_scanner_generated_flags_without_scan_root() {
    let start = Utc::now();
    let end = start;

    let mut generated = file("project/generated.c");
    generated.is_generated = Some(true);

    let mut plain = file("project/plain.c");
    plain.is_generated = Some(false);

    let mut missing = file("project/missing.c");
    missing.is_generated = None;

    let output = create_output(
        start,
        end,
        crate::scanner::ProcessResult {
            files: vec![dir("project"), generated, plain, missing],
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
                include_generated: true,
                verbose: false,
            },
        },
    );

    let generated_flags: Vec<_> = output
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.is_generated))
        .collect();

    assert_eq!(
        generated_flags,
        vec![
            ("project", Some(false)),
            ("project/generated.c", Some(true)),
            ("project/plain.c", Some(false)),
            ("project/missing.c", Some(false)),
        ]
    );
}

#[test]
fn create_output_score_only_keeps_clarity_without_full_summary_fields() {
    let start = Utc::now();
    let end = start;
    let mut license = file("project/LICENSE");
    license.license_expression = Some("mit".to_string());
    license.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/LICENSE".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(10),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: None,
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        identifier: None,
        detection_log: vec![],
    }];

    let output = create_output(
        start,
        end,
        crate::scanner::ProcessResult {
            files: vec![dir("project"), license],
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
            extra_errors: vec![],
            extra_warnings: vec![],
            header_options: serde_json::Map::new(),
            options: CreateOutputOptions {
                facet_rules: &[],
                include_classify: false,
                include_tallies_by_facet: false,
                include_summary: false,
                include_license_clarity_score: true,
                include_tallies: false,
                include_tallies_with_details: false,
                include_tallies_of_key_files: false,
                include_generated: false,
                verbose: false,
            },
        },
    );

    let summary = output.summary.expect("score-only summary exists");
    assert_eq!(summary.declared_license_expression.as_deref(), Some("mit"));
    assert!(summary.license_clarity_score.is_some());
    assert!(summary.declared_holder.is_none());
    assert!(summary.primary_language.is_none());
    assert!(summary.other_license_expressions.is_empty());
    assert!(summary.other_holders.is_empty());
    assert!(summary.other_languages.is_empty());
}

#[test]
fn create_output_preserves_file_level_license_clues_in_json_shape() {
    let start = Utc::now();
    let end = start;
    let mut clue_file = file("project/NOTICE");
    clue_file.license_clues = vec![Match {
        license_expression: "unknown-license-reference".to_string(),
        license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
        from_file: Some("project/NOTICE".to_string()),
        start_line: LineNumber::ONE,
        end_line: LineNumber::new(2).unwrap(),
        matcher: Some("2-aho".to_string()),
        score: MatchScore::MAX,
        matched_length: Some(19),
        match_coverage: Some(100.0),
        rule_relevance: Some(100),
        rule_identifier: Some("license-clue_1.RULE".to_string()),
        rule_url: Some("https://example.com/license-clue_1.RULE".to_string()),
        matched_text: Some(
            "This product currently only contains code developed by authors".to_string(),
        ),
        referenced_filenames: None,
        matched_text_diagnostics: None,
    }];

    let output = create_output(
        start,
        end,
        crate::scanner::ProcessResult {
            files: vec![dir("project"), clue_file],
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

    let value = serde_json::to_value(crate::output_schema::Output::from(&output))
        .expect("output should serialize");
    let notice = value["files"]
        .as_array()
        .expect("files array")
        .iter()
        .find(|entry| entry["path"] == json!("project/NOTICE"))
        .expect("notice file present");

    assert_eq!(notice["license_detections"], json!([]));
    assert_eq!(
        notice["detected_license_expression_spdx"],
        serde_json::Value::Null
    );
    assert_eq!(
        notice["license_clues"][0]["license_expression"],
        "unknown-license-reference"
    );
    assert_eq!(notice["license_clues"][0]["matcher"], "2-aho");
}

#[test]
fn create_output_preserves_empty_package_data_license_and_dependency_arrays() {
    let start = Utc::now();
    let end = start;
    let mut manifest = file("project/package.json");
    manifest.package_data = vec![PackageData {
        package_type: Some(PackageType::Npm),
        name: Some("demo".to_string()),
        version: Some("1.0.0".to_string()),
        ..PackageData::default()
    }];

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

    let value = serde_json::to_value(crate::output_schema::Output::from(&output))
        .expect("output should serialize");
    let package_data = value["files"]
        .as_array()
        .expect("files array")
        .iter()
        .find(|entry| entry["path"] == json!("project/package.json"))
        .and_then(|entry| entry["package_data"].as_array())
        .and_then(|package_data| package_data.first())
        .expect("package data entry present");

    assert_eq!(package_data["license_detections"], json!([]));
    assert_eq!(package_data["dependencies"], json!([]));
}

#[test]
fn create_output_tallies_by_facet_does_not_leak_resource_tallies() {
    let start = Utc::now();
    let end = start;
    let mut source = file("project/src/lib.rs");
    source.programming_language = Some("Rust".to_string());

    let facet_defs = ["dev=*.rs".to_string()];
    let facet_rules = build_facet_rules(&facet_defs).expect("facet rules compile");

    let output = create_output(
        start,
        end,
        crate::scanner::ProcessResult {
            files: vec![dir("project"), dir("project/src"), source],
            excluded_count: 0,
        },
        CreateOutputContext {
            total_dirs: 2,
            assembly_result: assembly::AssemblyResult {
                packages: vec![],
                dependencies: vec![],
            },
            license_detections: vec![],
            license_references: vec![],
            license_rule_references: vec![],
            spdx_license_list_version: "3.27".to_string(),
            extra_errors: vec![],
            extra_warnings: vec![],
            header_options: serde_json::Map::new(),
            options: CreateOutputOptions {
                facet_rules: &facet_rules,
                include_classify: false,
                include_tallies_by_facet: true,
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

    assert!(output.tallies_by_facet.is_some());
    assert!(output.files.iter().all(|file| file.tallies.is_none()));
}

#[test]
fn create_output_promotes_package_metadata_without_summary_flags() {
    let start = Utc::now();
    let end = start;
    let package_uid = "pkg:npm/demo?uuid=test".to_string();
    let mut license = file("project/LICENSE");
    license.for_packages = vec![PackageUid::from_raw(package_uid.clone())];
    license.copyrights = vec![Copyright {
        copyright: "Copyright Example Corp.".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];
    license.holders = vec![Holder {
        holder: "Example Corp.".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];
    let package = Package {
        package_uid: PackageUid::from_raw(package_uid),
        datafile_paths: vec!["project/package.json".to_string()],
        ..super::test_utils::package("pkg:npm/demo?uuid=test", "project/package.json")
    };

    let output = create_output(
        start,
        end,
        crate::scanner::ProcessResult {
            files: vec![dir("project"), license],
            excluded_count: 0,
        },
        CreateOutputContext {
            total_dirs: 1,
            assembly_result: assembly::AssemblyResult {
                packages: vec![package],
                dependencies: vec![],
            },
            license_detections: vec![],
            license_references: vec![],
            license_rule_references: vec![],
            spdx_license_list_version: "3.27".to_string(),
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

    assert_eq!(output.packages[0].holder.as_deref(), Some("Example Corp."));
    assert_eq!(
        output.packages[0].copyright.as_deref(),
        Some("Copyright Example Corp.")
    );
}

#[test]
fn create_output_summary_still_resolves_after_strip_root_normalization() {
    let start = Utc::now();
    let end = start;
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let root = temp.path().join("project");
    let manifest_path = root.join("demo.gemspec");
    std::fs::create_dir_all(&root).expect("root should exist");

    let mut manifest = file(manifest_path.to_str().unwrap());
    manifest.package_data = vec![crate::models::PackageData {
        package_type: Some(crate::models::PackageType::Gem),
        datasource_id: Some(crate::models::DatasourceId::Gemspec),
        declared_license_expression: Some("mit".to_string()),
        declared_license_expression_spdx: Some("MIT".to_string()),
        purl: Some("pkg:gem/demo@1.0.0".to_string()),
        ..Default::default()
    }];
    manifest.license_expression = Some("mit".to_string());
    manifest.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/demo.gemspec".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("1-spdx-id".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(1),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: None,
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        identifier: None,
        detection_log: vec![],
    }];

    let mut files = vec![dir(root.to_str().unwrap()), manifest];
    normalize_paths(&mut files, root.to_str().unwrap(), true, false);
    let assembly_result = assembly::assemble(&mut files);

    let output = create_output(
        start,
        end,
        crate::scanner::ProcessResult {
            files,
            excluded_count: 0,
        },
        CreateOutputContext {
            total_dirs: 1,
            assembly_result,
            license_detections: vec![],
            license_references: vec![],
            license_rule_references: vec![],
            spdx_license_list_version: "3.27".to_string(),
            extra_errors: vec![],
            extra_warnings: vec![],
            header_options: serde_json::Map::new(),
            options: CreateOutputOptions {
                facet_rules: &[],
                include_classify: false,
                include_tallies_by_facet: false,
                include_summary: true,
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
        output
            .summary
            .and_then(|summary| summary.declared_license_expression),
        Some("mit".to_string())
    );
}

#[test]
fn create_output_classify_only_sets_key_file_flags() {
    let start = Utc::now();
    let end = start;

    let output = create_output(
        start,
        end,
        crate::scanner::ProcessResult {
            files: vec![dir("project"), file("project/README.md")],
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
            extra_errors: vec![],
            extra_warnings: vec![],
            header_options: serde_json::Map::new(),
            options: CreateOutputOptions {
                facet_rules: &[],
                include_classify: true,
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

    let readme = output
        .files
        .iter()
        .find(|file| file.path == "project/README.md")
        .expect("README should exist");

    assert!(readme.is_readme);
    assert!(readme.is_top_level);
    assert!(readme.is_key_file);
}

#[test]
fn create_output_uses_scancode_header_timestamp_format() {
    let start = Utc
        .with_ymd_and_hms(2026, 4, 11, 9, 18, 28)
        .single()
        .expect("timestamp should be valid")
        .with_nanosecond(24_390_124)
        .expect("nanoseconds should be valid");
    let end = Utc
        .with_ymd_and_hms(2026, 4, 11, 9, 18, 29)
        .single()
        .expect("timestamp should be valid")
        .with_nanosecond(987_654_321)
        .expect("nanoseconds should be valid");

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

    let header = output.headers.first().expect("header should exist");
    assert_eq!(header.start_timestamp, "2026-04-11T091828.024390");
    assert_eq!(header.end_timestamp, "2026-04-11T091829.987654");
}
