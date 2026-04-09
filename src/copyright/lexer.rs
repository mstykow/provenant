//! Lexer (tokenizer + POS tagger) for copyright detection.
//!
//! Splits prepared text lines into tokens, then assigns each token a
//! part-of-speech (POS) tag using the compiled regex patterns. This is
//! the bridge between candidate line selection and grammar parsing.
//!
//! Pipeline: numbered lines → tokenize → POS tag → tagged tokens

use std::sync::LazyLock;

use regex::Regex;

use super::patterns::COMPILED_PATTERNS;
use super::types::{PosTag, Token};
use crate::models::LineNumber;

/// Splitter regex: splits on tabs, spaces, equals signs, and semicolons.
/// Matches Python's `re.compile(r'[\t =;]+').split`.
static SPLITTER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\t =;]+").unwrap());

/// Tokenize and POS-tag a group of numbered lines.
///
/// Takes an iterable of `(line_number, prepared_text)` tuples (output of
/// `collect_candidate_lines`) and returns a flat list of POS-tagged tokens.
///
/// Empty lines are handled specially: if the previous line starts with
/// "copyright" or ends with continuation markers ("by", "copyright", or
/// a digit), the empty line is skipped (continuation). Otherwise an
/// `EMPTY_LINE` token is emitted.
pub fn get_tokens(numbered_lines: &[(usize, String)]) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut last_line = String::new();

    for (start_line, line) in numbered_lines {
        if line.trim().is_empty() {
            let stripped = last_line
                .to_lowercase()
                .trim_matches(|c: char| c.is_ascii_punctuation())
                .to_string();

            if stripped.starts_with("copyright")
                || stripped.ends_with("by")
                || stripped.ends_with("copyright")
                || stripped.chars().last().is_some_and(|c| c.is_ascii_digit())
            {
                continue;
            } else {
                tokens.push(Token {
                    value: "\n".to_string(),
                    tag: PosTag::EmptyLine,
                    start_line: LineNumber::new(*start_line).expect("invalid line number"),
                });
                last_line.clear();
                continue;
            }
        }

        last_line.clone_from(line);

        for tok_str in SPLITTER.split(line) {
            let mut tok = tok_str.to_string();

            if tok.ends_with("',") {
                tok = tok.trim_end_matches(&[',', '\''][..]).to_string();
            }

            tok = tok.trim_matches(&['\'', ' '][..]).to_string();
            tok = tok.trim_end_matches(':').to_string();
            tok = tok.trim().to_string();

            if tok.is_empty() || tok == ":" || tok == "." {
                continue;
            }

            if tok.ends_with(',') {
                let base = tok.trim_end_matches(',').trim();
                if !base.is_empty() {
                    let tag = COMPILED_PATTERNS.match_token(base);
                    tokens.push(Token {
                        value: base.to_string(),
                        tag,
                        start_line: LineNumber::new(*start_line).expect("invalid line number"),
                    });
                    tokens.push(Token {
                        value: ",".to_string(),
                        tag: PosTag::Cc,
                        start_line: LineNumber::new(*start_line).expect("invalid line number"),
                    });
                    continue;
                }
            }

            let tag = COMPILED_PATTERNS.match_token(&tok);

            tokens.push(Token {
                value: tok,
                tag,
                start_line: LineNumber::new(*start_line).expect("invalid line number"),
            });
        }
    }

    retag_camel_case_junk_before_company_suffix_in_copyright_context(&mut tokens);

    tokens
}

fn retag_camel_case_junk_before_company_suffix_in_copyright_context(tokens: &mut [Token]) {
    if tokens.len() < 2 {
        return;
    }

    for i in 0..tokens.len().saturating_sub(1) {
        if tokens[i].tag != PosTag::Junk {
            continue;
        }
        if tokens[i + 1].tag != PosTag::Comp {
            continue;
        }
        if tokens[i].start_line != tokens[i + 1].start_line {
            continue;
        }
        if !is_camel_case_identifier_candidate(&tokens[i].value) {
            continue;
        }

        let mut has_copy_prefix = false;
        let mut j = i;
        while j > 0 {
            j -= 1;
            if tokens[j].start_line != tokens[i].start_line || tokens[j].tag == PosTag::EmptyLine {
                break;
            }
            if tokens[j].tag == PosTag::Copy {
                has_copy_prefix = true;
                break;
            }
        }

        if has_copy_prefix {
            tokens[i].tag = PosTag::Nnp;
        }
    }
}

fn is_camel_case_identifier_candidate(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_uppercase() {
        return false;
    }

    let mut has_lower = false;
    let mut has_inner_upper = false;
    for c in chars {
        if !c.is_ascii_alphanumeric() {
            return false;
        }
        if c.is_ascii_lowercase() {
            has_lower = true;
        } else if c.is_ascii_uppercase() {
            has_inner_upper = true;
        }
    }

    has_lower && has_inner_upper
}

#[cfg(test)]
#[path = "lexer_test.rs"]
mod tests;
