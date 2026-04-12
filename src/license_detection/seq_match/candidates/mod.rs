//! Candidate selection using set and multiset similarity.

use crate::license_detection::TokenMultiset;
use crate::license_detection::TokenSet;
use crate::license_detection::index::LicenseIndex;
use crate::license_detection::index::dictionary::TokenId;
use crate::license_detection::models::Rule;
use crate::license_detection::query::QueryRun;
use std::collections::{HashMap, HashSet};

use super::HIGH_RESEMBLANCE_THRESHOLD_TENTHS;

/// Score vector for ranking candidates using set similarity.
///
/// Contains metrics computed from set/multiset intersections.
///
/// Corresponds to Python: `ScoresVector` namedtuple in match_set.py (line 458)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScoresVector {
    /// True if the sets are highly similar (resemblance >= threshold)
    pub is_highly_resemblant: bool,
    /// Ordering key for containment.
    containment: OrderingKey,
    /// Ordering key for amplified resemblance.
    resemblance: OrderingKey,
    /// Ordering key for matched length.
    matched_length: OrderingKey,
    /// Rule ID for tie-breaking
    pub rid: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OrderingKey {
    numerator: u64,
    denominator: u64,
}

impl OrderingKey {
    const fn integer(value: u32) -> Self {
        Self {
            numerator: value as u64,
            denominator: 1,
        }
    }

    const fn ratio(numerator: u64, denominator: u64) -> Self {
        Self {
            numerator,
            denominator,
        }
    }
}

impl PartialOrd for OrderingKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderingKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (u128::from(self.numerator) * u128::from(other.denominator))
            .cmp(&(u128::from(other.numerator) * u128::from(self.denominator)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CandidateMetrics {
    pub(super) matched_length: usize,
    pub(super) query_len: usize,
    pub(super) rule_len: usize,
}

impl CandidateMetrics {
    fn new(matched_length: usize, query_len: usize, rule_len: usize) -> Self {
        Self {
            matched_length,
            query_len,
            rule_len,
        }
    }

    fn union_len(&self) -> usize {
        self.query_len + self.rule_len - self.matched_length
    }

    fn rounded_is_highly_resemblant(&self) -> bool {
        self.rounded_resemblance_threshold_tenths() >= HIGH_RESEMBLANCE_THRESHOLD_TENTHS
    }

    fn full_is_highly_resemblant(&self) -> bool {
        let matched = self.matched_length as u64;
        let union = self.union_len() as u64;
        let threshold_tenths = u64::from(HIGH_RESEMBLANCE_THRESHOLD_TENTHS);

        u128::from(matched) * 10 >= u128::from(union) * u128::from(threshold_tenths)
    }

    pub(super) fn containment_f32(&self) -> f32 {
        self.matched_length as f32 / self.rule_len as f32
    }

    pub(super) fn amplified_resemblance_f32(&self) -> f32 {
        let union_len = self.union_len() as f32;
        let resemblance = self.matched_length as f32 / union_len;
        resemblance * resemblance
    }

    fn rounded_containment_tenths(&self) -> u32 {
        quantize_ratio_tenths(self.matched_length as u64, self.rule_len as u64)
    }

    fn rounded_resemblance_threshold_tenths(&self) -> u32 {
        quantize_ratio_tenths(self.matched_length as u64, self.union_len() as u64)
    }

    fn rounded_amplified_resemblance_tenths(&self) -> u32 {
        quantize_squared_ratio_tenths(self.matched_length as u64, self.union_len() as u64)
    }

    fn rounded_matched_length_tenths(&self) -> u32 {
        quantize_ratio_tenths(self.matched_length as u64, 20)
    }

    fn rounded_score_vector(&self, rid: usize) -> ScoresVector {
        ScoresVector {
            is_highly_resemblant: self.rounded_is_highly_resemblant(),
            containment: OrderingKey::integer(self.rounded_containment_tenths()),
            resemblance: OrderingKey::integer(self.rounded_amplified_resemblance_tenths()),
            matched_length: OrderingKey::integer(self.rounded_matched_length_tenths()),
            rid,
        }
    }

    fn full_score_vector(&self, rid: usize) -> ScoresVector {
        ScoresVector {
            is_highly_resemblant: self.full_is_highly_resemblant(),
            containment: OrderingKey::ratio(self.matched_length as u64, self.rule_len as u64),
            resemblance: OrderingKey::ratio(self.matched_length as u64, self.union_len() as u64),
            matched_length: OrderingKey::ratio(self.matched_length as u64, 1),
            rid,
        }
    }
}

impl PartialOrd for ScoresVector {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScoresVector {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Python sorts ScoresVector namedtuple with reverse=True:
        // 1. is_highly_resemblant (True > False)
        // 2. containment (higher is better)
        // 3. resemblance (higher is better)
        // 4. matched_length (higher is better)
        // Note: Python does NOT use rid for tie-breaking in ScoresVector
        self.is_highly_resemblant
            .cmp(&other.is_highly_resemblant)
            .then_with(|| {
                self.containment
                    .partial_cmp(&other.containment)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                self.resemblance
                    .partial_cmp(&other.resemblance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                self.matched_length
                    .partial_cmp(&other.matched_length)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }
}

/// Candidate with its score vector and metadata.
///
/// Corresponds to the tuple structure used in Python: (scores_vectors, rid, rule, high_set_intersection)
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Candidate<'a> {
    /// Exact metrics used to derive ranking and dedupe keys.
    pub(super) metrics: CandidateMetrics,
    /// Rounded score vector for display/grouping
    score_vec_rounded: ScoresVector,
    /// Full score vector for sorting
    score_vec_full: ScoresVector,
    /// Rule ID
    pub(super) rid: usize,
    /// Reference to the rule (borrowed from LicenseIndex)
    pub(super) rule: &'a Rule,
    /// Set of high-value (legalese) tokens in the intersection
    pub(super) high_set_intersection: TokenSet,
}

#[derive(Debug, Clone)]
struct QueryData {
    query_set: TokenSet,
    query_mset: TokenMultiset,
    query_high_set: TokenSet,
    query_high_mset: TokenMultiset,
    query_set_len: usize,
    query_mset_len: usize,
}

impl QueryData {
    fn new(index: &LicenseIndex, query_run: &QueryRun) -> Option<Self> {
        let query_tokens = query_run.matchable_tokens();
        if query_tokens.is_empty() {
            return None;
        }

        let query_token_ids: Vec<TokenId> = query_tokens.iter().filter_map(|tid| *tid).collect();

        if query_token_ids.is_empty() {
            return None;
        }

        let query_set = TokenSet::from_token_ids(query_token_ids.iter().copied());
        let query_mset = TokenMultiset::from_token_ids(&query_token_ids);

        let query_high_set = TokenSet::from_u16_iter(
            query_set
                .iter()
                .filter(|tid| (*tid as usize) < index.len_legalese),
        );

        if query_high_set.is_empty() {
            return None;
        }

        let query_high_mset = query_mset.high_subset(&index.dictionary);
        let query_set_len = query_set.len();
        let query_mset_len = query_mset.total_count();

        Some(Self {
            query_set,
            query_mset,
            query_high_set,
            query_high_mset,
            query_set_len,
            query_mset_len,
        })
    }
}

impl PartialOrd for Candidate<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for Candidate<'_> {}

impl Ord for Candidate<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Python sorts the tuple ((svr, svf), rid, rule, ...) with reverse=True
        // So it compares (svr, svf) tuple first, which means:
        // 1. Compare rounded (svr) first
        // 2. Then compare full (svf) if rounded is equal
        // 3. Then compare rid if scores are still equal.
        compare_candidate_rank(
            &self.score_vec_rounded,
            &self.score_vec_full,
            self.rid,
            &other.score_vec_rounded,
            &other.score_vec_full,
            other.rid,
        )
    }
}

fn compare_candidate_rank(
    rounded: &ScoresVector,
    full: &ScoresVector,
    rid: usize,
    other_rounded: &ScoresVector,
    other_full: &ScoresVector,
    other_rid: usize,
) -> std::cmp::Ordering {
    rounded
        .cmp(other_rounded)
        .then_with(|| full.cmp(other_full))
        .then_with(|| rid.cmp(&other_rid))
}

fn quantize_ratio_tenths(numerator: u64, denominator: u64) -> u32 {
    quantize_ratio_tenths_wide(u128::from(numerator), u128::from(denominator))
}

fn quantize_squared_ratio_tenths(numerator: u64, denominator: u64) -> u32 {
    let numerator = u128::from(numerator);
    let denominator = u128::from(denominator);

    quantize_ratio_tenths_wide(numerator * numerator, denominator * denominator)
}

fn quantize_ratio_tenths_wide(numerator: u128, denominator: u128) -> u32 {
    debug_assert!(denominator > 0);

    let scaled = numerator * 10;
    let quotient = scaled / denominator;
    let remainder = scaled % denominator;

    match (remainder * 2).cmp(&denominator) {
        std::cmp::Ordering::Less => quotient as u32,
        std::cmp::Ordering::Greater => (quotient + 1) as u32,
        std::cmp::Ordering::Equal => {
            if quotient.is_multiple_of(2) {
                quotient as u32
            } else {
                (quotient + 1) as u32
            }
        }
    }
}

fn passes_minimum_containment(rule: &Rule, metrics: CandidateMetrics) -> bool {
    rule.minimum_coverage.is_none_or(|min_cont| {
        let matched = metrics.matched_length as u64;
        let rule_len = metrics.rule_len as u64;
        let min_cont = u64::from(min_cont);

        u128::from(matched) * 100 >= u128::from(rule_len) * u128::from(min_cont)
    })
}

fn build_ranked_candidate<'a>(
    rid: usize,
    rule: &'a Rule,
    high_set_intersection: TokenSet,
    metrics: CandidateMetrics,
    high_resemblance: bool,
) -> Option<Candidate<'a>> {
    if metrics.query_len == 0 || metrics.rule_len == 0 {
        return None;
    }

    if !passes_minimum_containment(rule, metrics) {
        return None;
    }

    let score_vec_rounded = metrics.rounded_score_vector(rid);
    let score_vec_full = metrics.full_score_vector(rid);

    if high_resemblance
        && (!score_vec_rounded.is_highly_resemblant || !score_vec_full.is_highly_resemblant)
    {
        return None;
    }

    Some(Candidate {
        metrics,
        rid,
        rule,
        high_set_intersection,
        score_vec_rounded,
        score_vec_full,
    })
}

fn find_set_candidates<'a>(
    index: &'a LicenseIndex,
    query_data: &QueryData,
    high_resemblance: bool,
) -> Vec<Candidate<'a>> {
    let candidate_rids: HashSet<usize> = query_data
        .query_high_set
        .iter()
        .filter_map(|tid| index.rids_by_high_tid.get(&TokenId::new(tid)))
        .flat_map(|rids| rids.iter().copied())
        .collect();

    let mut candidates = Vec::new();

    for rid in candidate_rids {
        let Some(rule) = index.rules_by_rid.get(rid) else {
            continue;
        };
        let Some(rule_set) = index.sets_by_rid.get(&rid) else {
            continue;
        };
        let Some(rule_high_set) = index.high_sets_by_rid.get(&rid) else {
            continue;
        };

        let high_intersection_size = query_data.query_high_set.intersection_count(rule_high_set);
        if high_intersection_size < rule.min_high_matched_length_unique {
            continue;
        }

        let high_set_intersection = query_data.query_high_set.intersection(rule_high_set);
        if high_set_intersection.is_empty() {
            continue;
        }

        let intersection = query_data.query_set.intersection(rule_set);
        if intersection.is_empty() {
            continue;
        }

        let matched_length = intersection.len();
        if matched_length < rule.min_matched_length_unique {
            continue;
        }

        let Some(candidate) = build_ranked_candidate(
            rid,
            rule,
            high_set_intersection,
            CandidateMetrics::new(matched_length, query_data.query_set_len, rule.length_unique),
            high_resemblance,
        ) else {
            continue;
        };

        candidates.push(candidate);
    }

    candidates
}

fn rescore_candidates_with_multisets<'a>(
    index: &'a LicenseIndex,
    query_data: &QueryData,
    shortlisted: Vec<Candidate<'a>>,
    high_resemblance: bool,
) -> Vec<Candidate<'a>> {
    let mut candidates = Vec::new();

    for candidate in shortlisted {
        let Some(rule_mset) = index.msets_by_rid.get(&candidate.rid) else {
            continue;
        };

        let rule_high_mset = rule_mset.high_subset(&index.dictionary);
        let high_intersection_mset = query_data.query_high_mset.intersection(&rule_high_mset);
        if high_intersection_mset.is_empty() {
            continue;
        }

        let high_matched_length = high_intersection_mset.total_count();
        if high_matched_length < candidate.rule.min_high_matched_length {
            continue;
        }

        let full_intersection_mset = query_data.query_mset.intersection(rule_mset);
        let matched_length = full_intersection_mset.total_count();
        if matched_length < candidate.rule.min_matched_length {
            continue;
        }

        let iset_len = rule_mset.total_count();

        let Some(candidate) = build_ranked_candidate(
            candidate.rid,
            candidate.rule,
            candidate.high_set_intersection,
            CandidateMetrics::new(matched_length, query_data.query_mset_len, iset_len),
            high_resemblance,
        ) else {
            continue;
        };

        candidates.push(candidate);
    }

    candidates
}

/// Key for grouping duplicate candidates.
///
/// Candidates with the same DupeGroupKey are considered duplicates,
/// and only the best one is kept.
///
/// Corresponds to Python: `filter_dupes.group_key()` in match_set.py (line 467-476)
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct DupeGroupKey {
    license_expression: String,
    is_highly_resemblant: bool,
    containment: i32,
    resemblance: i32,
    matched_length: i32,
    rule_length: usize,
}

impl DupeGroupKey {
    fn from_candidate(candidate: &Candidate<'_>) -> Self {
        Self {
            license_expression: candidate.rule.license_expression.clone(),
            is_highly_resemblant: candidate.metrics.rounded_is_highly_resemblant(),
            containment: candidate.metrics.rounded_containment_tenths() as i32,
            resemblance: candidate.metrics.rounded_amplified_resemblance_tenths() as i32,
            matched_length: candidate.metrics.rounded_matched_length_tenths() as i32,
            rule_length: candidate.rule.tokens.len(),
        }
    }
}

/// Filter duplicate candidates, keeping only the best from each group.
///
/// Candidates are grouped by (license_expression, is_highly_resemblant, containment,
/// resemblance, matched_length, rule_length). Within each group, candidates are
/// ranked by (score_vec_full, rule.identifier) and only the best is kept.
///
/// This matches Python's filter_dupes behavior where matched_length uses 1-decimal
/// precision (e.g., 6.9 and 6.7 are different, but 7 and 7 would be same).
///
/// Corresponds to Python: `filter_dupes()` in match_set.py (line 461-498)
pub(super) fn filter_dupes(candidates: Vec<Candidate<'_>>) -> Vec<Candidate<'_>> {
    let mut groups: HashMap<DupeGroupKey, Vec<Candidate>> = HashMap::new();

    for candidate in candidates {
        let key = DupeGroupKey::from_candidate(&candidate);
        groups.entry(key).or_default().push(candidate);
    }

    let mut result: Vec<Candidate> = Vec::new();
    for mut group in groups.into_values() {
        // Python: duplicates = sorted(duplicates, reverse=True, key=lambda x: (sv_full, rule.identifier))
        // Higher sv_full wins, then HIGHER identifier alphabetically (reverse=True)
        group.sort_by(|a, b| {
            b.score_vec_full
                .cmp(&a.score_vec_full)
                .then_with(|| b.rule.identifier.cmp(&a.rule.identifier))
        });
        if let Some(best) = group.into_iter().next() {
            result.push(best);
        }
    }

    result
}

/// Compute multiset-based candidates (Phase 2 refinement).
///
/// After selecting candidates using sets, this refines the ranking using multisets.
///
/// Corresponds to Python: `compute_candidates()` step 2 in match_set.py (line 311-350)
pub(crate) fn select_seq_candidates<'a>(
    index: &'a LicenseIndex,
    query_run: &QueryRun,
    high_resemblance: bool,
    top_n: usize,
) -> Vec<Candidate<'a>> {
    let Some(query_data) = QueryData::new(index, query_run) else {
        return Vec::new();
    };

    let mut candidates = find_set_candidates(index, &query_data, high_resemblance);

    if candidates.is_empty() {
        return Vec::new();
    }

    candidates.sort_by(|a, b| {
        compare_candidate_rank(
            &b.score_vec_rounded,
            &b.score_vec_full,
            b.rid,
            &a.score_vec_rounded,
            &a.score_vec_full,
            a.rid,
        )
    });

    candidates.truncate(top_n * 10);

    let mut candidates =
        rescore_candidates_with_multisets(index, &query_data, candidates, high_resemblance);

    candidates = filter_dupes(candidates);

    candidates.sort_by(|a, b| b.cmp(a));
    candidates.truncate(top_n);

    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license_detection::index::dictionary::tid;

    fn candidate<'a>(rid: usize, rule: &'a Rule, metrics: CandidateMetrics) -> Candidate<'a> {
        Candidate {
            metrics,
            score_vec_rounded: metrics.rounded_score_vector(rid),
            score_vec_full: metrics.full_score_vector(rid),
            rid,
            rule,
            high_set_intersection: TokenSet::new(),
        }
    }

    #[test]
    fn test_scores_vector_comparison() {
        let sv1 = CandidateMetrics::new(9, 10, 10).rounded_score_vector(0);
        let sv2 = CandidateMetrics::new(5, 10, 10).rounded_score_vector(1);

        assert!(sv1 > sv2);
    }

    #[test]
    fn test_quantize_ratio_tenths_uses_exact_half_even_rounding() {
        assert_eq!(quantize_ratio_tenths(1, 20), 0);
        assert_eq!(quantize_ratio_tenths(3, 20), 2);
        assert_eq!(quantize_ratio_tenths(5, 20), 2);
        assert_eq!(quantize_ratio_tenths(45, 20), 22);
        assert_eq!(quantize_ratio_tenths(87, 20), 44);
        assert_eq!(quantize_ratio_tenths(133, 20), 66);
    }

    #[test]
    fn test_candidate_ordering() {
        let rule1 = Rule {
            identifier: "test1".to_string(),
            license_expression: "mit".to_string(),
            text: String::new(),
            tokens: vec![],
            rule_kind: crate::license_detection::models::RuleKind::Text,
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
            min_matched_length: 0,
            min_high_matched_length: 0,
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
        };

        let rule2 = Rule {
            identifier: "test2".to_string(),
            license_expression: "apache".to_string(),
            text: String::new(),
            tokens: vec![],
            rule_kind: crate::license_detection::models::RuleKind::Text,
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
            min_matched_length: 0,
            min_high_matched_length: 0,
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
        };

        let candidate1 = candidate(0, &rule1, CandidateMetrics::new(9, 10, 10));
        let candidate2 = candidate(1, &rule2, CandidateMetrics::new(5, 10, 10));

        assert!(
            candidate1 > candidate2,
            "Higher containment candidate should rank higher"
        );
    }

    #[test]
    fn test_filter_dupes_matched_length_precision() {
        let rule1 = Rule {
            identifier: "x11-dec1.RULE".to_string(),
            license_expression: "x11-dec1".to_string(),
            text: String::new(),
            tokens: vec![tid(0); 138],
            rule_kind: crate::license_detection::models::RuleKind::Text,
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
            min_matched_length: 0,
            min_high_matched_length: 0,
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
        };

        let rule2 = Rule {
            identifier: "cmu-uc.RULE".to_string(),
            license_expression: "cmu-uc".to_string(),
            text: String::new(),
            tokens: vec![tid(0); 133],
            ..rule1.clone()
        };

        let candidate1 = candidate(1, &rule1, CandidateMetrics::new(138, 200, 276));
        let candidate2 = candidate(2, &rule2, CandidateMetrics::new(133, 200, 266));

        let candidates = vec![candidate1, candidate2];
        let filtered = filter_dupes(candidates);

        assert_eq!(
            filtered.len(),
            2,
            "Should keep both candidates when matched_length differs at 1-decimal precision: 138/20=6.9 vs 133/20=6.7"
        );
    }

    #[test]
    fn test_filter_dupes_same_group() {
        let rule1 = Rule {
            identifier: "mit.RULE".to_string(),
            license_expression: "mit".to_string(),
            text: String::new(),
            tokens: vec![tid(0); 100],
            rule_kind: crate::license_detection::models::RuleKind::Text,
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
            min_matched_length: 0,
            min_high_matched_length: 0,
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
        };

        let rule2 = Rule {
            identifier: "mit_2.RULE".to_string(),
            license_expression: "mit".to_string(),
            text: String::new(),
            tokens: vec![tid(0); 100],
            ..rule1.clone()
        };

        let candidate1 = candidate(1, &rule1, CandidateMetrics::new(100, 200, 200));
        let candidate2 = candidate(2, &rule2, CandidateMetrics::new(100, 200, 200));

        let candidates = vec![candidate1, candidate2];
        let filtered = filter_dupes(candidates);

        assert_eq!(
            filtered.len(),
            1,
            "Should keep only one candidate when all group keys match"
        );
    }

    #[test]
    fn test_filter_dupes_prefers_higher_identifier_when_full_scores_tie() {
        let rule_sa = Rule {
            identifier: "cc-by-sa-1.0.RULE".to_string(),
            license_expression: "cc-by-sa-1.0".to_string(),
            text: String::new(),
            tokens: vec![tid(0); 1960],
            rule_kind: crate::license_detection::models::RuleKind::Text,
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
            min_matched_length: 0,
            min_high_matched_length: 0,
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
        };

        let rule_nc_sa = Rule {
            identifier: "cc-by-nc-sa-1.0.RULE".to_string(),
            license_expression: "cc-by-nc-sa-1.0".to_string(),
            text: String::new(),
            tokens: vec![tid(0); 1829],
            rule_kind: crate::license_detection::models::RuleKind::Text,
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
            min_matched_length: 0,
            min_high_matched_length: 0,
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
        };

        let candidate_sa = candidate(1, &rule_sa, CandidateMetrics::new(100, 110, 111));
        let candidate_nc_sa = candidate(2, &rule_nc_sa, CandidateMetrics::new(100, 110, 111));

        let candidates = vec![candidate_nc_sa, candidate_sa];
        let filtered = filter_dupes(candidates);

        assert_eq!(
            filtered.len(),
            2,
            "Different license expressions should create different groups"
        );

        let mut rule_same1 = Rule {
            license_expression: "same".to_string(),
            tokens: vec![tid(0); 100],
            ..rule_sa.clone()
        };
        let mut rule_same2 = Rule {
            license_expression: "same".to_string(),
            tokens: vec![tid(0); 100],
            ..rule_nc_sa.clone()
        };

        let same_group_candidates = vec![
            Candidate {
                metrics: filtered[0].metrics,
                score_vec_rounded: filtered[0].score_vec_rounded,
                score_vec_full: filtered[0].score_vec_full,
                rid: filtered[0].rid,
                rule: &mut rule_same1,
                high_set_intersection: TokenSet::new(),
            },
            Candidate {
                metrics: filtered[1].metrics,
                score_vec_rounded: filtered[1].score_vec_rounded,
                score_vec_full: filtered[1].score_vec_full,
                rid: filtered[1].rid,
                rule: &mut rule_same2,
                high_set_intersection: TokenSet::new(),
            },
        ];

        let deduped = filter_dupes(same_group_candidates);
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].rule.identifier, "cc-by-sa-1.0.RULE");
    }

    #[test]
    fn test_candidate_ordering_uses_rid_after_equal_scores() {
        let rule_a = Rule {
            identifier: "a.RULE".to_string(),
            license_expression: "a".to_string(),
            text: String::new(),
            tokens: vec![tid(0); 10],
            rule_kind: crate::license_detection::models::RuleKind::Text,
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
            min_matched_length: 0,
            min_high_matched_length: 0,
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
        };

        let rule_z = Rule {
            identifier: "z.RULE".to_string(),
            ..rule_a.clone()
        };

        let candidate_low_rid = candidate(1, &rule_z, CandidateMetrics::new(9, 10, 10));

        let candidate_high_rid = Candidate {
            score_vec_rounded: ScoresVector {
                rid: 2,
                ..candidate_low_rid.score_vec_rounded
            },
            score_vec_full: ScoresVector {
                rid: 2,
                ..candidate_low_rid.score_vec_full
            },
            metrics: candidate_low_rid.metrics,
            rid: 2,
            rule: &rule_a,
            high_set_intersection: TokenSet::new(),
        };

        let mut sorted = [candidate_low_rid, candidate_high_rid];
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(
            sorted[0].rid, 2,
            "Python final candidate tuple ordering falls back to higher rid after equal scores"
        );
    }
}
