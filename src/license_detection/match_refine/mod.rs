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
pub fn filter_invalid_contained_unknown_matches<'a>(
    unknown_matches: &[LicenseMatch<'a>],
    good_matches: &[LicenseMatch<'a>],
) -> Vec<LicenseMatch<'a>> {
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
pub fn split_weak_matches<'a>(
    matches: &[LicenseMatch<'a>],
) -> (Vec<LicenseMatch<'a>>, Vec<LicenseMatch<'a>>) {
    let mut good = Vec::new();
    let mut weak = Vec::new();

    for m in matches {
        let is_false_positive = m.is_false_positive();
        let is_weak = (!is_false_positive && m.has_unknown())
            || (m.matcher == MatcherKind::Seq && m.len() <= SMALL_RULE && m.match_coverage <= 25.0);

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
/// * `matches` - Vector of raw LicenseMatch from all strategies
/// * `query` - Query object for spurious/gibberish filtering
///
/// # Returns
/// Vector of refined LicenseMatch ready for detection assembly
///
/// Based on Python: `refine_matches()` (lines 2691-2833)
pub fn refine_matches<'a>(
    matches: Vec<LicenseMatch<'a>>,
    query: &Query<'a>,
) -> Vec<LicenseMatch<'a>> {
    refine_matches_internal(matches, query, true)
}

/// Initial refinement without false positive filtering.
///
/// Used before split_weak_matches and unknown detection.
/// This matches Python's refine_matches with filter_false_positive=False.
///
/// Based on Python: `refine_matches()` at index.py:1073-1080
pub fn refine_matches_without_false_positive_filter<'a>(
    matches: Vec<LicenseMatch<'a>>,
    query: &Query<'a>,
) -> Vec<LicenseMatch<'a>> {
    refine_matches_internal(matches, query, false)
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
pub fn refine_aho_matches<'a>(
    _index: &LicenseIndex,
    matches: Vec<LicenseMatch<'a>>,
    query: &Query<'a>,
) -> Vec<LicenseMatch<'a>> {
    if matches.is_empty() {
        return Vec::new();
    }

    let (with_required_phrases, _missing_phrases) =
        filter_matches_missing_required_phrases(&matches, query);

    let non_spurious = filter_spurious_matches(&with_required_phrases, query);

    let above_min_cov = filter_below_rule_minimum_coverage(&non_spurious);

    let non_single_spurious = filter_matches_to_spurious_single_token(&above_min_cov, query, 5);

    let non_short = filter_too_short_matches(&non_single_spurious);

    let non_scattered = filter_short_matches_scattered_on_too_many_lines(&non_short);

    let non_gibberish = filter_invalid_matches_to_single_word_gibberish(&non_scattered, query);

    let merged_again = merge_overlapping_matches(&non_gibberish);

    let merged_again = filter_binary_low_coverage_same_expression_seq_bridges(merged_again, query);

    let (non_contained, discarded_contained) = filter_contained_matches(&merged_again);

    let (kept, discarded_overlapping) = filter_overlapping_matches(non_contained);

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

fn refine_matches_internal<'a>(
    matches: Vec<LicenseMatch<'a>>,
    query: &Query<'a>,
    filter_false_positive: bool,
) -> Vec<LicenseMatch<'a>> {
    if matches.is_empty() {
        return Vec::new();
    }

    let merged = merge_overlapping_matches(&matches);

    let (with_required_phrases, _missing_phrases) =
        filter_matches_missing_required_phrases(&merged, query);

    let non_spurious = filter_spurious_matches(&with_required_phrases, query);

    let above_min_cov = filter_below_rule_minimum_coverage(&non_spurious);

    let non_single_spurious = filter_matches_to_spurious_single_token(&above_min_cov, query, 5);

    let non_short = filter_too_short_matches(&non_single_spurious);

    let non_scattered = filter_short_matches_scattered_on_too_many_lines(&non_short);

    let non_gibberish = filter_invalid_matches_to_single_word_gibberish(&non_scattered, query);

    let merged_again = merge_overlapping_matches(&non_gibberish);

    let merged_again = filter_binary_low_coverage_same_expression_seq_bridges(merged_again, query);

    let (non_contained, discarded_contained) = filter_contained_matches(&merged_again);

    let (kept, discarded_overlapping) = filter_overlapping_matches(non_contained);

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
        let non_fp = filter_false_positive_matches(&non_contained_final);
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

fn filter_binary_low_coverage_same_expression_seq_bridges<'a>(
    matches: Vec<LicenseMatch<'a>>,
    query: &Query<'a>,
) -> Vec<LicenseMatch<'a>> {
    if !query.is_binary {
        return matches;
    }

    matches
        .iter()
        .filter(|m| {
            if m.matcher != MatcherKind::Seq || m.match_coverage >= 90.0 {
                return true;
            }

            !matches.iter().any(|other| {
                other.matcher == MatcherKind::Aho
                    && other.match_coverage >= 100.0
                    && other.license_expression() == m.license_expression()
                    && other.qoverlap(m) > 0
                    && !m.qcontains(other)
            })
        })
        .cloned()
        .collect()
}

#[cfg(test)]
#[allow(dead_code, unused_variables)]
mod tests {
    use super::*;
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

    fn create_false_positive_match(
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
            .is_false_positive(true)
            .build_match()
    }

    #[test]
    fn test_refine_matches_full_pipeline() {
        let mut index = LicenseIndex::with_legalese_count(10);
        let _ = index.false_positive_rids.insert(99);

        let m1 = create_test_match_with_rule_len("#1", 1, 10, 0.5, 100.0, 100, 100, 0);
        let m2 = create_test_match_with_rule_len("#1", 5, 15, 0.5, 100.0, 100, 100, 4);
        let m3 = create_test_match("#2", 20, 25, 0.5, 100.0, 80);
        let m4 = create_false_positive_match("#99", 30, 35, 0.5, 100.0, 100);

        let matches = vec![m1, m2, m3, m4];

        let query = Query::from_extracted_text("test text", &index, false).unwrap();
        let refined = refine_matches(matches, &query);

        assert_eq!(refined.len(), 2);

        let rule1_match = refined
            .iter()
            .find(|m| m.rule_identifier() == "#1")
            .unwrap();
        assert_eq!(rule1_match.start_line, 1);
        assert_eq!(rule1_match.end_line, 15);

        let rule2_match = refined
            .iter()
            .find(|m| m.rule_identifier() == "#2")
            .unwrap();
        assert_eq!(rule2_match.score, 80.0);
    }

    #[test]
    fn test_refine_matches_empty() {
        let index = LicenseIndex::with_legalese_count(10);
        let matches: Vec<LicenseMatch> = vec![];
        let query = Query::from_extracted_text("", &index, false).unwrap();

        let refined = refine_matches(matches, &query);

        assert_eq!(refined.len(), 0);
    }

    #[test]
    fn test_refine_matches_single() {
        let index = LicenseIndex::with_legalese_count(10);
        let matches = vec![create_test_match("#1", 1, 10, 0.5, 100.0, 100)];
        let query = Query::from_extracted_text("test text", &index, false).unwrap();

        let refined = refine_matches(matches, &query);

        assert_eq!(refined.len(), 1);
        assert_eq!(refined[0].score, 100.0);
    }

    #[test]
    fn test_refine_matches_no_merging_needed() {
        let index = LicenseIndex::with_legalese_count(10);

        let matches = vec![
            create_test_match("#1", 1, 10, 0.9, 90.0, 100),
            create_test_match("#2", 20, 30, 0.85, 85.0, 100),
        ];

        let query = Query::from_extracted_text("test text", &index, false).unwrap();

        let refined = refine_matches(matches, &query);

        assert_eq!(refined.len(), 2);
    }

    #[test]
    fn test_filter_binary_low_coverage_same_expression_seq_bridges_drops_seq_bridge() {
        let index = LicenseIndex::with_legalese_count(10);
        let query = Query::from_extracted_text("binary strings", &index, true).unwrap();

        let exact = TestMatchBuilder::default()
            .license_expression("bsd-new")
            .start_line(140)
            .end_line(140)
            .start_token(10)
            .end_token(16)
            .matcher(MatcherKind::Aho)
            .score(100.0)
            .matched_length(6)
            .rule_length(6)
            .match_coverage(100.0)
            .rule_relevance(100)
            .rule_identifier("#1")
            .hilen(50)
            .build_match();

        let seq = TestMatchBuilder::default()
            .license_expression("bsd-new")
            .start_line(140)
            .end_line(141)
            .start_token(10)
            .end_token(18)
            .matcher(MatcherKind::Seq)
            .score(10.0)
            .matched_length(7)
            .rule_length(7)
            .match_coverage(52.9)
            .rule_relevance(100)
            .rule_identifier("#2")
            .hilen(50)
            .qspan_positions(Some(vec![10, 11, 12, 13, 14, 16, 17]))
            .build_match();

        let filtered = filter_binary_low_coverage_same_expression_seq_bridges(
            vec![seq.clone(), exact.clone()],
            &query,
        );

        assert_eq!(filtered, vec![exact]);
    }

    #[test]
    fn test_refine_aho_matches_restores_inner_merge_before_containment() {
        let index = LicenseIndex::with_legalese_count(10);

        let first = create_test_match_with_rule_len("#1", 1, 10, 0.9, 50.0, 100, 20, 0);
        let second = create_test_match_with_rule_len("#1", 11, 20, 0.85, 50.0, 100, 20, 10);

        let query = Query::from_extracted_text("test text", &index, false).unwrap();
        let refined = refine_aho_matches(&index, vec![first, second], &query);

        assert_eq!(refined.len(), 1);
        assert_eq!(refined[0].rule_identifier(), "#1");
        assert_eq!(refined[0].start_line, 1);
        assert_eq!(refined[0].end_line, 20);
    }

    #[test]
    fn test_refine_matches_pipeline_preserves_non_overlapping_different_rules() {
        let index = LicenseIndex::with_legalese_count(10);

        let matches = vec![
            create_test_match("#1", 1, 10, 0.9, 90.0, 100),
            create_test_match("#2", 20, 30, 0.85, 85.0, 100),
            create_test_match("#3", 40, 50, 0.8, 80.0, 100),
        ];

        let query = Query::from_extracted_text("test text", &index, false).unwrap();
        let refined = refine_matches(matches, &query);

        assert_eq!(refined.len(), 3);
    }

    #[test]
    fn test_refine_matches_complex_scenario() {
        let mut index = LicenseIndex::with_legalese_count(10);
        let _ = index.false_positive_rids.insert(999);

        let m1 = TestMatchBuilder::default()
            .license_expression("mit")
            .start_line(1)
            .end_line(10)
            .start_token(1)
            .end_token(11)
            .matcher(MatcherKind::Aho)
            .score(0.7)
            .matched_length(100)
            .rule_length(100)
            .match_coverage(100.0)
            .rule_relevance(100)
            .rule_identifier("#1")
            .hilen(50)
            .rule_start_token(0)
            .build_match();
        let m2 = TestMatchBuilder::default()
            .license_expression("mit")
            .start_line(8)
            .end_line(15)
            .start_token(8)
            .end_token(16)
            .matcher(MatcherKind::Aho)
            .score(0.8)
            .matched_length(100)
            .rule_length(100)
            .match_coverage(100.0)
            .rule_relevance(100)
            .rule_identifier("#1")
            .hilen(50)
            .rule_start_token(7)
            .build_match();
        let m3 = TestMatchBuilder::default()
            .license_expression("mit")
            .start_line(20)
            .end_line(50)
            .start_token(20)
            .end_token(51)
            .matcher(MatcherKind::Aho)
            .score(0.9)
            .matched_length(300)
            .rule_length(300)
            .match_coverage(100.0)
            .rule_relevance(100)
            .rule_identifier("#2")
            .hilen(50)
            .rule_start_token(0)
            .build_match();
        let m4 = TestMatchBuilder::default()
            .license_expression("mit")
            .start_line(25)
            .end_line(45)
            .start_token(25)
            .end_token(46)
            .matcher(MatcherKind::Aho)
            .score(0.85)
            .matched_length(150)
            .rule_length(300)
            .match_coverage(100.0)
            .rule_relevance(100)
            .rule_identifier("#2")
            .hilen(50)
            .rule_start_token(5)
            .build_match();

        let matches = vec![m1, m2, m3, m4];

        let query = Query::from_extracted_text("test text", &index, false).unwrap();
        let refined = refine_matches(matches, &query);

        assert!(
            refined.len() >= 2,
            "Should have at least 2 matches after refinement"
        );
    }

    #[test]
    fn test_split_weak_matches_has_unknown() {
        let m = TestMatchBuilder::default()
            .license_expression("unknown")
            .matcher(MatcherKind::Hash)
            .matched_length(100)
            .match_coverage(100.0)
            .rule_length(100)
            .end_token(100)
            .build_match();

        let _index = LicenseIndex::with_legalese_count(10);
        let (good, weak) = split_weak_matches(std::slice::from_ref(&m));
        assert!(weak.contains(&m));
        assert!(!good.contains(&m));
    }

    #[test]
    fn test_split_weak_matches_short_seq_low_coverage() {
        let m = TestMatchBuilder::default()
            .license_expression("mit")
            .matcher(MatcherKind::Seq)
            .matched_length(10)
            .match_coverage(20.0)
            .rule_length(50)
            .end_token(10)
            .build_match();

        let _index = LicenseIndex::with_legalese_count(10);
        let (good, weak) = split_weak_matches(std::slice::from_ref(&m));
        assert!(weak.contains(&m));
        assert!(!good.contains(&m));
    }

    #[test]
    fn test_split_weak_matches_keeps_false_positive_unknown_out_of_weak_bucket() {
        let m = TestMatchBuilder::default()
            .license_expression("unknown")
            .matcher(MatcherKind::Aho)
            .matched_length(3)
            .rule_length(3)
            .match_coverage(100.0)
            .is_false_positive(true)
            .build_match();

        let _index = LicenseIndex::with_legalese_count(10);

        let (good, weak) = split_weak_matches(std::slice::from_ref(&m));
        assert!(good.contains(&m));
        assert!(!weak.contains(&m));
    }

    #[test]
    fn test_split_weak_matches_short_seq_high_coverage() {
        let m = TestMatchBuilder::default()
            .license_expression("mit")
            .matcher(MatcherKind::Seq)
            .matched_length(10)
            .match_coverage(80.0)
            .rule_length(15)
            .end_token(10)
            .build_match();

        let _index = LicenseIndex::with_legalese_count(10);
        let (good, weak) = split_weak_matches(std::slice::from_ref(&m));
        assert!(good.contains(&m));
        assert!(!weak.contains(&m));
    }

    #[test]
    fn test_split_weak_matches_non_seq_short() {
        let m = TestMatchBuilder::default()
            .license_expression("mit")
            .matcher(MatcherKind::Hash)
            .matched_length(10)
            .match_coverage(20.0)
            .rule_length(15)
            .end_token(10)
            .build_match();

        let _index = LicenseIndex::with_legalese_count(10);
        let (good, weak) = split_weak_matches(std::slice::from_ref(&m));
        assert!(good.contains(&m));
        assert!(!weak.contains(&m));
    }

    #[test]
    fn test_split_weak_matches_mixed() {
        let good_match = TestMatchBuilder::default()
            .license_expression("mit")
            .matcher(MatcherKind::Hash)
            .matched_length(50)
            .match_coverage(95.0)
            .rule_length(50)
            .end_token(50)
            .build_match();

        let weak_unknown = TestMatchBuilder::default()
            .license_expression("unknown")
            .matcher(MatcherKind::Unknown)
            .matched_length(30)
            .match_coverage(50.0)
            .rule_length(30)
            .end_token(30)
            .build_match();

        let weak_seq = TestMatchBuilder::default()
            .license_expression("apache-2.0")
            .matcher(MatcherKind::Seq)
            .matched_length(10)
            .match_coverage(20.0)
            .rule_length(50)
            .end_token(10)
            .build_match();

        let matches = vec![good_match.clone(), weak_unknown.clone(), weak_seq.clone()];
        let (good, weak) = split_weak_matches(&matches);

        assert_eq!(good.len(), 1);
        assert_eq!(weak.len(), 2);
        assert!(good.contains(&good_match));
        assert!(weak.contains(&weak_unknown));
        assert!(weak.contains(&weak_seq));
    }
}
