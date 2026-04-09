//! Match refinement - merge, filter, and finalize license matches.
//!
//! This module implements the final phase of license matching where raw matches
//! from all strategies are combined, refined, and finalized.
//!
//! Based on the Python ScanCode Toolkit implementation at:
//! reference/scancode-toolkit/src/licensedcode/match.py

mod false_positive;
pub(crate) mod filter_low_quality;
mod handle_overlaps;
mod merge;

use crate::license_detection::index::LicenseIndex;
use crate::license_detection::models::{LicenseMatch, MatcherKind};
use crate::license_detection::query::Query;

// Internal use only
use filter_low_quality::{
    filter_below_rule_minimum_coverage, filter_false_positive_matches,
    filter_invalid_matches_to_single_word_gibberish, filter_matches_missing_required_phrases,
    filter_matches_to_spurious_single_token, filter_short_matches_scattered_on_too_many_lines,
    filter_spurious_matches, filter_too_short_matches,
};
use merge::{filter_license_references_with_text_match, update_match_scores};

// Re-export for crate-internal use (debug_pipeline feature)
pub use handle_overlaps::{
    filter_contained_matches, filter_overlapping_matches, restore_non_overlapping,
};
pub use merge::merge_overlapping_matches;

// Public API re-exports for investigation tests
pub use false_positive::filter_false_positive_license_lists_matches;

const SMALL_RULE: usize = 15;

/// Filter unknown matches contained within good matches' qregion.
///
/// Unknown license matches that are fully contained within the qregion
/// (token span from start_token to end_token) of a known good match
/// should be discarded as they are redundant.
///
/// # Arguments
/// * `unknown_matches` - Slice of unknown license matches to filter
/// * `good_matches` - Slice of known good matches to check containment against
///
/// # Returns
/// Vector of unknown LicenseMatch with contained matches removed
///
/// Based on Python: `filter_invalid_contained_unknown_matches()` (match.py:1904-1926)
pub fn filter_invalid_contained_unknown_matches(
    unknown_matches: &[LicenseMatch],
    good_matches: &[LicenseMatch],
) -> Vec<LicenseMatch> {
    unknown_matches
        .iter()
        .filter(|unknown| {
            let unknown_start = unknown.start_token;
            let unknown_end = unknown.end_token;

            let is_contained = good_matches
                .iter()
                .any(|good| good.start_token <= unknown_start && good.end_token >= unknown_end);

            !is_contained
        })
        .cloned()
        .collect()
}

/// Split matches into good and weak matches.
///
/// Weak matches are:
/// - Matches to rules with "unknown" in their license expression
/// - Sequence matches with len() <= SMALL_RULE (15) AND coverage <= 25%
///
/// Weak matches are set aside before unknown license matching and reinjected later.
///
/// # Arguments
/// * `matches` - Slice of LicenseMatch to split
///
/// # Returns
/// Tuple of (good_matches, weak_matches)
///
/// Based on Python: `split_weak_matches()` (match.py:1740-1765)
pub fn split_weak_matches(
    index: &LicenseIndex,
    matches: &[LicenseMatch],
) -> (Vec<LicenseMatch>, Vec<LicenseMatch>) {
    let mut good = Vec::new();
    let mut weak = Vec::new();

    for m in matches {
        let is_false_positive = index.false_positive_rids.contains(&m.rid);
        let is_weak = (!is_false_positive && m.has_unknown())
            || (m.matcher == MatcherKind::Seq && m.len() <= SMALL_RULE && m.coverage() <= 25.0);

        if is_weak {
            weak.push(m.clone());
        } else {
            good.push(m.clone());
        }
    }

    (good, weak)
}

/// Main refinement function - applies all refinement operations to match results.
///
/// This is the main entry point for Phase 4.6 match refinement. It applies
/// filters in the same order as Python's refine_matches():
///
/// 1. Filter matches missing required phrases
/// 2. Filter spurious matches (low density)
/// 3. Filter below rule minimum coverage
/// 4. Filter spurious single-token matches
/// 5. Filter too short matches
/// 6. Filter scattered short matches
/// 7. Filter invalid single-word gibberish (binary files)
/// 8. Merge overlapping/adjacent matches
/// 9. Filter contained matches
/// 10. Filter overlapping matches
/// 11. Restore non-overlapping discarded matches
/// 12. Filter false positive matches
/// 13. Filter false positive license list matches
/// 14. Update match scores
///
/// The operations are applied in sequence to produce final refined matches.
///
/// # Arguments
/// * `index` - LicenseIndex containing false_positive_rids and rules_by_rid
/// * `matches` - Vector of raw LicenseMatch from all strategies
/// * `query` - Query object for spurious/gibberish filtering
///
/// # Returns
/// Vector of refined LicenseMatch ready for detection assembly
///
/// Based on Python: `refine_matches()` (lines 2691-2833)
pub fn refine_matches(
    index: &LicenseIndex,
    matches: Vec<LicenseMatch>,
    query: &Query,
) -> Vec<LicenseMatch> {
    refine_matches_internal(index, matches, query, true)
}

/// Initial refinement without false positive filtering.
///
/// Used before split_weak_matches and unknown detection.
/// This matches Python's refine_matches with filter_false_positive=False.
///
/// Based on Python: `refine_matches()` at index.py:1073-1080
pub fn refine_matches_without_false_positive_filter(
    index: &LicenseIndex,
    matches: Vec<LicenseMatch>,
    query: &Query,
) -> Vec<LicenseMatch> {
    refine_matches_internal(index, matches, query, false)
}

/// Refine Aho-Corasick matches.
///
/// This matches Python's `get_exact_matches()` which calls `refine_matches()` with `merge=False`.
/// Unlike full refinement, this:
/// - Skips initial merge (merge=False)
/// - Applies required phrase filtering
/// - Applies all quality filters
/// - Applies containment and overlap filtering with restore
/// - Skips final merge (merge=False)
///
/// Based on Python: `get_exact_matches()` at index.py:691-696
pub fn refine_aho_matches(
    index: &LicenseIndex,
    matches: Vec<LicenseMatch>,
    query: &Query,
) -> Vec<LicenseMatch> {
    if matches.is_empty() {
        return Vec::new();
    }

    let (with_required_phrases, _missing_phrases) =
        filter_matches_missing_required_phrases(index, &matches, query);

    let non_spurious = filter_spurious_matches(&with_required_phrases, query);

    let above_min_cov = filter_below_rule_minimum_coverage(index, &non_spurious);

    let non_single_spurious = filter_matches_to_spurious_single_token(&above_min_cov, query, 5);

    let non_short = filter_too_short_matches(index, &non_single_spurious);

    let non_scattered = filter_short_matches_scattered_on_too_many_lines(index, &non_short);

    let non_gibberish =
        filter_invalid_matches_to_single_word_gibberish(index, &non_scattered, query);

    let merged_again = merge_overlapping_matches(&non_gibberish);

    let merged_again = filter_binary_low_coverage_same_expression_seq_bridges(merged_again, query);

    let (non_contained, discarded_contained) = filter_contained_matches(&merged_again);

    let (kept, discarded_overlapping) = filter_overlapping_matches(non_contained, index);

    let mut matches_after_first_restore = kept.clone();

    if !discarded_contained.is_empty() {
        let (restored_contained, _) = restore_non_overlapping(&kept, discarded_contained);
        matches_after_first_restore.extend(restored_contained);
    }

    let mut final_matches = matches_after_first_restore.clone();

    if !discarded_overlapping.is_empty() {
        let (restored_overlapping, _) =
            restore_non_overlapping(&matches_after_first_restore, discarded_overlapping);
        final_matches.extend(restored_overlapping);
    }

    let (non_contained_final, _) = filter_contained_matches(&final_matches);

    let filtered_refs = filter_license_references_with_text_match(&non_contained_final);

    let mut final_scored = filtered_refs;
    update_match_scores(&mut final_scored, query);

    final_scored
}

fn refine_matches_internal(
    index: &LicenseIndex,
    matches: Vec<LicenseMatch>,
    query: &Query,
    filter_false_positive: bool,
) -> Vec<LicenseMatch> {
    if matches.is_empty() {
        return Vec::new();
    }

    let merged = merge_overlapping_matches(&matches);

    let (with_required_phrases, _missing_phrases) =
        filter_matches_missing_required_phrases(index, &merged, query);

    let non_spurious = filter_spurious_matches(&with_required_phrases, query);

    let above_min_cov = filter_below_rule_minimum_coverage(index, &non_spurious);

    let non_single_spurious = filter_matches_to_spurious_single_token(&above_min_cov, query, 5);

    let non_short = filter_too_short_matches(index, &non_single_spurious);

    let non_scattered = filter_short_matches_scattered_on_too_many_lines(index, &non_short);

    let non_gibberish =
        filter_invalid_matches_to_single_word_gibberish(index, &non_scattered, query);

    let merged_again = merge_overlapping_matches(&non_gibberish);

    let merged_again = filter_binary_low_coverage_same_expression_seq_bridges(merged_again, query);

    let (non_contained, discarded_contained) = filter_contained_matches(&merged_again);

    let (kept, discarded_overlapping) = filter_overlapping_matches(non_contained, index);

    let mut matches_after_first_restore = kept.clone();

    if !discarded_contained.is_empty() {
        let (restored_contained, _) = restore_non_overlapping(&kept, discarded_contained);
        matches_after_first_restore.extend(restored_contained);
    }

    let mut final_matches = matches_after_first_restore.clone();

    if !discarded_overlapping.is_empty() {
        let (restored_overlapping, _) =
            restore_non_overlapping(&matches_after_first_restore, discarded_overlapping);
        final_matches.extend(restored_overlapping);
    }

    let (non_contained_final, _) = filter_contained_matches(&final_matches);

    let result = if filter_false_positive {
        let non_fp = filter_false_positive_matches(index, &non_contained_final);
        let (kept, _discarded) = filter_false_positive_license_lists_matches(non_fp);
        kept
    } else {
        non_contained_final
    };

    let merged_final = merge_overlapping_matches(&result);

    let filtered_refs = filter_license_references_with_text_match(&merged_final);

    let mut final_scored = filtered_refs;
    update_match_scores(&mut final_scored, query);

    final_scored
}

fn filter_binary_low_coverage_same_expression_seq_bridges(
    matches: Vec<LicenseMatch>,
    query: &Query,
) -> Vec<LicenseMatch> {
    if !query.is_binary {
        return matches;
    }

    matches
        .iter()
        .filter(|m| {
            if m.matcher != MatcherKind::Seq || m.coverage() >= 90.0 {
                return true;
            }

            !matches.iter().any(|other| {
                other.matcher == MatcherKind::Aho
                    && other.coverage() == 100.0
                    && other.license_expression == m.license_expression
                    && other.qoverlap(m) > 0
                    && !m.qcontains(other)
            })
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license_detection::models::MatchCoordinates;
    use crate::license_detection::models::position_span::PositionSpan;
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
            coordinates: MatchCoordinates::query_region(PositionSpan::range(
                start_line,
                end_line + 1,
            )),
            candidate_resemblance: 0.0,
            candidate_containment: 0.0,
        }
    }

    #[test]
    fn test_refine_matches_full_pipeline() {
        let mut index = LicenseIndex::with_legalese_count(10);
        let _ = index.false_positive_rids.insert(99);

        let mut m1 = create_test_match("#1", 1, 10, 0.5, 100.0, 100);
        m1.rule_length = 100;
        m1.rule_start_token = 0;
        m1.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(1, 11),
            PositionSpan::range(0, 10),
            PositionSpan::empty(),
        );
        let mut m2 = create_test_match("#1", 5, 15, 0.5, 100.0, 100);
        m2.rule_length = 100;
        m2.rule_start_token = 4;
        m2.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(5, 16),
            PositionSpan::range(4, 15),
            PositionSpan::empty(),
        );
        let mut m3 = create_test_match("#2", 20, 25, 0.5, 100.0, 80);
        m3.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(20, 26),
            PositionSpan::range(0, 6),
            PositionSpan::empty(),
        );
        let mut m4 = create_test_match("#99", 30, 35, 0.5, 100.0, 100);
        m4.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(30, 36),
            PositionSpan::range(0, 6),
            PositionSpan::empty(),
        );

        let matches = vec![m1, m2, m3, m4];

        let query = Query::from_extracted_text("test text", &index, false).unwrap();
        let refined = refine_matches(&index, matches, &query);

        assert_eq!(refined.len(), 2);

        let rule1_match = refined.iter().find(|m| m.rule_identifier == "#1").unwrap();
        assert_eq!(rule1_match.start_line, LineNumber::ONE);
        assert_eq!(rule1_match.end_line, LineNumber::new(15).unwrap());

        let rule2_match = refined.iter().find(|m| m.rule_identifier == "#2").unwrap();
        assert_eq!(rule2_match.score, 80.0);
    }

    #[test]
    fn test_refine_matches_empty() {
        let index = LicenseIndex::with_legalese_count(10);
        let matches: Vec<LicenseMatch> = vec![];
        let query = Query::from_extracted_text("", &index, false).unwrap();

        let refined = refine_matches(&index, matches, &query);

        assert_eq!(refined.len(), 0);
    }

    #[test]
    fn test_refine_matches_single() {
        let index = LicenseIndex::with_legalese_count(10);
        let matches = vec![create_test_match("#1", 1, 10, 0.5, 100.0, 100)];
        let query = Query::from_extracted_text("test text", &index, false).unwrap();

        let refined = refine_matches(&index, matches, &query);

        assert_eq!(refined.len(), 1);
        assert_eq!(refined[0].score, 100.0);
    }

    #[test]
    fn test_refine_matches_no_merging_needed() {
        let index = LicenseIndex::with_legalese_count(10);

        let mut m1 = create_test_match("#1", 1, 10, 0.9, 90.0, 100);
        m1.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(1, 11),
            PositionSpan::range(0, 10),
            PositionSpan::empty(),
        );
        let mut m2 = create_test_match("#2", 20, 30, 0.85, 85.0, 100);
        m2.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(20, 31),
            PositionSpan::range(0, 11),
            PositionSpan::empty(),
        );

        let matches = vec![m1, m2];

        let query = Query::from_extracted_text("test text", &index, false).unwrap();

        let refined = refine_matches(&index, matches, &query);

        assert_eq!(refined.len(), 2);
    }

    #[test]
    fn test_filter_binary_low_coverage_same_expression_seq_bridges_drops_seq_bridge() {
        let index = LicenseIndex::with_legalese_count(10);
        let query = Query::from_extracted_text("binary strings", &index, true).unwrap();

        let mut exact = create_test_match("#1", 140, 140, 100.0, 100.0, 100);
        exact.license_expression = "bsd-new".to_string();
        exact.matcher = MatcherKind::Aho;
        exact.start_token = 10;
        exact.end_token = 16;
        exact.matched_length = 6;
        exact.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(10, 16),
            PositionSpan::empty(),
            PositionSpan::empty(),
        );

        let mut seq = create_test_match("#2", 140, 141, 10.0, 52.9, 100);
        seq.license_expression = "bsd-new".to_string();
        seq.matcher = MatcherKind::Seq;
        seq.start_token = 10;
        seq.end_token = 18;
        seq.matched_length = 7;
        seq.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::from_positions(vec![10, 11, 12, 13, 14, 16, 17]),
            PositionSpan::empty(),
            PositionSpan::empty(),
        );

        let filtered = filter_binary_low_coverage_same_expression_seq_bridges(
            vec![seq.clone(), exact.clone()],
            &query,
        );

        assert_eq!(filtered, vec![exact]);
    }

    #[test]
    fn test_refine_aho_matches_restores_inner_merge_before_containment() {
        let index = LicenseIndex::with_legalese_count(10);

        let mut first = create_test_match("#1", 1, 10, 0.9, 50.0, 100);
        first.rule_length = 20;
        first.rule_start_token = 0;
        first.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(1, 11),
            PositionSpan::range(0, 10),
            PositionSpan::empty(),
        );

        let mut second = create_test_match("#1", 11, 20, 0.85, 50.0, 100);
        second.rule_length = 20;
        second.rule_start_token = 10;
        second.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(11, 21),
            PositionSpan::range(10, 20),
            PositionSpan::empty(),
        );

        let query = Query::from_extracted_text("test text", &index, false).unwrap();
        let refined = refine_aho_matches(&index, vec![first, second], &query);

        assert_eq!(refined.len(), 1);
        assert_eq!(refined[0].rule_identifier, "#1");
        assert_eq!(refined[0].start_line, LineNumber::ONE);
        assert_eq!(refined[0].end_line, LineNumber::new(20).unwrap());
    }

    #[test]
    fn test_refine_matches_pipeline_preserves_non_overlapping_different_rules() {
        let index = LicenseIndex::with_legalese_count(10);

        let mut m1 = create_test_match("#1", 1, 10, 0.9, 90.0, 100);
        m1.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(1, 11),
            PositionSpan::range(0, 10),
            PositionSpan::empty(),
        );
        let mut m2 = create_test_match("#2", 20, 30, 0.85, 85.0, 100);
        m2.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(20, 31),
            PositionSpan::range(0, 11),
            PositionSpan::empty(),
        );
        let mut m3 = create_test_match("#3", 40, 50, 0.8, 80.0, 100);
        m3.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(40, 51),
            PositionSpan::range(0, 11),
            PositionSpan::empty(),
        );

        let matches = vec![m1, m2, m3];

        let query = Query::from_extracted_text("test text", &index, false).unwrap();
        let refined = refine_matches(&index, matches, &query);

        assert_eq!(refined.len(), 3);
    }

    #[test]
    fn test_refine_matches_complex_scenario() {
        let mut index = LicenseIndex::with_legalese_count(10);
        let _ = index.false_positive_rids.insert(999);

        let mut m1 = create_test_match("#1", 1, 10, 0.7, 100.0, 100);
        m1.matched_length = 100;
        m1.rule_length = 100;
        m1.rule_start_token = 0;
        m1.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(1, 11),
            PositionSpan::range(0, 10),
            PositionSpan::empty(),
        );
        let mut m2 = create_test_match("#1", 8, 15, 0.8, 100.0, 100);
        m2.matched_length = 100;
        m2.rule_length = 100;
        m2.rule_start_token = 7;
        m2.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(8, 16),
            PositionSpan::range(7, 15),
            PositionSpan::empty(),
        );
        let mut m3 = create_test_match("#2", 20, 50, 0.9, 100.0, 100);
        m3.matched_length = 300;
        m3.rule_length = 300;
        m3.rule_start_token = 0;
        m3.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(20, 51),
            PositionSpan::range(0, 31),
            PositionSpan::empty(),
        );
        let mut m4 = create_test_match("#2", 25, 45, 0.85, 100.0, 100);
        m4.matched_length = 150;
        m4.rule_length = 300;
        m4.rule_start_token = 5;
        m4.coordinates = MatchCoordinates::rule_aligned(
            PositionSpan::range(25, 46),
            PositionSpan::range(5, 26),
            PositionSpan::empty(),
        );

        let matches = vec![m1, m2, m3, m4];

        let query = Query::from_extracted_text("test text", &index, false).unwrap();
        let refined = refine_matches(&index, matches, &query);

        assert!(
            refined.len() >= 2,
            "Should have at least 2 matches after refinement"
        );
    }

    #[test]
    fn test_split_weak_matches_has_unknown() {
        let mut m = LicenseMatch {
            license_expression: "unknown".to_string(),
            matcher: crate::license_detection::models::MatcherKind::Hash,
            matched_length: 100,
            match_coverage: 100.0,
            ..LicenseMatch::default()
        };
        m.end_token = 100;
        m.rule_length = 100;

        let index = LicenseIndex::with_legalese_count(10);
        let (good, weak) = split_weak_matches(&index, &[m.clone()]);
        assert!(weak.contains(&m));
        assert!(!good.contains(&m));
    }

    #[test]
    fn test_split_weak_matches_short_seq_low_coverage() {
        let mut m = LicenseMatch {
            license_expression: "mit".to_string(),
            matcher: crate::license_detection::models::MatcherKind::Seq,
            matched_length: 10,
            match_coverage: 20.0,
            ..LicenseMatch::default()
        };
        m.end_token = 10;
        m.rule_length = 50;

        let index = LicenseIndex::with_legalese_count(10);
        let (good, weak) = split_weak_matches(&index, &[m.clone()]);
        assert!(weak.contains(&m));
        assert!(!good.contains(&m));
    }

    #[test]
    fn test_split_weak_matches_keeps_false_positive_unknown_out_of_weak_bucket() {
        let m = LicenseMatch {
            rid: 42,
            license_expression: "unknown".to_string(),
            matcher: crate::license_detection::models::MatcherKind::Aho,
            matched_length: 3,
            rule_length: 3,
            match_coverage: 100.0,
            ..LicenseMatch::default()
        };

        let mut index = LicenseIndex::with_legalese_count(10);
        index.false_positive_rids.insert(42);

        let (good, weak) = split_weak_matches(&index, std::slice::from_ref(&m));
        assert!(good.contains(&m));
        assert!(!weak.contains(&m));
    }

    #[test]
    fn test_split_weak_matches_short_seq_high_coverage() {
        let mut m = LicenseMatch {
            license_expression: "mit".to_string(),
            matcher: crate::license_detection::models::MatcherKind::Seq,
            matched_length: 10,
            match_coverage: 80.0,
            ..LicenseMatch::default()
        };
        m.end_token = 10;
        m.rule_length = 15;

        let index = LicenseIndex::with_legalese_count(10);
        let (good, weak) = split_weak_matches(&index, &[m.clone()]);
        assert!(good.contains(&m));
        assert!(!weak.contains(&m));
    }

    #[test]
    fn test_split_weak_matches_non_seq_short() {
        let mut m = LicenseMatch {
            license_expression: "mit".to_string(),
            matcher: crate::license_detection::models::MatcherKind::Hash,
            matched_length: 10,
            match_coverage: 20.0,
            ..LicenseMatch::default()
        };
        m.end_token = 10;
        m.rule_length = 15;

        let index = LicenseIndex::with_legalese_count(10);
        let (good, weak) = split_weak_matches(&index, &[m.clone()]);
        assert!(good.contains(&m));
        assert!(!weak.contains(&m));
    }

    #[test]
    fn test_split_weak_matches_mixed() {
        let mut good_match = LicenseMatch {
            license_expression: "mit".to_string(),
            matcher: crate::license_detection::models::MatcherKind::Hash,
            matched_length: 50,
            match_coverage: 95.0,
            ..LicenseMatch::default()
        };
        good_match.end_token = 50;
        good_match.rule_length = 50;

        let mut weak_unknown = LicenseMatch {
            license_expression: "unknown".to_string(),
            matcher: crate::license_detection::models::MatcherKind::Unknown,
            matched_length: 30,
            match_coverage: 50.0,
            ..LicenseMatch::default()
        };
        weak_unknown.end_token = 30;
        weak_unknown.rule_length = 30;

        let mut weak_seq = LicenseMatch {
            license_expression: "apache-2.0".to_string(),
            matcher: crate::license_detection::models::MatcherKind::Seq,
            matched_length: 10,
            match_coverage: 20.0,
            ..LicenseMatch::default()
        };
        weak_seq.end_token = 10;
        weak_seq.rule_length = 50;

        let matches = vec![good_match.clone(), weak_unknown.clone(), weak_seq.clone()];
        let index = LicenseIndex::with_legalese_count(10);
        let (good, weak) = split_weak_matches(&index, &matches);

        assert_eq!(good.len(), 1);
        assert_eq!(weak.len(), 2);
        assert!(good.contains(&good_match));
        assert!(weak.contains(&weak_unknown));
        assert!(weak.contains(&weak_seq));
    }
}
