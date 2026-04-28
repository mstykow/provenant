// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;
use std::time::Instant;

use regex::Regex;

use crate::copyright::line_tracking::{LineNumberIndex, PreparedLines};
use crate::copyright::refiner::{
    refine_author, refine_copyright, refine_holder, refine_holder_in_copyright_context,
};
use crate::copyright::types::{AuthorDetection, CopyrightDetection, HolderDetection};
use crate::models::LineNumber;

use super::author_heuristics::{
    looks_like_structured_json_author_fallback, refine_author_with_optional_handle_suffix,
};
use super::seen_text::SeenTextSets;
use super::token_utils::{group_by, normalize_whitespace};

mod author_repairs;
mod case_specific;
mod dedupe_and_shadow;
mod email_url_repairs;
mod metadata_repairs;
mod multiline_repairs;
mod year_repairs;

pub use author_repairs::*;
pub use case_specific::*;
pub use dedupe_and_shadow::*;
pub use email_url_repairs::*;
pub use metadata_repairs::*;
pub use multiline_repairs::*;
pub use year_repairs::*;

#[cfg(test)]
#[path = "../postprocess_transforms_test.rs"]
mod tests;

pub fn refine_final_copyrights(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.is_empty() {
        return;
    }

    *copyrights = copyrights
        .iter()
        .filter_map(|c| {
            let text = refine_copyright(&c.copyright)?;
            Some(CopyrightDetection {
                copyright: text,
                start_line: c.start_line,
                end_line: c.end_line,
            })
        })
        .collect();
}

pub fn refine_final_authors(authors: &mut Vec<AuthorDetection>) {
    if authors.is_empty() {
        return;
    }

    authors.retain(|author| !contains_markdown_link_author_fragment(&author.author));

    *authors = authors
        .iter()
        .filter_map(|a| {
            let author = if let Some(author) = refine_author(&a.author) {
                author
            } else if refine_author_with_optional_handle_suffix(&a.author).is_some()
                || looks_like_structured_json_author_fallback(&a.author)
                || looks_like_collective_institution_author(&a.author)
            {
                a.author.trim().to_string()
            } else {
                return None;
            };
            Some(AuthorDetection {
                author,
                start_line: a.start_line,
                end_line: a.end_line,
            })
        })
        .collect();
}

fn contains_markdown_link_author_fragment(author: &str) -> bool {
    let trimmed = author.trim();
    trimmed.contains("](http")
        || trimmed.contains("](https://")
        || trimmed.contains("] (http")
        || trimmed.contains("] (https://")
}

fn looks_like_collective_institution_author(author: &str) -> bool {
    let trimmed = author.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("the ") {
        return false;
    }

    let uppercase_word_count = trimmed
        .split_whitespace()
        .filter(|word| {
            word.chars()
                .find(|ch| ch.is_alphabetic())
                .is_some_and(|ch| ch.is_uppercase())
        })
        .count();

    uppercase_word_count >= 2
        && (lower.contains(" at the ") || lower.contains(" of the ") || trimmed.contains(". "))
}

fn is_trademark_boilerplate_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    (lower.contains("trademark") && lower.contains("logo"))
        || lower.contains(" the apache tomcat logo")
        || lower.contains(" the apache logo")
        || lower.contains(" logo and the ")
        || lower.contains("registered trademarks or trademarks")
}

pub fn deadline_exceeded(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|d| Instant::now() >= d)
}

pub fn add_found_at_short_variants(
    copyrights: &[CopyrightDetection],
    _holders: &[HolderDetection],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    if copyrights.is_empty() {
        return (Vec::new(), Vec::new());
    }

    static FOUND_AT_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^\(c\)\s+by\s+(?P<name>.+?)\s+found\s+at\b").unwrap());

    copyrights
        .iter()
        .filter_map(|c| {
            let cap = FOUND_AT_RE.captures(c.copyright.trim())?;
            let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
            (!name.is_empty()).then_some((
                CopyrightDetection {
                    copyright: format!("(c) by {name}"),
                    start_line: c.start_line,
                    end_line: c.end_line,
                },
                HolderDetection {
                    holder: name.to_string(),
                    start_line: c.start_line,
                    end_line: c.end_line,
                },
            ))
        })
        .unzip()
}
