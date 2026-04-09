//! License detection assembly and grouping logic.
//!
//! This module implements Phase 6 of the license detection pipeline:
//! grouping raw matches into LicenseDetection objects based on proximity

pub mod analysis;
pub mod grouping;
pub mod identifier;
mod types;

pub use grouping::{group_matches_by_region, sort_matches_by_line};
pub(crate) use types::{DetectionGroup, FileRegion, LicenseDetection, UniqueDetection};

use crate::license_detection::models::LicenseMatch;
use crate::license_detection::spdx_mapping::SpdxMapping;
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::license_detection::expression::parse_expression;
use analysis::{
    analyze_detection, classify_detection, compute_detection_score,
    determine_spdx_expression_from_scancode, filter_license_intros,
    has_correct_license_clue_matches,
};
pub(crate) use analysis::{determine_license_expression, determine_spdx_expression};
use identifier::{compute_detection_coverage, compute_detection_identifier};

/// Matches with line gap > this are considered separate groups.
/// Corresponds to Python's LINES_THRESHOLD = 4 (query.py:108)
const LINES_THRESHOLD: usize = 4;

// ============================================================================
// Detection Log Categories (Python parity: DetectionRule enum)
// ============================================================================

/// License clues - low quality matches.
pub const DETECTION_LOG_LICENSE_CLUES: &str = "license-clues";

pub const DETECTION_LOG_FALSE_POSITIVE: &str = "false-positive";

pub const DETECTION_LOG_LOW_QUALITY_MATCH_FRAGMENTS: &str = "low-quality-match-fragments";

pub const DETECTION_LOG_NOT_LICENSE_CLUES_AS_MORE_DETECTIONS_PRESENT: &str =
    "not-license-clues-as-more-detections-present";

/// Imperfect match coverage - at least one match has coverage < 100%.
pub const DETECTION_LOG_IMPERFECT_COVERAGE: &str = "imperfect-match-coverage";

/// Unknown match - matches with unknown license identifiers.
pub const DETECTION_LOG_UNKNOWN_MATCH: &str = "unknown-match";

/// Extra words - match contains extra text beyond the matched rule.
pub const DETECTION_LOG_EXTRA_WORDS: &str = "extra-words";

/// Undetected license - single undetected match (no license found).
pub const DETECTION_LOG_UNDETECTED_LICENSE: &str = "undetected-license";

/// Unknown intro followed by match - license intro followed by proper detection.
pub const DETECTION_LOG_UNKNOWN_INTRO_FOLLOWED_BY_MATCH: &str = "unknown-intro-followed-by-match";

/// Populate LicenseDetection from a DetectionGroup.
///
/// This function:
/// 1. Computes the detection score
/// 2. Determines the license expression
/// 3. Adds appropriate detection log entries
/// 4. Creates the identifier
///
/// Parameter `index` is reserved for future use (e.g., spdx conversion).
pub(crate) fn populate_detection_from_group(
    detection: &mut LicenseDetection,
    group: &DetectionGroup,
    source_text: Option<&str>,
) {
    if group.matches.is_empty() {
        return;
    }

    let log_category = analyze_detection(&group.matches, false);

    let matches_for_expression = select_matches_for_expression(&group.matches, log_category, false);

    detection.matches = group.matches.clone();
    detection.file_regions = collect_file_regions_from_matches(&detection.matches);

    let _score = compute_detection_score(&detection.matches);

    if should_compute_public_expression(log_category)
        && let Ok(expr) = determine_license_expression(&matches_for_expression, source_text)
    {
        detection.license_expression = Some(expr.clone());

        if let Ok(spdx_expr) = determine_spdx_expression(&matches_for_expression, source_text) {
            detection.license_expression_spdx = Some(spdx_expr);
        }
    }

    detection.detection_log.push(log_category.to_string());

    // Compute identifier like Python: detection.identifier = detection.identifier_with_expression
    if detection.license_expression.is_some() {
        detection.identifier = Some(compute_detection_identifier(detection));
    } else {
        detection.identifier = None;
    }
}

fn should_compute_public_expression(log_category: &str) -> bool {
    !matches!(
        log_category,
        DETECTION_LOG_FALSE_POSITIVE
            | DETECTION_LOG_LICENSE_CLUES
            | DETECTION_LOG_LOW_QUALITY_MATCH_FRAGMENTS
    )
}

/// Populate LicenseDetection from a DetectionGroup with SPDX mapping.
///
/// This function:
/// 1. Computes the detection score
/// 2. Determines the ScanCode license expression
/// 3. Determines the SPDX license expression using the mapping
/// 4. Adds appropriate detection log entries
/// 5. Creates the identifier
///
/// # Arguments
///
/// * `detection` - LicenseDetection to populate
/// * `group` - DetectionGroup containing the matches
/// * `spdx_mapping` - SpdxMapping for SPDX conversion
pub(crate) fn populate_detection_from_group_with_spdx(
    detection: &mut LicenseDetection,
    group: &DetectionGroup,
    spdx_mapping: &SpdxMapping,
    source_text: Option<&str>,
) {
    populate_detection_from_group(detection, group, source_text);

    for match_item in &mut detection.matches {
        if match_item.license_expression_spdx.is_none()
            && let Ok(spdx_expr) = determine_spdx_expression_from_scancode(
                &match_item.license_expression,
                spdx_mapping,
            )
        {
            match_item.license_expression_spdx = Some(spdx_expr);
        }
    }

    if let Some(ref scancode_expr) = detection.license_expression
        && let Ok(spdx_expr) = determine_spdx_expression_from_scancode(scancode_expr, spdx_mapping)
    {
        detection.license_expression_spdx = Some(spdx_expr);
    }
}

/// Create a basic LicenseDetection from a DetectionGroup.
///
/// This function properly populates all detection fields using the
/// analysis logic defined in populate_detection_from_group.
///
/// # Arguments
///
/// * `group` - DetectionGroup containing the matches
///
/// # Returns
///
/// A fully populated LicenseDetection
#[cfg(test)]
fn create_detection_from_group(group: &DetectionGroup) -> LicenseDetection {
    let mut detection = LicenseDetection {
        license_expression: None,
        license_expression_spdx: None,
        matches: Vec::new(),
        detection_log: Vec::new(),
        identifier: None,
        file_regions: Vec::new(),
    };

    if group.matches.is_empty() {
        return detection;
    }

    populate_detection_from_group(&mut detection, group, None);

    detection
}

pub(crate) fn empty_detection() -> LicenseDetection {
    LicenseDetection {
        license_expression: None,
        license_expression_spdx: None,
        matches: Vec::new(),
        detection_log: Vec::new(),
        identifier: None,
        file_regions: Vec::new(),
    }
}

pub(crate) fn attach_source_path_to_detections(
    detections: &mut [LicenseDetection],
    source_path: &str,
) {
    for detection in detections {
        for match_item in &mut detection.matches {
            if match_item.from_file.is_none() {
                match_item.from_file = Some(source_path.to_string());
            }
        }
        detection.file_regions = collect_file_regions_from_matches(&detection.matches);
    }
}

type SeenRegionKey = (String, usize, usize);
type UniqueDetectionEntry = (UniqueDetection, HashSet<SeenRegionKey>);

pub(crate) fn get_unique_detections(detections: &[LicenseDetection]) -> Vec<UniqueDetection> {
    let mut detections_by_identifier: BTreeMap<String, UniqueDetectionEntry> = BTreeMap::new();

    for detection in detections {
        let Some(identifier) = detection.identifier.as_ref() else {
            continue;
        };

        let (entry, seen_regions) = detections_by_identifier
            .entry(identifier.clone())
            .or_insert_with(|| {
                (
                    UniqueDetection {
                        identifier: identifier.clone(),
                        file_regions: Vec::new(),
                    },
                    HashSet::new(),
                )
            });

        for region in &detection.file_regions {
            let key = (
                region.path.clone(),
                region.start_line.get(),
                region.end_line.get(),
            );
            if seen_regions.insert(key) {
                entry.file_regions.push(region.clone());
            }
        }
    }

    detections_by_identifier
        .into_values()
        .map(|(unique_detection, _)| unique_detection)
        .collect()
}

fn collect_file_regions_from_matches(matches: &[LicenseMatch]) -> Vec<FileRegion> {
    let Some(first_path) = matches
        .iter()
        .find_map(|match_item| match_item.from_file.clone())
    else {
        return Vec::new();
    };

    let start_line = matches.iter().map(|match_item| match_item.start_line).min();
    let end_line = matches.iter().map(|match_item| match_item.end_line).max();

    match (start_line, end_line) {
        (Some(start_line), Some(end_line)) => vec![FileRegion {
            path: first_path,
            start_line,
            end_line,
        }],
        _ => Vec::new(),
    }
}

fn attach_aggregated_file_regions(detections: &mut [LicenseDetection]) {
    let unique_regions: HashMap<_, _> = get_unique_detections(detections)
        .into_iter()
        .map(|unique| (unique.identifier, unique.file_regions))
        .collect();

    for detection in detections {
        if let Some(identifier) = detection.identifier.as_ref()
            && let Some(file_regions) = unique_regions.get(identifier)
        {
            detection.file_regions = file_regions.clone();
        }
    }
}

pub(crate) fn select_matches_for_expression(
    matches: &[crate::license_detection::models::LicenseMatch],
    log_category: &str,
    post_scan: bool,
) -> Vec<crate::license_detection::models::LicenseMatch> {
    let mut filtered = if log_category == DETECTION_LOG_UNKNOWN_INTRO_FOLLOWED_BY_MATCH {
        filter_license_intros(matches)
    } else {
        matches.to_vec()
    };

    if post_scan && is_unknown_reference_follow_log(log_category) {
        let has_concrete_match = filtered
            .iter()
            .any(|match_item| !is_unknown_reference_like_match(match_item));
        if has_concrete_match {
            filtered.retain(|match_item| !is_unknown_reference_like_match(match_item));
        }

        let local_reference_sources = filtered
            .iter()
            .filter(|match_item| is_local_file_reference_match(match_item))
            .filter_map(|match_item| match_item.from_file.as_deref())
            .collect::<HashSet<_>>();
        let has_resolved_concrete_match = !local_reference_sources.is_empty()
            && filtered.iter().any(|match_item| {
                !is_local_file_reference_match(match_item)
                    && !is_unknown_reference_like_match(match_item)
                    && match_item
                        .from_file
                        .as_deref()
                        .is_some_and(|path| !local_reference_sources.contains(path))
            });
        if has_resolved_concrete_match {
            let without_imperfect_reference_fragments: Vec<_> = filtered
                .iter()
                .filter(|match_item| !is_imperfect_local_file_reference_match(match_item))
                .cloned()
                .collect();
            if !without_imperfect_reference_fragments.is_empty() {
                filtered = without_imperfect_reference_fragments;
            }
        }
    }

    if filtered.is_empty() {
        matches.to_vec()
    } else {
        filtered
    }
}

fn is_local_file_reference_match(
    match_item: &crate::license_detection::models::LicenseMatch,
) -> bool {
    match_item
        .referenced_filenames
        .as_ref()
        .is_some_and(|filenames| !filenames.is_empty())
}

fn is_imperfect_local_file_reference_match(
    match_item: &crate::license_detection::models::LicenseMatch,
) -> bool {
    is_local_file_reference_match(match_item) && match_item.coverage() < 100.0
}

fn is_unknown_reference_follow_log(log_category: &str) -> bool {
    matches!(
        log_category,
        "unknown-reference-to-local-file"
            | "unknown-reference-in-file-to-package"
            | "unknown-reference-in-file-to-nonexistent-package"
    )
}

fn is_unknown_reference_like_match(
    match_item: &crate::license_detection::models::LicenseMatch,
) -> bool {
    matches!(
        match_item.license_expression.as_str(),
        "unknown-license-reference" | "free-unknown"
    )
}

/// Filter detections by minimum score threshold.
///
/// Returns only detections with score >= min_score threshold.
///
/// Based on Python minimum score filtering in detection pipeline.
pub fn filter_detections_by_score(
    detections: Vec<LicenseDetection>,
    min_score: f32,
) -> Vec<LicenseDetection> {
    detections
        .into_iter()
        .filter(|detection| classify_detection(detection, min_score))
        .collect()
}

/// Remove duplicate detections (same identifier).
///
/// Groups detections by their identifier (license expression + content hash).
/// Detections with the same identifier represent the same license at the same
/// location. Detections with the same expression but different identifiers
/// represent the same license at DIFFERENT locations and should be kept separate.
///
/// Based on Python get_detections_by_id behavior in detection.py.
#[cfg(test)]
pub fn remove_duplicate_detections(detections: Vec<LicenseDetection>) -> Vec<LicenseDetection> {
    let mut detections_by_id: std::collections::HashMap<String, LicenseDetection> =
        std::collections::HashMap::new();

    for detection in detections {
        let identifier = detection
            .identifier
            .clone()
            .unwrap_or_else(|| compute_detection_identifier(&detection));

        let entry = detections_by_id.entry(identifier.clone());
        if let std::collections::hash_map::Entry::Vacant(e) = entry {
            let mut detection = detection;
            detection.identifier = Some(identifier);
            e.insert(detection);
        }
    }

    detections_by_id.into_values().collect()
}

/// Rank detections by score and coverage.
///
/// Sorts detections in descending order by:
/// 1. Detection score (higher is better)
/// 2. Detection coverage (higher is better)
///
/// Based on Python: sort_unique_detections() at detection.py:1003
pub fn rank_detections(mut detections: Vec<LicenseDetection>) -> Vec<LicenseDetection> {
    detections.sort_by(|a, b| {
        let score_a = compute_detection_score(&a.matches);
        let score_b = compute_detection_score(&b.matches);
        let coverage_a = compute_detection_coverage(&a.matches);
        let coverage_b = compute_detection_coverage(&b.matches);

        score_b
            .partial_cmp(&score_a)
            .unwrap()
            .then_with(|| coverage_b.partial_cmp(&coverage_a).unwrap())
            .then_with(|| a.identifier.cmp(&b.identifier))
    });

    detections
}

/// Sort detections by minimum line number (earliest match first).
///
/// This matches Python's qstart ordering, ensuring detections
/// earlier in the file come first in the results.
pub fn sort_detections_by_line(mut detections: Vec<LicenseDetection>) -> Vec<LicenseDetection> {
    detections.sort_by(|a, b| {
        let min_line_a = a
            .matches
            .iter()
            .map(|m| m.start_line)
            .min()
            .map(|ln| ln.get())
            .unwrap_or(0);
        let min_line_b = b
            .matches
            .iter()
            .map(|m| m.start_line)
            .min()
            .map(|ln| ln.get())
            .unwrap_or(0);
        min_line_a
            .cmp(&min_line_b)
            .then_with(|| a.identifier.cmp(&b.identifier))
    });
    detections
}

pub fn apply_detection_preferences(detections: Vec<LicenseDetection>) -> Vec<LicenseDetection> {
    detections
}

/// Main post-processing function for detections.
///
/// Applies the following steps in order:
/// 1. Filter detections by minimum score threshold
/// 2. Remove duplicate detections (same license expression)
/// 3. Apply detection preferences (based on matcher type)
/// 4. Rank detections by score and coverage
///
/// This is the main entry point for post-processing after all detections
/// have been created.
///
/// # Arguments
///
/// * `detections` - Raw detections from detection grouping
/// * `min_score` - Minimum score threshold (default: 90.0)
///
/// # Returns
///
/// Post-processed and ranked list of detections
pub fn post_process_detections(
    detections: Vec<LicenseDetection>,
    min_score: f32,
) -> Vec<LicenseDetection> {
    let filtered = filter_detections_by_score(detections, min_score);
    let promoted = promote_non_clue_no_expression_detections(filtered);
    // NOTE: We do NOT call remove_duplicate_detections here.
    //
    // Python's get_unique_detections() groups detections by identifier and creates
    // UniqueDetection objects with aggregated file_regions, but it does NOT remove
    // detections. The Python test infrastructure uses idx.match() which returns
    // raw matches without any deduplication.
    //
    // Calling remove_duplicate_detections would incorrectly merge detections that
    // have the same license expression at different locations (e.g., two bsd-new
    // licenses in different parts of a file). The identifier is computed from
    // license_expression + rule_identifier + score + matched_text_tokens, which
    // would be identical for same-license texts at different locations.
    //
    // TODO: Implement UniqueDetection with file_regions aggregation for output
    // formatting when we add full ScanCode output compatibility.
    let preferred = apply_detection_preferences(promoted);
    let ranked = rank_detections(preferred);
    let mut sorted = sort_detections_by_line(ranked);
    attach_aggregated_file_regions(&mut sorted);
    sorted
}

fn promote_non_clue_no_expression_detections(
    mut detections: Vec<LicenseDetection>,
) -> Vec<LicenseDetection> {
    if detections.len() <= 1 {
        return detections;
    }

    let detected_license_keys = detections
        .iter()
        .filter_map(|detection| detection.license_expression.as_deref())
        .flat_map(license_keys_from_expression)
        .collect::<std::collections::HashSet<_>>();

    for detection in &mut detections {
        if detection.license_expression.is_some()
            || has_correct_license_clue_matches(&detection.matches)
        {
            continue;
        }

        let Some(license_expression) = promoted_expression_from_matches(&detection.matches) else {
            continue;
        };
        let license_keys = license_keys_from_expression(&license_expression);

        if !license_keys.is_empty()
            && license_keys
                .iter()
                .all(|key| detected_license_keys.contains(key))
        {
            detection.license_expression = Some(license_expression.clone());
            detection.license_expression_spdx =
                determine_spdx_expression(&detection.matches, None).ok();
            detection
                .detection_log
                .push(DETECTION_LOG_NOT_LICENSE_CLUES_AS_MORE_DETECTIONS_PRESENT.to_string());
            detection.identifier = Some(compute_detection_identifier(detection));
        }
    }

    detections
}

fn promoted_expression_from_matches(
    matches: &[crate::license_detection::models::LicenseMatch],
) -> Option<String> {
    crate::utils::spdx::combine_license_expressions(
        matches
            .iter()
            .map(|match_item| match_item.license_expression.clone()),
    )
}

fn license_keys_from_expression(expression: &str) -> Vec<String> {
    parse_expression(expression)
        .map(|parsed| parsed.license_keys())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::identifier::compute_detection_identifier;
    use super::*;
    use crate::license_detection::models::{License, LicenseMatch, MatchCoordinates, PositionSpan};
    use crate::license_detection::spdx_mapping::build_spdx_mapping;
    use crate::models::LineNumber;

    fn create_test_match(
        start_line: usize,
        end_line: usize,
        matcher: &str,
        rule_identifier: &str,
    ) -> LicenseMatch {
        let start_line_ln = LineNumber::new(start_line).expect("valid start_line");
        let end_line_ln = LineNumber::new(end_line).expect("valid end_line");
        LicenseMatch {
            rid: 0,
            license_expression: "mit".to_string(),
            license_expression_spdx: Some("MIT".to_string()),
            from_file: Some("test.txt".to_string()),
            start_line: start_line_ln,
            end_line: end_line_ln,
            start_token: start_line,
            end_token: end_line + 1,
            matcher: matcher.parse().expect("invalid test matcher"),
            score: 95.0,
            matched_length: 100,
            match_coverage: 95.0,
            rule_relevance: 100,
            rule_identifier: rule_identifier.to_string(),
            rule_url: "https://example.com".to_string(),
            matched_text: Some("MIT License".to_string()),
            referenced_filenames: None,
            rule_kind: crate::license_detection::models::RuleKind::None,
            is_from_license: false,
            rule_length: 100,
            rule_start_token: 0,
            coordinates: MatchCoordinates::query_region(PositionSpan::range(
                start_line,
                end_line + 1,
            )),
            candidate_resemblance: 0.0,
            candidate_containment: 0.0,
        }
    }

    fn create_perfect_match(start_line: usize, end_line: usize) -> LicenseMatch {
        let mut m = create_test_match(start_line, end_line, "1-hash", "mit.LICENSE");
        m.match_coverage = 100.0;
        m.score = 100.0;
        m
    }

    #[test]
    fn select_matches_for_expression_keeps_local_file_references_in_main_detection() {
        let mut referenced = create_test_match(1, 3, "3-seq", "apache-2.0_910.RULE");
        referenced.license_expression = "apache-2.0".to_string();
        referenced.license_expression_spdx = Some("Apache-2.0".to_string());
        referenced.referenced_filenames = Some(vec!["LICENSE-APACHE".to_string()]);

        let mut disclaimer = create_test_match(4, 6, "3-seq", "warranty-disclaimer_18.RULE");
        disclaimer.license_expression = "warranty-disclaimer".to_string();
        disclaimer.license_expression_spdx =
            Some("LicenseRef-scancode-warranty-disclaimer".to_string());

        let selected = select_matches_for_expression(
            &[referenced.clone(), disclaimer.clone()],
            "unknown-reference-to-local-file",
            false,
        );

        assert_eq!(selected, vec![referenced, disclaimer]);
    }

    #[test]
    fn select_matches_for_expression_keeps_local_file_references_during_post_scan() {
        let mut referenced = create_test_match(1, 3, "3-seq", "apache-2.0_910.RULE");
        referenced.license_expression = "apache-2.0".to_string();
        referenced.license_expression_spdx = Some("Apache-2.0".to_string());
        referenced.referenced_filenames = Some(vec!["LICENSE-APACHE".to_string()]);

        let mut disclaimer = create_test_match(4, 6, "3-seq", "warranty-disclaimer_18.RULE");
        disclaimer.license_expression = "warranty-disclaimer".to_string();
        disclaimer.license_expression_spdx =
            Some("LicenseRef-scancode-warranty-disclaimer".to_string());

        let selected = select_matches_for_expression(
            &[referenced.clone(), disclaimer.clone()],
            "unknown-reference-to-local-file",
            true,
        );

        assert_eq!(selected, vec![referenced, disclaimer]);
    }

    #[test]
    fn select_matches_for_expression_drops_unknown_reference_placeholders_during_post_scan() {
        let mut unknown_reference = create_test_match(
            1,
            1,
            "2-aho",
            "unknown-license-reference_see_license_1.RULE",
        );
        unknown_reference.license_expression = "unknown-license-reference".to_string();
        unknown_reference.license_expression_spdx =
            Some("LicenseRef-scancode-unknown-license-reference".to_string());
        unknown_reference.referenced_filenames = Some(vec!["LICENSE".to_string()]);

        let mut referenced = create_test_match(2, 20, "1-hash", "mit.LICENSE");
        referenced.license_expression = "mit".to_string();
        referenced.license_expression_spdx = Some("MIT".to_string());
        referenced.from_file = Some("LICENSE".to_string());

        let selected = select_matches_for_expression(
            &[unknown_reference, referenced.clone()],
            "unknown-reference-to-local-file",
            true,
        );

        assert_eq!(selected, vec![referenced]);
    }

    #[test]
    fn select_matches_for_expression_drops_free_unknown_package_placeholder_during_post_scan() {
        let mut unknown_reference = create_test_match(1, 1, "2-aho", "free-unknown-package_1.RULE");
        unknown_reference.license_expression = "free-unknown".to_string();
        unknown_reference.license_expression_spdx =
            Some("LicenseRef-scancode-free-unknown".to_string());
        unknown_reference.referenced_filenames =
            Some(vec!["INHERIT_LICENSE_FROM_PACKAGE".to_string()]);

        let mut referenced = create_test_match(2, 2, "1-hash", "bsd-new_195.RULE");
        referenced.license_expression = "bsd-new".to_string();
        referenced.license_expression_spdx = Some("BSD-3-Clause".to_string());
        referenced.from_file = Some("PKG-INFO".to_string());

        let selected = select_matches_for_expression(
            &[unknown_reference, referenced.clone()],
            "unknown-reference-in-file-to-package",
            true,
        );

        assert_eq!(selected, vec![referenced]);
    }

    #[test]
    fn select_matches_for_expression_drops_imperfect_local_reference_fragments_during_post_scan() {
        let mut referenced_notice = create_test_match(1, 8, "3-seq", "gpl-1.0-plus_or_mit_2.RULE");
        referenced_notice.license_expression = "gpl-1.0-plus OR mit".to_string();
        referenced_notice.license_expression_spdx = Some("GPL-1.0-or-later OR MIT".to_string());
        referenced_notice.referenced_filenames = Some(vec!["LICENSE".to_string()]);
        referenced_notice.match_coverage = 41.79;
        referenced_notice.score = 41.79;

        let mut referenced_license =
            create_test_match(1, 582, "1-hash", "npsl-exception-0.95.LICENSE");
        referenced_license.license_expression = "npsl-exception-0.95".to_string();
        referenced_license.license_expression_spdx =
            Some("LicenseRef-scancode-npsl-exception-0.95".to_string());
        referenced_license.from_file = Some("LICENSE".to_string());
        referenced_license.match_coverage = 100.0;
        referenced_license.score = 100.0;

        let selected = select_matches_for_expression(
            &[referenced_notice, referenced_license.clone()],
            "unknown-reference-to-local-file",
            true,
        );

        assert_eq!(selected, vec![referenced_license]);
    }

    fn create_test_license() -> License {
        License {
            key: "mit".to_string(),
            short_name: Some("MIT".to_string()),
            name: "MIT License".to_string(),
            language: Some("en".to_string()),
            spdx_license_key: Some("MIT".to_string()),
            other_spdx_license_keys: vec![],
            category: Some("Permissive".to_string()),
            owner: None,
            homepage_url: None,
            text: "MIT License".to_string(),
            reference_urls: vec![],
            osi_license_key: Some("MIT".to_string()),
            text_urls: vec![],
            osi_url: None,
            faq_url: None,
            other_urls: vec![],
            notes: None,
            is_deprecated: false,
            is_exception: false,
            is_unknown: false,
            is_generic: false,
            replaced_by: vec![],
            minimum_coverage: None,
            standard_notice: None,
            ignorable_copyrights: None,
            ignorable_holders: None,
            ignorable_authors: None,
            ignorable_urls: None,
            ignorable_emails: None,
        }
    }

    #[test]
    fn test_create_detection_from_group_empty() {
        let group = DetectionGroup::new(Vec::new());
        let detection = create_detection_from_group(&group);
        assert!(detection.matches.is_empty());
        assert!(detection.license_expression.is_none());
    }

    #[test]
    fn test_create_detection_from_group_with_matches() {
        let match1 = create_perfect_match(1, 10);
        let group = DetectionGroup::new(vec![match1]);
        let detection = create_detection_from_group(&group);
        assert_eq!(detection.matches.len(), 1);
        assert!(detection.license_expression.is_some());
    }

    #[test]
    fn test_populate_detection_from_group_perfect() {
        let mut m = create_perfect_match(1, 10);
        m.match_coverage = 100.0;
        let group = DetectionGroup::new(vec![m]);
        let mut detection = LicenseDetection {
            license_expression: None,
            license_expression_spdx: None,
            matches: Vec::new(),
            detection_log: Vec::new(),
            identifier: None,
            file_regions: Vec::new(),
        };
        populate_detection_from_group(&mut detection, &group, None);
        assert_eq!(detection.matches.len(), 1);
        assert!(detection.license_expression.is_some());
        assert!(
            detection.detection_log.contains(&"".to_string()) || detection.detection_log.is_empty(),
            "Perfect detection has empty log"
        );
    }

    #[test]
    fn test_populate_detection_from_group_empty() {
        let group = DetectionGroup::new(Vec::new());
        let mut detection = LicenseDetection {
            license_expression: None,
            license_expression_spdx: None,
            matches: Vec::new(),
            detection_log: Vec::new(),
            identifier: None,
            file_regions: Vec::new(),
        };
        populate_detection_from_group(&mut detection, &group, None);
        assert!(detection.matches.is_empty());
        assert!(detection.license_expression.is_none());
    }

    #[test]
    fn test_populate_detection_from_group_false_positive() {
        let mut m = create_test_match(2000, 2005, "2-aho", "gpl_bare.LICENSE");
        m.rule_relevance = 50;
        m.score = 30.0;
        m.match_coverage = 30.0;
        m.rule_length = 3;
        let group = DetectionGroup::new(vec![m]);
        let mut detection = LicenseDetection {
            license_expression: None,
            license_expression_spdx: None,
            matches: Vec::new(),
            detection_log: Vec::new(),
            identifier: None,
            file_regions: Vec::new(),
        };
        populate_detection_from_group(&mut detection, &group, None);
        assert!(
            detection
                .detection_log
                .contains(&DETECTION_LOG_FALSE_POSITIVE.to_string())
        );
        assert!(detection.license_expression.is_none());
        assert!(detection.identifier.is_none());
    }

    #[test]
    fn test_populate_detection_from_group_license_clues_have_no_expression() {
        let mut m = create_perfect_match(1, 2);
        m.rule_kind = crate::license_detection::models::RuleKind::Clue;
        let group = DetectionGroup::new(vec![m]);
        let mut detection = LicenseDetection {
            license_expression: None,
            license_expression_spdx: None,
            matches: Vec::new(),
            detection_log: Vec::new(),
            identifier: None,
            file_regions: Vec::new(),
        };

        populate_detection_from_group(&mut detection, &group, None);

        assert!(
            detection
                .detection_log
                .contains(&DETECTION_LOG_LICENSE_CLUES.to_string())
        );
        assert!(detection.license_expression.is_none());
        assert!(detection.license_expression_spdx.is_none());
        assert!(detection.identifier.is_none());
    }

    #[test]
    fn test_populate_detection_from_group_low_quality_matches_have_no_expression() {
        let mut m = create_test_match(1, 3, "2-aho", "mit.LICENSE");
        m.match_coverage = 50.0;
        m.score = 50.0;
        let group = DetectionGroup::new(vec![m]);
        let mut detection = LicenseDetection {
            license_expression: None,
            license_expression_spdx: None,
            matches: Vec::new(),
            detection_log: Vec::new(),
            identifier: None,
            file_regions: Vec::new(),
        };

        populate_detection_from_group(&mut detection, &group, None);

        assert!(
            detection
                .detection_log
                .contains(&DETECTION_LOG_LOW_QUALITY_MATCH_FRAGMENTS.to_string())
        );
        assert!(detection.license_expression.is_none());
        assert!(detection.license_expression_spdx.is_none());
        assert!(detection.identifier.is_none());
    }

    #[test]
    fn test_populate_detection_from_group_with_spdx_perfect() {
        let mut m = create_perfect_match(1, 10);
        m.license_expression = "mit".to_string();
        m.license_expression_spdx = Some("MIT".to_string());
        let group = DetectionGroup::new(vec![m]);
        let licenses = vec![create_test_license()];
        let spdx_mapping = build_spdx_mapping(&licenses);
        let mut detection = LicenseDetection {
            license_expression: None,
            license_expression_spdx: None,
            matches: Vec::new(),
            detection_log: Vec::new(),
            identifier: None,
            file_regions: Vec::new(),
        };
        populate_detection_from_group_with_spdx(&mut detection, &group, &spdx_mapping, None);
        assert!(detection.license_expression_spdx.is_some());
    }

    #[test]
    fn test_populate_detection_from_group_with_spdx_empty() {
        let group = DetectionGroup::new(Vec::new());
        let licenses = vec![create_test_license()];
        let spdx_mapping = build_spdx_mapping(&licenses);
        let mut detection = LicenseDetection {
            license_expression: None,
            license_expression_spdx: None,
            matches: Vec::new(),
            detection_log: Vec::new(),
            identifier: None,
            file_regions: Vec::new(),
        };
        populate_detection_from_group_with_spdx(&mut detection, &group, &spdx_mapping, None);
        assert!(detection.matches.is_empty());
    }

    #[test]
    fn test_filter_detections_by_score_all_pass() {
        let mut detection = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![create_perfect_match(1, 10)],
            detection_log: vec!["perfect-detection".to_string()],
            identifier: None,
            file_regions: Vec::new(),
        };
        detection.identifier = Some(compute_detection_identifier(&detection));
        let filtered = filter_detections_by_score(vec![detection], 0.0);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_detections_by_score_some_filtered() {
        let mut d1 = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![create_perfect_match(1, 10)],
            detection_log: vec!["perfect-detection".to_string()],
            identifier: None,
            file_regions: Vec::new(),
        };
        d1.identifier = Some(compute_detection_identifier(&d1));

        let mut m = create_test_match(1, 10, "2-aho", "gpl_bare.LICENSE");
        m.rule_relevance = 50;
        m.score = 30.0;
        m.match_coverage = 30.0;
        let mut d2 = LicenseDetection {
            license_expression: Some("gpl".to_string()),
            license_expression_spdx: Some("GPL".to_string()),
            matches: vec![m],
            detection_log: vec![],
            identifier: None,
            file_regions: Vec::new(),
        };
        d2.identifier = Some(compute_detection_identifier(&d2));

        let filtered = filter_detections_by_score(vec![d1, d2], 50.0);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_detections_by_score_all_filtered() {
        let mut m = create_test_match(1, 10, "2-aho", "gpl_bare.LICENSE");
        m.rule_relevance = 50;
        m.score = 30.0;
        m.match_coverage = 30.0;
        let mut detection = LicenseDetection {
            license_expression: Some("gpl".to_string()),
            license_expression_spdx: Some("GPL".to_string()),
            matches: vec![m],
            detection_log: vec![],
            identifier: None,
            file_regions: Vec::new(),
        };
        detection.identifier = Some(compute_detection_identifier(&detection));
        let filtered = filter_detections_by_score(vec![detection], 50.0);
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_filter_detections_by_score_empty() {
        let filtered = filter_detections_by_score(vec![], 0.0);
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_remove_duplicate_detections_different_expressions() {
        let d1 = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![create_perfect_match(1, 10)],
            detection_log: vec![],
            identifier: Some("mit-abc123".to_string()),
            file_regions: Vec::new(),
        };
        let d2 = LicenseDetection {
            license_expression: Some("apache-2.0".to_string()),
            license_expression_spdx: Some("Apache-2.0".to_string()),
            matches: vec![create_perfect_match(20, 30)],
            detection_log: vec![],
            identifier: Some("apache-abc123".to_string()),
            file_regions: Vec::new(),
        };
        let result = remove_duplicate_detections(vec![d1, d2]);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_remove_duplicate_detections_same_expression_different_identifier() {
        let d1 = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![create_perfect_match(1, 10)],
            detection_log: vec![],
            identifier: Some("mit-abc123".to_string()),
            file_regions: Vec::new(),
        };
        let d2 = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![create_perfect_match(20, 30)],
            detection_log: vec![],
            identifier: Some("mit-def456".to_string()),
            file_regions: Vec::new(),
        };
        let result = remove_duplicate_detections(vec![d1, d2]);
        assert_eq!(
            result.len(),
            2,
            "Different identifiers should not be deduplicated"
        );
    }

    #[test]
    fn test_remove_duplicate_detections_empty() {
        let result = remove_duplicate_detections(vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_rank_detections_by_score() {
        let mut d1 = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![create_perfect_match(1, 10)],
            detection_log: vec![],
            identifier: None,
            file_regions: Vec::new(),
        };
        let mut d2 = LicenseDetection {
            license_expression: Some("apache-2.0".to_string()),
            license_expression_spdx: Some("Apache-2.0".to_string()),
            matches: vec![{
                let mut m = create_test_match(20, 30, "1-hash", "apache.LICENSE");
                m.score = 80.0;
                m
            }],
            detection_log: vec![],
            identifier: None,
            file_regions: Vec::new(),
        };
        d1.identifier = Some(compute_detection_identifier(&d1));
        d2.identifier = Some(compute_detection_identifier(&d2));
        let ranked = rank_detections(vec![d2, d1]);
        assert_eq!(ranked[0].license_expression, Some("mit".to_string()));
    }

    #[test]
    fn test_rank_detections_by_coverage_when_scores_equal() {
        let mut m1 = create_test_match(1, 10, "1-hash", "mit.LICENSE");
        m1.score = 90.0;
        m1.match_coverage = 100.0;
        let mut d1 = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![m1],
            detection_log: vec![],
            identifier: None,
            file_regions: Vec::new(),
        };
        let mut m2 = create_test_match(20, 30, "1-hash", "apache.LICENSE");
        m2.score = 90.0;
        m2.match_coverage = 80.0;
        let mut d2 = LicenseDetection {
            license_expression: Some("apache-2.0".to_string()),
            license_expression_spdx: Some("Apache-2.0".to_string()),
            matches: vec![m2],
            detection_log: vec![],
            identifier: None,
            file_regions: Vec::new(),
        };
        d1.identifier = Some(compute_detection_identifier(&d1));
        d2.identifier = Some(compute_detection_identifier(&d2));
        let ranked = rank_detections(vec![d2, d1]);
        assert_eq!(
            ranked[0].license_expression,
            Some("mit".to_string()),
            "Higher coverage should rank first"
        );
    }

    #[test]
    fn test_rank_detections_empty() {
        let result = rank_detections(vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_compute_detection_identifier_deterministic() {
        let m = create_perfect_match(1, 10);
        let d1 = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![m.clone()],
            detection_log: vec![],
            identifier: None,
            file_regions: Vec::new(),
        };
        let d2 = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![m],
            detection_log: vec![],
            identifier: None,
            file_regions: Vec::new(),
        };
        let id1 = compute_detection_identifier(&d1);
        let id2 = compute_detection_identifier(&d2);
        assert_eq!(id1, id2, "Same content should produce same identifier");
    }

    #[test]
    fn test_compute_detection_identifier_different_content() {
        let m1 = create_perfect_match(1, 10);
        let m2 = create_perfect_match(20, 30);
        let d1 = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![m1],
            detection_log: vec![],
            identifier: None,
            file_regions: Vec::new(),
        };
        let d2 = LicenseDetection {
            license_expression: Some("apache-2.0".to_string()),
            license_expression_spdx: Some("Apache-2.0".to_string()),
            matches: vec![m2],
            detection_log: vec![],
            identifier: None,
            file_regions: Vec::new(),
        };
        let id1 = compute_detection_identifier(&d1);
        let id2 = compute_detection_identifier(&d2);
        assert_ne!(
            id1, id2,
            "Different content should produce different identifiers"
        );
    }

    #[test]
    fn test_apply_detection_preferences_preserves_all_detections() {
        let d1 = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![create_perfect_match(1, 10)],
            detection_log: vec![],
            identifier: Some("mit-abc123".to_string()),
            file_regions: Vec::new(),
        };
        let d2 = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![create_perfect_match(20, 30)],
            detection_log: vec![],
            identifier: Some("mit-def456".to_string()),
            file_regions: Vec::new(),
        };
        let result = apply_detection_preferences(vec![d1, d2]);
        assert_eq!(
            result.len(),
            2,
            "Detections with same expression but different identifiers should be kept separate"
        );
    }

    #[test]
    fn test_apply_detection_preferences_different_expressions() {
        let d1 = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![create_perfect_match(1, 10)],
            detection_log: vec![],
            identifier: Some("mit-abc123".to_string()),
            file_regions: Vec::new(),
        };
        let d2 = LicenseDetection {
            license_expression: Some("apache-2.0".to_string()),
            license_expression_spdx: Some("Apache-2.0".to_string()),
            matches: vec![create_perfect_match(20, 30)],
            detection_log: vec![],
            identifier: Some("apache-abc123".to_string()),
            file_regions: Vec::new(),
        };
        let result = apply_detection_preferences(vec![d1, d2]);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_apply_detection_preferences_empty() {
        let result = apply_detection_preferences(vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_post_process_detections_full_pipeline() {
        let m = create_perfect_match(1, 10);
        let mut d = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![m],
            detection_log: vec!["perfect-detection".to_string()],
            identifier: None,
            file_regions: Vec::new(),
        };
        d.identifier = Some(compute_detection_identifier(&d));
        let result = post_process_detections(vec![d], 0.0);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_post_process_detections_all_filtered() {
        let mut m = create_test_match(1, 10, "2-aho", "gpl_bare.LICENSE");
        m.rule_relevance = 50;
        m.score = 30.0;
        m.match_coverage = 30.0;
        let mut d = LicenseDetection {
            license_expression: Some("gpl".to_string()),
            license_expression_spdx: Some("GPL".to_string()),
            matches: vec![m],
            detection_log: vec![],
            identifier: None,
            file_regions: Vec::new(),
        };
        d.identifier = Some(compute_detection_identifier(&d));
        let result = post_process_detections(vec![d], 50.0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_post_process_detections_empty() {
        let result = post_process_detections(vec![], 0.0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_post_process_detections_promotes_covered_low_quality_detection() {
        let mut proper_match = create_perfect_match(10, 30);
        proper_match.license_expression = "bsd-new".to_string();
        proper_match.license_expression_spdx = Some("BSD-3-Clause".to_string());

        let mut low_quality_match = create_test_match(31, 36, "3-seq", "bsd-new_1319.RULE");
        low_quality_match.license_expression = "bsd-new".to_string();
        low_quality_match.license_expression_spdx = Some("BSD-3-Clause".to_string());
        low_quality_match.match_coverage = 32.96;
        low_quality_match.score = 32.96;

        let proper = LicenseDetection {
            license_expression: Some("bsd-new".to_string()),
            license_expression_spdx: Some("BSD-3-Clause".to_string()),
            matches: vec![proper_match],
            detection_log: vec![],
            identifier: Some("bsd_new-proper".to_string()),
            file_regions: Vec::new(),
        };
        let low_quality = LicenseDetection {
            license_expression: None,
            license_expression_spdx: None,
            matches: vec![low_quality_match],
            detection_log: vec![DETECTION_LOG_LOW_QUALITY_MATCH_FRAGMENTS.to_string()],
            identifier: None,
            file_regions: Vec::new(),
        };

        let result = post_process_detections(vec![proper, low_quality], 0.0);
        let promoted = result
            .iter()
            .find(|detection| {
                detection.detection_log.contains(
                    &DETECTION_LOG_NOT_LICENSE_CLUES_AS_MORE_DETECTIONS_PRESENT.to_string(),
                )
            })
            .expect("promoted detection");

        assert_eq!(promoted.license_expression.as_deref(), Some("bsd-new"));
        assert_eq!(
            promoted.license_expression_spdx.as_deref(),
            Some("BSD-3-Clause")
        );
        assert!(promoted.identifier.is_some());
    }

    #[test]
    fn test_post_process_detections_does_not_promote_true_license_clues() {
        let mut proper_match = create_perfect_match(10, 30);
        proper_match.license_expression = "mit".to_string();
        proper_match.license_expression_spdx = Some("MIT".to_string());

        let mut clue_match = create_perfect_match(1, 2);
        clue_match.rule_kind = crate::license_detection::models::RuleKind::Clue;

        let proper = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![proper_match],
            detection_log: vec![],
            identifier: Some("mit-proper".to_string()),
            file_regions: Vec::new(),
        };
        let clue = LicenseDetection {
            license_expression: None,
            license_expression_spdx: None,
            matches: vec![clue_match],
            detection_log: vec![DETECTION_LOG_LICENSE_CLUES.to_string()],
            identifier: None,
            file_regions: Vec::new(),
        };

        let result = post_process_detections(vec![proper, clue], 0.0);
        let preserved_clue = result
            .iter()
            .find(|detection| {
                detection
                    .detection_log
                    .contains(&DETECTION_LOG_LICENSE_CLUES.to_string())
            })
            .expect("clue detection");

        assert!(preserved_clue.license_expression.is_none());
        assert!(preserved_clue.identifier.is_none());
        assert!(
            !preserved_clue
                .detection_log
                .contains(&DETECTION_LOG_NOT_LICENSE_CLUES_AS_MORE_DETECTIONS_PRESENT.to_string(),)
        );
    }

    #[test]
    fn test_sort_detections_by_line() {
        let d1 = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![create_perfect_match(20, 30)],
            detection_log: vec![],
            identifier: Some("mit-1".to_string()),
            file_regions: Vec::new(),
        };
        let d2 = LicenseDetection {
            license_expression: Some("apache-2.0".to_string()),
            license_expression_spdx: Some("Apache-2.0".to_string()),
            matches: vec![create_perfect_match(1, 10)],
            detection_log: vec![],
            identifier: Some("apache-1".to_string()),
            file_regions: Vec::new(),
        };
        let sorted = sort_detections_by_line(vec![d1, d2]);
        assert_eq!(sorted[0].matches[0].start_line, LineNumber::ONE);
        assert_eq!(
            sorted[1].matches[0].start_line,
            LineNumber::new(20).expect("valid")
        );
    }

    #[test]
    fn test_determine_spdx_expression_from_scancode_single() {
        let licenses = vec![create_test_license()];
        let mapping = build_spdx_mapping(&licenses);
        let result = determine_spdx_expression_from_scancode("mit", &mapping);
        assert!(result.is_ok());
    }

    #[test]
    fn test_determine_spdx_expression_from_scancode_multiple() {
        let licenses = vec![create_test_license()];
        let mapping = build_spdx_mapping(&licenses);
        let result = determine_spdx_expression_from_scancode("mit AND apache-2.0", &mapping);
        assert!(result.is_ok());
    }

    #[test]
    fn test_determine_spdx_expression_from_scancode_empty() {
        let licenses = vec![create_test_license()];
        let mapping = build_spdx_mapping(&licenses);
        let result = determine_spdx_expression_from_scancode("", &mapping);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");
    }

    #[test]
    fn test_determine_spdx_expression_from_scancode_custom_license() {
        let licenses = vec![create_test_license()];
        let mapping = build_spdx_mapping(&licenses);
        let result = determine_spdx_expression_from_scancode("custom-1", &mapping);
        assert!(result.is_ok());
    }

    #[test]
    fn test_populate_detection_from_group_generates_spdx_expression() {
        let mut m = create_perfect_match(1, 10);
        m.license_expression = "mit".to_string();
        m.license_expression_spdx = Some("MIT".to_string());
        let group = DetectionGroup::new(vec![m]);
        let licenses = vec![create_test_license()];
        let spdx_mapping = build_spdx_mapping(&licenses);
        let mut detection = LicenseDetection {
            license_expression: None,
            license_expression_spdx: None,
            matches: Vec::new(),
            detection_log: Vec::new(),
            identifier: None,
            file_regions: Vec::new(),
        };
        populate_detection_from_group_with_spdx(&mut detection, &group, &spdx_mapping, None);
        assert!(detection.license_expression_spdx.is_some());
    }

    #[test]
    fn test_populate_detection_from_group_with_spdx_multiple() {
        let mut m1 = create_perfect_match(1, 10);
        m1.license_expression = "mit".to_string();
        let mut m2 = create_perfect_match(11, 20);
        m2.license_expression = "apache-2.0".to_string();
        m2.license_expression_spdx = Some("Apache-2.0".to_string());
        let group = DetectionGroup::new(vec![m1, m2]);
        let licenses = vec![create_test_license()];
        let spdx_mapping = build_spdx_mapping(&licenses);
        let mut detection = LicenseDetection {
            license_expression: None,
            license_expression_spdx: None,
            matches: Vec::new(),
            detection_log: Vec::new(),
            identifier: None,
            file_regions: Vec::new(),
        };
        populate_detection_from_group_with_spdx(&mut detection, &group, &spdx_mapping, None);
        assert!(detection.license_expression.is_some());
    }

    #[test]
    fn test_populate_detection_from_group_with_spdx_custom_license() {
        let mut m = create_perfect_match(1, 10);
        m.license_expression = "custom-license".to_string();
        m.license_expression_spdx = Some("custom-license".to_string());
        let group = DetectionGroup::new(vec![m]);
        let licenses = vec![create_test_license()];
        let spdx_mapping = build_spdx_mapping(&licenses);
        let mut detection = LicenseDetection {
            license_expression: None,
            license_expression_spdx: None,
            matches: Vec::new(),
            detection_log: Vec::new(),
            identifier: None,
            file_regions: Vec::new(),
        };
        populate_detection_from_group_with_spdx(&mut detection, &group, &spdx_mapping, None);
        assert!(detection.license_expression.is_some());
    }

    #[test]
    fn test_create_detection_from_group_unknown_reference_filters() {
        let mut m = create_test_match(1, 10, "2-aho", "mit.LICENSE");
        m.rule_kind = crate::license_detection::models::RuleKind::Reference;
        let group = DetectionGroup::new(vec![m]);
        let detection = create_detection_from_group(&group);
        assert_eq!(detection.matches.len(), 1);
    }

    #[test]
    fn test_create_detection_from_group_keeps_known_local_file_reference_expression() {
        let mut m = create_test_match(1, 1, "1-hash", "zlib_5.RULE");
        m.license_expression = "zlib".to_string();
        m.license_expression_spdx = Some("Zlib".to_string());
        m.match_coverage = 100.0;
        m.score = 100.0;
        m.rule_relevance = 100;
        m.referenced_filenames = Some(vec!["zlib.h".to_string()]);

        let group = DetectionGroup::new(vec![m]);
        let detection = create_detection_from_group(&group);

        assert_eq!(detection.license_expression.as_deref(), Some("zlib"));
    }

    #[test]
    fn test_attach_source_path_to_detections_populates_file_regions() {
        let mut match_item = create_perfect_match(4, 8);
        match_item.from_file = None;
        let mut detections = vec![LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![match_item],
            detection_log: vec![],
            identifier: Some("mit-1".to_string()),
            file_regions: Vec::new(),
        }];

        attach_source_path_to_detections(&mut detections, "src/lib.rs");

        assert_eq!(
            detections[0].matches[0].from_file.as_deref(),
            Some("src/lib.rs")
        );
        assert_eq!(detections[0].file_regions.len(), 1);
        assert_eq!(detections[0].file_regions[0].path, "src/lib.rs");
        assert_eq!(
            detections[0].file_regions[0].start_line,
            LineNumber::new(4).expect("valid")
        );
        assert_eq!(
            detections[0].file_regions[0].end_line,
            LineNumber::new(8).expect("valid")
        );
    }

    #[test]
    fn test_attach_source_path_to_detections_uses_single_detection_region_span() {
        let mut first = create_perfect_match(4, 8);
        first.from_file = None;
        let mut second = create_perfect_match(20, 25);
        second.from_file = None;
        let mut detections = vec![LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![first, second],
            detection_log: vec![],
            identifier: Some("mit-1".to_string()),
            file_regions: Vec::new(),
        }];

        attach_source_path_to_detections(&mut detections, "src/lib.rs");

        assert_eq!(detections[0].file_regions.len(), 1);
        assert_eq!(detections[0].file_regions[0].path, "src/lib.rs");
        assert_eq!(
            detections[0].file_regions[0].start_line,
            LineNumber::new(4).expect("valid")
        );
        assert_eq!(
            detections[0].file_regions[0].end_line,
            LineNumber::new(25).expect("valid")
        );
    }

    #[test]
    fn test_get_unique_detections_aggregates_distinct_regions() {
        let first = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![create_perfect_match(1, 10)],
            detection_log: vec![],
            identifier: Some("mit-shared".to_string()),
            file_regions: vec![FileRegion {
                path: "src/one.rs".to_string(),
                start_line: LineNumber::ONE,
                end_line: LineNumber::new(10).expect("valid"),
            }],
        };
        let second = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![create_perfect_match(20, 30)],
            detection_log: vec![],
            identifier: Some("mit-shared".to_string()),
            file_regions: vec![FileRegion {
                path: "src/two.rs".to_string(),
                start_line: LineNumber::new(20).expect("valid"),
                end_line: LineNumber::new(30).expect("valid"),
            }],
        };
        let third = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![create_perfect_match(20, 30)],
            detection_log: vec![],
            identifier: Some("mit-shared".to_string()),
            file_regions: vec![FileRegion {
                path: "src/two.rs".to_string(),
                start_line: LineNumber::new(20).expect("valid"),
                end_line: LineNumber::new(30).expect("valid"),
            }],
        };

        let unique = get_unique_detections(&[first, second, third]);

        assert_eq!(unique.len(), 1);
        assert_eq!(unique[0].identifier, "mit-shared");
        assert_eq!(unique[0].file_regions.len(), 2);
    }

    #[test]
    fn test_get_unique_detections_skips_detections_without_identifier() {
        let detection = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![create_perfect_match(1, 10)],
            detection_log: vec![],
            identifier: None,
            file_regions: vec![FileRegion {
                path: "src/one.rs".to_string(),
                start_line: LineNumber::ONE,
                end_line: LineNumber::new(10).expect("valid"),
            }],
        };

        let unique = get_unique_detections(&[detection]);

        assert!(unique.is_empty());
    }

    #[test]
    fn test_post_process_detections_attaches_aggregated_file_regions() {
        let first = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![create_perfect_match(1, 10)],
            detection_log: vec![],
            identifier: Some("mit-shared".to_string()),
            file_regions: vec![FileRegion {
                path: "src/one.rs".to_string(),
                start_line: LineNumber::ONE,
                end_line: LineNumber::new(10).expect("valid"),
            }],
        };
        let second = LicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![create_perfect_match(20, 30)],
            detection_log: vec![],
            identifier: Some("mit-shared".to_string()),
            file_regions: vec![FileRegion {
                path: "src/two.rs".to_string(),
                start_line: LineNumber::new(20).expect("valid"),
                end_line: LineNumber::new(30).expect("valid"),
            }],
        };

        let processed = post_process_detections(vec![first, second], 0.0);

        assert_eq!(processed.len(), 2);
        assert_eq!(processed[0].file_regions.len(), 2);
        assert_eq!(processed[1].file_regions.len(), 2);
    }
}
