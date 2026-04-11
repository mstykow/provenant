use super::*;
use crate::license_detection::index::IndexedRuleMetadata;
use crate::license_detection::index::dictionary::TokenId;
use crate::license_detection::models::Rule;
use crate::license_detection::test_utils::{create_mock_query_with_tokens, create_test_index};
use crate::models::MatchScore;

fn tids(values: &[u16]) -> Vec<TokenId> {
    values.iter().copied().map(TokenId::new).collect()
}

fn create_test_rules_by_rid() -> Vec<Rule> {
    vec![
        Rule {
            identifier: "mit.LICENSE".to_string(),
            license_expression: "mit".to_string(),
            text: "MIT License".to_string(),
            tokens: tids(&[0, 1]),
            rule_kind: crate::license_detection::models::RuleKind::Text,
            is_false_positive: false,
            is_required_phrase: false,
            is_from_license: false,
            relevance: 100,
            minimum_coverage: None,
            has_stored_minimum_coverage: false,
            is_continuous: true,
            referenced_filenames: None,
            ignorable_urls: None,
            ignorable_emails: None,
            ignorable_copyrights: None,
            ignorable_holders: None,
            ignorable_authors: None,
            language: None,
            notes: None,
            length_unique: 2,
            high_length_unique: 2,
            high_length: 2,
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
        },
        Rule {
            identifier: "apache-2.0.LICENSE".to_string(),
            license_expression: "apache-2.0".to_string(),
            text: "Apache License 2.0".to_string(),
            tokens: tids(&[2, 3, 4]),
            rule_kind: crate::license_detection::models::RuleKind::Text,
            is_false_positive: false,
            is_required_phrase: false,
            is_from_license: false,
            relevance: 100,
            minimum_coverage: None,
            has_stored_minimum_coverage: false,
            is_continuous: true,
            referenced_filenames: None,
            ignorable_urls: None,
            ignorable_emails: None,
            ignorable_copyrights: None,
            ignorable_holders: None,
            ignorable_authors: None,
            language: None,
            notes: None,
            length_unique: 3,
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
        },
    ]
}

#[test]
fn test_compute_hash() {
    let tokens = tids(&[1, 2, 3, 4, 5]);
    let hash = compute_hash(&tokens);

    assert_eq!(hash.len(), 20);

    let tokens2 = tids(&[1, 2, 3, 4, 5]);
    let hash2 = compute_hash(&tokens2);

    assert_eq!(hash, hash2, "Same tokens should produce same hash");

    let hash_hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
    assert_eq!(
        hash_hex, "aaa562e5641b932d5d5ecae43b47793b33b3b5f0",
        "Hash should match Python implementation"
    );
}

#[test]
fn test_compute_hash_different_tokens() {
    let tokens1 = tids(&[1, 2, 3]);
    let hash1 = compute_hash(&tokens1);

    let tokens2 = tids(&[1, 2, 4]);
    let hash2 = compute_hash(&tokens2);

    assert_ne!(
        hash1, hash2,
        "Different tokens should produce different hashes"
    );
}

#[test]
fn test_index_hash() {
    let rule_tokens = tids(&[10, 20, 30]);
    let hash1 = compute_hash(&rule_tokens);
    let hash2 = compute_hash(&rule_tokens);

    assert_eq!(
        hash1, hash2,
        "compute_hash should be stable for rule tokens"
    );
}

#[test]
fn test_hash_match_no_match() {
    let mut index = create_test_index(&[("mit", 0), ("license", 1), ("apache", 2), ("2.0", 3)], 2);

    let rules_by_rid = create_test_rules_by_rid();
    let tids_by_rid = vec![tids(&[0, 1]), tids(&[2, 3, 4])];

    index.rid_by_hash.insert(compute_hash(&tids(&[5, 6, 7])), 0);
    index.rules_by_rid = rules_by_rid;
    index.tids_by_rid = tids_by_rid;

    let query_index = create_test_index(&[("token", 0)], 1);
    let query = create_mock_query_with_tokens(&[0, 1], &query_index);
    let matches = hash_match(&index, &query.whole_query_run());

    assert!(
        matches.is_empty(),
        "Should return empty list when no match found"
    );
}

#[test]
fn test_hash_match_with_match() {
    let mut index = create_test_index(&[("mit", 0), ("license", 1), ("apache", 2), ("2.0", 3)], 2);

    let rules_by_rid = create_test_rules_by_rid();
    let tids_by_rid = vec![tids(&[0, 1]), tids(&[2, 3, 4])];

    index.rid_by_hash.insert(compute_hash(&tids(&[0, 1])), 0);
    index.rules_by_rid = rules_by_rid;
    index.tids_by_rid = tids_by_rid;

    let query_index = create_test_index(&[("token", 0)], 1);
    let query = create_mock_query_with_tokens(&[0, 1], &query_index);
    let matches = hash_match(&index, &query.whole_query_run());

    assert_eq!(matches.len(), 1, "Should return exactly one match");
    assert_eq!(matches[0].matcher, MATCH_HASH);
    assert_eq!(matches[0].score, MatchScore::MAX);
    assert_eq!(matches[0].match_coverage, 100.0);
}

#[test]
fn test_hash_match_uses_precomputed_spdx_expression() {
    let mut index = create_test_index(&[("mit", 0), ("license", 1)], 2);

    index.rid_by_hash.insert(compute_hash(&tids(&[0, 1])), 0);
    index.rules_by_rid = vec![create_test_rules_by_rid()[0].clone()];
    index.tids_by_rid = vec![tids(&[0, 1])];
    index.rule_metadata_by_identifier.insert(
        "mit.LICENSE".to_string(),
        IndexedRuleMetadata {
            license_expression_spdx: Some("MIT".to_string()),
            skip_for_required_phrase_generation: false,
            replaced_by: vec![],
        },
    );

    let query_index = create_test_index(&[("token", 0)], 1);
    let query = create_mock_query_with_tokens(&[0, 1], &query_index);
    let matches = hash_match(&index, &query.whole_query_run());

    assert_eq!(matches[0].license_expression_spdx.as_deref(), Some("MIT"));
}

#[test]
fn test_hash_match_hispan_filters_legalese() {
    let mut index = create_test_index(&[("mit", 0), ("license", 1), ("apache", 2), ("2.0", 3)], 2);

    let rules_by_rid = create_test_rules_by_rid();
    let tids_by_rid = vec![tids(&[0, 1]), tids(&[2, 3, 4])];

    index.rid_by_hash.insert(compute_hash(&tids(&[0, 1])), 0);
    index.rules_by_rid = rules_by_rid;
    index.tids_by_rid = tids_by_rid;

    let query_index = create_test_index(&[("token", 0)], 1);
    let query = create_mock_query_with_tokens(&[0, 1], &query_index);
    let matches = hash_match(&index, &query.whole_query_run());

    assert_eq!(matches.len(), 1);
}

#[test]
fn test_match_hash_empty_tokens() {
    let tokens = tids(&[]);
    let hash = compute_hash(&tokens);

    assert_eq!(hash.len(), 20);
}

#[test]
fn test_match_hash_large_tokens() {
    let tokens: Vec<TokenId> = (0..1000).map(TokenId::new).collect();
    let hash = compute_hash(&tokens);

    assert_eq!(hash.len(), 20);

    let hash2 = compute_hash(&tokens);
    assert_eq!(hash, hash2);
}

#[test]
fn test_match_hash_single_token() {
    let tokens = tids(&[42]);
    let hash = compute_hash(&tokens);

    assert_eq!(hash.len(), 20);

    let hash2 = compute_hash(&tokens);
    assert_eq!(hash, hash2, "Same single token should produce same hash");
}

#[test]
fn test_match_hash_max_token_values() {
    let tokens = tids(&[u16::MAX, u16::MAX - 1, 0]);
    let hash = compute_hash(&tokens);

    assert_eq!(hash.len(), 20);

    let tokens2 = tids(&[u16::MAX, u16::MAX - 1, 0]);
    let hash2 = compute_hash(&tokens2);

    assert_eq!(
        hash, hash2,
        "Same max token values should produce same hash"
    );
}

#[test]
fn test_hash_match_multiple_rules_same_hash() {
    let mut index = create_test_index(&[("mit", 0), ("license", 1), ("apache", 2), ("2.0", 3)], 2);

    let rules_by_rid = create_test_rules_by_rid();
    let tids_by_rid = vec![tids(&[0, 1]), tids(&[2, 3, 4])];

    index.rid_by_hash.insert(compute_hash(&tids(&[0, 1])), 0);
    index.rid_by_hash.insert(compute_hash(&tids(&[0, 1])), 1);
    index.rules_by_rid = rules_by_rid;
    index.tids_by_rid = tids_by_rid;

    let query_index = create_test_index(&[("token", 0)], 1);
    let query = create_mock_query_with_tokens(&[0, 1], &query_index);
    let matches = hash_match(&index, &query.whole_query_run());

    assert_eq!(
        matches.len(),
        1,
        "Should return only one match even with hash collision"
    );
}

#[test]
fn test_hash_match_returns_correct_license_expression() {
    let mut index = create_test_index(&[("mit", 0), ("license", 1)], 2);

    let rules_by_rid = create_test_rules_by_rid();
    let tids_by_rid = vec![tids(&[0, 1])];

    index.rid_by_hash.insert(compute_hash(&tids(&[0, 1])), 0);
    index.rules_by_rid = rules_by_rid;
    index.tids_by_rid = tids_by_rid;

    let query_index = create_test_index(&[("token", 0)], 1);
    let query = create_mock_query_with_tokens(&[0, 1], &query_index);
    let matches = hash_match(&index, &query.whole_query_run());

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].license_expression, "mit");
    assert_eq!(matches[0].matcher, MATCH_HASH);
    assert_eq!(matches[0].score, MatchScore::MAX);
    assert_eq!(matches[0].match_coverage, 100.0);
}
