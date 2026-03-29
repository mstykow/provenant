//! Approximate sequence matching for license detection.
//!
//! This module implements sequence-based matching using set similarity for
//! candidate selection, followed by sequence alignment to find matching blocks.
//!
//! Based on Python ScanCode Toolkit implementation at:
//! reference/scancode-toolkit/src/licensedcode/match_seq.py
//!
//! ## Near-Duplicate Detection
//!
//! This module implements Phase 2 of Python's 3-phase matching pipeline:
//! 1. Phase 1: Hash & Aho-Corasick (exact matches)
//! 2. Phase 2: Near-duplicate detection - check whole file for high-resemblance candidates
//! 3. Phase 3: Query run matching (if no near-duplicates found)
//!
//! The near-duplicate detection finds rules with high resemblance (>= 0.8) to the
//! entire query, which helps match combined rules instead of partial rules.

mod candidates;
mod matching;

#[cfg(test)]
mod gfdl_debug_test;

pub use candidates::{Candidate, compute_candidates_with_msets};
pub use matching::seq_match_with_candidates;

use crate::license_detection::models::MatcherKind;

pub const MATCH_SEQ: MatcherKind = MatcherKind::Seq;

/// Default threshold for high resemblance (0.8 = 80% similarity).
pub const HIGH_RESEMBLANCE_THRESHOLD: f32 = 0.8;

/// Default number of top near-duplicate candidates to consider.
pub const MAX_NEAR_DUPE_CANDIDATES: usize = 10;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license_detection::index::LicenseIndex;
    use crate::license_detection::index::dictionary::{TokenId, TokenKind};
    use crate::license_detection::index::token_sets::build_set_and_mset;
    use crate::license_detection::models::Rule;
    use crate::license_detection::query::Query;
    use crate::license_detection::test_utils::create_test_index;
    use std::collections::{HashMap, HashSet};

    pub(super) fn create_seq_match_test_index() -> LicenseIndex {
        create_test_index(
            &[
                ("license", 0),
                ("copyright", 1),
                ("permission", 2),
                ("redistribute", 3),
                ("granted", 4),
            ],
            5,
        )
    }

    pub(super) fn add_test_rule(index: &mut LicenseIndex, text: &str, expression: &str) -> usize {
        let rid = index.rules_by_rid.len();
        let tokens: Vec<TokenId> = text
            .split_whitespace()
            .filter_map(|word| index.dictionary.get(word))
            .collect();

        let (set, mset) = build_set_and_mset(&tokens);
        let _ = index.sets_by_rid.insert(rid, set.clone());
        let _ = index.msets_by_rid.insert(rid, mset);

        let high_set: HashSet<TokenId> = set
            .iter()
            .filter(|&&tid| index.dictionary.token_kind(tid) == TokenKind::Legalese)
            .copied()
            .collect();
        if !high_set.is_empty() {
            let _ = index.high_sets_by_rid.insert(rid, high_set);
        }

        let mut high_postings: HashMap<TokenId, Vec<usize>> = HashMap::new();
        for (pos, &tid) in tokens.iter().enumerate() {
            if index.dictionary.token_kind(tid) == TokenKind::Legalese {
                high_postings.entry(tid).or_default().push(pos);
            }
        }
        let _ = index.high_postings_by_rid.insert(rid, high_postings);

        let rule = Rule {
            identifier: format!("{}.test", expression),
            license_expression: expression.to_string(),
            text: text.to_string(),
            tokens: tokens.clone(),
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
            length_unique: tokens.len(),
            high_length_unique: tokens
                .iter()
                .filter(|&&t| index.dictionary.token_kind(t) == TokenKind::Legalese)
                .count(),
            high_length: tokens.len(),
            min_matched_length: 1,
            min_high_matched_length: 1,
            min_matched_length_unique: 1,
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

        index.rules_by_rid.push(rule.clone());
        index.tids_by_rid.push(tokens.clone());
        index.approx_matchable_rids.insert(rid);

        // Also populate inverted index for high-value tokens
        for &tid in &tokens {
            if index.dictionary.token_kind(tid) == TokenKind::Legalese {
                index.rids_by_high_tid.entry(tid).or_default().insert(rid);
            }
        }

        rid
    }

    #[test]
    fn test_seq_match_basic() {
        let mut index = create_seq_match_test_index();

        add_test_rule(&mut index, "license copyright granted", "test-license");

        let text = "license copyright granted here";
        let query = Query::from_extracted_text(text, &index, false).unwrap();
        let query_run = query.whole_query_run();

        let candidates = compute_candidates_with_msets(&index, &query_run, false, 50);
        let matches = seq_match_with_candidates(&index, &query_run, &candidates);

        assert!(!matches.is_empty());
        assert_eq!(matches[0].matcher, MATCH_SEQ);
    }

    #[test]
    fn test_seq_match_partial_coverage_not_filtered() {
        let mut index = create_seq_match_test_index();

        add_test_rule(
            &mut index,
            "license copyright granted permission redistribute",
            "test-long-license",
        );

        let text = "license copyright";
        let query = Query::from_extracted_text(text, &index, false).unwrap();
        let query_run = query.whole_query_run();

        let candidates = compute_candidates_with_msets(&index, &query_run, false, 50);
        let matches = seq_match_with_candidates(&index, &query_run, &candidates);

        assert!(
            !matches.is_empty(),
            "Partial coverage matches should NOT be filtered (Python has no 50% coverage filter)"
        );
        assert!(matches[0].match_coverage < 50.0);
    }

    #[test]
    fn test_seq_match_empty_query() {
        let mut index = create_seq_match_test_index();

        add_test_rule(&mut index, "license copyright", "test-license");

        let text = "";
        let query = Query::from_extracted_text(text, &index, false).unwrap();
        let query_run = query.whole_query_run();

        let candidates = compute_candidates_with_msets(&index, &query_run, false, 50);
        let matches = seq_match_with_candidates(&index, &query_run, &candidates);

        assert!(matches.is_empty());
    }

    #[test]
    fn test_seq_match_constants() {
        assert_eq!(MATCH_SEQ.as_str(), "3-seq");
        assert_eq!(MATCH_SEQ.precedence(), 3);
    }

    #[test]
    fn test_seq_match_with_no_legalese_intersection() {
        let mut index = create_test_index(&[("word1", 10), ("word2", 11), ("word3", 12)], 5);

        add_test_rule(&mut index, "word1 word2 word3", "test-license");

        let text = "word1 word2 word3";
        let query = Query::from_extracted_text(text, &index, false).unwrap();
        let query_run = query.whole_query_run();

        let candidates = compute_candidates_with_msets(&index, &query_run, false, 50);
        let matches = seq_match_with_candidates(&index, &query_run, &candidates);

        assert!(
            matches.is_empty(),
            "Should not match when tokens are not legalese (above len_legalese)"
        );
    }

    #[test]
    fn test_seq_match_multiple_occurrences() {
        let mut index = create_seq_match_test_index();

        add_test_rule(&mut index, "license copyright granted", "test-license");

        let text = "license copyright granted some text license copyright granted more text";
        let query = Query::from_extracted_text(text, &index, false).unwrap();
        let query_run = query.whole_query_run();

        let candidates = compute_candidates_with_msets(&index, &query_run, false, 50);
        let matches = seq_match_with_candidates(&index, &query_run, &candidates);

        assert!(
            matches.len() >= 2,
            "Should find multiple matches for the same rule appearing multiple times in query, got {} matches",
            matches.len()
        );

        let license_expressions: Vec<&str> =
            matches.iter().map(|m| m.license_expression()).collect();
        assert!(
            license_expressions.iter().all(|&e| e == "test-license"),
            "All matches should be for test-license"
        );

        let start_lines: Vec<usize> = matches.iter().map(|m| m.start_line).collect();
        let end_lines: Vec<usize> = matches.iter().map(|m| m.end_line).collect();

        assert!(
            start_lines.iter().all(|&l| l >= 1),
            "Start lines should be valid"
        );
        assert!(
            end_lines.iter().all(|&l| l >= 1),
            "End lines should be valid"
        );
    }

    #[test]
    fn test_seq_match_line_numbers_accurate() {
        let mut index = create_seq_match_test_index();

        add_test_rule(&mut index, "license copyright granted", "test-license");

        let text = "line one\nlicense copyright granted\nline three";
        let query = Query::from_extracted_text(text, &index, false).unwrap();
        let query_run = query.whole_query_run();

        let candidates = compute_candidates_with_msets(&index, &query_run, false, 50);
        let matches = seq_match_with_candidates(&index, &query_run, &candidates);

        assert!(!matches.is_empty(), "Should find matches");

        let first_match = &matches[0];

        assert_eq!(
            first_match.start_line, 2,
            "Match should start on line 2 (where license tokens are), not line 1"
        );
        assert_eq!(
            first_match.end_line, 2,
            "Match should end on line 2 (where license tokens are), not line 3"
        );

        // matched_text is computed lazily at output time, not during matching
        assert!(
            first_match.matched_text.is_none(),
            "matched_text should be None during matching (computed lazily at output)"
        );

        // Verify we can compute it from the query
        let matched_text = query.matched_text(first_match.start_line, first_match.end_line);
        assert!(
            matched_text.contains("license"),
            "Computed matched text should contain 'license'"
        );
    }

    #[test]
    fn test_seq_match_line_numbers_partial_match() {
        let mut index = create_seq_match_test_index();

        add_test_rule(
            &mut index,
            "license copyright granted permission",
            "test-license",
        );

        let text = "line one\nlicense copyright\nline three";
        let query = Query::from_extracted_text(text, &index, false).unwrap();
        let query_run = query.whole_query_run();

        let candidates = compute_candidates_with_msets(&index, &query_run, false, 50);
        let matches = seq_match_with_candidates(&index, &query_run, &candidates);

        assert!(!matches.is_empty(), "Should find partial matches");

        let first_match = &matches[0];

        assert_eq!(
            first_match.start_line, 2,
            "Partial match should start on line 2"
        );
        assert_eq!(
            first_match.end_line, 2,
            "Partial match should end on line 2"
        );

        assert!(
            first_match.match_coverage < 100.0,
            "Should be partial coverage"
        );
    }
}
