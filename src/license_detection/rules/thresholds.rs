//! Compute match thresholds for license detection rules.

/// Minimum match length for token-based matching.
pub const MIN_MATCH_LENGTH: usize = 4;

/// Minimum match length for high-value (legalese) token matching.
pub const MIN_MATCH_HIGH_LENGTH: usize = 3;

/// Rules shorter than this are considered "small" (exact match only).
pub const SMALL_RULE: usize = 15;

/// Rules shorter than this are considered "tiny" (very short, special handling).
pub const TINY_RULE: usize = 6;

/// Compute thresholds considering the occurrence of all tokens.
///
/// This function computes the minimum match thresholds based on the total
/// length of the rule and the count of high-value (legalese) tokens.
///
/// # Arguments
///
/// * `minimum_coverage` - Required coverage percentage (0-100), None if not specified
/// * `length` - Total number of tokens in the rule
/// * `high_length` - Total count of legalese token occurrences
///
/// # Returns
///
/// A tuple of (updated_minimum_coverage, min_matched_length, min_high_matched_length)
pub fn compute_thresholds_occurrences(
    minimum_coverage: Option<u8>,
    length: usize,
    high_length: usize,
) -> (Option<u8>, usize, usize) {
    if minimum_coverage == Some(100) {
        return (minimum_coverage, length, high_length);
    }

    let (min_matched_length, min_high_matched_length, updated_coverage) = if length < 3 {
        (length, high_length, Some(100))
    } else if length < 10 {
        (length, high_length, Some(80))
    } else if length < 30 {
        (length / 2, high_length.min(MIN_MATCH_HIGH_LENGTH), Some(50))
    } else if length < 200 {
        (
            MIN_MATCH_LENGTH,
            high_length.min(MIN_MATCH_HIGH_LENGTH),
            minimum_coverage,
        )
    } else {
        (length / 10, high_length / 10, minimum_coverage)
    };

    (
        updated_coverage,
        min_matched_length,
        min_high_matched_length,
    )
}

/// Compute thresholds considering the occurrence of only unique tokens.
///
/// This function computes the minimum match thresholds based on the number of
/// unique tokens in the rule and the count of unique high-value (legalese) tokens.
///
/// # Arguments
///
/// * `minimum_coverage` - Required coverage percentage (0-100), None if not specified
/// * `length` - Total number of tokens in the rule
/// * `length_unique` - Count of unique token IDs in the rule
/// * `high_length_unique` - Count of unique legalese token IDs
///
/// # Returns
///
/// A tuple of (min_matched_length_unique, min_high_matched_length_unique)
pub fn compute_thresholds_unique(
    minimum_coverage: Option<u8>,
    length: usize,
    length_unique: usize,
    high_length_unique: usize,
) -> (usize, usize) {
    if minimum_coverage == Some(100) {
        return (length_unique, high_length_unique);
    }

    if length > 200 {
        (length / 10, high_length_unique / 10)
    } else if length < 5 {
        (length_unique, high_length_unique)
    } else if length < 10 {
        let min_matched = if length_unique < 2 {
            length_unique
        } else {
            length_unique - 1
        };
        (min_matched, high_length_unique)
    } else if length < 20 {
        (high_length_unique, high_length_unique)
    } else {
        let half = high_length_unique / 2;
        let high_u = if half > 0 { half } else { high_length_unique };
        (MIN_MATCH_LENGTH, high_u.min(MIN_MATCH_HIGH_LENGTH))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_thresholds_occurrences_100_coverage() {
        let (cov, min_len, min_high_len) = compute_thresholds_occurrences(Some(100), 50, 20);
        assert_eq!(cov, Some(100));
        assert_eq!(min_len, 50);
        assert_eq!(min_high_len, 20);
    }

    #[test]
    fn test_compute_thresholds_occurrences_tiny_rule() {
        let (cov, min_len, min_high_len) = compute_thresholds_occurrences(None, 2, 1);
        assert_eq!(cov, Some(100));
        assert_eq!(min_len, 2);
        assert_eq!(min_high_len, 1);
    }

    #[test]
    fn test_compute_thresholds_occurrences_small_rule() {
        let (cov, min_len, min_high_len) = compute_thresholds_occurrences(None, 8, 3);
        assert_eq!(cov, Some(80));
        assert_eq!(min_len, 8);
        assert_eq!(min_high_len, 3);
    }

    #[test]
    fn test_compute_thresholds_occurrences_medium_rule() {
        let (cov, min_len, min_high_len) = compute_thresholds_occurrences(None, 25, 10);
        assert_eq!(cov, Some(50));
        assert_eq!(min_len, 12);
        assert_eq!(min_high_len, 3);
    }

    #[test]
    fn test_compute_thresholds_occurrences_large_rule() {
        let (cov, min_len, min_high_len) = compute_thresholds_occurrences(None, 100, 40);
        assert_eq!(cov, None);
        assert_eq!(min_len, 4);
        assert_eq!(min_high_len, 3);
    }

    #[test]
    fn test_compute_thresholds_occurrences_very_large_rule() {
        let (cov, min_len, min_high_len) = compute_thresholds_occurrences(None, 500, 200);
        assert_eq!(cov, None);
        assert_eq!(min_len, 50);
        assert_eq!(min_high_len, 20);
    }

    #[test]
    fn test_compute_thresholds_unique_100_coverage() {
        let (min_len, min_high_len) = compute_thresholds_unique(Some(100), 50, 30, 15);
        assert_eq!(min_len, 30);
        assert_eq!(min_high_len, 15);
    }

    #[test]
    fn test_compute_thresholds_unique_very_large() {
        let (min_len, min_high_len) = compute_thresholds_unique(None, 500, 300, 150);
        assert_eq!(min_len, 50);
        assert_eq!(min_high_len, 15);
    }

    #[test]
    fn test_compute_thresholds_unique_tiny() {
        let (min_len, min_high_len) = compute_thresholds_unique(None, 3, 2, 1);
        assert_eq!(min_len, 2);
        assert_eq!(min_high_len, 1);
    }

    #[test]
    fn test_compute_thresholds_unique_small() {
        let (min_len, min_high_len) = compute_thresholds_unique(None, 8, 5, 3);
        assert_eq!(min_len, 4);
        assert_eq!(min_high_len, 3);
    }

    #[test]
    fn test_compute_thresholds_unique_medium() {
        let (min_len, min_high_len) = compute_thresholds_unique(None, 15, 10, 5);
        assert_eq!(min_len, 5);
        assert_eq!(min_high_len, 5);
    }

    #[test]
    fn test_compute_thresholds_unique_large() {
        let (min_len, min_high_len) = compute_thresholds_unique(None, 100, 40, 20);
        assert_eq!(min_len, 4);
        assert_eq!(min_high_len, 3);
    }

    #[test]
    fn test_constants() {
        assert_eq!(MIN_MATCH_LENGTH, 4);
        assert_eq!(MIN_MATCH_HIGH_LENGTH, 3);
        assert_eq!(SMALL_RULE, 15);
        assert_eq!(TINY_RULE, 6);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::super::super::index::dictionary::{TokenDictionary, TokenId};
    use super::super::super::models::Rule;
    use super::*;
    use crate::license_detection::{TokenMultiset, TokenSet};
    use std::collections::HashMap;

    /// Helper function to create a rule with mock tokens and compute thresholds.
    fn create_rule_with_thresholds(
        text: String,
        tokens: Vec<u16>,
        minimum_coverage: Option<u8>,
        len_legalese: usize,
    ) -> Rule {
        let tokens: Vec<TokenId> = tokens.into_iter().map(TokenId::new).collect();
        let mut rule = Rule {
            identifier: "test.RULE".to_string(),
            license_expression: "mit".to_string(),
            text,
            tokens: tokens.clone(),
            rule_kind: crate::license_detection::models::RuleKind::None,
            is_false_positive: false,
            is_required_phrase: false,
            is_from_license: false,
            relevance: 100,
            minimum_coverage,
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

        // Build token sets and multisets
        let legalese_entries: Vec<(String, u16)> = (0..len_legalese)
            .map(|i| (format!("legalese-{i}"), i as u16))
            .collect();
        let dictionary = TokenDictionary::new_with_legalese(
            &legalese_entries
                .iter()
                .map(|(token, id)| (token.as_str(), *id))
                .collect::<Vec<_>>(),
        );
        let tids_set = TokenSet::from_token_ids(tokens.iter().copied());
        let tids_mset = TokenMultiset::from_token_ids(&tokens);
        let tids_set_high = tids_set.high_subset(&dictionary);
        let tids_mset_high = tids_mset.high_subset(&dictionary);

        // Compute token counts
        rule.length_unique = tids_set.len();
        rule.high_length_unique = tids_set_high.len();
        rule.high_length = tids_mset_high.total_count();

        // Compute thresholds
        let (updated_coverage, min_len, min_high_len) =
            compute_thresholds_occurrences(rule.minimum_coverage, tokens.len(), rule.high_length);
        rule.minimum_coverage = updated_coverage;
        rule.min_matched_length = min_len;
        rule.min_high_matched_length = min_high_len;

        let (min_len_unique, min_high_len_unique) = compute_thresholds_unique(
            rule.minimum_coverage,
            tokens.len(),
            rule.length_unique,
            rule.high_length_unique,
        );
        rule.min_matched_length_unique = min_len_unique;
        rule.min_high_matched_length_unique = min_high_len_unique;

        // Rule classification
        rule.is_tiny = tokens.len() < TINY_RULE;
        rule.is_small = tokens.len() < SMALL_RULE;

        rule
    }

    #[test]
    fn test_threshold_computation_with_explicit_coverage() {
        // Rule with explicit 100% coverage
        // Note: len_legalese=10 means IDs 0-9 are legalese, so token 10 is NOT legalese
        let tokens = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 11]; // 11 is not legalese
        let len_legalese = 10;
        let rule = create_rule_with_thresholds(
            "MIT License text".to_string(),
            tokens.clone(),
            Some(100),
            len_legalese,
        );

        assert_eq!(rule.minimum_coverage, Some(100));
        assert_eq!(rule.min_matched_length, 10);
        assert_eq!(rule.min_high_matched_length, 9); // Only 1-9 are legalese
        assert_eq!(rule.min_matched_length_unique, 10);
        assert_eq!(rule.min_high_matched_length_unique, 9);
    }

    #[test]
    fn test_threshold_computation_full_pipeline_small_rule() {
        // Small rule with 8 tokens
        let tokens = vec![1, 2, 3, 4, 6, 7, 12, 15]; // Some legalese (0-9), some not
        let len_legalese = 10;
        let rule = create_rule_with_thresholds(
            "MIT License text here".to_string(),
            tokens,
            None,
            len_legalese,
        );

        assert!(!rule.is_tiny);
        assert!(rule.is_small);
        assert_eq!(rule.length_unique, 8);
        assert_eq!(rule.high_length_unique, 6); // Tokens 1,2,3,4,6,7 are legalese
        assert_eq!(rule.high_length, 6);
        assert_eq!(rule.minimum_coverage, Some(80));
        assert_eq!(rule.min_matched_length, 8);
        assert_eq!(rule.min_high_matched_length, 6);
    }

    #[test]
    fn test_threshold_computation_full_pipeline_medium_rule() {
        // Medium rule with 25 tokens
        let tokens: Vec<u16> = (0..25).collect(); // Many legalese
        let len_legalese = 10;
        let rule = create_rule_with_thresholds(
            "MIT License text here with more words".to_string(),
            tokens,
            None,
            len_legalese,
        );

        assert!(!rule.is_tiny);
        assert!(!rule.is_small);
        assert_eq!(rule.length_unique, 25);
        assert_eq!(rule.high_length_unique, 10); // Only 0-9 are legalese
        assert_eq!(rule.high_length, 10);
        assert_eq!(rule.minimum_coverage, Some(50));
        assert_eq!(rule.min_matched_length, 12);
        assert_eq!(rule.min_high_matched_length, 3);
    }

    #[test]
    fn test_threshold_computation_full_pipeline_tiny_rule() {
        // Tiny rule with 3 tokens
        // Note: In Python, length >= 3 AND length < 10 gives 80% coverage
        // Only length < 3 gives 100% coverage
        let tokens = vec![1, 2, 3]; // All legalese
        let len_legalese = 10;
        let rule =
            create_rule_with_thresholds("MIT License".to_string(), tokens, None, len_legalese);

        // TINY_RULE is 6, so a 3-token rule IS tiny and is_small
        assert!(rule.is_tiny);
        assert!(rule.is_small);
        assert_eq!(rule.length_unique, 3);
        assert_eq!(rule.high_length_unique, 3);
        assert_eq!(rule.high_length, 3);
        // For length >= 3 and < 10, coverage is 80%
        assert_eq!(rule.minimum_coverage, Some(80));
        assert_eq!(rule.min_matched_length, 3);
        assert_eq!(rule.min_high_matched_length, 3);
    }

    #[test]
    fn test_threshold_computation_unique_token_counts() {
        // Rule with repeated tokens to test unique counting
        let tokens = vec![1, 2, 3, 1, 2, 3, 1, 2, 3, 1, 2, 3];
        let len_legalese = 10;
        let rule = create_rule_with_thresholds(
            "MIT License MIT License MIT".to_string(),
            tokens,
            None,
            len_legalese,
        );

        assert_eq!(rule.length_unique, 3); // Only 3 unique tokens
        assert_eq!(rule.high_length_unique, 3); // All are legalese
        assert_eq!(rule.high_length, 12); // But 12 total occurrences
    }

    #[test]
    fn test_threshold_computation_no_high_tokens() {
        // Rule with no legalese tokens (weak rule)
        let tokens: Vec<u16> = (10..20).collect(); // All IDs >= len_legalese
        let len_legalese = 10;
        let rule = create_rule_with_thresholds(
            "Some text without legal words".to_string(),
            tokens,
            None,
            len_legalese,
        );

        assert_eq!(rule.high_length_unique, 0);
        assert_eq!(rule.high_length, 0);
        assert_eq!(rule.min_high_matched_length, 0);
    }
}
