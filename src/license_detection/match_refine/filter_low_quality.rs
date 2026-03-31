//! Low quality match filtering functions.
//!
//! This module contains functions for filtering matches based on quality criteria
//! like density, coverage, length, and required phrases.

use crate::license_detection::index::LicenseIndex;
use crate::license_detection::models::{LicenseMatch, MatcherKind};
use crate::license_detection::position_set::PositionSet;
use crate::license_detection::query::Query;

/// Filter spurious matches with low density.
///
/// Spurious matches are matches with low density (where the matched tokens
/// are separated by many unmatched tokens). This filter only applies to
/// sequence and unknown matcher types - exact matches are always kept.
///
/// Based on Python: `filter_spurious_matches()` (match.py:1768-1836)
pub(crate) fn filter_spurious_matches(
    matches: &[LicenseMatch],
    query: &Query,
) -> Vec<LicenseMatch> {
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
pub(crate) fn filter_below_rule_minimum_coverage(
    index: &LicenseIndex,
    matches: &[LicenseMatch],
) -> Vec<LicenseMatch> {
    matches
        .iter()
        .filter(|m| {
            if m.matcher != MatcherKind::Seq {
                return true;
            }

            let rid = m.rid;
            if let Some(rule) = index.rules_by_rid.get(rid)
                && let Some(min_cov) = rule.minimum_coverage
            {
                return m.coverage() >= min_cov as f32;
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
pub(crate) fn filter_short_matches_scattered_on_too_many_lines(
    index: &LicenseIndex,
    matches: &[LicenseMatch],
) -> Vec<LicenseMatch> {
    if matches.len() == 1 {
        return matches.to_vec();
    }

    matches
        .iter()
        .filter(|m| {
            let rid = m.rid;
            if let Some(rule) = index.rules_by_rid.get(rid)
                && rule.is_small
            {
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
pub(crate) fn filter_matches_missing_required_phrases(
    index: &LicenseIndex,
    matches: &[LicenseMatch],
    query: &Query,
) -> (Vec<LicenseMatch>, Vec<LicenseMatch>) {
    if matches.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut kept = Vec::new();
    let mut discarded = Vec::new();

    for m in matches {
        let rid = m.rid;

        let rule = match index.rules_by_rid.get(rid) {
            Some(r) => r,
            None => {
                kept.push(m.clone());
                continue;
            }
        };

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

        let ispan = m.ispan.to_vec();
        let ispan_set = m.ispan.to_position_set();
        let qspan = m.qspan.to_vec();

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

                let qkey_span_set: PositionSet = qkey_span.iter().copied().collect();
                let qkey_span_end = qkey_span.last().copied();

                let has_same_stopwords = {
                    let mut ok = true;
                    for (&qpos, &ipos) in qspan.iter().zip(ispan.iter()) {
                        if !qkey_span_set.contains(qpos) || Some(qpos) == qkey_span_end {
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
            .all(|span| ispan_set.contains_range(span.clone()));

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

            let qkey_span_set: PositionSet = qkey_span.iter().copied().collect();
            let qkey_span_end = qkey_span.last().copied();

            let has_same_stopwords = {
                let mut ok = true;
                for (&qpos, &ipos) in qspan.iter().zip(ispan.iter()) {
                    if !qkey_span_set.contains(qpos) || Some(qpos) == qkey_span_end {
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
pub(crate) fn filter_matches_to_spurious_single_token(
    matches: &[LicenseMatch],
    query: &Query,
    unknown_count: usize,
) -> Vec<LicenseMatch> {
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
                    .filter(|p| query.shorts_and_digits_pos.contains(*p))
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
                    .filter(|p| query.shorts_and_digits_pos.contains(*p))
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
/// Removes matches whose rule ID is in the index's false_positive_rids set.
///
/// Based on Python: `filter_false_positive_matches()` (lines 1950-1970)
pub(crate) fn filter_false_positive_matches(
    index: &LicenseIndex,
    matches: &[LicenseMatch],
) -> Vec<LicenseMatch> {
    let mut filtered = Vec::new();

    for m in matches {
        let rid = m.rid;
        if index.false_positive_rids.contains(&rid) {
            continue;
        }

        filtered.push(m.clone());
    }

    filtered
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
pub(crate) fn filter_invalid_matches_to_single_word_gibberish(
    index: &LicenseIndex,
    matches: &[LicenseMatch],
    query: &Query,
) -> Vec<LicenseMatch> {
    if !query.is_binary {
        return matches.to_vec();
    }

    matches
        .iter()
        .filter(|m| {
            let rid = m.rid;
            if let Some(rule) = index.rules_by_rid.get(rid)
                && rule.length_unique == 1
                && (rule.is_license_reference() || rule.is_license_clue())
            {
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

pub(crate) fn filter_too_short_matches(
    index: &LicenseIndex,
    matches: &[LicenseMatch],
) -> Vec<LicenseMatch> {
    matches
        .iter()
        .filter(|m| {
            if m.matcher != MatcherKind::Seq {
                return true;
            }

            let rid = m.rid;
            if let Some(rule) = index.rules_by_rid.get(rid) {
                return !m.is_small(
                    rule.min_matched_length,
                    rule.min_high_matched_length,
                    rule.is_small,
                );
            }

            true
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license_detection::models::Rule;
    use crate::license_detection::models::position_span::PositionSpan;
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
    ) -> LicenseMatch {
        let matched_len = end_line - start_line + 1;
        let rule_len = matched_len;
        let rid = parse_rule_id(rule_identifier).unwrap_or(0);
        LicenseMatch {
            rid,
            license_expression: "mit".to_string(),
            license_expression_spdx: Some("MIT".to_string()),
            from_file: None,
            start_line,
            end_line,
            start_token: start_line,
            end_token: end_line + 1,
            matcher: crate::license_detection::models::MatcherKind::Aho,
            score,
            matched_length: matched_len,
            rule_length: rule_len,
            match_coverage: coverage,
            rule_relevance: relevance,
            rule_identifier: rule_identifier.to_string(),
            rule_url: "https://example.com".to_string(),
            matched_text: None,
            referenced_filenames: None,
            rule_kind: crate::license_detection::models::RuleKind::None,
            is_from_license: false,
            rule_start_token: 0,
            qspan: PositionSpan::empty(),
            ispan: PositionSpan::empty(),
            hispan: PositionSpan::empty(),
            candidate_resemblance: 0.0,
            candidate_containment: 0.0,
        }
    }

    fn create_test_match_with_tokens(
        rule_identifier: &str,
        start_token: usize,
        end_token: usize,
        matched_length: usize,
    ) -> LicenseMatch {
        let rid = parse_rule_id(rule_identifier).unwrap_or(0);
        LicenseMatch {
            rid,
            license_expression: "mit".to_string(),
            license_expression_spdx: Some("MIT".to_string()),
            from_file: None,
            start_line: start_token,
            end_line: end_token.saturating_sub(1),
            start_token,
            end_token,
            matcher: crate::license_detection::models::MatcherKind::Aho,
            score: 1.0,
            matched_length,
            rule_length: matched_length,
            match_coverage: 100.0,
            rule_relevance: 100,
            rule_identifier: rule_identifier.to_string(),
            rule_url: "https://example.com".to_string(),
            matched_text: None,
            referenced_filenames: None,
            rule_kind: crate::license_detection::models::RuleKind::None,
            is_from_license: false,
            rule_start_token: 0,
            qspan: PositionSpan::empty(),
            ispan: PositionSpan::empty(),
            hispan: PositionSpan::empty(),
            candidate_resemblance: 0.0,
            candidate_containment: 0.0,
        }
    }

    #[test]
    fn test_filter_false_positive_matches_with_false_positive() {
        let mut index = LicenseIndex::with_legalese_count(10);
        let _ = index.false_positive_rids.insert(42);

        let matches = vec![
            create_test_match("#42", 1, 10, 0.9, 90.0, 100),
            create_test_match("#1", 15, 25, 0.85, 85.0, 100),
        ];

        let filtered = filter_false_positive_matches(&index, &matches);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].rule_identifier, "#1");
    }

    #[test]
    fn test_filter_false_positive_matches_no_false_positive() {
        let index = LicenseIndex::with_legalese_count(10);

        let matches = vec![
            create_test_match("#1", 1, 10, 0.9, 90.0, 100),
            create_test_match("#2", 15, 25, 0.85, 85.0, 100),
        ];

        let filtered = filter_false_positive_matches(&index, &matches);

        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_false_positive_matches_all_false_positive() {
        let mut index = LicenseIndex::with_legalese_count(10);
        let _ = index.false_positive_rids.insert(42);
        let _ = index.false_positive_rids.insert(43);

        let matches = vec![
            create_test_match("#42", 1, 10, 0.9, 90.0, 100),
            create_test_match("#43", 15, 25, 0.85, 85.0, 100),
        ];

        let filtered = filter_false_positive_matches(&index, &matches);

        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn test_filter_false_positive_matches_empty() {
        let index = LicenseIndex::with_legalese_count(10);
        let matches: Vec<LicenseMatch> = vec![];
        let filtered = filter_false_positive_matches(&index, &matches);
        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn test_filter_spurious_matches_keeps_non_seq_matchers() {
        let index = LicenseIndex::with_legalese_count(10);
        let query = Query::from_extracted_text("test text", &index, false).unwrap();
        let matches = vec![
            LicenseMatch {
                matcher: crate::license_detection::models::MatcherKind::Hash,
                matched_length: 5,
                ..create_test_match("#1", 1, 10, 1.0, 100.0, 100)
            },
            LicenseMatch {
                matcher: crate::license_detection::models::MatcherKind::Aho,
                matched_length: 5,
                ..create_test_match("#2", 1, 10, 1.0, 100.0, 100)
            },
        ];

        let filtered = filter_spurious_matches(&matches, &query);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_spurious_matches_keeps_high_density_seq() {
        let index = LicenseIndex::with_legalese_count(10);
        let query = Query::from_extracted_text("test text", &index, false).unwrap();
        let mut m = create_test_match("#1", 1, 10, 1.0, 100.0, 100);
        m.matcher = crate::license_detection::models::MatcherKind::Seq;
        m.matched_length = 50;
        m.qspan = PositionSpan::from_positions((0..50).collect());
        m.ispan = PositionSpan::from_positions((0..50).collect());

        let matches = vec![m];
        let filtered = filter_spurious_matches(&matches, &query);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_spurious_matches_filters_low_density_short() {
        let index = LicenseIndex::with_legalese_count(10);
        let query = Query::from_extracted_text("test text", &index, false).unwrap();
        let mut m = create_test_match("#1", 1, 10, 1.0, 100.0, 100);
        m.matcher = crate::license_detection::models::MatcherKind::Seq;
        m.matched_length = 5;
        m.start_token = 0;
        m.end_token = 100;
        m.qspan = PositionSpan::from_positions(vec![0, 50, 75, 80, 99]);

        let matches = vec![m];
        let filtered = filter_spurious_matches(&matches, &query);
        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn test_filter_spurious_matches_filters_unknown_matcher() {
        let index = LicenseIndex::with_legalese_count(10);
        let query = Query::from_extracted_text("test text", &index, false).unwrap();
        let mut m = create_test_match("#1", 1, 10, 1.0, 100.0, 100);
        m.matcher = MATCH_UNKNOWN;
        m.matched_length = 5;
        m.start_token = 0;
        m.end_token = 100;
        m.qspan = PositionSpan::from_positions(vec![0, 50, 75, 80, 99]);

        let matches = vec![m];
        let filtered = filter_spurious_matches(&matches, &query);
        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn test_filter_spurious_matches_keeps_medium_length() {
        let index = LicenseIndex::with_legalese_count(10);
        let query = Query::from_extracted_text("test text", &index, false).unwrap();
        let mut m = create_test_match("#1", 1, 10, 1.0, 100.0, 100);
        m.matcher = crate::license_detection::models::MatcherKind::Seq;
        m.matched_length = 25;
        m.start_token = 0;
        m.end_token = 30;
        m.qspan = PositionSpan::from_positions((0..25).collect());
        m.ispan = PositionSpan::from_positions((0..25).collect());
        m.hispan = PositionSpan::from_positions((0..10).collect());

        let matches = vec![m];
        let filtered = filter_spurious_matches(&matches, &query);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_spurious_matches_empty() {
        let index = LicenseIndex::with_legalese_count(10);
        let query = Query::from_extracted_text("test text", &index, false).unwrap();
        let matches: Vec<LicenseMatch> = vec![];
        let filtered = filter_spurious_matches(&matches, &query);
        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn test_filter_false_positive_matches_mixed_identifiers() {
        let mut index = LicenseIndex::with_legalese_count(10);
        let _ = index.false_positive_rids.insert(42);

        let matches = vec![
            create_test_match("#42", 1, 10, 0.9, 90.0, 100),
            create_test_match("mit.LICENSE", 15, 25, 0.85, 85.0, 100),
            create_test_match("#1", 30, 40, 0.8, 80.0, 100),
        ];

        let filtered = filter_false_positive_matches(&index, &matches);

        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().any(|m| m.rule_identifier == "mit.LICENSE"));
        assert!(filtered.iter().any(|m| m.rule_identifier == "#1"));
    }

    #[test]
    fn test_filter_too_short_matches_non_seq_match_kept() {
        let index = LicenseIndex::with_legalese_count(10);

        let mut m = create_test_match("#1", 1, 10, 0.9, 50.0, 100);
        m.matcher = crate::license_detection::models::MatcherKind::Aho;
        m.matched_length = 2;

        let matches = vec![m];
        let filtered = filter_too_short_matches(&index, &matches);

        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_too_short_matches_small_seq_match_filtered() {
        let mut index = LicenseIndex::with_legalese_count(10);
        index.rules_by_rid.push(Rule {
            identifier: "test".to_string(),
            license_expression: "mit".to_string(),
            text: "test".to_string(),
            tokens: vec![],
            rule_kind: crate::license_detection::models::RuleKind::None,
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
            length_unique: 0,
            high_length_unique: 0,
            high_length: 0,
            min_matched_length: 10,
            min_high_matched_length: 5,
            min_matched_length_unique: 0,
            min_high_matched_length_unique: 0,
            is_small: true,
            is_tiny: false,
            starts_with_license: false,
            ends_with_license: false,
            is_deprecated: false,
            spdx_license_key: None,
            other_spdx_license_keys: vec![],
            required_phrase_spans: vec![],
            stopwords_by_pos: std::collections::HashMap::new(),
        });

        let mut m = create_test_match("#0", 1, 10, 0.9, 50.0, 100);
        m.matcher = crate::license_detection::models::MatcherKind::Seq;
        m.matched_length = 5;
        m.hispan = PositionSpan::from_positions((0..2).collect());

        let matches = vec![m];
        let filtered = filter_too_short_matches(&index, &matches);

        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn test_filter_too_short_matches_large_seq_match_kept() {
        let mut index = LicenseIndex::with_legalese_count(10);
        index.rules_by_rid.push(Rule {
            identifier: "test".to_string(),
            license_expression: "mit".to_string(),
            text: "test".to_string(),
            tokens: vec![],
            rule_kind: crate::license_detection::models::RuleKind::None,
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
            length_unique: 0,
            high_length_unique: 0,
            high_length: 0,
            min_matched_length: 10,
            min_high_matched_length: 5,
            min_matched_length_unique: 0,
            min_high_matched_length_unique: 0,
            is_small: true,
            is_tiny: false,
            starts_with_license: false,
            ends_with_license: false,
            is_deprecated: false,
            spdx_license_key: None,
            other_spdx_license_keys: vec![],
            required_phrase_spans: vec![],
            stopwords_by_pos: std::collections::HashMap::new(),
        });

        let mut m = create_test_match("#0", 1, 10, 0.9, 90.0, 100);
        m.matcher = crate::license_detection::models::MatcherKind::Seq;
        m.matched_length = 15;
        m.hispan = PositionSpan::from_positions((0..8).collect());

        let matches = vec![m];
        let filtered = filter_too_short_matches(&index, &matches);

        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_below_rule_minimum_coverage_keeps_non_seq() {
        let index = LicenseIndex::with_legalese_count(10);
        let mut m = create_test_match("#0", 1, 10, 0.9, 50.0, 100);
        m.matcher = crate::license_detection::models::MatcherKind::Aho;

        let matches = vec![m];
        let filtered = filter_below_rule_minimum_coverage(&index, &matches);

        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_below_rule_minimum_coverage_filters_low_coverage() {
        let mut index = LicenseIndex::with_legalese_count(10);
        index.rules_by_rid.push(Rule {
            identifier: "test".to_string(),
            license_expression: "mit".to_string(),
            text: "test".to_string(),
            tokens: vec![],
            rule_kind: crate::license_detection::models::RuleKind::None,
            is_false_positive: false,
            is_required_phrase: false,
            is_from_license: false,
            relevance: 100,
            minimum_coverage: Some(80),
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
            length_unique: 0,
            high_length_unique: 0,
            high_length: 0,
            min_matched_length: 10,
            min_high_matched_length: 5,
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
        });

        let mut m = create_test_match("#0", 1, 10, 0.9, 50.0, 100);
        m.matcher = crate::license_detection::models::MatcherKind::Seq;

        let matches = vec![m];
        let filtered = filter_below_rule_minimum_coverage(&index, &matches);

        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn test_filter_below_rule_minimum_coverage_keeps_rounded_boundary_match() {
        let mut index = LicenseIndex::with_legalese_count(10);
        index.rules_by_rid.push(Rule {
            identifier: "test".to_string(),
            license_expression: "mit".to_string(),
            text: "test".to_string(),
            tokens: vec![],
            rule_kind: crate::license_detection::models::RuleKind::None,
            is_false_positive: false,
            is_required_phrase: false,
            is_from_license: false,
            relevance: 100,
            minimum_coverage: Some(80),
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
            length_unique: 0,
            high_length_unique: 0,
            high_length: 0,
            min_matched_length: 10,
            min_high_matched_length: 5,
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
        });

        let mut m = create_test_match("#0", 1, 10, 0.9, 79.996, 100);
        m.matcher = crate::license_detection::models::MatcherKind::Seq;

        let filtered = filter_below_rule_minimum_coverage(&index, &[m]);

        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_scattered_keeps_concentrated() {
        let mut index = LicenseIndex::with_legalese_count(10);
        index.rules_by_rid.push(Rule {
            identifier: "test".to_string(),
            license_expression: "mit".to_string(),
            text: "test".to_string(),
            tokens: vec![],
            rule_kind: crate::license_detection::models::RuleKind::None,
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
            length_unique: 0,
            high_length_unique: 0,
            high_length: 0,
            min_matched_length: 10,
            min_high_matched_length: 5,
            min_matched_length_unique: 0,
            min_high_matched_length_unique: 0,
            is_small: true,
            is_tiny: false,
            starts_with_license: false,
            ends_with_license: false,
            is_deprecated: false,
            spdx_license_key: None,
            other_spdx_license_keys: vec![],
            required_phrase_spans: vec![],
            stopwords_by_pos: std::collections::HashMap::new(),
        });

        let m1 = create_test_match_with_tokens("#0", 0, 10, 10);
        let m2 = create_test_match_with_tokens("#0", 20, 30, 10);

        let matches = vec![m1, m2];
        let filtered = filter_short_matches_scattered_on_too_many_lines(&index, &matches);

        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_scattered_filters_scattered() {
        let mut index = LicenseIndex::with_legalese_count(10);
        index.rules_by_rid.push(Rule {
            identifier: "test".to_string(),
            license_expression: "mit".to_string(),
            text: "test".to_string(),
            tokens: vec![],
            rule_kind: crate::license_detection::models::RuleKind::None,
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
            length_unique: 0,
            high_length_unique: 0,
            high_length: 0,
            min_matched_length: 10,
            min_high_matched_length: 5,
            min_matched_length_unique: 0,
            min_high_matched_length_unique: 0,
            is_small: true,
            is_tiny: false,
            starts_with_license: false,
            ends_with_license: false,
            is_deprecated: false,
            spdx_license_key: None,
            other_spdx_license_keys: vec![],
            required_phrase_spans: vec![],
            stopwords_by_pos: std::collections::HashMap::new(),
        });

        let mut m = create_test_match_with_tokens("#0", 0, 3, 3);
        m.start_line = 1;
        m.end_line = 50;

        let mut m2 = create_test_match_with_tokens("#0", 10, 13, 3);
        m2.start_line = 1;
        m2.end_line = 50;

        let matches = vec![m, m2];
        let filtered = filter_short_matches_scattered_on_too_many_lines(&index, &matches);

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
