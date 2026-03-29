//! Match grouping functions.

use super::LINES_THRESHOLD;
use super::types::DetectionGroup;
use crate::license_detection::models::LicenseMatch;

pub fn group_matches_by_region(matches: &[LicenseMatch]) -> Vec<DetectionGroup> {
    group_matches_by_region_with_threshold(matches, LINES_THRESHOLD)
}

/// Group matches by file region with a custom proximity threshold.
///
/// # Arguments
///
/// * `matches` - List of license matches to group, should be sorted by start_line
/// * `proximity_threshold` - Maximum line gap between matches to be in the same group
///
/// # Returns
///
/// A vector of DetectionGroup objects, each containing matches that form a region
pub(super) fn group_matches_by_region_with_threshold(
    matches: &[LicenseMatch],
    proximity_threshold: usize,
) -> Vec<DetectionGroup> {
    let mut groups = Vec::new();
    let mut current_group: Vec<LicenseMatch> = Vec::new();

    for match_item in matches {
        if current_group.is_empty() {
            current_group.push(match_item.clone());
            continue;
        }

        let previous_match = current_group.last().unwrap();

        if previous_match.is_license_intro() {
            current_group.push(match_item.clone());
        } else if match_item.is_license_intro() {
            if !current_group.is_empty() {
                groups.push(DetectionGroup::new(current_group.clone()));
            }
            current_group = vec![match_item.clone()];
        } else if match_item.is_license_clue() {
            if !current_group.is_empty() {
                groups.push(DetectionGroup::new(current_group.clone()));
            }
            groups.push(DetectionGroup::new(vec![match_item.clone()]));
            current_group = Vec::new();
        } else if should_group_together(previous_match, match_item, proximity_threshold) {
            current_group.push(match_item.clone());
        } else {
            if !current_group.is_empty() {
                groups.push(DetectionGroup::new(current_group.clone()));
            }
            current_group = vec![match_item.clone()];
        }
    }

    if !current_group.is_empty() {
        groups.push(DetectionGroup::new(current_group));
    }

    groups
}

/// Check if two matches should be in the same group based on line proximity.
///
/// Matches are grouped together when line gap is within threshold.
///
/// Based on Python's group_matches() at detection.py:1820-1868:
/// ```python
/// is_in_group_by_threshold = license_match.start_line <= previous_match.end_line + lines_threshold
/// ```
///
/// This means: GROUP if start_line <= prev_end_line + 4 (equivalent to line_gap <= 4)
pub(super) fn should_group_together(
    prev: &LicenseMatch,
    cur: &LicenseMatch,
    threshold: usize,
) -> bool {
    let line_gap = cur.start_line.saturating_sub(prev.end_line);
    line_gap <= threshold
}

/// Sort matches by start token position with tie-breaking criteria.
///
/// Python sorts matches at match.py:1099, 1220:
/// ```python
/// sorter = lambda m: (m.qspan.start, -m.hilen(), -m.len(), m.matcher_order)
/// ```
///
/// This ensures:
/// 1. Matches appear in file order (by start position)
/// 2. Longer, more specific matches come first (for overlapping matches)
/// 3. Higher hilen (legalese tokens) prioritized
/// 4. Exact matchers take priority over approximate
pub fn sort_matches_by_line(matches: &mut [LicenseMatch]) {
    matches.sort_by(|a, b| {
        a.start_token
            .cmp(&b.start_token)
            .then_with(|| b.hilen.cmp(&a.hilen))
            .then_with(|| b.len().cmp(&a.len()))
            .then_with(|| a.matcher_order().cmp(&b.matcher_order()))
    });
}
/// A detection is correct if:
/// - All matchers are "1-hash", "1-spdx-id", or "2-aho" (exact matchers)
/// - All match coverages are 100%
///
/// Based on Python: is_correct_detection() at detection.py:1078
#[cfg(test)]
pub(super) fn is_correct_detection(matches: &[LicenseMatch]) -> bool {
    if matches.is_empty() {
        return false;
    }

    let all_valid_matchers = matches.iter().all(|m| {
        matches!(
            m.matcher,
            crate::license_detection::models::MatcherKind::Hash
                | crate::license_detection::models::MatcherKind::SpdxId
                | crate::license_detection::models::MatcherKind::Aho
        )
    });

    let all_perfect_coverage = matches.iter().all(|m| m.match_coverage >= 100.0 - 0.01);

    all_valid_matchers && all_perfect_coverage
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license_detection::models::MatcherKind;
    use crate::license_detection::tests::TestMatchBuilder;

    fn create_test_match(
        start_line: usize,
        end_line: usize,
        matcher: &str,
        rule_identifier: &str,
    ) -> LicenseMatch {
        TestMatchBuilder::default()
            .license_expression("mit")
            .license_expression_spdx(Some("MIT".to_string()))
            .from_file(Some("test.txt".to_string()))
            .start_line(start_line)
            .end_line(end_line)
            .matcher(matcher.parse().expect("invalid test matcher"))
            .score(95.0)
            .matched_length(100)
            .rule_length(100)
            .match_coverage(95.0)
            .rule_relevance(100)
            .rule_identifier(rule_identifier)
            .rule_url("https://example.com".to_string())
            .matched_text(Some("MIT License".to_string()))
            .hilen(50)
            .build_match()
    }

    fn create_test_match_with_tokens(
        start_line: usize,
        end_line: usize,
        start_token: usize,
        end_token: usize,
    ) -> LicenseMatch {
        TestMatchBuilder::default()
            .license_expression("mit")
            .license_expression_spdx(Some("MIT".to_string()))
            .from_file(Some("test.txt".to_string()))
            .start_line(start_line)
            .end_line(end_line)
            .start_token(start_token)
            .end_token(end_token)
            .matcher(MatcherKind::Hash)
            .score(95.0)
            .matched_length(100)
            .rule_length(100)
            .match_coverage(95.0)
            .rule_relevance(100)
            .rule_identifier("mit.LICENSE")
            .rule_url("https://example.com".to_string())
            .matched_text(Some("MIT License".to_string()))
            .hilen(50)
            .build_match()
    }

    #[test]
    fn test_group_matches_empty() {
        let matches = Vec::new();
        let groups = group_matches_by_region(&matches);
        assert_eq!(groups.len(), 0);
    }

    #[test]
    fn test_group_matches_single() {
        let match1 = create_test_match(1, 5, "1-hash", "mit.LICENSE");
        let matches = vec![match1];
        let groups = group_matches_by_region(&matches);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].matches.len(), 1);
    }

    #[test]
    fn test_group_matches_within_threshold() {
        let match1 = create_test_match(1, 5, "1-hash", "mit.LICENSE");
        let match2 = create_test_match(6, 10, "2-aho", "mit.LICENSE");
        let matches = vec![match1, match2];
        let groups = group_matches_by_region(&matches);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].matches.len(), 2);
    }

    #[test]
    fn test_group_matches_separate_by_threshold() {
        let match1 = create_test_match(1, 5, "1-hash", "mit.LICENSE");
        let match2 = create_test_match(10, 15, "1-hash", "apache-2.0.LICENSE");
        let matches = vec![match1, match2];
        let groups = group_matches_by_region(&matches);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn test_group_matches_exactly_at_line_gap_threshold() {
        let match1 = create_test_match(1, 5, "1-hash", "mit.LICENSE");
        let match2 = create_test_match(8, 12, "2-aho", "mit.LICENSE");
        let matches = vec![match1, match2];
        let groups = group_matches_by_region(&matches);
        assert_eq!(groups.len(), 1, "Line gap 3 (8-5=3) should be grouped");
    }

    #[test]
    fn test_group_matches_just_past_line_gap_threshold() {
        let match1 = create_test_match(1, 5, "1-hash", "mit.LICENSE");
        let match2 = create_test_match(10, 14, "2-aho", "mit.LICENSE");
        let matches = vec![match1, match2];
        let groups = group_matches_by_region(&matches);
        assert_eq!(groups.len(), 2, "Line gap 5 (10-5=5) exceeds threshold 4");
    }

    #[test]
    fn test_group_matches_far_apart() {
        let match1 = create_test_match(1, 5, "1-hash", "mit.LICENSE");
        let match2 = create_test_match(20, 25, "1-hash", "apache-2.0.LICENSE");
        let matches = vec![match1, match2];
        let groups = group_matches_by_region(&matches);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn test_sort_matches_by_line() {
        let mut match1 = create_test_match(10, 15, "1-hash", "mit.LICENSE");
        match1.start_token = 100;
        let mut match2 = create_test_match(1, 5, "1-hash", "mit.LICENSE");
        match2.start_token = 10;
        let mut matches = vec![match1, match2];
        sort_matches_by_line(&mut matches);
        assert_eq!(matches[0].start_token, 10);
        assert_eq!(matches[1].start_token, 100);
    }

    #[test]
    fn test_grouping_within_both_thresholds() {
        let m1 = create_test_match_with_tokens(1, 10, 0, 50);
        let m2 = create_test_match_with_tokens(12, 20, 55, 100);
        let groups = group_matches_by_region(&[m1, m2]);
        assert_eq!(
            groups.len(),
            1,
            "Should group when line gap within threshold"
        );
    }

    #[test]
    fn test_grouping_separates_by_line_threshold() {
        let m1 = create_test_match_with_tokens(1, 10, 0, 50);
        let m2 = create_test_match_with_tokens(15, 25, 55, 100);
        let groups = group_matches_by_region(&[m1, m2]);
        assert_eq!(
            groups.len(),
            2,
            "Should separate when line gap exceeds threshold"
        );
    }

    #[test]
    fn test_grouping_at_exact_line_threshold() {
        let m1 = create_test_match_with_tokens(1, 10, 0, 50);
        let m2 = create_test_match_with_tokens(13, 20, 55, 100);
        let groups = group_matches_by_region(&[m1, m2]);
        assert_eq!(
            groups.len(),
            1,
            "Should group at exact line gap within threshold"
        );
    }

    #[test]
    fn test_group_matches_with_custom_threshold_zero() {
        let m1 = create_test_match(1, 5, "1-hash", "mit.LICENSE");
        let m2 = create_test_match(5, 10, "1-hash", "mit.LICENSE");
        let m3 = create_test_match(12, 15, "1-hash", "apache.LICENSE");
        let groups =
            group_matches_by_region_with_threshold(&[m1.clone(), m2.clone(), m3.clone()], 0);
        assert_eq!(groups.len(), 2, "Threshold 0 should only group gap=0");
    }

    #[test]
    fn test_group_matches_with_custom_threshold_large() {
        let m1 = create_test_match(1, 5, "1-hash", "mit.LICENSE");
        let m2 = create_test_match(50, 55, "1-hash", "mit.LICENSE");
        let groups = group_matches_by_region_with_threshold(&[m1, m2], 100);
        assert_eq!(
            groups.len(),
            1,
            "Large threshold should group distant matches"
        );
    }

    #[test]
    fn test_group_matches_threshold_exactly_at_boundary() {
        let m1 = create_test_match(1, 5, "1-hash", "mit.LICENSE");
        let m2_at_boundary = create_test_match(10, 15, "1-hash", "mit.LICENSE");
        let groups =
            group_matches_by_region_with_threshold(&[m1.clone(), m2_at_boundary.clone()], 4);
        assert_eq!(groups.len(), 2, "Threshold 4: should not group");
        let groups = group_matches_by_region_with_threshold(&[m1, m2_at_boundary], 5);
        assert_eq!(groups.len(), 1, "Threshold 5: should group");
    }

    #[test]
    fn test_is_correct_detection_perfect_hash() {
        let mut m = create_test_match(1, 5, "1-hash", "mit.LICENSE");
        m.match_coverage = 100.0;
        let matches = vec![m];
        assert!(is_correct_detection(&matches));
    }

    #[test]
    fn test_is_correct_detection_perfect_spdx() {
        let mut m = create_test_match(1, 5, "1-spdx-id", "mit.LICENSE");
        m.match_coverage = 100.0;
        let matches = vec![m];
        assert!(is_correct_detection(&matches));
    }

    #[test]
    fn test_is_correct_detection_perfect_aho() {
        let mut m = create_test_match(1, 5, "2-aho", "mit.LICENSE");
        m.match_coverage = 100.0;
        let matches = vec![m];
        assert!(is_correct_detection(&matches));
    }

    #[test]
    fn test_is_correct_detection_multiple_perfect() {
        let mut m1 = create_test_match(1, 10, "1-hash", "#1");
        m1.match_coverage = 100.0;
        let mut m2 = create_test_match(11, 20, "1-spdx-id", "#2");
        m2.match_coverage = 100.0;
        let matches = vec![m1, m2];
        assert!(is_correct_detection(&matches));
    }

    #[test]
    fn test_is_correct_detection_imperfect_coverage() {
        let m = create_test_match(1, 5, "1-hash", "mit.LICENSE");
        let matches = vec![m];
        assert!(!is_correct_detection(&matches));
    }

    #[test]
    fn test_is_correct_detection_unknown_matcher() {
        let mut m = create_test_match(1, 5, "6-unknown", "mit.LICENSE");
        m.match_coverage = 100.0;
        let matches = vec![m];
        assert!(!is_correct_detection(&matches));
    }

    #[test]
    fn test_is_correct_detection_empty() {
        let matches: Vec<LicenseMatch> = vec![];
        assert!(!is_correct_detection(&matches));
    }

    #[test]
    fn test_grouping_separates_by_token_threshold() {
        let m1 = create_test_match_with_tokens(1, 10, 0, 50);
        let m2 = create_test_match_with_tokens(12, 20, 65, 100);
        let groups = group_matches_by_region(&[m1, m2]);
        assert_eq!(
            groups.len(),
            1,
            "Should group when line gap (2) is within threshold - token gap is not used"
        );
    }

    #[test]
    fn test_grouping_at_exact_token_threshold() {
        let m1 = create_test_match_with_tokens(1, 10, 0, 50);
        let m2 = create_test_match_with_tokens(11, 20, 60, 100);
        let groups = group_matches_by_region(&[m1, m2]);
        assert_eq!(groups.len(), 1, "Should group when line gap is 1");
    }

    #[test]
    fn test_grouping_requires_both_thresholds() {
        let m1 = create_test_match_with_tokens(1, 10, 0, 50);
        let m2 = create_test_match_with_tokens(15, 25, 65, 100);
        let groups = group_matches_by_region(&[m1, m2]);
        assert_eq!(
            groups.len(),
            2,
            "Should separate when line gap (5) exceeds threshold (4)"
        );
    }
}
