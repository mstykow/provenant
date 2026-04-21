// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod test_cases {
    use crate::license_detection::index::LicenseIndex;
    use crate::license_detection::index::builder::{
        build_index, compute_is_approx_matchable, generate_url_variants, is_good_tokens_ngram,
        ngrams, tokens_to_bytes,
    };
    use crate::license_detection::index::dictionary::{KnownToken, TokenId, TokenKind, tid};
    use crate::license_detection::models::{License, Rule, RuleKind};

    fn known_tokens(entries: &[(u16, TokenKind)]) -> Vec<KnownToken> {
        entries
            .iter()
            .map(|(id, kind)| KnownToken {
                id: tid(*id),
                kind: *kind,
                is_digit_only: false,
                is_short_or_digit: false,
            })
            .collect()
    }

    fn find_rid_by_identifier(index: &LicenseIndex, identifier: &str) -> Option<usize> {
        index
            .rules_by_rid
            .iter()
            .position(|r| r.identifier == identifier)
    }

    fn find_rid_by_expression(index: &LicenseIndex, expression: &str) -> Vec<usize> {
        index
            .rules_by_rid
            .iter()
            .enumerate()
            .filter_map(|(rid, r)| {
                if r.license_expression == expression {
                    Some(rid)
                } else {
                    None
                }
            })
            .collect()
    }

    fn create_test_rule(text: &str, is_false_positive: bool) -> Rule {
        Rule {
            identifier: "test.RULE".to_string(),
            license_expression: "mit".to_string(),
            text: text.to_string(),
            tokens: vec![],
            rule_kind: crate::license_detection::models::RuleKind::None,
            is_false_positive,
            is_required_phrase: false,
            is_from_license: false,
            relevance: 100,
            minimum_coverage: None,
            has_stored_minimum_coverage: false,
            is_continuous: false,
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
            required_phrase_spans: vec![],
            stopwords_by_pos: std::collections::HashMap::new(),
        }
    }

    fn create_test_license(key: &str, name: &str) -> License {
        License {
            key: key.to_string(),
            short_name: Some(key.to_uppercase()),
            name: name.to_string(),
            language: Some("en".to_string()),
            spdx_license_key: Some(key.to_uppercase()),
            other_spdx_license_keys: vec![],
            category: Some("Permissive".to_string()),
            owner: Some("Example Owner".to_string()),
            homepage_url: Some("https://example.com/license".to_string()),
            text: format!("{} license text", name),
            reference_urls: vec!["https://example.com/license".to_string()],
            osi_license_key: Some(key.to_uppercase()),
            text_urls: vec!["https://example.com/text".to_string()],
            osi_url: Some("https://example.com/osi".to_string()),
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

    fn create_medium_rule_text() -> String {
        (0..25)
            .map(|i| format!("token{i}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn test_build_index_empty() {
        let index = build_index(vec![], vec![]);
        assert!(index.rules_by_rid.is_empty());
        assert!(index.tids_by_rid.is_empty());
        assert!(index.rid_by_hash.is_empty());
        assert!(index.false_positive_rids.is_empty());
        assert!(index.approx_matchable_rids.is_empty());
    }

    #[test]
    fn test_build_index_single_rule() {
        let mut rule = create_test_rule("MIT License", false);
        rule.identifier = "test.RULE".to_string();
        let rules = vec![rule];
        let licenses = vec![create_test_license("mit", "MIT License")];

        let index = build_index(rules, licenses);

        assert_eq!(index.rules_by_rid.len(), 2);
        assert_eq!(index.tids_by_rid.len(), 2);

        let rid = find_rid_by_identifier(&index, "test.RULE").expect("rule should exist");
        assert!(
            index
                .rid_by_hash
                .values()
                .any(|&stored_rid| stored_rid == rid)
        );
        assert!(!index.false_positive_rids.contains(&rid));
        assert!(index.licenses_by_key.contains_key("mit"));
    }

    #[test]
    fn test_build_index_false_positive() {
        let mut rule = create_test_rule("MIT License", true);
        rule.identifier = "fp.RULE".to_string();
        let rules = vec![rule];
        let index = build_index(rules, vec![]);

        assert_eq!(index.rules_by_rid.len(), 1);

        let rid = find_rid_by_identifier(&index, "fp.RULE").expect("rule should exist");
        assert!(index.false_positive_rids.contains(&rid));
        assert!(index.rid_by_hash.is_empty());
    }

    #[test]
    fn test_build_index_preserves_stored_minimum_coverage() {
        let medium_rule_text = create_medium_rule_text();

        let mut stored_rule = create_test_rule(&medium_rule_text, false);
        stored_rule.identifier = "stored.RULE".to_string();
        stored_rule.minimum_coverage = Some(99);
        stored_rule.has_stored_minimum_coverage = true;

        let mut computed_rule = create_test_rule(&medium_rule_text, false);
        computed_rule.identifier = "computed.RULE".to_string();

        let index = build_index(vec![stored_rule, computed_rule], vec![]);

        let stored = &index.rules_by_rid
            [find_rid_by_identifier(&index, "stored.RULE").expect("stored rule should exist")];
        assert_eq!(stored.minimum_coverage, Some(99));
        assert!(stored.has_stored_minimum_coverage);

        let computed = &index.rules_by_rid
            [find_rid_by_identifier(&index, "computed.RULE").expect("computed rule should exist")];
        assert_eq!(computed.minimum_coverage, Some(50));
        assert!(!computed.has_stored_minimum_coverage);
    }

    #[test]
    fn test_build_index_sets_and_msets() {
        let mut rule = create_test_rule("MIT License copyright permission", false);
        rule.identifier = "sets.RULE".to_string();
        let rules = vec![rule];
        let index = build_index(rules, vec![]);

        let rid = find_rid_by_identifier(&index, "sets.RULE").expect("rule should exist");
        assert!(index.sets_by_rid.contains_key(&rid));
        assert!(index.msets_by_rid.contains_key(&rid));
        assert!(!index.sets_by_rid[&rid].is_empty());
    }

    #[test]
    fn test_build_index_high_postings() {
        let mut rule =
            create_test_rule("licensed copyrighted permission granted authorized", false);
        rule.identifier = "hp.RULE".to_string();
        let rules = vec![rule];
        let index = build_index(rules, vec![]);

        let rid = find_rid_by_identifier(&index, "hp.RULE").expect("rule should exist");

        if !index.approx_matchable_rids.is_empty() {
            assert!(index.high_postings_by_rid.contains_key(&rid));
        }
    }

    #[test]
    fn test_build_index_digit_only_tokens() {
        let rules = vec![create_test_rule("version 123 456 789 test", false)];
        let index = build_index(rules, vec![]);

        let token_123 = index
            .dictionary
            .lookup("123")
            .expect("123 should be interned");
        assert!(token_123.is_digit_only);
    }

    #[test]
    fn test_compute_is_approx_matchable() {
        let mut rule = create_test_rule("test", false);
        rule.is_tiny = false;
        rule.is_small = false;
        rule.is_continuous = false;
        rule.is_required_phrase = false;
        rule.is_false_positive = false;
        rule.rule_kind = RuleKind::None;
        assert!(compute_is_approx_matchable(&rule));

        rule.is_false_positive = true;
        assert!(!compute_is_approx_matchable(&rule));
        rule.is_false_positive = false;

        rule.is_required_phrase = true;
        assert!(!compute_is_approx_matchable(&rule));
        rule.is_required_phrase = false;

        rule.is_tiny = true;
        assert!(!compute_is_approx_matchable(&rule));
        rule.is_tiny = false;

        rule.is_continuous = true;
        assert!(!compute_is_approx_matchable(&rule));
        rule.is_continuous = false;

        rule.is_small = true;
        rule.rule_kind = RuleKind::Reference;
        assert!(!compute_is_approx_matchable(&rule));
    }

    #[test]
    fn test_is_good_tokens_ngram() {
        let tokens = vec![
            "hello".to_string(),
            "world".to_string(),
            "license".to_string(),
        ];
        let tids = known_tokens(&[
            (100, TokenKind::Regular),
            (101, TokenKind::Regular),
            (0, TokenKind::Legalese),
        ]);
        assert!(is_good_tokens_ngram(&tokens, &tids));

        let tokens_with_year = vec!["2023".to_string(), "license".to_string(), "mit".to_string()];
        let tids_with_year = known_tokens(&[
            (500, TokenKind::Regular),
            (0, TokenKind::Legalese),
            (1, TokenKind::Legalese),
        ]);
        assert!(!is_good_tokens_ngram(&tokens_with_year, &tids_with_year));

        let tokens_all_digits = vec!["1".to_string(), "2".to_string(), "3".to_string()];
        let tids_all_digits = known_tokens(&[
            (100, TokenKind::Regular),
            (101, TokenKind::Regular),
            (102, TokenKind::Regular),
        ]);
        assert!(!is_good_tokens_ngram(&tokens_all_digits, &tids_all_digits));
    }

    #[test]
    fn test_tokens_to_bytes() {
        let tokens = vec![tid(1), tid(2), tid(3)];
        let bytes = tokens_to_bytes(&tokens);
        assert_eq!(bytes.len(), 6);
        assert_eq!(bytes[0], 1);
        assert_eq!(bytes[1], 0);
        assert_eq!(bytes[2], 2);
        assert_eq!(bytes[3], 0);
        assert_eq!(bytes[4], 3);
        assert_eq!(bytes[5], 0);
    }

    #[test]
    fn test_ngrams() {
        let items = vec![1, 2, 3, 4, 5];
        let ngrams_result = ngrams(&items, 3);
        assert_eq!(ngrams_result.len(), 3);
        assert_eq!(ngrams_result[0], vec![1, 2, 3]);
        assert_eq!(ngrams_result[1], vec![2, 3, 4]);
        assert_eq!(ngrams_result[2], vec![3, 4, 5]);

        let short_items = vec![1, 2];
        let short_ngrams = ngrams(&short_items, 3);
        assert!(short_ngrams.is_empty());
    }

    #[test]
    fn test_build_index_multiple_rules() {
        let mut rule1 = create_test_rule("MIT License", false);
        rule1.identifier = "mit.RULE".to_string();

        let mut rule2 = create_test_rule("Apache License", false);
        rule2.identifier = "apache.RULE".to_string();

        let mut rule3 = create_test_rule("GPL License", true);
        rule3.identifier = "gpl.RULE".to_string();

        let rules = vec![rule1, rule2, rule3];
        let index = build_index(rules, vec![]);

        assert_eq!(index.rules_by_rid.len(), 3);
        assert_eq!(index.tids_by_rid.len(), 3);
        assert_eq!(index.false_positive_rids.len(), 1);
        assert_eq!(index.rid_by_hash.len(), 2);

        let gpl_rid = find_rid_by_identifier(&index, "gpl.RULE").expect("gpl rule should exist");
        assert!(index.false_positive_rids.contains(&gpl_rid));
    }

    #[test]
    fn test_build_index_licenses() {
        let mut rule = create_test_rule("MIT License", false);
        rule.identifier = "mit.RULE".to_string();
        let rules = vec![rule];
        let licenses = vec![
            create_test_license("mit", "MIT License"),
            create_test_license("apache-2.0", "Apache License 2.0"),
        ];
        let index = build_index(rules, licenses);

        assert_eq!(index.licenses_by_key.len(), 2);
        assert!(index.licenses_by_key.contains_key("mit"));
        assert!(index.licenses_by_key.contains_key("apache-2.0"));

        let mit_license_rid = find_rid_by_identifier(&index, "mit.LICENSE");
        assert!(
            mit_license_rid.is_some(),
            "Should have mit.LICENSE from license"
        );
    }

    #[test]
    fn test_build_index_from_reference_rules() {
        use crate::license_detection::LicenseDetectionEngine;

        let engine =
            LicenseDetectionEngine::from_embedded().expect("required test dependency available");

        let index = engine.index();

        assert!(!index.rules_by_rid.is_empty(), "Should have rules loaded");
        assert!(!index.tids_by_rid.is_empty(), "Should have token IDs");
        assert!(
            !index.rid_by_hash.is_empty(),
            "Should have hash mappings for regular rules"
        );
        assert!(
            !index.rid_by_hash.is_empty(),
            "Should have regular rule hashes"
        );
        assert!(!index.sets_by_rid.is_empty(), "Should have token sets");
        assert!(
            !index.msets_by_rid.is_empty(),
            "Should have token multisets"
        );
        assert!(
            !index.licenses_by_key.is_empty(),
            "Should have licenses loaded"
        );

        assert!(index.len_legalese > 0, "Should have legalese tokens");
        assert!(
            index.dictionary.tokens_to_ids().count() >= index.len_legalese,
            "Dictionary should have at least legalese tokens"
        );

        let mut rules_with_empty_tokens = 0;
        for &rid in index.rid_by_hash.values() {
            let rule = &index.rules_by_rid[rid];
            if rule.tokens.is_empty() {
                rules_with_empty_tokens += 1;
            }
            assert!(
                index.sets_by_rid.contains_key(&rid),
                "Rule {} should have token set",
                rid
            );
            assert!(
                index.msets_by_rid.contains_key(&rid),
                "Rule {} should have token multiset",
                rid
            );
        }
        if rules_with_empty_tokens > 0 {
            eprintln!(
                "Note: {} rules have empty tokens (likely non-ASCII text like Japanese/Chinese)",
                rules_with_empty_tokens
            );
        }

        if !index.approx_matchable_rids.is_empty() {
            for &rid in &index.approx_matchable_rids {
                let rule = &index.rules_by_rid[rid];
                assert!(!rule.is_false_positive);
            }
        }
    }

    #[test]
    fn test_build_index_automaton_functional() {
        let mut rule1 = create_test_rule("MIT License copyright permission", false);
        rule1.identifier = "auto1.RULE".to_string();

        let mut rule2 = create_test_rule("Apache License Version 2.0", false);
        rule2.identifier = "auto2.RULE".to_string();

        let mut rule3 = create_test_rule("GNU General Public License", false);
        rule3.identifier = "auto3.RULE".to_string();

        let rules = vec![rule1, rule2, rule3];
        let index = build_index(rules, vec![]);

        assert_eq!(index.rules_by_rid.len(), 3, "Should have 3 rules indexed");
        assert_eq!(index.rid_by_hash.len(), 3);

        let rid = find_rid_by_identifier(&index, "auto1.RULE").expect("rule should exist");
        let rule_tokens = &index.tids_by_rid[rid];
        let pattern: Vec<u8> = rule_tokens.iter().flat_map(|t| t.to_le_bytes()).collect();

        let matches: Vec<_> = index
            .rules_automaton
            .find_overlapping_iter(&pattern)
            .collect();
        assert!(!matches.is_empty(), "Automaton should find the pattern");
    }

    #[test]
    fn test_build_index_rule_thresholds_computed() {
        let rule_text = "Permission is hereby granted free of charge to any person obtaining a copy of this software and associated documentation files the MIT License";
        let mut rule = create_test_rule(rule_text, false);
        rule.identifier = "thresholds.RULE".to_string();
        let rules = vec![rule];
        let index = build_index(rules, vec![]);

        let rid = find_rid_by_identifier(&index, "thresholds.RULE").expect("rule should exist");
        let rule = &index.rules_by_rid[rid];

        assert!(rule.length_unique > 0, "length_unique should be computed");
        assert!(
            rule.min_matched_length > 0,
            "min_matched_length should be computed"
        );

        assert!(
            rule.min_matched_length_unique > 0,
            "min_matched_length_unique should be computed"
        );
    }

    #[test]
    fn test_build_index_approx_matchable_classification() {
        let mut regular_rule = create_test_rule(
            "Permission is hereby granted free of charge to any person obtaining a copy",
            false,
        );
        regular_rule.identifier = "regular.RULE".to_string();
        regular_rule.rule_kind = RuleKind::Text;

        let mut tiny_rule = create_test_rule("MIT", false);
        tiny_rule.identifier = "tiny.RULE".to_string();
        tiny_rule.rule_kind = RuleKind::None;

        let mut false_positive_rule = create_test_rule("Some text", true);
        false_positive_rule.identifier = "false_positive.RULE".to_string();

        let mut reference_rule = create_test_rule("MIT License", false);
        reference_rule.identifier = "reference.RULE".to_string();
        reference_rule.rule_kind = RuleKind::Reference;

        let rules = vec![
            regular_rule.clone(),
            tiny_rule.clone(),
            false_positive_rule.clone(),
            reference_rule,
        ];
        let index = build_index(rules, vec![]);

        fn find_rid_by_identifier(index: &LicenseIndex, identifier: &str) -> Option<usize> {
            index
                .rules_by_rid
                .iter()
                .position(|r| r.identifier == identifier)
        }

        let regular_rid = find_rid_by_identifier(&index, "regular.RULE").expect("regular rule");
        let tiny_rid = find_rid_by_identifier(&index, "tiny.RULE").expect("tiny rule");
        let fp_rid = find_rid_by_identifier(&index, "false_positive.RULE").expect("fp rule");

        assert!(
            index.rid_by_hash.values().any(|&rid| rid == regular_rid),
            "Regular rule should participate in exact matching"
        );
        assert!(
            index.rid_by_hash.values().any(|&rid| rid == tiny_rid),
            "Tiny rule should participate in exact matching"
        );
        assert!(
            !index.rid_by_hash.values().any(|&rid| rid == fp_rid),
            "False positive should not participate in exact matching"
        );
        assert!(
            index.false_positive_rids.contains(&fp_rid),
            "False positive should be in false_positive_rids"
        );
    }

    #[test]
    fn test_build_index_high_postings_populated() {
        let mut rule = create_test_rule(
            "licensed copyrighted permission granted authorized distributed modification sublicense",
            false,
        );
        rule.identifier = "high_postings.RULE".to_string();
        let rules = vec![rule];
        let index = build_index(rules, vec![]);

        let rid = find_rid_by_identifier(&index, "high_postings.RULE").expect("rule should exist");

        if index.approx_matchable_rids.contains(&rid) {
            assert!(
                index.high_postings_by_rid.contains_key(&rid),
                "Should have high postings for approx-matchable rule with legalese"
            );

            let postings = &index.high_postings_by_rid[&rid];
            assert!(!postings.is_empty(), "Postings should have entries");
        }
    }

    #[test]
    fn test_build_index_keeps_tiny_rule_approx_matchable_for_python_parity() {
        let mut tiny_rule = create_test_rule("permission granted authorized", false);
        tiny_rule.identifier = "tiny-legalese.RULE".to_string();

        let index = build_index(vec![tiny_rule], vec![]);
        let rid = find_rid_by_identifier(&index, "tiny-legalese.RULE").expect("tiny rule");
        let stored_rule = &index.rules_by_rid[rid];

        assert!(stored_rule.is_tiny);
        assert!(index.approx_matchable_rids.contains(&rid));
        assert!(index.high_postings_by_rid.contains_key(&rid));
        assert!(!compute_is_approx_matchable(stored_rule));
    }

    #[test]
    fn test_build_index_keeps_small_reference_rule_approx_matchable_for_python_parity() {
        let mut reference_rule = create_test_rule(
            "licensed under this license permission granted today",
            false,
        );
        reference_rule.identifier = "small-reference.RULE".to_string();
        reference_rule.rule_kind = RuleKind::Reference;

        let index = build_index(vec![reference_rule], vec![]);
        let rid = find_rid_by_identifier(&index, "small-reference.RULE").expect("reference rule");
        let stored_rule = &index.rules_by_rid[rid];

        assert!(stored_rule.is_small);
        assert!(index.approx_matchable_rids.contains(&rid));
        assert!(index.high_postings_by_rid.contains_key(&rid));
        assert!(!compute_is_approx_matchable(stored_rule));
    }

    #[test]
    fn test_build_index_unknown_automaton() {
        let long_rule_text = "Permission is hereby granted free of charge to any person obtaining a copy of this software and associated documentation files the MIT License terms conditions";
        let rules = vec![create_test_rule(long_rule_text, false)];
        let index = build_index(rules, vec![]);

        let unknown_matches: Vec<_> = index
            .unknown_automaton
            .find_overlapping_iter(b"test")
            .collect();
        assert!(
            unknown_matches.is_empty(),
            "Unknown automaton should not match random text"
        );
    }

    #[test]
    fn test_build_index_with_actual_mit_license() {
        let mit_text = r#"Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE."#;

        let mut mit_rule = create_test_rule(mit_text, false);
        mit_rule.identifier = "mit-custom.RULE".to_string();
        mit_rule.rule_kind = RuleKind::Text;
        mit_rule.license_expression = "mit".to_string();

        let rules = vec![mit_rule];
        let licenses = vec![create_test_license("mit", "MIT License")];

        let index = build_index(rules, licenses);

        assert_eq!(
            index.rules_by_rid.len(),
            2,
            "Should have 2 rules: mit.LICENSE and mit-custom.RULE"
        );
        assert!(index.licenses_by_key.contains_key("mit"));

        let mit_license_rid = find_rid_by_identifier(&index, "mit.LICENSE");
        let mit_custom_rid = find_rid_by_identifier(&index, "mit-custom.RULE");

        if let Some(rid) = mit_license_rid {
            assert!(
                index
                    .rid_by_hash
                    .values()
                    .any(|&stored_rid| stored_rid == rid)
            );
            assert!(!index.false_positive_rids.contains(&rid));
        }

        if let Some(rid) = mit_custom_rid {
            assert!(
                index
                    .rid_by_hash
                    .values()
                    .any(|&stored_rid| stored_rid == rid)
            );
            assert!(!index.false_positive_rids.contains(&rid));
        }
    }

    #[test]
    fn test_build_index_empty_license_text() {
        let mut license = create_test_license("empty", "Empty License");
        license.text = String::new();
        let licenses = vec![license];
        let index = build_index(vec![], licenses);

        assert!(index.licenses_by_key.contains_key("empty"));
        assert_eq!(
            index.rules_by_rid.len(),
            1,
            "Empty license text creates rule with 'unknown-spdx license identifier'"
        );
        let rule = &index.rules_by_rid[0];
        assert_eq!(rule.text, "unknown-spdx license identifier");
    }

    #[test]
    fn test_build_index_rules_sorted_by_identifier() {
        let mut rule_z = create_test_rule("Z rule text", false);
        rule_z.identifier = "z_rule.RULE".to_string();

        let mut rule_a = create_test_rule("A rule text", false);
        rule_a.identifier = "a_rule.RULE".to_string();

        let mut rule_m = create_test_rule("M rule text", false);
        rule_m.identifier = "m_rule.RULE".to_string();

        let rules = vec![rule_z, rule_a, rule_m];
        let index = build_index(rules, vec![]);

        let identifiers: Vec<&str> = index
            .rules_by_rid
            .iter()
            .map(|r| r.identifier.as_str())
            .collect();

        assert!(
            identifiers.windows(2).all(|w| w[0] <= w[1]),
            "Rules should be sorted by identifier"
        );
    }

    #[test]
    fn test_build_index_duplicate_keys_licenses() {
        let license1 = create_test_license("mit", "MIT License 1");
        let license2 = create_test_license("mit", "MIT License 2");
        let licenses = vec![license1, license2];

        let index = build_index(vec![], licenses);

        assert_eq!(
            index.licenses_by_key.len(),
            1,
            "Duplicate keys should be overwritten"
        );
        assert_eq!(
            index.licenses_by_key.get("mit").unwrap().name,
            "MIT License 2",
            "Later license should win"
        );
    }

    #[test]
    fn test_build_index_find_by_expression() {
        let mut rule1 = create_test_rule("MIT text", false);
        rule1.identifier = "rule1.RULE".to_string();
        rule1.license_expression = "mit".to_string();

        let mut rule2 = create_test_rule("Apache text", false);
        rule2.identifier = "rule2.RULE".to_string();
        rule2.license_expression = "apache-2.0".to_string();

        let mut rule3 = create_test_rule("MIT variant", false);
        rule3.identifier = "rule3.RULE".to_string();
        rule3.license_expression = "mit".to_string();

        let rules = vec![rule1, rule2, rule3];
        let index = build_index(rules, vec![]);

        let mit_rids = find_rid_by_expression(&index, "mit");
        let apache_rids = find_rid_by_expression(&index, "apache-2.0");

        assert_eq!(mit_rids.len(), 2, "Should find 2 MIT rules");
        assert_eq!(apache_rids.len(), 1, "Should find 1 Apache rule");
    }

    #[test]
    fn test_build_index_weak_rule_no_approx_matchable() {
        let mut weak_rule = create_test_rule("hello world foo bar baz", false);
        weak_rule.identifier = "weak.RULE".to_string();
        weak_rule.text = "hello world foo bar baz qux".to_string();
        let rules = vec![weak_rule];
        let index = build_index(rules, vec![]);

        let rid = find_rid_by_identifier(&index, "weak.RULE").expect("rule should exist");

        assert!(
            !index.approx_matchable_rids.contains(&rid),
            "Weak rule (no legalese tokens) should not be approx_matchable"
        );
        assert!(
            !index.high_postings_by_rid.contains_key(&rid),
            "Weak rule should not have high postings"
        );
    }

    #[test]
    fn test_build_index_continuous_rule_not_approx_matchable() {
        let mut continuous_rule =
            create_test_rule("licensed copyrighted permission granted authorized", false);
        continuous_rule.identifier = "continuous.RULE".to_string();
        continuous_rule.is_continuous = true;
        let rules = vec![continuous_rule];
        let index = build_index(rules, vec![]);

        let rid = find_rid_by_identifier(&index, "continuous.RULE").expect("rule should exist");

        assert!(
            !index.approx_matchable_rids.contains(&rid),
            "Continuous rule should not be approx_matchable"
        );
    }

    #[test]
    fn test_build_index_required_phrase_not_approx_matchable() {
        let mut required_phrase = create_test_rule(
            "licensed copyrighted permission granted authorized distributed",
            false,
        );
        required_phrase.identifier = "required.RULE".to_string();
        required_phrase.is_required_phrase = true;
        let rules = vec![required_phrase];
        let index = build_index(rules, vec![]);

        let rid = find_rid_by_identifier(&index, "required.RULE").expect("rule should exist");

        assert!(
            !index.approx_matchable_rids.contains(&rid),
            "Required phrase should not be approx_matchable"
        );
    }

    #[test]
    fn test_generate_url_variants_https_to_http() {
        let text = "See https://www.boost.org/LICENSE_1_0.txt for details";
        let ignorable_urls = Some(vec!["https://www.boost.org/LICENSE_1_0.txt".to_string()]);

        let variants = generate_url_variants(text, &ignorable_urls);
        assert_eq!(variants.len(), 1);
        assert!(variants[0].contains("http://www.boost.org/LICENSE_1_0.txt"));
        assert!(!variants[0].contains("https://"));
    }

    #[test]
    fn test_generate_url_variants_http_to_https() {
        let text = "See http://www.boost.org/LICENSE_1_0.txt for details";
        let ignorable_urls = Some(vec!["http://www.boost.org/LICENSE_1_0.txt".to_string()]);

        let variants = generate_url_variants(text, &ignorable_urls);
        assert_eq!(variants.len(), 1);
        assert!(variants[0].contains("https://www.boost.org/LICENSE_1_0.txt"));
        assert!(!variants[0].contains("http://"));
    }

    #[test]
    fn test_generate_url_variants_none() {
        let text = "Some text without URLs";
        let variants = generate_url_variants(text, &None);
        assert!(variants.is_empty());
    }

    #[test]
    fn test_generate_url_variants_empty() {
        let text = "Some text";
        let variants = generate_url_variants(text, &Some(vec![]));
        assert!(variants.is_empty());
    }

    #[test]
    fn test_build_index_mit_or_boost_rule_variants() {
        use crate::license_detection::LicenseDetectionEngine;

        let engine =
            LicenseDetectionEngine::from_embedded().expect("required test dependency available");

        let index = engine.index();

        // Find the mit_or_boost-1.0_1.RULE
        let target_rid = index
            .rules_by_rid
            .iter()
            .position(|r| r.identifier == "mit_or_boost-1.0_1.RULE");

        if let Some(rid) = target_rid {
            let rule = &index.rules_by_rid[rid];
            eprintln!("Found rule: {}", rule.identifier);
            eprintln!("License expression: {}", rule.license_expression);
            eprintln!("Ignorable URLs: {:?}", rule.ignorable_urls);

            // The rule text should have https://
            let has_https = rule.text.contains("https://");
            eprintln!("Rule text has https://: {}", has_https);

            let variants = generate_url_variants(&rule.text, &rule.ignorable_urls);
            eprintln!("Generated URL variants: {}", variants.len());

            assert_eq!(
                variants.len(),
                1,
                "Rule should generate one scheme-flipped variant"
            );
            assert!(variants[0].contains("http://www.boost.org/LICENSE_1_0.txt"));
        } else {
            eprintln!("Rule mit_or_boost-1.0_1.RULE not found");
        }
    }

    #[test]
    fn test_sequence_matching_bsl_file() {
        use crate::license_detection::query::Query;
        use crate::license_detection::seq_match::HIGH_RESEMBLANCE_THRESHOLD;
        use crate::license_detection::{LicenseDetectionEngine, TokenSet};
        use std::path::Path;

        let test_file = Path::new(
            "testdata/license-golden/datadriven/external/fossology-tests/Dual-license/BSL-1.0_or_MIT.txt",
        );
        assert!(
            test_file.exists(),
            "missing fixture: {}",
            test_file.display()
        );

        let engine =
            LicenseDetectionEngine::from_embedded().expect("required test dependency available");

        let index = engine.index();
        let text = std::fs::read_to_string(test_file).unwrap();
        let query = Query::from_extracted_text(&text, index, false).expect("Query creation failed");

        eprintln!("Query token count: {}", query.tokens.len());

        // Build query set and mset
        let query_set = TokenSet::from_token_ids(query.tokens.iter().copied());
        eprintln!("Query unique tokens: {}", query_set.len());

        // Find the mit_or_boost rule and compare
        let target_rid = index
            .rules_by_rid
            .iter()
            .position(|r| r.identifier == "mit_or_boost-1.0_1.RULE");
        if let Some(rid) = target_rid {
            let rule = &index.rules_by_rid[rid];
            let rule_tokens = &index.tids_by_rid[rid];
            eprintln!("\nRule token count: {}", rule_tokens.len());

            // Use the indexed set from the index
            let indexed_set = index
                .sets_by_rid
                .get(&rid)
                .expect("Should have indexed set");
            eprintln!(
                "Rule unique tokens (from indexed set): {}",
                indexed_set.len()
            );

            let intersection_count = query_set.intersection_count(indexed_set);
            eprintln!("Intersection unique tokens: {}", intersection_count);

            // Compute union count
            let indexed_only_count = indexed_set
                .iter()
                .filter(|&t| !query_set.contains(&t))
                .count();
            let union_len = query_set.len() + indexed_only_count;
            let resemblance = intersection_count as f32 / union_len as f32;
            eprintln!("Set resemblance: {:.3}", resemblance);
            eprintln!(
                "High resemblance threshold: {:.3}",
                HIGH_RESEMBLANCE_THRESHOLD
            );

            // Check for http/https tokens
            let http_tid = index.dictionary.get("http");
            let https_tid = index.dictionary.get("https");
            eprintln!("\nhttp token id: {:?}", http_tid);
            eprintln!("https token id: {:?}", https_tid);
            eprintln!(
                "Query has http: {:?}",
                http_tid.map(|t| query_set.contains_token_id(t))
            );
            eprintln!(
                "Indexed set has http: {:?}",
                http_tid.map(|t| indexed_set.contains_token_id(t))
            );
            eprintln!(
                "Indexed set has https: {:?}",
                https_tid.map(|t| indexed_set.contains_token_id(t))
            );

            // Check if rule is approx_matchable
            eprintln!(
                "\nRule is approx_matchable: {}",
                index.approx_matchable_rids.contains(&rid)
            );

            // Check the threshold values
            eprintln!("\nRule thresholds:");
            eprintln!(
                "  min_matched_length_unique: {}",
                rule.min_matched_length_unique
            );
            eprintln!(
                "  min_high_matched_length_unique: {}",
                rule.min_high_matched_length_unique
            );
            eprintln!("  min_matched_length: {}", rule.min_matched_length);
            eprintln!(
                "  min_high_matched_length: {}",
                rule.min_high_matched_length
            );

            // Check if query meets thresholds
            let high_intersection_count = indexed_set
                .iter()
                .filter(|&t| {
                    index.dictionary.token_kind(TokenId::new(t)) == TokenKind::Legalese
                        && query_set.contains_token_id(TokenId::new(t))
                })
                .count();
            eprintln!(
                "\nHigh token intersection count: {}",
                high_intersection_count
            );
        }
    }

    #[test]
    fn test_full_detection_bsl_file() {
        use crate::license_detection::LicenseDetectionEngine;
        use std::path::Path;

        let test_file = Path::new(
            "testdata/license-golden/datadriven/external/fossology-tests/Dual-license/BSL-1.0_or_MIT.txt",
        );
        assert!(
            test_file.exists(),
            "missing fixture: {}",
            test_file.display()
        );

        let engine =
            LicenseDetectionEngine::from_embedded().expect("required test dependency available");

        let text = std::fs::read_to_string(test_file).unwrap();

        let detections = engine
            .detect_with_kind(&text, false, false)
            .expect("Detection failed");

        eprintln!("\nDetection results:");
        for d in &detections {
            eprintln!(
                "  - {}",
                d.license_expression.as_deref().unwrap_or("unknown")
            );
            for m in &d.matches {
                eprintln!(
                    "      {} (matcher: {}, coverage: {:.1}%, score: {}, tokens: {}-{})",
                    m.license_expression,
                    m.matcher,
                    m.match_coverage,
                    m.score,
                    m.start_token,
                    m.end_token
                );
            }
        }

        // Check if mit_or_boost rule exists in index
        let index = engine.index();
        let mit_or_boost_rid = index
            .rules_by_rid
            .iter()
            .position(|r| r.identifier == "mit_or_boost-1.0_1.RULE");
        if let Some(rid) = mit_or_boost_rid {
            eprintln!("\nmit_or_boost-1.0_1.RULE found at rid {}", rid);
        }
    }

    #[test]
    fn test_build_index_from_loaded_filters_deprecated() {
        use crate::license_detection::index::build_index_from_loaded;
        use crate::license_detection::models::{LoadedLicense, LoadedRule};

        let active_rule = LoadedRule {
            identifier: "active.RULE".to_string(),
            license_expression: "mit".to_string(),
            text: "MIT License".to_string(),
            rule_kind: RuleKind::Text,
            is_false_positive: false,
            is_required_phrase: false,
            skip_for_required_phrase_generation: false,
            relevance: Some(100),
            minimum_coverage: None,
            has_stored_minimum_coverage: false,
            is_continuous: false,
            referenced_filenames: None,
            ignorable_urls: None,
            ignorable_emails: None,
            ignorable_copyrights: None,
            ignorable_holders: None,
            ignorable_authors: None,
            language: None,
            notes: None,
            is_deprecated: false,
            replaced_by: vec![],
        };

        let deprecated_rule = LoadedRule {
            identifier: "deprecated.RULE".to_string(),
            license_expression: "old-license".to_string(),
            text: "Old License".to_string(),
            rule_kind: RuleKind::Text,
            is_false_positive: false,
            is_required_phrase: false,
            skip_for_required_phrase_generation: false,
            relevance: Some(100),
            minimum_coverage: None,
            has_stored_minimum_coverage: false,
            is_continuous: false,
            referenced_filenames: None,
            ignorable_urls: None,
            ignorable_emails: None,
            ignorable_copyrights: None,
            ignorable_holders: None,
            ignorable_authors: None,
            language: None,
            notes: None,
            is_deprecated: true,
            replaced_by: vec!["mit".to_string()],
        };

        let active_license = LoadedLicense {
            key: "mit".to_string(),
            short_name: Some("MIT".to_string()),
            name: "MIT License".to_string(),
            language: Some("en".to_string()),
            spdx_license_key: Some("MIT".to_string()),
            other_spdx_license_keys: vec![],
            category: Some("Permissive".to_string()),
            owner: Some("Example Owner".to_string()),
            homepage_url: Some("https://example.com/license".to_string()),
            text: "MIT License text".to_string(),
            reference_urls: vec!["https://example.com/license".to_string()],
            osi_license_key: Some("MIT".to_string()),
            text_urls: vec!["https://example.com/text".to_string()],
            osi_url: Some("https://example.com/osi".to_string()),
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
        };

        let deprecated_license = LoadedLicense {
            key: "old-license".to_string(),
            short_name: Some("Old".to_string()),
            name: "Old License".to_string(),
            language: Some("en".to_string()),
            spdx_license_key: None,
            other_spdx_license_keys: vec![],
            category: None,
            owner: None,
            homepage_url: None,
            text: "Old License text".to_string(),
            reference_urls: vec![],
            osi_license_key: None,
            text_urls: vec![],
            osi_url: None,
            faq_url: None,
            other_urls: vec![],
            notes: None,
            is_deprecated: true,
            is_exception: false,
            is_unknown: false,
            is_generic: false,
            replaced_by: vec!["mit".to_string()],
            minimum_coverage: None,
            standard_notice: None,
            ignorable_copyrights: None,
            ignorable_holders: None,
            ignorable_authors: None,
            ignorable_urls: None,
            ignorable_emails: None,
        };

        let index_without_deprecated = build_index_from_loaded(
            vec![active_rule.clone(), deprecated_rule.clone()],
            vec![active_license.clone(), deprecated_license.clone()],
            false,
        );

        assert!(
            index_without_deprecated
                .rules_by_rid
                .iter()
                .any(|r| r.identifier == "active.RULE"),
            "Should include active rule"
        );
        assert!(
            !index_without_deprecated
                .rules_by_rid
                .iter()
                .any(|r| r.identifier == "deprecated.RULE"),
            "Should NOT include deprecated rule"
        );
        assert_eq!(
            index_without_deprecated
                .rule_metadata_by_identifier
                .get("active.RULE")
                .and_then(|metadata| metadata.license_expression_spdx.as_deref()),
            Some("MIT")
        );
        assert_eq!(
            index_without_deprecated
                .rule_metadata_by_identifier
                .get("active.RULE")
                .map(|metadata| metadata.skip_for_required_phrase_generation),
            Some(false)
        );
        assert!(
            index_without_deprecated.licenses_by_key.contains_key("mit"),
            "Should include active license"
        );
        assert!(
            !index_without_deprecated
                .licenses_by_key
                .contains_key("old-license"),
            "Should NOT include deprecated license"
        );

        let index_with_deprecated = build_index_from_loaded(
            vec![active_rule, deprecated_rule],
            vec![active_license, deprecated_license],
            true,
        );

        assert!(
            index_with_deprecated
                .rules_by_rid
                .iter()
                .any(|r| r.identifier == "active.RULE"),
            "Should include active rule"
        );
        assert!(
            index_with_deprecated
                .rules_by_rid
                .iter()
                .any(|r| r.identifier == "deprecated.RULE"),
            "Should include deprecated rule when with_deprecated=true"
        );
        assert_eq!(
            index_with_deprecated
                .rule_metadata_by_identifier
                .get("deprecated.RULE")
                .map(|metadata| metadata.replaced_by.clone()),
            Some(vec!["mit".to_string()])
        );
        assert!(
            index_with_deprecated.licenses_by_key.contains_key("mit"),
            "Should include active license"
        );
        assert!(
            index_with_deprecated
                .licenses_by_key
                .contains_key("old-license"),
            "Should include deprecated license when with_deprecated=true"
        );
    }
}
