// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
