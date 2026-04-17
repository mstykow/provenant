//! Shared test utilities for license detection tests.
//!
//! This module provides common helper functions used across multiple test modules
//! to reduce code duplication and ensure consistent test setup.

use std::collections::HashMap;

use crate::license_detection::index::LicenseIndex;
use crate::license_detection::index::dictionary::{TokenDictionary, TokenId};
use crate::license_detection::models::Rule;
use crate::license_detection::position_set::PositionSet;
use crate::license_detection::query::Query;

/// Creates a test index with customizable legalese tokens.
///
/// # Arguments
/// * `legalese` - Slice of (token string, token id) pairs for the dictionary
/// * `len_legalese` - Number of high-value legalese tokens (for high_matchables)
///
/// # Returns
/// A `LicenseIndex` configured with the given dictionary
pub fn create_test_index(legalese: &[(&str, u16)], len_legalese: usize) -> LicenseIndex {
    let dictionary = TokenDictionary::new_with_legalese_pairs(
        &legalese.iter().map(|(s, i)| (*s, *i)).collect::<Vec<_>>(),
    );

    let mut index = LicenseIndex::new(dictionary);
    index.len_legalese = len_legalese;
    index
}

/// Creates a test index with default legalese tokens for general testing.
///
/// Provides common license-related tokens: mit, license, apache, 2.0
/// with len_legalese set to 2.
pub fn create_test_index_default() -> LicenseIndex {
    create_test_index(&[("mit", 0), ("license", 1), ("apache", 2), ("2.0", 3)], 2)
}

/// Creates a mock Rule for testing matchers.
///
/// # Arguments
/// * `license_expression` - The license expression (e.g., "mit", "apache-2.0")
/// * `tokens` - Token IDs for this rule
/// * `is_small` - Whether the rule is considered "small"
/// * `is_tiny` - Whether the rule is considered "tiny"
///
/// # Returns
/// A `Rule` with sensible defaults for testing
pub fn create_mock_rule(
    license_expression: &str,
    tokens: Vec<u16>,
    is_small: bool,
    is_tiny: bool,
) -> Rule {
    let tokens: Vec<TokenId> = tokens.into_iter().map(TokenId::new).collect();
    let length_unique = tokens.len();
    Rule {
        identifier: format!("{}.LICENSE", license_expression),
        license_expression: license_expression.to_string(),
        text: String::new(),
        tokens,
        rule_kind: crate::license_detection::models::RuleKind::Text,
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
        length_unique,
        high_length_unique: length_unique,
        high_length: length_unique,
        min_matched_length: 0,
        min_high_matched_length: 0,
        min_matched_length_unique: 0,
        min_high_matched_length_unique: 0,
        is_small,
        is_tiny,
        starts_with_license: false,
        ends_with_license: false,
        is_deprecated: false,
        spdx_license_key: None,
        other_spdx_license_keys: vec![],
    }
}

/// Creates a mock Rule with just a license expression and relevance.
///
/// Simplified version for tests that don't need token matching.
///
/// # Arguments
/// * `license_expression` - The license expression
/// * `relevance` - Relevance score for the rule
///
/// # Returns
/// A `Rule` with empty tokens and the given relevance
pub fn create_mock_rule_simple(license_expression: &str, relevance: u8) -> Rule {
    Rule {
        identifier: format!("{}.LICENSE", license_expression),
        license_expression: license_expression.to_string(),
        text: String::new(),
        tokens: Vec::new(),
        rule_kind: crate::license_detection::models::RuleKind::None,
        is_false_positive: false,
        is_required_phrase: false,
        is_from_license: false,
        relevance,
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
    }
}

/// Creates a mock Query with the given tokens.
///
/// # Arguments
/// * `tokens` - Token IDs for the query
/// * `index` - The license index (borrowed)
///
/// # Returns
/// A `Query` spanning the entire token range
pub fn create_mock_query_with_tokens<'a>(tokens: &[u16], index: &'a LicenseIndex) -> Query<'a> {
    let tokens: Vec<TokenId> = tokens.iter().copied().map(TokenId::new).collect();
    let token_count = tokens.len();
    let line_by_pos = vec![1; token_count];

    Query {
        text: String::new(),
        tokens,
        line_by_pos,
        unknowns_by_pos: HashMap::new(),
        stopwords_by_pos: HashMap::new(),
        shorts_and_digits_pos: PositionSet::new(),
        high_matchables: (0..token_count).collect(),
        low_matchables: PositionSet::new(),
        is_binary: false,
        query_run_ranges: Vec::new(),
        spdx_lines: Vec::new(),
        index,
    }
}
