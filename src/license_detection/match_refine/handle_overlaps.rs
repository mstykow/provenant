//! Overlap handling for license matches.
//!
//! This module contains functions for detecting and resolving overlapping matches
//! based on containment, overlap ratios, and license expression relationships.

use crate::license_detection::expression::licensing_contains;
use crate::license_detection::index::LicenseIndex;
use crate::license_detection::models::{LicenseMatch, MatcherKind};
use crate::license_detection::position_set::PositionSet;

use super::merge::merge_overlapping_matches;

const OVERLAP_SMALL: f64 = 0.10;
const OVERLAP_MEDIUM: f64 = 0.40;
const OVERLAP_LARGE: f64 = 0.70;
const OVERLAP_EXTRA_LARGE: f64 = 0.90;

/// Filter matches that are contained within other matches.
///
/// A match A is contained in match B if:
/// - A's qspan (token positions) is contained in B's qspan, OR
/// - B's license expression subsumes A's expression (e.g., "gpl-2.0 WITH exception" subsumes "gpl-2.0")
///
/// This uses token positions (start_token/end_token) instead of line numbers
/// for more precise containment detection, matching Python's qcontains behavior.
/// Expression subsumption handles WITH expressions where the base license should
/// not appear separately when the WITH expression is detected.
///
/// The containing (larger) match is kept, the contained (smaller) match is removed.
/// This function does NOT group by rule_identifier - matches from different rules
/// can contain each other.
///
/// # Arguments
/// * `matches` - Slice of LicenseMatch to filter
///
/// # Returns
/// Tuple of (kept matches, discarded matches)
///
/// Based on Python: `filter_contained_matches()` using qspan containment and expression subsumption
pub fn filter_contained_matches(
    matches: &[LicenseMatch],
) -> (Vec<LicenseMatch>, Vec<LicenseMatch>) {
    if matches.len() < 2 {
        return (matches.to_vec(), Vec::new());
    }

    let mut matches: Vec<LicenseMatch> = matches.to_vec();
    let mut discarded = Vec::new();

    matches.sort_by(|a, b| {
        a.qstart()
            .cmp(&b.qstart())
            .then_with(|| b.hilen().cmp(&a.hilen()))
            .then_with(|| b.len().cmp(&a.len()))
            .then_with(|| a.matcher_order().cmp(&b.matcher_order()))
    });

    let mut i = 0;
    while i < matches.len().saturating_sub(1) {
        let mut j = i + 1;
        while j < matches.len() {
            let current = matches[i].clone();
            let next = matches[j].clone();

            let (_, current_qend) = current.qspan_bounds();
            let (_, next_qend) = next.qspan_bounds();

            if next_qend > current_qend {
                break;
            }

            if current.qspan_eq(&next) {
                if is_generic_license_reference_notice(&current)
                    != is_generic_license_reference_notice(&next)
                {
                    if is_generic_license_reference_notice(&current) {
                        discarded.push(matches.remove(j));
                        continue;
                    }

                    discarded.push(matches.remove(i));
                    i = i.saturating_sub(1);
                    break;
                }

                if current.coverage() >= next.coverage() {
                    discarded.push(matches.remove(j));
                    continue;
                } else {
                    discarded.push(matches.remove(i));
                    i = i.saturating_sub(1);
                    break;
                }
            }

            if current.qcontains(&next) {
                discarded.push(matches.remove(j));
                continue;
            }
            if next.qcontains(&current) {
                discarded.push(matches.remove(i));
                i = i.saturating_sub(1);
                break;
            }

            j += 1;
        }
        i += 1;
    }

    (matches, discarded)
}

fn is_false_positive(m: &LicenseMatch, index: &LicenseIndex) -> bool {
    index.false_positive_rids.contains(&m.rid)
}

fn is_strong_exact_match(match_item: &LicenseMatch) -> bool {
    match_item.matcher == MatcherKind::Aho && match_item.coverage() == 100.0
}

fn is_low_confidence_seq_match(match_item: &LicenseMatch) -> bool {
    match_item.matcher == MatcherKind::Seq && match_item.coverage() < 10.0
}

fn licensing_contains_match(current: &LicenseMatch, other: &LicenseMatch) -> bool {
    if current.license_expression.is_empty() || other.license_expression.is_empty() {
        return false;
    }
    licensing_contains(&current.license_expression, &other.license_expression)
}

fn is_generic_license_reference_notice(match_item: &LicenseMatch) -> bool {
    match_item
        .referenced_filenames
        .as_ref()
        .is_some_and(|filenames| {
            filenames.len() == 1
                && filenames[0].eq_ignore_ascii_case("LICENSE")
                && match_item.license_expression.contains(" OR ")
        })
}

/// Filter overlapping matches based on overlap ratios and license expressions.
///
/// This function handles complex overlapping scenarios where multiple matches
/// overlap at the same location. It uses overlap ratios and license expression
/// relationships to determine which matches to keep.
///
/// # Arguments
/// * `matches` - Vector of LicenseMatch to filter
/// * `index` - LicenseIndex for false positive checking
///
/// # Returns
/// Tuple of (kept matches, discarded matches)
pub fn filter_overlapping_matches(
    matches: Vec<LicenseMatch>,
    index: &LicenseIndex,
) -> (Vec<LicenseMatch>, Vec<LicenseMatch>) {
    if matches.len() < 2 {
        return (matches, vec![]);
    }

    let mut matches = matches;
    let mut discarded: Vec<LicenseMatch> = vec![];

    matches.sort_by(|a, b| {
        a.qstart()
            .cmp(&b.qstart())
            .then_with(|| b.hilen().cmp(&a.hilen()))
            .then_with(|| b.len().cmp(&a.len()))
            .then_with(|| a.matcher_order().cmp(&b.matcher_order()))
    });

    let mut i = 0;
    while i < matches.len().saturating_sub(1) {
        let mut j = i + 1;
        while j < matches.len() {
            let (_, current_qend) = matches[i].qspan_bounds();
            let (next_qstart, _) = matches[j].qspan_bounds();

            if next_qstart > current_qend {
                break;
            }

            let both_fp =
                is_false_positive(&matches[i], index) && is_false_positive(&matches[j], index);
            if both_fp {
                j += 1;
                continue;
            }

            let overlap = matches[i].qoverlap(&matches[j]);
            if overlap == 0 {
                j += 1;
                continue;
            }

            let next_len = matches[j].len();
            let current_len = matches[i].len();

            if next_len == 0 || current_len == 0 {
                j += 1;
                continue;
            }

            let overlap_ratio_to_next = overlap as f64 / next_len as f64;
            let overlap_ratio_to_current = overlap as f64 / current_len as f64;

            let extra_large_next = overlap_ratio_to_next >= OVERLAP_EXTRA_LARGE;
            let large_next = overlap_ratio_to_next >= OVERLAP_LARGE;
            let medium_next = overlap_ratio_to_next >= OVERLAP_MEDIUM;
            let small_next = overlap_ratio_to_next >= OVERLAP_SMALL;

            let extra_large_current = overlap_ratio_to_current >= OVERLAP_EXTRA_LARGE;
            let large_current = overlap_ratio_to_current >= OVERLAP_LARGE;
            let medium_current = overlap_ratio_to_current >= OVERLAP_MEDIUM;
            let small_current = overlap_ratio_to_current >= OVERLAP_SMALL;

            let current_len_val = matches[i].len();
            let next_len_val = matches[j].len();
            let current_hilen = matches[i].hilen();
            let next_hilen = matches[j].hilen();
            let current_is_generic_license_notice =
                is_generic_license_reference_notice(&matches[i]);
            let next_is_generic_license_notice = is_generic_license_reference_notice(&matches[j]);

            if medium_next
                && is_strong_exact_match(&matches[i])
                && is_low_confidence_seq_match(&matches[j])
            {
                discarded.push(matches.remove(j));
                continue;
            }

            if medium_current
                && is_low_confidence_seq_match(&matches[i])
                && is_strong_exact_match(&matches[j])
            {
                discarded.push(matches.remove(i));
                i = i.saturating_sub(1);
                break;
            }

            // Note: We do NOT use candidate_resemblance for tie-breaking here.
            // candidate_resemblance is a GLOBAL measure based on multiset intersection
            // over the entire query and rule, not the actual matched region.
            // Using it for LOCAL overlap decisions produces wrong results.
            // See: CC-BY-SA-2.0 vs CC-BY-NC-SA-2.0 where NC-SA has higher
            // candidate_resemblance but lower actual coverage.

            // When overlap is >= 90%, prefer higher coverage when lengths are equal.
            // This ensures that for matches with identical qspan, the one with better
            // coverage is kept. See: gfdl-1.1 vs gfdl-1.1-plus where both match the
            // same text but gfdl-1.1 has higher coverage.
            if extra_large_next
                && extra_large_current
                && current_len_val == next_len_val
                && current_is_generic_license_notice != next_is_generic_license_notice
            {
                if current_is_generic_license_notice {
                    discarded.push(matches.remove(j));
                    continue;
                }

                discarded.push(matches.remove(i));
                i = i.saturating_sub(1);
                break;
            }

            if extra_large_next && current_len_val >= next_len_val {
                // If lengths are equal, prefer higher coverage
                if current_len_val == next_len_val && matches[i].coverage() < matches[j].coverage()
                {
                    discarded.push(matches.remove(i));
                    i = i.saturating_sub(1);
                    break;
                }
                discarded.push(matches.remove(j));
                continue;
            }

            if extra_large_current && current_len_val <= next_len_val {
                // If lengths are equal, prefer higher coverage
                if current_len_val == next_len_val && matches[i].coverage() < matches[j].coverage()
                {
                    discarded.push(matches.remove(j));
                    continue;
                }
                discarded.push(matches.remove(i));
                i = i.saturating_sub(1);
                break;
            }

            if large_next && current_len_val >= next_len_val && current_hilen >= next_hilen {
                discarded.push(matches.remove(j));
                continue;
            }

            if large_current && current_len_val <= next_len_val && current_hilen <= next_hilen {
                discarded.push(matches.remove(i));
                i = i.saturating_sub(1);
                break;
            }

            if medium_next {
                if licensing_contains_match(&matches[i], &matches[j])
                    && current_len_val >= next_len_val
                    && current_hilen >= next_hilen
                {
                    discarded.push(matches.remove(j));
                    continue;
                }

                if licensing_contains_match(&matches[j], &matches[i])
                    && current_len_val <= next_len_val
                    && current_hilen <= next_hilen
                {
                    discarded.push(matches.remove(i));
                    i = i.saturating_sub(1);
                    break;
                }

                if next_len_val == 2
                    && current_len_val >= next_len_val + 2
                    && current_hilen >= next_hilen
                {
                    let current_ends = index
                        .rules_by_rid
                        .get(matches[i].rid)
                        .map(|r| r.ends_with_license)
                        .unwrap_or(false);
                    let next_starts = index
                        .rules_by_rid
                        .get(matches[j].rid)
                        .map(|r| r.starts_with_license)
                        .unwrap_or(false);

                    if current_ends && next_starts {
                        discarded.push(matches.remove(j));
                        continue;
                    }
                }
            }

            if medium_current {
                if licensing_contains_match(&matches[i], &matches[j])
                    && current_len_val >= next_len_val
                    && current_hilen >= next_hilen
                {
                    discarded.push(matches.remove(j));
                    continue;
                }

                if licensing_contains_match(&matches[j], &matches[i])
                    && current_len_val <= next_len_val
                    && current_hilen <= next_hilen
                {
                    discarded.push(matches.remove(i));
                    i = i.saturating_sub(1);
                    break;
                }
            }

            if small_next
                && matches[i].surround(&matches[j])
                && licensing_contains_match(&matches[i], &matches[j])
                && current_len_val >= next_len_val
                && current_hilen >= next_hilen
            {
                discarded.push(matches.remove(j));
                continue;
            }

            if small_current
                && matches[j].surround(&matches[i])
                && licensing_contains_match(&matches[j], &matches[i])
                && current_len_val <= next_len_val
                && current_hilen <= next_hilen
            {
                discarded.push(matches.remove(i));
                i = i.saturating_sub(1);
                break;
            }

            if i > 0 {
                let prev_next_overlap = matches[i - 1].qspan_overlap(&matches[j]);

                if prev_next_overlap == 0 {
                    let cpo = matches[i].qspan_overlap(&matches[i - 1]);
                    let cno = matches[i].qspan_overlap(&matches[j]);

                    if cpo > 0 && cno > 0 {
                        let overlap_len = cpo + cno;
                        let clen = matches[i].len();

                        if overlap_len as f64 >= clen as f64 * 0.9 {
                            discarded.push(matches.remove(i));
                            i = i.saturating_sub(1);
                            break;
                        }
                    }
                }
            }

            j += 1;
        }
        i += 1;
    }

    (matches, discarded)
}

/// Restore non-overlapping discarded matches.
///
/// After filtering, some matches may have been discarded that don't actually
/// overlap with the kept matches. This function restores those non-overlapping
/// discarded matches.
///
/// # Arguments
/// * `matches` - Slice of kept LicenseMatch
/// * `discarded` - Vector of discarded LicenseMatch to check for restoration
///
/// # Returns
/// Tuple of (restored matches, still-discarded matches)
pub fn restore_non_overlapping(
    matches: &[LicenseMatch],
    discarded: Vec<LicenseMatch>,
) -> (Vec<LicenseMatch>, Vec<LicenseMatch>) {
    let mut all_matched_positions = PositionSet::new();
    for m in matches {
        all_matched_positions.extend_from_span(m.query_span());
    }

    let mut to_keep = Vec::new();
    let mut to_discard = Vec::new();

    let merged_discarded = merge_overlapping_matches(&discarded);

    for disc in merged_discarded {
        if !all_matched_positions.overlaps_span(disc.query_span()) {
            to_keep.push(disc);
        } else {
            to_discard.push(disc);
        }
    }

    (to_keep, to_discard)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license_detection::models::MatchCoordinates;
    use crate::license_detection::models::position_span::PositionSpan;
    use crate::models::LineNumber;
    use crate::models::MatchScore;

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
        score: MatchScore,
        coverage: f32,
        relevance: u8,
    ) -> LicenseMatch {
        let matched_len = end_line - start_line + 1;
        let rule_len = matched_len;
        let rid = parse_rule_id(rule_identifier).unwrap_or(0);
        let start_line_ln = LineNumber::new(start_line).expect("valid start_line");
        let end_line_ln = LineNumber::new(end_line).expect("valid end_line");
        LicenseMatch {
            rid,
            license_expression: "mit".to_string(),
            license_expression_spdx: Some("MIT".to_string()),
            from_file: None,
            start_line: start_line_ln,
            end_line: end_line_ln,
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
            coordinates: MatchCoordinates::query_region(PositionSpan::range(
                start_line,
                end_line + 1,
            )),
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
            start_line: LineNumber::from_0_indexed(start_token),
            end_line: if end_token == 0 {
                LineNumber::ONE
            } else {
                LineNumber::from_0_indexed(end_token - 1)
            },
            start_token,
            end_token,
            matcher: crate::license_detection::models::MatcherKind::Aho,
            score: MatchScore::MAX,
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
                PositionSpan::range(start_token, end_token),
                PositionSpan::range(0, matched_length),
                PositionSpan::range(0, matched_length / 2),
            ),
            candidate_resemblance: 0.0,
            candidate_containment: 0.0,
        }
    }

    #[test]
    fn test_filter_contained_matches_simple() {
        let matches = vec![
            create_test_match("#1", 1, 20, MatchScore::from_percentage(0.9), 90.0, 100),
            create_test_match("#1", 5, 15, MatchScore::from_percentage(0.85), 85.0, 100),
        ];

        let (filtered, _) = filter_contained_matches(&matches);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].start_line, LineNumber::ONE);
        assert_eq!(filtered[0].end_line, LineNumber::new(20).expect("valid"));
    }

    #[test]
    fn test_filter_contained_matches_multiple() {
        let matches = vec![
            create_test_match("#1", 1, 30, MatchScore::from_percentage(0.9), 90.0, 100),
            create_test_match("#1", 5, 10, MatchScore::from_percentage(0.8), 80.0, 100),
            create_test_match("#1", 15, 20, MatchScore::from_percentage(0.85), 85.0, 100),
        ];

        let (filtered, _) = filter_contained_matches(&matches);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].start_line, LineNumber::ONE);
        assert_eq!(filtered[0].end_line, LineNumber::new(30).expect("valid"));
    }

    #[test]
    fn test_filter_contained_matches_different_rules() {
        let mut m1 = create_test_match("#1", 1, 20, MatchScore::from_percentage(0.9), 90.0, 100);
        m1.matched_length = 200;
        m1.coordinates = MatchCoordinates::query_region(PositionSpan::range(1, 21));
        let mut m2 = create_test_match("#2", 5, 15, MatchScore::from_percentage(0.85), 85.0, 100);
        m2.matched_length = 100;
        m2.coordinates = MatchCoordinates::query_region(PositionSpan::range(5, 16));
        let matches = vec![m1, m2];

        let (filtered, _) = filter_contained_matches(&matches);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].rule_identifier, "#1");
    }

    #[test]
    fn test_filter_contained_matches_no_containment() {
        let mut m1 = create_test_match("#1", 1, 10, MatchScore::from_percentage(0.9), 90.0, 100);
        m1.coordinates = MatchCoordinates::query_region(PositionSpan::range(1, 11));
        let mut m2 = create_test_match("#1", 15, 25, MatchScore::from_percentage(0.85), 85.0, 100);
        m2.coordinates = MatchCoordinates::query_region(PositionSpan::range(15, 26));
        let matches = vec![m1, m2];

        let (filtered, _) = filter_contained_matches(&matches);

        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_contained_matches_empty() {
        let matches: Vec<LicenseMatch> = vec![];
        let (filtered, _) = filter_contained_matches(&matches);
        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn test_filter_contained_matches_single() {
        let matches = vec![create_test_match(
            "#1",
            1,
            10,
            MatchScore::from_percentage(0.9),
            90.0,
            100,
        )];
        let (filtered, _) = filter_contained_matches(&matches);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_contained_matches_partial_overlap_no_containment() {
        let mut m1 = create_test_match("#1", 1, 20, MatchScore::from_percentage(0.9), 90.0, 100);
        m1.matched_length = 150;
        m1.coordinates = MatchCoordinates::query_region(PositionSpan::range(1, 21));
        let mut m2 = create_test_match("#2", 15, 30, MatchScore::from_percentage(0.85), 85.0, 100);
        m2.matched_length = 100;
        m2.coordinates = MatchCoordinates::query_region(PositionSpan::range(15, 31));
        let matches = vec![m1, m2];

        let (filtered, _) = filter_contained_matches(&matches);

        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_contained_matches_equal_start_different_end() {
        let mut m1 = create_test_match("#1", 1, 30, MatchScore::from_percentage(0.9), 90.0, 100);
        m1.matched_length = 200;
        let mut m2 = create_test_match("#2", 1, 15, MatchScore::from_percentage(0.85), 85.0, 100);
        m2.matched_length = 100;
        let matches = vec![m1, m2];

        let (filtered, _) = filter_contained_matches(&matches);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].end_line, LineNumber::new(30).expect("valid"));
    }

    #[test]
    fn test_filter_contained_matches_nested_containment() {
        let mut outer = create_test_match("#1", 1, 50, MatchScore::from_percentage(0.9), 90.0, 100);
        outer.matched_length = 300;
        let mut middle =
            create_test_match("#2", 10, 40, MatchScore::from_percentage(0.85), 85.0, 100);
        middle.matched_length = 200;
        let mut inner =
            create_test_match("#3", 15, 35, MatchScore::from_percentage(0.8), 80.0, 100);
        inner.matched_length = 100;
        let matches = vec![inner, middle, outer];

        let (filtered, _) = filter_contained_matches(&matches);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].start_line, LineNumber::ONE);
        assert_eq!(filtered[0].end_line, LineNumber::new(50).expect("valid"));
    }

    #[test]
    fn test_filter_contained_matches_same_boundaries_different_matched_length() {
        let mut m1 = create_test_match("#1", 1, 10, MatchScore::from_percentage(0.9), 90.0, 100);
        m1.matched_length = 200;
        let mut m2 = create_test_match("#2", 1, 10, MatchScore::from_percentage(0.85), 85.0, 100);
        m2.matched_length = 100;
        let matches = vec![m1, m2];

        let (filtered, _) = filter_contained_matches(&matches);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].matched_length, 200);
    }

    #[test]
    fn test_filter_contained_matches_token_positions_fully_contained() {
        let outer = create_test_match_with_tokens("#1", 0, 20, 20);
        let inner = create_test_match_with_tokens("#2", 5, 15, 10);
        let matches = vec![outer, inner];

        let (filtered, _) = filter_contained_matches(&matches);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].start_token, 0);
        assert_eq!(filtered[0].end_token, 20);
    }

    #[test]
    fn test_filter_contained_matches_token_positions_partial_overlap_not_contained() {
        let m1 = create_test_match_with_tokens("#1", 0, 10, 10);
        let m2 = create_test_match_with_tokens("#2", 5, 15, 10);
        let matches = vec![m1, m2];

        let (filtered, _) = filter_contained_matches(&matches);

        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_contained_matches_token_positions_non_overlapping() {
        let m1 = create_test_match_with_tokens("#1", 0, 10, 10);
        let m2 = create_test_match_with_tokens("#2", 20, 30, 10);
        let matches = vec![m1, m2];

        let (filtered, _) = filter_contained_matches(&matches);

        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_contained_matches_token_positions_nested_containment() {
        let outer = create_test_match_with_tokens("#1", 0, 50, 50);
        let middle = create_test_match_with_tokens("#2", 10, 40, 30);
        let inner = create_test_match_with_tokens("#3", 15, 35, 20);
        let matches = vec![inner, middle, outer];

        let (filtered, _) = filter_contained_matches(&matches);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].start_token, 0);
        assert_eq!(filtered[0].end_token, 50);
    }

    #[test]
    fn test_filter_contained_matches_token_positions_same_boundaries() {
        let m1 = create_test_match_with_tokens("#1", 0, 10, 10);
        let m2 = create_test_match_with_tokens("#2", 0, 10, 10);
        let matches = vec![m1, m2];

        let (filtered, _) = filter_contained_matches(&matches);

        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_contained_matches_token_positions_multiple_contained() {
        let outer = create_test_match_with_tokens("#1", 0, 100, 100);
        let inner1 = create_test_match_with_tokens("#2", 10, 20, 10);
        let inner2 = create_test_match_with_tokens("#3", 30, 40, 10);
        let inner3 = create_test_match_with_tokens("#4", 50, 60, 10);
        let matches = vec![outer, inner1, inner2, inner3];

        let (filtered, _) = filter_contained_matches(&matches);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].start_token, 0);
        assert_eq!(filtered[0].end_token, 100);
    }

    #[test]
    fn test_filter_contained_matches_gpl_variant_issue() {
        let gpl_1_0 = create_test_match_with_tokens("#20560", 10, 19, 9);
        let gpl_2_0 = create_test_match_with_tokens("#16218", 10, 32, 22);
        let matches = vec![gpl_1_0.clone(), gpl_2_0.clone()];

        let (filtered, _) = filter_contained_matches(&matches);

        assert_eq!(filtered.len(), 1, "Should filter contained GPL match");
        assert_eq!(
            filtered[0].rule_identifier, "#16218",
            "Should keep gpl-2.0-plus"
        );
        assert_eq!(filtered[0].end_token, 32, "Should have correct end_token");
    }

    #[test]
    fn test_filter_contained_matches_gpl_variant_zero_tokens() {
        let mut gpl_1_0 = create_test_match_with_tokens("#20560", 0, 0, 9);
        gpl_1_0.start_line = LineNumber::new(13).expect("valid");
        gpl_1_0.end_line = LineNumber::new(14).expect("valid");

        let mut gpl_2_0 = create_test_match_with_tokens("#16218", 0, 0, 22);
        gpl_2_0.start_line = LineNumber::new(13).expect("valid");
        gpl_2_0.end_line = LineNumber::new(15).expect("valid");

        let matches = vec![gpl_1_0.clone(), gpl_2_0.clone()];

        let (filtered, _) = filter_contained_matches(&matches);

        assert_eq!(
            filtered.len(),
            1,
            "Should filter contained GPL match (line-based)"
        );
        assert_eq!(
            filtered[0].rule_identifier, "#16218",
            "Should keep gpl-2.0-plus"
        );
    }

    #[test]
    fn test_filter_contained_matches_prefers_reference_rule_for_same_qspan() {
        let mut referenced = create_test_match(
            "gpl-1.0-plus_or_mit_2.RULE",
            1,
            1,
            MatchScore::from_percentage(50.75),
            50.75,
            100,
        );
        referenced.license_expression = "gpl-1.0-plus OR mit".to_string();
        referenced.license_expression_spdx = Some("GPL-1.0-or-later OR MIT".to_string());
        referenced.matched_length = 34;
        referenced.coordinates = MatchCoordinates::query_region(PositionSpan::range(2166, 2200));
        referenced.referenced_filenames = Some(vec!["LICENSE".to_string()]);

        let mut plain = create_test_match(
            "gpl-2.0_290.RULE",
            1,
            1,
            MatchScore::from_percentage(60.71),
            60.71,
            100,
        );
        plain.license_expression = "gpl-2.0".to_string();
        plain.license_expression_spdx = Some("GPL-2.0-only".to_string());
        plain.matched_length = 34;
        plain.coordinates = MatchCoordinates::query_region(PositionSpan::range(2166, 2200));

        let (filtered, discarded) = filter_contained_matches(&[referenced.clone(), plain.clone()]);

        assert_eq!(filtered, vec![referenced]);
        assert_eq!(discarded, vec![plain]);
    }

    #[test]
    fn test_filter_contained_matches_does_not_prefer_non_license_filename_reference() {
        let mut referenced = create_test_match(
            "ruby_or_gpl_1.RULE",
            1,
            1,
            MatchScore::from_percentage(86.83),
            86.83,
            100,
        );
        referenced.license_expression = "ruby OR gpl-2.0".to_string();
        referenced.license_expression_spdx = Some("Ruby OR GPL-2.0-only".to_string());
        referenced.matched_length = 323;
        referenced.coordinates = MatchCoordinates::query_region(PositionSpan::range(1, 324));
        referenced.referenced_filenames = Some(vec!["COPYING.txt".to_string()]);

        let mut plain = create_test_match(
            "ruby_1.RULE",
            1,
            1,
            MatchScore::from_percentage(95.0),
            95.0,
            100,
        );
        plain.license_expression = "ruby".to_string();
        plain.license_expression_spdx = Some("Ruby".to_string());
        plain.matched_length = 323;
        plain.coordinates = MatchCoordinates::query_region(PositionSpan::range(1, 324));

        let (filtered, discarded) = filter_contained_matches(&[referenced, plain.clone()]);

        assert_eq!(filtered, vec![plain]);
        assert_eq!(discarded.len(), 1);
    }

    #[test]
    fn test_filter_overlapping_matches_empty() {
        let index = LicenseIndex::with_legalese_count(10);
        let matches: Vec<LicenseMatch> = vec![];

        let (kept, discarded) = filter_overlapping_matches(matches, &index);

        assert_eq!(kept.len(), 0);
        assert_eq!(discarded.len(), 0);
    }

    #[test]
    fn test_filter_overlapping_matches_single() {
        let index = LicenseIndex::with_legalese_count(10);
        let matches = vec![create_test_match(
            "#1",
            1,
            10,
            MatchScore::from_percentage(0.9),
            90.0,
            100,
        )];

        let (kept, discarded) = filter_overlapping_matches(matches, &index);

        assert_eq!(kept.len(), 1);
        assert_eq!(discarded.len(), 0);
    }

    #[test]
    fn test_filter_overlapping_matches_non_overlapping() {
        let index = LicenseIndex::with_legalese_count(10);
        let matches = vec![
            create_test_match("#1", 1, 10, MatchScore::from_percentage(0.9), 90.0, 100),
            create_test_match("#2", 20, 30, MatchScore::from_percentage(0.85), 85.0, 100),
        ];

        let (kept, discarded) = filter_overlapping_matches(matches, &index);

        assert_eq!(kept.len(), 2);
        assert_eq!(discarded.len(), 0);
    }

    #[test]
    fn test_filter_overlapping_matches_extra_large_discard_shorter() {
        let index = LicenseIndex::with_legalese_count(10);
        let mut m1 = create_test_match("#1", 1, 100, MatchScore::from_percentage(0.9), 90.0, 100);
        m1.matched_length = 100;
        m1.coordinates = MatchCoordinates::query_region(PositionSpan::range(1, 101));
        let mut m2 = create_test_match("#2", 5, 100, MatchScore::from_percentage(0.85), 85.0, 100);
        m2.matched_length = 10;
        m2.coordinates = MatchCoordinates::query_region(PositionSpan::range(5, 101));

        let matches = vec![m1, m2];

        let (kept, discarded) = filter_overlapping_matches(matches, &index);

        assert_eq!(kept.len(), 1);
        assert_eq!(discarded.len(), 1);
        assert_eq!(kept[0].matched_length, 100);
    }

    #[test]
    fn test_filter_overlapping_matches_large_with_hilen() {
        let index = LicenseIndex::with_legalese_count(10);
        let mut m1 = create_test_match("#1", 1, 100, MatchScore::from_percentage(0.9), 90.0, 100);
        m1.matched_length = 100;
        m1.coordinates = MatchCoordinates::query_region(PositionSpan::range(1, 101));
        let mut m2 = create_test_match("#2", 30, 100, MatchScore::from_percentage(0.85), 85.0, 100);
        m2.matched_length = 10;
        m2.coordinates = MatchCoordinates::query_region(PositionSpan::range(30, 101));

        let matches = vec![m1, m2];

        let (kept, discarded) = filter_overlapping_matches(matches, &index);

        assert_eq!(kept.len(), 1);
        assert_eq!(discarded.len(), 1);
    }

    #[test]
    fn test_filter_overlapping_matches_false_positive_skip() {
        let mut index = LicenseIndex::with_legalese_count(10);
        let _ = index.false_positive_rids.insert(1);
        let _ = index.false_positive_rids.insert(2);

        let mut m1 = create_test_match("#1", 1, 20, MatchScore::from_percentage(0.9), 90.0, 100);
        m1.matched_length = 100;
        let mut m2 = create_test_match("#2", 10, 30, MatchScore::from_percentage(0.85), 85.0, 100);
        m2.matched_length = 100;

        let matches = vec![m1, m2];

        let (kept, discarded) = filter_overlapping_matches(matches, &index);

        assert_eq!(kept.len(), 2);
        assert_eq!(discarded.len(), 0);
    }

    #[test]
    fn test_filter_overlapping_matches_sandwich_detection() {
        let index = LicenseIndex::with_legalese_count(10);

        let mut prev = create_test_match("#1", 1, 10, MatchScore::from_percentage(0.9), 90.0, 100);
        prev.matched_length = 100;
        let mut current =
            create_test_match("#2", 5, 15, MatchScore::from_percentage(0.85), 85.0, 100);
        current.matched_length = 50;
        let mut next = create_test_match("#3", 12, 25, MatchScore::from_percentage(0.8), 80.0, 100);
        next.matched_length = 100;

        let matches = vec![prev, current, next];

        let (kept, discarded) = filter_overlapping_matches(matches, &index);

        assert!(kept.len() >= 2);
        assert!(!discarded.is_empty() || kept.len() == 3);
    }

    #[test]
    fn test_filter_overlapping_matches_sorting_order() {
        let index = LicenseIndex::with_legalese_count(10);

        let m1 = create_test_match("#1", 25, 35, MatchScore::from_percentage(0.9), 90.0, 100);
        let m2 = create_test_match("#2", 1, 10, MatchScore::from_percentage(0.85), 85.0, 100);
        let m3 = create_test_match("#3", 40, 50, MatchScore::from_percentage(0.8), 80.0, 100);

        let matches = vec![m1, m2, m3];

        let (kept, _) = filter_overlapping_matches(matches, &index);

        assert_eq!(kept.len(), 3);
        assert_eq!(kept[0].start_line, LineNumber::ONE);
        assert_eq!(kept[1].start_line, LineNumber::new(25).expect("valid"));
        assert_eq!(kept[2].start_line, LineNumber::new(40).expect("valid"));
    }

    #[test]
    fn test_filter_overlapping_matches_partial_overlap_no_filter() {
        let index = LicenseIndex::with_legalese_count(10);

        let mut m1 = create_test_match("#1", 1, 20, MatchScore::from_percentage(0.9), 90.0, 100);
        m1.matched_length = 200;
        let mut m2 = create_test_match("#2", 15, 35, MatchScore::from_percentage(0.85), 85.0, 100);
        m2.matched_length = 150;

        let matches = vec![m1, m2];

        let (kept, discarded) = filter_overlapping_matches(matches, &index);

        assert_eq!(kept.len(), 2);
        assert_eq!(discarded.len(), 0);
    }

    #[test]
    fn test_filter_overlapping_matches_surround_check() {
        let index = LicenseIndex::with_legalese_count(10);

        let mut outer =
            create_test_match("#1", 1, 100, MatchScore::from_percentage(0.9), 90.0, 100);
        outer.matched_length = 500;
        outer.coordinates = MatchCoordinates::query_region(PositionSpan::range(1, 101));
        let mut inner =
            create_test_match("#2", 20, 30, MatchScore::from_percentage(0.85), 85.0, 100);
        inner.matched_length = 50;
        inner.coordinates = MatchCoordinates::query_region(PositionSpan::range(20, 31));

        let matches = vec![outer, inner];

        let (kept, discarded) = filter_overlapping_matches(matches, &index);

        assert_eq!(kept.len(), 1);
        assert_eq!(discarded.len(), 1);
        assert!(kept[0].rule_identifier == "#1" || kept[0].matched_length == 500);
    }

    #[test]
    fn test_calculate_overlap_no_overlap() {
        let mut m1 = create_test_match("#1", 1, 10, MatchScore::from_percentage(0.9), 90.0, 100);
        m1.coordinates = MatchCoordinates::query_region(PositionSpan::range(1, 11));
        let mut m2 = create_test_match("#2", 20, 30, MatchScore::from_percentage(0.85), 85.0, 100);
        m2.coordinates = MatchCoordinates::query_region(PositionSpan::range(20, 31));

        assert_eq!(m1.qoverlap(&m2), 0);
        assert_eq!(m2.qoverlap(&m1), 0);
    }

    #[test]
    fn test_calculate_overlap_partial() {
        let mut m1 = create_test_match("#1", 1, 10, MatchScore::from_percentage(0.9), 90.0, 100);
        m1.coordinates = MatchCoordinates::query_region(PositionSpan::range(1, 11));
        let mut m2 = create_test_match("#2", 5, 15, MatchScore::from_percentage(0.85), 85.0, 100);
        m2.coordinates = MatchCoordinates::query_region(PositionSpan::range(5, 16));

        assert_eq!(m1.qoverlap(&m2), 6);
        assert_eq!(m2.qoverlap(&m1), 6);
    }

    #[test]
    fn test_calculate_overlap_contained() {
        let mut m1 = create_test_match("#1", 1, 20, MatchScore::from_percentage(0.9), 90.0, 100);
        m1.coordinates = MatchCoordinates::query_region(PositionSpan::range(1, 21));
        let mut m2 = create_test_match("#2", 5, 15, MatchScore::from_percentage(0.85), 85.0, 100);
        m2.coordinates = MatchCoordinates::query_region(PositionSpan::range(5, 16));

        assert_eq!(m1.qoverlap(&m2), 11);
        assert_eq!(m2.qoverlap(&m1), 11);
    }

    #[test]
    fn test_calculate_overlap_identical() {
        let mut m1 = create_test_match("#1", 1, 10, MatchScore::from_percentage(0.9), 90.0, 100);
        m1.coordinates = MatchCoordinates::query_region(PositionSpan::range(1, 11));
        let mut m2 = create_test_match("#2", 1, 10, MatchScore::from_percentage(0.85), 85.0, 100);
        m2.coordinates = MatchCoordinates::query_region(PositionSpan::range(1, 11));

        assert_eq!(m1.qoverlap(&m2), 10);
    }

    #[test]
    fn test_calculate_overlap_adjacent() {
        let mut m1 = create_test_match("#1", 1, 10, MatchScore::from_percentage(0.9), 90.0, 100);
        m1.coordinates = MatchCoordinates::query_region(PositionSpan::range(1, 11));
        let mut m2 = create_test_match("#2", 11, 20, MatchScore::from_percentage(0.85), 85.0, 100);
        m2.coordinates = MatchCoordinates::query_region(PositionSpan::range(11, 21));

        assert_eq!(m1.qoverlap(&m2), 0);
    }

    #[test]
    fn test_restore_non_overlapping_empty_both() {
        let kept: Vec<LicenseMatch> = vec![];
        let discarded: Vec<LicenseMatch> = vec![];

        let (to_keep, to_discard) = restore_non_overlapping(&kept, discarded);

        assert_eq!(to_keep.len(), 0);
        assert_eq!(to_discard.len(), 0);
    }

    #[test]
    fn test_restore_non_overlapping_empty_kept() {
        let kept: Vec<LicenseMatch> = vec![];
        let discarded = vec![
            create_test_match("#1", 1, 10, MatchScore::from_percentage(0.9), 90.0, 100),
            create_test_match("#2", 20, 30, MatchScore::from_percentage(0.85), 85.0, 100),
        ];

        let (to_keep, to_discard) = restore_non_overlapping(&kept, discarded);

        assert_eq!(to_keep.len(), 2);
        assert_eq!(to_discard.len(), 0);
    }

    #[test]
    fn test_restore_non_overlapping_empty_discarded() {
        let kept = vec![create_test_match(
            "#1",
            1,
            10,
            MatchScore::from_percentage(0.9),
            90.0,
            100,
        )];
        let discarded: Vec<LicenseMatch> = vec![];

        let (to_keep, to_discard) = restore_non_overlapping(&kept, discarded);

        assert_eq!(to_keep.len(), 0);
        assert_eq!(to_discard.len(), 0);
    }

    #[test]
    fn test_restore_non_overlapping_non_overlapping_restored() {
        let kept = vec![create_test_match(
            "#1",
            1,
            10,
            MatchScore::from_percentage(0.9),
            90.0,
            100,
        )];
        let discarded = vec![
            create_test_match("#2", 50, 60, MatchScore::from_percentage(0.85), 85.0, 100),
            create_test_match("#3", 100, 110, MatchScore::from_percentage(0.8), 80.0, 100),
        ];

        let (to_keep, to_discard) = restore_non_overlapping(&kept, discarded);

        assert_eq!(to_keep.len(), 2);
        assert_eq!(to_discard.len(), 0);
    }

    #[test]
    fn test_restore_non_overlapping_overlapping_not_restored() {
        let kept = vec![create_test_match(
            "#1",
            1,
            20,
            MatchScore::from_percentage(0.9),
            90.0,
            100,
        )];
        let discarded = vec![
            create_test_match("#2", 5, 15, MatchScore::from_percentage(0.85), 85.0, 100),
            create_test_match("#3", 10, 25, MatchScore::from_percentage(0.8), 80.0, 100),
        ];

        let (to_keep, to_discard) = restore_non_overlapping(&kept, discarded);

        assert_eq!(to_keep.len(), 0);
        assert_eq!(to_discard.len(), 2);
    }

    #[test]
    fn test_restore_non_overlapping_partial_overlap() {
        let kept = vec![create_test_match(
            "#1",
            10,
            20,
            MatchScore::from_percentage(0.9),
            90.0,
            100,
        )];
        let discarded = vec![
            create_test_match("#2", 1, 5, MatchScore::from_percentage(0.85), 85.0, 100),
            create_test_match("#3", 15, 25, MatchScore::from_percentage(0.8), 80.0, 100),
            create_test_match("#4", 50, 60, MatchScore::from_percentage(0.9), 90.0, 100),
        ];

        let (to_keep, to_discard) = restore_non_overlapping(&kept, discarded);

        assert_eq!(to_keep.len(), 2);
        assert_eq!(to_discard.len(), 1);

        let kept_identifiers: Vec<&str> =
            to_keep.iter().map(|m| m.rule_identifier.as_str()).collect();
        assert!(kept_identifiers.contains(&"#2"));
        assert!(kept_identifiers.contains(&"#4"));

        assert_eq!(to_discard[0].rule_identifier, "#3");
    }

    #[test]
    fn test_restore_non_overlapping_multiple_kept() {
        let kept = vec![
            create_test_match("#1", 1, 10, MatchScore::from_percentage(0.9), 90.0, 100),
            create_test_match("#2", 30, 40, MatchScore::from_percentage(0.85), 85.0, 100),
        ];
        let discarded = vec![
            create_test_match("#3", 15, 20, MatchScore::from_percentage(0.8), 80.0, 100),
            create_test_match("#4", 5, 15, MatchScore::from_percentage(0.9), 90.0, 100),
            create_test_match("#5", 50, 60, MatchScore::from_percentage(0.9), 90.0, 100),
        ];

        let (to_keep, to_discard) = restore_non_overlapping(&kept, discarded);

        assert_eq!(to_keep.len(), 2);
        assert_eq!(to_discard.len(), 1);

        let kept_identifiers: Vec<&str> =
            to_keep.iter().map(|m| m.rule_identifier.as_str()).collect();
        assert!(kept_identifiers.contains(&"#3"));
        assert!(kept_identifiers.contains(&"#5"));

        assert_eq!(to_discard[0].rule_identifier, "#4");
    }

    #[test]
    fn test_restore_non_overlapping_merges_discarded() {
        let kept = vec![create_test_match(
            "#1",
            1,
            10,
            MatchScore::from_percentage(0.9),
            100.0,
            100,
        )];
        let mut m1 = create_test_match("#2", 50, 60, MatchScore::from_percentage(0.85), 100.0, 100);
        m1.rule_length = 100;
        m1.rule_start_token = 0;
        m1.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(50, 61),
            PositionSpan::range(0, 11),
            PositionSpan::empty(),
        );
        let mut m2 = create_test_match("#2", 55, 65, MatchScore::from_percentage(0.8), 100.0, 100);
        m2.rule_length = 100;
        m2.rule_start_token = 5;
        m2.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(55, 66),
            PositionSpan::range(5, 16),
            PositionSpan::empty(),
        );

        let discarded = vec![m1, m2];

        let (to_keep, _to_discard) = restore_non_overlapping(&kept, discarded);

        assert_eq!(to_keep.len(), 1);
        assert_eq!(to_keep[0].rule_identifier, "#2");
        assert_eq!(to_keep[0].start_line, LineNumber::new(50).expect("valid"));
        assert_eq!(to_keep[0].end_line, LineNumber::new(65).expect("valid"));
    }

    #[test]
    fn test_restore_non_overlapping_adjacent_not_overlapping() {
        let kept = vec![create_test_match(
            "#1",
            1,
            10,
            MatchScore::from_percentage(0.9),
            90.0,
            100,
        )];
        let discarded = vec![create_test_match(
            "#2",
            11,
            20,
            MatchScore::from_percentage(0.85),
            85.0,
            100,
        )];

        let (to_keep, to_discard) = restore_non_overlapping(&kept, discarded);

        assert_eq!(to_keep.len(), 1);
        assert_eq!(to_discard.len(), 0);
    }

    #[test]
    fn test_restore_non_overlapping_touching_is_overlapping() {
        let kept = vec![create_test_match(
            "#1",
            1,
            10,
            MatchScore::from_percentage(0.9),
            90.0,
            100,
        )];
        let discarded = vec![create_test_match(
            "#2",
            10,
            20,
            MatchScore::from_percentage(0.85),
            85.0,
            100,
        )];

        let (to_keep, to_discard) = restore_non_overlapping(&kept, discarded);

        assert_eq!(to_keep.len(), 0);
        assert_eq!(to_discard.len(), 1);
    }

    #[test]
    fn test_filter_overlapping_matches_prefers_higher_coverage() {
        let index = LicenseIndex::with_legalese_count(10);

        let mut m1 = create_test_match(
            "gfdl-1.1_13.RULE",
            1,
            10,
            MatchScore::from_percentage(78.7),
            78.7,
            100,
        );
        m1.start_token = 5;
        m1.end_token = 77;
        m1.matched_length = 48;
        m1.matcher = crate::license_detection::models::MatcherKind::Seq;
        m1.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(5, 77),
            PositionSpan::empty(),
            PositionSpan::range(0, 14),
        );

        let mut m2 = create_test_match(
            "gfdl-1.1-plus_5.RULE",
            1,
            10,
            MatchScore::from_percentage(68.6),
            68.6,
            100,
        );
        m2.start_token = 5;
        m2.end_token = 77;
        m2.matched_length = 48;
        m2.matcher = crate::license_detection::models::MatcherKind::Seq;
        m2.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(5, 77),
            PositionSpan::empty(),
            PositionSpan::range(0, 14),
        );

        let matches = vec![m1, m2];
        let (kept, _discarded) = filter_overlapping_matches(matches, &index);

        // Should keep the match with higher coverage (gfdl-1.1 at 78.7%)
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].rule_identifier, "gfdl-1.1_13.RULE");
        assert!(kept[0].match_coverage > 70.0);
    }

    #[test]
    fn test_filter_overlapping_matches_prefers_reference_rule_over_plain_overlap() {
        let index = LicenseIndex::with_legalese_count(10);

        let mut referenced = create_test_match(
            "gpl-1.0-plus_or_mit_2.RULE",
            1,
            1,
            MatchScore::from_percentage(50.75),
            50.75,
            100,
        );
        referenced.license_expression = "gpl-1.0-plus OR mit".to_string();
        referenced.license_expression_spdx = Some("GPL-1.0-or-later OR MIT".to_string());
        referenced.matched_length = 34;
        referenced.matcher = crate::license_detection::models::MatcherKind::Seq;
        referenced.coordinates = MatchCoordinates::query_region(PositionSpan::range(2166, 2200));
        referenced.referenced_filenames = Some(vec!["LICENSE".to_string()]);

        let mut plain = create_test_match(
            "gpl-2.0_290.RULE",
            1,
            1,
            MatchScore::from_percentage(60.71),
            60.71,
            100,
        );
        plain.license_expression = "gpl-2.0".to_string();
        plain.license_expression_spdx = Some("GPL-2.0-only".to_string());
        plain.matched_length = 34;
        plain.matcher = crate::license_detection::models::MatcherKind::Seq;
        plain.coordinates = MatchCoordinates::query_region(PositionSpan::range(2166, 2200));

        let (kept, discarded) =
            filter_overlapping_matches(vec![referenced.clone(), plain.clone()], &index);

        assert_eq!(kept, vec![referenced]);
        assert_eq!(discarded, vec![plain]);
    }

    #[test]
    fn test_filter_overlapping_matches_keeps_exact_match_between_weak_seq_wrappers() {
        let index = LicenseIndex::with_legalese_count(10);

        let mut left =
            create_test_match("bsd-new_and_lgpl-2.0_1.RULE", 9227, 9227, 4.82, 4.82, 100);
        left.matcher = crate::license_detection::models::MatcherKind::Seq;
        left.license_expression = "bsd-new AND lgpl-2.0".to_string();
        left.license_expression_spdx = Some("BSD-3-Clause AND LGPL-2.0-only".to_string());
        left.start_token = 100;
        left.end_token = 108;
        left.matched_length = 8;
        left.coordinates = MatchCoordinates::query_region(PositionSpan::range(100, 108));

        let mut exact = create_test_match("lgpl-2.1-plus_161.RULE", 9227, 9227, 100.0, 100.0, 100);
        exact.matcher = crate::license_detection::models::MatcherKind::Aho;
        exact.license_expression = "lgpl-2.1-plus".to_string();
        exact.license_expression_spdx = Some("LGPL-2.1-or-later".to_string());
        exact.start_token = 102;
        exact.end_token = 110;
        exact.matched_length = 8;
        exact.coordinates = MatchCoordinates::query_region(PositionSpan::range(102, 110));

        let mut right = create_test_match(
            "mpl-1.1_or_lgpl-2.1-plus_or_apache-2.0_5.RULE",
            9227,
            9227,
            4.76,
            4.76,
            100,
        );
        right.matcher = crate::license_detection::models::MatcherKind::Seq;
        right.license_expression = "mpl-1.1 OR lgpl-2.1-plus OR apache-2.0".to_string();
        right.license_expression_spdx =
            Some("MPL-1.1 OR LGPL-2.1-or-later OR Apache-2.0".to_string());
        right.start_token = 101;
        right.end_token = 109;
        right.matched_length = 8;
        right.coordinates = MatchCoordinates::query_region(PositionSpan::range(101, 109));

        let (kept, discarded) =
            filter_overlapping_matches(vec![left.clone(), exact.clone(), right.clone()], &index);

        assert!(
            kept.iter()
                .any(|m| m.rule_identifier == exact.rule_identifier),
            "kept: {:?}, discarded: {:?}",
            kept.iter().map(|m| &m.rule_identifier).collect::<Vec<_>>(),
            discarded
                .iter()
                .map(|m| &m.rule_identifier)
                .collect::<Vec<_>>()
        );
    }
}
