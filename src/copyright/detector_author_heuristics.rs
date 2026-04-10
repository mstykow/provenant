use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use super::normalize_whitespace;
use crate::copyright::line_tracking::PreparedLineCache;
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

pub(super) fn extract_markup_authors(content: &str, authors: &mut Vec<AuthorDetection>) {
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

    let mut seen: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();

    for captures in [
        AUTHOR_ATTR_DQ_RE.captures_iter(content).collect::<Vec<_>>(),
        AUTHOR_ATTR_SQ_RE.captures_iter(content).collect::<Vec<_>>(),
    ] {
        for cap in captures {
            let Some(full) = cap.get(0) else {
                continue;
            };
            let value = cap.get(1).map(|m| m.as_str()).unwrap_or("").trim();
            let normalized = normalize_markup_author_value(value);
            let Some(author) = refine_author(&normalized) else {
                continue;
            };
            if seen.insert(author.clone()) {
                let line = line_number_for_offset(content, full.start());
                authors.push(AuthorDetection {
                    author,
                    start_line: line,
                    end_line: line,
                });
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
        if seen.insert(author.clone()) {
            let line = line_number_for_offset(content, full.start());
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

pub(super) fn extract_dash_bullet_attribution_authors(
    prepared_cache: &mut PreparedLineCache<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    let mut seen: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();

    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(raw_line) = prepared_cache.raw_by_index(idx) else {
            continue;
        };
        let trimmed = raw_line.trim_start();
        if !trimmed.starts_with("- ") {
            continue;
        }
        let Some(author) = extract_dash_bullet_attribution_author(trimmed) else {
            continue;
        };
        if seen.insert(author.clone()) {
            authors.push(AuthorDetection {
                author,
                start_line: LineNumber::new(ln).expect("invalid line number"),
                end_line: LineNumber::new(ln).expect("invalid line number"),
            });
        }
    }
}

pub(super) fn extract_rst_field_authors(
    prepared_cache: &mut PreparedLineCache<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static RST_FIELD_AUTHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^:?(?:author(?:\s*&\s*maintainer)?|updated\s+by)\s*:\s*(?P<tail>.+)$")
            .unwrap()
    });
    static ATTRIBUTION_PREFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?:written|updated|authored|created|developed|maintained)\s+by\s+")
            .unwrap()
    });

    let mut seen: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();

    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(prepared) = prepared_cache.get_by_index(idx) else {
            continue;
        };
        let line = prepared.trim();
        if line.is_empty() {
            continue;
        }

        let Some(cap) = RST_FIELD_AUTHOR_RE.captures(line) else {
            continue;
        };
        let tail = cap.name("tail").map(|m| m.as_str()).unwrap_or("").trim();
        if tail.is_empty() {
            continue;
        }
        let stripped_tail = ATTRIBUTION_PREFIX_RE.replace(tail, "");
        let trimmed = trim_attribution_tail(stripped_tail.as_ref());
        let Some(author) = refine_author(&trimmed) else {
            continue;
        };
        if seen.insert(author.clone()) {
            authors.push(AuthorDetection {
                author,
                start_line: LineNumber::new(ln).expect("invalid line number"),
                end_line: LineNumber::new(ln).expect("invalid line number"),
            });
        }
    }
}

fn extract_author_colon_bullet_roster(
    segments: &[String],
    start_line: usize,
    authors: &mut Vec<AuthorDetection>,
    seen: &mut HashSet<String>,
) -> bool {
    static BARE_EMAIL_AUTHOR_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(?P<who>.+?<[^>\s]*@[^>\s]*>)\s*,?$").unwrap());

    if segments.is_empty()
        || !segments
            .iter()
            .any(|segment| segment.trim_start().starts_with('-'))
    {
        return false;
    }

    let mut extracted_any = false;
    for (offset, segment) in segments.iter().enumerate() {
        let trimmed = segment.trim();
        let line_no = start_line + offset;

        if !trimmed.trim_start().starts_with('-') {
            let lower = trimmed.to_ascii_lowercase();
            if lower.starts_with("with the help of ") {
                continue;
            }
            return false;
        }

        if let Some(author) = extract_dash_bullet_attribution_author(trimmed) {
            if seen.insert(author.clone()) {
                authors.push(AuthorDetection {
                    author,
                    start_line: LineNumber::new(line_no).expect("valid"),
                    end_line: LineNumber::new(line_no).expect("valid"),
                });
            }
            extracted_any = true;
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
        if seen.insert(author.clone()) {
            authors.push(AuthorDetection {
                author,
                start_line: LineNumber::new(line_no).expect("valid"),
                end_line: LineNumber::new(line_no).expect("valid"),
            });
        }
        extracted_any = true;
    }

    extracted_any
}

pub(super) fn extract_multiline_written_by_author_blocks(
    prepared_cache: &mut PreparedLineCache<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static WRITTEN_BY_SINGLE_LINE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^\s*written\s+by\s+(?P<who>.+?)(?:\s+for\b|$)").unwrap());
    static AUTHOR_EMAIL_HEAD_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?P<head>.+?<[^>]+>)(?:\s+(?:for|to)\b.*)?$").unwrap());
    static WRITTEN_BY_PREFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?:original(?:ly)?\s+)?(?:original\s+driver\s+)?(?:written|authored|created|developed)\s+by\s+(?P<who>.+)$",
        )
        .unwrap()
    });
    static MAINTAINED_BY_PREFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?:(?:it|this\s+package)\s+is\s+)?maintained(?:\s+for\s+debian)?\s+by\s+(?P<who>.+)$",
        )
        .unwrap()
    });

    let mut seen: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();

    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(raw_line) = prepared_cache.raw_by_index(idx) else {
            continue;
        };
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

            if let Some(author) = refine_author(who)
                && seen.insert(author.clone())
            {
                authors.push(AuthorDetection {
                    author,
                    start_line: LineNumber::new(ln).expect("invalid line number"),
                    end_line: LineNumber::new(ln).expect("invalid line number"),
                });
            }
        }
    }

    let mut i = 0;
    while i < prepared_cache.len() {
        let ln = i + 1;
        let Some(prepared) = prepared_cache.get_by_index(i) else {
            i += 1;
            continue;
        };
        let line = prepared.trim();
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
            i += 1;
            continue;
        }

        let mut block_lines: Vec<(usize, String)> = Vec::new();
        block_lines.push((ln, line.to_string()));

        let mut j = i + 1;
        while j < prepared_cache.len() {
            let next_ln = j + 1;
            let Some(next_prepared) = prepared_cache.get_by_index(j) else {
                break;
            };
            let next_line = next_prepared.trim();
            if next_line.is_empty() {
                break;
            }
            let next_lower = next_line.to_ascii_lowercase();
            if next_lower.starts_with("copyright") {
                break;
            }
            if !(next_lower.contains(" by ")
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

            block_lines.push((next_ln, next_line.to_string()));
            j += 1;
        }

        if block_lines.len() < 2 {
            i += 1;
            continue;
        }

        let start_line = block_lines.first().map(|(l, _)| *l).unwrap_or(ln);
        let end_line = block_lines.last().map(|(l, _)| *l).unwrap_or(ln);

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
            let combined_candidate = WRITTEN_BY_PREFIX_RE
                .captures(&combined_raw)
                .or_else(|| MAINTAINED_BY_PREFIX_RE.captures(&combined_raw))
                .and_then(|cap| cap.name("who").map(|m| m.as_str().trim()))
                .unwrap_or(combined_raw.as_str())
                .trim_end_matches('.')
                .trim();
            if let Some(combined) = refine_author(combined_candidate)
                && seen.insert(combined.clone())
            {
                authors.retain(|a| a.start_line.get() < start_line || a.end_line.get() > end_line);
                authors.push(AuthorDetection {
                    author: combined,
                    start_line: LineNumber::new(start_line).expect("valid"),
                    end_line: LineNumber::new(end_line).expect("valid"),
                });
                i = j;
                continue;
            }
        }

        let mut extracted_any = false;
        for (_l, raw_line) in &block_lines {
            let candidate = raw_line.trim();
            if let Some(cap) = WRITTEN_BY_PREFIX_RE
                .captures(candidate)
                .or_else(|| MAINTAINED_BY_PREFIX_RE.captures(candidate))
            {
                let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
                if !who.is_empty() {
                    let who = who.trim_end_matches('.').trim();
                    if !who.to_ascii_lowercase().starts_with("the ") {
                        if let Some(author) = refine_author(who)
                            && seen.insert(author.clone())
                        {
                            authors.push(AuthorDetection {
                                author,
                                start_line: LineNumber::new(start_line).expect("valid"),
                                end_line: LineNumber::new(end_line).expect("valid"),
                            });
                        }
                        extracted_any = true;
                    }
                    continue;
                }
            }
        }

        if !extracted_any {
            let combined_raw = block_lines
                .iter()
                .map(|(_, raw_line)| raw_line.trim())
                .collect::<Vec<_>>()
                .join(" ");
            let combined_candidate = WRITTEN_BY_PREFIX_RE
                .captures(&combined_raw)
                .or_else(|| MAINTAINED_BY_PREFIX_RE.captures(&combined_raw))
                .and_then(|cap| cap.name("who").map(|m| m.as_str().trim()))
                .unwrap_or(combined_raw.as_str())
                .trim_end_matches('.')
                .trim();
            if let Some(combined) = refine_author(combined_candidate)
                && seen.insert(combined.clone())
            {
                authors.retain(|a| a.start_line.get() < start_line || a.end_line.get() > end_line);
                authors.push(AuthorDetection {
                    author: combined,
                    start_line: LineNumber::new(start_line).expect("valid"),
                    end_line: LineNumber::new(end_line).expect("valid"),
                });
            }
        }

        i = j;
    }
}

pub(super) fn drop_merged_dash_bullet_attribution_authors(authors: &mut Vec<AuthorDetection>) {
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

pub(super) fn extract_json_excerpt_developed_by_authors(
    content: &str,
    authors: &mut Vec<AuthorDetection>,
) {
    if content.is_empty() {
        return;
    }

    static JSON_DEVELOPED_BY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?is)"(?:excerpt|description)"\s*:\s*"[^"\n]{0,800}?\bdeveloped\s+by\s+(?P<who>[A-Z][A-Za-z0-9.&+'-]*(?:\s+[A-Z][A-Za-z0-9.&+'-]*){0,4})(?:[.,;]|\")"#,
        )
        .unwrap()
    });

    let mut seen: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();
    for cap in JSON_DEVELOPED_BY_RE.captures_iter(content) {
        let who = cap
            .name("who")
            .map(|m| m.as_str())
            .unwrap_or("")
            .trim()
            .trim_end_matches(&['.', ';', ','][..]);
        if who.is_empty() {
            continue;
        }
        let Some(author) = refine_author(who) else {
            continue;
        };
        if seen.insert(author.clone()) {
            authors.push(AuthorDetection {
                author,
                start_line: LineNumber::ONE,
                end_line: LineNumber::ONE,
            });
        }
    }
}

pub(super) fn extract_modified_portion_developed_by_authors(
    content: &str,
    authors: &mut Vec<AuthorDetection>,
) {
    if content.is_empty() {
        return;
    }

    static MODIFIED_PORTION_DEVELOPED_BY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?ims)^[^\n]*modified\s+portion[^\n]*developed\s+by\s+(?P<who>[A-Z][A-Za-z0-9.&+'-]*(?:\s+[A-Z][A-Za-z0-9.&+'-]*){0,4})\.\s*(?:\r?\n\s*(?:#|//|/\*+|\*|--)?\s*\((?P<url>https?://[^)\s]+)\)\.?)?"#,
        )
        .unwrap()
    });

    let mut seen: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();

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

        if seen.insert(author.clone()) {
            let start_line = line_number_for_offset(content, full.start());
            let end_line = line_number_for_offset(content, full.end());
            authors.push(AuthorDetection {
                author,
                start_line,
                end_line,
            });
        }
    }
}

pub(super) fn extract_module_author_macros(
    content: &str,
    copyrights: &[CopyrightDetection],
    holders: &[HolderDetection],
    authors: &mut Vec<AuthorDetection>,
) {
    if content.is_empty() {
        return;
    }
    if !copyrights.is_empty() || !holders.is_empty() || !authors.is_empty() {
        return;
    }

    static MODULE_AUTHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?i)MODULE_AUTHOR\s*\(\s*\"(?P<who>[^\"]+)\"\s*\)"#).unwrap()
    });

    let mut seen: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();
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
            if seen.insert(author.clone()) {
                authors.push(AuthorDetection {
                    author,
                    start_line: LineNumber::new(ln).expect("invalid line number"),
                    end_line: LineNumber::new(ln).expect("invalid line number"),
                });
            }
        }
    }
}

pub(super) fn extract_was_developed_by_author_blocks(
    prepared_cache: &mut PreparedLineCache<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static WAS_DEVELOPED_BY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\bwas\s+developed\s+by\s+(?P<who>.+)$").unwrap());
    static WITH_PARTICIPATION_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\bwith\s+participation\b").unwrap());

    let mut seen: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();

    let mut i = 0;
    while i < prepared_cache.len() {
        let ln = i + 1;
        let Some(prepared) = prepared_cache.get_by_index(i) else {
            i += 1;
            continue;
        };
        let line = prepared.trim();
        if line.is_empty() {
            i += 1;
            continue;
        }

        let Some(cap) = WAS_DEVELOPED_BY_RE.captures(line) else {
            i += 1;
            continue;
        };
        let mut parts: Vec<String> = Vec::new();
        let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
        if who.is_empty() {
            i += 1;
            continue;
        }
        parts.push(who.to_string());

        let mut end_ln = ln;
        let mut j = i + 1;
        while j < prepared_cache.len() {
            let next_ln = j + 1;
            let Some(next_prepared) = prepared_cache.get_by_index(j) else {
                break;
            };
            let next_line = next_prepared.trim();
            if next_line.is_empty() {
                break;
            }

            let next_lower = next_line.to_ascii_lowercase();
            if next_lower.starts_with("copyright") {
                break;
            }

            if let Some(m) = WITH_PARTICIPATION_RE.find(next_line) {
                let prefix = next_line[..m.start()].trim_end();
                if !prefix.is_empty() {
                    parts.push(prefix.to_string());
                    end_ln = next_ln;
                }
                break;
            }

            parts.push(next_line.to_string());
            end_ln = next_ln;

            if end_ln.saturating_sub(ln) >= 3 {
                break;
            }

            j += 1;
        }

        let joined = parts.join(" ");
        let joined = joined.split_whitespace().collect::<Vec<_>>().join(" ");
        if joined.is_empty() {
            i += 1;
            continue;
        }

        let author = refine_author(&joined).unwrap_or(joined);
        if author.is_empty() {
            i += 1;
            continue;
        }

        if seen.insert(author.clone()) {
            authors.push(AuthorDetection {
                author,
                start_line: LineNumber::new(ln).expect("invalid line number"),
                end_line: LineNumber::new(end_ln).expect("invalid line number"),
            });
        }

        i += 1;
    }
}

pub(super) fn extract_author_colon_blocks(
    prepared_cache: &mut PreparedLineCache<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static AUTHOR_COLON_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^author(?:s|\(s\)|s\(s\))?\s*:\s*(?P<tail>.*)$").unwrap()
    });
    static YEAR_ONLY_COPY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^copyright\s+\(c\)\s*(?:\d{4}(?:\s*,\s*\d{4})*|\d{4}-\d{4})\s*$").unwrap()
    });

    let mut seen: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();

    let mut i = 0;
    while i < prepared_cache.len() {
        let ln = i + 1;
        let Some(line) = prepared_cache
            .get_by_index(i)
            .map(|p| p.trim().trim_start_matches('*').trim_start().to_string())
        else {
            i += 1;
            continue;
        };
        if line.is_empty() {
            i += 1;
            continue;
        }

        let Some(cap) = AUTHOR_COLON_RE.captures(&line) else {
            i += 1;
            continue;
        };

        let mut skip = false;
        let mut prev_idx = i;
        while prev_idx > 0 {
            prev_idx -= 1;
            let Some(prev) = prepared_cache
                .get_by_index(prev_idx)
                .map(|p| p.trim().to_string())
            else {
                break;
            };
            if prev.is_empty() {
                continue;
            }
            if YEAR_ONLY_COPY_RE.is_match(&prev) {
                skip = true;
            }
            break;
        }
        if skip {
            i += 1;
            continue;
        }

        let tail = cap.name("tail").map(|m| m.as_str()).unwrap_or("").trim();

        let label_raw = line.split(':').next().unwrap_or("").trim();
        let label_is_all_caps = !label_raw.is_empty()
            && label_raw.chars().any(|c| c.is_ascii_uppercase())
            && !label_raw.chars().any(|c| c.is_ascii_lowercase());
        if label_is_all_caps {
            i += 1;
            continue;
        }

        let mut segments: Vec<String> = Vec::new();
        if !tail.is_empty() {
            let Some(initial_tail) = sanitize_author_colon_tail(tail) else {
                i += 1;
                continue;
            };
            segments.push(initial_tail);
        }
        let mut j = i + 1;
        let mut added = 0usize;
        while j < prepared_cache.len() {
            let Some(next_prepared) = prepared_cache.get_by_index(j) else {
                break;
            };
            let next_line = next_prepared.trim().trim_start_matches('*').trim_start();
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

            let mut include = false;
            if next_line.contains(':') {
                if next_lower.starts_with("devices")
                    || next_lower.starts_with("status")
                    || next_lower.starts_with("return")
                {
                    include = true;
                } else {
                    break;
                }
            }
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
                j += 1;
                if added >= 4 {
                    break;
                }
                let combined_len: usize = segments.iter().map(|s| s.len()).sum();
                if combined_len > 320 {
                    break;
                }
                if next_lower.starts_with("return") {
                    break;
                }
                if next_lower.starts_with("devices") {
                    let tail = next_line
                        .split_once(':')
                        .map(|(_, t)| t.trim())
                        .unwrap_or("");
                    if !tail.is_empty() {
                        break;
                    }
                }
                continue;
            }
            break;
        }

        if segments.is_empty() {
            i += 1;
            continue;
        }

        let start_line = ln;
        let end_line = if j == i + 1 { start_line } else { j };
        if extract_author_colon_bullet_roster(&segments, start_line, authors, &mut seen) {
            i = j;
            continue;
        }
        let combined_raw = segments.join(" ");
        let Some(combined) = refine_author(&combined_raw) else {
            i += 1;
            continue;
        };

        if seen.insert(combined.clone()) {
            authors.retain(|a| a.start_line.get() < start_line || a.end_line.get() > end_line);
            authors.push(AuthorDetection {
                author: combined,
                start_line: LineNumber::new(start_line).expect("valid"),
                end_line: LineNumber::new(end_line).expect("valid"),
            });
        }

        i = j;
    }
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
            r#"(?i),(?:\s*['"]?(?:url|version|wiki|gav|labels|developerid|email|name|previoustimestamp|previousversion|releasetimestamp|requiredcore|scm|title|builddate|dependencies|sha1)\b.*)$"#,
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

pub(super) fn extract_code_written_by_author_blocks(
    prepared_cache: &mut PreparedLineCache<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static HEADER_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\bcode\s+written\s+by\b").unwrap());
    static BODY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)\bwritten\s+by\s+(?P<body>.+)$").unwrap());
    static STOP_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?is)(?P<prefix>.+?\bDonald\s+wrote\s+the\s+SMC\s+91c92\s+code)\b").unwrap()
    });

    let mut seen: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();

    let mut i = 0;
    while i < prepared_cache.len() {
        let ln = i + 1;
        let Some(prepared) = prepared_cache.get_by_index(i) else {
            i += 1;
            continue;
        };
        let line = prepared.trim();
        if line.is_empty() {
            i += 1;
            continue;
        }
        if !HEADER_RE.is_match(line) {
            i += 1;
            continue;
        }

        let mut combined = line.to_string();
        let mut j = i + 1;
        while j < prepared_cache.len() {
            let Some(next_prepared) = prepared_cache.get_by_index(j) else {
                break;
            };
            let next = next_prepared.trim();
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
            j += 1;
        }

        let Some(cap) = BODY_RE.captures(&combined) else {
            i = j;
            continue;
        };
        let body = cap.name("body").map(|m| m.as_str()).unwrap_or("").trim();
        if body.is_empty() {
            i = j;
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
            i = j;
            continue;
        };
        if seen.insert(author.clone()) {
            authors.push(AuthorDetection {
                author,
                start_line: LineNumber::new(ln).expect("invalid line number"),
                end_line: LineNumber::new(j).expect("invalid line number"),
            });
        }

        i = j;
    }
}

pub(super) fn extract_developed_and_created_by_authors(
    prepared_cache: &mut PreparedLineCache<'_>,
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

    let mut seen: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();

    for start_idx in 0..prepared_cache.len() {
        let Some(prepared0) = prepared_cache.get_by_index(start_idx) else {
            continue;
        };
        if !PREFIX_RE.is_match(prepared0.trim()) {
            continue;
        }

        let mut parts: Vec<String> = Vec::new();
        let mut end_idx = start_idx;

        for idx in start_idx..prepared_cache.len() {
            let Some(prepared) = prepared_cache.get_by_index(idx) else {
                break;
            };
            let line = prepared.trim();
            if line.is_empty() {
                break;
            }
            if line.to_ascii_lowercase().contains("http") {
                break;
            }

            let piece = if idx == start_idx {
                PREFIX_RE.replace(line, "").to_string()
            } else {
                line.to_string()
            };
            if !piece.trim().is_empty() {
                parts.push(piece);
            }
            end_idx = idx;
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
        if seen.insert(author.clone()) {
            authors.push(AuthorDetection {
                author: author.clone(),
                start_line: LineNumber::from_0_indexed(start_idx),
                end_line: LineNumber::from_0_indexed(end_idx),
            });
        }

        authors.retain(|a| !(author.starts_with(&a.author) && a.author.len() < author.len()));
    }
}

pub(super) fn extract_with_additional_hacking_by_authors(
    prepared_cache: &mut PreparedLineCache<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^\s*with\s+additional\s+hacking\s+by\s+(?P<who>.+?)\s*$").unwrap()
    });

    let mut seen: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();

    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(prepared) = prepared_cache.get_by_index(idx) else {
            continue;
        };
        let line = prepared.trim();
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
        if let Some(author) = refine_author(who)
            && seen.insert(author.clone())
        {
            authors.push(AuthorDetection {
                author,
                start_line: LineNumber::new(ln).expect("invalid line number"),
                end_line: LineNumber::new(ln).expect("invalid line number"),
            });
        }
    }
}

pub(super) fn merge_metadata_author_and_email_lines(
    prepared_cache: &mut PreparedLineCache<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    let has_metadata = (0..prepared_cache.len()).any(|idx| {
        prepared_cache
            .get_by_index(idx)
            .is_some_and(|l| l.trim_start().starts_with("Metadata-Version:"))
    });

    if !has_metadata {
        return;
    }

    let mut seen: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();

    for idx in 0..prepared_cache.len() {
        let author_ln = idx + 1;
        let Some(author_line) = prepared_cache
            .get_by_index(idx)
            .map(|p| p.trim().to_string())
        else {
            continue;
        };
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

        for j in (idx + 1)..prepared_cache.len() {
            let email_ln = j + 1;
            let Some(email_line) = prepared_cache.get_by_index(j).map(|p| p.trim().to_string())
            else {
                break;
            };
            if email_line.is_empty() {
                break;
            }
            if email_line.to_ascii_lowercase().starts_with("author:") {
                break;
            }

            if !email_line.to_ascii_lowercase().starts_with("author-email") {
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

            if seen.insert(combined.clone()) {
                authors.push(AuthorDetection {
                    author: combined,
                    start_line: LineNumber::new(author_ln).expect("invalid line number"),
                    end_line: LineNumber::new(email_ln).expect("invalid line number"),
                });
            }

            authors.retain(|a| {
                if a.start_line.get() == author_ln
                    && a.end_line.get() == author_ln
                    && a.author == name
                {
                    return false;
                }
                if a.start_line.get() == email_ln
                    && a.end_line.get() == email_ln
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

pub(super) fn extract_debian_maintainer_authors(
    prepared_cache: &mut PreparedLineCache<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() {
        return;
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

    let mut seen: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();

    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(prepared) = prepared_cache.get_by_index(idx) else {
            continue;
        };
        let line = prepared.trim();
        if line.is_empty() {
            continue;
        }

        let who_raw = if let Some(cap) = CO_MAINTAINER_RE.captures(line) {
            cap.name("who").map(|m| m.as_str()).unwrap_or("")
        } else if let Some(cap) = DEBIANIZED_BY_RE.captures(line) {
            cap.name("who").map(|m| m.as_str()).unwrap_or("")
        } else if let Some(cap) = MAINTAINED_BY_RE.captures(line) {
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

        if seen.insert(author.clone()) {
            authors.push(AuthorDetection {
                author,
                start_line: LineNumber::new(ln).expect("invalid line number"),
                end_line: LineNumber::new(ln).expect("invalid line number"),
            });
        }
    }
}

pub(super) fn extract_created_by_project_author(
    prepared_cache: &mut PreparedLineCache<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static CREATED_BY_PROJECT_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\bcreated\s+by\s+the\s+project\b").unwrap());

    let mut seen: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();

    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(prepared) = prepared_cache.get_by_index(idx) else {
            continue;
        };
        if CREATED_BY_PROJECT_RE.is_match(prepared.trim()) {
            let author = "the Project".to_string();
            if seen.insert(author.clone()) {
                authors.push(AuthorDetection {
                    author,
                    start_line: LineNumber::new(ln).expect("invalid line number"),
                    end_line: LineNumber::new(ln).expect("invalid line number"),
                });
            }
            break;
        }
    }
}

pub(super) fn extract_created_by_authors(
    prepared_cache: &mut PreparedLineCache<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static CREATED_BY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^\s*created\s+by\s+(?P<who>.+?)\s*$").unwrap());

    let mut seen: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();

    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(prepared) = prepared_cache.get_by_index(idx) else {
            continue;
        };
        let line = prepared.trim();
        if line.is_empty() {
            continue;
        }

        let Some(cap) = CREATED_BY_RE.captures(line) else {
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

        let Some(author) = refine_author(who) else {
            continue;
        };
        if seen.insert(author.clone()) {
            authors.push(AuthorDetection {
                author: author.clone(),
                start_line: LineNumber::new(ln).expect("invalid line number"),
                end_line: LineNumber::new(ln).expect("invalid line number"),
            });
        }

        authors.retain(|a| !(author.starts_with(&a.author) && a.author.len() < author.len()));
    }
}

pub(super) fn extract_written_by_comma_and_copyright_authors(
    prepared_cache: &mut PreparedLineCache<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static WRITTEN_BY_AND_COPYRIGHT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bwritten\s+by\s+(?P<who>.+?),\s+and\s+copyright\b").unwrap()
    });

    let mut seen: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();

    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(prepared) = prepared_cache.get_by_index(idx) else {
            continue;
        };
        let line = prepared.trim();
        if line.is_empty() {
            continue;
        }

        let Some(cap) = WRITTEN_BY_AND_COPYRIGHT_RE.captures(line) else {
            continue;
        };
        let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
        if who.is_empty() {
            continue;
        }
        let Some(author) = refine_author(who) else {
            continue;
        };
        if seen.insert(author.clone()) {
            authors.retain(|a| !(a.start_line.get() == ln && a.end_line.get() == ln));
            authors.push(AuthorDetection {
                author,
                start_line: LineNumber::new(ln).expect("invalid line number"),
                end_line: LineNumber::new(ln).expect("invalid line number"),
            });
        }
    }
}

pub(super) fn extract_package_comment_named_authors(
    prepared_cache: &mut PreparedLineCache<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static COMMENT_AUTHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)\b(?:was originally written by|it is now maintained by|this package is maintained for debian by)\s+(?P<who>.+?)(?:\.(?:\s|$)|$)",
        )
        .unwrap()
    });
    static RAW_ANGLE_EMAIL_AUTHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<name>[A-Z][^<>@]+?)\s*<(?P<email>[^>\s]+@[^>\s]+)>$").unwrap()
    });

    let mut seen: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();

    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(prepared) = prepared_cache.get_by_index(idx) else {
            continue;
        };
        let line = prepared.trim();
        if line.is_empty() {
            continue;
        }

        for cap in COMMENT_AUTHOR_RE.captures_iter(line) {
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

            if let Some(author) = author
                && seen.insert(author.clone())
            {
                authors.push(AuthorDetection {
                    author,
                    start_line: LineNumber::new(ln).expect("invalid line number"),
                    end_line: LineNumber::new(ln).expect("invalid line number"),
                });
            }
        }
    }
}

pub(super) fn extract_developed_by_sentence_authors(
    prepared_cache: &mut PreparedLineCache<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static DEVELOPED_BY_PREFIX_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^\s*developed\s+by\s+(?P<rest>.+)$").unwrap());

    let mut seen: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();

    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(prepared) = prepared_cache.get_by_index(idx) else {
            continue;
        };
        let line = prepared.trim();
        if line.is_empty() {
            continue;
        }

        let Some(cap) = DEVELOPED_BY_PREFIX_RE.captures(line) else {
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

        if seen.insert(author.clone()) {
            authors.push(AuthorDetection {
                author,
                start_line: LineNumber::new(ln).expect("invalid line number"),
                end_line: LineNumber::new(ln).expect("invalid line number"),
            });
        }
    }
}

pub(super) fn extract_developed_by_phrase_authors(
    prepared_cache: &mut PreparedLineCache<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static DEVELOPED_BY_PHRASE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bdeveloped\s+by\s+(?P<who>.+?)\s+and\s+to\s+credit\b").unwrap()
    });

    let mut seen: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();

    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(prepared) = prepared_cache.get_by_index(idx) else {
            continue;
        };
        let line = prepared.trim();
        if line.is_empty() {
            continue;
        }

        for cap in DEVELOPED_BY_PHRASE_RE.captures_iter(line) {
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

            if seen.insert(author.clone()) {
                authors.push(AuthorDetection {
                    author,
                    start_line: LineNumber::new(ln).expect("invalid line number"),
                    end_line: LineNumber::new(ln).expect("invalid line number"),
                });
            }
        }
    }
}

pub(super) fn extract_maintained_by_authors(
    prepared_cache: &mut PreparedLineCache<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static MAINTAINED_BY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bmaintained\s+by\s+(?P<who>.+?)(?:\s+(?:on|since|for)\b|$)").unwrap()
    });

    let mut seen: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();

    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(prepared) = prepared_cache.get_by_index(idx) else {
            continue;
        };
        let line = prepared.trim();
        if line.is_empty() {
            continue;
        }
        for cap in MAINTAINED_BY_RE.captures_iter(line) {
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
            if seen.insert(author.clone()) {
                authors.push(AuthorDetection {
                    author,
                    start_line: LineNumber::new(ln).expect("invalid line number"),
                    end_line: LineNumber::new(ln).expect("invalid line number"),
                });
            }
        }
    }
}

pub(super) fn extract_converted_to_by_authors(
    prepared_cache: &mut PreparedLineCache<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static CONVERTED_BY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^\s*converted\b.*\bby\s+(?P<who>.+)$").unwrap());
    static CONVERTED_TO_THE_BY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^\s*converted\s+to\s+the\b.*\bby\s+(?P<who>.+)$").unwrap()
    });
    static CONVERTED_TO_VERSION_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\bconverted\s+to\s+\d+\.\d+\b").unwrap());

    let mut seen: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();

    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(prepared) = prepared_cache.get_by_index(idx) else {
            continue;
        };
        let line = prepared.trim().trim_start_matches('*').trim_start();
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
        if seen.insert(author.clone()) {
            authors.push(AuthorDetection {
                author: author.clone(),
                start_line: LineNumber::new(ln).expect("invalid line number"),
                end_line: LineNumber::new(ln).expect("invalid line number"),
            });
        }
        if add_converted_variant {
            let converted = format!("{author} Converted");
            if seen.insert(converted.clone()) {
                authors.push(AuthorDetection {
                    author: converted,
                    start_line: LineNumber::new(ln).expect("invalid line number"),
                    end_line: LineNumber::new(ln).expect("invalid line number"),
                });
            }
        }
    }
}

pub(super) fn extract_various_bugfixes_and_enhancements_by_authors(
    prepared_cache: &mut PreparedLineCache<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static VARIOUS_BUGFIXES_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^\s*various\s+bugfixes\s+and\s+enhancements\s+by\s+(?P<who>.+)$").unwrap()
    });

    let mut seen: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();

    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(prepared) = prepared_cache.get_by_index(idx) else {
            continue;
        };
        let line = prepared.trim().trim_start_matches('*').trim_start();
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
        if seen.insert(author.clone()) {
            authors.push(AuthorDetection {
                author,
                start_line: LineNumber::new(ln).expect("invalid line number"),
                end_line: LineNumber::new(ln).expect("invalid line number"),
            });
        }
    }
}

pub(super) fn drop_authors_embedded_in_copyrights(
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

pub(super) fn drop_author_colon_lines_absorbed_into_year_only_copyrights(
    prepared_cache: &PreparedLineCache<'_>,
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
        let start_line = author.start_line.get();
        let end_line = author.end_line.get();
        if start_line != end_line || start_line <= 1 {
            return true;
        }

        let Some(raw_line) = prepared_cache.raw_by_index(start_line - 1) else {
            return true;
        };
        let normalized_line = raw_line.trim().trim_start_matches('*').trim_start();
        if !AUTHOR_LINE_RE.is_match(normalized_line) {
            return true;
        }

        copyrights.iter().all(|copyright| {
            if copyright.start_line.get() != start_line - 1
                || copyright.end_line.get() != start_line
            {
                return true;
            }
            !YEAR_ONLY_COPY_RE.is_match(copyright.copyright.as_str())
        })
    });
}

pub(super) fn drop_shadowed_prefix_authors(authors: &mut Vec<AuthorDetection>) {
    if authors.len() < 2 {
        return;
    }
    let mut drop: Vec<bool> = vec![false; authors.len()];
    for i in 0..authors.len() {
        let a = authors[i].author.trim();
        if a.is_empty() {
            continue;
        }
        for (j, other) in authors.iter().enumerate() {
            if i == j {
                continue;
            }
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
                    drop[i] = true;
                    break;
                }
                if !a_has_email && b_has_email && boundary {
                    drop[i] = true;
                    break;
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
                        drop[i] = true;
                        break;
                    }
                }
            }
        }
    }
    if drop.iter().all(|d| !*d) {
        return;
    }
    let mut kept = Vec::with_capacity(authors.len());
    for (i, a) in authors.iter().cloned().enumerate() {
        if !drop[i] {
            kept.push(a);
        }
    }
    *authors = kept;
}

pub(super) fn drop_ref_markup_authors(authors: &mut Vec<AuthorDetection>) {
    authors.retain(|author| !author.author.contains("@ref"));
}

pub(super) fn normalize_json_blob_authors(raw_lines: &[&str], authors: &mut Vec<AuthorDetection>) {
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

        let replacement = if let Some(name) = extract_author_name_from_json_window(&window) {
            refine_author(&name)
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

fn extract_author_name_from_json_window(window: &str) -> Option<String> {
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

pub(super) fn drop_comedi_ds_status_devices_authors(
    content: &str,
    copyrights: &[CopyrightDetection],
    authors: &mut Vec<AuthorDetection>,
) {
    if authors.is_empty() {
        return;
    }

    let lower = content.to_ascii_lowercase();
    if !lower.contains("author") || !lower.contains("status") {
        return;
    }
    if !content.lines().any(|l| l.contains("Author: ds")) {
        return;
    }

    let has_any_copyright = !copyrights.is_empty();
    let drop_for_national_instruments = lower.contains("national instruments");

    authors.retain(|a| {
        let s = a.author.trim();
        if !s.to_ascii_lowercase().starts_with("ds status") {
            return true;
        }
        if !has_any_copyright {
            return false;
        }
        if drop_for_national_instruments {
            return false;
        }
        true
    });
}

pub(super) fn drop_written_by_authors_preceded_by_copyright(
    prepared_cache: &mut PreparedLineCache<'_>,
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
    for i in 1..prepared_cache.len() {
        let Some(line) = prepared_cache.get_by_index(i).map(|p| p.trim().to_string()) else {
            continue;
        };
        let Some(cap) = WRITTEN_BY_RE.captures(&line) else {
            continue;
        };
        let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
        if who.is_empty() {
            continue;
        }
        let Some(prev) = prepared_cache
            .get_by_index(i - 1)
            .map(|p| p.trim().to_string())
        else {
            continue;
        };
        if !COPYRIGHT_HINT_RE.is_match(&prev) {
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

pub(super) fn extract_dense_name_email_author_lists(
    prepared_cache: &mut PreparedLineCache<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static NAME_EMAIL_LINE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<name>[^<\n]{2,120})\s*<(?P<email>[^>\s]+@[^>\s]+)>\s*$").unwrap()
    });

    let mut non_empty_lines: Vec<(usize, String)> = Vec::new();
    for idx in 0..prepared_cache.len() {
        let Some(prepared) = prepared_cache.get_by_index(idx) else {
            continue;
        };
        let line = prepared.trim();
        if line.is_empty() {
            continue;
        }
        non_empty_lines.push((idx + 1, line.to_string()));
    }
    if non_empty_lines.len() < 2 {
        return;
    }

    let mut matched: Vec<(usize, String)> = Vec::new();
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
        return;
    }
    if matched.len() * 2 < non_empty_lines.len() {
        return;
    }

    let mut seen: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();
    for (ln, candidate) in matched {
        let Some(author) = refine_author(&candidate) else {
            continue;
        };
        if seen.insert(author.clone()) {
            authors.push(AuthorDetection {
                author,
                start_line: LineNumber::new(ln).expect("invalid line number"),
                end_line: LineNumber::new(ln).expect("invalid line number"),
            });
        }
    }
}
