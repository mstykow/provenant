//! License Detection Engine

pub mod aho_match;
pub mod automaton;
pub(crate) mod detection;
pub mod embedded;
mod position_set;
mod token_multiset;
mod token_set;

#[cfg(test)]
mod embedded_test;
pub mod expression;
#[cfg(test)]
mod golden_test;
#[cfg(feature = "golden-tests")]
pub mod golden_utils;
pub mod hash_match;
pub mod index;
mod match_refine;
pub mod models;
pub mod query;
pub mod rules;
pub mod seq_match;
pub mod spdx_lid;
pub mod spdx_mapping;
#[cfg(test)]
mod test_utils;
pub mod tokenize;
pub mod unknown_match;

use bit_set::BitSet;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;

use crate::license_detection::embedded::index::load_license_index_from_bytes;
use crate::license_detection::index::build_index_from_loaded;
use crate::license_detection::query::Query;
use crate::license_detection::rules::{
    load_loaded_licenses_from_directory, load_loaded_rules_from_directory,
};
use crate::license_detection::spdx_mapping::{SpdxMapping, build_spdx_mapping};
use crate::utils::text::strip_utf8_bom_str;

use crate::license_detection::detection::{
    attach_source_path_to_detections, empty_detection, populate_detection_from_group_with_spdx,
};
use crate::license_detection::models::MatcherKind;

/// Path to the license rules directory in the reference scancode-toolkit submodule.
/// Used by test code and the xtask generate-license-loader-artifact binary.
#[allow(dead_code)]
pub const SCANCODE_LICENSES_RULES_PATH: &str =
    "reference/scancode-toolkit/src/licensedcode/data/rules";

/// Path to the licenses directory in the reference scancode-toolkit submodule.
/// Used by test code and the xtask generate-license-loader-artifact binary.
#[allow(dead_code)]
pub const SCANCODE_LICENSES_LICENSES_PATH: &str =
    "reference/scancode-toolkit/src/licensedcode/data/licenses";

/// Path to the license data directory in the reference scancode-toolkit submodule.
/// Used by test code and the xtask generate-license-loader-artifact binary.
#[allow(dead_code)]
pub const SCANCODE_LICENSES_DATA_PATH: &str = "reference/scancode-toolkit/src/licensedcode/data";

pub const DEFAULT_LICENSEDB_URL_TEMPLATE: &str = "https://scancode-licensedb.aboutcode.org/{}";

pub(crate) use detection::{
    LicenseDetection, group_matches_by_region, post_process_detections, sort_matches_by_line,
};
pub use models::LicenseMatch;

pub use aho_match::aho_match;
pub use hash_match::hash_match;
pub use match_refine::{
    filter_invalid_contained_unknown_matches, merge_overlapping_matches, refine_matches,
    refine_matches_without_false_positive_filter, split_weak_matches,
};
pub use position_set::PositionSet;
pub use spdx_lid::spdx_lid_match;
pub use token_multiset::TokenMultiset;
pub use token_set::TokenSet;
pub use unknown_match::unknown_match;

use self::seq_match::{MAX_NEAR_DUPE_CANDIDATES, select_seq_candidates, seq_match_with_candidates};

/// License detection engine that orchestrates the detection pipeline.
///
/// The engine loads license rules and builds an index for efficient matching.
/// It supports multiple matching strategies (hash, SPDX-LID, Aho-Corasick, sequence)
/// and combines their results into final license detections.
#[derive(Debug, Clone)]
pub struct LicenseDetectionEngine {
    index: Arc<index::LicenseIndex>,
    spdx_mapping: SpdxMapping,
}

const MAX_DETECTION_SIZE: usize = 10 * 1024 * 1024; // 10MB
const MAX_REGULAR_SEQ_CANDIDATES: usize = 70;
const MAX_REDUNDANT_SEQ_CONTAINER_BOUNDARY_GAP: usize = 8;
const MAX_REDUNDANT_SEQ_CONTAINER_UNMATCHED_GAP: usize = 2;

fn truncate_detection_text(clean_text: &str) -> &str {
    if clean_text.len() <= MAX_DETECTION_SIZE {
        return clean_text;
    }

    log::warn!(
        "Content size {} exceeds limit {}, truncating for detection",
        clean_text.len(),
        MAX_DETECTION_SIZE
    );

    let boundary = clean_text.floor_char_boundary(MAX_DETECTION_SIZE);
    &clean_text[..boundary]
}

fn query_span_for_match(m: &LicenseMatch) -> Option<models::PositionSpan> {
    (!m.query_span().is_empty()).then(|| m.query_span().clone())
}

fn has_full_match_coverage(m: &LicenseMatch) -> bool {
    m.coverage() == 100.0
}

fn is_redundant_same_expression_seq_container(
    container: &LicenseMatch,
    candidate_contained_matches: &[LicenseMatch],
) -> bool {
    let container_is_redundant_coverage =
        has_full_match_coverage(container) || container.coverage() >= 99.0;
    if container.matcher != MatcherKind::Seq || !container_is_redundant_coverage {
        return false;
    }

    let container_qspan_set = container.qspan_set();

    let mut contained: Vec<&LicenseMatch> = candidate_contained_matches
        .iter()
        .filter(|m| {
            m.matcher == MatcherKind::Aho
                && has_full_match_coverage(m)
                && m.license_expression == container.license_expression
                && m.overlaps_with(&container_qspan_set)
        })
        .collect();

    if contained.len() < 2 {
        return false;
    }

    let material_children = contained.iter().filter(|m| m.matched_length > 1).count();
    if material_children < 2 {
        return false;
    }

    contained.sort_by_key(|m| m.qspan_bounds());

    let mut child_union = PositionSet::new();
    for m in &contained {
        child_union.extend_from_span(m.query_span());
    }

    let container_only_positions = container_qspan_set.difference(&child_union);
    let child_only_positions = child_union.difference(&container_qspan_set);

    let mut bridge_positions = BitSet::new();
    for pair in contained.windows(2) {
        let (_, previous_end) = pair[0].qspan_bounds();
        let (next_start, _) = pair[1].qspan_bounds();

        if next_start < previous_end {
            return false;
        }

        for pos in previous_end..next_start {
            bridge_positions.insert(pos);
        }
    }

    let container_only_boundary_positions = container_only_positions
        .iter()
        .filter(|&pos| !bridge_positions.contains(pos))
        .count();

    if container_only_positions.len() == 1
        && container_only_boundary_positions == 0
        && child_only_positions.is_empty()
    {
        return false;
    }

    if child_only_positions.is_empty()
        && container_only_positions.len() == container_only_boundary_positions
        && container_only_boundary_positions <= 3
    {
        let earliest_child = contained
            .iter()
            .map(|m| m.qspan_bounds().0)
            .min()
            .unwrap_or(usize::MAX);
        let latest_child = contained
            .iter()
            .map(|m| m.qspan_bounds().1.saturating_sub(1))
            .max()
            .unwrap_or(0);

        let is_one_sided_boundary = container_only_positions
            .iter()
            .all(|pos| pos < earliest_child)
            || container_only_positions
                .iter()
                .all(|pos| pos > latest_child);

        if is_one_sided_boundary {
            return false;
        }
    }

    let max_container_only_positions =
        MAX_REDUNDANT_SEQ_CONTAINER_BOUNDARY_GAP * contained.len() + 1;
    let max_container_boundary_positions =
        MAX_REDUNDANT_SEQ_CONTAINER_BOUNDARY_GAP * (contained.len() - 1);
    let max_child_only_positions = MAX_REDUNDANT_SEQ_CONTAINER_UNMATCHED_GAP + 1;

    container_only_positions.len() <= max_container_only_positions
        && container_only_boundary_positions <= max_container_boundary_positions
        && child_only_positions.len() <= max_child_only_positions
}

fn filter_redundant_same_expression_seq_containers(
    seq_matches: Vec<LicenseMatch>,
    candidate_contained_matches: &[LicenseMatch],
) -> Vec<LicenseMatch> {
    seq_matches
        .into_iter()
        .filter(|m| !is_redundant_same_expression_seq_container(m, candidate_contained_matches))
        .collect()
}

fn is_redundant_low_coverage_composite_seq_wrapper(
    container: &LicenseMatch,
    candidate_contained_matches: &[LicenseMatch],
) -> bool {
    if container.matcher != seq_match::MATCH_SEQ || container.coverage() >= 30.0 {
        return false;
    }

    let container_qspan_set = container.qspan_set();

    let children: Vec<&LicenseMatch> = candidate_contained_matches
        .iter()
        .filter(|m| {
            m.matcher == aho_match::MATCH_AHO
                && has_full_match_coverage(m)
                && m.license_expression != container.license_expression
                && m.overlaps_with(&container_qspan_set)
        })
        .collect();

    if children.len() < 2 {
        return false;
    }

    let unique_expressions: HashSet<&str> = children
        .iter()
        .map(|m| m.license_expression.as_str())
        .collect();
    if unique_expressions.len() < 2 {
        return false;
    }

    let mut child_union = PositionSet::new();
    for m in &children {
        child_union.extend_from_span(m.query_span());
    }

    let container_only_positions = container_qspan_set.difference(&child_union);
    let child_only_positions = child_union.difference(&container_qspan_set);

    let mut sorted_children = children;
    sorted_children.sort_by_key(|m| m.qspan_bounds());

    let mut bridge_positions = BitSet::new();
    for pair in sorted_children.windows(2) {
        let (_, previous_end) = pair[0].qspan_bounds();
        let (next_start, _) = pair[1].qspan_bounds();
        for pos in previous_end..next_start {
            bridge_positions.insert(pos);
        }
    }

    let container_only_boundary_positions = container_only_positions
        .iter()
        .filter(|&pos| !bridge_positions.contains(pos))
        .count();

    child_only_positions.is_empty()
        && container_only_positions.len() <= MAX_REDUNDANT_SEQ_CONTAINER_BOUNDARY_GAP
        && container_only_boundary_positions <= MAX_REDUNDANT_SEQ_CONTAINER_BOUNDARY_GAP
}

fn filter_redundant_low_coverage_composite_seq_wrappers(
    seq_matches: Vec<LicenseMatch>,
    candidate_contained_matches: &[LicenseMatch],
) -> Vec<LicenseMatch> {
    seq_matches
        .into_iter()
        .filter(|m| {
            !is_redundant_low_coverage_composite_seq_wrapper(m, candidate_contained_matches)
        })
        .collect()
}

fn subtract_spdx_match_qspans(
    query: &mut Query<'_>,
    matched_qspans: &mut Vec<models::PositionSpan>,
    aho_extra_matchables: &mut PositionSet,
    spdx_matches: &[LicenseMatch],
) {
    for m in spdx_matches {
        let Some(span) = query_span_for_match(m) else {
            continue;
        };

        aho_extra_matchables.extend_from_span(&span);
        query.subtract(&span);

        if has_full_match_coverage(m) {
            matched_qspans.push(span);
        }
    }
}

fn merge_and_prepare_aho_matches(
    index: &index::LicenseIndex,
    query: &mut Query<'_>,
    matched_qspans: &mut Vec<models::PositionSpan>,
    refined_aho: &[LicenseMatch],
) -> (Vec<LicenseMatch>, bool) {
    let merged_aho = merge_overlapping_matches(refined_aho);
    let mut saw_long_exact_license_text_match = false;

    for m in &merged_aho {
        let Some(span) = query_span_for_match(m) else {
            continue;
        };

        if has_full_match_coverage(m) {
            matched_qspans.push(span.clone());
        }

        if index
            .rules_by_rid
            .get(m.rid)
            .is_some_and(|rule| rule.is_license_text())
            && m.rule_length > 120
            && m.coverage() > 98.0
        {
            query.subtract(&span);
            saw_long_exact_license_text_match = true;
        }
    }

    (merged_aho, saw_long_exact_license_text_match)
}

fn collect_whole_query_exact_followup_matches(
    index: &index::LicenseIndex,
    query: &mut Query<'_>,
    matched_qspans: &mut Vec<models::PositionSpan>,
    whole_run: &query::QueryRun<'_>,
) -> Vec<LicenseMatch> {
    let mut seq_all_matches = Vec::new();

    if whole_run.is_matchable(false, matched_qspans) {
        let near_dupe_candidates =
            select_seq_candidates(index, whole_run, true, MAX_NEAR_DUPE_CANDIDATES);

        if !near_dupe_candidates.is_empty() {
            let near_dupe_matches =
                seq_match_with_candidates(index, whole_run, &near_dupe_candidates);

            for m in &near_dupe_matches {
                if !m.query_span().is_empty() {
                    let span = m.query_span().clone();
                    query.subtract(&span);
                    matched_qspans.push(span);
                }
            }

            seq_all_matches.extend(near_dupe_matches);
        }
    }

    seq_all_matches
}

fn collect_regular_seq_matches(
    index: &index::LicenseIndex,
    query: &Query<'_>,
    matched_qspans: &[models::PositionSpan],
    candidate_contained_matches: &[LicenseMatch],
) -> Vec<LicenseMatch> {
    let mut seq_all_matches = Vec::new();

    for query_run in query.query_runs() {
        if !query_run.is_matchable(false, matched_qspans) {
            continue;
        }

        let candidates =
            select_seq_candidates(index, &query_run, false, MAX_REGULAR_SEQ_CANDIDATES);
        if !candidates.is_empty() {
            let matches = seq_match_with_candidates(index, &query_run, &candidates);
            seq_all_matches.extend(matches);
        }
    }

    let merged_seq = merge_overlapping_matches(&seq_all_matches);
    let filtered_same_expression =
        filter_redundant_same_expression_seq_containers(merged_seq, candidate_contained_matches);
    filter_redundant_low_coverage_composite_seq_wrappers(
        filtered_same_expression,
        candidate_contained_matches,
    )
}

impl LicenseDetectionEngine {
    /// Create a new license detection engine from a pre-built license index.
    ///
    /// This is an internal constructor used by `from_directory()` and `from_embedded()`.
    /// It builds the SPDX mapping from the licenses in the index.
    fn from_index(index: index::LicenseIndex) -> Result<Self> {
        let mut license_vec: Vec<_> = index.licenses_by_key.values().cloned().collect();
        license_vec.sort_by(|a, b| a.key.cmp(&b.key));
        let spdx_mapping = build_spdx_mapping(&license_vec);

        Ok(Self {
            index: Arc::new(index),
            spdx_mapping,
        })
    }

    /// Create a new license detection engine from the embedded license index.
    ///
    /// This method loads the build-time embedded license artifact and constructs
    /// the runtime license index. This eliminates the runtime dependency on the
    /// ScanCode rules directory.
    ///
    /// # Returns
    /// A Result containing the engine or an error
    pub fn from_embedded() -> Result<Self> {
        let artifact_bytes = include_bytes!("../../resources/license_detection/license_index.zst");
        let index = load_license_index_from_bytes(artifact_bytes)
            .map_err(|e| anyhow::anyhow!("Failed to load embedded license index: {}", e))?;
        Self::from_index(index)
    }

    /// Create a new license detection engine from a directory of license rules.
    ///
    /// # Arguments
    /// * `rules_path` - Path to directory containing .LICENSE and .RULE files
    ///
    /// # Returns
    /// A Result containing the engine or an error
    pub fn from_directory(rules_path: &Path) -> Result<Self> {
        let (rules_dir, licenses_dir) = if rules_path.ends_with("data") {
            (rules_path.join("rules"), rules_path.join("licenses"))
        } else if rules_path.ends_with("rules") {
            let parent = rules_path.parent().ok_or_else(|| {
                anyhow::anyhow!("Cannot determine parent directory for rules path")
            })?;
            (rules_path.to_path_buf(), parent.join("licenses"))
        } else {
            (rules_path.to_path_buf(), rules_path.to_path_buf())
        };

        let loaded_rules = load_loaded_rules_from_directory(&rules_dir)?;
        let loaded_licenses = load_loaded_licenses_from_directory(&licenses_dir)?;
        let index = build_index_from_loaded(loaded_rules, loaded_licenses, false);

        Self::from_index(index)
    }

    pub fn detect_with_kind(
        &self,
        text: &str,
        unknown_licenses: bool,
        binary_derived: bool,
    ) -> Result<Vec<LicenseDetection>> {
        self.detect_with_kind_with_score(text, unknown_licenses, binary_derived, 0.0)
    }

    pub fn detect_with_kind_with_score(
        &self,
        text: &str,
        unknown_licenses: bool,
        binary_derived: bool,
        min_score: f32,
    ) -> Result<Vec<LicenseDetection>> {
        let clean_text = strip_utf8_bom_str(text);

        let content = truncate_detection_text(clean_text);

        let mut query = Query::from_extracted_text(content, &self.index, binary_derived)?;
        let whole_query_run = query.whole_query_run();

        let mut all_matches = Vec::new();
        let mut candidate_contained_matches = Vec::new();
        let mut aho_extra_matchables = PositionSet::new();
        let mut matched_qspans: Vec<models::PositionSpan> = Vec::new();

        // Phase 1a: Hash matching
        // Python returns immediately if hash matches found (index.py:987-991)
        {
            let hash_matches = hash_match(&self.index, &whole_query_run);

            if !hash_matches.is_empty() {
                let mut matches = hash_matches;
                sort_matches_by_line(&mut matches);

                let groups = group_matches_by_region(&matches);
                let detections: Vec<LicenseDetection> = groups
                    .iter()
                    .map(|group| {
                        let mut detection = empty_detection();
                        populate_detection_from_group_with_spdx(
                            &mut detection,
                            group,
                            &self.spdx_mapping,
                            Some(content),
                        );
                        detection
                    })
                    .collect();

                return Ok(post_process_detections(detections, min_score));
            }
        }

        // Phase 1b: SPDX-LID matching
        {
            let spdx_matches = spdx_lid_match(&self.index, &query);
            subtract_spdx_match_qspans(
                &mut query,
                &mut matched_qspans,
                &mut aho_extra_matchables,
                &spdx_matches,
            );
            all_matches.extend(spdx_matches);
        }

        // Phase 1c: Aho-Corasick matching
        {
            let aho_matches = if aho_extra_matchables.is_empty() {
                aho_match(&self.index, &whole_query_run)
            } else {
                aho_match::aho_match_with_extra_matchables(
                    &self.index,
                    &whole_query_run,
                    Some(&aho_extra_matchables),
                )
            };

            // Python's get_exact_matches() calls refine_matches with merge=False
            // This applies quality filters including required phrase filtering
            let refined_aho = match_refine::refine_aho_matches(&self.index, aho_matches, &query);
            candidate_contained_matches.extend(refined_aho.clone());
            let (merged_aho, _) = merge_and_prepare_aho_matches(
                &self.index,
                &mut query,
                &mut matched_qspans,
                &refined_aho,
            );
            all_matches.extend(merged_aho);

            let whole_query_followup = collect_whole_query_exact_followup_matches(
                &self.index,
                &mut query,
                &mut matched_qspans,
                &whole_query_run,
            );
            all_matches.extend(whole_query_followup);

            let merged_seq = collect_regular_seq_matches(
                &self.index,
                &query,
                &matched_qspans,
                &candidate_contained_matches,
            );
            all_matches.extend(merged_seq);
        }

        // Step 1: Initial refine WITHOUT false positive filtering
        // Python: refine_matches with filter_false_positive=False (index.py:1073-1080)
        let merged_matches =
            refine_matches_without_false_positive_filter(&self.index, all_matches, &query);

        // Step 2: Unknown detection and weak match handling
        // Python: index.py:1079-1118 - only runs when unknown_licenses=True
        let refined_matches = if unknown_licenses {
            // Split weak from good - Python: index.py:1083
            let (good_matches, weak_matches) = split_weak_matches(&self.index, &merged_matches);

            // Unknown detection on uncovered regions - Python: index.py:1093-1114
            let unknown_matches = unknown_match(&self.index, &query, &good_matches);
            let filtered_unknown =
                filter_invalid_contained_unknown_matches(&unknown_matches, &good_matches);

            let mut all_matches = good_matches;
            all_matches.extend(filtered_unknown);
            // reinject weak matches and let refine matches keep the bests
            // Python: index.py:1117-1118
            all_matches.extend(weak_matches);
            all_matches
        } else {
            merged_matches
        };

        // Step 5: Final refine WITH false positive filtering - Python: index.py:1130-1145
        let refined = refine_matches(&self.index, refined_matches, &query);

        let mut sorted = refined;
        sort_matches_by_line(&mut sorted);

        let groups = group_matches_by_region(&sorted);

        let detections: Vec<LicenseDetection> = groups
            .iter()
            .map(|group| {
                let mut detection = empty_detection();
                populate_detection_from_group_with_spdx(
                    &mut detection,
                    group,
                    &self.spdx_mapping,
                    Some(content),
                );
                detection
            })
            .collect();

        let detections = post_process_detections(detections, min_score);

        Ok(detections)
    }

    pub fn detect_with_kind_and_source(
        &self,
        text: &str,
        unknown_licenses: bool,
        binary_derived: bool,
        source_path: &str,
    ) -> Result<Vec<LicenseDetection>> {
        let mut detections = self.detect_with_kind(text, unknown_licenses, binary_derived)?;
        attach_source_path_to_detections(&mut detections, source_path);
        Ok(detections)
    }

    pub fn detect_with_kind_and_source_with_score(
        &self,
        text: &str,
        unknown_licenses: bool,
        binary_derived: bool,
        source_path: &str,
        min_score: f32,
    ) -> Result<Vec<LicenseDetection>> {
        let mut detections =
            self.detect_with_kind_with_score(text, unknown_licenses, binary_derived, min_score)?;
        attach_source_path_to_detections(&mut detections, source_path);
        Ok(detections)
    }

    /// Detect licenses and return raw matches (like Python's idx.match()).
    ///
    /// This is primarily used by golden tests and maintenance tooling that need
    /// raw match sequences before grouping or post-processing into detections.
    #[cfg(any(test, feature = "golden-tests"))]
    pub fn detect_matches_with_kind(
        &self,
        text: &str,
        unknown_licenses: bool,
        binary_derived: bool,
    ) -> Result<Vec<LicenseMatch>> {
        let clean_text = strip_utf8_bom_str(text);

        let content = truncate_detection_text(clean_text);

        let mut query = Query::from_extracted_text(content, &self.index, binary_derived)?;
        let whole_query_run = query.whole_query_run();

        let mut all_matches = Vec::new();
        let mut candidate_contained_matches = Vec::new();
        let mut aho_extra_matchables = PositionSet::new();
        let mut matched_qspans: Vec<models::PositionSpan> = Vec::new();

        // Phase 1a: Hash matching
        {
            let hash_matches = hash_match(&self.index, &whole_query_run);

            if !hash_matches.is_empty() {
                let mut matches = hash_matches;
                sort_matches_by_line(&mut matches);
                return Ok(matches);
            }
        }

        // Phase 1b: SPDX-LID matching
        {
            let spdx_matches = spdx_lid_match(&self.index, &query);
            subtract_spdx_match_qspans(
                &mut query,
                &mut matched_qspans,
                &mut aho_extra_matchables,
                &spdx_matches,
            );
            all_matches.extend(spdx_matches);
        }

        // Phase 1c: Aho-Corasick matching
        {
            let aho_matches = if aho_extra_matchables.is_empty() {
                aho_match(&self.index, &whole_query_run)
            } else {
                aho_match::aho_match_with_extra_matchables(
                    &self.index,
                    &whole_query_run,
                    Some(&aho_extra_matchables),
                )
            };
            let refined_aho = match_refine::refine_aho_matches(&self.index, aho_matches, &query);
            candidate_contained_matches.extend(refined_aho.clone());
            let (merged_aho, _) = merge_and_prepare_aho_matches(
                &self.index,
                &mut query,
                &mut matched_qspans,
                &refined_aho,
            );
            all_matches.extend(merged_aho);

            let whole_query_followup = collect_whole_query_exact_followup_matches(
                &self.index,
                &mut query,
                &mut matched_qspans,
                &whole_query_run,
            );
            all_matches.extend(whole_query_followup);

            let merged_seq = collect_regular_seq_matches(
                &self.index,
                &query,
                &matched_qspans,
                &candidate_contained_matches,
            );
            all_matches.extend(merged_seq);
        }

        // Step 1: Initial refine WITHOUT false positive filtering
        let merged_matches =
            refine_matches_without_false_positive_filter(&self.index, all_matches, &query);

        // Step 2: Unknown detection and weak match handling
        let refined_matches = if unknown_licenses {
            let (good_matches, weak_matches) = split_weak_matches(&self.index, &merged_matches);
            let unknown_matches = unknown_match(&self.index, &query, &good_matches);
            let filtered_unknown =
                filter_invalid_contained_unknown_matches(&unknown_matches, &good_matches);

            let mut all_matches = good_matches;
            all_matches.extend(filtered_unknown);
            all_matches.extend(weak_matches);
            all_matches
        } else {
            merged_matches
        };

        // Step 3: Final refine WITH false positive filtering - Python: index.py:1130-1145
        let refined = refine_matches(&self.index, refined_matches, &query);

        let mut sorted = refined;
        sort_matches_by_line(&mut sorted);

        // Return raw matches (NOT grouped) - this is Python's idx.match() behavior
        Ok(sorted)
    }

    /// Get a reference to the license index.
    pub fn index(&self) -> &index::LicenseIndex {
        &self.index
    }

    /// Get a reference to the SPDX mapping.
    #[cfg(test)]
    pub fn spdx_mapping(&self) -> &SpdxMapping {
        &self.spdx_mapping
    }
}

#[cfg(test)]
mod tests;
