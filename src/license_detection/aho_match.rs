// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Aho-Corasick exact matching for license detection.
//!
//! This module implements Aho-Corasick multi-pattern matching for license detection.
//! Token sequences from rules are encoded as bytes and used to build the automaton,
//! which can then efficiently find all matches in query token sequences.
//!
//! Based on the Python ScanCode Toolkit implementation at:
//! reference/scancode-toolkit/src/licensedcode/match_aho.py

use crate::license_detection::index::LicenseIndex;
use crate::license_detection::index::dictionary::{TokenId, TokenKind};
use crate::license_detection::models::position_span::PositionSpan;
use crate::license_detection::models::{LicenseMatch, MatchCoordinates, MatcherKind};
use crate::license_detection::position_set::PositionSet;
use crate::license_detection::query::QueryRun;
use crate::models::LineNumber;
use crate::models::MatchScore;
use std::time::Instant;

pub const MATCH_AHO: MatcherKind = MatcherKind::Aho;

/// Encode u16 token sequence as bytes.
///
/// Each token is encoded as 2 bytes in little-endian format.
/// This is necessary because Aho-Corasick works on bytes, not u16 values directly.
///
/// # Arguments
/// * `tokens` - Slice of token IDs to encode
///
/// # Returns
/// Byte vector where each token is represented as 2 little-endian bytes
fn tokens_to_bytes(tokens: &[TokenId]) -> Vec<u8> {
    tokens.iter().flat_map(|t| t.to_le_bytes()).collect()
}

/// Convert byte position to token position.
///
/// Since each token is encoded as 2 bytes, we divide the byte position by 2.
///
/// # Arguments
/// * `byte_pos` - Byte position in the encoded bytes
///
/// # Returns
/// Token position (byte_pos / 2)
#[inline]
fn byte_pos_to_token_pos(byte_pos: usize) -> usize {
    byte_pos / 2
}

/// Perform Aho-Corasick exact matching for a query run.
///
/// This function matches the query token sequence against all rules in the automaton,
/// finding all exact occurrences of rule token sequences. For each match, it verifies
/// that all positions are matchable and creates a LicenseMatch with proper coverage scores.
///
/// # Arguments
/// * `index` - The license index containing the automaton and rules
/// * `query_run` - The query run to match
///
/// # Returns
/// Vector of matches found by the Aho-Corasick automaton
///
/// Corresponds to Python: `exact_match()` (lines 84-138)
pub fn aho_match(index: &LicenseIndex, query_run: &QueryRun) -> Vec<LicenseMatch> {
    aho_match_with_extra_matchables(index, query_run, None, None)
        .expect("Aho matching without deadline should not time out")
}

pub(crate) fn aho_match_with_deadline(
    index: &LicenseIndex,
    query_run: &QueryRun,
    deadline: Option<Instant>,
) -> anyhow::Result<Vec<LicenseMatch>> {
    aho_match_with_extra_matchables(index, query_run, None, deadline)
}

/// Perform Aho-Corasick exact matching with temporary extra matchable positions.
///
/// This is used to preserve pre-subtraction SPDX positions for Phase 1c exact AHO
/// eligibility checks only, while keeping the live query matchables unchanged for
/// all later phases.
pub fn aho_match_with_extra_matchables(
    index: &LicenseIndex,
    query_run: &QueryRun,
    extra_matchable_positions: Option<&PositionSet>,
    deadline: Option<Instant>,
) -> anyhow::Result<Vec<LicenseMatch>> {
    let mut matches = Vec::new();

    let query_tokens = query_run.tokens();
    if query_tokens.is_empty() {
        return Ok(matches);
    }

    let encoded_query = tokens_to_bytes(query_tokens);
    let qbegin = query_run.start;

    let matchables = query_run.matchables(true);

    let automaton = &index.rules_automaton;

    for (match_index, ac_match) in automaton.find_overlapping_iter(&encoded_query).enumerate() {
        if match_index.is_multiple_of(1024) {
            crate::license_detection::ensure_within_deadline(deadline)?;
        }

        let pattern_id = ac_match.pattern;
        let byte_start = ac_match.start;
        let byte_end = ac_match.end;

        let qstart = qbegin + byte_pos_to_token_pos(byte_start);
        let qend = qbegin + byte_pos_to_token_pos(byte_end);

        let is_entirely_matchable = if let Some(extra_matchables) = extra_matchable_positions {
            (qstart..qend).all(|pos| matchables.contains(pos) || extra_matchables.contains(pos))
        } else {
            (qstart..qend).all(|pos| matchables.contains(pos))
        };

        if !is_entirely_matchable {
            continue;
        }

        let Some(rids) = index.pattern_id_to_rid.get(pattern_id) else {
            continue;
        };

        for &rid in rids {
            if rid >= index.rules_by_rid.len() {
                continue;
            }

            let matched_length = qend - qstart;

            // Skip zero-length matches (empty patterns)
            if matched_length == 0 {
                continue;
            }

            let rule = &index.rules_by_rid[rid];
            let rule_tids = &index.tids_by_rid[rid];
            let rule_length = rule.tokens.len();

            let match_coverage = if rule_length > 0 {
                LicenseMatch::round_metric((matched_length as f32 / rule_length as f32) * 100.0)
            } else {
                100.0
            };

            let start_line = query_run
                .line_for_pos(qstart)
                .and_then(LineNumber::new)
                .unwrap_or(LineNumber::ONE);

            let end_line = if qend > qstart {
                query_run
                    .line_for_pos(qend.saturating_sub(1))
                    .and_then(LineNumber::new)
                    .unwrap_or(start_line)
            } else {
                start_line
            };

            let score = if rule_length > 0 {
                MatchScore::from_percentage((matched_length as f64 / rule_length as f64) * 100.0)
            } else {
                MatchScore::MAX
            };

            let qspan = PositionSpan::range(qstart, qend);
            let ispan = PositionSpan::range(0, matched_length);
            let hispan = PositionSpan::from_positions(
                (0..matched_length)
                    .filter(|&p| index.dictionary.token_kind(rule_tids[p]) == TokenKind::Legalese),
            );

            let license_match = LicenseMatch {
                license_expression: rule.license_expression.clone(),
                license_expression_spdx: index
                    .rule_metadata_by_identifier
                    .get(&rule.identifier)
                    .and_then(|metadata| metadata.license_expression_spdx.clone()),
                from_file: None,
                start_line,
                end_line,
                start_token: qstart,
                end_token: qend,
                matcher: MATCH_AHO,
                score,
                matched_length,
                rule_length,
                match_coverage,
                rule_relevance: rule.relevance,
                rid,
                rule_identifier: rule.identifier.clone(),
                rule_url: rule.rule_url().unwrap_or_default(),
                matched_text: None,
                referenced_filenames: rule.referenced_filenames.clone(),
                rule_kind: rule.kind(),
                is_from_license: rule.is_from_license,
                rule_start_token: 0,
                coordinates: MatchCoordinates::rule_aligned(qspan, ispan, hispan),
                candidate_resemblance: 0.0,
                candidate_containment: 0.0,
            };

            matches.push(license_match);
        }
    }

    if let Some(extra_matchables) = extra_matchable_positions {
        crate::license_detection::ensure_within_deadline(deadline)?;
        let live_matchables = query_run.matchables(true);
        matches = matches
            .iter()
            .enumerate()
            .filter(|(i, m)| {
                if !m.is_license_reference() {
                    return true;
                }

                let uses_extra =
                    (m.start_token..m.end_token).any(|pos| extra_matchables.contains(pos));
                let uses_live = (m.start_token..m.end_token)
                    .any(|pos| live_matchables.contains(pos) && !extra_matchables.contains(pos));

                if !(uses_extra && uses_live) {
                    return true;
                }

                !matches.iter().enumerate().any(|(j, inner)| {
                    j != *i
                        && inner.is_license_reference()
                        && inner.rule_identifier.starts_with("spdx_license_id_")
                        && inner.license_expression == m.license_expression
                        && inner.start_token >= m.start_token
                        && inner.end_token <= m.end_token
                        && (inner.start_token..inner.end_token)
                            .all(|pos| extra_matchables.contains(pos))
                })
            })
            .map(|(_, m)| m.clone())
            .collect();
    }

    Ok(matches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license_detection::automaton::AutomatonBuilder;
    use crate::license_detection::index::IndexedRuleMetadata;
    use crate::license_detection::index::dictionary::{TokenId, tid};
    use crate::license_detection::test_utils::{
        create_mock_query_with_tokens, create_mock_rule, create_test_index_default,
    };

    fn tids(values: &[u16]) -> Vec<TokenId> {
        values.iter().copied().map(TokenId::new).collect()
    }

    #[test]
    fn test_tokens_to_bytes_empty() {
        let tokens = tids(&[]);
        let bytes = tokens_to_bytes(&tokens);
        assert!(bytes.is_empty());
    }

    #[test]
    fn test_tokens_to_bytes_single() {
        let tokens = tids(&[1u16]);
        let bytes = tokens_to_bytes(&tokens);
        assert_eq!(bytes, vec![1, 0]);
    }

    #[test]
    fn test_tokens_to_bytes_multiple() {
        let tokens = tids(&[1u16, 2, 3, 255, 256]);
        let bytes = tokens_to_bytes(&tokens);
        assert_eq!(bytes, vec![1, 0, 2, 0, 3, 0, 255, 0, 0, 1]);
    }

    #[test]
    fn test_byte_pos_to_token_pos() {
        assert_eq!(byte_pos_to_token_pos(0), 0);
        assert_eq!(byte_pos_to_token_pos(1), 0);
        assert_eq!(byte_pos_to_token_pos(2), 1);
        assert_eq!(byte_pos_to_token_pos(3), 1);
        assert_eq!(byte_pos_to_token_pos(4), 2);
        assert_eq!(byte_pos_to_token_pos(10), 5);
    }

    #[test]
    fn test_aho_match_empty_query() {
        let index = create_test_index_default();
        let query = create_mock_query_with_tokens(&[], &index);
        let run = query.whole_query_run();

        let matches = aho_match(run.get_index(), &run);

        assert!(matches.is_empty());
    }

    #[test]
    fn test_aho_match_no_automaton_patterns() {
        let mut index = create_test_index_default();
        index.rules_automaton = AutomatonBuilder::new().build();

        let query = create_mock_query_with_tokens(&[0, 1, 2], &index);
        let run = query.whole_query_run();

        let matches = aho_match(run.get_index(), &run);

        assert!(matches.is_empty());
    }

    #[test]
    fn test_aho_match_with_simple_pattern() {
        let mut index = create_test_index_default();

        let rule_tokens = tids(&[0u16, 1]);
        let pattern_bytes = tokens_to_bytes(&rule_tokens);

        let mut builder = AutomatonBuilder::new();
        builder.add_pattern(&pattern_bytes);
        let automaton = builder.build();

        index.rules_automaton = automaton;
        index
            .rules_by_rid
            .push(create_mock_rule("mit", vec![0, 1], false, false));
        index.tids_by_rid.push(tids(&[0, 1]));
        index.pattern_id_to_rid.push(vec![0]);

        let query = crate::license_detection::query::Query {
            text: String::new(),
            tokens: tids(&[0, 1]),
            line_by_pos: vec![1, 1],
            unknowns_by_pos: std::collections::HashMap::new(),
            stopwords_by_pos: std::collections::HashMap::new(),
            shorts_and_digits_pos: PositionSet::new(),
            high_matchables: (0..2).collect(),
            low_matchables: PositionSet::new(),
            is_binary: false,
            query_run_ranges: Vec::new(),
            spdx_lines: Vec::new(),
            index: &index,
        };

        let run = query.whole_query_run();
        let matches = aho_match(run.get_index(), &run);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].matcher, MATCH_AHO);
        assert_eq!(matches[0].score, MatchScore::MAX);
        assert_eq!(matches[0].match_coverage, 100.0);
        assert!(!matches[0].rule_url.is_empty());
    }

    #[test]
    fn test_aho_match_coverage() {
        let mut index = create_test_index_default();

        let rule_tokens = tids(&[0u16, 1, 2]);
        let pattern_bytes = tokens_to_bytes(&rule_tokens);

        let mut builder = AutomatonBuilder::new();
        builder.add_pattern(&pattern_bytes);
        let automaton = builder.build();

        index.rules_automaton = automaton;
        index
            .rules_by_rid
            .push(create_mock_rule("apache-2.0", vec![0, 1, 2], false, false));
        index.tids_by_rid.push(tids(&[0, 1, 2]));
        index.pattern_id_to_rid.push(vec![0]);

        let query = crate::license_detection::query::Query {
            text: String::new(),
            tokens: tids(&[0, 1, 2]),
            line_by_pos: vec![1, 1, 1],
            unknowns_by_pos: std::collections::HashMap::new(),
            stopwords_by_pos: std::collections::HashMap::new(),
            shorts_and_digits_pos: PositionSet::new(),
            high_matchables: (0..3).collect(),
            low_matchables: PositionSet::new(),
            is_binary: false,
            query_run_ranges: Vec::new(),
            spdx_lines: Vec::new(),
            index: &index,
        };

        let run = query.whole_query_run();
        let matches = aho_match(run.get_index(), &run);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].matched_length, 3);
        assert_eq!(matches[0].match_coverage, 100.0);
    }

    #[test]
    fn test_aho_match_multiple_patterns() {
        let mut index = create_test_index_default();

        let pattern1 = tokens_to_bytes(&tids(&[0u16, 1]));
        let pattern2 = tokens_to_bytes(&tids(&[2u16, 3]));

        let mut builder = AutomatonBuilder::new();
        builder.add_pattern(&pattern1);
        builder.add_pattern(&pattern2);
        let automaton = builder.build();

        index.rules_automaton = automaton;
        index
            .rules_by_rid
            .push(create_mock_rule("mit", vec![0, 1], true, false));
        index
            .rules_by_rid
            .push(create_mock_rule("apache-2.0", vec![2, 3], true, false));
        index.tids_by_rid.push(tids(&[0, 1]));
        index.tids_by_rid.push(tids(&[2, 3]));
        index.pattern_id_to_rid.push(vec![0]);
        index.pattern_id_to_rid.push(vec![1]);

        let query = crate::license_detection::query::Query {
            text: String::new(),
            tokens: tids(&[0, 1, 2, 3]),
            line_by_pos: vec![1, 1, 2, 2],
            unknowns_by_pos: std::collections::HashMap::new(),
            stopwords_by_pos: std::collections::HashMap::new(),
            shorts_and_digits_pos: PositionSet::new(),
            high_matchables: (0..4).collect(),
            low_matchables: PositionSet::new(),
            is_binary: false,
            query_run_ranges: Vec::new(),
            spdx_lines: Vec::new(),
            index: &index,
        };

        let run = query.whole_query_run();
        let matches = aho_match(run.get_index(), &run);

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].license_expression, "mit");
        assert_eq!(matches[0].matched_length, 2);
        assert_eq!(matches[1].license_expression, "apache-2.0");
        assert_eq!(matches[1].matched_length, 2);
    }

    #[test]
    fn test_aho_match_filters_non_matchable() {
        let mut index = create_test_index_default();

        let pattern = tokens_to_bytes(&tids(&[0u16, 1, 2]));

        let mut builder = AutomatonBuilder::new();
        builder.add_pattern(&pattern);
        let automaton = builder.build();

        index.rules_automaton = automaton;
        index
            .rules_by_rid
            .push(create_mock_rule("mit", vec![0, 1, 2], false, false));
        index.tids_by_rid.push(tids(&[0, 1, 2]));
        index.pattern_id_to_rid.push(vec![0]);

        let query = crate::license_detection::query::Query {
            text: String::new(),
            tokens: tids(&[0, 1, 2]),
            line_by_pos: vec![1, 1, 1],
            unknowns_by_pos: std::collections::HashMap::new(),
            stopwords_by_pos: std::collections::HashMap::new(),
            shorts_and_digits_pos: PositionSet::new(),
            high_matchables: PositionSet::new(),
            low_matchables: PositionSet::new(),
            is_binary: false,
            query_run_ranges: Vec::new(),
            spdx_lines: Vec::new(),
            index: &index,
        };

        let run = query.whole_query_run();
        let matches = aho_match(run.get_index(), &run);

        assert!(
            matches.is_empty(),
            "Should not match non-matchable positions"
        );
    }

    #[test]
    fn test_aho_match_with_extra_matchables_restores_subtracted_positions_for_eligibility() {
        let mut index = create_test_index_default();

        let pattern = tokens_to_bytes(&tids(&[0u16, 1, 2]));

        let mut builder = AutomatonBuilder::new();
        builder.add_pattern(&pattern);
        let automaton = builder.build();

        index.rules_automaton = automaton;
        index
            .rules_by_rid
            .push(create_mock_rule("mit", vec![0, 1, 2], false, false));
        index.tids_by_rid.push(tids(&[0, 1, 2]));
        index.pattern_id_to_rid.push(vec![0]);

        let query = crate::license_detection::query::Query {
            text: String::new(),
            tokens: tids(&[0, 1, 2]),
            line_by_pos: vec![1, 1, 1],
            unknowns_by_pos: std::collections::HashMap::new(),
            stopwords_by_pos: std::collections::HashMap::new(),
            shorts_and_digits_pos: PositionSet::new(),
            high_matchables: [0usize, 2].into_iter().collect(),
            low_matchables: PositionSet::new(),
            is_binary: false,
            query_run_ranges: Vec::new(),
            spdx_lines: Vec::new(),
            index: &index,
        };

        let run = query.whole_query_run();

        assert!(aho_match(run.get_index(), &run).is_empty());

        let extra_matchables: PositionSet = [1usize].into_iter().collect();
        let matches =
            aho_match_with_extra_matchables(run.get_index(), &run, Some(&extra_matchables), None)
                .expect("Aho matching with extra matchables should succeed");

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].start_token, 0);
        assert_eq!(matches[0].end_token, 3);
        assert_eq!(matches[0].match_coverage, 100.0);
    }

    #[test]
    fn test_aho_match_with_extra_matchables_drops_mixed_reference_when_spdx_submatch_exists() {
        let mut index = create_test_index_default();

        let short_pattern = tokens_to_bytes(&tids(&[0u16, 1]));
        let long_pattern = tokens_to_bytes(&tids(&[0u16, 1, 2]));

        let mut builder = AutomatonBuilder::new();
        builder.add_pattern(&short_pattern);
        builder.add_pattern(&long_pattern);
        let automaton = builder.build();

        index.rules_automaton = automaton;

        let mut short_rule = create_mock_rule("cecill-c", vec![0, 1], false, false);
        short_rule.identifier = "spdx_license_id_cecill-c_for_cecill-c.RULE".to_string();
        short_rule.rule_kind = crate::license_detection::models::RuleKind::Reference;
        index.rules_by_rid.push(short_rule);
        index.tids_by_rid.push(tids(&[0, 1]));
        index.pattern_id_to_rid.push(vec![0]);

        let mut long_rule = create_mock_rule("cecill-c", vec![0, 1, 2], false, false);
        long_rule.identifier = "cecill-c_3.RULE".to_string();
        long_rule.rule_kind = crate::license_detection::models::RuleKind::Reference;
        index.rules_by_rid.push(long_rule);
        index.tids_by_rid.push(tids(&[0, 1, 2]));
        index.pattern_id_to_rid.push(vec![1]);

        let query = crate::license_detection::query::Query {
            text: String::new(),
            tokens: tids(&[0, 1, 2]),
            line_by_pos: vec![1, 1, 1],
            unknowns_by_pos: std::collections::HashMap::new(),
            stopwords_by_pos: std::collections::HashMap::new(),
            shorts_and_digits_pos: PositionSet::new(),
            high_matchables: [2usize].into_iter().collect(),
            low_matchables: PositionSet::new(),
            is_binary: false,
            query_run_ranges: Vec::new(),
            spdx_lines: Vec::new(),
            index: &index,
        };

        let run = query.whole_query_run();
        let extra_matchables: PositionSet = [0usize, 1].into_iter().collect();
        let matches =
            aho_match_with_extra_matchables(run.get_index(), &run, Some(&extra_matchables), None)
                .expect("Aho matching with extra matchables should succeed");

        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].rule_identifier,
            "spdx_license_id_cecill-c_for_cecill-c.RULE"
        );
    }

    #[test]
    fn test_aho_match_line_numbers() {
        let mut index = create_test_index_default();

        let pattern = tokens_to_bytes(&tids(&[0u16, 1]));

        let mut builder = AutomatonBuilder::new();
        builder.add_pattern(&pattern);
        let automaton = builder.build();

        index.rules_automaton = automaton;
        index
            .rules_by_rid
            .push(create_mock_rule("mit", vec![0, 1], true, false));
        index.tids_by_rid.push(tids(&[0, 1]));
        index.pattern_id_to_rid.push(vec![0]);

        let query = crate::license_detection::query::Query {
            text: String::new(),
            tokens: tids(&[0, 1]),
            line_by_pos: vec![5, 5],
            unknowns_by_pos: std::collections::HashMap::new(),
            stopwords_by_pos: std::collections::HashMap::new(),
            shorts_and_digits_pos: PositionSet::new(),
            high_matchables: (0..2).collect(),
            low_matchables: PositionSet::new(),
            is_binary: false,
            query_run_ranges: Vec::new(),
            spdx_lines: Vec::new(),
            index: &index,
        };

        let run = query.whole_query_run();
        let matches = aho_match(run.get_index(), &run);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].start_line, LineNumber::new(5).expect("valid"));
        assert_eq!(matches[0].end_line, LineNumber::new(5).expect("valid"));
    }

    #[test]
    fn test_aho_match_uses_precomputed_spdx_expression() {
        let mut index = create_test_index_default();

        let pattern = tokens_to_bytes(&tids(&[0u16, 1]));
        let mut builder = AutomatonBuilder::new();
        builder.add_pattern(&pattern);

        index.rules_automaton = builder.build();
        index
            .rules_by_rid
            .push(create_mock_rule("mit", vec![0, 1], true, false));
        index.tids_by_rid.push(tids(&[0, 1]));
        index.pattern_id_to_rid.push(vec![0]);
        index.rule_metadata_by_identifier.insert(
            "mit.LICENSE".to_string(),
            IndexedRuleMetadata {
                license_expression_spdx: Some("MIT".to_string()),
                skip_for_required_phrase_generation: false,
                replaced_by: vec![],
            },
        );

        let query = create_mock_query_with_tokens(&[0, 1], &index);
        let run = query.whole_query_run();
        let matches = aho_match(run.get_index(), &run);

        assert_eq!(matches[0].license_expression_spdx.as_deref(), Some("MIT"));
    }

    #[test]
    fn test_constants() {
        assert_eq!(MATCH_AHO.as_str(), "2-aho");
        assert_eq!(MATCH_AHO.precedence(), 1);
    }

    #[test]
    fn test_aho_match_overlapping_patterns() {
        let mut index = create_test_index_default();

        let pattern1 = tokens_to_bytes(&tids(&[0u16, 1, 2]));
        let pattern2 = tokens_to_bytes(&tids(&[1u16, 2]));

        let mut builder = AutomatonBuilder::new();
        builder.add_pattern(&pattern1);
        builder.add_pattern(&pattern2);
        let automaton = builder.build();

        index.rules_automaton = automaton;
        index
            .rules_by_rid
            .push(create_mock_rule("mit-full", vec![0, 1, 2], true, false));
        index
            .rules_by_rid
            .push(create_mock_rule("mit-partial", vec![1, 2], true, false));
        index.tids_by_rid.push(tids(&[0, 1, 2]));
        index.tids_by_rid.push(tids(&[1, 2]));
        index.pattern_id_to_rid.push(vec![0]);
        index.pattern_id_to_rid.push(vec![1]);

        let query = crate::license_detection::query::Query {
            text: String::new(),
            tokens: tids(&[0, 1, 2]),
            line_by_pos: vec![1, 1, 1],
            unknowns_by_pos: std::collections::HashMap::new(),
            stopwords_by_pos: std::collections::HashMap::new(),
            shorts_and_digits_pos: PositionSet::new(),
            high_matchables: (0..3).collect(),
            low_matchables: PositionSet::new(),
            is_binary: false,
            query_run_ranges: Vec::new(),
            spdx_lines: Vec::new(),
            index: &index,
        };

        let run = query.whole_query_run();
        let matches = aho_match(run.get_index(), &run);

        assert!(!matches.is_empty(), "Should find overlapping matches");
    }

    #[test]
    fn test_aho_match_zero_length_pattern() {
        let mut index = create_test_index_default();

        let pattern = tokens_to_bytes(&tids(&[0u16]));

        let mut builder = AutomatonBuilder::new();
        builder.add_pattern(&pattern);
        let automaton = builder.build();

        index.rules_automaton = automaton;
        index
            .rules_by_rid
            .push(create_mock_rule("single-token", vec![0], false, false));
        index.tids_by_rid.push(tids(&[0]));
        index.pattern_id_to_rid.push(vec![0]);

        let query = crate::license_detection::query::Query {
            text: String::new(),
            tokens: tids(&[0]),
            line_by_pos: vec![1],
            unknowns_by_pos: std::collections::HashMap::new(),
            stopwords_by_pos: std::collections::HashMap::new(),
            shorts_and_digits_pos: PositionSet::new(),
            high_matchables: PositionSet::new(),
            low_matchables: PositionSet::new(),
            is_binary: false,
            query_run_ranges: Vec::new(),
            spdx_lines: Vec::new(),
            index: &index,
        };

        let run = query.whole_query_run();
        let matches = aho_match(run.get_index(), &run);

        assert!(
            matches.is_empty(),
            "Should not match single low-value token"
        );
    }

    #[test]
    fn test_aho_match_long_query() {
        let mut index = create_test_index_default();

        let pattern = tokens_to_bytes(&tids(&[0u16, 1]));

        let mut builder = AutomatonBuilder::new();
        builder.add_pattern(&pattern);
        let automaton = builder.build();

        index.rules_automaton = automaton;
        index
            .rules_by_rid
            .push(create_mock_rule("mit", vec![0, 1], true, false));
        index.tids_by_rid.push(tids(&[0, 1]));
        index.pattern_id_to_rid.push(vec![0]);

        let tokens: Vec<TokenId> = (0u16..1000).map(|i| tid(i % 2)).collect();
        let line_by_pos: Vec<usize> = (0..1000).map(|i| i / 80 + 1).collect();

        let query = crate::license_detection::query::Query {
            text: String::new(),
            tokens,
            line_by_pos,
            unknowns_by_pos: std::collections::HashMap::new(),
            stopwords_by_pos: std::collections::HashMap::new(),
            shorts_and_digits_pos: PositionSet::new(),
            high_matchables: (0..1000).collect(),
            low_matchables: PositionSet::new(),
            is_binary: false,
            query_run_ranges: Vec::new(),
            spdx_lines: Vec::new(),
            index: &index,
        };

        let run = query.whole_query_run();
        let matches = aho_match(run.get_index(), &run);

        assert!(
            matches.len() > 1,
            "Should find multiple matches in long query"
        );
    }

    #[test]
    fn test_aho_match_score_calculation() {
        let mut index = create_test_index_default();

        let rule_tokens = tids(&[0u16, 1, 2, 3, 4]);
        let pattern_bytes = tokens_to_bytes(&rule_tokens);

        let mut builder = AutomatonBuilder::new();
        builder.add_pattern(&pattern_bytes);
        let automaton = builder.build();

        index.rules_automaton = automaton;
        index.rules_by_rid.push(create_mock_rule(
            "apache-2.0",
            vec![0, 1, 2, 3, 4],
            true,
            false,
        ));
        index.tids_by_rid.push(tids(&[0, 1, 2, 3, 4]));
        index.pattern_id_to_rid.push(vec![0]);

        let query = crate::license_detection::query::Query {
            text: String::new(),
            tokens: tids(&[0, 1, 2, 3, 4]),
            line_by_pos: vec![1, 1, 1, 1, 1],
            unknowns_by_pos: std::collections::HashMap::new(),
            stopwords_by_pos: std::collections::HashMap::new(),
            shorts_and_digits_pos: PositionSet::new(),
            high_matchables: (0..5).collect(),
            low_matchables: PositionSet::new(),
            is_binary: false,
            query_run_ranges: Vec::new(),
            spdx_lines: Vec::new(),
            index: &index,
        };

        let run = query.whole_query_run();
        let matches = aho_match(run.get_index(), &run);

        assert_eq!(matches.len(), 1);
        assert!(
            (matches[0].score.value() - 100.0).abs() < 0.001,
            "Full match should have score 100.0"
        );
        assert_eq!(matches[0].matched_length, 5);
        assert_eq!(matches[0].match_coverage, 100.0);
    }

    #[test]
    fn test_aho_match_token_boundary_bug() {
        let mut index = create_test_index_default();

        let pattern_tid: u16 = 12575;
        let pattern_bytes = pattern_tid.to_le_bytes().to_vec();

        let mut builder = AutomatonBuilder::new();
        builder.add_pattern(&pattern_bytes);
        let automaton = builder.build();

        index.rules_automaton = automaton;
        index.rules_by_rid.push(create_mock_rule(
            "cc-by-nc-sa-2.0",
            vec![pattern_tid],
            true,
            false,
        ));
        index.tids_by_rid.push(tids(&[pattern_tid]));
        index.pattern_id_to_rid.push(vec![0]);

        let exit_tid: u16 = 8045;
        let next_tid: u16 = 18993;

        assert_eq!(exit_tid.to_le_bytes(), [109, 31]);
        assert_eq!(next_tid.to_le_bytes(), [49, 74]);
        assert_eq!(pattern_tid.to_le_bytes(), [31, 49]);

        let query = crate::license_detection::query::Query {
            text: String::new(),
            tokens: tids(&[exit_tid, next_tid]),
            line_by_pos: vec![1, 1],
            unknowns_by_pos: std::collections::HashMap::new(),
            stopwords_by_pos: std::collections::HashMap::new(),
            shorts_and_digits_pos: PositionSet::new(),
            high_matchables: (0..2).collect(),
            low_matchables: PositionSet::new(),
            is_binary: false,
            query_run_ranges: Vec::new(),
            spdx_lines: Vec::new(),
            index: &index,
        };

        let run = query.whole_query_run();
        let matches = aho_match(run.get_index(), &run);

        assert!(
            matches.is_empty(),
            "Should NOT match across token boundaries! Pattern [31, 49] (token {}) appears at bytes 1-2 (crossing token boundary), but should not match. Got {} matches",
            pattern_tid,
            matches.len()
        );
    }

    #[test]
    fn test_aho_match_single_token_matches_correctly() {
        let mut index = create_test_index_default();

        let pattern_tid: u16 = 12575;
        let pattern_bytes = pattern_tid.to_le_bytes().to_vec();

        let mut builder = AutomatonBuilder::new();
        builder.add_pattern(&pattern_bytes);
        let automaton = builder.build();

        index.rules_automaton = automaton;
        index.rules_by_rid.push(create_mock_rule(
            "cc-by-nc-sa-2.0",
            vec![pattern_tid],
            true,
            false,
        ));
        index.tids_by_rid.push(tids(&[pattern_tid]));
        index.pattern_id_to_rid.push(vec![0]);

        let query = crate::license_detection::query::Query {
            text: String::new(),
            tokens: tids(&[0, 1, pattern_tid, 2, 3]),
            line_by_pos: vec![1, 1, 1, 1, 1],
            unknowns_by_pos: std::collections::HashMap::new(),
            stopwords_by_pos: std::collections::HashMap::new(),
            shorts_and_digits_pos: PositionSet::new(),
            high_matchables: (0..5).collect(),
            low_matchables: PositionSet::new(),
            is_binary: false,
            query_run_ranges: Vec::new(),
            spdx_lines: Vec::new(),
            index: &index,
        };

        let run = query.whole_query_run();
        let matches = aho_match(run.get_index(), &run);

        assert_eq!(
            matches.len(),
            1,
            "Should match the single token at position 2"
        );
        assert_eq!(matches[0].matched_length, 1);
    }
}
