//! Match merging functions.
//!
//! This module contains functions for merging overlapping and adjacent matches,
//! updating match scores, and filtering license references.

use std::collections::HashSet;

use crate::license_detection::models::LicenseMatch;
use crate::license_detection::query::Query;

const MAX_DIST: usize = 50;

fn combine_matches<'a>(a: &LicenseMatch<'a>, b: &LicenseMatch<'a>) -> LicenseMatch<'a> {
    assert_eq!(
        a.rule_identifier(),
        b.rule_identifier(),
        "Cannot combine matches with different rules: {} vs {}",
        a.rule_identifier(),
        b.rule_identifier()
    );

    let mut merged = a.clone();

    let mut qspan: HashSet<usize> = HashSet::new();
    qspan.extend(a.qspan_iter());
    qspan.extend(b.qspan_iter());
    let mut qspan_vec: Vec<usize> = qspan.into_iter().collect();
    qspan_vec.sort();

    let mut ispan: HashSet<usize> = HashSet::new();
    ispan.extend(a.ispan_iter());
    ispan.extend(b.ispan_iter());
    let mut ispan_vec: Vec<usize> = ispan.into_iter().collect();
    ispan_vec.sort();

    let a_hispan: Vec<usize> = a.hispan();
    let b_hispan: Vec<usize> = b.hispan();
    let mut a_hispan_set: HashSet<usize> = HashSet::with_capacity(a_hispan.len());
    a_hispan_set.extend(a_hispan.iter().copied());
    let mut b_hispan_set: HashSet<usize> = HashSet::with_capacity(b_hispan.len());
    b_hispan_set.extend(b_hispan.iter().copied());
    let combined_hispan: HashSet<usize> = a_hispan_set.union(&b_hispan_set).copied().collect();
    let mut hispan_vec: Vec<usize> = combined_hispan.into_iter().collect();
    hispan_vec.sort();
    let hilen = hispan_vec.len();

    merged.start_token = *qspan_vec.first().unwrap_or(&a.start_token);
    merged.end_token = qspan_vec.last().map(|&x| x + 1).unwrap_or(a.end_token);
    merged.rule_start_token = *ispan_vec.first().unwrap_or(&a.rule_start_token);
    merged.matched_length = qspan_vec.len();
    merged.hilen = hilen;
    merged.hispan_positions = if hispan_vec.is_empty() {
        None
    } else {
        Some(hispan_vec)
    };
    merged.start_line = a.start_line.min(b.start_line);
    merged.end_line = a.end_line.max(b.end_line);
    merged.score = a.score.max(b.score);
    merged.qspan_positions = Some(qspan_vec);
    merged.ispan_positions = Some(ispan_vec);

    let rule_length = merged.rule_length();
    if rule_length > 0 {
        merged.match_coverage =
            (merged.matched_length.min(rule_length) as f32 / rule_length as f32) * 100.0;
    }

    merged
}

/// Merge overlapping and adjacent matches for the same rule.
///
/// Based on Python: `merge_matches()` (match.py:869-1068)
/// Uses distance-based merging with multiple merge conditions.
pub fn merge_overlapping_matches<'a>(matches: &[LicenseMatch<'a>]) -> Vec<LicenseMatch<'a>> {
    if matches.is_empty() {
        return Vec::new();
    }

    if matches.len() == 1 {
        return matches.to_vec();
    }

    let mut sorted: Vec<&LicenseMatch<'a>> = matches.iter().collect();
    sorted.sort_by(|a, b| {
        a.rule_identifier()
            .cmp(b.rule_identifier())
            .then_with(|| a.qstart().cmp(&b.qstart()))
            .then_with(|| b.hilen.cmp(&a.hilen))
            .then_with(|| b.matched_length.cmp(&a.matched_length))
            .then_with(|| a.matcher_order().cmp(&b.matcher_order()))
    });

    let mut grouped: Vec<Vec<&LicenseMatch<'a>>> = Vec::new();
    let mut current_group: Vec<&LicenseMatch<'a>> = Vec::new();

    for m in sorted {
        if current_group.is_empty() || current_group[0].rule_identifier() == m.rule_identifier() {
            current_group.push(m);
        } else {
            grouped.push(current_group);
            current_group = vec![m];
        }
    }
    if !current_group.is_empty() {
        grouped.push(current_group);
    }

    let mut merged = Vec::new();

    for rule_matches in grouped {
        if rule_matches.len() == 1 {
            merged.push(rule_matches[0].clone());
            continue;
        }

        let rule_length = rule_matches[0].rule_length();
        let max_rule_side_dist = (rule_length / 2).clamp(1, MAX_DIST);

        let mut rule_matches: Vec<LicenseMatch<'a>> =
            rule_matches.iter().map(|m| (*m).clone()).collect();
        let mut i = 0;

        let mut current_qspan_set: HashSet<usize> = HashSet::with_capacity(64);
        let mut next_qspan_set: HashSet<usize> = HashSet::with_capacity(64);
        let mut current_ispan_set: HashSet<usize> = HashSet::with_capacity(64);
        let mut next_ispan_set: HashSet<usize> = HashSet::with_capacity(64);

        while i < rule_matches.len().saturating_sub(1) {
            let mut j = i + 1;

            while j < rule_matches.len() {
                let current = &rule_matches[i];
                let next = &rule_matches[j];

                if current.qdistance_to(next) > max_rule_side_dist
                    || current.idistance_to(next) > max_rule_side_dist
                {
                    break;
                }

                current_qspan_set.clear();
                next_qspan_set.clear();
                current_ispan_set.clear();
                next_ispan_set.clear();

                current_qspan_set.extend(current.qspan_iter());
                next_qspan_set.extend(next.qspan_iter());
                current_ispan_set.extend(current.ispan_iter());
                next_ispan_set.extend(next.ispan_iter());

                if current_qspan_set == next_qspan_set && current_ispan_set == next_ispan_set {
                    rule_matches.remove(j);
                    continue;
                }

                if current.ispan() == next.ispan() && current.qoverlap(next) > 0 {
                    let current_mag = current.qspan_magnitude();
                    let next_mag = next.qspan_magnitude();
                    if current_mag <= next_mag {
                        rule_matches.remove(j);
                        continue;
                    } else {
                        rule_matches.remove(i);
                        i = i.saturating_sub(1);
                        break;
                    }
                }

                if current.qcontains(next) {
                    rule_matches.remove(j);
                    continue;
                }
                if next.qcontains(current) {
                    rule_matches.remove(i);
                    i = i.saturating_sub(1);
                    break;
                }

                if current.surround(next) {
                    let combined = combine_matches(current, next);
                    if combined.qspan().len() == combined.ispan().len() {
                        rule_matches[i] = combined;
                        rule_matches.remove(j);
                        continue;
                    }
                }
                if next.surround(current) {
                    let combined = combine_matches(current, next);
                    if combined.qspan().len() == combined.ispan().len() {
                        rule_matches[j] = combined;
                        rule_matches.remove(i);
                        i = i.saturating_sub(1);
                        break;
                    }
                }

                if next.is_after(current) {
                    rule_matches[i] = combine_matches(current, next);
                    rule_matches.remove(j);
                    continue;
                }

                let (cur_qstart, cur_qend) = current.qspan_bounds();
                let (next_qstart, next_qend) = next.qspan_bounds();
                let (cur_istart, cur_iend) = current.ispan_bounds();
                let (next_istart, next_iend) = next.ispan_bounds();

                if cur_qstart <= next_qstart
                    && cur_qend <= next_qend
                    && cur_istart <= next_istart
                    && cur_iend <= next_iend
                {
                    let qoverlap = current.qoverlap(next);
                    if qoverlap > 0 {
                        let ioverlap = current.ispan_overlap(next);
                        if qoverlap == ioverlap {
                            rule_matches[i] = combine_matches(current, next);
                            rule_matches.remove(j);
                            continue;
                        }
                    }
                }

                j += 1;
            }
            i += 1;
        }
        merged.extend(rule_matches);
    }

    merged
}

/// Update match scores for all matches.
///
/// Computes scores using Python's formula:
/// `score = query_coverage * rule_coverage * relevance * 100`
///
/// Where:
/// - query_coverage = len() / qmagnitude() (ratio of matched to query region)
/// - rule_coverage = len() / rule_length (ratio of matched to rule)
/// - relevance = rule_relevance / 100
///
/// Special case: when both coverages < 1, use rule_coverage only.
///
/// # Arguments
/// * `matches` - Mutable slice of LicenseMatch to update
/// * `query` - Query reference for qmagnitude calculation
///
/// Based on Python: LicenseMatch.score() at match.py:592-619
pub(super) fn update_match_scores<'a>(matches: &mut [LicenseMatch<'a>], query: &Query<'a>) {
    for m in matches.iter_mut() {
        m.score = compute_match_score(m, query);
    }
}

fn compute_match_score<'a>(m: &LicenseMatch<'a>, query: &Query<'a>) -> f32 {
    let relevance = m.rule_relevance() as f32 / 100.0;
    if relevance < 0.001 {
        return 0.0;
    }

    let qmagnitude = m.qmagnitude(query);
    if qmagnitude == 0 {
        return 0.0;
    }

    let query_coverage = m.len() as f32 / qmagnitude as f32;
    let rule_coverage = m.icoverage();

    if query_coverage < 1.0 && rule_coverage < 1.0 {
        return (rule_coverage * relevance * 100.0).round();
    }

    (query_coverage * rule_coverage * relevance * 100.0).round()
}

/// Filter license reference matches when a license text match exists for the same expression
/// AND the reference is contained within the text match's region.
///
/// This handles cases where a short license reference appears within or directly overlapping
/// with the full license text. The reference is redundant in such cases.
///
/// A reference is discarded ONLY when:
/// - It has the same license_expression as a license text match
/// - It is shorter than the license text match
/// - It is CONTAINED within the text match's qregion (token span)
///
/// References at DIFFERENT locations are kept (e.g., MIT.t10 where "The MIT License"
/// header at line 1 is separate from the license text at lines 5-20).
pub(super) fn filter_license_references_with_text_match<'a>(
    matches: &[LicenseMatch<'a>],
) -> Vec<LicenseMatch<'a>> {
    if matches.len() < 2 {
        return matches.to_vec();
    }

    let mut to_discard = std::collections::HashSet::new();

    for i in 0..matches.len() {
        for j in 0..matches.len() {
            if i == j {
                continue;
            }

            let current = &matches[i];
            let other = &matches[j];

            if current.license_expression() == other.license_expression() {
                let current_is_ref = current.is_license_reference() && !current.is_license_text();
                let other_is_text = other.is_license_text() && !other.is_license_reference();

                if current_is_ref
                    && other_is_text
                    && current.matched_length < other.matched_length
                    && other.qcontains(current)
                {
                    to_discard.insert(i);
                }
            }
        }
    }

    if to_discard.is_empty() {
        return matches.to_vec();
    }

    let mut result = Vec::with_capacity(matches.len() - to_discard.len());
    for (i, m) in matches.iter().enumerate() {
        if !to_discard.contains(&i) {
            result.push(m.clone());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license_detection::index::LicenseIndex;
    use crate::license_detection::models::MatcherKind;
    use crate::license_detection::tests::TestMatchBuilder;

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

    #[allow(clippy::too_many_arguments)]
    fn create_test_match_with_rule_len(
        rule_identifier: &str,
        start_line: usize,
        end_line: usize,
        score: f32,
        coverage: f32,
        relevance: u8,
        rule_len: usize,
        rule_start_token: usize,
    ) -> LicenseMatch<'static> {
        let matched_len = end_line - start_line + 1;
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
            .rule_start_token(rule_start_token)
            .build_match()
    }

    #[allow(dead_code)]
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
    fn test_parse_rule_id_valid_hashes() {
        assert_eq!(parse_rule_id("#0"), Some(0));
        assert_eq!(parse_rule_id("#1"), Some(1));
        assert_eq!(parse_rule_id("#42"), Some(42));
        assert_eq!(parse_rule_id("#100"), Some(100));
        assert_eq!(parse_rule_id("#999"), Some(999));
    }

    #[test]
    fn test_parse_rule_id_plain_numbers() {
        assert_eq!(parse_rule_id("0"), Some(0));
        assert_eq!(parse_rule_id("42"), Some(42));
        assert_eq!(parse_rule_id("100"), Some(100));
    }

    #[test]
    fn test_parse_rule_id_invalid_formats() {
        assert_eq!(parse_rule_id(""), None);
        assert_eq!(parse_rule_id("#"), None);
        assert_eq!(parse_rule_id("#-1"), None);
        assert_eq!(parse_rule_id("invalid"), None);
        assert_eq!(parse_rule_id("#abc"), None);
        assert_eq!(parse_rule_id("mit.LICENSE"), None);
    }

    #[test]
    fn test_merge_overlapping_matches_same_rule() {
        let m1 = TestMatchBuilder::default()
            .license_expression("mit")
            .start_line(1)
            .end_line(10)
            .rule_length(100)
            .rule_start_token(0)
            .rule_identifier("#1")
            .score(0.9)
            .match_coverage(100.0)
            .build_match();
        let m2 = TestMatchBuilder::default()
            .license_expression("mit")
            .start_line(5)
            .end_line(15)
            .rule_length(100)
            .rule_start_token(4)
            .rule_identifier("#1")
            .score(0.85)
            .match_coverage(100.0)
            .build_match();

        let matches = vec![m1, m2];

        let merged = merge_overlapping_matches(&matches);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].rule_identifier(), "#1");
        assert_eq!(merged[0].start_line, 1);
        assert_eq!(merged[0].end_line, 15);
        assert_eq!(merged[0].score, 0.9);
    }

    #[test]
    fn test_merge_adjacent_matches_same_rule() {
        let m1 = TestMatchBuilder::default()
            .license_expression("mit")
            .start_line(1)
            .end_line(10)
            .rule_length(100)
            .rule_start_token(0)
            .rule_identifier("#1")
            .score(0.9)
            .match_coverage(100.0)
            .build_match();
        let m2 = TestMatchBuilder::default()
            .license_expression("mit")
            .start_line(10)
            .end_line(20)
            .rule_length(100)
            .rule_start_token(9)
            .rule_identifier("#1")
            .score(0.85)
            .match_coverage(100.0)
            .build_match();

        let matches = vec![m1, m2];

        let merged = merge_overlapping_matches(&matches);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].rule_identifier(), "#1");
        assert_eq!(merged[0].start_line, 1);
        assert_eq!(merged[0].end_line, 20);
        assert_eq!(merged[0].score, 0.9);
    }

    #[test]
    fn test_merge_no_overlap_different_rules() {
        let matches = vec![
            create_test_match("#1", 1, 10, 0.9, 90.0, 100),
            create_test_match("#2", 5, 15, 0.85, 85.0, 100),
        ];

        let merged = merge_overlapping_matches(&matches);

        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn test_merge_no_overlap_same_rule() {
        let matches = vec![
            create_test_match("#1", 1, 10, 0.9, 90.0, 100),
            create_test_match("#1", 20, 30, 0.85, 85.0, 100),
        ];

        let merged = merge_overlapping_matches(&matches);

        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn test_merge_multiple_matches_same_rule() {
        let m1 = create_test_match_with_rule_len("#1", 1, 5, 0.8, 100.0, 100, 100, 0);
        let m2 = create_test_match_with_rule_len("#1", 5, 10, 0.9, 100.0, 100, 100, 4);
        let m3 = create_test_match_with_rule_len("#1", 10, 15, 0.85, 100.0, 100, 100, 9);

        let matches = vec![m1, m2, m3];

        let merged = merge_overlapping_matches(&matches);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].start_line, 1);
        assert_eq!(merged[0].end_line, 15);
    }

    #[test]
    fn test_merge_empty_matches() {
        let matches: Vec<LicenseMatch> = vec![];
        let merged = merge_overlapping_matches(&matches);
        assert_eq!(merged.len(), 0);
    }

    #[test]
    fn test_merge_single_match() {
        let matches = vec![create_test_match("#1", 1, 10, 0.9, 90.0, 100)];
        let merged = merge_overlapping_matches(&matches);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].start_line, 1);
        assert_eq!(merged[0].end_line, 10);
    }

    #[test]
    fn test_update_match_scores_basic() {
        let index = LicenseIndex::with_legalese_count(10);
        let query = Query::from_extracted_text("test text", &index, false).unwrap();
        let mut matches = vec![create_test_match("#1", 1, 10, 0.5, 100.0, 100)];

        update_match_scores(&mut matches, &query);

        assert_eq!(matches[0].score, 100.0);
    }

    #[test]
    fn test_update_match_scores_multiple() {
        let index = LicenseIndex::with_legalese_count(10);
        let query = Query::from_extracted_text("test text", &index, false).unwrap();
        let mut matches = vec![
            create_test_match("#1", 1, 10, 0.5, 100.0, 80),
            create_test_match("#2", 15, 25, 0.5, 100.0, 100),
        ];

        update_match_scores(&mut matches, &query);

        assert_eq!(matches[0].score, 80.0);
        assert_eq!(matches[1].score, 100.0);
    }

    #[test]
    fn test_update_match_scores_idempotent() {
        let index = LicenseIndex::with_legalese_count(10);
        let query = Query::from_extracted_text("test text", &index, false).unwrap();
        let mut matches = vec![create_test_match("#1", 1, 10, 50.0, 50.0, 100)];

        update_match_scores(&mut matches, &query);
        let score1 = matches[0].score;

        update_match_scores(&mut matches, &query);
        let score2 = matches[0].score;

        assert_eq!(score1, score2);
    }

    #[test]
    fn test_update_match_scores_empty() {
        let index = LicenseIndex::with_legalese_count(10);
        let query = Query::from_extracted_text("test text", &index, false).unwrap();
        let mut matches: Vec<LicenseMatch> = vec![];
        update_match_scores(&mut matches, &query);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_merge_partially_overlapping_matches_same_rule() {
        let m1 = create_test_match_with_rule_len("#1", 1, 15, 0.9, 100.0, 100, 100, 0);
        let m2 = create_test_match_with_rule_len("#1", 10, 25, 0.85, 100.0, 100, 100, 9);

        let matches = vec![m1, m2];

        let merged = merge_overlapping_matches(&matches);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].start_line, 1);
        assert_eq!(merged[0].end_line, 25);
    }

    #[test]
    fn test_merge_matches_with_gap_larger_than_one() {
        let matches = vec![
            create_test_match("#1", 1, 10, 0.9, 100.0, 100),
            create_test_match("#1", 15, 25, 0.85, 100.0, 100),
        ];

        let merged = merge_overlapping_matches(&matches);

        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].start_line, 1);
        assert_eq!(merged[0].end_line, 10);
        assert_eq!(merged[1].start_line, 15);
        assert_eq!(merged[1].end_line, 25);
    }

    #[test]
    fn test_merge_preserves_max_score() {
        let m1 = create_test_match_with_rule_len("#1", 1, 10, 0.7, 100.0, 100, 100, 0);
        let m2 = create_test_match_with_rule_len("#1", 5, 15, 0.95, 100.0, 100, 100, 4);
        let m3 = create_test_match_with_rule_len("#1", 12, 20, 0.8, 100.0, 100, 100, 11);

        let matches = vec![m1, m2, m3];

        let merged = merge_overlapping_matches(&matches);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].score, 0.95);
    }

    #[test]
    fn test_qspan_magnitude_contiguous() {
        let m = TestMatchBuilder::default()
            .license_expression("mit")
            .start_line(1)
            .end_line(10)
            .start_token(5)
            .end_token(15)
            .rule_length(10)
            .build_match();
        assert_eq!(m.qspan_magnitude(), 10);
    }

    #[test]
    fn test_qspan_magnitude_non_contiguous() {
        let m = TestMatchBuilder::default()
            .license_expression("mit")
            .start_line(1)
            .end_line(10)
            .start_token(1)
            .end_token(11)
            .rule_length(10)
            .qspan_positions(Some(vec![4, 8]))
            .build_match();
        assert_eq!(m.qspan_magnitude(), 5);
    }

    #[test]
    fn test_qspan_magnitude_empty() {
        let m = TestMatchBuilder::default()
            .license_expression("mit")
            .start_line(1)
            .end_line(10)
            .start_token(1)
            .end_token(11)
            .rule_length(10)
            .qspan_positions(Some(vec![]))
            .build_match();
        assert_eq!(m.qspan_magnitude(), 0);
    }

    #[test]
    fn test_merge_equal_ispan_dense_vs_sparse() {
        let dense = TestMatchBuilder::default()
            .license_expression("mit")
            .start_line(1)
            .end_line(10)
            .start_token(1)
            .end_token(11)
            .matched_length(100)
            .rule_length(100)
            .rule_start_token(0)
            .qspan_positions(None)
            .rule_identifier("#1")
            .build_match();

        let sparse = TestMatchBuilder::default()
            .license_expression("mit")
            .start_line(1)
            .end_line(10)
            .start_token(1)
            .end_token(11)
            .matched_length(100)
            .rule_length(100)
            .rule_start_token(0)
            .qspan_positions(Some(vec![1, 5, 10, 20, 50]))
            .rule_identifier("#1")
            .build_match();

        let merged = merge_overlapping_matches(&[dense.clone(), sparse.clone()]);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].qspan_magnitude(), 10);
    }

    #[test]
    fn test_merge_equal_ispan_dense_vs_sparse_reversed() {
        let dense = TestMatchBuilder::default()
            .license_expression("mit")
            .start_line(1)
            .end_line(10)
            .start_token(1)
            .end_token(11)
            .matched_length(100)
            .rule_length(100)
            .rule_start_token(0)
            .qspan_positions(None)
            .rule_identifier("#1")
            .build_match();

        let sparse = TestMatchBuilder::default()
            .license_expression("mit")
            .start_line(1)
            .end_line(10)
            .start_token(1)
            .end_token(11)
            .matched_length(100)
            .rule_length(100)
            .rule_start_token(0)
            .qspan_positions(Some(vec![1, 5, 10, 20, 50]))
            .rule_identifier("#1")
            .build_match();

        let merged = merge_overlapping_matches(&[sparse.clone(), dense.clone()]);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].qspan_magnitude(), 10);
    }

    #[test]
    fn test_merge_equal_ispan_same_magnitude() {
        let m1 = TestMatchBuilder::default()
            .license_expression("mit")
            .start_line(1)
            .end_line(10)
            .start_token(1)
            .end_token(11)
            .matched_length(100)
            .rule_length(100)
            .rule_start_token(0)
            .rule_identifier("#1")
            .build_match();

        let m2 = TestMatchBuilder::default()
            .license_expression("mit")
            .start_line(1)
            .end_line(10)
            .start_token(1)
            .end_token(11)
            .matched_length(100)
            .rule_length(100)
            .rule_start_token(0)
            .rule_identifier("#1")
            .build_match();

        let merged = merge_overlapping_matches(&[m1, m2]);

        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn test_parse_rule_id_with_whitespace() {
        assert_eq!(parse_rule_id("  #42  "), Some(42));
        assert_eq!(parse_rule_id("  42  "), Some(42));
    }
}
