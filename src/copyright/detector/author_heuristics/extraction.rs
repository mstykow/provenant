// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use super::super::token_utils::normalize_whitespace;
use super::{
    extract_author_name_from_json_window, json_window_contains_code_like_author_usage,
    refine_author_with_optional_handle_suffix, refine_json_author_candidate,
};
use crate::copyright::line_tracking::PreparedLines;
use crate::copyright::prepare::prepare_text_line;
use crate::copyright::refiner::refine_author;
use crate::copyright::types::{AuthorDetection, CopyrightDetection, HolderDetection};
use crate::models::LineNumber;

fn line_number_for_offset(content: &str, offset: usize) -> LineNumber {
    LineNumber::from_0_indexed(content[..offset].bytes().filter(|b| *b == b'\n').count())
}

fn decode_markup_entities(value: &str) -> String {
    static DECIMAL_ENTITY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"&#(?P<code>\d+);?").unwrap());
    static HEX_ENTITY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"&#x(?P<code>[0-9a-fA-F]+);?").unwrap());

    let mut out = value
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&#38;", "&")
        .replace("&#34;", "\"")
        .replace("&#39;", "'")
        .replace("&#60;", "<")
        .replace("&#62;", ">");

    out = HEX_ENTITY_RE
        .replace_all(&out, |caps: &regex::Captures| {
            caps.name("code")
                .and_then(|m| u32::from_str_radix(m.as_str(), 16).ok())
                .and_then(char::from_u32)
                .map(|ch| ch.to_string())
                .unwrap_or_else(|| caps.get(0).map(|m| m.as_str()).unwrap_or("").to_string())
        })
        .into_owned();

    out = DECIMAL_ENTITY_RE
        .replace_all(&out, |caps: &regex::Captures| {
            caps.name("code")
                .and_then(|m| m.as_str().parse::<u32>().ok())
                .and_then(char::from_u32)
                .map(|ch| ch.to_string())
                .unwrap_or_else(|| caps.get(0).map(|m| m.as_str()).unwrap_or("").to_string())
        })
        .into_owned();

    out
}

fn repair_latin1_mojibake(value: &str) -> String {
    let likely_mojibake = value.contains('Ã')
        || value.contains('Â')
        || value.contains('Ð')
        || value.contains('Ñ')
        || value.contains('â');
    if !likely_mojibake {
        return value.to_string();
    }

    let mut bytes = Vec::with_capacity(value.len());
    for ch in value.chars() {
        let code = ch as u32;
        if code > 0xFF {
            return value.to_string();
        }
        bytes.push(code as u8);
    }

    String::from_utf8(bytes).unwrap_or_else(|_| value.to_string())
}

fn normalize_markup_author_value(value: &str) -> String {
    let decoded = decode_markup_entities(value);
    let repaired = repair_latin1_mojibake(&decoded);
    let prepared = prepare_text_line(&repaired);
    normalize_whitespace(&prepared)
}

fn split_markup_author_candidates(value: &str) -> Vec<String> {
    let normalized = normalize_markup_author_value(value);
    let parts: Vec<String> = normalized
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect();

    if parts.len() >= 2
        && parts.iter().all(|part| {
            part.contains(' ')
                || part.split_whitespace().count() >= 2
                || part.chars().filter(|ch| *ch == '.').count() >= 1
        })
    {
        parts
    } else {
        vec![normalized]
    }
}

pub(in super::super) fn extract_markup_authors(content: &str, authors: &mut Vec<AuthorDetection>) {
    if content.is_empty() {
        return;
    }

    static AUTHOR_ATTR_DQ_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?is)<[^>]*\bauthor\s*=\s*\"([^\"]+)\"[^>]*>"#).unwrap());
    static AUTHOR_ATTR_SQ_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?is)<[^>]*\bauthor\s*=\s*'([^']+)'[^>]*>"#).unwrap());
    static DOCBOOK_AUTHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?is)<div[^>]*class\s*=\s*(?:\"[^\"]*\bauthor\b[^\"]*\"|'[^']*\bauthor\b[^']*')[^>]*>.*?<span[^>]*class\s*=\s*(?:\"[^\"]*firstname[^\"]*\"|'[^']*firstname[^']*')[^>]*>\s*(?P<first>[^<]+?)\s*</span>\s*<span[^>]*class\s*=\s*(?:\"[^\"]*surname[^\"]*\"|'[^']*surname[^']*')[^>]*>\s*(?P<last>[^<]+?)\s*</span>.*?</div>"#,
        )
        .unwrap()
    });

    let mut seen: HashSet<(String, LineNumber)> = authors
        .iter()
        .map(|a| (a.author.clone(), a.start_line))
        .collect();

    for captures in [
        AUTHOR_ATTR_DQ_RE.captures_iter(content).collect::<Vec<_>>(),
        AUTHOR_ATTR_SQ_RE.captures_iter(content).collect::<Vec<_>>(),
    ] {
        for cap in captures {
            let Some(full) = cap.get(0) else {
                continue;
            };
            let value = cap.get(1).map(|m| m.as_str()).unwrap_or("").trim();
            let line = line_number_for_offset(content, full.start());
            for candidate in split_markup_author_candidates(value) {
                let Some(author) = refine_author(&candidate) else {
                    continue;
                };
                if seen.insert((author.clone(), line)) {
                    authors.push(AuthorDetection {
                        author,
                        start_line: line,
                        end_line: line,
                    });
                }
            }
        }
    }

    for cap in DOCBOOK_AUTHOR_RE.captures_iter(content) {
        let Some(full) = cap.get(0) else {
            continue;
        };
        let first = cap.name("first").map(|m| m.as_str()).unwrap_or("").trim();
        let last = cap.name("last").map(|m| m.as_str()).unwrap_or("").trim();
        if first.is_empty() || last.is_empty() {
            continue;
        }
        let Some(author) = refine_author(&format!("{first} {last}")) else {
            continue;
        };
        let line = line_number_for_offset(content, full.start());
        if seen.insert((author.clone(), line)) {
            authors.push(AuthorDetection {
                author,
                start_line: line,
                end_line: line,
            });
        }
    }
}

fn strip_leading_dash_bullet(line: &str) -> &str {
    line.trim_start()
        .strip_prefix("- ")
        .map(str::trim_start)
        .unwrap_or_else(|| line.trim())
}

fn trim_attribution_tail(who: &str) -> String {
    static WITH_HELP_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\s+with\s+the\s+help\s+of\b.*$").unwrap());
    static TRAILING_TIMESTAMP_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\s+\d{4}/\d{2}/\d{2}(?:\s+\d{2}:\d{2}:\d{2})?\s*$").unwrap());

    let without_help = WITH_HELP_RE.replace(who, "");
    let without_timestamp = TRAILING_TIMESTAMP_RE.replace(without_help.as_ref(), "");
    let trimmed = without_timestamp.trim().trim_end_matches('.').trim();
    if trimmed.is_empty() {
        who.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

fn trim_following_sentence_clause(who: &str) -> String {
    static FOLLOWING_SENTENCE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?is)^(?P<head>.+?)\.\s+(?:it|this|these|those|the|a|an)\b.*$").unwrap()
    });

    let trimmed = who.trim();
    if let Some(cap) = FOLLOWING_SENTENCE_RE.captures(trimmed) {
        let head = cap.name("head").map(|m| m.as_str()).unwrap_or("").trim();
        if !head.is_empty() {
            return head.to_string();
        }
    }

    trimmed.to_string()
}

fn trim_notice_support_sentence(who: &str) -> String {
    static NOTICE_SUPPORT_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)^(?P<head>.+?)\.\s+Visit\b.*$").unwrap());

    let trimmed = who.trim();
    if let Some(cap) = NOTICE_SUPPORT_RE.captures(trimmed) {
        let head = cap.name("head").map(|m| m.as_str()).unwrap_or("").trim();
        if !head.is_empty() {
            return head.to_string();
        }
    }

    trimmed.to_string()
}

fn refine_notice_collective_author(who: &str) -> Option<String> {
    let trimmed = trim_notice_support_sentence(&trim_following_sentence_clause(who))
        .trim_end_matches(&['.', ';', ','][..])
        .trim_matches(&['"', '\''][..])
        .trim()
        .to_string();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(author) = refine_author(&trimmed) {
        return Some(author);
    }

    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("the ") {
        return None;
    }

    let collective_suffix = [" project", " foundation", " group", " team", " committee"]
        .iter()
        .any(|suffix| lower.contains(suffix));
    if !collective_suffix {
        return None;
    }

    let mut capitalized = trimmed.clone();
    capitalized.replace_range(..1, "T");
    if refine_author(&capitalized).is_some() {
        return Some(trimmed);
    }

    let capitalized_words = trimmed
        .split_whitespace()
        .filter_map(|word| word.chars().find(|ch| ch.is_alphabetic()))
        .filter(|ch| ch.is_uppercase())
        .count();
    let has_url = trimmed.contains("http://") || trimmed.contains("https://");
    if has_url && capitalized_words >= 2 {
        Some(trimmed)
    } else {
        None
    }
}

fn extract_written_by_subject(line: &str) -> Option<String> {
    static WRITTEN_BY_PREFIX_LINE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?:original(?:ly)?\s+)?(?:original\s+driver\s+)?(?:written|authored|created|developed)\s+by\s+(?P<who>.+)$",
        )
        .unwrap()
    });
    static WRITTEN_BY_ANYWHERE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)\b(?:original(?:ly)?\s+)?(?:original\s+driver\s+)?(?:written|authored|created|developed)\s+by\s+(?P<who>.+)$",
        )
        .unwrap()
    });

    WRITTEN_BY_PREFIX_LINE_RE
        .captures(line)
        .or_else(|| WRITTEN_BY_ANYWHERE_RE.captures(line))
        .and_then(|cap| cap.name("who").map(|m| m.as_str().trim().to_string()))
}

fn extract_dash_bullet_attribution_author(line: &str) -> Option<String> {
    static DASH_BULLET_BY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?:(?:written|updated|authored|created|developed|modified)\s+by|added\s+to\s+by|(?:ported|adapted)(?:\s+to\s+[^\r\n]*?)?\s+by|valuable\s+contributions\s+by)\s+(?P<who>.+)$",
        )
        .unwrap()
    });

    let normalized = strip_leading_dash_bullet(line);
    let captures = DASH_BULLET_BY_RE.captures(normalized)?;
    let who = captures
        .name("who")
        .map(|m| m.as_str())
        .unwrap_or("")
        .trim();
    if who.is_empty() {
        return None;
    }
    let trimmed = trim_attribution_tail(who);
    refine_author(&trimmed)
}

pub(in super::super) fn extract_dash_bullet_attribution_authors(
    prepared_cache: &PreparedLines<'_>,
) -> Vec<AuthorDetection> {
    if prepared_cache.is_empty() {
        return Vec::new();
    }

    prepared_cache
        .iter()
        .filter_map(|line| {
            let trimmed = line.raw.trim_start();
            if !trimmed.starts_with("- ") {
                return None;
            }
            let author = extract_dash_bullet_attribution_author(trimmed)?;
            Some(AuthorDetection {
                author,
                start_line: line.line_number,
                end_line: line.line_number,
            })
        })
        .collect()
}

pub(in super::super) fn extract_name_contributed_authors(
    prepared_cache: &PreparedLines<'_>,
) -> Vec<AuthorDetection> {
    if prepared_cache.is_empty() {
        return Vec::new();
    }

    prepared_cache
        .iter()
        .filter_map(|line| {
            let trimmed = line.raw.trim();
            let (who, _) = trimmed.split_once(" contributed")?;
            let words: Vec<&str> = who.split_whitespace().collect();
            if !(2..=4).contains(&words.len()) {
                return None;
            }
            if !words
                .iter()
                .all(|word| looks_like_contributed_person_name_token(word))
            {
                return None;
            }
            if words
                .iter()
                .any(|word| is_contributed_non_person_token(word))
            {
                return None;
            }
            let author = refine_author(who)?;
            Some(AuthorDetection {
                author,
                start_line: line.line_number,
                end_line: line.line_number,
            })
        })
        .collect()
}

fn looks_like_contributed_person_name_token(word: &str) -> bool {
    let trimmed_word = word.trim_matches(|ch: char| {
        !ch.is_alphabetic() && ch != '\'' && ch != '’' && ch != '.' && ch != '-'
    });
    trimmed_word
        .chars()
        .next()
        .is_some_and(|ch| ch.is_uppercase())
        && trimmed_word.chars().any(|ch| ch.is_alphabetic())
}

fn is_contributed_non_person_token(word: &str) -> bool {
    matches!(
        word.trim_matches(|ch: char| !ch.is_alphabetic())
            .to_ascii_lowercase()
            .as_str(),
        "company"
            | "co"
            | "corp"
            | "corporation"
            | "foundation"
            | "group"
            | "inc"
            | "limited"
            | "ltd"
            | "llc"
            | "llp"
            | "organization"
            | "partnership"
            | "portions"
            | "team"
    )
}

pub(in super::super) fn extract_rst_field_authors(
    prepared_cache: &PreparedLines<'_>,
) -> Vec<AuthorDetection> {
    if prepared_cache.is_empty() {
        return Vec::new();
    }

    static RST_FIELD_AUTHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^:?(?:author(?:\s*&\s*maintainer)?|updated\s+by)\s*:\s*(?P<tail>.+)$")
            .unwrap()
    });
    static ATTRIBUTION_PREFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?:written|updated|authored|created|developed|maintained)\s+by\s+")
            .unwrap()
    });

    prepared_cache
        .iter_non_empty()
        .filter_map(|line| {
            let cap = RST_FIELD_AUTHOR_RE.captures(line.prepared)?;
            let tail = cap.name("tail").map(|m| m.as_str()).unwrap_or("").trim();
            if tail.is_empty() {
                return None;
            }
            let stripped_tail = ATTRIBUTION_PREFIX_RE.replace(tail, "");
            let trimmed = trim_attribution_tail(stripped_tail.as_ref());
            let author = refine_author(&trimmed)?;
            Some(AuthorDetection {
                author,
                start_line: line.line_number,
                end_line: line.line_number,
            })
        })
        .collect()
}

fn extract_author_colon_bullet_roster(
    segments: &[String],
    start_line: usize,
) -> Vec<AuthorDetection> {
    static BARE_EMAIL_AUTHOR_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(?P<who>.+?<[^>\s]*@[^>\s]*>)\s*,?$").unwrap());

    let mut authors = Vec::new();

    if segments.is_empty()
        || !segments
            .iter()
            .any(|segment| segment.trim_start().starts_with('-'))
    {
        return authors;
    }

    for (offset, segment) in segments.iter().enumerate() {
        let trimmed = segment.trim();
        let line_no = start_line + offset;

        if !trimmed.trim_start().starts_with('-') {
            let lower = trimmed.to_ascii_lowercase();
            if lower.starts_with("with the help of ") {
                continue;
            }
            return authors;
        }

        if let Some(author) = extract_dash_bullet_attribution_author(trimmed) {
            authors.push(AuthorDetection {
                author,
                start_line: LineNumber::new(line_no).expect("valid"),
                end_line: LineNumber::new(line_no).expect("valid"),
            });
            continue;
        }

        let normalized = strip_leading_dash_bullet(trimmed);
        let Some(cap) = BARE_EMAIL_AUTHOR_RE.captures(normalized) else {
            continue;
        };
        if offset > 0 && segments[offset - 1].trim_end().ends_with(',') {
            continue;
        }
        let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
        let Some(author) = refine_author(who) else {
            continue;
        };
        authors.push(AuthorDetection {
            author,
            start_line: LineNumber::new(line_no).expect("valid"),
            end_line: LineNumber::new(line_no).expect("valid"),
        });
    }

    authors
}

pub(in super::super) fn extract_multiline_written_by_author_blocks(
    prepared_cache: &PreparedLines<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static WRITTEN_BY_SINGLE_LINE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^\s*written\s+by\s+(?P<who>.+?)(?:\s+for\b|$)").unwrap());
    static AUTHOR_EMAIL_HEAD_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?P<head>.+?<[^>]+>)(?:\s+(?:for|to)\b.*)?$").unwrap());
    static MAINTAINED_BY_PREFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?:(?:it|this\s+package)\s+is\s+)?maintained(?:\s+for\s+debian)?\s+by\s+(?P<who>.+)$",
        )
        .unwrap()
    });

    for prepared_line in prepared_cache.iter() {
        let raw_line = prepared_line.raw;
        let ln = prepared_line.line_number;
        if raw_line.is_empty() {
            continue;
        }
        let line = strip_leading_dash_bullet(raw_line.trim());
        if line.is_empty() {
            continue;
        }
        if !line.to_ascii_lowercase().starts_with("written by ") {
            continue;
        }

        if let Some(cap) = WRITTEN_BY_SINGLE_LINE_RE.captures(line) {
            let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
            if who.is_empty() {
                continue;
            }
            let who_words: Vec<&str> = who.split_whitespace().collect();
            if who_words.len() < 2 {
                continue;
            }

            let has_email = who.contains('@') || who.contains('<');
            if !has_email {
                continue;
            }

            let who = if let Some(cap) = AUTHOR_EMAIL_HEAD_RE.captures(who) {
                cap.name("head").map(|m| m.as_str()).unwrap_or(who).trim()
            } else {
                who
            };

            if let Some(author) = refine_author(who) {
                authors.push(AuthorDetection {
                    author,
                    start_line: ln,
                    end_line: ln,
                });
            }
        }
    }

    let mut line_number = LineNumber::ONE;
    while let Some(prepared_line) = prepared_cache.line(line_number) {
        let line = prepared_line.prepared;
        let normalized_line = strip_leading_dash_bullet(line);
        let lower = normalized_line.to_ascii_lowercase();

        let is_start = !normalized_line.is_empty()
            && !lower.starts_with("copyright")
            && !lower.contains("copyright")
            && (lower.starts_with("written by ")
                || lower.starts_with("originally written by ")
                || lower.starts_with("original driver written by ")
                || lower.contains(" written by "));

        if !is_start {
            line_number = line_number.next();
            continue;
        }

        let mut block_lines: Vec<(LineNumber, String)> = Vec::new();
        block_lines.push((prepared_line.line_number, line.to_string()));

        let mut next_line_number = prepared_line.line_number.next();
        while let Some(next_line) = prepared_cache.line(next_line_number) {
            let next_line = next_line.prepared;
            if next_line.is_empty() {
                break;
            }
            let next_lower = next_line.to_ascii_lowercase();
            if next_lower.starts_with("copyright") {
                break;
            }
            if !(next_lower.contains(" by ")
                || next_lower.starts_with("for ")
                || next_lower.starts_with("overhauled by ")
                || next_lower.starts_with("ported ")
                || next_lower.starts_with("updated ")
                || next_lower.starts_with("kernel ")
                || next_lower.starts_with("extensive ")
                || next_lower.starts_with("revised ")
                || next_lower.starts_with("implemented ")
                || next_lower.starts_with("copied from "))
            {
                break;
            }

            block_lines.push((next_line_number, next_line.to_string()));
            next_line_number = next_line_number.next();
        }

        if block_lines.len() < 2 {
            line_number = line_number.next();
            continue;
        }

        let start_line = block_lines
            .first()
            .map(|(line_number, _)| *line_number)
            .unwrap_or(prepared_line.line_number);
        let end_line = block_lines
            .last()
            .map(|(line_number, _)| *line_number)
            .unwrap_or(prepared_line.line_number);

        let prefer_combined_block = block_lines.iter().skip(1).any(|(_, raw_line)| {
            let lower = raw_line.trim().to_ascii_lowercase();
            lower.starts_with("overhauled by ")
                || lower.starts_with("ported ")
                || lower.starts_with("updated ")
                || lower.starts_with("kernel ")
                || lower.starts_with("extensive ")
                || lower.starts_with("revised ")
                || lower.starts_with("implemented ")
                || lower.starts_with("copied from ")
        });

        if prefer_combined_block {
            let combined_raw = block_lines
                .iter()
                .map(|(_, raw_line)| raw_line.trim())
                .collect::<Vec<_>>()
                .join(" ");
            let combined_candidate = extract_written_by_subject(&combined_raw)
                .or_else(|| {
                    MAINTAINED_BY_PREFIX_RE
                        .captures(&combined_raw)
                        .and_then(|cap| cap.name("who").map(|m| m.as_str().trim().to_string()))
                })
                .unwrap_or(combined_raw);
            let combined_candidate = trim_following_sentence_clause(&combined_candidate);
            let combined_candidate = combined_candidate.trim_end_matches('.').trim();
            if let Some(combined) = refine_author(combined_candidate) {
                authors.retain(|a| a.start_line < start_line || a.end_line > end_line);
                authors.push(AuthorDetection {
                    author: combined,
                    start_line,
                    end_line,
                });
                line_number = next_line_number;
                continue;
            }
        }

        let mut extracted_any = false;
        for (_l, raw_line) in &block_lines {
            let candidate = raw_line.trim();
            if let Some(who) = extract_written_by_subject(candidate).or_else(|| {
                MAINTAINED_BY_PREFIX_RE
                    .captures(candidate)
                    .and_then(|cap| cap.name("who").map(|m| m.as_str().trim().to_string()))
            }) {
                let who = trim_following_sentence_clause(&who);
                let who = who.trim_end_matches('.').trim();
                if !who.to_ascii_lowercase().starts_with("the ") {
                    if let Some(author) = refine_author(who) {
                        authors.push(AuthorDetection {
                            author,
                            start_line,
                            end_line,
                        });
                    }
                    extracted_any = true;
                }
                continue;
            }
        }

        if !extracted_any {
            let combined_raw = block_lines
                .iter()
                .map(|(_, raw_line)| raw_line.trim())
                .collect::<Vec<_>>()
                .join(" ");
            let combined_candidate = extract_written_by_subject(&combined_raw)
                .or_else(|| {
                    MAINTAINED_BY_PREFIX_RE
                        .captures(&combined_raw)
                        .and_then(|cap| cap.name("who").map(|m| m.as_str().trim().to_string()))
                })
                .unwrap_or(combined_raw);
            let combined_candidate = trim_following_sentence_clause(&combined_candidate);
            let combined_candidate = combined_candidate.trim_end_matches('.').trim();
            if let Some(combined) = refine_author(combined_candidate) {
                authors.retain(|a| a.start_line < start_line || a.end_line > end_line);
                authors.push(AuthorDetection {
                    author: combined,
                    start_line,
                    end_line,
                });
            }
        }

        line_number = next_line_number;
    }
}

pub(in super::super) fn extract_json_excerpt_developed_by_authors(
    content: &str,
) -> Vec<AuthorDetection> {
    if content.is_empty() {
        return Vec::new();
    }

    static JSON_DEVELOPED_BY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?is)"(?:excerpt|description)"\s*:\s*"[^"\n]{0,800}?\bdeveloped\s+by\s+(?P<who>[A-Z][A-Za-z0-9.&+'-]*(?:\s+[A-Z][A-Za-z0-9.&+'-]*){0,4})(?:[.,;]|\")"#,
        )
        .unwrap()
    });

    JSON_DEVELOPED_BY_RE
        .captures_iter(content)
        .filter_map(|cap| {
            let who = cap
                .name("who")
                .map(|m| m.as_str())
                .unwrap_or("")
                .trim()
                .trim_end_matches(&['.', ';', ','][..]);
            if who.is_empty() {
                return None;
            }
            let author = refine_author(who)?;
            Some(AuthorDetection {
                author,
                start_line: LineNumber::ONE,
                end_line: LineNumber::ONE,
            })
        })
        .collect()
}

pub(in super::super) fn extract_modified_portion_developed_by_authors(
    content: &str,
) -> Vec<AuthorDetection> {
    let mut authors = Vec::new();
    if content.is_empty() {
        return authors;
    }

    static MODIFIED_PORTION_DEVELOPED_BY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?ims)^[^\n]*modified\s+portion[^\n]*developed\s+by\s+(?P<who>[A-Z][A-Za-z0-9.&+'-]*(?:\s+[A-Z][A-Za-z0-9.&+'-]*){0,4})\.\s*(?:\r?\n\s*(?:#|//|/\*+|\*|--)?\s*\((?P<url>https?://[^)\s]+)\)\.?)?"#,
        )
        .unwrap()
    });

    for cap in MODIFIED_PORTION_DEVELOPED_BY_RE.captures_iter(content) {
        let Some(full) = cap.get(0) else {
            continue;
        };
        let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
        if who.is_empty() {
            continue;
        }

        let mut author = who.to_string();
        if let Some(url) = cap.name("url").map(|m| m.as_str().trim())
            && !url.is_empty()
        {
            author.push_str(". (");
            author.push_str(url);
            author.push(')');
        }

        let start_line = line_number_for_offset(content, full.start());
        let end_line = line_number_for_offset(content, full.end());
        authors.push(AuthorDetection {
            author,
            start_line,
            end_line,
        });
    }

    authors
}

pub(in super::super) fn extract_module_author_macros(
    content: &str,
    copyrights: &[CopyrightDetection],
    holders: &[HolderDetection],
) -> (
    Vec<CopyrightDetection>,
    Vec<HolderDetection>,
    Vec<AuthorDetection>,
) {
    let authors = Vec::new();
    if content.is_empty() {
        return (Vec::new(), Vec::new(), authors);
    }
    if !copyrights.is_empty() || !holders.is_empty() {
        return (Vec::new(), Vec::new(), authors);
    }

    static MODULE_AUTHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?i)MODULE_AUTHOR\s*\(\s*\"(?P<who>[^\"]+)\"\s*\)"#).unwrap()
    });

    let mut authors = Vec::new();
    for (idx, raw) in content.lines().enumerate() {
        let ln = idx + 1;
        let line = raw.trim();
        if line.is_empty() || !line.contains("MODULE_AUTHOR") {
            continue;
        }

        for cap in MODULE_AUTHOR_RE.captures_iter(line) {
            let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
            if who.is_empty() {
                continue;
            }
            let who = who.replace(r#"\""#, "\"");
            let Some(author) = refine_author(&who) else {
                continue;
            };
            authors.push(AuthorDetection {
                author,
                start_line: LineNumber::new(ln).expect("invalid line number"),
                end_line: LineNumber::new(ln).expect("invalid line number"),
            });
        }
    }

    (Vec::new(), Vec::new(), authors)
}

pub(in super::super) fn extract_was_developed_by_author_blocks(
    prepared_cache: &PreparedLines<'_>,
) -> Vec<AuthorDetection> {
    let mut authors = Vec::new();
    if prepared_cache.is_empty() {
        return authors;
    }

    static WAS_DEVELOPED_BY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\bwas\s+developed\s+by\s+(?P<who>.+)$").unwrap());
    static WITH_PARTICIPATION_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\bwith\s+participation\b").unwrap());

    let mut line_number = LineNumber::ONE;
    while let Some(line) = prepared_cache.line(line_number) {
        if line.prepared.is_empty() {
            line_number = line_number.next();
            continue;
        }

        let Some(cap) = WAS_DEVELOPED_BY_RE.captures(line.prepared) else {
            line_number = line_number.next();
            continue;
        };
        let mut parts: Vec<String> = Vec::new();
        let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
        if who.is_empty() {
            line_number = line_number.next();
            continue;
        }
        parts.push(who.to_string());

        let mut end_line = line.line_number;
        let mut next_line_number = line.line_number.next();
        while let Some(next_line) = prepared_cache.line(next_line_number) {
            if next_line.prepared.is_empty() {
                break;
            }

            let next_lower = next_line.prepared.to_ascii_lowercase();
            if next_lower.starts_with("copyright") {
                break;
            }

            if let Some(m) = WITH_PARTICIPATION_RE.find(next_line.prepared) {
                let prefix = next_line.prepared[..m.start()].trim_end();
                if !prefix.is_empty() {
                    parts.push(prefix.to_string());
                    end_line = next_line.line_number;
                }
                break;
            }

            parts.push(next_line.prepared.to_string());
            end_line = next_line.line_number;

            if end_line.get().saturating_sub(line.line_number.get()) >= 3 {
                break;
            }

            next_line_number = next_line_number.next();
        }

        let joined = parts.join(" ");
        let joined = joined.split_whitespace().collect::<Vec<_>>().join(" ");
        if joined.is_empty() {
            line_number = line_number.next();
            continue;
        }

        let author = refine_author(&joined).unwrap_or(joined);
        if author.is_empty() {
            line_number = line_number.next();
            continue;
        }

        authors.push(AuthorDetection {
            author,
            start_line: line.line_number,
            end_line,
        });

        line_number = line_number.next();
    }

    authors
}

pub(in super::super) fn extract_author_colon_blocks(
    prepared_cache: &PreparedLines<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static AUTHOR_COLON_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?:(?:primary|original)(?:\s+[^:]{0,40})?\s+)?author(?:s|\(s\)|s\(s\))?\s*:\s*(?P<tail>.*)$",
        )
        .unwrap()
    });
    static YEAR_ONLY_COPY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^copyright\s+\(c\)\s*(?:\d{4}(?:\s*,\s*\d{4})*|\d{4}-\d{4})\s*$").unwrap()
    });

    let mut line_number = LineNumber::ONE;
    while let Some(prepared_line) = prepared_cache.line(line_number) {
        let line = trim_author_label_prefix(prepared_line.prepared);
        if line.is_empty() {
            line_number = line_number.next();
            continue;
        }

        let Some(cap) = AUTHOR_COLON_RE.captures(&line) else {
            line_number = line_number.next();
            continue;
        };

        let mut skip = false;
        let mut prev_line_number = prepared_line.line_number;
        while prev_line_number > LineNumber::ONE {
            prev_line_number = prev_line_number.prev().expect("valid");
            let Some(prev) = prepared_cache.line(prev_line_number) else {
                break;
            };
            if prev.prepared.is_empty() {
                continue;
            }
            if YEAR_ONLY_COPY_RE.is_match(prev.prepared) {
                skip = true;
            }
            break;
        }
        if skip {
            line_number = line_number.next();
            continue;
        }

        let tail = cap.name("tail").map(|m| m.as_str()).unwrap_or("").trim();
        let label_lower = line
            .split(':')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        let original_or_primary_label =
            label_lower.contains("original") || label_lower.contains("primary");
        let single_line_original_or_primary = !tail.is_empty() && original_or_primary_label;
        let collect_following_original_authors =
            original_or_primary_label && label_lower.contains("authors");

        let label_raw = line.split(':').next().unwrap_or("").trim();
        let label_is_all_caps = !label_raw.is_empty()
            && label_raw.chars().any(|c| c.is_ascii_uppercase())
            && !label_raw.chars().any(|c| c.is_ascii_lowercase());
        if label_is_all_caps {
            line_number = line_number.next();
            continue;
        }

        let mut segments: Vec<String> = Vec::new();
        if !tail.is_empty() {
            let Some(initial_tail) = sanitize_author_colon_tail(tail) else {
                line_number = line_number.next();
                continue;
            };
            segments.push(initial_tail);
        }
        let mut next_line_number = prepared_line.line_number.next();
        let mut added = 0usize;
        if !single_line_original_or_primary || collect_following_original_authors {
            while let Some(next_prepared) = prepared_cache.line(next_line_number) {
                let next_line_buf = trim_author_label_prefix(next_prepared.prepared);
                let next_line = next_line_buf.as_str();
                if next_line.is_empty() {
                    break;
                }
                let next_lower = next_line.to_ascii_lowercase();
                if is_author_metadata_line(next_line) {
                    break;
                }
                if next_lower.starts_with("copyright") {
                    break;
                }
                if next_lower.starts_with("fixed") || next_lower.starts_with("software") {
                    break;
                }
                if next_lower.starts_with("updated")
                    || next_lower.starts_with("date")
                    || next_lower.starts_with("borrows")
                    || next_lower.starts_with("files")
                {
                    break;
                }
                if next_lower.starts_with("et al") {
                    break;
                }

                if next_line.contains(':') {
                    break;
                }

                let mut include = false;
                if !include {
                    include = next_line.contains('@')
                        || next_line.contains('<')
                        || next_line.contains(',')
                        || next_line
                            .chars()
                            .find(|c| !c.is_whitespace())
                            .is_some_and(|c| c.is_ascii_uppercase());
                }
                if include {
                    segments.push(next_line.to_string());
                    added += 1;
                    next_line_number = next_line_number.next();
                    if added >= 4 {
                        break;
                    }
                    let combined_len: usize = segments.iter().map(|s| s.len()).sum();
                    if combined_len > 320 {
                        break;
                    }
                    continue;
                }
                break;
            }
        }

        if segments.is_empty() {
            line_number = line_number.next();
            continue;
        }

        let start_line = prepared_line.line_number;
        let end_line = if next_line_number == prepared_line.line_number.next() {
            start_line
        } else {
            next_line_number.prev().expect("valid")
        };
        let bullet_results = extract_author_colon_bullet_roster(&segments, start_line.get());
        if !bullet_results.is_empty() {
            authors.extend(bullet_results);
            line_number = next_line_number;
            continue;
        }
        if collect_following_original_authors {
            let mut extracted_any = false;
            for segment in &segments {
                let Some(author) = refine_author_with_optional_handle_suffix(segment) else {
                    continue;
                };
                authors.push(AuthorDetection {
                    author,
                    start_line,
                    end_line,
                });
                extracted_any = true;
            }
            if extracted_any {
                line_number = next_line_number;
                continue;
            }
        }
        if segments.len() == 1 {
            let inline_results = extract_author_colon_inline_roster(&segments[0], start_line.get());
            if !inline_results.is_empty() {
                authors.extend(inline_results);
                line_number = next_line_number;
                continue;
            }
        }
        let combined_raw = segments.join(" ");
        let Some(combined) = refine_author_with_optional_handle_suffix(&combined_raw) else {
            line_number = line_number.next();
            continue;
        };

        authors.retain(|a| a.start_line < start_line || a.end_line > end_line);
        authors.push(AuthorDetection {
            author: combined,
            start_line,
            end_line,
        });

        line_number = next_line_number;
    }
}

fn extract_author_colon_inline_roster(tail: &str, line_number: usize) -> Vec<AuthorDetection> {
    let mut authors = Vec::new();

    for candidate in tail.split(" - ") {
        let Some(author) = refine_author_with_optional_handle_suffix(candidate) else {
            continue;
        };
        authors.push(AuthorDetection {
            author,
            start_line: LineNumber::new(line_number).expect("valid"),
            end_line: LineNumber::new(line_number).expect("valid"),
        });
    }

    authors
}

fn sanitize_author_colon_tail(tail: &str) -> Option<String> {
    let trimmed = tail.trim();
    if trimmed.is_empty() {
        return None;
    }

    static JSON_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?i)(?:^|[\s,{])(?:['"]?name['"]?\s*[:=]\s*|name'\s+)(?P<name>[A-Z][A-Za-z0-9_.-]*(?:\s+[A-Z][A-Za-z0-9_.-]*){0,5})"#,
        )
        .unwrap()
    });
    static METADATA_SPLIT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?i),(?:\s*['"]?(?:url|version|wiki|gav|labels|developerid|email|name|previoustimestamp|previousversion|releasetimestamp|requiredcore|scm|title|builddate|dependencies|sha1)\b.*|\s*maintained\s+by\b.*)$"#,
        )
        .unwrap()
    });

    let lower = trimmed.to_ascii_lowercase();
    let object_like = lower.contains("@type")
        || lower.contains("type'")
        || lower.contains("type ")
        || lower.contains("disambiguatingdescription")
        || lower.contains("sponsor'")
        || lower.contains("logo");

    if object_like {
        if let Some(cap) = JSON_NAME_RE.captures(trimmed) {
            let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
        return None;
    }

    if let Some(mat) = METADATA_SPLIT_RE.find(trimmed) {
        let prefix = trimmed[..mat.start()].trim().trim_end_matches(',').trim();
        if !prefix.is_empty() {
            return Some(prefix.to_string());
        }
        return None;
    }

    Some(trimmed.to_string())
}

fn is_author_metadata_line(line: &str) -> bool {
    let lower = line.trim().to_ascii_lowercase();
    lower.starts_with("url:")
        || lower.starts_with("version:")
        || lower.starts_with("wiki:")
        || lower.starts_with("gav:")
        || lower.starts_with("labels:")
        || lower.starts_with("title:")
        || lower.starts_with("builddate:")
        || lower.starts_with("dependencies:")
        || lower.starts_with("sha1:")
        || lower.starts_with("developerid:")
        || lower.starts_with("email:")
        || lower.starts_with("name:")
        || lower.starts_with("previoustimestamp:")
        || lower.starts_with("previousversion:")
        || lower.starts_with("releasetimestamp:")
        || lower.starts_with("requiredcore:")
        || lower.starts_with("scm:")
        || lower.starts_with("disambiguatingdescription")
}

fn trim_author_label_prefix(line: &str) -> String {
    line.trim()
        .trim_start_matches(['*', '#'])
        .trim_start()
        .to_string()
}

pub(in super::super) fn extract_code_written_by_author_blocks(
    prepared_cache: &PreparedLines<'_>,
) -> Vec<AuthorDetection> {
    let mut authors = Vec::new();
    if prepared_cache.is_empty() {
        return authors;
    }

    static HEADER_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\bcode\s+written\s+by\b").unwrap());
    static BODY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)\bwritten\s+by\s+(?P<body>.+)$").unwrap());
    static STOP_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?is)(?P<prefix>.+?\bDonald\s+wrote\s+the\s+SMC\s+91c92\s+code)\b").unwrap()
    });

    let mut line_number = LineNumber::ONE;
    while let Some(prepared_line) = prepared_cache.line(line_number) {
        let line = prepared_line.prepared;
        if line.is_empty() {
            line_number = line_number.next();
            continue;
        }
        if !HEADER_RE.is_match(line) {
            line_number = line_number.next();
            continue;
        }

        let mut combined = line.to_string();
        let mut next_line_number = prepared_line.line_number.next();
        while let Some(next_prepared) = prepared_cache.line(next_line_number) {
            let next = next_prepared.prepared;
            if next.is_empty() {
                break;
            }
            combined.push(' ');
            combined.push_str(next);
            if next.contains(".  ") || next.ends_with('.') {
                break;
            }
            if combined.len() > 800 {
                break;
            }
            next_line_number = next_line_number.next();
        }

        let Some(cap) = BODY_RE.captures(&combined) else {
            line_number = next_line_number;
            continue;
        };
        let body = cap.name("body").map(|m| m.as_str()).unwrap_or("").trim();
        if body.is_empty() {
            line_number = next_line_number;
            continue;
        }

        let mut candidate = body.to_string();
        if let Some(cap2) = STOP_RE.captures(body) {
            let prefix = cap2.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
            if !prefix.is_empty() {
                candidate = prefix.to_string();
            }
        }

        let Some(author) = refine_author(&candidate) else {
            line_number = next_line_number;
            continue;
        };
        authors.push(AuthorDetection {
            author,
            start_line: prepared_line.line_number,
            end_line: next_line_number.prev().expect("valid"),
        });

        line_number = next_line_number;
    }

    authors
}

pub(in super::super) fn extract_developed_and_created_by_authors(
    prepared_cache: &PreparedLines<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    static PREFIX_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^\s*developed\s+and\s+created\s+by\s+").unwrap());
    static URL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\bhttps?://\S+").unwrap());
    static IFROSS_TAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bon\s+free\s+and\s+open\s+source\s+software\b.*$").unwrap()
    });

    if prepared_cache.is_empty() {
        return;
    }

    for prepared_line in prepared_cache.iter_non_empty() {
        if !PREFIX_RE.is_match(prepared_line.prepared) {
            continue;
        }

        let mut parts: Vec<String> = Vec::new();
        let mut line_number = prepared_line.line_number;
        let mut end_line = prepared_line.line_number;

        while let Some(current_line) = prepared_cache.line(line_number) {
            let line = current_line.prepared;
            if line.is_empty() {
                break;
            }
            if line.to_ascii_lowercase().contains("http") {
                break;
            }

            let piece = if line_number == prepared_line.line_number {
                PREFIX_RE.replace(line, "").to_string()
            } else {
                line.to_string()
            };
            if !piece.trim().is_empty() {
                parts.push(piece);
            }
            end_line = line_number;
            line_number = line_number.next();
        }

        if parts.is_empty() {
            continue;
        }

        let mut combined = normalize_whitespace(&parts.join(" "));
        combined = combined.replace(['(', ')'], " ");
        combined = URL_RE.replace_all(&combined, " ").into_owned();
        combined = IFROSS_TAIL_RE.replace_all(&combined, " ").into_owned();
        combined = normalize_whitespace(&combined);
        combined = combined.trim().to_string();
        if combined.is_empty() {
            continue;
        }

        let Some(author) = refine_author(&combined) else {
            continue;
        };
        authors.push(AuthorDetection {
            author: author.clone(),
            start_line: prepared_line.line_number,
            end_line,
        });

        authors.retain(|a| !(author.starts_with(&a.author) && a.author.len() < author.len()));
    }
}

pub(in super::super) fn extract_with_additional_hacking_by_authors(
    prepared_cache: &PreparedLines<'_>,
) -> Vec<AuthorDetection> {
    let mut authors = Vec::new();
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^\s*with\s+additional\s+hacking\s+by\s+(?P<who>.+?)\s*$").unwrap()
    });

    for prepared_line in prepared_cache.iter_non_empty() {
        let Some(cap) = RE.captures(prepared_line.prepared) else {
            continue;
        };
        let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
        if who.is_empty() {
            continue;
        }
        if let Some(author) = refine_author(who) {
            authors.push(AuthorDetection {
                author,
                start_line: prepared_line.line_number,
                end_line: prepared_line.line_number,
            });
        }
    }

    authors
}

pub(in super::super) fn extract_parenthesized_inline_by_authors(
    raw_lines: &[&str],
) -> Vec<AuthorDetection> {
    let mut authors = Vec::new();
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^\s*copyright\b.*\((?:written|authored|created|developed)\s+by\s+(?P<who>[^)]+)\)",
        )
        .unwrap()
    });

    for (idx, raw) in raw_lines.iter().enumerate() {
        let ln = idx + 1;
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let Some(cap) = RE.captures(line) else {
            continue;
        };
        let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
        if who.is_empty() {
            continue;
        }
        if let Some(author) = refine_author(who) {
            authors.push(AuthorDetection {
                author,
                start_line: LineNumber::new(ln).expect("invalid line number"),
                end_line: LineNumber::new(ln).expect("invalid line number"),
            });
        }
    }

    authors
}

pub(in super::super) fn merge_metadata_author_and_email_lines(
    prepared_cache: &PreparedLines<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    let has_metadata = prepared_cache
        .iter()
        .any(|line| line.prepared.trim_start().starts_with("Metadata-Version:"));

    if !has_metadata {
        return;
    }

    for prepared_line in prepared_cache.iter_non_empty() {
        let author_line = prepared_line.prepared;
        if author_line.is_empty() {
            continue;
        }
        if !author_line.to_ascii_lowercase().starts_with("author:") {
            continue;
        }
        let Some((_, name_raw)) = author_line.split_once(':') else {
            continue;
        };
        let name = name_raw.trim();
        if name.is_empty() {
            continue;
        }

        let mut next_line_number = prepared_line.line_number.next();
        while let Some(email_line) = prepared_cache.line(next_line_number) {
            let email_line = email_line.prepared;
            if email_line.is_empty() {
                break;
            }
            if email_line.to_ascii_lowercase().starts_with("author:") {
                break;
            }

            if !email_line.to_ascii_lowercase().starts_with("author-email") {
                next_line_number = next_line_number.next();
                continue;
            }
            let Some((_, email_raw)) = email_line.split_once(':') else {
                continue;
            };
            let email = email_raw.trim();
            if email.is_empty() {
                continue;
            }

            let combined_raw = format!("{name} Author-email {email}");
            let combined = normalize_whitespace(&combined_raw);

            authors.push(AuthorDetection {
                author: combined,
                start_line: prepared_line.line_number,
                end_line: next_line_number,
            });

            authors.retain(|a| {
                if a.start_line == prepared_line.line_number
                    && a.end_line == prepared_line.line_number
                    && a.author == name
                {
                    return false;
                }
                if a.start_line == next_line_number
                    && a.end_line == next_line_number
                    && a.author.to_ascii_lowercase() == format!("author-email {email}")
                {
                    return false;
                }
                true
            });

            break;
        }
    }
}

pub(in super::super) fn extract_debian_maintainer_authors(
    prepared_cache: &PreparedLines<'_>,
) -> Vec<AuthorDetection> {
    let mut authors = Vec::new();
    if prepared_cache.is_empty() {
        return authors;
    }

    static DEBIANIZED_BY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bdebianized\s+by\s+(?P<who>.+?)(?:\s+on\b|\s*$)").unwrap()
    });
    static CO_MAINTAINER_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?:debianized\s+by|new\s+co-maintainer|co-maintainer)\s+(?P<who>.+?)(?:\s+\d{4}-\d{2}-\d{1,2})?\s*$",
        )
        .unwrap()
    });
    static MAINTAINED_BY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^maintained\s+by\s+(?P<who>.+?)(?:\s+on\b|\s+since\b|\s*$)").unwrap()
    });

    for prepared_line in prepared_cache.iter_non_empty() {
        let who_raw = if let Some(cap) = CO_MAINTAINER_RE.captures(prepared_line.prepared) {
            cap.name("who").map(|m| m.as_str()).unwrap_or("")
        } else if let Some(cap) = DEBIANIZED_BY_RE.captures(prepared_line.prepared) {
            cap.name("who").map(|m| m.as_str()).unwrap_or("")
        } else if let Some(cap) = MAINTAINED_BY_RE.captures(prepared_line.prepared) {
            cap.name("who").map(|m| m.as_str()).unwrap_or("")
        } else {
            ""
        };

        let who = who_raw.trim();
        if who.is_empty() {
            continue;
        }

        let Some(author) = refine_author(who) else {
            continue;
        };

        authors.push(AuthorDetection {
            author,
            start_line: prepared_line.line_number,
            end_line: prepared_line.line_number,
        });
    }

    authors
}

pub(in super::super) fn extract_maintainers_label_authors(
    prepared_cache: &PreparedLines<'_>,
) -> Vec<AuthorDetection> {
    let mut authors = Vec::new();
    if prepared_cache.is_empty() {
        return authors;
    }

    static MAINTAINERS_LABEL_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^maintainers?\s*:?[ \t]+(?P<who>.+)$").unwrap());
    static GITREPO_SUFFIX_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\s+GitRepo\s+https?://\S+.*$").unwrap());

    for prepared_line in prepared_cache.iter_non_empty() {
        let line = prepared_line.prepared.trim_start_matches('*').trim_start();
        if line.is_empty() {
            continue;
        }

        let Some(cap) = MAINTAINERS_LABEL_RE.captures(line) else {
            continue;
        };

        let who_raw = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
        if who_raw.is_empty() || (!who_raw.contains('@') && !who_raw.contains('<')) {
            continue;
        }

        let candidate = GITREPO_SUFFIX_RE.replace(who_raw, "");
        let candidate = candidate.trim().trim_end_matches(',').trim();
        let author = normalize_whitespace(candidate);
        if author.is_empty() {
            continue;
        }

        authors.push(AuthorDetection {
            author,
            start_line: prepared_line.line_number,
            end_line: prepared_line.line_number,
        });
    }

    authors
}

pub(in super::super) fn extract_created_by_project_author(
    prepared_cache: &PreparedLines<'_>,
) -> Vec<AuthorDetection> {
    let mut authors = Vec::new();
    if prepared_cache.is_empty() {
        return authors;
    }

    static CREATED_BY_PROJECT_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\bcreated\s+by\s+the\s+project\b").unwrap());

    for prepared_line in prepared_cache.iter_non_empty() {
        if CREATED_BY_PROJECT_RE.is_match(prepared_line.prepared) {
            let author = "the Project".to_string();
            authors.push(AuthorDetection {
                author,
                start_line: prepared_line.line_number,
                end_line: prepared_line.line_number,
            });
            break;
        }
    }

    authors
}

pub(in super::super) fn extract_created_by_authors(
    prepared_cache: &PreparedLines<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static CREATED_BY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^\s*created\s+by\s+(?P<who>.+?)\s*$").unwrap());

    for prepared_line in prepared_cache.iter_non_empty() {
        let Some(cap) = CREATED_BY_RE.captures(prepared_line.prepared) else {
            continue;
        };
        let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
        if who.is_empty() {
            continue;
        }

        let who_lower = who.to_ascii_lowercase();
        let has_email_like =
            who.contains('@') || (who_lower.contains(" at ") && who_lower.contains(" dot "));
        if !has_email_like {
            continue;
        }

        let Some(author) = refine_author_with_optional_handle_suffix(who) else {
            continue;
        };
        authors.push(AuthorDetection {
            author: author.clone(),
            start_line: prepared_line.line_number,
            end_line: prepared_line.line_number,
        });

        authors.retain(|a| {
            !(a.start_line == prepared_line.line_number
                && a.end_line == prepared_line.line_number
                && author.starts_with(&a.author)
                && a.author.len() < author.len())
        });
    }
}

pub(in super::super) fn extract_toml_author_assignment_authors(
    raw_lines: &[&str],
) -> Vec<AuthorDetection> {
    let mut authors = Vec::new();
    if raw_lines.is_empty() {
        return authors;
    }

    static TOML_AUTHOR_ASSIGNMENT_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?i)^\s*authors?\s*=\s*(?P<rhs>.+?)\s*$"#).unwrap());
    static QUOTED_VALUE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"\"(?P<value>(?:\\.|[^\"])*)\""#).unwrap());

    for (idx, raw_line) in raw_lines.iter().enumerate() {
        let ln = idx + 1;
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        let Some(cap) = TOML_AUTHOR_ASSIGNMENT_RE.captures(line) else {
            continue;
        };
        let rhs = cap.name("rhs").map(|m| m.as_str()).unwrap_or("").trim();
        if rhs.is_empty() {
            continue;
        }
        let rhs_lower = rhs.to_ascii_lowercase();
        if rhs_lower.contains("new author") || rhs_lower.contains("name:") {
            continue;
        }

        let values: Vec<String> = QUOTED_VALUE_RE
            .captures_iter(rhs)
            .filter_map(|value_cap| {
                value_cap
                    .name("value")
                    .map(|m| m.as_str().trim().to_string())
            })
            .filter(|value| !value.is_empty())
            .collect();
        if values.is_empty() {
            continue;
        }

        let candidates: Vec<String> = if values.len() == 1 {
            values
        } else {
            vec![values.join(" ")]
        };

        for candidate in candidates {
            let Some(author) = refine_author_with_optional_handle_suffix(&candidate) else {
                continue;
            };
            authors.push(AuthorDetection {
                author,
                start_line: LineNumber::new(ln).expect("invalid line number"),
                end_line: LineNumber::new(ln).expect("invalid line number"),
            });
        }
    }

    authors
}

pub(in super::super) fn extract_written_by_comma_and_copyright_authors(
    prepared_cache: &PreparedLines<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static WRITTEN_BY_AND_COPYRIGHT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bwritten\s+by\s+(?P<who>.+?),\s+and\s+copyright\b").unwrap()
    });

    for prepared_line in prepared_cache.iter_non_empty() {
        let Some(cap) = WRITTEN_BY_AND_COPYRIGHT_RE.captures(prepared_line.prepared) else {
            continue;
        };
        let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
        if who.is_empty() {
            continue;
        }
        let Some(author) = refine_author(who) else {
            continue;
        };
        authors.retain(|a| {
            !(a.start_line == prepared_line.line_number && a.end_line == prepared_line.line_number)
        });
        authors.push(AuthorDetection {
            author,
            start_line: prepared_line.line_number,
            end_line: prepared_line.line_number,
        });
    }
}

pub(in super::super) fn extract_package_comment_named_authors(
    prepared_cache: &PreparedLines<'_>,
) -> Vec<AuthorDetection> {
    let mut authors = Vec::new();
    if prepared_cache.is_empty() {
        return authors;
    }

    static COMMENT_AUTHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)\b(?:was originally written by|was originally implemented by|it is now maintained by|this package is maintained for debian by)\s+(?P<who>.+?)(?:[.,;](?:\s|$)|$)",
        )
        .unwrap()
    });
    static RAW_ANGLE_EMAIL_AUTHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<name>[A-Z][^<>@]+?)\s*<(?P<email>[^>\s]+@[^>\s]+)>$").unwrap()
    });

    for prepared_line in prepared_cache.iter_non_empty() {
        for cap in COMMENT_AUTHOR_RE.captures_iter(prepared_line.prepared) {
            let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
            if who.is_empty() || who.to_ascii_lowercase().starts_with("the ") {
                continue;
            }

            let author = if let Some(cap) = RAW_ANGLE_EMAIL_AUTHOR_RE.captures(who) {
                let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
                let email = cap.name("email").map(|m| m.as_str()).unwrap_or("").trim();
                (!name.is_empty() && !email.is_empty()).then(|| format!("{name} <{email}>"))
            } else {
                refine_author(who)
            };

            if let Some(author) = author {
                authors.push(AuthorDetection {
                    author,
                    start_line: prepared_line.line_number,
                    end_line: prepared_line.line_number,
                });
            }
        }
    }

    authors
}

pub(in super::super) fn extract_developed_by_sentence_authors(
    prepared_cache: &PreparedLines<'_>,
) -> Vec<AuthorDetection> {
    let mut authors = Vec::new();
    if prepared_cache.is_empty() {
        return authors;
    }

    static DEVELOPED_BY_PREFIX_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^\s*developed\s+by\s+(?P<rest>.+)$").unwrap());

    for prepared_line in prepared_cache.iter_non_empty() {
        let Some(cap) = DEVELOPED_BY_PREFIX_RE.captures(prepared_line.prepared) else {
            continue;
        };
        let rest = cap.name("rest").map(|m| m.as_str()).unwrap_or("").trim();
        if rest.is_empty() {
            continue;
        }

        let rest_lower = rest.to_ascii_lowercase();
        let Some(is_idx) = rest_lower.find(" is ") else {
            continue;
        };
        let before_is = rest[..is_idx].trim_end();
        let Some(split_idx) = before_is.rfind(". ") else {
            continue;
        };
        let p1 = before_is[..split_idx + 1].trim();
        let p2 = before_is[split_idx + 2..].trim();
        if p1.is_empty() || p2.is_empty() {
            continue;
        }

        let candidate = format!("{p1} {p2}");
        let author = refine_author(&candidate).unwrap_or(candidate);
        if author.is_empty() {
            continue;
        }

        authors.push(AuthorDetection {
            author,
            start_line: prepared_line.line_number,
            end_line: prepared_line.line_number,
        });
    }

    authors
}

pub(in super::super) fn extract_developed_by_phrase_authors(
    prepared_cache: &PreparedLines<'_>,
) -> Vec<AuthorDetection> {
    let mut authors = Vec::new();
    if prepared_cache.is_empty() {
        return authors;
    }

    static DEVELOPED_BY_PHRASE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bdeveloped\s+by\s+(?P<who>.+?)\s+and\s+to\s+credit\b").unwrap()
    });

    for prepared_line in prepared_cache.iter_non_empty() {
        for cap in DEVELOPED_BY_PHRASE_RE.captures_iter(prepared_line.prepared) {
            let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
            if who.is_empty() {
                continue;
            }

            if who.split_whitespace().count() < 4 {
                continue;
            }

            let author = refine_author(who).unwrap_or_else(|| who.to_string());
            if author.is_empty() {
                continue;
            }

            authors.push(AuthorDetection {
                author,
                start_line: prepared_line.line_number,
                end_line: prepared_line.line_number,
            });
        }
    }

    authors
}

pub(in super::super) fn extract_developed_by_contributors_authors(
    prepared_cache: &PreparedLines<'_>,
) -> Vec<AuthorDetection> {
    let mut authors = Vec::new();
    if prepared_cache.is_empty() {
        return authors;
    }

    static DEVELOPED_BY_CONTRIBUTORS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)\bdeveloped\s+by\s+(?P<who>(?:the\s+)?.+?\band\s+its\s+contributors)\.?(?:\s|$)",
        )
        .unwrap()
    });

    for prepared_line in prepared_cache.iter_non_empty() {
        if !prepared_line
            .prepared
            .to_ascii_lowercase()
            .contains("developed by")
        {
            continue;
        }

        let mut window = prepared_line.prepared.to_string();
        let mut end_line = prepared_line.line_number;
        if !window.to_ascii_lowercase().contains("contributors")
            && let Some(next) = prepared_cache.line(prepared_line.line_number.next())
            && !next.prepared.is_empty()
        {
            window.push(' ');
            window.push_str(next.prepared);
            end_line = next.line_number;
        }

        let Some(cap) = DEVELOPED_BY_CONTRIBUTORS_RE.captures(&window) else {
            continue;
        };
        let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
        if who.is_empty() {
            continue;
        }

        let Some(author) = refine_author(who) else {
            continue;
        };

        authors.push(AuthorDetection {
            author,
            start_line: prepared_line.line_number,
            end_line,
        });
    }

    authors
}

pub(in super::super) fn extract_notice_developed_by_authors(
    prepared_cache: &PreparedLines<'_>,
) -> Vec<AuthorDetection> {
    let mut authors = Vec::new();
    if prepared_cache.is_empty() {
        return authors;
    }

    const NOTICE_PREFIX: &str = "this product includes software developed by";
    const NOTICE_ALSO_PREFIX: &str = "this product also includes software developed by";

    for prepared_line in prepared_cache.iter_non_empty() {
        let lower = prepared_line.prepared.to_ascii_lowercase();
        if !lower.contains("this product") || !lower.contains("includes software developed by") {
            continue;
        }

        let mut window = prepared_line.prepared.to_string();
        let mut end_line = prepared_line.line_number;
        let should_extend = lower.trim_end().ends_with("developed by")
            || (!window.contains(')') && !window.trim_end().ends_with('.'));
        if should_extend {
            for offset in 1..=2 {
                let next_value = prepared_line.line_number.get() + offset;
                let Some(next_line) =
                    prepared_cache.line(LineNumber::new(next_value).expect("valid line"))
                else {
                    break;
                };
                if next_line.prepared.is_empty() {
                    break;
                }
                window.push(' ');
                window.push_str(next_line.prepared);
                end_line = next_line.line_number;
                if next_line.prepared.contains('.') {
                    break;
                }
            }
        }

        let window_lower = window.to_ascii_lowercase();
        let who = if let Some(index) = window_lower.find(NOTICE_ALSO_PREFIX) {
            &window[index + NOTICE_ALSO_PREFIX.len()..]
        } else if let Some(index) = window_lower.find(NOTICE_PREFIX) {
            &window[index + NOTICE_PREFIX.len()..]
        } else {
            continue;
        };
        let who = who
            .trim()
            .trim_matches(&['"', '\''][..])
            .trim_end_matches(&['.', ';', '"', '\''][..])
            .trim();
        let who_lower = who.to_ascii_lowercase();
        let has_url = who.contains("http://") || who.contains("https://");
        if !has_url && !who_lower.starts_with("the ") {
            continue;
        }
        let Some(author) = refine_notice_collective_author(who) else {
            continue;
        };

        authors.push(AuthorDetection {
            author,
            start_line: prepared_line.line_number,
            end_line,
        });
    }

    authors
}

pub(in super::super) fn extract_json_author_object_authors(
    raw_lines: &[&str],
) -> Vec<AuthorDetection> {
    let mut authors = Vec::new();
    if raw_lines.is_empty() {
        return authors;
    }

    for (idx, line) in raw_lines.iter().enumerate() {
        if !line.contains("\"author\"") {
            continue;
        }

        let start = idx.saturating_sub(1);
        let end = (idx + 4).min(raw_lines.len());
        let window = raw_lines[start..end].join(" ");
        if json_window_contains_code_like_author_usage(&window) {
            continue;
        }
        let Some(name) = extract_author_name_from_json_window(&window) else {
            continue;
        };
        let Some(author) = refine_json_author_candidate(&name, &window) else {
            continue;
        };

        authors.push(AuthorDetection {
            author,
            start_line: LineNumber::new(idx + 1).expect("invalid line number"),
            end_line: LineNumber::new(end).expect("invalid line number"),
        });
    }

    authors
}

pub(in super::super) fn extract_maintained_by_authors(
    prepared_cache: &PreparedLines<'_>,
) -> Vec<AuthorDetection> {
    let mut authors = Vec::new();
    if prepared_cache.is_empty() {
        return authors;
    }

    static MAINTAINED_BY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bmaintained\s+by\s+(?P<who>.+?)(?:\s+(?:on|since|for)\b|$)").unwrap()
    });

    for prepared_line in prepared_cache.iter_non_empty() {
        for cap in MAINTAINED_BY_RE.captures_iter(prepared_line.prepared) {
            let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
            if who.is_empty() {
                continue;
            }
            if !who.to_ascii_lowercase().starts_with("the ") {
                continue;
            }
            let Some(author) = refine_author(who) else {
                continue;
            };
            authors.push(AuthorDetection {
                author,
                start_line: prepared_line.line_number,
                end_line: prepared_line.line_number,
            });
        }
    }

    authors
}

pub(in super::super) fn extract_converted_to_by_authors(
    prepared_cache: &PreparedLines<'_>,
) -> Vec<AuthorDetection> {
    let mut authors = Vec::new();
    if prepared_cache.is_empty() {
        return authors;
    }

    static CONVERTED_BY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^\s*converted\b.*\bby\s+(?P<who>.+)$").unwrap());
    static CONVERTED_TO_THE_BY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^\s*converted\s+to\s+the\b.*\bby\s+(?P<who>.+)$").unwrap()
    });
    static CONVERTED_TO_VERSION_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\bconverted\s+to\s+\d+\.\d+\b").unwrap());

    for prepared_line in prepared_cache.iter_non_empty() {
        let line = prepared_line.prepared.trim_start_matches('*').trim_start();
        if line.is_empty() {
            continue;
        }

        if CONVERTED_TO_VERSION_RE.is_match(line) {
            continue;
        }

        let mut add_converted_variant = false;
        let who_raw = if let Some(cap) = CONVERTED_TO_THE_BY_RE.captures(line) {
            add_converted_variant = true;
            cap.name("who").map(|m| m.as_str()).unwrap_or("")
        } else if let Some(cap) = CONVERTED_BY_RE.captures(line) {
            cap.name("who").map(|m| m.as_str()).unwrap_or("")
        } else {
            ""
        };

        let who = who_raw.trim();
        if who.is_empty() {
            continue;
        }

        if !who.contains('@') && !who.contains('<') {
            continue;
        }
        let Some(author) = refine_author(who) else {
            continue;
        };
        authors.push(AuthorDetection {
            author: author.clone(),
            start_line: prepared_line.line_number,
            end_line: prepared_line.line_number,
        });
        if add_converted_variant {
            let converted = format!("{author} Converted");
            authors.push(AuthorDetection {
                author: converted,
                start_line: prepared_line.line_number,
                end_line: prepared_line.line_number,
            });
        }
    }

    authors
}

pub(in super::super) fn extract_various_bugfixes_and_enhancements_by_authors(
    prepared_cache: &PreparedLines<'_>,
) -> Vec<AuthorDetection> {
    let mut authors = Vec::new();
    if prepared_cache.is_empty() {
        return authors;
    }

    static VARIOUS_BUGFIXES_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^\s*various\s+bugfixes\s+and\s+enhancements\s+by\s+(?P<who>.+)$").unwrap()
    });

    for prepared_line in prepared_cache.iter_non_empty() {
        let line = prepared_line.prepared.trim_start_matches('*').trim_start();
        if line.is_empty() {
            continue;
        }
        let Some(cap) = VARIOUS_BUGFIXES_RE.captures(line) else {
            continue;
        };
        let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
        if who.is_empty() {
            continue;
        }
        if !who.contains('@') && !who.contains('<') {
            continue;
        }
        let Some(author) = refine_author(who) else {
            continue;
        };
        authors.push(AuthorDetection {
            author,
            start_line: prepared_line.line_number,
            end_line: prepared_line.line_number,
        });
    }

    authors
}

pub(in super::super) fn extract_dense_name_email_author_lists(
    prepared_cache: &PreparedLines<'_>,
) -> Vec<AuthorDetection> {
    let mut authors = Vec::new();
    if prepared_cache.is_empty() {
        return authors;
    }

    static NAME_EMAIL_LINE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<name>[^<\n]{2,120})\s*<(?P<email>[^>\s]+@[^>\s]+)>\s*$").unwrap()
    });

    let non_empty_lines: Vec<(LineNumber, String)> = prepared_cache
        .iter_non_empty()
        .map(|line| (line.line_number, line.prepared.to_string()))
        .collect();
    if non_empty_lines.len() < 2 {
        return authors;
    }

    let mut matched: Vec<(LineNumber, String)> = Vec::new();
    for (ln, line) in &non_empty_lines {
        let Some(cap) = NAME_EMAIL_LINE_RE.captures(line) else {
            continue;
        };
        let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
        let email = cap.name("email").map(|m| m.as_str()).unwrap_or("").trim();
        if name.is_empty() || email.is_empty() {
            continue;
        }
        let name_lower = name.to_ascii_lowercase();
        if name.contains(':')
            || name_lower.contains("author")
            || name_lower.contains("maintainer")
            || name_lower.contains("copyright")
        {
            continue;
        }
        matched.push((*ln, format!("{name} <{email}>")));
    }

    if matched.len() < 2 {
        return authors;
    }
    if matched.len() * 2 < non_empty_lines.len() {
        return authors;
    }

    for (ln, candidate) in matched {
        let Some(author) = refine_author(&candidate) else {
            continue;
        };
        authors.push(AuthorDetection {
            author,
            start_line: ln,
            end_line: ln,
        });
    }

    authors
}
