// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Linux CREDITS file detection and parsing.
//!
//! Detects structured `N:/E:/W:` format used by Linux kernel, LLVM, Botan, u-boot, etc.
//! An entry looks like:
//!   N: Jack Lloyd
//!   E: lloyd@randombit.net
//!   W: http://www.randombit.net/
//!   P: (PGP key - ignored)
//!   B: (bitcoin - ignored)

use std::path::Path;

use super::types::AuthorDetection;
use crate::models::LineNumber;

/// Filenames recognized as CREDITS/AUTHORS files (case-insensitive)
const CREDITS_FILENAMES: &[&str] = &[
    "credit",
    "credits",
    "credits.rst",
    "credits.txt",
    "credits.md",
    "author",
    "authors",
    "authors.rst",
    "authors.txt",
    "authors.md",
];

/// Check if a file path is a CREDITS/AUTHORS file by its filename.
pub fn is_credits_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            let lower = name.to_lowercase();
            CREDITS_FILENAMES.contains(&lower.as_str())
        })
}

/// Detect authors from a CREDITS-formatted file's content.
///
/// Parses `N:` (name), `E:` (email), `W:` (web URL) lines.
/// Groups entries by blank lines. Bails out after 50 lines if no structured format found.
/// Returns empty vec if file doesn't contain CREDITS format.
pub fn detect_credits_authors(content: &str) -> Vec<AuthorDetection> {
    let mut results = Vec::new();
    let mut has_credits = false;
    let mut current_group: Vec<(usize, &str)> = Vec::new();

    for (idx, line) in content.lines().enumerate() {
        let line_number = idx + 1; // 1-based
        let trimmed = line.trim();

        // Empty line: emit current group
        if trimmed.is_empty() {
            if let Some(detection) = process_credit_group(&current_group) {
                results.push(detection);
            }
            current_group.clear();
            continue;
        }

        // Check for N:, E:, W: lines
        if trimmed.starts_with("N:") || trimmed.starts_with("E:") || trimmed.starts_with("W:") {
            has_credits = true;
            current_group.push((line_number, trimmed));
        }

        // Bail out if no structured credits in first 50 lines
        if line_number > 50 && !has_credits {
            return results;
        }
    }

    // Process any remaining group
    if let Some(detection) = process_credit_group(&current_group) {
        results.push(detection);
    }

    results
}

/// Process a single group of N:/E:/W: lines into an AuthorDetection.
fn process_credit_group(group: &[(usize, &str)]) -> Option<AuthorDetection> {
    let mut names = Vec::new();
    let mut emails = Vec::new();
    let mut webs = Vec::new();

    let start_line = group.first()?.0;
    let end_line = group.last()?.0;

    for &(_, line) in group {
        // Split on first ':'
        if let Some((ltype, value)) = line.split_once(':') {
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            match ltype.trim() {
                "N" => names.push(value),
                "E" => emails.push(value),
                "W" => webs.push(value),
                _ => {}
            }
        }
    }

    // Build author string: "Name email url"
    let items: Vec<String> = [names, emails, webs]
        .iter()
        .filter(|v| !v.is_empty())
        .map(|v| v.join(" "))
        .collect();

    let author = items.join(" ");
    if author.is_empty() {
        return None;
    }

    Some(AuthorDetection {
        author,
        start_line: LineNumber::new(start_line).expect("invalid line number"),
        end_line: LineNumber::new(end_line).expect("invalid line number"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_is_credits_file() {
        assert!(is_credits_file(&PathBuf::from("CREDITS")));
        assert!(is_credits_file(&PathBuf::from("credits")));
        assert!(is_credits_file(&PathBuf::from("Credits.txt")));
        assert!(is_credits_file(&PathBuf::from("AUTHORS")));
        assert!(is_credits_file(&PathBuf::from("authors.md")));
        assert!(is_credits_file(&PathBuf::from("AUTHORS.rst")));
        assert!(!is_credits_file(&PathBuf::from("README.md")));
        assert!(!is_credits_file(&PathBuf::from("LICENSE")));
        assert!(!is_credits_file(&PathBuf::from("src/credits.rs")));
    }

    #[test]
    fn test_detect_credits_authors_simple() {
        let content = "\
N: Jack Lloyd
E: lloyd@randombit.net
W: http://www.randombit.net/
";
        let authors = detect_credits_authors(content);
        assert_eq!(authors.len(), 1);
        assert_eq!(
            authors[0].author,
            "Jack Lloyd lloyd@randombit.net http://www.randombit.net/"
        );
        assert_eq!(authors[0].start_line, LineNumber::ONE);
        assert_eq!(authors[0].end_line, LineNumber::new(3).unwrap());
    }

    #[test]
    fn test_detect_credits_authors_multiple() {
        let content = "\
N: Alice Smith
E: alice@example.com

N: Bob Jones
E: bob@example.com
W: https://bob.example.com
";
        let authors = detect_credits_authors(content);
        assert_eq!(authors.len(), 2);
        assert_eq!(authors[0].author, "Alice Smith alice@example.com");
        assert_eq!(
            authors[1].author,
            "Bob Jones bob@example.com https://bob.example.com"
        );
    }

    #[test]
    fn test_detect_credits_authors_name_only() {
        let content = "N: John Doe\n";
        let authors = detect_credits_authors(content);
        assert_eq!(authors.len(), 1);
        assert_eq!(authors[0].author, "John Doe");
    }

    #[test]
    fn test_detect_credits_authors_empty() {
        let authors = detect_credits_authors("");
        assert!(authors.is_empty());
    }

    #[test]
    fn test_detect_credits_authors_no_credits_format() {
        let content = "This is just a regular text file.\nWith no structured credits.\n";
        let authors = detect_credits_authors(content);
        assert!(authors.is_empty());
    }

    #[test]
    fn test_detect_credits_authors_bail_after_50_lines() {
        // Create 60 lines of regular text — should bail after line 50
        let mut content = String::new();
        for i in 1..=60 {
            content.push_str(&format!("Line {} of regular text\n", i));
        }
        content.push_str("N: Late Author\nE: late@example.com\n");
        let authors = detect_credits_authors(&content);
        assert!(authors.is_empty());
    }

    #[test]
    fn test_detect_credits_ignores_pgp_and_bitcoin() {
        let content = "\
N: Jack Lloyd
E: lloyd@randombit.net
W: http://www.randombit.net/
P: 3F69 2E64 6D92 3BBE E7AE 9258 5C0F 96E8 4EC1 6D6B
B: 1DwxWb2J4vuX4vjsbzaCXW696rZfeamahz
";
        let authors = detect_credits_authors(content);
        assert_eq!(authors.len(), 1);
        // P: and B: lines should be ignored
        assert_eq!(
            authors[0].author,
            "Jack Lloyd lloyd@randombit.net http://www.randombit.net/"
        );
    }
}
