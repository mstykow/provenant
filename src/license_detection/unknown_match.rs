//! Unknown license detection using ngram matching.

use crate::license_detection::automaton::Automaton;
use once_cell::sync::Lazy;
use regex::Regex;
use sha1::{Digest, Sha1};

use crate::license_detection::index::LicenseIndex;
use crate::license_detection::index::dictionary::{TokenId, TokenKind};
use crate::license_detection::models::position_span::PositionSpan;
use crate::license_detection::models::{LicenseMatch, MatchCoordinates, MatcherKind};
use crate::license_detection::position_set::PositionSet;
use crate::license_detection::query::Query;
use crate::license_detection::tokenize::STOPWORDS;
use crate::models::LineNumber;
use crate::models::MatchScore;

pub const MATCH_UNKNOWN: MatcherKind = MatcherKind::Unknown;

const UNKNOWN_NGRAM_LENGTH: usize = 6;

const MIN_NGRAM_MATCHES: usize = 3;

const MIN_REGION_LENGTH: usize = 5;

static QUERY_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[^_\W]+\+?[^_\W]*").expect("Invalid regex pattern"));
static MATCHED_TEXT_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?P<token>[^_\W]+\+?[^_\W]*)|(?P<punct>[_\W\s\+]+[_\W\s]?)")
        .expect("Invalid matched text regex pattern")
});

#[derive(Clone)]
struct MatchedTextToken {
    value: String,
    line_num: usize,
    pos: Option<usize>,
    is_text: bool,
    is_known: bool,
    is_matched: bool,
}

pub fn unknown_match(
    index: &LicenseIndex,
    query: &Query,
    known_matches: &[LicenseMatch],
) -> Vec<LicenseMatch> {
    let mut unknown_matches = Vec::new();

    if query.tokens.is_empty() {
        return unknown_matches;
    }

    let query_len = query.tokens.len();

    let covered_positions = compute_covered_positions(query, known_matches);

    let unmatched_regions = find_unmatched_regions(query_len, &covered_positions);

    let automaton = &index.unknown_automaton;

    for region in unmatched_regions {
        let start = region.0;
        let end = region.1;

        let region_length = end - start;
        if region_length < MIN_REGION_LENGTH {
            continue;
        }

        let matched_ngrams = get_matched_ngrams(&query.tokens, start, end, automaton);

        if matched_ngrams.len() < MIN_NGRAM_MATCHES {
            continue;
        }

        let qspan = compute_qspan_union(&matched_ngrams);

        if qspan.is_empty() {
            continue;
        }

        let qspan_length = qspan.len();

        // DEBUG
        #[cfg(debug_assertions)]
        {
            eprintln!("\n=== UNKNOWN MATCH DEBUG ===");
            eprintln!("Region: {}-{} ({} tokens)", start, end, region_length);
            eprintln!("matched_ngrams: {} matches", matched_ngrams.len());
            eprintln!("qspan: {:?}", qspan);
            eprintln!(
                "qspan_length: {} (threshold: {})",
                qspan_length,
                UNKNOWN_NGRAM_LENGTH * 4
            );
        }

        if qspan_length < UNKNOWN_NGRAM_LENGTH * 4 {
            continue;
        }

        let hispan = compute_hispan_from_qspan(&query.tokens, &qspan, index);

        #[cfg(debug_assertions)]
        {
            eprintln!("hispan: {} (threshold: 5)", hispan);
        }

        if hispan < 5 {
            continue;
        }

        if let Some(match_result) = create_unknown_match_from_qspan(query, &qspan) {
            unknown_matches.push(match_result);
        }
    }

    unknown_matches
}

fn compute_covered_positions(_query: &Query, known_matches: &[LicenseMatch]) -> PositionSet {
    let mut covered = PositionSet::new();
    for m in known_matches {
        covered.extend_from_span(m.query_span());
    }
    covered
}

fn find_unmatched_regions(
    query_len: usize,
    covered_positions: &PositionSet,
) -> Vec<(usize, usize)> {
    let mut regions = Vec::new();

    if query_len == 0 {
        return regions;
    }

    let mut region_start = None;

    for pos in 0..query_len {
        if !covered_positions.contains(pos) {
            if region_start.is_none() {
                region_start = Some(pos);
            }
        } else if let Some(start) = region_start {
            regions.push((start, pos));
            region_start = None;
        }
    }

    if let Some(start) = region_start {
        regions.push((start, query_len));
    }

    regions
}

fn get_matched_ngrams(
    tokens: &[TokenId],
    start: usize,
    end: usize,
    automaton: &Automaton,
) -> Vec<(usize, usize)> {
    if start >= end || end > tokens.len() {
        return Vec::new();
    }

    let region_tokens = &tokens[start..end];

    let region_bytes: Vec<u8> = region_tokens
        .iter()
        .flat_map(|tid| tid.to_le_bytes())
        .collect();

    let offset = UNKNOWN_NGRAM_LENGTH;
    let mut matches = Vec::new();

    for m in automaton.find_overlapping_iter(&region_bytes) {
        let local_qend = m.end / 2;
        let qend = start + local_qend;
        let qstart = qend.saturating_sub(offset);
        matches.push((qstart, qend));
    }

    matches
}

fn compute_qspan_union(positions: &[(usize, usize)]) -> PositionSet {
    if positions.is_empty() {
        return PositionSet::new();
    }

    let mut sorted: Vec<_> = positions.to_vec();
    sorted.sort_by_key(|p| p.0);

    let mut merged: Vec<(usize, usize)> = Vec::new();
    let mut current = sorted[0];

    for (start, end) in sorted.into_iter().skip(1) {
        if start <= current.1 {
            current.1 = current.1.max(end);
        } else {
            merged.push(current);
            current = (start, end);
        }
    }
    merged.push(current);

    let mut result = PositionSet::new();
    for (start, end) in merged {
        result.extend_from_span(&PositionSpan::range(start, end));
    }
    result
}

fn compute_hispan_from_qspan(
    tokens: &[TokenId],
    qspan: &PositionSet,
    index: &LicenseIndex,
) -> usize {
    qspan
        .iter()
        .filter(|&pos| {
            tokens
                .get(pos)
                .is_some_and(|&tid| index.dictionary.token_kind(tid) == TokenKind::Legalese)
        })
        .count()
}

fn create_unknown_match_from_qspan(query: &Query, qspan: &PositionSet) -> Option<LicenseMatch> {
    if qspan.is_empty() {
        return None;
    }

    let match_len = qspan.len();

    let start = qspan.min_pos();
    let end = qspan.max_pos() + 1;

    let start_line = query
        .line_by_pos
        .get(start)
        .copied()
        .and_then(LineNumber::new)
        .unwrap_or(LineNumber::ONE);
    let end_line = query
        .line_by_pos
        .get(end.saturating_sub(1))
        .copied()
        .and_then(LineNumber::new)
        .unwrap_or(start_line);

    let qspan_positions: Vec<usize> = qspan.iter().collect();
    let synthetic_rule_text =
        build_unknown_rule_text(query, &qspan_positions, start_line, end_line);
    let rule_identifier = build_unknown_rule_identifier(&synthetic_rule_text);

    let ngram_count = qspan.len();

    let score = calculate_score(ngram_count, match_len);

    let qspan_span = qspan.to_position_span();

    LicenseMatch {
        rid: 0,
        license_expression: "unknown".to_string(),
        license_expression_spdx: None,
        from_file: None,
        start_line,
        end_line,
        start_token: start,
        end_token: end,
        matcher: MATCH_UNKNOWN,
        score,
        matched_length: match_len,
        rule_length: match_len,
        match_coverage: 100.0,
        rule_relevance: 50,
        rule_identifier,
        rule_url: String::new(),
        matched_text: None,
        referenced_filenames: None,
        rule_kind: crate::license_detection::models::RuleKind::None,
        is_from_license: false,
        rule_start_token: 0,
        coordinates: MatchCoordinates::query_region(qspan_span),
        candidate_resemblance: 0.0,
        candidate_containment: 0.0,
    }
    .into()
}

fn build_unknown_rule_text(
    query: &Query,
    qspan_positions: &[usize],
    start_line: LineNumber,
    end_line: LineNumber,
) -> String {
    let Some(&start_pos) = qspan_positions.first() else {
        return String::new();
    };
    let Some(&end_pos) = qspan_positions.last() else {
        return String::new();
    };

    let matched_positions: PositionSet = qspan_positions.iter().copied().collect();
    let tokens = tokenize_matched_unknown_text(&query.text, query);
    let reportable_tokens = collect_reportable_unknown_tokens(
        tokens,
        &matched_positions,
        start_pos,
        end_pos,
        start_line.get(),
        end_line.get(),
    );
    let line_endings = collect_line_endings(&query.text);

    render_unknown_rule_tokens(&reportable_tokens, &line_endings)
}

fn tokenize_matched_unknown_text(text: &str, query: &Query) -> Vec<MatchedTextToken> {
    let mut tokens = Vec::new();
    let mut pos = 0usize;
    let mut line_num = 1usize;

    for line in text.split_inclusive('\n') {
        for capture in MATCHED_TEXT_PATTERN.captures_iter(line) {
            if let Some(token_match) = capture.name("token") {
                let token_text = token_match.as_str();
                let retokenized: Vec<String> = QUERY_PATTERN
                    .find_iter(&token_text.to_lowercase())
                    .map(|m| m.as_str().to_string())
                    .filter(|token| !STOPWORDS.contains(token.as_str()))
                    .collect();

                if retokenized.is_empty() {
                    tokens.push(MatchedTextToken {
                        value: token_text.to_string(),
                        line_num,
                        pos: None,
                        is_text: true,
                        is_known: false,
                        is_matched: false,
                    });
                } else if retokenized.len() == 1 {
                    let token = &retokenized[0];
                    let is_known = query.index.dictionary.get(token).is_some();
                    let token_pos = if is_known {
                        let current_pos = pos;
                        pos += 1;
                        Some(current_pos)
                    } else {
                        None
                    };

                    tokens.push(MatchedTextToken {
                        value: token_text.to_string(),
                        line_num,
                        pos: token_pos,
                        is_text: true,
                        is_known,
                        is_matched: false,
                    });
                } else {
                    for token in retokenized {
                        let is_known = query.index.dictionary.get(&token).is_some();
                        let token_pos = if is_known {
                            let current_pos = pos;
                            pos += 1;
                            Some(current_pos)
                        } else {
                            None
                        };

                        tokens.push(MatchedTextToken {
                            value: token,
                            line_num,
                            pos: token_pos,
                            is_text: true,
                            is_known,
                            is_matched: false,
                        });
                    }
                }
            } else if let Some(punct_match) = capture.name("punct") {
                tokens.push(MatchedTextToken {
                    value: punct_match.as_str().to_string(),
                    line_num,
                    pos: None,
                    is_text: false,
                    is_known: false,
                    is_matched: false,
                });
            }
        }

        line_num += 1;
    }

    tokens
}

fn collect_reportable_unknown_tokens(
    tokens: Vec<MatchedTextToken>,
    matched_positions: &PositionSet,
    start_pos: usize,
    end_pos: usize,
    start_line: usize,
    end_line: usize,
) -> Vec<MatchedTextToken> {
    let mut reportable = Vec::new();
    let mut started = false;
    let mut finished = false;
    let mut end_real_pos = None;
    let mut last_real_pos = None;

    for (real_pos, mut token) in tokens.into_iter().enumerate() {
        if token.line_num < start_line {
            continue;
        }

        if token.line_num > end_line {
            break;
        }

        let mut is_included = false;

        if token
            .pos
            .is_some_and(|pos| token.is_known && matched_positions.contains(pos))
        {
            token.is_matched = true;
            is_included = true;
        }

        if !started && token.pos == Some(start_pos) {
            started = true;
            is_included = true;
        }

        if started && !finished {
            is_included = true;
        }

        if token.pos == Some(end_pos) {
            finished = true;
            started = false;
            end_real_pos = Some(real_pos);
        }

        if finished && !started && end_real_pos.is_some() && last_real_pos == end_real_pos {
            end_real_pos = None;
            if !token.is_text && !token.value.trim().is_empty() {
                is_included = true;
            }
        }

        last_real_pos = Some(real_pos);

        if is_included {
            reportable.push(token);
        }
    }

    reportable
}

fn collect_line_endings(text: &str) -> Vec<String> {
    text.split_inclusive('\n')
        .map(|line| {
            if line.ends_with("\r\n") {
                "\r\n".to_string()
            } else if line.ends_with('\n') {
                "\n".to_string()
            } else {
                String::new()
            }
        })
        .collect()
}

fn render_unknown_rule_tokens(tokens: &[MatchedTextToken], line_endings: &[String]) -> String {
    let mut rendered = String::new();
    let mut previous_line: Option<usize> = None;

    for token in tokens {
        if let Some(prev_line) = previous_line
            && token.line_num > prev_line
        {
            for line in prev_line..token.line_num {
                if let Some(line_ending) = line_endings.get(line.saturating_sub(1)) {
                    rendered.push_str(line_ending.as_str());
                }
            }
        }

        let token_value = if token.is_text {
            token.value.as_str()
        } else {
            token
                .value
                .strip_suffix("\r\n")
                .or_else(|| token.value.strip_suffix('\n'))
                .unwrap_or(token.value.as_str())
        };

        if token.is_text && !STOPWORDS.contains(token.value.to_lowercase().as_str()) {
            if token.is_matched {
                rendered.push_str(token_value);
            } else {
                rendered.push('.');
            }
        } else {
            rendered.push_str(token_value);
        }

        previous_line = Some(token.line_num);
    }

    rendered
}

fn build_unknown_rule_identifier(rule_text: &str) -> String {
    let content = format!("None{}", python_str_repr(rule_text));
    let mut hasher = Sha1::new();
    hasher.update(content.as_bytes());
    let digest = hasher.finalize();

    format!("license-detection-unknown-{}", hex::encode(digest))
}

fn python_str_repr(text: &str) -> String {
    let use_double_quotes = text.contains('\'') && !text.contains('"');
    let quote = if use_double_quotes { '"' } else { '\'' };
    let mut escaped = String::with_capacity(text.len());

    for ch in text.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\'' if !use_double_quotes => escaped.push_str("\\'"),
            '"' if use_double_quotes => escaped.push_str("\\\""),
            _ => escaped.push(ch),
        }
    }

    format!("{quote}{escaped}{quote}")
}

fn calculate_score(ngram_count: usize, match_len: usize) -> MatchScore {
    if match_len == 0 {
        return MatchScore::default();
    }

    let density = ngram_count as f64 / match_len as f64;

    MatchScore::from_percentage(density.min(1.0) * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license_detection::index::LicenseIndex;
    use crate::license_detection::index::dictionary::{TokenId, tid};
    use crate::license_detection::query::Query;

    fn tids(values: &[u16]) -> Vec<TokenId> {
        values.iter().copied().map(tid).collect()
    }

    #[test]
    fn test_unknown_match_empty_query() {
        let index = LicenseIndex::with_legalese_count(10);
        let query = Query::from_extracted_text("", &index, false).expect("Failed to create query");
        let known_matches = vec![];

        let matches = unknown_match(&index, &query, &known_matches);

        assert!(matches.is_empty());
    }

    #[test]
    fn test_find_unmatched_regions_no_coverage() {
        let query_len = 10;
        let covered_positions = PositionSet::new();

        let regions = find_unmatched_regions(query_len, &covered_positions);

        assert_eq!(regions, vec![(0, 10)]);
    }

    #[test]
    fn test_find_unmatched_regions_full_coverage() {
        let query_len = 10;
        let covered_positions: PositionSet = (0..10).collect();

        let regions = find_unmatched_regions(query_len, &covered_positions);

        assert!(regions.is_empty());
    }

    #[test]
    fn test_find_unmatched_regions_partial_coverage() {
        let query_len = 20;
        let covered_positions: PositionSet = [0, 1, 2, 12, 13, 14, 15, 16, 17, 18, 19]
            .iter()
            .copied()
            .collect();

        let regions = find_unmatched_regions(query_len, &covered_positions);

        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0], (3, 12));
    }

    #[test]
    fn test_find_unmatched_regions_trailing_unmatched() {
        let query_len = 20;
        let covered_positions: PositionSet = [0, 1, 2, 3, 4, 5].iter().copied().collect();

        let regions = find_unmatched_regions(query_len, &covered_positions);

        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0], (6, 20));
    }

    #[test]
    fn test_compute_qspan_union_empty() {
        let positions: Vec<(usize, usize)> = Vec::new();
        let merged = compute_qspan_union(&positions);
        assert!(merged.is_empty());
    }

    #[test]
    fn test_compute_qspan_union_single() {
        let positions = vec![(5, 11)];
        let merged = compute_qspan_union(&positions);
        assert_eq!(merged.len(), 6);
        assert!(merged.contains(5));
        assert!(merged.contains(10));
        assert!(!merged.contains(4));
        assert!(!merged.contains(11));
    }

    #[test]
    fn test_compute_qspan_union_overlapping() {
        let positions = vec![(5, 11), (8, 14), (20, 26)];
        let merged = compute_qspan_union(&positions);
        assert_eq!(merged.len(), 15);
        assert!(merged.contains(5));
        assert!(merged.contains(13));
        assert!(!merged.contains(14));
        assert!(merged.contains(20));
        assert!(merged.contains(25));
        assert!(!merged.contains(26));
    }

    #[test]
    fn test_compute_qspan_union_adjacent() {
        let positions = vec![(5, 11), (11, 17)];
        let merged = compute_qspan_union(&positions);
        assert_eq!(merged.len(), 12);
        assert!(merged.contains(5));
        assert!(merged.contains(16));
        assert!(!merged.contains(4));
        assert!(!merged.contains(17));
    }

    #[test]
    fn test_compute_qspan_union_unsorted() {
        let positions = vec![(20, 26), (5, 11), (8, 14)];
        let merged = compute_qspan_union(&positions);
        assert_eq!(merged.len(), 15);
        assert!(merged.contains(5));
        assert!(merged.contains(13));
        assert!(merged.contains(20));
        assert!(merged.contains(25));
    }

    #[test]
    fn test_compute_hispan_from_qspan() {
        let mut index = LicenseIndex::with_legalese_count(0);
        let legalese_entries: Vec<(String, u16)> = (0..15)
            .map(|i| (format!("legalese-{i}"), i as u16))
            .collect();
        index.dictionary =
            crate::license_detection::index::dictionary::TokenDictionary::new_with_legalese(
                &legalese_entries
                    .iter()
                    .map(|(token, id)| (token.as_str(), *id))
                    .collect::<Vec<_>>(),
            );

        let mut tokens: Vec<TokenId> = (0..15)
            .map(|i| {
                index
                    .dictionary
                    .get_token_id(&format!("legalese-{i}"))
                    .unwrap()
            })
            .collect();
        for i in 15..30 {
            tokens.push(index.dictionary.get_or_assign(&format!("regular-{i}")));
        }
        let qspan: PositionSet = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 20, 21, 22, 23, 24]
            .iter()
            .copied()
            .collect();
        let hispan = compute_hispan_from_qspan(&tokens, &qspan, &index);
        assert_eq!(hispan, 10);
    }

    #[test]
    fn test_get_matched_ngrams_empty_automaton() {
        use crate::license_detection::automaton::AutomatonBuilder;

        let tokens = tids(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let automaton = AutomatonBuilder::new().build();

        let matches = get_matched_ngrams(&tokens, 0, 8, &automaton);

        assert!(matches.is_empty());
    }

    #[test]
    fn test_get_matched_ngrams_with_matches() {
        use crate::license_detection::automaton::AutomatonBuilder;

        let tokens: Vec<TokenId> = (0..30).map(tid).collect();
        let ngram: Vec<u8> = vec![0, 0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0];

        let mut builder = AutomatonBuilder::new();
        builder.add_pattern(&ngram);
        let automaton = builder.build();

        let matches = get_matched_ngrams(&tokens, 0, 30, &automaton);

        assert!(!matches.is_empty(), "Should find ngram matches");

        for (qstart, qend) in &matches {
            assert_eq!(*qend - *qstart, UNKNOWN_NGRAM_LENGTH);
        }
    }

    #[test]
    fn test_get_matched_ngrams_keeps_overlapping_matches() {
        use crate::license_detection::automaton::AutomatonBuilder;

        let tokens = tids(&[1, 2, 3, 1, 2, 3, 1, 2, 3]);
        let overlapping_ngram: Vec<u8> = tokens[..UNKNOWN_NGRAM_LENGTH]
            .iter()
            .flat_map(|tid| tid.to_le_bytes())
            .collect();

        let mut builder = AutomatonBuilder::new();
        builder.add_pattern(&overlapping_ngram);
        let automaton = builder.build();

        let matches = get_matched_ngrams(&tokens, 0, tokens.len(), &automaton);

        assert_eq!(matches, vec![(0, 6), (3, 9)]);
    }

    #[test]
    fn test_calculate_score() {
        let score1 = calculate_score(5, 10);
        let score2 = calculate_score(10, 10);
        let score3 = calculate_score(0, 10);

        assert!(score2 > score1);
        assert!(score2 <= MatchScore::MAX);
        assert_eq!(score3, MatchScore::default());
    }

    #[test]
    fn test_find_unmatched_regions_leading_unmatched() {
        let query_len = 20;
        let covered_positions: PositionSet = [10, 11, 12, 13, 14, 15, 16, 17, 18, 19]
            .iter()
            .copied()
            .collect();

        let regions = find_unmatched_regions(query_len, &covered_positions);

        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0], (0, 10));
    }

    #[test]
    fn test_find_unmatched_regions_middle_gap() {
        let query_len = 30;
        let covered_positions: PositionSet =
            [0, 1, 2, 3, 4, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29]
                .iter()
                .copied()
                .collect();

        let regions = find_unmatched_regions(query_len, &covered_positions);

        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0], (5, 20));
    }

    #[test]
    fn test_compute_covered_positions_gapped_qspan() {
        let index = LicenseIndex::with_legalese_count(10);
        let query = Query::from_extracted_text("some license text here", &index, false)
            .expect("Failed to create query");

        let known_matches = vec![LicenseMatch {
            rid: 0,
            license_expression: "test".to_string(),
            license_expression_spdx: Some("TEST".to_string()),
            from_file: None,
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            start_token: 0,
            end_token: 10,
            matcher: MatcherKind::Aho,
            score: MatchScore::MAX,
            matched_length: 6,
            rule_length: 6,
            match_coverage: 100.0,
            rule_relevance: 100,
            rule_identifier: "test-rule".to_string(),
            rule_url: String::new(),
            matched_text: Some("matched text".to_string()),
            referenced_filenames: None,
            rule_kind: crate::license_detection::models::RuleKind::None,
            is_from_license: false,
            rule_start_token: 0,
            coordinates: MatchCoordinates::query_region(PositionSpan::from_positions(vec![
                0, 1, 2, 7, 8, 9,
            ])),
            candidate_resemblance: 0.0,
            candidate_containment: 0.0,
        }];

        let covered = compute_covered_positions(&query, &known_matches);

        assert!(covered.contains(0), "Should contain position 0");
        assert!(covered.contains(2), "Should contain position 2");
        assert!(covered.contains(7), "Should contain position 7");
        assert!(covered.contains(9), "Should contain position 9");
        assert!(!covered.contains(3), "Should NOT contain position 3 (gap)");
        assert!(!covered.contains(5), "Should NOT contain position 5 (gap)");
        assert!(
            !covered.contains(10),
            "Should NOT contain position 10 (outside)"
        );
    }

    #[test]
    fn test_compute_covered_positions_fallback_contiguous() {
        let index = LicenseIndex::with_legalese_count(10);
        let query = Query::from_extracted_text("some license text here", &index, false)
            .expect("Failed to create query");

        let known_matches = vec![LicenseMatch {
            rid: 0,
            license_expression: "test".to_string(),
            license_expression_spdx: Some("TEST".to_string()),
            from_file: None,
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            start_token: 5,
            end_token: 10,
            matcher: MatcherKind::Aho,
            score: MatchScore::MAX,
            matched_length: 5,
            rule_length: 5,
            match_coverage: 100.0,
            rule_relevance: 100,
            rule_identifier: "test-rule".to_string(),
            rule_url: String::new(),
            matched_text: Some("matched text".to_string()),
            referenced_filenames: None,
            rule_kind: crate::license_detection::models::RuleKind::None,
            is_from_license: false,
            rule_start_token: 0,
            coordinates: MatchCoordinates::query_region(PositionSpan::range(5, 10)),
            candidate_resemblance: 0.0,
            candidate_containment: 0.0,
        }];

        let covered = compute_covered_positions(&query, &known_matches);

        assert!(covered.contains(5), "Should contain position 5");
        assert!(covered.contains(7), "Should contain position 7");
        assert!(covered.contains(9), "Should contain position 9");
        assert!(
            !covered.contains(4),
            "Should NOT contain position 4 (before)"
        );
        assert!(
            !covered.contains(10),
            "Should NOT contain position 10 (after)"
        );
    }

    #[test]
    fn test_compute_covered_positions_qspan_creates_extra_unmatched_region() {
        let index = LicenseIndex::with_legalese_count(10);
        let query = Query::from_extracted_text("some license text here", &index, false)
            .expect("Failed to create query");

        let known_matches = vec![LicenseMatch {
            rid: 0,
            license_expression: "test".to_string(),
            license_expression_spdx: Some("TEST".to_string()),
            from_file: None,
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            start_token: 0,
            end_token: 15,
            matcher: MatcherKind::Aho,
            score: MatchScore::MAX,
            matched_length: 8,
            rule_length: 8,
            match_coverage: 100.0,
            rule_relevance: 100,
            rule_identifier: "test-rule".to_string(),
            rule_url: String::new(),
            matched_text: Some("matched text".to_string()),
            referenced_filenames: None,
            rule_kind: crate::license_detection::models::RuleKind::None,
            is_from_license: false,
            rule_start_token: 0,
            coordinates: MatchCoordinates::query_region(PositionSpan::from_positions(vec![
                0, 1, 2, 3, 11, 12, 13, 14,
            ])),
            candidate_resemblance: 0.0,
            candidate_containment: 0.0,
        }];

        let covered = compute_covered_positions(&query, &known_matches);
        let regions = find_unmatched_regions(20, &covered);

        assert!(
            regions.contains(&(4, 11)),
            "Should have unmatched region 4-11 (the gap in qspan_positions), got: {:?}",
            regions
        );
        assert!(
            regions.contains(&(15, 20)),
            "Should have trailing unmatched region 15-20, got: {:?}",
            regions
        );

        let contiguous_covered: PositionSet = (0..15).collect();
        let contiguous_regions = find_unmatched_regions(20, &contiguous_covered);
        assert_eq!(
            contiguous_regions,
            vec![(15, 20)],
            "Contiguous coverage would collapse the gap, producing only trailing region"
        );
    }

    #[test]
    fn test_create_unknown_match_from_qspan_valid() {
        use crate::license_detection::test_utils::create_mock_query_with_tokens;

        let index = LicenseIndex::with_legalese_count(10);

        let tokens: Vec<u16> = (0..30).collect();
        let query = create_mock_query_with_tokens(&tokens, &index);

        let qspan: PositionSet = (0..30).collect();

        let match_result = create_unknown_match_from_qspan(&query, &qspan);

        assert!(
            match_result.is_some(),
            "Should create unknown match for sufficient length"
        );

        let m = match_result.unwrap();
        assert_eq!(m.license_expression, "unknown");
        assert_eq!(m.matcher, MATCH_UNKNOWN);
        assert!(!m.coordinates.query_span().is_empty());
    }

    #[test]
    fn test_unknown_match_with_known_matches() {
        let index = LicenseIndex::with_legalese_count(10);
        let text = "some text that is license related and should be detected";
        let query =
            Query::from_extracted_text(text, &index, false).expect("Failed to create query");

        let known_matches = vec![LicenseMatch {
            rid: 0,
            license_expression: "mit".to_string(),
            license_expression_spdx: Some("MIT".to_string()),
            from_file: None,
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            start_token: 0,
            end_token: 5,
            matcher: MatcherKind::Aho,
            score: MatchScore::MAX,
            matched_length: 5,
            rule_length: 5,
            match_coverage: 100.0,
            rule_relevance: 100,
            rule_identifier: "test-rule".to_string(),
            rule_url: String::new(),
            matched_text: Some("some text".to_string()),
            referenced_filenames: None,
            rule_kind: crate::license_detection::models::RuleKind::None,
            is_from_license: false,
            rule_start_token: 0,
            coordinates: MatchCoordinates::query_region(PositionSpan::range(0, 5)),
            candidate_resemblance: 0.0,
            candidate_containment: 0.0,
        }];

        let matches = unknown_match(&index, &query, &known_matches);

        assert!(
            matches.is_empty() || matches[0].start_line > LineNumber::ONE,
            "Should not re-detect known regions"
        );
    }

    #[test]
    fn test_calculate_score_edge_cases() {
        let score_zero_length = calculate_score(10, 0);
        assert_eq!(
            score_zero_length,
            MatchScore::default(),
            "Zero length should have zero score"
        );

        let score_zero_ngrams = calculate_score(0, 100);
        assert_eq!(
            score_zero_ngrams,
            MatchScore::default(),
            "Zero ngrams should have zero score"
        );

        let score_high_density = calculate_score(100, 50);
        assert_eq!(
            score_high_density,
            MatchScore::MAX,
            "High density should be capped at 100.0"
        );
    }

    #[test]
    fn test_get_matched_ngrams_out_of_bounds() {
        use crate::license_detection::automaton::AutomatonBuilder;

        let tokens = tids(&[1, 2, 3]);
        let automaton = AutomatonBuilder::new().build();

        let matches = get_matched_ngrams(&tokens, 5, 10, &automaton);
        assert!(matches.is_empty(), "Out of bounds should return empty");

        let matches = get_matched_ngrams(&tokens, 2, 1, &automaton);
        assert!(matches.is_empty(), "Invalid range should return empty");
    }
}
