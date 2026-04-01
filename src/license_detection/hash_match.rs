//! Hash-based exact matching for license detection.
//!
//! This module implements the hash matching strategy which computes a hash of the
//! entire query token sequence and looks for exact matches in the index.

use sha1::{Digest, Sha1};

use crate::license_detection::index::LicenseIndex;
use crate::license_detection::index::dictionary::{TokenId, TokenKind};
use crate::license_detection::models::position_span::PositionSpan;
use crate::license_detection::models::{LicenseMatch, MatchCoordinates, MatcherKind};
use crate::license_detection::query::QueryRun;

pub const MATCH_HASH: MatcherKind = MatcherKind::Hash;

/// Compute a SHA1 hash of a token sequence.
///
/// Converts token IDs to signed 16-bit integers (matching Python's `array('h')`),
/// serializes them as little-endian bytes, and computes the SHA1 hash.
///
/// # Arguments
/// * `tokens` - Slice of token IDs
///
/// # Returns
/// 20-byte SHA1 digest
///
/// Corresponds to Python: `tokens_hash()` (lines 44-49)
pub fn compute_hash(tokens: &[TokenId]) -> [u8; 20] {
    let mut hasher = Sha1::new();

    for token in tokens {
        let signed = token.raw() as i16;
        hasher.update(signed.to_le_bytes());
    }

    hasher.finalize().into()
}

/// Perform hash-based matching for a query run.
///
/// Computes the hash of the query token sequence and looks for exact matches
/// in the index. If found, returns a single LicenseMatch with 100% coverage.
///
/// # Arguments
/// * `index` - The license index
/// * `query_run` - The query run to match
///
/// # Returns
/// Vector of matches (0 or 1 match)
///
/// Corresponds to Python: `hash_match()` (lines 59-87)
pub fn hash_match(index: &LicenseIndex, query_run: &QueryRun) -> Vec<LicenseMatch> {
    let mut matches = Vec::new();
    let query_hash = compute_hash(query_run.tokens());

    if let Some(&rid) = index.rid_by_hash.get(&query_hash) {
        let rule = &index.rules_by_rid[rid];
        let itokens = &index.tids_by_rid[rid];

        let rule_length = rule.tokens.len();

        let matched_length = query_run.tokens().len();
        let match_coverage = 100.0;

        let start_line = query_run.line_for_pos(query_run.start).unwrap_or(1);
        let end_line = if let Some(end) = query_run.end {
            query_run.line_for_pos(end).unwrap_or(start_line)
        } else {
            start_line
        };

        let end = query_run.end.unwrap_or(query_run.start);
        let qspan = PositionSpan::range(query_run.start, end + 1);
        let ispan = PositionSpan::range(0, rule_length);
        let hispan = PositionSpan::from_positions(
            (0..rule_length)
                .filter(|&p| index.dictionary.token_kind(itokens[p]) == TokenKind::Legalese),
        );

        let license_match = LicenseMatch {
            license_expression: rule.license_expression.clone(),
            license_expression_spdx: index
                .rule_metadata_by_identifier
                .get(&rule.identifier)
                .and_then(|metadata| metadata.license_expression_spdx.clone()),
            from_file: None,
            start_line,
            end_line,
            start_token: query_run.start,
            end_token: query_run.end.map_or(query_run.start, |e| e + 1),
            matcher: MATCH_HASH,
            score: 100.0,
            matched_length,
            rule_length,
            match_coverage,
            rule_relevance: rule.relevance,
            rid,
            rule_identifier: rule.identifier.clone(),
            rule_url: rule.rule_url().unwrap_or_default(),
            matched_text: None,
            referenced_filenames: rule.referenced_filenames.clone(),
            rule_kind: rule.kind(),
            is_from_license: rule.is_from_license,
            rule_start_token: 0,
            coordinates: MatchCoordinates::rule_aligned(qspan, ispan, hispan),
            candidate_resemblance: 0.0,
            candidate_containment: 0.0,
        };

        matches.push(license_match);
    }

    matches
}

#[cfg(test)]
#[path = "hash_match_test.rs"]
mod tests;
