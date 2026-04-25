// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use super::super::token_utils::normalize_whitespace;
use crate::copyright::line_tracking::PreparedLines;
use crate::copyright::prepare::prepare_text_line;
use crate::copyright::refiner::{looks_like_name_with_parenthesized_url, refine_author};
use crate::copyright::types::{AuthorDetection, CopyrightDetection};
use crate::models::LineNumber;

pub(in super::super) fn drop_merged_dash_bullet_attribution_authors(
    authors: &mut Vec<AuthorDetection>,
) {
    authors.retain(|author| {
        let lower = author.author.to_ascii_lowercase();
        !(lower.contains(" - updated by ")
            || lower.contains(" - added to by ")
            || lower.contains(" - ported to ")
            || lower.contains(" - adapted to ")
            || lower.contains(" - modified by ")
            || lower.contains(" - valuable contributions by ")
            || lower.starts_with("mainline integration by ")
            || lower.starts_with("updated by ")
            || lower.starts_with("added to by ")
            || lower.starts_with("ported to ")
            || lower.starts_with("adapted to ")
            || lower.starts_with("modified by ")
            || lower.starts_with("valuable contributions by "))
    });
}

pub(in super::super) fn refine_author_with_optional_handle_suffix(
    candidate: &str,
) -> Option<String> {
    static TRAILING_HANDLE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\s*\(@[A-Za-z0-9_.-]+\)\s*$").unwrap());

    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        return None;
    }

    let without_handle = TRAILING_HANDLE_RE.replace(trimmed, "").trim().to_string();
    if without_handle != trimmed {
        return refine_author(&without_handle);
    }

    refine_author(trimmed)
}

pub(in super::super) fn drop_authors_embedded_in_copyrights(
    copyrights: &[CopyrightDetection],
    authors: &mut Vec<AuthorDetection>,
) {
    if copyrights.is_empty() || authors.is_empty() {
        return;
    }

    authors.retain(|a| {
        let a_lower = a.author.to_lowercase();
        !copyrights.iter().any(|c| {
            if a.start_line < c.start_line || a.end_line > c.end_line {
                return false;
            }
            let c_lower = c.copyright.to_lowercase();
            if a.author.contains('@')
                && c_lower.starts_with("copyright")
                && c_lower.contains(&a_lower)
            {
                return true;
            }
            if c_lower.contains("authors") {
                if a.author.contains('@') {
                    return false;
                }
                return c_lower.contains(&a_lower);
            }
            if c_lower.contains("author") {
                return c_lower.contains(&a_lower);
            }
            false
        })
    });
}

pub(in super::super) fn drop_authors_from_copyright_by_lines(
    prepared_cache: &PreparedLines<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if authors.is_empty() || prepared_cache.is_empty() {
        return;
    }

    static YEAR_PREFIX_BY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^\s*(?:[/#*;!-]+\s*)?(?:\d{4}(?:[-–/]\d{1,4})*|\d{4}\s*[-–]\s*\d{4})\s+by\s+",
        )
        .unwrap()
    });
    static INLINE_ATTRIBUTION_PARENS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\((?:written|authored|created|developed)\s+by\s+[^)]+\)").unwrap()
    });

    authors.retain(|author| {
        let author_lower = author.author.to_ascii_lowercase();
        let mut line_number = author.start_line;

        loop {
            let Some(raw) = prepared_cache.line(line_number).map(|line| line.raw) else {
                return true;
            };
            let lower = raw.to_ascii_lowercase();
            if INLINE_ATTRIBUTION_PARENS_RE.is_match(raw) {
                if line_number == author.end_line {
                    return true;
                }
                line_number = line_number.next();
                continue;
            }
            if lower.trim_start().starts_with("copyright")
                && lower.contains(" by ")
                && lower.contains(&author_lower)
            {
                return false;
            }

            if let Some(prev_line_number) = line_number.prev()
                && let Some(prev) = prepared_cache.line(prev_line_number).map(|line| line.raw)
            {
                let prev_lower = prev.to_ascii_lowercase();
                if prev_lower.contains("copyright")
                    && YEAR_PREFIX_BY_RE.is_match(raw)
                    && lower.contains(&author_lower)
                {
                    return false;
                }
            }

            if line_number == author.end_line {
                return true;
            }
            line_number = line_number.next();
        }
    });
}

pub(in super::super) fn drop_author_colon_lines_absorbed_into_year_only_copyrights(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &[CopyrightDetection],
    authors: &mut Vec<AuthorDetection>,
) {
    if copyrights.is_empty() || authors.is_empty() || prepared_cache.raw_line_count() < 2 {
        return;
    }

    static YEAR_ONLY_COPY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?ix)^copyright\s*\(c\)\s*(?P<years>[0-9\s,\-–/]+)\s+.+$").unwrap()
    });
    static AUTHOR_LINE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?ix)^author\s*:\s*(?P<name>[^<]+?)\s*(?:<\s*(?P<email>[^>\s]+@[^>\s]+)\s*>)?\s*$",
        )
        .unwrap()
    });

    authors.retain(|author| {
        if author.start_line != author.end_line {
            return true;
        }
        let Some(previous_line_number) = author.start_line.prev() else {
            return true;
        };

        let Some(raw_line) = prepared_cache.line(author.start_line).map(|line| line.raw) else {
            return true;
        };
        let normalized_line = raw_line.trim().trim_start_matches('*').trim_start();
        if !AUTHOR_LINE_RE.is_match(normalized_line) {
            return true;
        }

        copyrights.iter().all(|copyright| {
            if copyright.start_line != previous_line_number
                || copyright.end_line != author.start_line
            {
                return true;
            }
            !YEAR_ONLY_COPY_RE.is_match(copyright.copyright.as_str())
        })
    });
}

pub(in super::super) fn drop_shadowed_prefix_authors(authors: &mut Vec<AuthorDetection>) {
    if authors.len() < 2 {
        return;
    }
    let originals = authors.clone();

    authors.retain(|author| {
        let a = author.author.trim();
        if a.is_empty() {
            return true;
        }

        for other in &originals {
            let b = other.author.trim();
            if b.len() <= a.len() {
                continue;
            }
            if let Some(stripped) = b.strip_prefix(a) {
                let tail = stripped.trim_start();
                let boundary = b
                    .as_bytes()
                    .get(a.len())
                    .is_some_and(|ch| ch.is_ascii_whitespace() || matches!(ch, b',' | b'/' | b'('));

                let a_has_email = a.contains('@') || a.contains('<');
                let b_has_email = b.contains('@') || b.contains('<');

                let short_word = a.split_whitespace().count() == 1;
                if short_word && boundary {
                    if a.chars().all(|c| c.is_ascii_lowercase()) {
                        continue;
                    }
                    return false;
                }
                if !a_has_email && b_has_email && boundary {
                    return false;
                }
                if boundary {
                    let tail_lower = tail.to_ascii_lowercase();
                    if a_has_email && b_has_email {
                        continue;
                    }
                    if tail.starts_with(',')
                        || tail.starts_with('<')
                        || tail_lower.starts_with("or")
                        || tail_lower.starts_with("and")
                        || tail_lower.starts_with("author-email")
                    {
                        return false;
                    }
                }
            }
        }

        true
    });
}

pub(in super::super) fn drop_shadowed_compound_email_authors(authors: &mut Vec<AuthorDetection>) {
    if authors.is_empty() {
        return;
    }

    static EMAIL_AUTHOR_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"[A-Z][^<\n]{0,120}?<[^>\s]+@[^>\s]+>"#).unwrap());
    static TRAILING_CURRENT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"^(?P<author>[A-Z][^<\n]{0,120}?<[^>\s]+@[^>\s]+>)\s+Current(?:\s+.*)?$"#)
            .unwrap()
    });
    static TRAILING_MODULE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"^(?P<author>[A-Z][^<\n]{0,120}?<[^>\s]+@[^>\s]+>)\s+MODULE_[A-Z_].*$"#)
            .unwrap()
    });

    let existing: HashSet<String> = authors
        .iter()
        .map(|a| a.author.trim().to_string())
        .collect();

    authors.retain(|author| {
        let raw = author.author.trim();

        if let Some(cap) = TRAILING_CURRENT_RE.captures(raw) {
            let clean = cap
                .name("author")
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();
            return clean.is_empty() || !existing.contains(&clean);
        }

        if let Some(cap) = TRAILING_MODULE_RE.captures(raw) {
            let clean = cap
                .name("author")
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();
            return clean.is_empty() || !existing.contains(&clean);
        }

        let matches: Vec<String> = EMAIL_AUTHOR_RE
            .find_iter(raw)
            .map(|m| m.as_str().trim().to_string())
            .collect();
        !(matches.len() >= 2 && matches.iter().all(|candidate| existing.contains(candidate)))
    });
}

pub(in super::super) fn drop_ref_markup_authors(authors: &mut Vec<AuthorDetection>) {
    authors.retain(|author| !author.author.contains("@ref"));
}

pub(in super::super) fn normalize_json_blob_authors(
    raw_lines: &[&str],
    authors: &mut Vec<AuthorDetection>,
) {
    let mut normalized: Vec<AuthorDetection> = Vec::with_capacity(authors.len());
    let mut seen: HashSet<(LineNumber, LineNumber, String)> = HashSet::new();

    for author in authors.iter() {
        let Some(window) =
            json_author_window(raw_lines, author.start_line.get(), author.end_line.get())
        else {
            let key = (author.start_line, author.end_line, author.author.clone());
            if seen.insert(key) {
                normalized.push(author.clone());
            }
            continue;
        };

        if json_window_contains_code_like_author_usage(&window) {
            continue;
        }

        let replacement = if let Some(name) = extract_author_name_from_json_window(&window) {
            refine_json_author_candidate(&name, &window)
        } else if json_window_contains_developed_by(&window, &author.author) {
            refine_author(&author.author)
        } else {
            None
        };

        let Some(author_name) = replacement else {
            continue;
        };

        let key = (author.start_line, author.end_line, author_name.clone());
        if seen.insert(key) {
            normalized.push(AuthorDetection {
                author: author_name,
                start_line: author.start_line,
                end_line: author.end_line,
            });
        }
    }

    *authors = normalized;
}

pub(in super::super) fn drop_json_code_example_authors(
    raw_lines: &[&str],
    authors: &mut Vec<AuthorDetection>,
) {
    if raw_lines.is_empty() || authors.is_empty() {
        return;
    }

    authors.retain(|author| {
        if let Some(window) =
            surrounding_author_window(raw_lines, author.start_line.get(), author.end_line.get())
            && window_contains_code_style_author_usage(&window)
        {
            return false;
        }

        let Some(window) =
            json_author_window(raw_lines, author.start_line.get(), author.end_line.get())
        else {
            return true;
        };

        if json_window_contains_code_like_author_usage(&window) {
            return false;
        }

        if json_window_has_metadata_context(&window)
            || json_window_is_simple_author_only_fragment(&window)
        {
            return true;
        }

        let Some(name) = extract_author_name_from_json_window(&window) else {
            return true;
        };
        looks_like_name_with_parenthesized_url(name.trim())
    });
}

fn surrounding_author_window(
    raw_lines: &[&str],
    start_line: usize,
    end_line: usize,
) -> Option<String> {
    if start_line == 0
        || end_line == 0
        || start_line > raw_lines.len()
        || end_line > raw_lines.len()
    {
        return None;
    }
    let start = start_line.saturating_sub(2).max(1);
    let end = (end_line + 2).min(raw_lines.len());
    Some(raw_lines[start - 1..end].join(" "))
}

fn window_contains_code_style_author_usage(window: &str) -> bool {
    static CODE_OPERATOR_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"\$[A-Za-z_][A-Za-z0-9_]*"#).unwrap());
    static QUOTED_AUTHOR_ASSIGNMENT_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"\"author\"\s*:"#).unwrap());
    static BARE_OBJECT_AUTHOR_ASSIGNMENT_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"\{[^\r\n]*\bauthor\s*:\s*\"[^\"]+\""#).unwrap());

    CODE_OPERATOR_RE.is_match(window)
        && (QUOTED_AUTHOR_ASSIGNMENT_RE.is_match(window)
            || (window.contains('{')
                && window.contains('}')
                && BARE_OBJECT_AUTHOR_ASSIGNMENT_RE.is_match(window)))
}

fn is_json_like_line(line: &str) -> bool {
    static JSON_FIELD_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"^\s*\{?\s*"[^"]+"\s*:\s*(?:"[^"]*"|\{|\[|true|false|null|-?\d)"#).unwrap()
    });

    JSON_FIELD_RE.is_match(line) || line.trim() == "{" || line.trim() == "}" || line.trim() == "},"
}

fn json_author_window(raw_lines: &[&str], start_line: usize, end_line: usize) -> Option<String> {
    if start_line == 0
        || end_line == 0
        || start_line > raw_lines.len()
        || end_line > raw_lines.len()
    {
        return None;
    }
    let start = start_line.saturating_sub(2).max(1);
    let end = (end_line + 2).min(raw_lines.len());
    let lines = &raw_lines[start - 1..end];
    if !lines.iter().any(|line| is_json_like_line(line)) {
        return None;
    }
    Some(lines.join(" "))
}

pub(in super::super) fn json_window_contains_code_like_author_usage(window: &str) -> bool {
    static JSON_CODE_OPERATOR_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"\$[A-Za-z_][A-Za-z0-9_]*"#).unwrap());
    static JSON_AUTHOR_NUMERIC_VALUE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?i)(?:\"author\"|\bauthor)\s*:\s*(?:-?\d+|true|false|null)"#).unwrap()
    });

    JSON_CODE_OPERATOR_RE.is_match(window) || JSON_AUTHOR_NUMERIC_VALUE_RE.is_match(window)
}

fn json_window_has_metadata_context(window: &str) -> bool {
    static JSON_METADATA_KEY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?i)\"(?:name|supplier|publisher|version|license|licenses|bomFormat|components|purl|homepage|description|package|url)\"\s*:"#,
        )
        .unwrap()
    });

    JSON_METADATA_KEY_RE.is_match(window)
}

fn json_window_is_simple_author_only_fragment(window: &str) -> bool {
    static JSON_AUTHOR_ONLY_STRING_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?is)^\s*\{?\s*\"author\"\s*:\s*\"[^\"]+\"\s*,?\s*\}?\s*$"#).unwrap()
    });
    static JSON_AUTHOR_ONLY_OBJECT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?is)^\s*\{?\s*\"author\"\s*:\s*\{\s*\"name\"\s*:\s*\"[^\"]+\"(?:\s*,\s*\"(?:url|email)\"\s*:\s*\"[^\"]+\")*\s*\}\s*,?\s*\}?\s*$"#,
        )
        .unwrap()
    });

    JSON_AUTHOR_ONLY_STRING_RE.is_match(window) || JSON_AUTHOR_ONLY_OBJECT_RE.is_match(window)
}

pub(in super::super) fn looks_like_structured_json_author_fallback(value: &str) -> bool {
    let trimmed = value.trim();
    if looks_like_name_with_parenthesized_url(trimmed) {
        return true;
    }

    if trimmed.is_empty()
        || trimmed.contains('@')
        || trimmed.contains("http://")
        || trimmed.contains("https://")
        || json_window_contains_code_like_author_usage(trimmed)
    {
        return false;
    }

    let words: Vec<&str> = trimmed.split_whitespace().collect();
    if words.is_empty() {
        return false;
    }

    if words.len() == 1 {
        let token =
            words[0].trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '-' && ch != '\'');
        if token.len() < 4 {
            return false;
        }
        let lower = token.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "author" | "authors" | "guide" | "description" | "project"
        ) {
            return false;
        }
        return token
            .chars()
            .any(|ch| ch.is_uppercase() || ch.is_ascii_digit());
    }

    let lower = trimmed.to_ascii_lowercase();
    [
        " authors",
        " project",
        " team",
        " group",
        " foundation",
        " committee",
        " communities",
        " consortium",
        " developers",
    ]
    .iter()
    .any(|suffix| lower.ends_with(suffix))
}

pub(in super::super) fn refine_json_author_candidate(name: &str, window: &str) -> Option<String> {
    if json_window_contains_code_like_author_usage(window) {
        return None;
    }

    let prepared = prepare_text_line(name);
    let normalized = normalize_whitespace(&prepared);
    let trimmed = normalized
        .trim()
        .trim_end_matches(&[',', ';', '.'][..])
        .trim();

    if !json_window_has_metadata_context(window)
        && !json_window_is_simple_author_only_fragment(window)
        && !looks_like_name_with_parenthesized_url(trimmed)
    {
        return None;
    }

    if let Some(author) = refine_author(name) {
        return Some(author);
    }

    if !json_window_has_metadata_context(window)
        && !json_window_is_simple_author_only_fragment(window)
        && !looks_like_name_with_parenthesized_url(trimmed)
    {
        return None;
    }

    if looks_like_structured_json_author_fallback(trimmed) {
        Some(trimmed.to_string())
    } else {
        None
    }
}

pub(in super::super) fn extract_author_name_from_json_window(window: &str) -> Option<String> {
    static JSON_AUTHOR_OBJECT_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?is)"author"\s*:\s*\{[^{}]*?"name"\s*:\s*"(?P<name>[^"]+)""#).unwrap()
    });
    static JSON_AUTHOR_STRING_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?is)"author"\s*:\s*"(?P<name>[^"]+)""#).unwrap());

    JSON_AUTHOR_OBJECT_NAME_RE
        .captures(window)
        .or_else(|| JSON_AUTHOR_STRING_RE.captures(window))
        .and_then(|cap| cap.name("name"))
        .map(|m| m.as_str().trim().to_string())
        .filter(|name| !name.is_empty())
}

fn json_window_contains_developed_by(window: &str, author: &str) -> bool {
    let needle = format!("developed by {}", author.trim().to_ascii_lowercase());
    window.to_ascii_lowercase().contains(&needle)
}

pub(in super::super) fn drop_written_by_authors_preceded_by_copyright(
    prepared_cache: &PreparedLines<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if authors.is_empty() {
        return;
    }

    static WRITTEN_BY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^\s*written\s+by\s+(?P<who>.+)$").unwrap());
    static COPYRIGHT_HINT_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\bcopyright\b|\(c\)").unwrap());

    if prepared_cache.len() < 2 {
        return;
    }

    let mut to_drop: HashSet<String> = HashSet::new();
    for (prev, line) in prepared_cache.adjacent_pairs() {
        let Some(cap) = WRITTEN_BY_RE.captures(line.prepared) else {
            continue;
        };
        let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
        if who.is_empty() {
            continue;
        }
        if who.contains('@')
            || who.contains('<')
            || who.contains("http://")
            || who.contains("https://")
        {
            continue;
        }
        if !COPYRIGHT_HINT_RE.is_match(prev.prepared) {
            continue;
        }
        if let Some(author) = refine_author(who) {
            to_drop.insert(author);
        }
    }

    if to_drop.is_empty() {
        return;
    }
    authors.retain(|a| !to_drop.contains(&a.author));
}
