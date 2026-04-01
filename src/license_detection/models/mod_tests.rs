#[cfg(test)]
mod tests {
    use crate::license_detection::index::LicenseIndex;
    use crate::license_detection::index::dictionary::tid;
    use crate::license_detection::models::{License, LicenseMatch, MatcherKind, Rule, RuleKind};
    use crate::models::Match as OutputMatch;
    use std::collections::HashMap;

    fn create_test_index() -> LicenseIndex {
        LicenseIndex::with_legalese_count(10)
    }

    fn create_license() -> License {
        License {
            key: "mit".to_string(),
            short_name: Some("MIT".to_string()),
            name: "MIT License".to_string(),
            language: Some("en".to_string()),
            spdx_license_key: Some("MIT".to_string()),
            other_spdx_license_keys: vec![],
            category: Some("Permissive".to_string()),
            owner: Some("Example Owner".to_string()),
            homepage_url: Some("https://opensource.org/licenses/MIT".to_string()),
            text: "MIT License text here...".to_string(),
            reference_urls: vec!["https://opensource.org/licenses/MIT".to_string()],
            osi_license_key: Some("MIT".to_string()),
            text_urls: vec!["https://opensource.org/licenses/MIT".to_string()],
            osi_url: Some("https://opensource.org/licenses/MIT".to_string()),
            faq_url: None,
            other_urls: vec![],
            notes: None,
            is_deprecated: false,
            is_exception: false,
            is_unknown: false,
            is_generic: false,
            replaced_by: vec![],
            minimum_coverage: None,
            standard_notice: None,
            ignorable_copyrights: None,
            ignorable_holders: None,
            ignorable_authors: None,
            ignorable_urls: None,
            ignorable_emails: None,
        }
    }

    fn create_rule() -> Rule {
        Rule {
            identifier: "mit_123.RULE".to_string(),
            license_expression: "mit".to_string(),
            text: "MIT License".to_string(),
            tokens: vec![],
            rule_kind: RuleKind::Notice,
            is_false_positive: false,
            is_required_phrase: false,
            is_from_license: false,
            relevance: 90,
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

    fn create_license_match() -> LicenseMatch {
        LicenseMatch {
            rid: 0,
            license_expression: "mit".to_string(),
            license_expression_spdx: Some("MIT".to_string()),
            from_file: Some("README.md".to_string()),
            start_line: 1,
            end_line: 5,
            start_token: 0,
            end_token: 100,
            matcher: crate::license_detection::models::MatcherKind::Hash,
            score: 0.95,
            matched_length: 100,
            rule_length: 100,
            matched_token_positions: None,
            match_coverage: 95.0,
            rule_relevance: 100,
            rule_identifier: "mit.LICENSE".to_string(),
            rule_url: "https://scancode-licensedb.aboutcode.org/mit".to_string(),
            matched_text: Some("MIT License text...".to_string()),
            referenced_filenames: None,
            rule_kind: RuleKind::None,
            is_from_license: false,
            hilen: 50,
            rule_start_token: 0,
            qspan_positions: None,
            ispan_positions: None,
            hispan_positions: None,
            candidate_resemblance: 0.0,
            candidate_containment: 0.0,
        }
    }

    #[test]
    fn test_license_creation_with_all_fields() {
        let license = create_license();

        assert_eq!(license.key, "mit");
        assert_eq!(license.name, "MIT License");
        assert_eq!(license.spdx_license_key, Some("MIT".to_string()));
        assert_eq!(license.category, Some("Permissive".to_string()));
        assert!(!license.is_deprecated);
        assert!(license.replaced_by.is_empty());
    }

    #[test]
    fn test_license_creation_with_minimal_fields() {
        let license = License {
            key: "unknown".to_string(),
            short_name: None,
            name: String::new(),
            language: None,
            spdx_license_key: None,
            other_spdx_license_keys: vec![],
            category: None,
            owner: None,
            homepage_url: None,
            text: String::new(),
            reference_urls: vec![],
            osi_license_key: None,
            text_urls: vec![],
            osi_url: None,
            faq_url: None,
            other_urls: vec![],
            notes: None,
            is_deprecated: false,
            is_exception: false,
            is_unknown: true,
            is_generic: false,
            replaced_by: vec![],
            minimum_coverage: None,
            standard_notice: None,
            ignorable_copyrights: None,
            ignorable_holders: None,
            ignorable_authors: None,
            ignorable_urls: None,
            ignorable_emails: None,
        };

        assert_eq!(license.key, "unknown");
        assert!(license.name.is_empty());
        assert!(license.spdx_license_key.is_none());
        assert!(license.reference_urls.is_empty());
    }

    #[test]
    fn test_license_deprecated_with_replaced_by() {
        let license = License {
            key: "old-license".to_string(),
            short_name: Some("Old License".to_string()),
            name: "Old License".to_string(),
            language: Some("en".to_string()),
            spdx_license_key: None,
            other_spdx_license_keys: vec![],
            category: None,
            owner: None,
            homepage_url: None,
            text: String::new(),
            reference_urls: vec![],
            osi_license_key: None,
            text_urls: vec![],
            osi_url: None,
            faq_url: None,
            other_urls: vec![],
            notes: Some("Deprecated in favor of new-license".to_string()),
            is_deprecated: true,
            is_exception: false,
            is_unknown: false,
            is_generic: false,
            replaced_by: vec!["new-license".to_string()],
            minimum_coverage: None,
            standard_notice: None,
            ignorable_copyrights: None,
            ignorable_holders: None,
            ignorable_authors: None,
            ignorable_urls: None,
            ignorable_emails: None,
        };

        assert!(license.is_deprecated);
        assert_eq!(license.replaced_by, vec!["new-license"]);
    }

    #[test]
    fn test_license_with_ignorable_fields() {
        let license = License {
            key: "apache-2.0".to_string(),
            short_name: Some("Apache 2.0".to_string()),
            name: "Apache 2.0".to_string(),
            language: Some("en".to_string()),
            spdx_license_key: Some("Apache-2.0".to_string()),
            other_spdx_license_keys: vec![],
            category: Some("Permissive".to_string()),
            owner: Some("Apache Software Foundation".to_string()),
            homepage_url: Some("https://www.apache.org/licenses/LICENSE-2.0".to_string()),
            text: "Apache License text...".to_string(),
            reference_urls: vec![],
            osi_license_key: Some("Apache-2.0".to_string()),
            text_urls: vec![],
            osi_url: None,
            faq_url: None,
            other_urls: vec![],
            notes: None,
            is_deprecated: false,
            is_exception: false,
            is_unknown: false,
            is_generic: false,
            replaced_by: vec![],
            minimum_coverage: Some(95),
            standard_notice: None,
            ignorable_copyrights: Some(vec!["Copyright 2000 Apache".to_string()]),
            ignorable_holders: Some(vec!["Apache Software Foundation".to_string()]),
            ignorable_authors: Some(vec!["Apache".to_string()]),
            ignorable_urls: Some(vec!["https://apache.org".to_string()]),
            ignorable_emails: Some(vec!["legal@apache.org".to_string()]),
        };

        assert_eq!(license.minimum_coverage, Some(95));
        assert_eq!(
            license.ignorable_copyrights,
            Some(vec!["Copyright 2000 Apache".to_string()])
        );
        assert_eq!(
            license.ignorable_holders,
            Some(vec!["Apache Software Foundation".to_string()])
        );
    }

    #[test]
    fn test_rule_creation_with_all_fields() {
        let rule = create_rule();

        assert_eq!(rule.identifier, "mit_123.RULE");
        assert_eq!(rule.license_expression, "mit");
        assert!(rule.is_license_notice());
        assert!(!rule.is_license_text());
        assert!(!rule.is_license_reference());
        assert!(!rule.is_license_tag());
        assert_eq!(rule.relevance, 90);
        assert_eq!(
            rule.rule_url(),
            Some(
                "https://github.com/nexB/scancode-toolkit/tree/develop/src/licensedcode/data/rules/mit_123.RULE"
                    .to_string()
            )
        );
    }

    #[test]
    fn test_rule_url_uses_license_expression_for_license_rules() {
        let mut rule = create_rule();
        rule.identifier = "mit.LICENSE".to_string();
        rule.is_from_license = true;

        assert_eq!(
            rule.rule_url(),
            Some(
                "https://github.com/nexB/scancode-toolkit/tree/develop/src/licensedcode/data/licenses/mit.LICENSE"
                    .to_string()
            )
        );
    }

    #[test]
    fn test_rule_url_uses_rules_directory_when_not_from_license() {
        let mut rule = create_rule();
        rule.identifier = "mit.LICENSE".to_string();
        rule.is_from_license = false;

        assert_eq!(
            rule.rule_url(),
            Some(
                "https://github.com/nexB/scancode-toolkit/tree/develop/src/licensedcode/data/rules/mit.LICENSE"
                    .to_string()
            )
        );
    }

    #[test]
    fn test_rule_url_is_absent_for_empty_identifier() {
        let mut rule = create_rule();
        rule.identifier.clear();

        assert_eq!(rule.rule_url(), None);
    }

    #[test]
    fn test_rule_creation_with_minimal_fields() {
        let rule = Rule {
            identifier: String::new(),
            license_expression: String::new(),
            text: String::new(),
            tokens: vec![],
            rule_kind: RuleKind::None,
            is_false_positive: false,
            is_required_phrase: false,
            is_from_license: false,
            relevance: 0,
            minimum_coverage: None,
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
        };

        assert!(rule.identifier.is_empty());
        assert!(rule.license_expression.is_empty());
        assert_eq!(rule.relevance, 0);
    }

    #[test]
    fn test_rule_license_flags_mutually_exclusive() {
        let mut rule = create_rule();
        assert!(rule.is_license_notice());

        rule.rule_kind = RuleKind::Text;
        assert!(rule.is_license_text());
        assert!(!rule.is_license_notice());

        let flag_count = [
            rule.is_license_text(),
            rule.is_license_notice(),
            rule.is_license_reference(),
            rule.is_license_tag(),
            rule.is_license_intro(),
            rule.is_license_clue(),
        ]
        .iter()
        .filter(|&&f| f)
        .count();
        assert_eq!(flag_count, 1);
    }

    #[test]
    fn test_rule_with_tokens() {
        let rule = Rule {
            identifier: "test.RULE".to_string(),
            license_expression: "test".to_string(),
            text: "test text".to_string(),
            tokens: vec![tid(1), tid(2), tid(3), tid(4), tid(5)],
            rule_kind: RuleKind::Notice,
            is_false_positive: false,
            is_required_phrase: false,
            is_from_license: false,
            relevance: 100,
            minimum_coverage: None,
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
            language: None,
            notes: None,
            length_unique: 5,
            high_length_unique: 0,
            high_length: 0,
            min_matched_length: 0,
            min_high_matched_length: 0,
            min_matched_length_unique: 0,
            min_high_matched_length_unique: 0,
            is_small: true,
            is_tiny: false,
            starts_with_license: false,
            ends_with_license: false,
            is_deprecated: false,
            spdx_license_key: None,
            other_spdx_license_keys: vec![],
        };

        assert_eq!(rule.tokens.len(), 5);
        assert!(rule.is_small);
    }

    #[test]
    fn test_rule_small_and_tiny_flags() {
        let mut rule = create_rule();

        rule.is_small = true;
        rule.is_tiny = true;
        assert!(rule.is_small);
        assert!(rule.is_tiny);

        rule.is_tiny = false;
        assert!(rule.is_small);
        assert!(!rule.is_tiny);
    }

    #[test]
    fn test_rule_threshold_fields() {
        let rule = Rule {
            identifier: "complex.RULE".to_string(),
            license_expression: "complex".to_string(),
            text: "complex text".to_string(),
            tokens: vec![],
            rule_kind: crate::license_detection::models::RuleKind::Notice,
            is_false_positive: false,
            is_required_phrase: false,
            is_from_license: false,
            relevance: 100,
            minimum_coverage: Some(80),
            has_stored_minimum_coverage: false,
            is_continuous: true,
            referenced_filenames: Some(vec!["LICENSE".to_string()]),
            ignorable_urls: Some(vec!["https://example.com".to_string()]),
            ignorable_emails: Some(vec!["test@example.com".to_string()]),
            ignorable_copyrights: None,
            ignorable_holders: None,
            ignorable_authors: None,
            language: Some("en".to_string()),
            notes: Some("Test rule".to_string()),
            length_unique: 10,
            high_length_unique: 5,
            high_length: 8,
            min_matched_length: 4,
            min_high_matched_length: 2,
            min_matched_length_unique: 3,
            min_high_matched_length_unique: 1,
            is_small: false,
            is_tiny: false,
            starts_with_license: false,
            ends_with_license: false,
            is_deprecated: false,
            spdx_license_key: None,
            other_spdx_license_keys: vec![],
            required_phrase_spans: vec![],
            stopwords_by_pos: std::collections::HashMap::new(),
        };

        assert_eq!(rule.minimum_coverage, Some(80));
        assert_eq!(rule.referenced_filenames, Some(vec!["LICENSE".to_string()]));
        assert_eq!(rule.length_unique, 10);
        assert_eq!(rule.high_length, 8);
        assert_eq!(rule.min_matched_length, 4);
    }

    #[test]
    fn test_license_match_creation_with_all_fields() {
        let match_result = create_license_match();

        assert_eq!(match_result.license_expression, "mit");
        assert_eq!(
            match_result.license_expression_spdx,
            Some("MIT".to_string())
        );
        assert_eq!(match_result.from_file, Some("README.md".to_string()));
        assert_eq!(match_result.start_line, 1);
        assert_eq!(match_result.end_line, 5);
        assert_eq!(match_result.matcher, MatcherKind::Hash);
        assert!((match_result.score - 0.95).abs() < 0.001);
    }

    #[test]
    fn test_license_match_creation_with_minimal_fields() {
        let match_result = LicenseMatch {
            rid: 0,
            license_expression: "mit".to_string(),
            license_expression_spdx: Some("MIT".to_string()),
            from_file: None,
            start_line: 0,
            end_line: 0,
            start_token: 0,
            end_token: 0,
            matcher: MatcherKind::Hash,
            score: 0.0,
            matched_length: 0,
            rule_length: 0,
            match_coverage: 0.0,
            rule_relevance: 0,
            rule_identifier: String::new(),
            rule_url: String::new(),
            matched_text: None,
            referenced_filenames: None,
            rule_kind: crate::license_detection::models::RuleKind::None,
            is_from_license: false,
            matched_token_positions: None,
            hilen: 0,
            rule_start_token: 0,
            qspan_positions: None,
            ispan_positions: None,
            hispan_positions: None,
            candidate_resemblance: 0.0,
            candidate_containment: 0.0,
        };

        assert!(match_result.from_file.is_none());
        assert_eq!(match_result.start_line, 0);
        assert_eq!(match_result.score, 0.0);
        assert!(match_result.matched_text.is_none());
    }

    #[test]
    fn test_license_match_score_boundaries() {
        let mut match_result = create_license_match();

        match_result.score = 0.0;
        assert_eq!(match_result.score, 0.0);

        match_result.score = 1.0;
        assert_eq!(match_result.score, 1.0);

        match_result.score = 0.5;
        assert!((match_result.score - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_license_match_coverage_boundaries() {
        let mut match_result = create_license_match();

        match_result.match_coverage = 0.0;
        assert_eq!(match_result.match_coverage, 0.0);

        match_result.match_coverage = 100.0;
        assert_eq!(match_result.match_coverage, 100.0);

        match_result.match_coverage = 50.0;
        assert_eq!(match_result.match_coverage, 50.0);
    }

    #[test]
    fn test_license_match_coverage_rounds_to_two_decimals() {
        let mut match_result = create_license_match();

        match_result.match_coverage = 33.334;

        assert_eq!(match_result.coverage(), 33.33);
    }

    #[test]
    fn test_license_match_serialization() {
        let match_result = create_license_match();
        let json = serde_json::to_string(&match_result).unwrap();

        assert!(json.contains("\"license_expression\":\"mit\""));
        assert!(json.contains("\"license_expression_spdx\":\"MIT\""));
        assert!(json.contains("\"start_line\":1"));
    }

    #[test]
    fn test_license_match_deserialization() {
        let json = r#"{
            "license_expression": "apache-2.0",
            "license_expression_spdx": "Apache-2.0",
            "from_file": "LICENSE",
            "start_line": 10,
            "end_line": 20,
            "matcher": "1-hash",
            "score": 0.99,
            "matched_length": 500,
            "match_coverage": 99.0,
            "rule_relevance": 95,
            "rule_identifier": "apache-2.0.LICENSE",
            "rule_url": "https://example.org/apache-2.0",
            "matched_text": "Apache License",
            "referenced_filenames": ["NOTICE"],
            "is_license_intro": false,
            "is_license_clue": false
        }"#;

        let match_result: LicenseMatch = serde_json::from_str(json).unwrap();

        assert_eq!(match_result.license_expression, "apache-2.0");
        assert_eq!(match_result.start_line, 10);
        assert_eq!(match_result.end_line, 20);
        assert!((match_result.score - 0.99).abs() < 0.001);
        assert_eq!(
            match_result.referenced_filenames,
            Some(vec!["NOTICE".to_string()])
        );
    }

    #[test]
    fn test_license_match_roundtrip_serialization() {
        let original = create_license_match();
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: LicenseMatch = serde_json::from_str(&json).unwrap();

        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_output_match_serializes_null_rule_url() {
        let output_match = OutputMatch {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: None,
            start_line: 1,
            end_line: 1,
            matcher: Some("1-hash".to_string()),
            score: 100.0,
            matched_length: Some(3),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("spdx-license-identifier-mit-deadbeef".to_string()),
            rule_url: None,
            matched_text: Some("MIT".to_string()),
            referenced_filenames: None,
            matched_text_diagnostics: None,
        };

        let json = serde_json::to_value(&output_match).unwrap();

        assert!(json.get("rule_url").is_some());
        assert!(json["rule_url"].is_null());
    }

    #[test]
    fn test_license_match_with_referenced_filenames() {
        let match_result = LicenseMatch {
            rid: 0,
            license_expression: "mit".to_string(),
            license_expression_spdx: Some("MIT".to_string()),
            from_file: Some("README.md".to_string()),
            start_line: 1,
            end_line: 5,
            start_token: 0,
            end_token: 100,
            matcher: crate::license_detection::models::MatcherKind::Hash,
            score: 0.95,
            matched_length: 100,
            rule_length: 100,
            matched_token_positions: None,
            match_coverage: 95.0,
            rule_relevance: 100,
            rule_identifier: "mit.LICENSE".to_string(),
            rule_url: "https://scancode-licensedb.aboutcode.org/mit".to_string(),
            matched_text: Some("MIT License text...".to_string()),
            referenced_filenames: Some(vec!["LICENSE".to_string(), "COPYING".to_string()]),
            rule_kind: crate::license_detection::models::RuleKind::None,
            is_from_license: false,
            hilen: 50,
            rule_start_token: 0,
            qspan_positions: None,
            ispan_positions: None,
            hispan_positions: None,
            candidate_resemblance: 0.0,
            candidate_containment: 0.0,
        };

        assert_eq!(
            match_result.referenced_filenames,
            Some(vec!["LICENSE".to_string(), "COPYING".to_string()])
        );
    }

    #[test]
    fn test_len_contiguous() {
        let match_result = create_license_match();
        assert_eq!(match_result.len(), 100);
    }

    #[test]
    fn test_len_non_contiguous() {
        let mut match_result = create_license_match();
        match_result.matched_token_positions = Some(vec![0, 2, 5, 10]);
        assert_eq!(match_result.len(), 4);
    }

    #[test]
    fn test_len_zero() {
        let mut match_result = create_license_match();
        match_result.start_token = 0;
        match_result.end_token = 0;
        assert_eq!(match_result.len(), 0);
    }

    #[test]
    fn test_qdensity_contiguous() {
        use std::collections::{HashMap, HashSet};
        let index = create_test_index();
        let match_result = create_license_match();
        let query = crate::license_detection::query::Query {
            text: String::new(),
            tokens: vec![],
            line_by_pos: vec![],
            unknowns_by_pos: HashMap::new(),
            stopwords_by_pos: HashMap::new(),
            shorts_and_digits_pos: HashSet::new(),
            high_matchables: bit_set::BitSet::new(),
            low_matchables: bit_set::BitSet::new(),
            is_binary: false,
            query_run_ranges: vec![],
            spdx_lines: vec![],
            index: &index,
        };
        assert!((match_result.qdensity(&query) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_qdensity_sparse() {
        use std::collections::{HashMap, HashSet};
        let index = create_test_index();
        let mut match_result = create_license_match();
        match_result.matched_token_positions = Some(vec![0, 10]);
        let query = crate::license_detection::query::Query {
            text: String::new(),
            tokens: vec![],
            line_by_pos: vec![],
            unknowns_by_pos: HashMap::new(),
            stopwords_by_pos: HashMap::new(),
            shorts_and_digits_pos: HashSet::new(),
            high_matchables: bit_set::BitSet::new(),
            low_matchables: bit_set::BitSet::new(),
            is_binary: false,
            query_run_ranges: vec![],
            spdx_lines: vec![],
            index: &index,
        };
        let expected = 2.0 / 11.0;
        assert!((match_result.qdensity(&query) - expected).abs() < 0.001);
    }

    #[test]
    fn test_qdensity_zero() {
        use std::collections::{HashMap, HashSet};
        let index = create_test_index();
        let mut match_result = create_license_match();
        match_result.start_token = 0;
        match_result.end_token = 0;
        let query = crate::license_detection::query::Query {
            text: String::new(),
            tokens: vec![],
            line_by_pos: vec![],
            unknowns_by_pos: HashMap::new(),
            stopwords_by_pos: HashMap::new(),
            shorts_and_digits_pos: HashSet::new(),
            high_matchables: bit_set::BitSet::new(),
            low_matchables: bit_set::BitSet::new(),
            is_binary: false,
            query_run_ranges: vec![],
            spdx_lines: vec![],
            index: &index,
        };
        assert_eq!(match_result.qdensity(&query), 0.0);
    }

    #[test]
    fn test_qdensity_with_unknowns() {
        use std::collections::{HashMap, HashSet};
        let index = create_test_index();
        let mut match_result = create_license_match();
        match_result.start_token = 0;
        match_result.end_token = 10;
        match_result.matched_token_positions = Some(vec![0, 5, 9]);
        let mut unknowns_by_pos = HashMap::new();
        unknowns_by_pos.insert(Some(0), 2);
        unknowns_by_pos.insert(Some(5), 3);
        let query = crate::license_detection::query::Query {
            text: String::new(),
            tokens: vec![],
            line_by_pos: vec![],
            unknowns_by_pos,
            stopwords_by_pos: HashMap::new(),
            shorts_and_digits_pos: HashSet::new(),
            high_matchables: bit_set::BitSet::new(),
            low_matchables: bit_set::BitSet::new(),
            is_binary: false,
            query_run_ranges: vec![],
            spdx_lines: vec![],
            index: &index,
        };
        let expected = 3.0 / (10.0 + 2.0 + 3.0);
        assert!((match_result.qdensity(&query) - expected).abs() < 0.001);
    }

    #[test]
    fn test_qmagnitude_non_contiguous() {
        use std::collections::{HashMap, HashSet};
        let index = create_test_index();
        let mut match_result = create_license_match();
        match_result.qspan_positions = Some(vec![0, 5, 10]);
        let mut unknowns_by_pos = HashMap::new();
        unknowns_by_pos.insert(Some(0), 2);
        unknowns_by_pos.insert(Some(5), 3);
        let query = crate::license_detection::query::Query {
            text: String::new(),
            tokens: vec![],
            line_by_pos: vec![],
            unknowns_by_pos,
            stopwords_by_pos: HashMap::new(),
            shorts_and_digits_pos: HashSet::new(),
            high_matchables: bit_set::BitSet::new(),
            low_matchables: bit_set::BitSet::new(),
            is_binary: false,
            query_run_ranges: vec![],
            spdx_lines: vec![],
            index: &index,
        };
        let expected = 11 + 2 + 3;
        assert_eq!(match_result.qmagnitude(&query), expected);
    }

    #[test]
    fn test_qmagnitude_excludes_end_position() {
        use std::collections::{HashMap, HashSet};
        let index = create_test_index();
        let mut match_result = create_license_match();
        match_result.qspan_positions = Some(vec![0, 5, 10]);
        let mut unknowns_by_pos = HashMap::new();
        unknowns_by_pos.insert(Some(10), 100);
        let query = crate::license_detection::query::Query {
            text: String::new(),
            tokens: vec![],
            line_by_pos: vec![],
            unknowns_by_pos,
            stopwords_by_pos: HashMap::new(),
            shorts_and_digits_pos: HashSet::new(),
            high_matchables: bit_set::BitSet::new(),
            low_matchables: bit_set::BitSet::new(),
            is_binary: false,
            query_run_ranges: vec![],
            spdx_lines: vec![],
            index: &index,
        };
        assert_eq!(match_result.qmagnitude(&query), 11);
    }

    #[test]
    fn test_idensity_contiguous() {
        let match_result = create_license_match();
        assert!((match_result.idensity() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_idensity_sparse_ispan() {
        let mut match_result = create_license_match();
        match_result.matched_length = 2;
        match_result.ispan_positions = Some(vec![0, 10]);
        let expected = 2.0 / 11.0;
        assert!((match_result.idensity() - expected).abs() < 0.001);
    }

    #[test]
    fn test_idensity_uses_ispan_not_qspan() {
        let mut match_result = create_license_match();
        match_result.start_token = 0;
        match_result.end_token = 20;
        match_result.matched_length = 5;
        match_result.ispan_positions = Some(vec![0, 2, 4, 6, 8]);
        assert!((match_result.idensity() - (5.0 / 9.0)).abs() < 0.001);
    }

    #[test]
    fn test_idensity_zero() {
        let mut match_result = create_license_match();
        match_result.matched_length = 0;
        assert_eq!(match_result.idensity(), 0.0);
    }

    #[test]
    fn test_surround_true() {
        let outer = LicenseMatch {
            start_line: 1,
            end_line: 20,
            ..create_license_match()
        };
        let inner = LicenseMatch {
            start_line: 5,
            end_line: 15,
            ..create_license_match()
        };
        assert!(outer.surround(&inner));
    }

    #[test]
    fn test_surround_false_same_start() {
        let outer = LicenseMatch {
            start_line: 1,
            end_line: 20,
            start_token: 1,
            end_token: 20,
            ..create_license_match()
        };
        let inner = LicenseMatch {
            start_line: 1,
            end_line: 15,
            start_token: 1,
            end_token: 15,
            ..create_license_match()
        };
        assert!(outer.surround(&inner));
    }

    #[test]
    fn test_surround_false_same_end() {
        let outer = LicenseMatch {
            start_line: 1,
            end_line: 20,
            start_token: 1,
            end_token: 20,
            ..create_license_match()
        };
        let inner = LicenseMatch {
            start_line: 5,
            end_line: 20,
            start_token: 5,
            end_token: 20,
            ..create_license_match()
        };
        assert!(outer.surround(&inner));
    }

    #[test]
    fn test_surround_false_reversed() {
        let outer = LicenseMatch {
            start_line: 5,
            end_line: 15,
            start_token: 5,
            end_token: 15,
            ..create_license_match()
        };
        let inner = LicenseMatch {
            start_line: 1,
            end_line: 20,
            start_token: 1,
            end_token: 20,
            ..create_license_match()
        };
        assert!(!outer.surround(&inner));
    }

    #[test]
    fn test_surround_false_adjacent() {
        let first = LicenseMatch {
            start_line: 1,
            end_line: 10,
            start_token: 1,
            end_token: 10,
            ..create_license_match()
        };
        let second = LicenseMatch {
            start_line: 11,
            end_line: 20,
            start_token: 11,
            end_token: 20,
            ..create_license_match()
        };
        assert!(!first.surround(&second));
        assert!(!second.surround(&first));
    }

    #[test]
    fn test_qcontains_simple_contained() {
        let outer = LicenseMatch {
            start_token: 0,
            end_token: 20,
            ..create_license_match()
        };
        let inner = LicenseMatch {
            start_token: 5,
            end_token: 15,
            ..create_license_match()
        };
        assert!(outer.qcontains(&inner));
        assert!(!inner.qcontains(&outer));
    }

    #[test]
    fn test_qcontains_same_boundaries() {
        let a = LicenseMatch {
            start_token: 0,
            end_token: 10,
            ..create_license_match()
        };
        let b = LicenseMatch {
            start_token: 0,
            end_token: 10,
            ..create_license_match()
        };
        assert!(a.qcontains(&b));
        assert!(b.qcontains(&a));
    }

    #[test]
    fn test_qcontains_overlapping_not_contained() {
        let a = LicenseMatch {
            start_token: 0,
            end_token: 10,
            ..create_license_match()
        };
        let b = LicenseMatch {
            start_token: 5,
            end_token: 15,
            ..create_license_match()
        };
        assert!(!a.qcontains(&b));
        assert!(!b.qcontains(&a));
    }

    #[test]
    fn test_qcontains_no_overlap() {
        let a = LicenseMatch {
            start_token: 0,
            end_token: 5,
            ..create_license_match()
        };
        let b = LicenseMatch {
            start_token: 10,
            end_token: 15,
            ..create_license_match()
        };
        assert!(!a.qcontains(&b));
        assert!(!b.qcontains(&a));
    }

    #[test]
    fn test_qcontains_start_overlap_only() {
        let a = LicenseMatch {
            start_token: 0,
            end_token: 10,
            ..create_license_match()
        };
        let b = LicenseMatch {
            start_token: 0,
            end_token: 15,
            ..create_license_match()
        };
        assert!(!a.qcontains(&b));
        assert!(b.qcontains(&a));
    }

    #[test]
    fn test_qcontains_end_overlap_only() {
        let a = LicenseMatch {
            start_token: 5,
            end_token: 15,
            ..create_license_match()
        };
        let b = LicenseMatch {
            start_token: 0,
            end_token: 15,
            ..create_license_match()
        };
        assert!(!a.qcontains(&b));
        assert!(b.qcontains(&a));
    }

    #[test]
    fn test_qcontains_zero_tokens_fallback_line_contained() {
        let outer = LicenseMatch {
            start_token: 0,
            end_token: 0,
            start_line: 1,
            end_line: 20,
            ..create_license_match()
        };
        let inner = LicenseMatch {
            start_token: 0,
            end_token: 0,
            start_line: 5,
            end_line: 15,
            ..create_license_match()
        };
        assert!(outer.qcontains(&inner));
        assert!(!inner.qcontains(&outer));
    }

    #[test]
    fn test_qcontains_zero_tokens_fallback_same_lines() {
        let a = LicenseMatch {
            start_token: 0,
            end_token: 0,
            start_line: 1,
            end_line: 10,
            ..create_license_match()
        };
        let b = LicenseMatch {
            start_token: 0,
            end_token: 0,
            start_line: 1,
            end_line: 10,
            ..create_license_match()
        };
        assert!(a.qcontains(&b));
        assert!(b.qcontains(&a));
    }

    #[test]
    fn test_qcontains_zero_tokens_fallback_no_containment() {
        let a = LicenseMatch {
            start_token: 0,
            end_token: 0,
            start_line: 1,
            end_line: 10,
            ..create_license_match()
        };
        let b = LicenseMatch {
            start_token: 0,
            end_token: 0,
            start_line: 5,
            end_line: 15,
            ..create_license_match()
        };
        assert!(!a.qcontains(&b));
        assert!(!b.qcontains(&a));
    }

    #[test]
    fn test_qcontains_zero_tokens_fallback_different_lines() {
        let a = LicenseMatch {
            start_token: 0,
            end_token: 0,
            start_line: 1,
            end_line: 5,
            ..create_license_match()
        };
        let b = LicenseMatch {
            start_token: 0,
            end_token: 0,
            start_line: 10,
            end_line: 15,
            ..create_license_match()
        };
        assert!(!a.qcontains(&b));
        assert!(!b.qcontains(&a));
    }

    #[test]
    fn test_qcontains_mixed_tokens_uses_token_positions() {
        let a = LicenseMatch {
            start_token: 0,
            end_token: 0,
            start_line: 1,
            end_line: 20,
            ..create_license_match()
        };
        let b = LicenseMatch {
            start_token: 5,
            end_token: 10,
            start_line: 5,
            end_line: 15,
            ..create_license_match()
        };
        assert!(!a.qcontains(&b));
    }

    #[test]
    fn test_qdistance_to_overlapping() {
        let mut a = create_license_match();
        a.start_token = 0;
        a.end_token = 10;
        let mut b = create_license_match();
        b.start_token = 5;
        b.end_token = 15;
        assert_eq!(a.qdistance_to(&b), 0);
        assert_eq!(b.qdistance_to(&a), 0);
    }

    #[test]
    fn test_qdistance_to_touching() {
        let mut a = create_license_match();
        a.start_token = 0;
        a.end_token = 10;
        let mut b = create_license_match();
        b.start_token = 10;
        b.end_token = 20;
        assert_eq!(a.qdistance_to(&b), 1);
        assert_eq!(b.qdistance_to(&a), 1);
    }

    #[test]
    fn test_qdistance_to_separated() {
        let mut a = create_license_match();
        a.start_token = 0;
        a.end_token = 5;
        let mut b = create_license_match();
        b.start_token = 15;
        b.end_token = 20;
        assert_eq!(a.qdistance_to(&b), 11);
        assert_eq!(b.qdistance_to(&a), 11);
    }

    #[test]
    fn test_qdistance_to_gapped_spans_matches_python_semantics() {
        let mut a = create_license_match();
        a.qspan_positions = Some(vec![55]);
        a.start_token = 55;
        a.end_token = 56;

        let mut b = create_license_match();
        b.qspan_positions = Some(vec![57, 58]);
        b.start_token = 57;
        b.end_token = 59;

        assert_eq!(a.qdistance_to(&b), 2);
        assert_eq!(b.qdistance_to(&a), 2);
    }

    #[test]
    fn test_has_unknown_true() {
        let mut m = create_license_match();
        m.license_expression = "unknown".to_string();
        assert!(m.has_unknown());

        m.license_expression = "mit OR unknown".to_string();
        assert!(m.has_unknown());

        m.license_expression = "unknown-license-ref".to_string();
        assert!(m.has_unknown());
    }

    #[test]
    fn test_has_unknown_false() {
        let mut m = create_license_match();
        m.license_expression = "mit".to_string();
        assert!(!m.has_unknown());

        m.license_expression = "apache-2.0".to_string();
        assert!(!m.has_unknown());
    }
}
