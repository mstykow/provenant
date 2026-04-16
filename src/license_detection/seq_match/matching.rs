//! Sequence matching algorithms for finding matching blocks.

use crate::license_detection::index::LicenseIndex;
use crate::license_detection::index::dictionary::TokenId;
use crate::license_detection::models::position_span::PositionSpan;
use crate::license_detection::models::{LicenseMatch, MatchCoordinates};
use crate::license_detection::query::QueryRun;
use crate::models::LineNumber;
use crate::models::MatchScore;
use bit_set::BitSet;
use std::collections::HashMap;
use std::time::Instant;

use super::MATCH_SEQ;
use super::candidates::Candidate;

struct MatchSearchContext<'a> {
    query_tokens: &'a [TokenId],
    rule_tokens: &'a [TokenId],
    high_postings: &'a HashMap<TokenId, Vec<usize>>,
    len_legalese: usize,
    matchables: &'a BitSet,
    deadline: Option<Instant>,
}

/// Find the longest matching block between query and rule token sequences.
///
/// Uses dynamic programming to find the longest contiguous matching subsequence.
///
/// Corresponds to Python: `find_longest_match()` in seq.py (line 19)
///
/// # Arguments
///
/// * `query_tokens` - Query token sequence (called `a` in Python)
/// * `rule_tokens` - Rule token sequence (called `b` in Python)
/// * `query_lo` - Start position in query (inclusive)
/// * `query_hi` - End position in query (exclusive)
/// * `rule_lo` - Start position in rule (inclusive)
/// * `rule_hi` - End position in rule (exclusive)
/// * `high_postings` - Mapping of rule token IDs to their positions (b2j in Python)
/// * `len_legalese` - Token IDs below this are "good" tokens
/// * `matchables` - Set of matchable positions in query
///
/// # Returns
///
/// Tuple of (query_start, rule_start, match_length)
#[allow(clippy::needless_range_loop)]
fn find_longest_match_impl(
    context: &MatchSearchContext<'_>,
    query_lo: usize,
    query_hi: usize,
    rule_lo: usize,
    rule_hi: usize,
) -> anyhow::Result<(usize, usize, usize)> {
    let mut best_i = query_lo;
    let mut best_j = rule_lo;
    let mut best_size = 0;

    let mut j2len: HashMap<usize, usize> = HashMap::new();

    for (offset, i) in (query_lo..query_hi).enumerate() {
        if offset.is_multiple_of(256) {
            crate::license_detection::ensure_within_deadline(context.deadline)?;
        }

        let mut new_j2len: HashMap<usize, usize> = HashMap::new();
        let cur_a = context.query_tokens[i];

        if cur_a.as_usize() < context.len_legalese
            && context.matchables.contains(i)
            && let Some(positions) = context.high_postings.get(&cur_a)
        {
            for &j in positions {
                if j < rule_lo {
                    continue;
                }
                if j >= rule_hi {
                    break;
                }

                let prev_len = if j > 0 {
                    j2len.get(&(j - 1)).copied().unwrap_or(0)
                } else {
                    0
                };
                let k = prev_len + 1;
                new_j2len.insert(j, k);

                if k > best_size {
                    best_i = i + 1 - k;
                    best_j = j + 1 - k;
                    best_size = k;
                }
            }
        }
        j2len = new_j2len;
    }

    if best_size > 0 {
        while best_i > query_lo
            && best_j > rule_lo
            && context.query_tokens[best_i - 1] == context.rule_tokens[best_j - 1]
            && context.matchables.contains(best_i - 1)
        {
            best_i -= 1;
            best_j -= 1;
            best_size += 1;
        }

        while best_i + best_size < query_hi
            && best_j + best_size < rule_hi
            && context.query_tokens[best_i + best_size] == context.rule_tokens[best_j + best_size]
            && context.matchables.contains(best_i + best_size)
        {
            best_size += 1;
        }
    }

    Ok((best_i, best_j, best_size))
}

/// Find all matching blocks between query and rule token sequences using divide-and-conquer.
///
/// Uses a queue-based algorithm to find longest match, then recursively processes
/// left and right regions to find all matches.
///
/// Corresponds to Python: `match_blocks()` in seq.py (line 107)
///
/// # Arguments
///
/// * `query_tokens` - Query token sequence (called `a` in Python)
/// * `rule_tokens` - Rule token sequence (called `b` in Python)
/// * `query_start` - Start position in query (inclusive)
/// * `query_end` - End position in query (exclusive)
/// * `high_postings` - Mapping of rule token IDs to their positions (b2j in Python)
/// * `len_legalese` - Token IDs below this are "good" tokens
/// * `matchables` - Set of matchable positions in query
///
/// # Returns
///
/// Vector of matching blocks as (query_pos, rule_pos, length)
fn match_blocks_impl(
    context: &MatchSearchContext<'_>,
    query_start: usize,
    query_end: usize,
) -> anyhow::Result<Vec<(usize, usize, usize)>> {
    if context.query_tokens.is_empty() || context.rule_tokens.is_empty() {
        return Ok(Vec::new());
    }

    let mut queue: Vec<(usize, usize, usize, usize)> =
        vec![(query_start, query_end, 0, context.rule_tokens.len())];
    let mut matching_blocks: Vec<(usize, usize, usize)> = Vec::new();

    let mut loop_count = 0usize;
    while let Some((alo, ahi, blo, bhi)) = queue.pop() {
        if loop_count.is_multiple_of(32) {
            crate::license_detection::ensure_within_deadline(context.deadline)?;
        }
        loop_count += 1;

        let (i, j, k) = find_longest_match_impl(context, alo, ahi, blo, bhi)?;

        if k > 0 {
            matching_blocks.push((i, j, k));

            if alo < i && blo < j {
                queue.push((alo, i, blo, j));
            }
            if i + k < ahi && j + k < bhi {
                queue.push((i + k, ahi, j + k, bhi));
            }
        }
    }

    matching_blocks.sort();

    let mut non_adjacent: Vec<(usize, usize, usize)> = Vec::new();
    let mut i1 = 0usize;
    let mut j1 = 0usize;
    let mut k1 = 0usize;

    for (i2, j2, k2) in matching_blocks {
        if i1 + k1 == i2 && j1 + k1 == j2 {
            k1 += k2;
        } else {
            if k1 > 0 {
                non_adjacent.push((i1, j1, k1));
            }
            i1 = i2;
            j1 = j2;
            k1 = k2;
        }
    }

    if k1 > 0 {
        non_adjacent.push((i1, j1, k1));
    }

    Ok(non_adjacent)
}

/// Sequence matching against pre-selected candidates.
///
/// Used by Phase 2 (near-duplicate detection) to match the whole file
/// against a small set of high-resemblance candidates.
///
/// # Arguments
///
/// * `index` - License index
/// * `query_run` - Query run to match (typically the whole file)
/// * `candidates` - Pre-selected candidates from `compute_candidates()`
///
/// # Returns
///
/// Vector of LicenseMatch results
pub(crate) fn seq_match_with_candidates(
    index: &LicenseIndex,
    query_run: &QueryRun,
    candidates: &[Candidate<'_>],
) -> Vec<LicenseMatch> {
    seq_match_with_candidates_and_deadline(index, query_run, candidates, None)
        .expect("Sequence matching without deadline should not time out")
}

pub(crate) fn seq_match_with_candidates_and_deadline(
    index: &LicenseIndex,
    query_run: &QueryRun,
    candidates: &[Candidate<'_>],
    deadline: Option<Instant>,
) -> anyhow::Result<Vec<LicenseMatch>> {
    let mut matches = Vec::new();

    for (candidate_index, candidate) in candidates.iter().enumerate() {
        if candidate_index.is_multiple_of(8) {
            crate::license_detection::ensure_within_deadline(deadline)?;
        }

        let rid = candidate.rid;
        let rule_tokens = index.tids_by_rid.get(rid);
        let high_postings: HashMap<TokenId, Vec<usize>> = index
            .high_postings_by_rid
            .get(&rid)
            .map(|hp| {
                hp.iter()
                    .filter(|(tid, _)| candidate.high_set_intersection.contains(&tid.raw()))
                    .map(|(&tid, postings)| (tid, postings.clone()))
                    .collect()
            })
            .unwrap_or_default();

        if let Some(rule_tokens) = rule_tokens {
            let query_tokens = query_run.tokens();
            let len_legalese = index.len_legalese;

            let qbegin = 0usize;
            let qfinish = query_tokens.len().saturating_sub(1);

            let matchables: BitSet = query_run
                .matchables(true)
                .iter()
                .map(|pos| pos - query_run.start)
                .collect();
            let context = MatchSearchContext {
                query_tokens,
                rule_tokens,
                high_postings: &high_postings,
                len_legalese,
                matchables: &matchables,
                deadline,
            };

            let mut qstart = qbegin;
            let mut loop_count = 0usize;

            while qstart <= qfinish {
                if loop_count.is_multiple_of(32) {
                    crate::license_detection::ensure_within_deadline(deadline)?;
                }
                loop_count += 1;

                let has_remaining_matchables = matchables.iter().any(|pos| pos >= qstart);
                if !has_remaining_matchables {
                    break;
                }
                let blocks = match_blocks_impl(&context, qstart, qfinish + 1)?;

                if blocks.is_empty() {
                    break;
                }

                let mut max_qend = qstart;

                for (qpos, ipos, mlen) in blocks {
                    if mlen < 1 {
                        continue;
                    }

                    let qspan_end = qpos + mlen;
                    max_qend = max_qend.max(qspan_end);

                    if mlen == 1 && query_tokens[qpos].as_usize() >= len_legalese {
                        continue;
                    }

                    let rule_length = rule_tokens.len();
                    if rule_length == 0 {
                        continue;
                    }

                    let qend = qpos + mlen - 1;
                    let abs_qpos = qpos + query_run.start;
                    let abs_qend = qend + query_run.start;
                    let start_line = query_run
                        .line_for_pos(abs_qpos)
                        .and_then(LineNumber::new)
                        .unwrap_or(LineNumber::ONE);
                    let end_line = query_run
                        .line_for_pos(abs_qend)
                        .and_then(LineNumber::new)
                        .unwrap_or(start_line);

                    let qspan =
                        PositionSpan::range(qpos + query_run.start, qpos + mlen + query_run.start);
                    let ispan = PositionSpan::range(ipos, ipos + mlen);
                    let hispan = PositionSpan::from_positions((ipos..ipos + mlen).filter(|&p| {
                        rule_tokens
                            .get(p)
                            .is_some_and(|t| t.as_usize() < len_legalese)
                    }));

                    let rule_coverage = mlen as f32 / rule_length as f32;
                    let match_coverage = LicenseMatch::round_metric(rule_coverage * 100.0);

                    let score = MatchScore::from_percentage(
                        f64::from(match_coverage) * f64::from(candidate.rule.relevance) / 100.0,
                    );

                    let license_match = LicenseMatch {
                        license_expression: candidate.rule.license_expression.clone(),
                        license_expression_spdx: index
                            .rule_metadata_by_identifier
                            .get(&candidate.rule.identifier)
                            .and_then(|metadata| metadata.license_expression_spdx.clone()),
                        from_file: None,
                        start_line,
                        end_line,
                        start_token: abs_qpos,
                        end_token: abs_qend + 1,
                        matcher: MATCH_SEQ,
                        score,
                        matched_length: mlen,
                        rule_length,
                        match_coverage,
                        rule_relevance: candidate.rule.relevance,
                        rid,
                        rule_identifier: candidate.rule.identifier.clone(),
                        rule_url: candidate.rule.rule_url().unwrap_or_default(),
                        matched_text: None,
                        referenced_filenames: candidate.rule.referenced_filenames.clone(),
                        rule_kind: candidate.rule.kind(),
                        is_from_license: candidate.rule.is_from_license,
                        rule_start_token: ipos,
                        coordinates: MatchCoordinates::rule_aligned(qspan, ispan, hispan),
                        candidate_resemblance: candidate.metrics.amplified_resemblance_f32(),
                        candidate_containment: candidate.metrics.containment_f32(),
                    };

                    matches.push(license_match);
                }

                qstart = max_qend;
            }
        }
    }

    Ok(matches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license_detection::index::dictionary::{TokenId, tid};
    use std::ops::Range;

    fn tids(values: &[u16]) -> Vec<TokenId> {
        values.iter().copied().map(tid).collect()
    }

    fn find_longest_match_for_test(
        query_tokens: &[TokenId],
        rule_tokens: &[TokenId],
        query_range: Range<usize>,
        rule_range: Range<usize>,
        high_postings: &HashMap<TokenId, Vec<usize>>,
        len_legalese: usize,
        matchables: &BitSet,
    ) -> (usize, usize, usize) {
        let context = MatchSearchContext {
            query_tokens,
            rule_tokens,
            high_postings,
            len_legalese,
            matchables,
            deadline: None,
        };
        find_longest_match_impl(
            &context,
            query_range.start,
            query_range.end,
            rule_range.start,
            rule_range.end,
        )
        .expect("Sequence matching without deadline should not time out")
    }

    fn match_blocks_for_test(
        query_tokens: &[TokenId],
        rule_tokens: &[TokenId],
        query_range: Range<usize>,
        high_postings: &HashMap<TokenId, Vec<usize>>,
        len_legalese: usize,
        matchables: &BitSet,
    ) -> Vec<(usize, usize, usize)> {
        let context = MatchSearchContext {
            query_tokens,
            rule_tokens,
            high_postings,
            len_legalese,
            matchables,
            deadline: None,
        };
        match_blocks_impl(&context, query_range.start, query_range.end)
            .expect("Sequence block matching without deadline should not time out")
    }

    #[test]
    fn test_find_longest_match_basic() {
        let query_tokens = tids(&[0, 1, 2, 3]);
        let rule_tokens = tids(&[0, 1, 2, 3]);
        let mut high_postings: HashMap<TokenId, Vec<usize>> = HashMap::new();
        high_postings.insert(tid(0), vec![0]);
        high_postings.insert(tid(1), vec![1]);
        high_postings.insert(tid(2), vec![2]);
        high_postings.insert(tid(3), vec![3]);

        let matchables: BitSet = (0..query_tokens.len()).collect();

        let result = find_longest_match_for_test(
            &query_tokens,
            &rule_tokens,
            0..query_tokens.len(),
            0..rule_tokens.len(),
            &high_postings,
            5,
            &matchables,
        );

        assert_eq!(result, (0, 0, 4), "Should find full match");
    }

    #[test]
    fn test_find_longest_match_with_gap() {
        let query_tokens = tids(&[0, 1, 99, 2, 3]);
        let rule_tokens = tids(&[0, 1, 2, 3]);
        let mut high_postings: HashMap<TokenId, Vec<usize>> = HashMap::new();
        high_postings.insert(tid(0), vec![0]);
        high_postings.insert(tid(1), vec![1]);
        high_postings.insert(tid(2), vec![2]);
        high_postings.insert(tid(3), vec![3]);

        let matchables: BitSet = (0..query_tokens.len()).collect();

        let result = find_longest_match_for_test(
            &query_tokens,
            &rule_tokens,
            0..query_tokens.len(),
            0..rule_tokens.len(),
            &high_postings,
            5,
            &matchables,
        );

        assert_eq!(
            result.2, 2,
            "Should find longest contiguous match (length 2)"
        );
        assert!(
            result == (0, 0, 2) || result == (3, 2, 2),
            "Should find either [0,1] or [2,3] match, got {:?}",
            result
        );
    }

    #[test]
    fn test_find_longest_match_uses_high_postings() {
        let query_tokens = tids(&[0, 10, 2]);
        let rule_tokens = tids(&[0, 1, 2]);
        let mut high_postings: HashMap<TokenId, Vec<usize>> = HashMap::new();
        high_postings.insert(tid(0), vec![0]);
        high_postings.insert(tid(2), vec![2]);

        let matchables: BitSet = (0..query_tokens.len()).collect();

        let result = find_longest_match_for_test(
            &query_tokens,
            &rule_tokens,
            0..query_tokens.len(),
            0..rule_tokens.len(),
            &high_postings,
            5,
            &matchables,
        );

        assert_eq!(
            result.2, 1,
            "Token 10 is not in high_postings and doesn't match token 1, so LCS finds separate matches"
        );
    }

    #[test]
    fn test_find_longest_match_no_match() {
        let query_tokens = tids(&[10, 11, 12]);
        let rule_tokens = tids(&[0, 1, 2]);
        let high_postings: HashMap<TokenId, Vec<usize>> = HashMap::new();

        let matchables: BitSet = (0..query_tokens.len()).collect();

        let result = find_longest_match_for_test(
            &query_tokens,
            &rule_tokens,
            0..query_tokens.len(),
            0..rule_tokens.len(),
            &high_postings,
            5,
            &matchables,
        );

        assert_eq!(
            result,
            (0, 0, 0),
            "Should return (alo, blo, 0) for no match"
        );
    }

    #[test]
    fn test_find_longest_match_respects_bounds() {
        let query_tokens = tids(&[0, 1, 2, 0, 1, 2, 0, 1, 2]);
        let rule_tokens = tids(&[0, 1, 2]);
        let mut high_postings: HashMap<TokenId, Vec<usize>> = HashMap::new();
        high_postings.insert(tid(0), vec![0]);
        high_postings.insert(tid(1), vec![1]);
        high_postings.insert(tid(2), vec![2]);

        let matchables: BitSet = (0..query_tokens.len()).collect();

        let result = find_longest_match_for_test(
            &query_tokens,
            &rule_tokens,
            3..6,
            0..rule_tokens.len(),
            &high_postings,
            5,
            &matchables,
        );

        assert_eq!(
            result,
            (3, 0, 3),
            "Should find match within query bounds [3,6)"
        );
    }

    #[test]
    fn test_find_longest_match_non_matchable_position() {
        let query_tokens = tids(&[0, 1, 2]);
        let rule_tokens = tids(&[0, 1, 2]);
        let mut high_postings: HashMap<TokenId, Vec<usize>> = HashMap::new();
        high_postings.insert(tid(0), vec![0]);
        high_postings.insert(tid(1), vec![1]);
        high_postings.insert(tid(2), vec![2]);

        let matchables: BitSet = [0, 2].into_iter().collect();

        let result = find_longest_match_for_test(
            &query_tokens,
            &rule_tokens,
            0..query_tokens.len(),
            0..rule_tokens.len(),
            &high_postings,
            5,
            &matchables,
        );

        assert_eq!(
            result.2, 1,
            "Position 1 is not matchable, so longest match should be 1"
        );
    }

    #[test]
    fn test_match_blocks_divide_conquer() {
        let query_tokens = tids(&[0, 1, 2, 3]);
        let rule_tokens = tids(&[0, 1, 2, 3]);
        let mut high_postings: HashMap<TokenId, Vec<usize>> = HashMap::new();
        high_postings.insert(tid(0), vec![0]);
        high_postings.insert(tid(1), vec![1]);
        high_postings.insert(tid(2), vec![2]);
        high_postings.insert(tid(3), vec![3]);

        let matchables: BitSet = (0..query_tokens.len()).collect();

        let blocks = match_blocks_for_test(
            &query_tokens,
            &rule_tokens,
            0..query_tokens.len(),
            &high_postings,
            5,
            &matchables,
        );

        assert_eq!(blocks.len(), 1, "Should find single full match");
        assert_eq!(blocks[0], (0, 0, 4), "Should match entire sequence");
    }

    #[test]
    fn test_match_blocks_collapse_adjacent() {
        let query_tokens = tids(&[0, 1, 2, 3, 4]);
        let rule_tokens = tids(&[0, 1, 2, 3, 4]);
        let mut high_postings: HashMap<TokenId, Vec<usize>> = HashMap::new();
        for (i, &tid) in query_tokens.iter().enumerate() {
            high_postings.entry(tid).or_default().push(i);
        }

        let matchables: BitSet = (0..query_tokens.len()).collect();

        let blocks = match_blocks_for_test(
            &query_tokens,
            &rule_tokens,
            0..query_tokens.len(),
            &high_postings,
            5,
            &matchables,
        );

        assert_eq!(
            blocks.len(),
            1,
            "Adjacent blocks should be collapsed into one"
        );
        assert_eq!(blocks[0].2, 5, "Collapsed block should have full length");
    }

    #[test]
    fn test_match_blocks_no_match() {
        let query_tokens = tids(&[10, 11, 12]);
        let rule_tokens = tids(&[0, 1, 2]);
        let high_postings: HashMap<TokenId, Vec<usize>> = HashMap::new();

        let matchables: BitSet = (0..query_tokens.len()).collect();

        let blocks = match_blocks_for_test(
            &query_tokens,
            &rule_tokens,
            0..query_tokens.len(),
            &high_postings,
            5,
            &matchables,
        );

        assert!(blocks.is_empty(), "Should return empty when no matches");
    }

    #[test]
    fn test_match_blocks_empty_query() {
        let query_tokens = tids(&[]);
        let rule_tokens = tids(&[0, 1, 2]);
        let high_postings: HashMap<TokenId, Vec<usize>> = HashMap::new();
        let matchables: BitSet = BitSet::new();

        let blocks = match_blocks_for_test(
            &query_tokens,
            &rule_tokens,
            0..query_tokens.len(),
            &high_postings,
            5,
            &matchables,
        );

        assert!(blocks.is_empty());
    }

    #[test]
    fn test_match_blocks_with_gap() {
        let query_tokens = tids(&[0, 1, 99, 2, 3]);
        let rule_tokens = tids(&[0, 1, 2, 3]);
        let mut high_postings: HashMap<TokenId, Vec<usize>> = HashMap::new();
        high_postings.insert(tid(0), vec![0]);
        high_postings.insert(tid(1), vec![1]);
        high_postings.insert(tid(2), vec![2]);
        high_postings.insert(tid(3), vec![3]);

        let matchables: BitSet = (0..query_tokens.len()).collect();

        let blocks = match_blocks_for_test(
            &query_tokens,
            &rule_tokens,
            0..query_tokens.len(),
            &high_postings,
            5,
            &matchables,
        );

        assert!(!blocks.is_empty(), "Should find matches despite gap");
        assert!(
            blocks.iter().any(|b| b.2 >= 2),
            "Should find at least one block of length >= 2"
        );
    }

    #[test]
    fn test_match_blocks_empty_rule() {
        let query_tokens = tids(&[0, 1, 2]);
        let rule_tokens = tids(&[]);
        let high_postings: HashMap<TokenId, Vec<usize>> = HashMap::new();
        let matchables: BitSet = (0..query_tokens.len()).collect();

        let blocks = match_blocks_for_test(
            &query_tokens,
            &rule_tokens,
            0..query_tokens.len(),
            &high_postings,
            5,
            &matchables,
        );

        assert!(blocks.is_empty());
    }

    #[test]
    fn test_match_blocks_multiple_regions() {
        let query_tokens = tids(&[0, 1, 99, 2, 3, 88, 0, 1]);
        let rule_tokens = tids(&[0, 1, 2, 3]);
        let mut high_postings: HashMap<TokenId, Vec<usize>> = HashMap::new();
        high_postings.insert(tid(0), vec![0]);
        high_postings.insert(tid(1), vec![1]);
        high_postings.insert(tid(2), vec![2]);
        high_postings.insert(tid(3), vec![3]);

        let matchables: BitSet = (0..query_tokens.len()).collect();

        let blocks = match_blocks_for_test(
            &query_tokens,
            &rule_tokens,
            0..query_tokens.len(),
            &high_postings,
            5,
            &matchables,
        );

        assert!(
            blocks.len() >= 2,
            "Should find multiple match regions, got {:?}",
            blocks
        );
    }

    #[test]
    fn test_match_blocks_with_range() {
        let query_tokens = tids(&[0, 1, 2, 99, 0, 1, 2]);
        let rule_tokens = tids(&[0, 1, 2]);
        let mut high_postings: HashMap<TokenId, Vec<usize>> = HashMap::new();
        high_postings.insert(tid(0), vec![0]);
        high_postings.insert(tid(1), vec![1]);
        high_postings.insert(tid(2), vec![2]);

        let matchables: BitSet = (0..query_tokens.len()).collect();

        let blocks = match_blocks_for_test(
            &query_tokens,
            &rule_tokens,
            0..3,
            &high_postings,
            5,
            &matchables,
        );

        assert_eq!(
            blocks.len(),
            1,
            "Should only find one match in the restricted range"
        );
        assert_eq!(blocks[0], (0, 0, 3));

        let blocks2 = match_blocks_for_test(
            &query_tokens,
            &rule_tokens,
            4..query_tokens.len(),
            &high_postings,
            5,
            &matchables,
        );
        assert_eq!(blocks2.len(), 1, "Should find the second occurrence");
        assert_eq!(blocks2[0], (4, 0, 3));
    }

    #[test]
    fn test_extend_match_into_low_tokens() {
        let query_tokens = tids(&[0, 1, 2, 10, 11]);
        let rule_tokens = tids(&[0, 1, 2, 10, 11]);
        let mut high_postings: HashMap<TokenId, Vec<usize>> = HashMap::new();
        high_postings.insert(tid(0), vec![0]);
        high_postings.insert(tid(1), vec![1]);
        high_postings.insert(tid(2), vec![2]);

        let matchables: BitSet = (0..query_tokens.len()).collect();

        let blocks = match_blocks_for_test(
            &query_tokens,
            &rule_tokens,
            0..query_tokens.len(),
            &high_postings,
            5,
            &matchables,
        );

        assert_eq!(blocks.len(), 1, "Should find single extended match");
        assert_eq!(
            blocks[0].2, 5,
            "Match should extend into low-token areas (positions 3,4) when they are in matchables"
        );
    }

    #[test]
    fn test_extend_match_blocked_by_non_matchable() {
        let query_tokens = tids(&[0, 1, 2, 10, 11]);
        let rule_tokens = tids(&[0, 1, 2, 10, 11]);
        let mut high_postings: HashMap<TokenId, Vec<usize>> = HashMap::new();
        high_postings.insert(tid(0), vec![0]);
        high_postings.insert(tid(1), vec![1]);
        high_postings.insert(tid(2), vec![2]);

        let matchables: BitSet = [0, 1, 2].into_iter().collect();

        let blocks = match_blocks_for_test(
            &query_tokens,
            &rule_tokens,
            0..query_tokens.len(),
            &high_postings,
            5,
            &matchables,
        );

        assert_eq!(blocks.len(), 1, "Should find one match block");
        assert_eq!(
            blocks[0].2, 3,
            "Match should stop at position 3 because positions 3,4 are not in matchables"
        );
    }
}
