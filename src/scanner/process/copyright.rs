// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::binary_text::{
    extract_named_author_from_binary_line, has_binary_name_like_shape, has_excessive_at_noise,
    has_sufficient_alphabetic_content, is_binary_string_author_candidate, is_company_like_suffix,
};
use crate::copyright::{self, AuthorDetection, CopyrightDetection, HolderDetection};
use crate::models::{Author, Copyright, FileInfoBuilder, Holder, LineNumber};
use regex::Regex;
use std::collections::HashSet;
use std::path::Path;
use std::sync::LazyLock;
use std::time::Duration;

pub(super) fn extract_copyright_information(
    file_info_builder: &mut FileInfoBuilder,
    path: &Path,
    text_content: &str,
    timeout_seconds: f64,
    from_binary_strings: bool,
) {
    if copyright::is_credits_file(path) {
        let author_detections = copyright::detect_credits_authors(text_content);
        if !author_detections.is_empty() {
            file_info_builder.authors(
                author_detections
                    .into_iter()
                    .map(|a| Author {
                        author: a.author,
                        start_line: a.start_line,
                        end_line: a.end_line,
                    })
                    .collect(),
            );
            return;
        }
    }

    let max_runtime = if timeout_seconds.is_finite() && timeout_seconds > 0.0 {
        Some(Duration::from_secs_f64(timeout_seconds))
    } else {
        None
    };

    let (copyrights, holders, authors) = copyright::detect_copyrights(text_content, max_runtime);
    let (copyrights, holders, authors) = if from_binary_strings {
        prune_binary_string_detections(text_content, copyrights, holders, authors)
    } else {
        (copyrights, holders, authors)
    };

    file_info_builder.copyrights(
        copyrights
            .into_iter()
            .map(|c| Copyright {
                copyright: c.copyright,
                start_line: c.start_line,
                end_line: c.end_line,
            })
            .collect::<Vec<Copyright>>(),
    );
    file_info_builder.holders(
        holders
            .into_iter()
            .map(|h| Holder {
                holder: h.holder,
                start_line: h.start_line,
                end_line: h.end_line,
            })
            .collect::<Vec<Holder>>(),
    );
    let mut authors = authors;
    authors.extend(extract_patch_header_author_supplements(text_content));
    authors.extend(extract_comment_author_supplements(text_content));
    let mut seen_authors = HashSet::new();
    authors.retain(|author| {
        seen_authors.insert((author.author.clone(), author.start_line, author.end_line))
    });

    file_info_builder.authors(
        authors
            .into_iter()
            .map(|a| Author {
                author: a.author,
                start_line: a.start_line,
                end_line: a.end_line,
            })
            .collect::<Vec<Author>>(),
    );
}

fn prune_binary_string_detections(
    text_content: &str,
    copyrights: Vec<CopyrightDetection>,
    holders: Vec<HolderDetection>,
    authors: Vec<AuthorDetection>,
) -> (
    Vec<CopyrightDetection>,
    Vec<HolderDetection>,
    Vec<AuthorDetection>,
) {
    let kept_copyrights: Vec<CopyrightDetection> = copyrights
        .into_iter()
        .filter(|c| is_binary_string_copyright_candidate(&c.copyright))
        .collect();

    let kept_holders: Vec<HolderDetection> = holders
        .into_iter()
        .filter(|holder| {
            kept_copyrights.iter().any(|copyright| {
                ranges_overlap(
                    holder.start_line,
                    holder.end_line,
                    copyright.start_line,
                    copyright.end_line,
                )
            })
        })
        .collect();

    let kept_authors = authors
        .into_iter()
        .filter(|author| is_binary_string_author_candidate(&author.author))
        .chain(extract_binary_string_author_supplements(text_content))
        .filter({
            let mut seen = HashSet::new();
            move |author| seen.insert(author.author.clone())
        })
        .collect();

    (kept_copyrights, kept_holders, kept_authors)
}

fn ranges_overlap(
    a_start: LineNumber,
    a_end: LineNumber,
    b_start: LineNumber,
    b_end: LineNumber,
) -> bool {
    a_start <= b_end && b_start <= a_end
}

fn is_binary_string_copyright_candidate(text: &str) -> bool {
    if contains_year(text) {
        return true;
    }

    let trimmed = text.trim();
    let lower = trimmed.to_ascii_lowercase();
    let tail = if let Some(tail) = lower.strip_prefix("copyright") {
        tail.trim()
    } else {
        lower.trim()
    };
    let original_tail = if lower.starts_with("copyright") {
        trimmed["copyright".len()..].trim()
    } else {
        trimmed
    };

    if tail.is_empty() || !has_sufficient_alphabetic_content(tail) || has_excessive_at_noise(tail) {
        return false;
    }

    let alpha_tokens: Vec<&str> = tail
        .split_whitespace()
        .filter(|token| token.chars().any(|c| c.is_alphabetic()))
        .collect();

    if alpha_tokens.len() <= 1 {
        return has_explicit_copyright_marker(text)
            && alpha_tokens.iter().any(|token| {
                is_company_like_suffix(token.trim_matches(|c: char| !c.is_alphanumeric()))
            });
    }

    if !has_explicit_copyright_marker(text) {
        return false;
    }

    has_binary_name_like_shape(original_tail)
}

fn extract_binary_string_author_supplements(text_content: &str) -> Vec<AuthorDetection> {
    let mut authors = Vec::new();

    for (line_index, line) in text_content.lines().enumerate() {
        if let Some(author) = extract_named_author_from_binary_line(line) {
            authors.push(AuthorDetection {
                author,
                start_line: LineNumber::from_0_indexed(line_index),
                end_line: LineNumber::from_0_indexed(line_index),
            });
        }
    }

    authors
}

fn extract_patch_header_author_supplements(text_content: &str) -> Vec<AuthorDetection> {
    static PATCH_AUTHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?:from:|patch by|signed-off-by:|co-developed-by:|authored-by:)\s+(?P<author>[^<\n]+<[^>]+>)\s*$",
        )
        .expect("valid patch header author regex")
    });

    text_content
        .lines()
        .enumerate()
        .filter_map(|(line_index, line)| {
            let captures = PATCH_AUTHOR_RE.captures(line.trim())?;
            let author = captures.name("author")?.as_str().trim();
            Some(AuthorDetection {
                author: author.to_string(),
                start_line: LineNumber::from_0_indexed(line_index),
                end_line: LineNumber::from_0_indexed(line_index),
            })
        })
        .collect()
}

fn extract_comment_author_supplements(text_content: &str) -> Vec<AuthorDetection> {
    static COMMENT_AUTHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)\b(?:written|edited|modified|updated|originally)\s+by\s+(?P<author>[^<\n]+<[^>]+>)\s*\.?$|^(?:[#;/*!\-\s]+)?(?:[^<\n]*?\bby\s+(?P<author2>[^<\n]+<[^>]+>))\s*\.?$",
        )
        .expect("valid comment author regex")
    });
    static COMMENT_PAREN_CONTACT_AUTHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)\b(?:written|edited|modified|updated|originally)\s+by\s+(?P<name>[^()\n]+?)\s*\(\s*(?P<contact>(?:[^)\s]+@[^)\s]+|https?://[^)\s]+))\s*\)\s*\.?$|^(?:[#;/*!\-\s]+)?(?:[^()\n]*?\bby\s+(?P<name2>[^()\n]+?)\s*\(\s*(?P<contact2>(?:[^)\s]+@[^)\s]+|https?://[^)\s]+))\s*\))\s*\.?$",
        )
        .expect("valid parenthesized contact author regex")
    });
    static DOCKER_MAINTAINER_LABEL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?i)^label\s+maintainer\s*=\s*[\"']?(?P<author>[^\"'\n]+<[^>]+>)[\"']?\s*$"#)
            .expect("valid docker maintainer label regex")
    });
    static EMAIL_PAREN_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(?P<email>[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,63})\s*\((?P<name>[^)]+)\)")
            .expect("valid email paren name regex")
    });

    let mut authors = Vec::new();

    for (line_index, line) in text_content.lines().enumerate() {
        let trimmed = line.trim();
        let normalized = normalize_comment_author_line(trimmed);
        let line_number = LineNumber::from_0_indexed(line_index);

        if let Some(captures) = COMMENT_AUTHOR_RE.captures(&normalized)
            && let Some(author) = captures
                .name("author")
                .or_else(|| captures.name("author2"))
                .map(|m| m.as_str().trim())
        {
            authors.push(AuthorDetection {
                author: normalize_comment_author_candidate(author),
                start_line: line_number,
                end_line: line_number,
            });
        }

        if let Some(captures) = COMMENT_PAREN_CONTACT_AUTHOR_RE.captures(&normalized) {
            let name = captures
                .name("name")
                .or_else(|| captures.name("name2"))
                .map(|m| m.as_str().trim());
            let contact = captures
                .name("contact")
                .or_else(|| captures.name("contact2"))
                .map(|m| m.as_str().trim());

            if let (Some(name), Some(contact)) = (name, contact) {
                authors.push(AuthorDetection {
                    author: normalize_parenthesized_contact_author(name, contact),
                    start_line: line_number,
                    end_line: line_number,
                });
            }
        }

        if let Some(captures) = DOCKER_MAINTAINER_LABEL_RE.captures(trimmed)
            && let Some(author) = captures.name("author").map(|m| m.as_str().trim())
        {
            authors.push(AuthorDetection {
                author: author.to_string(),
                start_line: line_number,
                end_line: line_number,
            });
        }

        for captures in EMAIL_PAREN_NAME_RE.captures_iter(trimmed) {
            let Some(email) = captures.name("email").map(|m| m.as_str().trim()) else {
                continue;
            };
            let Some(name) = captures.name("name").map(|m| m.as_str().trim()) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            authors.push(AuthorDetection {
                author: format!("{name} <{email}>"),
                start_line: line_number,
                end_line: line_number,
            });
        }
    }

    authors
}

fn normalize_comment_author_line(line: &str) -> String {
    line.trim()
        .trim_end_matches("*/")
        .trim_end_matches("-->")
        .trim()
        .to_string()
}

fn normalize_comment_author_candidate(author: &str) -> String {
    static ANGLE_URL_AUTHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<name>[^<>]+?)\s*<\s*(?P<url>https?://[^>\s]+)\s*>\s*$")
            .expect("valid angle url author regex")
    });

    let trimmed = author.trim().trim_end_matches('.').trim();
    if let Some(captures) = ANGLE_URL_AUTHOR_RE.captures(trimmed) {
        let name = captures
            .name("name")
            .map(|m| m.as_str().trim())
            .unwrap_or(trimmed);
        let url = captures
            .name("url")
            .map(|m| m.as_str().trim_end_matches('/'))
            .unwrap_or(trimmed);
        return format!("{name} {url}");
    }

    trimmed.to_string()
}

fn normalize_parenthesized_contact_author(name: &str, contact: &str) -> String {
    let normalized_name = name.trim().trim_end_matches('.').trim();
    let normalized_contact = if contact.starts_with("http://") || contact.starts_with("https://") {
        contact.trim_end_matches('/')
    } else {
        contact.trim()
    };
    format!("{normalized_name} ({normalized_contact})")
}

fn has_explicit_copyright_marker(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("(c)") || lower.contains('©') || lower.contains("copr")
}

fn contains_year(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.windows(4).any(|window| {
        window.iter().all(|b| b.is_ascii_digit())
            && matches!(window[0], b'1' | b'2')
            && matches!(window[1], b'9' | b'0')
    })
}

#[cfg(test)]
#[path = "copyright_test.rs"]
mod tests;
