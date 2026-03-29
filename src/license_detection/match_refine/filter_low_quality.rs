//! Low quality match filtering functions.
//!
//! This module contains functions for filtering matches based on quality criteria
//! like density, coverage, length, and required phrases.

use std::collections::HashSet;

use crate::license_detection::models::{LicenseMatch, MatcherKind};
use crate::license_detection::query::Query;

/// Filter spurious matches with low density.
///
/// Spurious matches are matches with low density (where the matched tokens
/// are separated by many unmatched tokens). This filter only applies to
/// sequence and unknown matcher types - exact matches are always kept.
///
/// Based on Python: `filter_spurious_matches()` (match.py:1768-1836)
pub(crate) fn filter_spurious_matches<'a>(
    matches: &[LicenseMatch<'a>],
    query: &Query<'a>,
) -> Vec<LicenseMatch<'a>> {
    matches
        .iter()
        .filter(|m| {
            let is_seq_or_unknown =
                m.matcher == MatcherKind::Seq || m.matcher == MatcherKind::Unknown;
            if !is_seq_or_unknown {
                return true;
            }

            let qdens = m.qdensity(query);
            let idens = m.idensity();
            let mlen = m.matched_length;
            let hilen = m.hilen();

            if mlen < 10 && (qdens < 0.1 || idens < 0.1) {
                return false;
            }
            if mlen < 15 && (qdens < 0.2 || idens < 0.2) {
                return false;
            }
            if mlen < 20 && hilen < 5 && (qdens < 0.3 || idens < 0.3) {
                return false;
            }
            if mlen < 30 && hilen < 8 && (qdens < 0.4 || idens < 0.4) {
                return false;
            }
            if qdens < 0.4 || idens < 0.4 {
                return false;
            }

            true
        })
        .cloned()
        .collect()
}

/// Filter matches below rule's minimum_coverage threshold.
///
/// Rules can have a `minimum_coverage` attribute that specifies the minimum
/// match coverage required for the match to be valid. Matches with coverage
/// below this threshold should be discarded.
///
/// This filter only applies to sequence matches (matcher == "3-seq").
/// Exact matches (hash, aho, spdx) are always kept.
///
/// Based on Python: `filter_below_rule_minimum_coverage()` (lines 1551-1587)
pub(crate) fn filter_below_rule_minimum_coverage<'a>(
    matches: &[LicenseMatch<'a>],
) -> Vec<LicenseMatch<'a>> {
    matches
        .iter()
        .filter(|m| {
            if m.matcher != MatcherKind::Seq {
                return true;
            }

            if let Some(min_cov) = m.rule.minimum_coverage {
                return m.match_coverage >= min_cov as f32;
            }

            true
        })
        .cloned()
        .collect()
}

/// Filter short matches scattered on too many lines.
///
/// Short matches that are scattered across more lines than their token count
/// are likely spurious and should be filtered. For example, a 3-token match
/// spanning 50 lines is probably not a valid license reference.
///
/// This filter only applies to small rules (rule.is_small == true).
/// License tag rules get a +2 tolerance on matched_len comparison.
///
/// Based on Python: `filter_short_matches_scattered_on_too_many_lines()` (lines 1931-1972)
pub(crate) fn filter_short_matches_scattered_on_too_many_lines<'a>(
    matches: &[LicenseMatch<'a>],
) -> Vec<LicenseMatch<'a>> {
    if matches.len() == 1 {
        return matches.to_vec();
    }

    matches
        .iter()
        .filter(|m| {
            let rule = m.rule;
            if rule.is_small {
                let matched_len = m.len();
                let line_span = m.end_line.saturating_sub(m.start_line) + 1;

                let effective_matched_len = if rule.is_license_tag() {
                    matched_len + 2
                } else {
                    matched_len
                };

                if line_span > effective_matched_len {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect()
}

/// Filter matches that are missing required phrases.
///
/// A match to a rule with required phrases ({{...}} markers) must contain
/// all those required phrases in the matched region. If any required phrase
/// is missing or interrupted by unknown/stopwords, the match is discarded.
///
/// This also handles:
/// - `is_continuous` rules: the entire match must be continuous
/// - `is_required_phrase` rules: same as is_continuous
///
/// Based on Python: `filter_matches_missing_required_phrases()` (match.py:2154-2328)
pub(crate) fn filter_matches_missing_required_phrases<'a>(
    matches: &[LicenseMatch<'a>],
    query: &Query<'a>,
) -> (Vec<LicenseMatch<'a>>, Vec<LicenseMatch<'a>>) {
    if matches.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut kept = Vec::new();
    let mut discarded = Vec::new();

    for m in matches {
        let rule = m.rule;

        let is_continuous = rule.is_continuous || rule.is_required_phrase;
        let ikey_spans = &rule.required_phrase_spans;

        if ikey_spans.is_empty() && !is_continuous {
            kept.push(m.clone());
            continue;
        }

        if is_continuous && !m.is_continuous(query) {
            discarded.push(m.clone());
            continue;
        }

        let ispan = m.ispan();
        let ispan_set: HashSet<usize> = ispan.iter().copied().collect();
        let qspan = m.qspan();

        if is_continuous {
            if !ispan.is_empty() {
                let qkey_span: Vec<usize> = qspan.clone();

                if let Some(_qkey_end) = qkey_span.last() {
                    let contains_unknown = qkey_span
                        .iter()
                        .take(qkey_span.len() - 1)
                        .any(|&qpos| query.unknowns_by_pos.contains_key(&Some(qpos as i32)));

                    if contains_unknown {
                        discarded.push(m.clone());
                        continue;
                    }
                }

                let qkey_span_set: HashSet<usize> = qkey_span.iter().copied().collect();
                let qkey_span_end = qkey_span.last().copied();

                let has_same_stopwords = {
                    let mut ok = true;
                    for (&qpos, &ipos) in qspan.iter().zip(ispan.iter()) {
                        if !qkey_span_set.contains(&qpos) || Some(qpos) == qkey_span_end {
                            continue;
                        }

                        let i_stop = rule.stopwords_by_pos.get(&ipos).copied().unwrap_or(0);
                        let q_stop = query
                            .stopwords_by_pos
                            .get(&Some(qpos as i32))
                            .copied()
                            .unwrap_or(0);

                        if i_stop != q_stop {
                            ok = false;
                            break;
                        }
                    }
                    ok
                };

                if !has_same_stopwords {
                    discarded.push(m.clone());
                    continue;
                }
            }
            kept.push(m.clone());
            continue;
        }

        let all_contained = ikey_spans
            .iter()
            .all(|span| (span.start..span.end).all(|pos| ispan_set.contains(&pos)));

        if !all_contained {
            discarded.push(m.clone());
            continue;
        }

        let mut is_valid = true;

        for ikey_span in ikey_spans {
            let qkey_span: Vec<usize> = qspan
                .iter()
                .zip(ispan.iter())
                .filter_map(|(&qpos, &ipos)| {
                    if ikey_span.contains(&ipos) {
                        Some(qpos)
                    } else {
                        None
                    }
                })
                .collect();

            if qkey_span.len() > 1 {
                for i in 1..qkey_span.len() {
                    if qkey_span[i] != qkey_span[i - 1] + 1 {
                        is_valid = false;
                        break;
                    }
                }
                if !is_valid {
                    break;
                }
            }

            if let Some(_qkey_end) = qkey_span.last() {
                let contains_unknown = qkey_span
                    .iter()
                    .take(qkey_span.len() - 1)
                    .any(|&qpos| query.unknowns_by_pos.contains_key(&Some(qpos as i32)));

                if contains_unknown {
                    is_valid = false;
                    break;
                }
            }

            let qkey_span_set: HashSet<usize> = qkey_span.iter().copied().collect();
            let qkey_span_end = qkey_span.last().copied();

            let has_same_stopwords = {
                let mut ok = true;
                for (&qpos, &ipos) in qspan.iter().zip(ispan.iter()) {
                    if !qkey_span_set.contains(&qpos) || Some(qpos) == qkey_span_end {
                        continue;
                    }

                    let i_stop = rule.stopwords_by_pos.get(&ipos).copied().unwrap_or(0);
                    let q_stop = query
                        .stopwords_by_pos
                        .get(&Some(qpos as i32))
                        .copied()
                        .unwrap_or(0);

                    if i_stop != q_stop {
                        ok = false;
                        break;
                    }
                }
                ok
            };

            if !has_same_stopwords {
                is_valid = false;
                break;
            }
        }

        if is_valid {
            kept.push(m.clone());
        } else {
            discarded.push(m.clone());
        }
    }

    (kept, discarded)
}

/// Filter single-token matches surrounded by many unknown/short/digit tokens.
///
/// A "spurious" single token match is a match to a single token that is
/// surrounded on both sides by at least `unknown_count` tokens that are either
/// unknown tokens, short tokens composed of a single character, tokens
/// composed only of digits or several punctuations and stopwords.
///
/// This filter only applies to sequence matches (matcher == "3-seq") with
/// exactly 1 matched token.
///
/// Based on Python: `filter_matches_to_spurious_single_token()` (lines 1622-1700)
pub(crate) fn filter_matches_to_spurious_single_token<'a>(
    matches: &[LicenseMatch<'a>],
    query: &Query<'a>,
    unknown_count: usize,
) -> Vec<LicenseMatch<'a>> {
    matches
        .iter()
        .filter(|m| {
            if m.matcher != MatcherKind::Seq {
                return true;
            }
            if m.len() != 1 {
                return true;
            }

            let qstart = m.start_token;

            let before = query
                .unknowns_by_pos
                .get(&Some(qstart as i32 - 1))
                .copied()
                .unwrap_or(0)
                + (qstart.saturating_sub(unknown_count)..qstart)
                    .filter(|p| query.shorts_and_digits_pos.contains(p))
                    .count();

            if before < unknown_count {
                return true;
            }

            let after = query
                .unknowns_by_pos
                .get(&Some(qstart as i32))
                .copied()
                .unwrap_or(0)
                + (qstart + 1..qstart + 1 + unknown_count)
                    .filter(|p| query.shorts_and_digits_pos.contains(p))
                    .count();

            if after >= unknown_count {
                return false;
            }

            true
        })
        .cloned()
        .collect()
}

/// Filter matches to false positive rules.
///
/// Removes matches whose rule is marked as a false positive.
///
/// Based on Python: `filter_false_positive_matches()` (lines 1950-1970)
pub(crate) fn filter_false_positive_matches<'a>(
    matches: &[LicenseMatch<'a>],
) -> Vec<LicenseMatch<'a>> {
    matches
        .iter()
        .filter(|m| !m.is_false_positive())
        .cloned()
        .collect()
}

/// Check if a matched text is a valid short match.
///
/// A short match is valid if:
/// - The matched text equals the rule text (exact match)
/// - The matched text equals rule text when normalized (whitespace)
/// - For rules >= 5 chars, all matches are considered valid
/// - Length difference equals max_diff (allowed extra chars)
/// - Matched text is title case or same case throughout
/// - Rule text is contained in matched text
fn is_valid_short_match(matched_text: &str, rule_text: &str, max_diff: usize) -> bool {
    let matched = matched_text.trim();
    let rule = rule_text.trim();

    if matched == rule {
        return true;
    }

    let normalized_matched: String = matched.split_whitespace().collect::<Vec<_>>().join(" ");
    let normalized_rule: String = rule.split_whitespace().collect::<Vec<_>>().join(" ");

    if normalized_matched == normalized_rule {
        return true;
    }

    if normalized_rule.len() >= 5 {
        return true;
    }

    let diff_len = normalized_matched.len().abs_diff(normalized_rule.len());
    if diff_len > 0 && diff_len != max_diff {
        return false;
    }

    let (matched_check, rule_check) = if rule.ends_with('+') {
        (matched.trim_end_matches('+'), rule.trim_end_matches('+'))
    } else {
        (matched, rule)
    };

    let is_title_case = matched_check
        .chars()
        .next()
        .map(|c| c.is_ascii_uppercase())
        .unwrap_or(false)
        && matched_check
            .chars()
            .skip(1)
            .all(|c| !c.is_ascii_uppercase());

    if is_title_case {
        return true;
    }

    let is_same_case = matched_check.to_lowercase() == matched_check
        || matched_check.to_uppercase() == matched_check;

    if is_same_case {
        return true;
    }

    if matched_check.contains(rule_check) {
        return true;
    }

    false
}

/// Filter invalid matches to single-word gibberish in binary files.
///
/// Filters gibberish matches considered as invalid under these conditions:
/// - The scanned file is a binary file
/// - The matched rule has a single word (length_unique == 1)
/// - The matched rule "is_license_reference" or "is_license_clue"
/// - The matched rule has a low relevance (< 80)
/// - The matched text has leading/trailing punctuation or mixed case issues
///
/// Based on Python: `filter_invalid_matches_to_single_word_gibberish()` (lines 1839-1901)
pub(crate) fn filter_invalid_matches_to_single_word_gibberish<'a>(
    matches: &[LicenseMatch<'a>],
    query: &Query<'a>,
) -> Vec<LicenseMatch<'a>> {
    if !query.is_binary {
        return matches.to_vec();
    }

    matches
        .iter()
        .filter(|m| {
            let rule = m.rule;
            if rule.length_unique == 1 && (rule.is_license_reference() || rule.is_license_clue()) {
                let matched_text = match &m.matched_text {
                    Some(text) => text.clone(),
                    None => query.matched_text(m.start_line, m.end_line),
                };
                let max_diff = if rule.relevance >= 80 { 1 } else { 0 };

                if !is_valid_short_match(&matched_text, &rule.text, max_diff) {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect()
}

pub(crate) fn filter_too_short_matches<'a>(matches: &[LicenseMatch<'a>]) -> Vec<LicenseMatch<'a>> {
    matches
        .iter()
        .filter(|m| {
            if m.matcher != MatcherKind::Seq {
                return true;
            }

            let rule = m.rule;
            !m.is_small(
                rule.min_matched_length,
                rule.min_high_matched_length,
                rule.is_small,
            )
        })
        .cloned()
        .collect()
}

#[cfg(test)]
#[allow(dead_code, unused_variables)]
mod tests {
    use super::*;
    use crate::license_detection::index::LicenseIndex;
    use crate::license_detection::tests::TestMatchBuilder;
    use crate::license_detection::unknown_match::MATCH_UNKNOWN;

    fn parse_rule_id(rule_identifier: &str) -> Option<usize> {
        let trimmed = rule_identifier.trim();
        if let Some(stripped) = trimmed.strip_prefix('#') {
            stripped.parse().ok()
        } else {
            trimmed.parse().ok()
        }
    }

    fn create_test_match(
        rule_identifier: &str,
        start_line: usize,
        end_line: usize,
        score: f32,
        coverage: f32,
        relevance: u8,
    ) -> LicenseMatch<'static> {
        let matched_len = end_line - start_line + 1;
        let rule_len = matched_len;
        TestMatchBuilder::default()
            .license_expression("mit")
            .license_expression_spdx(Some("MIT".to_string()))
            .start_line(start_line)
            .end_line(end_line)
            .start_token(start_line)
            .end_token(end_line + 1)
            .matcher(MatcherKind::Aho)
            .score(score)
            .matched_length(matched_len)
            .rule_length(rule_len)
            .match_coverage(coverage)
            .rule_relevance(relevance)
            .rule_identifier(rule_identifier)
            .rule_url("https://example.com".to_string())
            .hilen(50)
            .build_match()
    }

    fn create_test_match_with_tokens(
        rule_identifier: &str,
        start_token: usize,
        end_token: usize,
        matched_length: usize,
    ) -> LicenseMatch<'static> {
        TestMatchBuilder::default()
            .license_expression("mit")
            .license_expression_spdx(Some("MIT".to_string()))
            .start_line(start_token)
            .end_line(end_token.saturating_sub(1))
            .start_token(start_token)
            .end_token(end_token)
            .matcher(MatcherKind::Aho)
            .score(1.0)
            .matched_length(matched_length)
            .rule_length(matched_length)
            .match_coverage(100.0)
            .rule_relevance(100)
            .rule_identifier(rule_identifier)
            .rule_url("https://example.com".to_string())
            .hilen(matched_length / 2)
            .build_match()
    }

    #[test]
    fn test_filter_false_positive_matches_with_false_positive() {
        let m1 = TestMatchBuilder::default()
            .license_expression("mit")
            .rule_identifier("#42")
            .start_line(1)
            .end_line(10)
            .score(0.9)
            .match_coverage(90.0)
            .rule_relevance(100)
            .is_false_positive(true)
            .build_match();
        let m2 = TestMatchBuilder::default()
            .license_expression("mit")
            .rule_identifier("#1")
            .start_line(15)
            .end_line(25)
            .score(0.85)
            .match_coverage(85.0)
            .rule_relevance(100)
            .build_match();
        let matches = vec![m1, m2];

        let filtered = filter_false_positive_matches(&matches);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].rule_identifier(), "#1");
    }

    #[test]
    fn test_filter_false_positive_matches_no_false_positive() {
        let matches = vec![
            create_test_match("#1", 1, 10, 0.9, 90.0, 100),
            create_test_match("#2", 15, 25, 0.85, 85.0, 100),
        ];

        let filtered = filter_false_positive_matches(&matches);

        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_false_positive_matches_all_false_positive() {
        let matches = vec![
            create_test_match("#42", 1, 10, 0.9, 90.0, 100),
            create_test_match("#43", 15, 25, 0.85, 85.0, 100),
        ];
        let matches: Vec<LicenseMatch<'static>> = matches
            .into_iter()
            .map(|m| {
                TestMatchBuilder::default()
                    .license_expression("mit")
                    .rule_identifier(m.rule_identifier())
                    .start_line(m.start_line)
                    .end_line(m.end_line)
                    .score(m.score)
                    .match_coverage(m.match_coverage)
                    .rule_relevance(m.rule_relevance())
                    .is_false_positive(true)
                    .build_match()
            })
            .collect();

        let filtered = filter_false_positive_matches(&matches);

        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn test_filter_false_positive_matches_empty() {
        let matches: Vec<LicenseMatch<'static>> = vec![];
        let filtered = filter_false_positive_matches(&matches);
        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn test_filter_spurious_matches_keeps_non_seq_matchers() {
        let index = LicenseIndex::with_legalese_count(10);
        let query = Query::from_extracted_text("test text", &index, false).unwrap();
        let m1 = TestMatchBuilder::default()
            .license_expression("mit")
            .matcher(crate::license_detection::models::MatcherKind::Hash)
            .matched_length(5)
            .start_line(1)
            .end_line(10)
            .rule_identifier("#1")
            .build_match();
        let m2 = TestMatchBuilder::default()
            .license_expression("mit")
            .matcher(crate::license_detection::models::MatcherKind::Aho)
            .matched_length(5)
            .start_line(1)
            .end_line(10)
            .rule_identifier("#2")
            .build_match();
        let matches = vec![m1, m2];

        let filtered = filter_spurious_matches(&matches, &query);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_spurious_matches_keeps_high_density_seq() {
        let index = LicenseIndex::with_legalese_count(10);
        let query = Query::from_extracted_text("test text", &index, false).unwrap();
        let m = TestMatchBuilder::default()
            .license_expression("mit")
            .matcher(crate::license_detection::models::MatcherKind::Seq)
            .matched_length(50)
            .matched_token_positions(Some((0..50).collect()))
            .start_line(1)
            .end_line(10)
            .rule_identifier("#1")
            .build_match();

        let matches = vec![m];
        let filtered = filter_spurious_matches(&matches, &query);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_spurious_matches_filters_low_density_short() {
        let index = LicenseIndex::with_legalese_count(10);
        let query = Query::from_extracted_text("test text", &index, false).unwrap();
        let m = TestMatchBuilder::default()
            .license_expression("mit")
            .matcher(crate::license_detection::models::MatcherKind::Seq)
            .matched_length(5)
            .start_token(0)
            .end_token(100)
            .matched_token_positions(Some(vec![0, 50, 75, 80, 99]))
            .start_line(1)
            .end_line(10)
            .rule_identifier("#1")
            .build_match();

        let matches = vec![m];
        let filtered = filter_spurious_matches(&matches, &query);
        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn test_filter_spurious_matches_filters_unknown_matcher() {
        let index = LicenseIndex::with_legalese_count(10);
        let query = Query::from_extracted_text("test text", &index, false).unwrap();
        let m = TestMatchBuilder::default()
            .license_expression("mit")
            .matcher(MATCH_UNKNOWN)
            .matched_length(5)
            .start_token(0)
            .end_token(100)
            .matched_token_positions(Some(vec![0, 50, 75, 80, 99]))
            .start_line(1)
            .end_line(10)
            .rule_identifier("#1")
            .build_match();

        let matches = vec![m];
        let filtered = filter_spurious_matches(&matches, &query);
        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn test_filter_spurious_matches_keeps_medium_length() {
        let index = LicenseIndex::with_legalese_count(10);
        let query = Query::from_extracted_text("test text", &index, false).unwrap();
        let m = TestMatchBuilder::default()
            .license_expression("mit")
            .matcher(crate::license_detection::models::MatcherKind::Seq)
            .matched_length(25)
            .start_token(0)
            .end_token(30)
            .matched_token_positions(Some((0..25).collect()))
            .hilen(10)
            .start_line(1)
            .end_line(10)
            .rule_identifier("#1")
            .build_match();

        let matches = vec![m];
        let filtered = filter_spurious_matches(&matches, &query);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_spurious_matches_empty() {
        let index = LicenseIndex::with_legalese_count(10);
        let query = Query::from_extracted_text("test text", &index, false).unwrap();
        let matches: Vec<LicenseMatch<'static>> = vec![];
        let filtered = filter_spurious_matches(&matches, &query);
        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn test_filter_false_positive_matches_mixed_identifiers() {
        let m1 = TestMatchBuilder::default()
            .license_expression("mit")
            .rule_identifier("#42")
            .start_line(1)
            .end_line(10)
            .score(0.9)
            .match_coverage(90.0)
            .rule_relevance(100)
            .is_false_positive(true)
            .build_match();
        let m2 = TestMatchBuilder::default()
            .license_expression("mit")
            .rule_identifier("mit.LICENSE")
            .start_line(15)
            .end_line(25)
            .score(0.85)
            .match_coverage(85.0)
            .rule_relevance(100)
            .build_match();
        let m3 = TestMatchBuilder::default()
            .license_expression("mit")
            .rule_identifier("#1")
            .start_line(30)
            .end_line(40)
            .score(0.8)
            .match_coverage(80.0)
            .rule_relevance(100)
            .build_match();
        let matches = vec![m1, m2, m3];

        let filtered = filter_false_positive_matches(&matches);

        assert_eq!(filtered.len(), 2);
        assert!(
            filtered
                .iter()
                .any(|m| m.rule_identifier() == "mit.LICENSE")
        );
        assert!(filtered.iter().any(|m| m.rule_identifier() == "#1"));
    }

    #[test]
    fn test_filter_too_short_matches_non_seq_match_kept() {
        let m = TestMatchBuilder::default()
            .license_expression("mit")
            .matcher(crate::license_detection::models::MatcherKind::Aho)
            .matched_length(2)
            .start_line(1)
            .end_line(10)
            .rule_identifier("#1")
            .build_match();

        let matches = vec![m];
        let filtered = filter_too_short_matches(&matches);

        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_too_short_matches_small_seq_match_filtered() {
        let m = TestMatchBuilder::default()
            .license_expression("mit")
            .matcher(crate::license_detection::models::MatcherKind::Seq)
            .matched_length(5)
            .hilen(2)
            .start_line(1)
            .end_line(10)
            .rule_identifier("#0")
            .is_small(true)
            .min_matched_length(10)
            .min_high_matched_length(5)
            .build_match();

        let matches = vec![m];
        let filtered = filter_too_short_matches(&matches);

        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn test_filter_too_short_matches_large_seq_match_kept() {
        let m = TestMatchBuilder::default()
            .license_expression("mit")
            .matcher(crate::license_detection::models::MatcherKind::Seq)
            .matched_length(15)
            .hilen(8)
            .start_line(1)
            .end_line(10)
            .rule_identifier("#0")
            .is_small(true)
            .min_matched_length(10)
            .min_high_matched_length(5)
            .build_match();

        let matches = vec![m];
        let filtered = filter_too_short_matches(&matches);

        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_below_rule_minimum_coverage_keeps_non_seq() {
        let m = TestMatchBuilder::default()
            .license_expression("mit")
            .matcher(crate::license_detection::models::MatcherKind::Aho)
            .start_line(1)
            .end_line(10)
            .rule_identifier("#0")
            .build_match();

        let matches = vec![m];
        let filtered = filter_below_rule_minimum_coverage(&matches);

        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_below_rule_minimum_coverage_filters_low_coverage() {
        let m = TestMatchBuilder::default()
            .license_expression("mit")
            .matcher(crate::license_detection::models::MatcherKind::Seq)
            .match_coverage(50.0)
            .start_line(1)
            .end_line(10)
            .rule_identifier("#0")
            .minimum_coverage(Some(80))
            .build_match();

        let matches = vec![m];
        let filtered = filter_below_rule_minimum_coverage(&matches);

        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn test_filter_scattered_keeps_concentrated() {
        let m1 = TestMatchBuilder::default()
            .license_expression("mit")
            .start_token(0)
            .end_token(10)
            .matched_length(10)
            .start_line(1)
            .end_line(10)
            .rule_identifier("#0")
            .is_small(true)
            .build_match();
        let m2 = TestMatchBuilder::default()
            .license_expression("mit")
            .start_token(20)
            .end_token(30)
            .matched_length(10)
            .start_line(11)
            .end_line(20)
            .rule_identifier("#0")
            .is_small(true)
            .build_match();

        let matches = vec![m1, m2];
        let filtered = filter_short_matches_scattered_on_too_many_lines(&matches);

        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_scattered_filters_scattered() {
        let m = TestMatchBuilder::default()
            .license_expression("mit")
            .start_token(0)
            .end_token(3)
            .matched_length(3)
            .start_line(1)
            .end_line(50)
            .rule_identifier("#0")
            .is_small(true)
            .build_match();

        let m2 = TestMatchBuilder::default()
            .license_expression("mit")
            .start_token(10)
            .end_token(13)
            .matched_length(3)
            .start_line(1)
            .end_line(50)
            .rule_identifier("#0")
            .is_small(true)
            .build_match();

        let matches = vec![m, m2];
        let filtered = filter_short_matches_scattered_on_too_many_lines(&matches);

        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn test_is_valid_short_match_exact() {
        assert!(is_valid_short_match("GPL", "GPL", 0));
        assert!(is_valid_short_match("gpl", "GPL", 0));
        assert!(is_valid_short_match("MIT", "MIT", 0));
    }

    #[test]
    fn test_is_valid_short_match_with_diff() {
        assert!(is_valid_short_match("gpl~", "GPL", 1));
        assert!(!is_valid_short_match("gpl~", "GPL", 0));
    }

    #[test]
    fn test_is_valid_short_match_rejects_punctuation() {
        assert!(!is_valid_short_match("~gpl", "GPL", 0));
        assert!(!is_valid_short_match("gpl)", "GPL", 0));
        assert!(is_valid_short_match("gpl+", "gpl+", 0));
    }

    #[test]
    fn test_is_valid_short_match_rejects_mixed_case() {
        assert!(!is_valid_short_match("gPl", "GPL", 0));
        assert!(is_valid_short_match("Gpl", "GPL", 0));
    }
}
