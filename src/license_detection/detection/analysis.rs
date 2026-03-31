//! Detection analysis and heuristics.

use super::types::LicenseDetection;
use super::*;
use crate::license_detection::expression::combine_expressions_and;
use crate::license_detection::models::{LicenseMatch, MatcherKind};
use crate::utils::spdx::combine_license_expressions;

/// Coverage value below which detections are not perfect.
/// Any value < 100 means detection is imperfect.
pub const IMPERFECT_MATCH_COVERAGE_THR: f32 = 100.0;

/// Coverage values below this are reported as license clues.
pub const CLUES_MATCH_COVERAGE_THR: f32 = 60.0;

/// False positive threshold for rule length (in tokens).
/// Rules with length <= this are potential false positives.
pub const FALSE_POSITIVE_RULE_LENGTH_THRESHOLD: usize = 3;

/// False positive threshold for start line.
/// Matches after this line with short rules are potential false positives.
pub const FALSE_POSITIVE_START_LINE_THRESHOLD: usize = 1000;

/// Check if match coverage is below threshold.
///
/// Based on Python: is_match_coverage_less_than_threshold() at detection.py:1095
///
/// - If any_matches is True (default), returns True if ANY match has coverage < threshold
/// - If any_matches is False, returns True if NONE of the matches have coverage > threshold
pub(super) fn is_match_coverage_below_threshold(
    matches: &[LicenseMatch],
    threshold: f32,
    any_matches: bool,
) -> bool {
    if any_matches {
        return matches.iter().any(|m| m.coverage() < threshold);
    }
    !matches.iter().any(|m| m.coverage() > threshold)
}

/// Check if all matches have unknown license identifiers.
pub(super) fn has_unknown_matches(matches: &[LicenseMatch]) -> bool {
    matches
        .iter()
        .any(|m| m.rule_identifier.contains("unknown") || m.license_expression.contains("unknown"))
}

/// Check if matches have extra words.
///
/// Extra words are present when score < (coverage * relevance) / 100.
/// Based on Python: calculate_query_coverage_coefficient() at detection.py:1124
/// and has_extra_words() at detection.py:1139
pub(super) fn has_extra_words(matches: &[LicenseMatch]) -> bool {
    matches.iter().any(|m| {
        let score_coverage_relevance = m.coverage() * m.rule_relevance as f32 / 100.0;
        score_coverage_relevance - m.score > 0.01
    })
}

/// Check if detection is a false positive.
///
/// False positives are identified based on:
/// - Single matches with bare identifiers and low relevance
/// - GPL matches with short length
/// - Late matches with short rules and low relevance
/// - Tag matches with short length
///
/// Based on Python: is_false_positive() at detection.py:1162
pub(super) fn is_false_positive(matches: &[LicenseMatch]) -> bool {
    if matches.is_empty() {
        return false;
    }

    let has_full_relevance = matches.iter().all(|m| m.rule_relevance == 100);

    let copyright_words = ["copyright", "(c)"];
    let has_copyrights = matches.iter().all(|m| {
        m.matched_text
            .as_ref()
            .map(|text| {
                let text_lower = text.to_lowercase();
                copyright_words.iter().any(|word| text_lower.contains(word))
            })
            .unwrap_or(false)
    });

    if has_copyrights || has_full_relevance {
        return false;
    }

    let start_line = matches.iter().map(|m| m.start_line).min().unwrap_or(0);

    let bare_rules = ["gpl_bare", "freeware_bare", "public-domain_bare"];
    let is_bare_rule = matches.iter().all(|m| {
        bare_rules
            .iter()
            .any(|bare| m.rule_identifier.to_lowercase().contains(bare))
    });

    let is_gpl = matches.iter().all(|m| {
        let id = m.rule_identifier.to_lowercase();
        id.contains("gpl") && !id.contains("lgpl")
    });

    // Use rule_length (token count) instead of matched_length (character count)
    let rule_length_values: Vec<usize> = matches.iter().map(|m| m.rule_length).collect();

    let all_rule_length_one = rule_length_values.iter().all(|&l| l == 1);

    let all_low_relevance = matches.iter().all(|m| m.rule_relevance < 60);

    let is_single = matches.len() == 1;

    // Check if all matches are license tags with length == 1
    let all_is_license_tag = matches.iter().all(LicenseMatch::is_license_tag);

    // Check 1: Single bare rule with low relevance
    if is_single && is_bare_rule && all_low_relevance {
        return true;
    }

    // Check 2: GPL with rule_length == 1 (matching Python's all_match_rule_length_one)
    if is_gpl && all_rule_length_one {
        return true;
    }

    // Check 3: Late matches (after line 1000) with short rules (<=3 tokens) and low relevance
    // Python: any(rule_length <= 3) not all(rule_length == 1)
    if all_low_relevance
        && start_line > FALSE_POSITIVE_START_LINE_THRESHOLD
        && rule_length_values
            .iter()
            .any(|&l| l <= FALSE_POSITIVE_RULE_LENGTH_THRESHOLD)
    {
        return true;
    }

    // Check 4: License tags with short rule length
    if all_is_license_tag && all_rule_length_one {
        return true;
    }

    false
}

/// Check if matches are low quality based on coverage.
///
/// Low quality matches have:
/// - Coverage < CLUES_MATCH_COVERAGE_THR
/// - OR (coverage < IMPERFECT_MATCH_COVERAGE_THR AND has extra words)
///
/// Based on Python: is_low_quality_matches() at detection.py:1223
pub(super) fn is_low_quality_matches(matches: &[LicenseMatch]) -> bool {
    matches.iter().all(|m| {
        m.coverage() < CLUES_MATCH_COVERAGE_THR
            || (m.coverage() < IMPERFECT_MATCH_COVERAGE_THR
                && has_extra_words(std::slice::from_ref(m)))
    })
}

/// Check if any match has correct license clue.
pub(super) fn has_correct_license_clue_matches(matches: &[LicenseMatch]) -> bool {
    !matches.is_empty()
        && matches.iter().all(|m| {
            matches!(
                m.matcher,
                MatcherKind::Hash | MatcherKind::SpdxId | MatcherKind::Aho
            )
        })
        && matches.iter().all(|m| m.coverage() == 100.0)
        && matches.iter().all(LicenseMatch::is_license_clue)
}

/// Check if matches represent undetected licenses.
///
/// Returns true if matches were detected by the "undetected" matcher.
/// Based on Python: is_undetected_license_matches() at detection.py:1376
pub(super) fn is_undetected_license_matches(matches: &[LicenseMatch]) -> bool {
    !matches.is_empty() && matches.iter().all(|m| m.matcher == MatcherKind::Undetected)
}

/// Check if there are unknown license intros before detection.
///
/// Based on Python: has_unknown_intro_before_detection() at detection.py:1196
pub(super) fn has_unknown_intro_before_detection(matches: &[LicenseMatch]) -> bool {
    // Python: if len(license_matches) == 1: return False
    if matches.len() == 1 {
        return false;
    }

    // Python: if all([is_unknown_intro(match) for match in license_matches]): return False
    if matches.iter().all(is_unknown_intro) {
        return false;
    }

    for m in matches {
        if m.matcher == MatcherKind::Undetected {
            continue;
        }
        let has_unknown = m.license_expression.contains("unknown");
        let is_intro =
            m.is_license_intro() || m.is_license_clue() || m.license_expression == "free-unknown";
        if has_unknown && is_intro {
            // Check if there's a non-intro, non-unknown match after this
            let has_unknown_intro = matches.iter().any(|other| {
                other.matcher != MatcherKind::Undetected
                    && other.start_line > m.start_line
                    && !other.rule_identifier.contains("unknown")
                    && !other.license_expression.contains("unknown")
                    && !other.is_license_intro()
                    && !other.is_license_clue()
            });

            if has_unknown_intro {
                let coverage_ok = m.coverage() >= IMPERFECT_MATCH_COVERAGE_THR;
                let not_unknown = !m.rule_identifier.contains("unknown")
                    && !m.license_expression.contains("unknown");
                if coverage_ok && not_unknown {
                    return true;
                }
            }
        }
    }

    if matches.iter().any(is_unknown_intro) {
        let filtered_matches = filter_license_intros(matches);
        if filtered_matches.len() != matches.len()
            && is_match_coverage_below_threshold(
                &filtered_matches,
                IMPERFECT_MATCH_COVERAGE_THR,
                false,
            )
        {
            return true;
        }
    }

    false
}

/// Check if a match is an unknown license intro.
///
/// Based on Python: is_unknown_intro() at detection.py:1250-1262
pub(super) fn is_unknown_intro(m: &LicenseMatch) -> bool {
    let has_unknown = m.license_expression.contains("unknown");
    has_unknown
        && (m.is_license_intro() || m.is_license_clue() || m.license_expression == "free-unknown")
}

/// Check if a match should be considered a license intro for filtering.
///
/// A match is considered a license intro if it has is_license_intro or
/// is_license_clue flag set OR its license_expression is "free-unknown",
/// AND it was matched by the "2-aho" matcher OR has 100% match coverage.
///
/// Based on Python: is_license_intro() at detection.py:1349-1365
pub(super) fn is_license_intro(match_item: &LicenseMatch) -> bool {
    (match_item.is_license_intro()
        || match_item.is_license_clue()
        || match_item.license_expression == "free-unknown")
        && (match_item.matcher == MatcherKind::Aho || match_item.coverage() == 100.0)
}

/// Filter out license intro matches from a list of matches.
///
/// Based on Python: filter_license_intros() at detection.py:1368-1383
pub(super) fn filter_license_intros(matches: &[LicenseMatch]) -> Vec<LicenseMatch> {
    matches
        .iter()
        .filter(|m| !is_license_intro(m))
        .cloned()
        .collect()
}

/// Check if a match references a local file.
///
/// Based on Python: is_license_reference_local_file() at detection.py:1368-1377
pub(super) fn is_license_reference_local_file(m: &LicenseMatch) -> bool {
    m.referenced_filenames
        .as_ref()
        .is_some_and(|v| !v.is_empty())
}

/// Filter out license reference matches that point to local files.
///
/// Based on Python: filter_license_references() at detection.py:1404-1419
#[cfg(test)]
pub(super) fn filter_license_references(matches: &[LicenseMatch]) -> Vec<LicenseMatch> {
    matches
        .iter()
        .filter(|m| !is_license_reference_local_file(m))
        .cloned()
        .collect()
}

/// Filter out both license intros and local file references.
///
/// Based on Python: filter_license_intros_and_references() at detection.py:1422-1440
pub(super) fn filter_license_intros_and_references(matches: &[LicenseMatch]) -> Vec<LicenseMatch> {
    matches
        .iter()
        .filter(|m| !is_license_intro(m) && !is_license_reference_local_file(m))
        .cloned()
        .collect()
}

/// Check if any matches reference local files.
fn has_references_to_local_files(matches: &[LicenseMatch]) -> bool {
    matches.iter().any(is_license_reference_local_file)
}

/// Analyze detection and return detection log message.
///
/// Based on Python: analyze_detection() at detection.py:1445-1561
pub(super) fn analyze_detection(matches: &[LicenseMatch], package_license: bool) -> &'static str {
    if matches.is_empty() {
        return "";
    }

    // Check 1: Undetected matches
    if is_undetected_license_matches(matches) {
        return DETECTION_LOG_UNDETECTED_LICENSE;
    }

    // Check 2: Unknown intro before detection
    if has_unknown_intro_before_detection(matches) {
        return "unknown-intro-followed-by-match";
    }

    // Check 3: References to local files
    if has_references_to_local_files(matches) {
        return "unknown-reference-to-local-file";
    }

    // Check 4: False positive (unless package_license is set)
    if !package_license && is_false_positive(matches) {
        return "false-positive";
    }

    // Check 5: License clues
    if !package_license && has_correct_license_clue_matches(matches) {
        return DETECTION_LOG_LICENSE_CLUES;
    }

    // Check 6: Perfect detection (correct AND no unknowns AND no extra words)
    if is_correct_detection_non_unknown(matches) {
        return "";
    }

    // Check 7: Unknown matches
    if has_unknown_matches(matches) {
        return DETECTION_LOG_UNKNOWN_MATCH;
    }

    // Check 8: Low quality matches
    if !package_license && is_low_quality_matches(matches) {
        return "low-quality-match-fragments";
    }

    // Check 9: Imperfect coverage
    if matches
        .iter()
        .any(|m| m.coverage() < IMPERFECT_MATCH_COVERAGE_THR)
    {
        return DETECTION_LOG_IMPERFECT_COVERAGE;
    }

    // Check 10: Extra words
    if has_extra_words(matches) {
        return DETECTION_LOG_EXTRA_WORDS;
    }

    ""
}

fn is_correct_detection_non_unknown(matches: &[LicenseMatch]) -> bool {
    matches.iter().all(|m| m.coverage() == 100.0)
        && !has_unknown_matches(matches)
        && !has_extra_words(matches)
}

/// Compute detection score from matches.
///
/// Based on Python: compute_detection_score() at detection.py:1585-1608
pub fn compute_detection_score(matches: &[LicenseMatch]) -> f32 {
    if matches.is_empty() {
        return 0.0;
    }

    let total_length: f32 = matches.iter().map(|m| m.matched_length as f32).sum();
    if total_length == 0.0 {
        return 0.0;
    }

    let weighted_score: f32 = matches
        .iter()
        .map(|m| m.score * (m.matched_length as f32 / total_length))
        .sum();

    ((weighted_score * 100.0).round() / 100.0).min(100.0)
}

/// Determine license expression from matches.
///
/// Combines license expressions from all matches using AND/OR relationships.
///
/// Based on Python: determine_license_expression() at detection.py:1611-1635
pub fn determine_license_expression(matches: &[LicenseMatch]) -> Result<String, String> {
    if matches.is_empty() {
        return Err("No matches to determine expression from".to_string());
    }

    let expressions: Vec<&str> = matches
        .iter()
        .map(|m| m.license_expression.as_str())
        .collect();

    combine_expressions_and(&expressions, true)
        .map_err(|e| format!("Failed to combine expressions: {}", e))
}

/// Determine SPDX expression from matches.
///
/// Converts license expressions to SPDX identifiers.
///
/// Based on Python: determine_spdx_expression() at detection.py:1638-1671
pub fn determine_spdx_expression(matches: &[LicenseMatch]) -> Result<String, String> {
    if matches.is_empty() {
        return Err("No matches to determine SPDX expression from".to_string());
    }

    let expressions: Option<Vec<&str>> = matches
        .iter()
        .map(|m| m.license_expression_spdx.as_deref())
        .collect();

    let expressions = expressions
        .ok_or_else(|| "Missing SPDX expressions for one or more matches".to_string())?;

    combine_license_expressions(expressions.into_iter().map(str::to_string))
        .ok_or_else(|| "Failed to combine SPDX expressions".to_string())
}

/// Determine SPDX expression from ScanCode license keys.
///
/// Based on Python: determine_spdx_expression_from_scancode() at detection.py:1674-1709
pub fn determine_spdx_expression_from_scancode(
    scancode_expression: &str,
    spdx_mapping: &SpdxMapping,
) -> Result<String, String> {
    if scancode_expression.is_empty() {
        return Ok(String::new());
    }

    spdx_mapping
        .expression_scancode_to_spdx(scancode_expression)
        .map_err(|e| e.to_string())
}
///
/// A detection is valid if:
/// - Score meets minimum threshold
/// - Not identified as low quality matches
/// - Not identified as false positive
///
/// Based on Python: is_correct_detection_non_unknown() at detection.py:1066
pub(super) fn classify_detection(detection: &LicenseDetection, min_score: f32) -> bool {
    if detection.matches.is_empty() {
        return false;
    }

    let score = compute_detection_score(&detection.matches);
    let meets_score_threshold = score >= min_score - 0.01;
    let not_false_positive = !is_false_positive(&detection.matches);

    // Python does NOT filter out low-quality matches - it returns them with
    // "low-quality-matches" in detection_log but still includes them.
    // See: detection.py get_detected_license_expression() lines 1565-1571
    meets_score_threshold && not_false_positive
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license_detection::models::{LicenseMatch, MatcherKind, PositionSpan};

    fn create_test_match(coverage: f32, rule_identifier: &str) -> LicenseMatch {
        LicenseMatch {
            rid: 0,
            license_expression: "mit".to_string(),
            license_expression_spdx: Some("MIT".to_string()),
            from_file: Some("test.txt".to_string()),
            start_line: 1,
            end_line: 10,
            start_token: 0,
            end_token: 0,
            matcher: crate::license_detection::models::MatcherKind::Hash,
            score: 95.0,
            matched_length: 100,
            match_coverage: coverage,
            rule_relevance: 100,
            rule_identifier: rule_identifier.to_string(),
            rule_url: "https://example.com".to_string(),
            matched_text: Some("MIT License".to_string()),
            referenced_filenames: None,
            rule_kind: crate::license_detection::models::RuleKind::None,
            is_from_license: false,
            rule_length: 100,
            rule_start_token: 0,
            qspan: PositionSpan::empty(),
            ispan: PositionSpan::empty(),
            hispan: PositionSpan::empty(),
            candidate_resemblance: 0.0,
            candidate_containment: 0.0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn create_test_match_full(
        license_expression: &str,
        matcher: &str,
        start_line: usize,
        end_line: usize,
        score: f32,
        matched_length: usize,
        rule_length: usize,
        match_coverage: f32,
        rule_relevance: u8,
        rule_identifier: &str,
    ) -> LicenseMatch {
        LicenseMatch {
            rid: 0,
            license_expression: license_expression.to_string(),
            license_expression_spdx: Some(license_expression.to_string()),
            from_file: Some("test.txt".to_string()),
            start_line,
            end_line,
            start_token: 0,
            end_token: 0,
            matcher: matcher.parse().expect("invalid test matcher"),
            score,
            matched_length,
            rule_length,
            match_coverage,
            rule_relevance,
            rule_identifier: rule_identifier.to_string(),
            rule_url: "https://example.com".to_string(),
            matched_text: Some("License text".to_string()),
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
    fn test_is_match_coverage_below_threshold_above() {
        let matches = vec![create_test_match(95.0, "mit.LICENSE")];
        assert!(!is_match_coverage_below_threshold(&matches, 70.0, true));
    }

    #[test]
    fn test_is_match_coverage_below_threshold_below() {
        let matches = vec![create_test_match(65.0, "mit.LICENSE")];
        assert!(is_match_coverage_below_threshold(&matches, 70.0, true));
    }

    #[test]
    fn test_is_match_coverage_below_threshold_exact() {
        let matches = vec![create_test_match_full(
            "mit",
            "1-hash",
            1,
            10,
            60.0,
            100,
            100,
            60.0,
            100,
            "mit.LICENSE",
        )];
        assert!(!is_match_coverage_below_threshold(&matches, 60.0, true));
    }

    #[test]
    fn test_is_match_coverage_below_threshold_uses_rounded_coverage() {
        let matches = vec![create_test_match_full(
            "mit",
            "1-hash",
            1,
            10,
            80.0,
            100,
            100,
            79.996,
            100,
            "mit.LICENSE",
        )];
        assert!(!is_match_coverage_below_threshold(&matches, 80.0, true));
    }

    #[test]
    fn test_has_correct_license_clue_matches_requires_full_coverage() {
        let mut m = create_test_match_full(
            "mit",
            "1-hash",
            1,
            10,
            100.0,
            100,
            100,
            99.99,
            100,
            "mit.LICENSE",
        );
        m.rule_kind = crate::license_detection::models::RuleKind::Clue;

        assert!(!has_correct_license_clue_matches(&[m]));
    }

    #[test]
    fn test_is_match_coverage_below_threshold_empty() {
        let matches: Vec<LicenseMatch> = vec![];
        assert!(!is_match_coverage_below_threshold(&matches, 70.0, true));
    }

    #[test]
    fn test_has_unknown_matches_false() {
        let matches = vec![create_test_match(95.0, "mit.LICENSE")];
        assert!(!has_unknown_matches(&matches));
    }

    #[test]
    fn test_has_unknown_matches_true_in_identifier() {
        let matches = vec![create_test_match(95.0, "unknown.LICENSE")];
        assert!(has_unknown_matches(&matches));
    }

    #[test]
    fn test_has_unknown_matches_true_in_expression() {
        let mut m = create_test_match(95.0, "mit.LICENSE");
        m.license_expression = "unknown".to_string();
        let matches = vec![m];
        assert!(has_unknown_matches(&matches));
    }

    #[test]
    fn test_has_extra_words_false() {
        let matches = vec![create_test_match(95.0, "mit.LICENSE")];
        assert!(!has_extra_words(&matches));
    }

    #[test]
    fn test_has_extra_words_true() {
        let mut m = create_test_match(95.0, "mit.LICENSE");
        m.score = 50.0;
        let matches = vec![m];
        assert!(has_extra_words(&matches));
    }

    #[test]
    fn test_is_false_positive_empty() {
        let matches: Vec<LicenseMatch> = vec![];
        assert!(!is_false_positive(&matches));
    }

    #[test]
    fn test_is_false_positive_perfect_match() {
        let matches = vec![create_test_match_full(
            "mit",
            "1-hash",
            1,
            10,
            95.0,
            100,
            100,
            100.0,
            100,
            "mit.LICENSE",
        )];
        assert!(!is_false_positive(&matches));
    }

    #[test]
    fn test_is_false_positive_bare_single() {
        let matches = vec![create_test_match_full(
            "gpl",
            "2-aho",
            2000,
            2005,
            30.0,
            3,
            3,
            30.0,
            50,
            "gpl_bare.LICENSE",
        )];
        assert!(is_false_positive(&matches));
    }

    #[test]
    fn test_is_false_positive_gpl_short() {
        let matches = vec![create_test_match_full(
            "gpl-2.0",
            "2-aho",
            1,
            10,
            50.0,
            2,
            1,
            50.0,
            50,
            "gpl-2.0.LICENSE",
        )];
        assert!(is_false_positive(&matches));
    }

    #[test]
    fn test_is_false_positive_lgpl_short_not_filtered() {
        let matches = vec![create_test_match_full(
            "lgpl-2.0-plus",
            "2-aho",
            6,
            8,
            50.0,
            1,
            1,
            100.0,
            60,
            "lgpl_bare_single_word.RULE",
        )];
        assert!(!is_false_positive(&matches));
    }

    #[test]
    fn test_is_false_positive_late_short_low_relevance() {
        let matches = vec![create_test_match_full(
            "mit",
            "2-aho",
            1500,
            1505,
            30.0,
            3,
            1,
            30.0,
            50,
            "mit.LICENSE",
        )];
        assert!(is_false_positive(&matches));
    }

    #[test]
    fn test_is_false_positive_single_license_reference_short() {
        let mut m = create_test_match_full(
            "borceux",
            "2-aho",
            1,
            10,
            100.0,
            1,
            1,
            100.0,
            80,
            "borceux.LICENSE",
        );
        m.rule_kind = crate::license_detection::models::RuleKind::Reference;
        let matches = vec![m];
        assert!(!is_false_positive(&matches));
    }

    #[test]
    fn test_is_false_positive_single_license_reference_long_rule() {
        let mut m = create_test_match_full(
            "some-license",
            "2-aho",
            1,
            10,
            100.0,
            10,
            10,
            100.0,
            80,
            "some-license.LICENSE",
        );
        m.rule_kind = crate::license_detection::models::RuleKind::Reference;
        let matches = vec![m];
        assert!(!is_false_positive(&matches));
    }

    #[test]
    fn test_is_false_positive_single_license_reference_full_relevance() {
        let mut m = create_test_match_full(
            "some-license",
            "2-aho",
            1,
            10,
            100.0,
            1,
            1,
            100.0,
            100,
            "some-license.LICENSE",
        );
        m.rule_kind = crate::license_detection::models::RuleKind::Reference;
        let matches = vec![m];
        assert!(!is_false_positive(&matches));
    }

    #[test]
    fn test_is_false_positive_with_copyright_word() {
        let mut m = create_test_match_full(
            "gpl-2.0",
            "2-aho",
            1,
            10,
            50.0,
            100,
            1,
            50.0,
            50,
            "gpl-2.0.LICENSE",
        );
        m.matched_text = Some("This is copyrighted material under GPL".to_string());
        let matches = vec![m];
        assert!(!is_false_positive(&matches));
    }

    #[test]
    fn test_is_false_positive_with_c_symbol() {
        let mut m = create_test_match_full(
            "mit", "2-aho", 1500, 1510, 30.0, 10, 2, 30.0, 50, "mit.RULE",
        );
        m.matched_text = Some("Licensed under MIT (c) 2024".to_string());
        let matches = vec![m];
        assert!(!is_false_positive(&matches));
    }

    #[test]
    fn test_is_false_positive_without_copyright_word() {
        let mut m = create_test_match_full(
            "gpl-2.0",
            "2-aho",
            1,
            10,
            50.0,
            5,
            1,
            50.0,
            50,
            "gpl-2.0.LICENSE",
        );
        m.matched_text = Some("GPL licensed software".to_string());
        let matches = vec![m];
        assert!(is_false_positive(&matches));
    }

    #[test]
    fn test_is_false_positive_partial_copyright() {
        let mut m1 = create_test_match_full(
            "gpl-2.0",
            "2-aho",
            1,
            5,
            50.0,
            10,
            1,
            50.0,
            50,
            "gpl-2.0.LICENSE",
        );
        m1.matched_text = Some("Copyright GPL".to_string());
        let mut m2 = create_test_match_full(
            "gpl-2.0",
            "2-aho",
            6,
            10,
            50.0,
            10,
            1,
            50.0,
            50,
            "gpl-2.0.LICENSE",
        );
        m2.matched_text = Some("GPL licensed".to_string());
        let matches = vec![m1, m2];
        assert!(is_false_positive(&matches));
    }

    #[test]
    fn test_is_false_positive_all_matches_with_copyright() {
        let mut m1 = create_test_match_full(
            "gpl-2.0",
            "2-aho",
            1,
            5,
            50.0,
            10,
            1,
            50.0,
            50,
            "gpl-2.0.LICENSE",
        );
        m1.matched_text = Some("Copyright GPL".to_string());
        let mut m2 = create_test_match_full(
            "gpl-2.0",
            "2-aho",
            6,
            10,
            50.0,
            10,
            1,
            50.0,
            50,
            "gpl-2.0.LICENSE",
        );
        m2.matched_text = Some("(c) GPL".to_string());
        let matches = vec![m1, m2];
        assert!(!is_false_positive(&matches));
    }

    #[test]
    fn test_is_false_positive_matched_text_none() {
        let mut m = create_test_match_full(
            "gpl-2.0",
            "2-aho",
            1,
            10,
            50.0,
            5,
            1,
            50.0,
            50,
            "gpl-2.0.LICENSE",
        );
        m.matched_text = None;
        let matches = vec![m];
        assert!(is_false_positive(&matches));
    }

    #[test]
    fn test_is_false_positive_copyright_case_insensitive() {
        let mut m =
            create_test_match_full("mit", "2-aho", 1, 10, 50.0, 10, 1, 50.0, 50, "mit.RULE");
        m.matched_text = Some("COPYRIGHT HOLDER NAME".to_string());
        let matches = vec![m];
        assert!(!is_false_positive(&matches));
    }

    #[test]
    fn test_is_false_positive_copyright_empty_string() {
        let mut m = create_test_match_full(
            "gpl-2.0",
            "2-aho",
            1,
            10,
            50.0,
            5,
            1,
            50.0,
            50,
            "gpl-2.0.LICENSE",
        );
        m.matched_text = Some("".to_string());
        let matches = vec![m];
        assert!(is_false_positive(&matches));
    }

    #[test]
    fn test_is_low_quality_matches_low_coverage() {
        let matches = vec![create_test_match_full(
            "mit",
            "2-aho",
            1,
            10,
            40.0,
            20,
            20,
            40.0,
            80,
            "mit.LICENSE",
        )];
        assert!(is_low_quality_matches(&matches));
    }

    #[test]
    fn test_is_low_quality_matches_false_perfect() {
        let matches = vec![create_test_match_full(
            "mit",
            "1-hash",
            1,
            10,
            95.0,
            100,
            100,
            100.0,
            100,
            "mit.LICENSE",
        )];
        assert!(!is_low_quality_matches(&matches));
    }

    #[test]
    fn test_is_low_quality_matches_empty() {
        let matches: Vec<LicenseMatch> = vec![];
        assert!(is_low_quality_matches(&matches));
    }

    #[test]
    fn test_compute_detection_score_single() {
        let matches = vec![create_test_match(95.0, "mit.LICENSE")];
        let score = compute_detection_score(&matches);
        assert!(score > 90.0);
    }

    #[test]
    fn test_compute_detection_score_multiple_equal() {
        let m1 = create_test_match_full(
            "mit",
            "1-hash",
            1,
            10,
            90.0,
            100,
            100,
            90.0,
            100,
            "mit.LICENSE",
        );
        let m2 = create_test_match_full(
            "mit",
            "1-hash",
            11,
            20,
            90.0,
            100,
            100,
            90.0,
            100,
            "mit.LICENSE",
        );
        let matches = vec![m1, m2];
        let score = compute_detection_score(&matches);
        assert!((score - 90.0).abs() < 0.1);
    }

    #[test]
    fn test_compute_detection_score_weighted() {
        let m1 = create_test_match_full(
            "mit",
            "1-hash",
            1,
            10,
            80.0,
            100,
            100,
            80.0,
            100,
            "mit.LICENSE",
        );
        let m2 = create_test_match_full(
            "mit",
            "1-hash",
            11,
            20,
            100.0,
            100,
            100,
            100.0,
            100,
            "mit.LICENSE",
        );
        let matches = vec![m1, m2];
        let score = compute_detection_score(&matches);
        assert!(score > 80.0 && score < 100.0);
    }

    #[test]
    fn test_compute_detection_score_empty() {
        let matches: Vec<LicenseMatch> = vec![];
        let score = compute_detection_score(&matches);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_compute_detection_score_capped_at_100() {
        let m = create_test_match_full(
            "mit",
            "1-hash",
            1,
            10,
            100.0,
            100,
            100,
            100.0,
            100,
            "mit.LICENSE",
        );
        let matches = vec![m];
        let score = compute_detection_score(&matches);
        assert_eq!(score, 100.0);
    }

    #[test]
    fn test_compute_detection_score_weights_by_match_length_only() {
        let m1 = create_test_match_full(
            "mit",
            "1-hash",
            1,
            100,
            50.0,
            100,
            100,
            100.0,
            50,
            "mit.LICENSE",
        );
        let m2 = create_test_match_full(
            "apache-2.0",
            "1-hash",
            101,
            200,
            100.0,
            100,
            100,
            100.0,
            100,
            "apache.LICENSE",
        );

        let score = compute_detection_score(&[m1, m2]);
        assert_eq!(score, 75.0);
    }

    #[test]
    fn test_compute_detection_score_rounds_to_two_decimals() {
        let m1 = create_test_match_full(
            "mit",
            "1-hash",
            1,
            100,
            80.0,
            100,
            100,
            20.0,
            100,
            "mit.LICENSE",
        );
        let m2 = create_test_match_full(
            "apache-2.0",
            "1-hash",
            101,
            110,
            100.0,
            10,
            100,
            100.0,
            100,
            "apache.LICENSE",
        );

        let score = compute_detection_score(&[m1, m2]);
        assert_eq!(score, 81.82);
    }

    #[test]
    fn test_determine_license_expression_single() {
        let matches = vec![create_test_match(95.0, "mit.LICENSE")];
        let result = determine_license_expression(&matches);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "mit");
    }

    #[test]
    fn test_determine_license_expression_multiple() {
        let m1 = create_test_match_full(
            "mit",
            "1-hash",
            1,
            10,
            100.0,
            100,
            100,
            100.0,
            100,
            "mit.LICENSE",
        );
        let mut m2 = create_test_match_full(
            "apache-2.0",
            "1-hash",
            11,
            20,
            100.0,
            100,
            100,
            100.0,
            100,
            "apache.LICENSE",
        );
        m2.license_expression = "apache-2.0".to_string();
        let matches = vec![m1, m2];
        let result = determine_license_expression(&matches);
        assert!(result.is_ok());
        let expr = result.unwrap();
        assert!(expr.contains("mit"));
        assert!(expr.contains("apache-2.0"));
    }

    #[test]
    fn test_determine_license_expression_flattens_same_operator_chain() {
        let expressions = [
            "apache-2.0",
            "bsd-3-clause",
            "gpl-2.0-only",
            "licenseref-scancode-oracle-openjdk-exception-2.0",
            "apsl-1.0",
            "apsl-2.0",
        ];

        let matches = expressions
            .iter()
            .enumerate()
            .map(|(index, expression)| {
                let mut license_match = create_test_match_full(
                    expression,
                    "1-hash",
                    index + 1,
                    index + 1,
                    100.0,
                    100,
                    100,
                    100.0,
                    100,
                    expression,
                );
                license_match.license_expression = (*expression).to_string();
                license_match
            })
            .collect::<Vec<_>>();

        let result = determine_license_expression(&matches);

        assert_eq!(
            result.as_deref(),
            Ok(
                "apache-2.0 AND bsd-3-clause AND gpl-2.0-only AND licenseref-scancode-oracle-openjdk-exception-2.0 AND apsl-1.0 AND apsl-2.0"
            )
        );
    }

    #[test]
    fn test_determine_license_expression_simplifies_absorbed_compound() {
        let m1 = create_test_match_full(
            "mit",
            "1-hash",
            1,
            10,
            100.0,
            100,
            100,
            100.0,
            100,
            "mit.LICENSE",
        );
        let mut m2 = create_test_match_full(
            "mit",
            "1-hash",
            11,
            20,
            100.0,
            100,
            100,
            100.0,
            100,
            "apache.LICENSE",
        );
        m2.license_expression = "mit OR apache-2.0".to_string();

        let result = determine_license_expression(&[m1, m2]);

        assert_eq!(result.as_deref(), Ok("mit"));
    }

    #[test]
    fn test_determine_license_expression_empty() {
        let matches: Vec<LicenseMatch> = vec![];
        let result = determine_license_expression(&matches);
        assert!(result.is_err());
    }

    #[test]
    fn test_classify_detection_valid_perfect() {
        let m = create_test_match_full(
            "mit",
            "1-hash",
            1,
            10,
            95.0,
            100,
            100,
            100.0,
            100,
            "mit.LICENSE",
        );
        let detection = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![m],
            detection_log: vec!["perfect-detection".to_string()],
            identifier: None,
            file_regions: Vec::new(),
        };
        assert!(classify_detection(&detection, 0.0));
    }

    #[test]
    fn test_classify_detection_invalid_low_score() {
        let m = create_test_match_full(
            "mit",
            "2-aho",
            1,
            10,
            30.0,
            100,
            100,
            30.0,
            50,
            "mit.LICENSE",
        );
        let detection = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![m],
            detection_log: vec![],
            identifier: None,
            file_regions: Vec::new(),
        };
        assert!(!classify_detection(&detection, 50.0));
    }

    #[test]
    fn test_classify_detection_invalid_false_positive() {
        let m = create_test_match_full(
            "gpl",
            "2-aho",
            2000,
            2005,
            30.0,
            3,
            3,
            30.0,
            50,
            "gpl_bare.LICENSE",
        );
        let detection = LicenseDetection {
            license_expression: Some("gpl".to_string()),
            license_expression_spdx: Some("GPL".to_string()),
            matches: vec![m],
            detection_log: vec![],
            identifier: None,
            file_regions: Vec::new(),
        };
        assert!(!classify_detection(&detection, 0.0));
    }

    #[test]
    fn test_classify_detection_invalid_empty() {
        let detection = LicenseDetection {
            license_expression: None,
            license_expression_spdx: None,
            matches: vec![],
            detection_log: vec![],
            identifier: None,
            file_regions: Vec::new(),
        };
        assert!(!classify_detection(&detection, 0.0));
    }

    #[test]
    fn test_classify_detection_score_threshold() {
        let m = create_test_match_full(
            "mit",
            "1-hash",
            1,
            10,
            45.0,
            100,
            100,
            45.0,
            100,
            "mit.LICENSE",
        );
        let detection = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![m],
            detection_log: vec![],
            identifier: None,
            file_regions: Vec::new(),
        };
        assert!(classify_detection(&detection, 45.0));
        assert!(!classify_detection(&detection, 50.0));
    }

    #[test]
    fn test_classify_detection_perfect_matches() {
        let m = create_test_match_full(
            "mit",
            "1-hash",
            1,
            10,
            100.0,
            100,
            100,
            100.0,
            100,
            "mit.LICENSE",
        );
        let detection = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![m],
            detection_log: vec!["perfect-detection".to_string()],
            identifier: None,
            file_regions: Vec::new(),
        };
        assert!(classify_detection(&detection, 0.0));
    }

    #[test]
    fn test_determine_spdx_expression_single() {
        let matches = vec![create_test_match(95.0, "mit.LICENSE")];
        let result = determine_spdx_expression(&matches);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "MIT");
    }

    #[test]
    fn test_determine_spdx_expression_multiple() {
        let m1 = create_test_match_full(
            "mit",
            "1-hash",
            1,
            10,
            100.0,
            100,
            100,
            100.0,
            100,
            "mit.LICENSE",
        );
        let mut m2 = create_test_match_full(
            "apache-2.0",
            "1-hash",
            11,
            20,
            100.0,
            100,
            100,
            100.0,
            100,
            "apache.LICENSE",
        );
        m2.license_expression_spdx = Some("Apache-2.0".to_string());
        let matches = vec![m1, m2];
        let result = determine_spdx_expression(&matches);
        assert!(result.is_ok());
    }

    #[test]
    fn test_determine_spdx_expression_simplifies_absorbed_compound() {
        let mut m1 = create_test_match_full(
            "mit",
            "1-hash",
            1,
            10,
            100.0,
            100,
            100,
            100.0,
            100,
            "mit.LICENSE",
        );
        m1.license_expression_spdx = Some("MIT".to_string());

        let mut m2 = create_test_match_full(
            "mit",
            "1-hash",
            11,
            20,
            100.0,
            100,
            100,
            100.0,
            100,
            "apache.LICENSE",
        );
        m2.license_expression_spdx = Some("MIT OR Apache-2.0".to_string());

        let result = determine_spdx_expression(&[m1, m2]);

        assert_eq!(result.as_deref(), Ok("MIT"));
    }

    #[test]
    fn test_determine_spdx_expression_empty() {
        let matches: Vec<LicenseMatch> = vec![];
        let result = determine_spdx_expression(&matches);
        assert!(result.is_err());
    }

    #[test]
    fn test_is_undetected_license_matches_single_undetected() {
        let mut m = create_test_match(100.0, "mit.LICENSE");
        m.matcher = crate::license_detection::models::MatcherKind::Undetected;
        let matches = vec![m];
        assert!(is_undetected_license_matches(&matches));
    }

    #[test]
    fn test_is_undetected_license_matches_wrong_matcher() {
        let matches = vec![create_test_match(100.0, "mit.LICENSE")];
        assert!(!is_undetected_license_matches(&matches));
    }

    #[test]
    fn test_is_undetected_license_matches_multiple() {
        let mut m1 = create_test_match(100.0, "mit.LICENSE");
        m1.matcher = crate::license_detection::models::MatcherKind::Undetected;
        let mut m2 = create_test_match(100.0, "apache.LICENSE");
        m2.matcher = crate::license_detection::models::MatcherKind::Undetected;
        let matches = vec![m1, m2];
        assert!(is_undetected_license_matches(&matches));
    }

    #[test]
    fn test_is_undetected_license_matches_empty() {
        let matches: Vec<LicenseMatch> = vec![];
        assert!(!is_undetected_license_matches(&matches));
    }

    #[test]
    fn test_analyze_detection_undetected() {
        let mut m = create_test_match(100.0, "mit.LICENSE");
        m.matcher = crate::license_detection::models::MatcherKind::Undetected;
        let matches = vec![m];
        assert_eq!(
            analyze_detection(&matches, false),
            DETECTION_LOG_UNDETECTED_LICENSE
        );
    }

    #[test]
    fn test_analyze_detection_perfect() {
        let m = create_test_match_full(
            "mit",
            "1-hash",
            1,
            10,
            100.0,
            100,
            100,
            100.0,
            100,
            "mit.LICENSE",
        );
        let matches = vec![m];
        assert_eq!(analyze_detection(&matches, false), "");
    }

    #[test]
    fn test_analyze_detection_false_positive() {
        let matches = vec![create_test_match_full(
            "gpl",
            "2-aho",
            2000,
            2005,
            30.0,
            3,
            3,
            30.0,
            50,
            "gpl_bare.LICENSE",
        )];
        assert_eq!(analyze_detection(&matches, false), "false-positive");
    }

    #[test]
    fn test_analyze_detection_unknown_match() {
        let matches = vec![create_test_match(95.0, "unknown.LICENSE")];
        assert_eq!(
            analyze_detection(&matches, false),
            DETECTION_LOG_UNKNOWN_MATCH
        );
    }

    #[test]
    fn test_analyze_detection_imperfect_coverage() {
        let m = create_test_match_full(
            "mit",
            "1-hash",
            1,
            10,
            80.0,
            100,
            100,
            80.0,
            100,
            "mit.LICENSE",
        );
        let matches = vec![m];
        assert_eq!(
            analyze_detection(&matches, false),
            DETECTION_LOG_IMPERFECT_COVERAGE
        );
    }

    #[test]
    fn test_analyze_detection_mixed_clue_and_detection_is_not_license_clues() {
        let mut clue = create_test_match_full(
            "mit",
            "2-aho",
            1,
            3,
            100.0,
            100,
            100,
            100.0,
            100,
            "mit-clue.RULE",
        );
        clue.rule_kind = crate::license_detection::models::RuleKind::Clue;

        let detection = create_test_match_full(
            "mit",
            "1-hash",
            4,
            20,
            100.0,
            100,
            100,
            100.0,
            100,
            "mit.LICENSE",
        );

        let matches = vec![clue, detection];
        assert_eq!(analyze_detection(&matches, false), "");
    }

    #[test]
    fn test_is_unknown_intro_true_with_is_license_intro_flag() {
        let mut m = create_test_match(100.0, "mit.LICENSE");
        m.license_expression = "unknown".to_string();
        m.rule_kind = crate::license_detection::models::RuleKind::Intro;
        assert!(is_unknown_intro(&m));
    }

    #[test]
    fn test_is_unknown_intro_true_with_is_license_clue_flag() {
        let mut m = create_test_match(100.0, "mit.LICENSE");
        m.license_expression = "unknown".to_string();
        m.rule_kind = crate::license_detection::models::RuleKind::Clue;
        assert!(is_unknown_intro(&m));
    }

    #[test]
    fn test_is_unknown_intro_true_with_free_unknown_expression() {
        let mut m = create_test_match(100.0, "mit.LICENSE");
        m.license_expression = "free-unknown".to_string();
        assert!(is_unknown_intro(&m));
    }

    #[test]
    fn test_is_unknown_intro_false_no_unknown_in_expression() {
        let m = create_test_match(100.0, "mit.LICENSE");
        assert!(!is_unknown_intro(&m));
    }

    #[test]
    fn test_is_unknown_intro_false_no_flags_or_free_unknown() {
        let mut m = create_test_match(100.0, "mit.LICENSE");
        m.license_expression = "unknown".to_string();
        m.rule_kind = crate::license_detection::models::RuleKind::None;

        assert!(!is_unknown_intro(&m));
    }

    #[test]
    fn test_is_license_reference_local_file_true() {
        let mut m = create_test_match(100.0, "mit.LICENSE");
        m.referenced_filenames = Some(vec!["LICENSE".to_string()]);
        assert!(is_license_reference_local_file(&m));
    }

    #[test]
    fn test_is_license_reference_local_file_true_multiple() {
        let mut m = create_test_match(100.0, "apache-2.0.COPYING");
        m.referenced_filenames = Some(vec!["COPYING".to_string()]);
        assert!(is_license_reference_local_file(&m));
    }

    #[test]
    fn test_is_license_reference_local_file_false_empty() {
        let m = create_test_match(100.0, "mit.RULE");
        assert!(!is_license_reference_local_file(&m));
    }

    #[test]
    fn test_filter_license_references_filters_matches() {
        let mut m1 = create_test_match(100.0, "mit.LICENSE");
        m1.referenced_filenames = Some(vec!["LICENSE".to_string()]);
        let m2 = create_test_match(100.0, "mit.RULE");
        let filtered = filter_license_references(&[m1, m2]);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_license_references_returns_original_when_empty() {
        let filtered = filter_license_references(&[]);
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_filter_license_references_no_filtering_needed() {
        let m1 = create_test_match(100.0, "mit.RULE");
        let m2 = create_test_match(100.0, "apache.RULE");
        let filtered = filter_license_references(&[m1, m2]);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_license_intros_and_references_filters_both() {
        let mut m1 = create_test_match(100.0, "mit.LICENSE");
        m1.rule_kind = crate::license_detection::models::RuleKind::Intro;
        m1.matcher = MatcherKind::Aho;
        m1.match_coverage = 100.0;
        m1.referenced_filenames = Some(vec!["LICENSE".to_string()]);
        let m2 = create_test_match(100.0, "mit.RULE");
        let filtered = filter_license_intros_and_references(&[m1, m2]);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_has_unknown_intro_before_detection_single_match_returns_false() {
        let m = create_test_match(100.0, "mit.LICENSE");
        let matches = vec![m];
        assert!(!has_unknown_intro_before_detection(&matches));
    }

    #[test]
    fn test_is_license_reference_local_file_false_none() {
        let m = create_test_match(100.0, "mit.RULE");
        assert!(!is_license_reference_local_file(&m));
    }
}
