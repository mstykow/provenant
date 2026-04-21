// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Copyright hint markers and year detection.
//!
//! Provides functions to identify lines that may contain copyright information
//! based on marker strings, year patterns, and simple heuristics for filtering
//! out gibberish/binary content.

use std::sync::LazyLock;

use regex::Regex;

/// Markers that suggest a line may contain copyright information.
/// A line containing any of these (case-insensitive) is a candidate.
pub const HINT_MARKERS: &[&str] = &[
    "©",
    // C sign in Restructured Text:
    "|copy|",
    "&#169",
    "&#xa9",
    "169",
    "xa9",
    "u00a9",
    "00a9",
    "\u{00a9}", // © character
    "\\251",
    // have copyright but also (c)opyright and ©opyright
    "opyr",
    // have copyleft
    "opyl",
    "copr",
    "reserv",
    "auth",
    "contrib",
    "commit",
    "filecontributor",
    "devel",
    // Debian markup
    "<s>",
    "</s>",
    "<s/>",
    "by ", // note the trailing space
    // common for emails
    "@",
];

/// Regex matching years from 1960–2099 surrounded by punctuation/whitespace.
///
/// Improvement over Python: the original only covers up to the current year
/// (dynamically computed). We statically cover 1960–2099 for simplicity and
/// forward-compatibility.
static YEAR_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[\(\.,\-\)\s]+(19[6-9][0-9]|20[0-9]{2})([\(\.,\-\)\s]+|$)").unwrap()
});

/// Check if a line contains a copyright-relevant year (1960–2099).
pub fn has_year(line: &str) -> bool {
    YEAR_REGEX.is_match(line)
}

/// Check if a line contains any copyright hint marker (case-insensitive).
pub fn has_copyright_hint(line: &str) -> bool {
    let lower = line.to_lowercase();
    HINT_MARKERS.iter().any(|marker| lower.contains(marker)) || has_c_marker_hint(line)
}

fn has_c_marker_hint(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    for (idx, _) in lower.match_indices("(c)") {
        if idx > 0 {
            let prev = lower.as_bytes()[idx - 1];
            if prev.is_ascii_alphanumeric() {
                continue;
            }
        }

        let mut j = idx + 3;
        while j < line.len() {
            let b = line.as_bytes()[j];
            if b.is_ascii_whitespace() {
                j += 1;
                continue;
            }
            return b.is_ascii_digit() || b.is_ascii_uppercase();
        }
    }
    false
}

/// Check if a line is a candidate for copyright detection.
///
/// A line is a candidate if it contains a hint marker OR a year.
pub fn is_candidate(line: &str) -> bool {
    has_copyright_hint(line) || has_year(line)
}

/// Check if a string ends with a year (for multi-line continuation logic).
///
/// Strips trailing punctuation and whitespace, then checks if the last 4
/// characters form a year in the range 1960–2099.
pub fn has_trailing_year(s: &str) -> bool {
    let trimmed = s.trim_end_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace());
    if trimmed.len() < 4 {
        return false;
    }
    let last4 = &trimmed[trimmed.len() - 4..];
    if let Ok(year) = last4.parse::<u32>() {
        (1960..=2099).contains(&year)
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── has_copyright_hint ──────────────────────────────────────────

    #[test]
    fn test_hint_copyright_symbol() {
        assert!(has_copyright_hint("© 2024 Acme Inc"));
    }

    #[test]
    fn test_hint_c_in_parens() {
        assert!(has_copyright_hint("Copyright (C) 2024"));
    }

    #[test]
    fn test_hint_rst_copy() {
        assert!(has_copyright_hint("Some |copy| notice"));
    }

    #[test]
    fn test_hint_html_entity_decimal() {
        assert!(has_copyright_hint("&#169; 2024 Foo"));
    }

    #[test]
    fn test_hint_html_entity_hex() {
        assert!(has_copyright_hint("&#xa9; 2024 Foo"));
    }

    #[test]
    fn test_hint_opyr() {
        assert!(has_copyright_hint("Copyright 2024"));
    }

    #[test]
    fn test_hint_opyl() {
        assert!(has_copyright_hint("Copyleft notice"));
    }

    #[test]
    fn test_hint_copr() {
        assert!(has_copyright_hint("Copr. 2024 Foo"));
    }

    #[test]
    fn test_hint_right() {
        assert!(has_copyright_hint("All rights reserved"));
    }

    #[test]
    fn test_hint_reserv() {
        assert!(has_copyright_hint("All Rights Reserved."));
    }

    #[test]
    fn test_hint_auth() {
        assert!(has_copyright_hint("@author John Doe"));
    }

    #[test]
    fn test_hint_filecontributor() {
        assert!(has_copyright_hint("SPDX-FileContributor: Jane"));
    }

    #[test]
    fn test_hint_devel() {
        assert!(has_copyright_hint("Developed by Acme"));
    }

    #[test]
    fn test_hint_debian_markup() {
        assert!(has_copyright_hint("<s>John Doe</s>"));
    }

    #[test]
    fn test_hint_by_with_space() {
        assert!(has_copyright_hint("Written by John"));
    }

    #[test]
    fn test_hint_at_sign() {
        assert!(has_copyright_hint("user@example.com"));
    }

    #[test]
    fn test_hint_negative_no_markers() {
        assert!(!has_copyright_hint("This is a plain line of code"));
    }

    #[test]
    fn test_hint_negative_empty() {
        assert!(!has_copyright_hint(""));
    }

    #[test]
    fn test_hint_case_insensitive() {
        assert!(has_copyright_hint("COPYRIGHT 2024"));
        assert!(has_copyright_hint("AUTHOR: John"));
        assert!(has_copyright_hint("DEVELOPED BY Acme"));
    }

    // ── has_year ────────────────────────────────────────────────────

    #[test]
    fn test_year_1959_no_match() {
        assert!(!has_year(" 1959 "));
    }

    #[test]
    fn test_year_1960_match() {
        assert!(has_year(" 1960 "));
    }

    #[test]
    fn test_year_2024_match() {
        assert!(has_year(" 2024 "));
    }

    #[test]
    fn test_year_2039_match() {
        assert!(has_year(" 2039 "));
    }

    #[test]
    fn test_year_2040_match() {
        // Python reference would miss this (only goes up to current year).
        assert!(has_year(" 2040 "));
    }

    #[test]
    fn test_year_2099_match() {
        assert!(has_year(" 2099 "));
    }

    #[test]
    fn test_year_2100_no_match() {
        assert!(!has_year(" 2100 "));
    }

    #[test]
    fn test_year_in_copyright_line() {
        assert!(has_year("Copyright (c) 2024 Acme Inc."));
    }

    #[test]
    fn test_year_with_dash_separator() {
        assert!(has_year("2020-2024"));
    }

    #[test]
    fn test_year_no_surrounding_punct() {
        // Year must be preceded by punctuation or whitespace.
        assert!(!has_year("abc2024def"));
    }

    #[test]
    fn test_year_at_end_of_line() {
        assert!(has_year("Copyright 2024"));
    }

    // ── is_candidate ────────────────────────────────────────────────

    #[test]
    fn test_candidate_with_hint() {
        assert!(is_candidate("Copyright 2024 Acme"));
    }

    #[test]
    fn test_candidate_with_year_only() {
        // No hint marker, but has a year.
        assert!(is_candidate("Some notice 2024 "));
    }

    #[test]
    fn test_candidate_negative() {
        assert!(!is_candidate("Just a plain line of code"));
    }

    // ── has_trailing_year ───────────────────────────────────────────

    #[test]
    fn test_trailing_year_present() {
        assert!(has_trailing_year("some text 2024"));
        assert!(has_trailing_year("some text 2024."));
        assert!(has_trailing_year("some text 2024, "));
    }

    #[test]
    fn test_trailing_year_absent() {
        assert!(!has_trailing_year("some text"));
        assert!(!has_trailing_year("some text abc"));
    }

    #[test]
    fn test_trailing_year_boundary_low() {
        assert!(has_trailing_year("text 1960"));
        assert!(!has_trailing_year("text 1959"));
    }

    #[test]
    fn test_trailing_year_boundary_high() {
        assert!(has_trailing_year("text 2099"));
        assert!(!has_trailing_year("text 2100"));
    }

    #[test]
    fn test_trailing_year_short_string() {
        assert!(!has_trailing_year("20"));
        assert!(!has_trailing_year(""));
    }
}
