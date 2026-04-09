//! Match merging functions.
//!
//! This module contains functions for merging overlapping and adjacent matches,
//! updating match scores, and filtering license references.

use crate::license_detection::models::{LicenseMatch, MatchCoordinates, MatcherKind};
use crate::license_detection::position_set::PositionSet;
use crate::license_detection::query::Query;

const MAX_DIST: usize = 50;

fn combine_matches(a: &LicenseMatch, b: &LicenseMatch) -> LicenseMatch {
    assert_eq!(
        a.rule_identifier, b.rule_identifier,
        "Cannot combine matches with different rules: {} vs {}",
        a.rule_identifier, b.rule_identifier
    );

    let mut merged = a.clone();
    let has_rule_alignment = a.has_rule_alignment() && b.has_rule_alignment();

    let mut qspan_set = a.qspan_set();
    let b_qspan = b.query_span().clone();
    qspan_set.extend_from_span(&b_qspan);

    let (ispan_set, hispan_set) = if has_rule_alignment {
        let mut ispan_set = a.ispan_set();
        let b_ispan = b.effective_ispan();
        ispan_set.extend_from_span(&b_ispan);

        let mut hispan_set = a
            .coordinates
            .hispan()
            .map_or_else(PositionSet::new, |h| h.to_position_set());
        if let Some(b_hispan) = b.coordinates.hispan() {
            hispan_set.extend_from_span(b_hispan);
        }

        (Some(ispan_set), Some(hispan_set))
    } else {
        (None, None)
    };

    merged.start_token = if qspan_set.is_empty() {
        a.start_token
    } else {
        qspan_set.min_pos()
    };
    merged.end_token = if qspan_set.is_empty() {
        a.end_token
    } else {
        qspan_set.max_pos() + 1
    };
    merged.rule_start_token = ispan_set.as_ref().map_or(a.rule_start_token, |ispan_set| {
        if ispan_set.is_empty() {
            a.rule_start_token
        } else {
            ispan_set.min_pos()
        }
    });
    merged.matched_length = qspan_set.len();
    merged.start_line = a.start_line.min(b.start_line);
    merged.end_line = a.end_line.max(b.end_line);
    merged.score = a.score.max(b.score);

    let qspan = qspan_set.to_position_span();
    merged.coordinates = if let (Some(ispan_set), Some(hispan_set)) = (ispan_set, hispan_set) {
        MatchCoordinates::rule_aligned(
            qspan,
            ispan_set.to_position_span(),
            hispan_set.to_position_span(),
        )
    } else {
        MatchCoordinates::query_region(qspan)
    };

    if merged.rule_length > 0 {
        merged.match_coverage = LicenseMatch::round_metric(
            (merged.matched_length.min(merged.rule_length) as f32 / merged.rule_length as f32)
                * 100.0,
        );
    }

    merged
}

/// Merge overlapping and adjacent matches for the same rule.
///
/// Based on Python: `merge_matches()` (match.py:869-1068)
/// Uses distance-based merging with multiple merge conditions.
pub fn merge_overlapping_matches(matches: &[LicenseMatch]) -> Vec<LicenseMatch> {
    if matches.is_empty() {
        return Vec::new();
    }

    if matches.len() == 1 {
        return matches.to_vec();
    }

    let mut sorted: Vec<&LicenseMatch> = matches.iter().collect();
    sorted.sort_by(|a, b| {
        a.rule_identifier
            .cmp(&b.rule_identifier)
            .then_with(|| a.qstart().cmp(&b.qstart()))
            .then_with(|| b.hilen().cmp(&a.hilen()))
            .then_with(|| b.matched_length.cmp(&a.matched_length))
            .then_with(|| a.matcher_order().cmp(&b.matcher_order()))
    });

    let mut grouped: Vec<Vec<&LicenseMatch>> = Vec::new();
    let mut current_group: Vec<&LicenseMatch> = Vec::new();

    for m in sorted {
        if current_group.is_empty() || current_group[0].rule_identifier == m.rule_identifier {
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

        let rule_length = rule_matches[0].rule_length;
        let max_rule_side_dist = (rule_length / 2).clamp(1, MAX_DIST);

        let mut rule_matches: Vec<LicenseMatch> =
            rule_matches.iter().map(|m| (*m).clone()).collect();
        let mut i = 0;

        while i < rule_matches.len().saturating_sub(1) {
            let mut j = i + 1;

            while j < rule_matches.len() {
                let current = &rule_matches[i];
                let next = &rule_matches[j];
                let same_coordinate_kind =
                    current.has_rule_alignment() == next.has_rule_alignment();
                let both_rule_aligned = current.has_rule_alignment() && next.has_rule_alignment();

                if !same_coordinate_kind {
                    j += 1;
                    continue;
                }

                if !both_rule_aligned
                    && current.matcher == MatcherKind::SpdxId
                    && next.matcher == MatcherKind::SpdxId
                {
                    j += 1;
                    continue;
                }

                if current.qdistance_to(next) > max_rule_side_dist {
                    break;
                }

                if both_rule_aligned && current.idistance_to(next) > max_rule_side_dist {
                    break;
                }

                if current.qspan_set() == next.qspan_set()
                    && (!both_rule_aligned || current.ispan_set() == next.ispan_set())
                {
                    rule_matches.remove(j);
                    continue;
                }

                if both_rule_aligned
                    && current.ispan_set() == next.ispan_set()
                    && current.qoverlap(next) > 0
                {
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
                    if !both_rule_aligned
                        || combined.query_span().len() == combined.effective_ispan().len()
                    {
                        rule_matches[i] = combined;
                        rule_matches.remove(j);
                        continue;
                    }
                }
                if next.surround(current) {
                    let combined = combine_matches(current, next);
                    if !both_rule_aligned
                        || combined.query_span().len() == combined.effective_ispan().len()
                    {
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

                if both_rule_aligned {
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
pub(super) fn update_match_scores(matches: &mut [LicenseMatch], query: &Query) {
    for m in matches.iter_mut() {
        m.score = compute_match_score(m, query);
    }
}

fn compute_match_score(m: &LicenseMatch, query: &Query) -> f32 {
    let relevance = m.rule_relevance as f32 / 100.0;
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
        return ((rule_coverage * relevance * 100.0) * 100.0).round() / 100.0;
    }

    ((query_coverage * rule_coverage * relevance * 100.0) * 100.0).round() / 100.0
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
pub(super) fn filter_license_references_with_text_match(
    matches: &[LicenseMatch],
) -> Vec<LicenseMatch> {
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

            if current.license_expression == other.license_expression {
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

    matches
        .iter()
        .enumerate()
        .filter(|(i, _)| !to_discard.contains(i))
        .map(|(_, m)| m.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license_detection::index::LicenseIndex;
    use crate::license_detection::models::PositionSpan;
    use crate::models::LineNumber;

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
        let qspan = PositionSpan::range(start_line, end_line + 1);
        let ispan = PositionSpan::range(0, matched_len);
        let hispan = PositionSpan::range(0, matched_len / 2);
        LicenseMatch {
            rid,
            license_expression: "mit".to_string(),
            license_expression_spdx: Some("MIT".to_string()),
            from_file: None,
            start_line: LineNumber::new(start_line).unwrap(),
            end_line: LineNumber::new(end_line).unwrap(),
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
            coordinates: MatchCoordinates::rule_aligned(qspan, ispan, hispan),
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
        let qspan = PositionSpan::range(start_token, end_token);
        LicenseMatch {
            rid,
            license_expression: "mit".to_string(),
            license_expression_spdx: Some("MIT".to_string()),
            from_file: None,
            start_line: LineNumber::from_0_indexed(start_token),
            end_line: LineNumber::from_0_indexed(end_token.saturating_sub(1)),
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
            coordinates: MatchCoordinates::rule_aligned(
                qspan,
                PositionSpan::empty(),
                PositionSpan::empty(),
            ),
            candidate_resemblance: 0.0,
            candidate_containment: 0.0,
        }
    }

    fn create_test_query_region_match(
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
            start_line: LineNumber::from_0_indexed(start_token),
            end_line: LineNumber::from_0_indexed(end_token.saturating_sub(1)),
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
            coordinates: MatchCoordinates::query_region(PositionSpan::range(
                start_token,
                end_token,
            )),
            candidate_resemblance: 0.0,
            candidate_containment: 0.0,
        }
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
        let mut m1 = create_test_match("#1", 1, 10, 0.9, 100.0, 100);
        m1.rule_length = 100;
        m1.rule_start_token = 0;
        m1.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(1, 11),
            PositionSpan::range(0, 10),
            PositionSpan::empty(),
        );
        let mut m2 = create_test_match("#1", 5, 15, 0.85, 100.0, 100);
        m2.rule_length = 100;
        m2.rule_start_token = 4;
        m2.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(5, 16),
            PositionSpan::range(4, 15),
            PositionSpan::empty(),
        );

        let matches = vec![m1, m2];

        let merged = merge_overlapping_matches(&matches);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].rule_identifier, "#1");
        assert_eq!(merged[0].start_line, LineNumber::ONE);
        assert_eq!(merged[0].end_line, LineNumber::new(15).unwrap());
        assert_eq!(merged[0].score, 0.9);
    }

    #[test]
    fn test_merge_adjacent_matches_same_rule() {
        let mut m1 = create_test_match("#1", 1, 10, 0.9, 100.0, 100);
        m1.rule_length = 100;
        m1.rule_start_token = 0;
        m1.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(1, 11),
            PositionSpan::range(0, 10),
            PositionSpan::empty(),
        );
        let mut m2 = create_test_match("#1", 10, 20, 0.85, 100.0, 100);
        m2.rule_length = 100;
        m2.rule_start_token = 9;
        m2.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(10, 21),
            PositionSpan::range(9, 20),
            PositionSpan::empty(),
        );

        let matches = vec![m1, m2];

        let merged = merge_overlapping_matches(&matches);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].rule_identifier, "#1");
        assert_eq!(merged[0].start_line, LineNumber::ONE);
        assert_eq!(merged[0].end_line, LineNumber::new(20).unwrap());
        assert_eq!(merged[0].score, 0.9);
    }

    #[test]
    fn test_merge_adjacent_unknown_matches_preserve_query_region_coordinates() {
        let mut m1 = create_test_query_region_match("#1", 10, 20, 10);
        m1.rule_length = 100;
        m1.matcher = crate::license_detection::models::MatcherKind::Unknown;
        let mut m2 = create_test_query_region_match("#1", 20, 30, 10);
        m2.rule_length = 100;
        m2.matcher = crate::license_detection::models::MatcherKind::Unknown;

        let merged = merge_overlapping_matches(&[m1, m2]);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].qspan_bounds(), (10, 30));
        assert_eq!(merged[0].matched_length, 20);
        assert!(!merged[0].has_rule_alignment());
    }

    #[test]
    fn test_do_not_merge_adjacent_spdx_query_region_matches() {
        let mut m1 = create_test_query_region_match("#1", 10, 20, 10);
        m1.rule_length = 100;
        m1.matcher = crate::license_detection::models::MatcherKind::SpdxId;
        let mut m2 = create_test_query_region_match("#1", 20, 30, 10);
        m2.rule_length = 100;
        m2.matcher = crate::license_detection::models::MatcherKind::SpdxId;

        let merged = merge_overlapping_matches(&[m1, m2]);

        assert_eq!(merged.len(), 2);
        assert!(merged.iter().all(|m| !m.has_rule_alignment()));
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
        let mut m1 = create_test_match("#1", 1, 5, 0.8, 100.0, 100);
        m1.rule_length = 100;
        m1.rule_start_token = 0;
        m1.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(1, 6),
            PositionSpan::range(0, 5),
            PositionSpan::empty(),
        );
        let mut m2 = create_test_match("#1", 5, 10, 0.9, 100.0, 100);
        m2.rule_length = 100;
        m2.rule_start_token = 4;
        m2.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(5, 11),
            PositionSpan::range(4, 10),
            PositionSpan::empty(),
        );
        let mut m3 = create_test_match("#1", 10, 15, 0.85, 100.0, 100);
        m3.rule_length = 100;
        m3.rule_start_token = 9;
        m3.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(10, 16),
            PositionSpan::range(9, 15),
            PositionSpan::empty(),
        );

        let matches = vec![m1, m2, m3];

        let merged = merge_overlapping_matches(&matches);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].start_line, LineNumber::ONE);
        assert_eq!(merged[0].end_line, LineNumber::new(15).unwrap());
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
        assert_eq!(merged[0].start_line, LineNumber::ONE);
        assert_eq!(merged[0].end_line, LineNumber::new(10).unwrap());
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
    fn test_update_match_scores_keeps_two_decimal_precision() {
        let index = LicenseIndex::with_legalese_count(10);
        let query = Query::from_extracted_text("test text", &index, false).unwrap();
        let mut matches = vec![create_test_match("#1", 1, 2, 0.0, 0.0, 100)];
        matches[0].rule_length = 3;

        update_match_scores(&mut matches, &query);

        assert!((matches[0].score - 66.67).abs() < 0.001);
    }

    #[test]
    fn test_merge_partially_overlapping_matches_same_rule() {
        let mut m1 = create_test_match("#1", 1, 15, 0.9, 100.0, 100);
        m1.rule_length = 100;
        m1.rule_start_token = 0;
        m1.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(1, 16),
            PositionSpan::range(0, 15),
            PositionSpan::empty(),
        );
        let mut m2 = create_test_match("#1", 10, 25, 0.85, 100.0, 100);
        m2.rule_length = 100;
        m2.rule_start_token = 9;
        m2.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(10, 26),
            PositionSpan::range(9, 25),
            PositionSpan::empty(),
        );

        let matches = vec![m1, m2];

        let merged = merge_overlapping_matches(&matches);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].start_line, LineNumber::ONE);
        assert_eq!(merged[0].end_line, LineNumber::new(25).unwrap());
    }

    #[test]
    fn test_merge_matches_with_gap_larger_than_one() {
        let mut m1 = create_test_match("#1", 1, 10, 0.9, 100.0, 100);
        m1.rule_length = 10;
        m1.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(1, 11),
            PositionSpan::range(0, 10),
            PositionSpan::empty(),
        );
        let mut m2 = create_test_match("#1", 20, 30, 0.85, 100.0, 100);
        m2.rule_length = 11;
        m2.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(20, 31),
            PositionSpan::range(19, 30),
            PositionSpan::empty(),
        );

        let matches = vec![m1, m2];

        let merged = merge_overlapping_matches(&matches);

        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].start_line, LineNumber::ONE);
        assert_eq!(merged[0].end_line, LineNumber::new(10).unwrap());
        assert_eq!(merged[1].start_line, LineNumber::new(20).unwrap());
        assert_eq!(merged[1].end_line, LineNumber::new(30).unwrap());
    }

    #[test]
    fn test_merge_preserves_max_score() {
        let mut m1 = create_test_match("#1", 1, 10, 0.7, 100.0, 100);
        m1.rule_length = 100;
        m1.rule_start_token = 0;
        m1.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(1, 11),
            PositionSpan::range(0, 10),
            PositionSpan::empty(),
        );
        let mut m2 = create_test_match("#1", 5, 15, 0.95, 100.0, 100);
        m2.rule_length = 100;
        m2.rule_start_token = 4;
        m2.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(5, 16),
            PositionSpan::range(4, 15),
            PositionSpan::empty(),
        );
        let mut m3 = create_test_match("#1", 12, 20, 0.8, 100.0, 100);
        m3.rule_length = 100;
        m3.rule_start_token = 11;
        m3.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(12, 21),
            PositionSpan::range(11, 20),
            PositionSpan::empty(),
        );

        let matches = vec![m1, m2, m3];

        let merged = merge_overlapping_matches(&matches);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].score, 0.95);
    }

    #[test]
    fn test_qspan_magnitude_contiguous() {
        let mut m = create_test_match("#1", 1, 10, 0.9, 90.0, 100);
        m.start_token = 5;
        m.end_token = 15;
        m.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(5, 15),
            PositionSpan::empty(),
            PositionSpan::empty(),
        );
        assert_eq!(m.qspan_magnitude(), 10);
    }

    #[test]
    fn test_qspan_magnitude_non_contiguous() {
        let mut m = create_test_match("#1", 1, 10, 0.9, 90.0, 100);
        m.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::from_positions(vec![4, 8]),
            PositionSpan::empty(),
            PositionSpan::empty(),
        );
        assert_eq!(m.qspan_magnitude(), 5);
    }

    #[test]
    fn test_qspan_magnitude_empty() {
        let mut m = create_test_match("#1", 1, 10, 0.9, 90.0, 100);
        m.start_token = 5;
        m.end_token = 5;
        m.coordinates = MatchCoordinates::query_region(PositionSpan::empty());
        assert_eq!(m.qspan_magnitude(), 0);
    }

    #[test]
    fn test_merge_equal_ispan_dense_vs_sparse() {
        let mut dense = create_test_match_with_tokens("#1", 1, 11, 100);
        dense.rule_start_token = 0;
        dense.matched_length = 100;
        dense.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(1, 11),
            PositionSpan::empty(),
            PositionSpan::empty(),
        );

        let mut sparse = create_test_match_with_tokens("#1", 1, 11, 100);
        sparse.rule_start_token = 0;
        sparse.matched_length = 100;
        sparse.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::from_positions(vec![1, 5, 10, 20, 50]),
            PositionSpan::empty(),
            PositionSpan::empty(),
        );

        let merged = merge_overlapping_matches(&[dense.clone(), sparse.clone()]);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].qspan_magnitude(), 10);
    }

    #[test]
    fn test_merge_equal_ispan_dense_vs_sparse_reversed() {
        let mut dense = create_test_match_with_tokens("#1", 1, 11, 100);
        dense.rule_start_token = 0;
        dense.matched_length = 100;
        dense.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(1, 11),
            PositionSpan::empty(),
            PositionSpan::empty(),
        );

        let mut sparse = create_test_match_with_tokens("#1", 1, 11, 100);
        sparse.rule_start_token = 0;
        sparse.matched_length = 100;
        sparse.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::from_positions(vec![1, 5, 10, 20, 50]),
            PositionSpan::empty(),
            PositionSpan::empty(),
        );

        let merged = merge_overlapping_matches(&[sparse.clone(), dense.clone()]);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].qspan_magnitude(), 10);
    }

    #[test]
    fn test_merge_equal_ispan_same_magnitude() {
        let mut m1 = create_test_match_with_tokens("#1", 1, 11, 100);
        m1.rule_start_token = 0;
        m1.matched_length = 100;

        let mut m2 = create_test_match_with_tokens("#1", 1, 11, 100);
        m2.rule_start_token = 0;
        m2.matched_length = 100;

        let merged = merge_overlapping_matches(&[m1, m2]);

        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn test_parse_rule_id_with_whitespace() {
        assert_eq!(parse_rule_id("  #42  "), Some(42));
        assert_eq!(parse_rule_id("  42  "), Some(42));
    }
}
