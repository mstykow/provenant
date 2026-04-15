//! Query processing - tokenized input for license matching.

use crate::license_detection::index::LicenseIndex;
use crate::license_detection::index::dictionary::{KnownToken, QueryToken, TokenId, TokenKind};
use crate::license_detection::models::PositionSpan;
use crate::license_detection::position_set::PositionSet;
use crate::license_detection::spdx_lid::split_spdx_lid;
use crate::license_detection::tokenize::STOPWORDS;
use crate::license_detection::tokenize::tokenize_as_ids;
use regex::Regex;
use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::sync::LazyLock;

static QUERY_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[^_\W]+\+?[^_\W]*").expect("valid query regex"));
static MATCHED_TEXT_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?P<token>[^_\W]+\+?[^_\W]*)|(?P<punct>[_\W\s\+]+[_\W\s]?)")
        .expect("valid matched text regex")
});

#[derive(Clone)]
struct MatchedTextToken {
    value: String,
    line_num: usize,
    pos: Option<usize>,
    is_text: bool,
    is_matched: bool,
}

///
/// Query holds:
/// - Known token IDs (tokens existing in the index dictionary)
/// - Token positions and their corresponding line numbers (line_by_pos)
/// - Unknown tokens (tokens not in dictionary) tracked per position
/// - Stopwords tracked per position
/// - Positions with short/digit-only tokens
/// - High and low matchable token positions (for tracking what's been matched)
///
/// Based on Python Query class at:
/// reference/scancode-toolkit/src/licensedcode/query.py (lines 155-295)
#[derive(Debug)]
pub struct Query<'a> {
    /// The original input text.
    ///
    /// Corresponds to Python: `self.query_string` (line 215)
    pub text: String,

    /// Token IDs for known tokens (tokens found in the index dictionary)
    ///
    /// Corresponds to Python: `self.tokens = []` (line 228)
    pub tokens: Vec<TokenId>,

    /// Mapping from token position to line number (1-based)
    ///
    /// Each token position in `self.tokens` maps to the line number where it appears.
    /// This is used for match position reporting.
    ///
    /// Corresponds to Python: `self.line_by_pos = []` (line 231)
    pub line_by_pos: Vec<usize>,

    /// Mapping from token position to count of unknown tokens after that position
    ///
    /// Unknown tokens are those not found in the dictionary. We track them by
    /// counting how many unknown tokens appear after each known position.
    /// Unknown tokens before the first known token are tracked with the key `None`.
    ///
    /// Corresponds to Python: `self.unknowns_by_pos = {}` (line 236)
    pub unknowns_by_pos: HashMap<Option<usize>, usize>,

    /// Mapping from token position to count of stopwords after that position
    ///
    /// Similar to unknown_tokens, but for stopwords.
    ///
    /// Corresponds to Python: `self.stopwords_by_pos = {}` (line 244)
    pub stopwords_by_pos: HashMap<Option<usize>, usize>,

    /// Set of positions with single-character or digit-only tokens
    ///
    /// These tokens have special handling in matching.
    ///
    /// Corresponds to Python: `self.shorts_and_digits_pos = set()` (line 249)
    pub shorts_and_digits_pos: PositionSet,

    /// High-value matchable token positions (legalese tokens)
    ///
    /// These are tokens with ID < len_legalese.
    ///
    /// Corresponds to Python: `self.high_matchables` (line 293)
    pub high_matchables: PositionSet,

    /// Low-value matchable token positions (non-legalese tokens)
    ///
    /// These are tokens with ID >= len_legalese.
    ///
    /// Corresponds to Python: `self.low_matchables` (line 294)
    pub low_matchables: PositionSet,

    /// True if the query is detected as binary content
    ///
    /// Corresponds to Python: `self.is_binary = False` (line 225)
    pub is_binary: bool,

    /// Raw query run ranges (start, end) computed during tokenization.
    ///
    /// QueryRuns are created on-demand from these ranges.
    ///
    /// Corresponds to Python: `self.query_runs = []` (line 274)
    pub(crate) query_run_ranges: Vec<(usize, Option<usize>)>,

    /// SPDX-License-Identifier lines found during tokenization.
    ///
    /// Each tuple is (spdx_text, start_token_pos, end_token_pos).
    /// Used for creating LicenseMatches with correct token positions.
    ///
    /// Corresponds to Python: `self.spdx_lines = []` (line 507)
    pub spdx_lines: Vec<(String, usize, usize)>,

    /// Reference to the license index for dictionary access and metadata
    pub index: &'a LicenseIndex,
}

pub fn matched_text_from_text(text: &str, start_line: usize, end_line: usize) -> String {
    if start_line == 0 || end_line == 0 || start_line > end_line {
        return String::new();
    }

    text.lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let line_num = idx + 1;
            if line_num >= start_line && line_num <= end_line {
                Some(line)
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn matched_text_diagnostics_from_text(
    text: &str,
    query: &Query<'_>,
    matched_positions: &PositionSet,
    start_pos: usize,
    end_pos: usize,
    start_line: usize,
    end_line: usize,
) -> String {
    let tokens = tokenize_matched_text(text, query);
    let reportable_tokens = collect_reportable_tokens(
        tokens,
        matched_positions,
        start_pos,
        end_pos,
        start_line,
        end_line,
    );
    let line_endings = collect_line_endings(text);

    render_diagnostic_tokens(&reportable_tokens, &line_endings)
}

fn tokenize_matched_text(text: &str, query: &Query<'_>) -> Vec<MatchedTextToken> {
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
                        is_matched: false,
                    });
                } else if retokenized.len() == 1 {
                    let token = &retokenized[0];
                    let token_pos = if query.index.dictionary.get(token).is_some() {
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
                        is_matched: false,
                    });
                } else {
                    for token in retokenized {
                        let token_pos = if query.index.dictionary.get(&token).is_some() {
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
                    is_matched: false,
                });
            }
        }

        line_num += 1;
    }

    tokens
}

fn collect_reportable_tokens(
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

        if token.pos.is_some_and(|pos| matched_positions.contains(pos)) {
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

fn render_diagnostic_tokens(tokens: &[MatchedTextToken], line_endings: &[String]) -> String {
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
                rendered.push('[');
                rendered.push_str(token_value);
                rendered.push(']');
            }
        } else {
            rendered.push_str(token_value);
        }

        previous_line = Some(token.line_num);
    }

    rendered
}

impl<'a> Query<'a> {
    /// Create a new query from text string and license index.
    ///
    /// This tokenizes the input text, looks up each token in the index dictionary,
    /// and builds the query structures for matching.
    ///
    /// # Arguments
    /// * `text` - The input text to tokenize
    /// * `index` - The license index containing the token dictionary
    ///
    /// # Returns
    /// A Result containing the Query or an error if binary detection fails
    ///
    /// Detection scans file-like text, so this uses Python's
    /// `build_query(..., text_line_threshold=15)` threshold.
    const TEXT_LINE_THRESHOLD: usize = 15;
    const BINARY_LINE_THRESHOLD: usize = 50;
    const MAX_TOKEN_PER_LINE: usize = 25;

    fn compute_spdx_offset(
        tokens: &[QueryToken],
        dictionary: &crate::license_detection::index::dictionary::TokenDictionary,
    ) -> Option<usize> {
        let get_known_id = |i: usize| -> Option<TokenId> {
            match tokens.get(i)? {
                QueryToken::Known(known) => Some(known.id),
                _ => None,
            }
        };

        let spdx_id = dictionary.get("spdx")?;
        let license_id = dictionary.get("license")?;
        let identifier_id = dictionary.get("identifier")?;
        let licence_id = dictionary.get("licence");

        let licenses_id = dictionary.get("licenses");
        let nuget_id = dictionary.get("nuget");
        let org_id = dictionary.get("org");

        let is_spdx_prefix = |ids: [Option<TokenId>; 3]| -> bool {
            ids.iter().all(|id| id.is_some())
                && ids[0] == Some(spdx_id)
                && (ids[1] == Some(license_id) || ids[1] == licence_id)
                && ids[2] == Some(identifier_id)
        };

        let is_nuget_prefix = |ids: [Option<TokenId>; 3]| -> bool {
            licenses_id.is_some()
                && nuget_id.is_some()
                && org_id.is_some()
                && ids[0] == licenses_id
                && ids[1] == Some(nuget_id.unwrap())
                && ids[2] == Some(org_id.unwrap())
        };

        if tokens.len() >= 3 {
            let first_three = [get_known_id(0), get_known_id(1), get_known_id(2)];
            if is_spdx_prefix(first_three) || is_nuget_prefix(first_three) {
                return Some(0);
            }
        }

        if tokens.len() >= 4 {
            let second_three = [get_known_id(1), get_known_id(2), get_known_id(3)];
            if is_spdx_prefix(second_three) || is_nuget_prefix(second_three) {
                return Some(1);
            }
        }

        if tokens.len() >= 5 {
            let third_three = [get_known_id(2), get_known_id(3), get_known_id(4)];
            if is_spdx_prefix(third_three) || is_nuget_prefix(third_three) {
                return Some(2);
            }
        }

        None
    }

    pub fn from_extracted_text(
        text: &str,
        index: &'a LicenseIndex,
        binary_derived: bool,
    ) -> Result<Self, anyhow::Error> {
        let line_threshold = if binary_derived {
            Self::BINARY_LINE_THRESHOLD
        } else {
            Self::TEXT_LINE_THRESHOLD
        };

        Self::with_source_options(text, index, line_threshold, Some(binary_derived))
    }

    /// Iterate over query runs.
    ///
    /// Corresponds to Python: `query.query_runs` property iteration
    pub fn query_runs(&self) -> Vec<QueryRun<'_>> {
        self.query_run_ranges
            .iter()
            .map(|&(start, end)| QueryRun::new(self, start, end))
            .collect()
    }

    fn with_source_options(
        text: &str,
        index: &'a LicenseIndex,
        line_threshold: usize,
        binary_derived: Option<bool>,
    ) -> Result<Self, anyhow::Error> {
        let is_binary = match binary_derived {
            Some(is_binary) => is_binary,
            None => Self::detect_binary(text)?,
        };
        let has_long_lines = Self::detect_long_lines(text);

        let mut tokens = Vec::new();
        let mut line_by_pos = Vec::new();
        let mut unknowns_by_pos: HashMap<Option<usize>, usize> = HashMap::new();
        let mut stopwords_by_pos: HashMap<Option<usize>, usize> = HashMap::new();
        let mut shorts_and_digits_pos = PositionSet::new();
        let mut spdx_lines: Vec<(String, usize, usize)> = Vec::new();

        let mut known_pos: Option<usize> = None;
        let mut started = false;
        let mut current_line = 1usize;

        let mut tokens_by_line: Vec<Vec<Option<KnownToken>>> = Vec::new();

        for line in text.lines() {
            let line_trimmed = line.trim();
            let mut line_tokens: Vec<Option<KnownToken>> = Vec::new();

            let mut line_first_known_pos = None;

            let line_query_tokens = tokenize_as_ids(line_trimmed, &index.dictionary);

            for query_token in &line_query_tokens {
                match query_token {
                    QueryToken::Known(known_token) => {
                        known_pos = Some(known_pos.map_or(0, |p| p + 1));
                        started = true;
                        tokens.push(known_token.id);
                        line_by_pos.push(current_line);
                        line_tokens.push(Some(*known_token));

                        if line_first_known_pos.is_none() {
                            line_first_known_pos = known_pos;
                        }

                        if known_token.is_short_or_digit {
                            let _ = shorts_and_digits_pos.insert(known_pos.unwrap());
                        }
                    }
                    QueryToken::Unknown if !started => {
                        *unknowns_by_pos.entry(None).or_insert(0) += 1;
                        line_tokens.push(None);
                    }
                    QueryToken::Unknown => {
                        *unknowns_by_pos.entry(known_pos).or_insert(0) += 1;
                        line_tokens.push(None);
                    }
                    QueryToken::Stopword if !started => {
                        *stopwords_by_pos.entry(None).or_insert(0) += 1;
                    }
                    QueryToken::Stopword => {
                        *stopwords_by_pos.entry(known_pos).or_insert(0) += 1;
                    }
                }
            }

            let line_last_known_pos = known_pos;

            let spdx_start_offset =
                Self::compute_spdx_offset(&line_query_tokens, &index.dictionary);

            if let Some(offset) = spdx_start_offset
                && let Some(line_first_known_pos) = line_first_known_pos
            {
                let (spdx_prefix, spdx_expression) = split_spdx_lid(line);
                let spdx_text = format!("{}{}", spdx_prefix.unwrap_or_default(), spdx_expression);
                let spdx_start_known_pos = line_first_known_pos + offset;

                if spdx_start_known_pos <= line_last_known_pos.unwrap() {
                    let spdx_end = line_last_known_pos.unwrap() + 1;
                    spdx_lines.push((spdx_text, spdx_start_known_pos, spdx_end));
                }
            }

            tokens_by_line.push(line_tokens);
            current_line += 1;
        }

        let high_matchables: PositionSet = tokens
            .iter()
            .enumerate()
            .filter(|(_pos, tid)| index.dictionary.token_kind(**tid) == TokenKind::Legalese)
            .map(|(pos, _tid)| pos)
            .collect();

        let low_matchables: PositionSet = tokens
            .iter()
            .enumerate()
            .filter(|(_pos, tid)| index.dictionary.token_kind(**tid) == TokenKind::Regular)
            .map(|(pos, _tid)| pos)
            .collect();

        let query_runs = Self::compute_query_runs(&tokens_by_line, line_threshold, has_long_lines);

        Ok(Query {
            text: text.to_string(),
            tokens,
            line_by_pos,
            unknowns_by_pos,
            stopwords_by_pos,
            shorts_and_digits_pos,
            high_matchables,
            low_matchables,
            is_binary,
            query_run_ranges: query_runs,
            spdx_lines,
            index,
        })
    }

    /// Detect if text is binary content.
    ///
    /// Binary detection checks for:
    /// - Null bytes (0x00)
    /// - High ratio of non-printable characters
    ///
    /// # Arguments
    /// * `text` - The text to analyze
    ///
    /// # Returns
    /// true if binary, false otherwise
    ///
    /// Corresponds to Python: `typecode.get_type().is_binary` usage (lines 123-135)
    fn detect_binary(text: &str) -> Result<bool, anyhow::Error> {
        let null_byte_count = text.bytes().filter(|&b| b == 0).count();

        if null_byte_count > 0 {
            return Ok(true);
        }

        let non_printable_ratio = text
            .chars()
            .filter(|&c| {
                !c.is_ascii() && !c.is_ascii_graphic() && c != '\n' && c != '\r' && c != '\t'
            })
            .count() as f64
            / text.len().max(1) as f64;

        Ok(non_printable_ratio > 0.3)
    }

    /// Detect if text has very long lines (for minified JS/CSS).
    ///
    /// # Arguments
    /// * `text` - The text to analyze
    ///
    /// # Returns
    /// true if there are lines with many tokens, false otherwise
    ///
    /// Corresponds to Python: `typecode.get_type().is_text_with_long_lines` usage
    fn detect_long_lines(text: &str) -> bool {
        text.lines()
            .any(|line| crate::license_detection::tokenize::count_tokens(line) > 25)
    }

    fn break_long_lines(lines: &[Vec<Option<KnownToken>>]) -> Vec<Vec<Option<KnownToken>>> {
        lines
            .iter()
            .flat_map(|line| {
                if line.is_empty() {
                    return Vec::new();
                }

                if line.len() <= Self::MAX_TOKEN_PER_LINE {
                    vec![line.clone()]
                } else {
                    line.chunks(Self::MAX_TOKEN_PER_LINE)
                        .map(|chunk| chunk.to_vec())
                        .collect()
                }
            })
            .collect()
    }

    fn compute_query_runs(
        tokens_by_line: &[Vec<Option<KnownToken>>],
        line_threshold: usize,
        has_long_lines: bool,
    ) -> Vec<(usize, Option<usize>)> {
        let processed_lines = if has_long_lines {
            Self::break_long_lines(tokens_by_line)
        } else {
            tokens_by_line.to_vec()
        };

        let mut query_runs = Vec::new();
        let mut query_run_start = 0usize;
        let mut query_run_end = None;
        let mut empty_lines = 0usize;
        let mut pos = 0usize;
        let mut query_run_is_all_digit = true;

        for line_tokens in processed_lines {
            if query_run_end.is_some() && empty_lines >= line_threshold {
                if !query_run_is_all_digit {
                    query_runs.push((query_run_start, query_run_end));
                }
                query_run_start = pos;
                query_run_end = None;
                empty_lines = 0;
                query_run_is_all_digit = true;
            }

            if query_run_end.is_none() {
                query_run_start = pos;
            }

            if line_tokens.is_empty() {
                empty_lines += 1;
                continue;
            }

            let line_is_all_digit = line_tokens
                .iter()
                .all(|token_id| token_id.map(|known| known.is_digit_only).unwrap_or(true));
            let mut line_has_known_tokens = false;
            let mut line_has_good_tokens = false;

            for known in line_tokens.into_iter().flatten() {
                line_has_known_tokens = true;
                if known.kind == TokenKind::Legalese {
                    line_has_good_tokens = true;
                }
                if !known.is_digit_only {
                    query_run_is_all_digit = false;
                }
                query_run_end = Some(pos);
                pos += 1;
            }

            if line_is_all_digit || !line_has_known_tokens {
                empty_lines += 1;
                continue;
            }

            if line_has_good_tokens {
                empty_lines = 0;
            } else {
                empty_lines += 1;
            }
        }

        if let Some(end) = query_run_end
            && !query_run_is_all_digit
        {
            query_runs.push((query_run_start, Some(end)));
        }

        query_runs
    }

    /// Get the length of the query in tokens.
    ///
    /// Get the line number for a token position.
    ///
    /// # Arguments
    /// * `pos` - The token position
    ///
    /// # Returns
    /// The line number (1-based)
    #[inline]
    pub fn line_for_pos(&self, pos: usize) -> Option<usize> {
        self.line_by_pos.get(pos).copied()
    }

    /// Check if the query is empty (no known tokens).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// Get a query run covering the entire query.
    ///
    /// Corresponds to Python: `whole_query_run()` method (lines 306-317)
    pub fn whole_query_run(&self) -> QueryRun<'a> {
        QueryRun::whole_query_snapshot(self)
    }

    /// Subtract matched span positions from matchables.
    ///
    /// This removes the positions from both high and low matchables.
    ///
    /// # Arguments
    /// * `span` - The span of positions to subtract
    ///
    /// Corresponds to Python: `subtract()` method (lines 328-334)
    pub fn subtract(&mut self, span: &PositionSpan) {
        self.high_matchables.remove_span(span);
        self.low_matchables.remove_span(span);
    }

    /// Extract matched text for a given line range.
    ///
    /// Returns the text from the original input between start_line and end_line
    /// (both inclusive, 1-indexed).
    ///
    /// # Arguments
    /// * `start_line` - Starting line number (1-indexed)
    /// * `end_line` - Ending line number (1-indexed)
    ///
    /// # Returns
    /// The matched text, or empty string if lines are out of range
    ///
    /// Corresponds to Python: `matched_text()` method in match.py (lines 757-795)
    pub fn matched_text(&self, start_line: usize, end_line: usize) -> String {
        matched_text_from_text(&self.text, start_line, end_line)
    }
}

#[derive(Debug, Clone)]
struct WholeQueryRunSnapshot<'a> {
    index: &'a LicenseIndex,
    tokens: Vec<TokenId>,
    line_by_pos: Vec<usize>,
    high_matchables: PositionSet,
    low_matchables: PositionSet,
}

/// A query run is a slice of query tokens identified by a start and end positions.
///
/// Query runs break a query into manageable chunks for efficient matching.
/// They track matchable token positions and support subtraction of matched spans.
///
/// Based on Python QueryRun class at:
/// reference/scancode-toolkit/src/licensedcode/query.py (lines 720-914)
#[derive(Debug, Clone)]
pub struct QueryRun<'a> {
    query: Option<&'a Query<'a>>,
    whole_query_snapshot: Option<WholeQueryRunSnapshot<'a>>,
    pub start: usize,
    pub end: Option<usize>,
    cached_high_matchables: OnceCell<PositionSet>,
    cached_low_matchables: OnceCell<PositionSet>,
    combined_matchables: RefCell<Option<PositionSet>>,
}

impl<'a> QueryRun<'a> {
    /// Create a new query run from a query with start and end positions.
    ///
    /// # Arguments
    /// * `query` - The parent query
    /// * `start` - The start position (inclusive)
    /// * `end` - The end position (inclusive), or None for an empty run
    ///
    /// Corresponds to Python: `QueryRun.__init__()` (lines 735-749)
    pub fn new(query: &'a Query<'a>, start: usize, end: Option<usize>) -> Self {
        Self {
            query: Some(query),
            whole_query_snapshot: None,
            start,
            end,
            cached_high_matchables: OnceCell::new(),
            cached_low_matchables: OnceCell::new(),
            combined_matchables: RefCell::new(None),
        }
    }

    fn whole_query_snapshot(query: &Query<'a>) -> Self {
        let end = if query.is_empty() {
            None
        } else {
            Some(query.tokens.len() - 1)
        };

        Self {
            query: None,
            whole_query_snapshot: Some(WholeQueryRunSnapshot {
                index: query.index,
                tokens: query.tokens.clone(),
                line_by_pos: query.line_by_pos.clone(),
                high_matchables: query.high_matchables.clone(),
                low_matchables: query.low_matchables.clone(),
            }),
            start: 0,
            end,
            cached_high_matchables: OnceCell::new(),
            cached_low_matchables: OnceCell::new(),
            combined_matchables: RefCell::new(None),
        }
    }

    fn source_tokens(&self) -> &[TokenId] {
        if let Some(query) = self.query {
            &query.tokens
        } else {
            &self
                .whole_query_snapshot
                .as_ref()
                .expect("snapshot-backed whole query run should have snapshot data")
                .tokens
        }
    }

    fn source_line_by_pos(&self) -> &[usize] {
        if let Some(query) = self.query {
            &query.line_by_pos
        } else {
            &self
                .whole_query_snapshot
                .as_ref()
                .expect("snapshot-backed whole query run should have snapshot data")
                .line_by_pos
        }
    }

    fn source_high_matchables(&self) -> &PositionSet {
        if let Some(query) = self.query {
            &query.high_matchables
        } else {
            &self
                .whole_query_snapshot
                .as_ref()
                .expect("snapshot-backed whole query run should have snapshot data")
                .high_matchables
        }
    }

    fn source_low_matchables(&self) -> &PositionSet {
        if let Some(query) = self.query {
            &query.low_matchables
        } else {
            &self
                .whole_query_snapshot
                .as_ref()
                .expect("snapshot-backed whole query run should have snapshot data")
                .low_matchables
        }
    }

    /// Get the license index used by this query run.
    pub fn get_index(&self) -> &LicenseIndex {
        if let Some(query) = self.query {
            query.index
        } else {
            self.whole_query_snapshot
                .as_ref()
                .expect("snapshot-backed whole query run should have snapshot data")
                .index
        }
    }

    /// Get the line number for a specific token position.
    ///
    /// # Arguments
    /// * `pos` - Absolute token position in the query
    ///
    /// # Returns
    /// The line number (1-based), or None if position is out of range
    pub fn line_for_pos(&self, pos: usize) -> Option<usize> {
        self.source_line_by_pos().get(pos).copied()
    }

    /// Get the sequence of token IDs for this run.
    ///
    /// Returns empty slice if end is None.
    ///
    /// Corresponds to Python: `tokens` property (lines 779-786)
    pub fn tokens(&self) -> &[TokenId] {
        match self.end {
            Some(end) => &self.source_tokens()[self.start..=end],
            None => &[],
        }
    }

    /// Iterate over token IDs with their absolute positions.
    ///
    /// Corresponds to Python: `tokens_with_pos()` method (lines 788-789)
    pub fn tokens_with_pos(&self) -> impl Iterator<Item = (usize, TokenId)> + '_ {
        self.tokens()
            .iter()
            .copied()
            .enumerate()
            .map(|(i, tid)| (self.start + i, tid))
    }

    /// Check if this query run contains only digit tokens.
    ///
    /// Corresponds to Python: `is_digits_only()` method (lines 791-796)
    pub fn is_digits_only(&self) -> bool {
        self.tokens()
            .iter()
            .all(|&tid| self.get_index().dictionary.is_digit_only_token(tid))
    }

    /// Check if this query run has matchable tokens.
    ///
    /// # Arguments
    /// * `include_low` - If true, include low-value tokens in the check
    /// * `exclude_positions` - Optional set of spans containing positions to exclude
    ///
    /// Returns true if there are matchable tokens remaining
    ///
    /// Corresponds to Python: `is_matchable()` method (lines 798-818)
    pub fn is_matchable(&self, include_low: bool, exclude_positions: &[PositionSpan]) -> bool {
        if self.is_digits_only() {
            return false;
        }

        let matchables = self.matchables(include_low);

        if exclude_positions.is_empty() {
            return !matchables.is_empty();
        }

        let mut matchable_set = matchables;
        for span in exclude_positions {
            matchable_set.remove_span(span);
        }

        !matchable_set.is_empty()
    }

    pub fn matchables(&self, include_low: bool) -> PositionSet {
        if include_low {
            if let Some(ref cached) = *self.combined_matchables.borrow() {
                return cached.clone();
            }
            let combined = self.low_matchables().union(&self.high_matchables());
            *self.combined_matchables.borrow_mut() = Some(combined.clone());
            combined
        } else {
            self.high_matchables()
        }
    }

    pub fn matchable_tokens(&self) -> Vec<Option<TokenId>> {
        let high_matchables = self.high_matchables();
        if high_matchables.is_empty() {
            return Vec::new();
        }

        let matchables = self.matchables(true);
        self.tokens_with_pos()
            .map(|(pos, tid)| {
                if matchables.contains(pos) {
                    Some(tid)
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn high_matchables(&self) -> PositionSet {
        self.cached_high_matchables
            .get_or_init(|| {
                let start = self.start;
                let end = self.end.map(|e| e + 1).unwrap_or(usize::MAX);
                let source = self.source_high_matchables();
                let live_span = PositionSpan::new(start, end);
                source
                    .iter()
                    .filter(|&pos| live_span.contains(pos))
                    .collect()
            })
            .clone()
    }

    pub fn low_matchables(&self) -> PositionSet {
        self.cached_low_matchables
            .get_or_init(|| {
                let start = self.start;
                let end = self.end.map(|e| e + 1).unwrap_or(usize::MAX);
                let source = self.source_low_matchables();
                let live_span = PositionSpan::new(start, end);
                source
                    .iter()
                    .filter(|&pos| live_span.contains(pos))
                    .collect()
            })
            .clone()
    }
}

#[cfg(test)]
mod test;
