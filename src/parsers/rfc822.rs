// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Shared RFC822-style metadata parser.
//!
//! This module provides a reusable parser for RFC822/RFC2822-like metadata formats
//! used across multiple ecosystems:
//!
//! - Python: PKG-INFO, METADATA files
//! - Debian: debian/control, dpkg/status, .dsc, .changes
//! - Alpine: APKBUILD metadata
//!
//! # Format Specification
//!
//! RFC822 metadata consists of key-value headers followed by an optional body:
//! - Headers: `Key: Value` format, one per line
//! - Continuation lines: start with space or tab (appended to previous header)
//! - Duplicate fields: stored as `Vec<String>` (e.g., multiple `Classifier:` in PKG-INFO)
//! - Body: separated from headers by a blank line
//! - Field names: normalized to lowercase for case-insensitive matching
//!
//! # Debian-Specific Extensions
//!
//! Debian control files use a variant where:
//! - ` .` (space + dot) represents a blank line within a multiline field
//! - Multiple paragraphs are separated by blank lines
//! - The body concept is replaced by multi-paragraph parsing

use std::collections::HashMap;

use crate::parser_warn as warn;
use crate::parsers::utils::{MAX_ITERATION_COUNT, truncate_field};

/// Parsed RFC822 metadata containing headers and an optional body.
///
/// Headers are stored as `HashMap<String, Vec<String>>` to support duplicate
/// field names (e.g., multiple `Classifier:` headers in Python PKG-INFO).
/// All field names are normalized to lowercase.
#[derive(Debug, Clone)]
pub struct Rfc822Metadata {
    /// Headers parsed from the metadata, with lowercase keys.
    /// Duplicate headers are stored as multiple entries in the Vec.
    pub headers: HashMap<String, Vec<String>>,
    /// Body content after the first blank line separator.
    /// Empty string if no body is present.
    pub body: String,
}

/// Parses RFC822-style metadata content into headers and body.
///
/// This parser handles:
/// - Standard `Key: Value` headers
/// - Continuation lines (starting with space or tab)
/// - Duplicate field names (stored as multiple values)
/// - Case-insensitive field names (normalized to lowercase)
/// - Body separation at first blank line
///
/// # Arguments
///
/// * `content` - The raw RFC822-style metadata content
///
/// # Returns
///
/// An `Rfc822Metadata` struct with parsed headers and body.
///
/// # Examples
///
/// ```ignore
/// let content = "Name: example\nVersion: 1.0\n\nBody text here";
/// let metadata = parse_rfc822_content(content);
/// assert_eq!(get_header_first(&metadata.headers, "name"), Some("example".to_string()));
/// assert_eq!(metadata.body, "Body text here");
/// ```
pub fn parse_rfc822_content(content: &str) -> Rfc822Metadata {
    let mut headers: HashMap<String, Vec<String>> = HashMap::new();
    let mut current_name: Option<String> = None;
    let mut current_value = String::new();
    let mut body_lines: Vec<String> = Vec::new();
    let mut in_headers = true;

    for (i, line) in content.lines().enumerate() {
        if i >= MAX_ITERATION_COUNT {
            warn!(
                "RFC822 parser iteration limit reached ({MAX_ITERATION_COUNT}), truncating input"
            );
            break;
        }

        if in_headers {
            if line.is_empty() {
                if let Some(name) = current_name.take() {
                    current_value = truncate_field(current_value);
                    add_header_value(&mut headers, &name, &current_value);
                    current_value.clear();
                }
                in_headers = false;
                continue;
            }

            if line.starts_with(' ') || line.starts_with('\t') {
                if !current_value.is_empty() {
                    current_value.push(' ');
                }
                current_value.push_str(line.trim_start());
                continue;
            }

            if let Some(name) = current_name.take() {
                current_value = truncate_field(current_value);
                add_header_value(&mut headers, &name, &current_value);
                current_value.clear();
            }

            if let Some((name, value)) = line.split_once(':') {
                current_name = Some(name.trim().to_ascii_lowercase());
                current_value = value.trim_start().to_string();
            }
        } else {
            body_lines.push(line.to_string());
        }
    }

    // Flush last header if still open (no trailing blank line)
    if let Some(name) = current_name.take() {
        current_value = truncate_field(current_value);
        add_header_value(&mut headers, &name, &current_value);
    }

    let mut body = body_lines.join("\n");
    body = body.trim_end_matches(['\n', '\r']).to_string();

    Rfc822Metadata { headers, body }
}

/// Parses multi-paragraph RFC822-style content (e.g., debian/control, dpkg/status).
///
/// Splits the content at blank lines and parses each paragraph independently.
/// Each paragraph is returned as a separate `Rfc822Metadata` (body will be empty
/// since paragraphs don't have a body concept - they're all headers).
///
/// # Arguments
///
/// * `content` - The raw multi-paragraph content
///
/// # Returns
///
/// A `Vec<Rfc822Metadata>`, one per paragraph. Empty paragraphs are skipped.
pub fn parse_rfc822_paragraphs(content: &str) -> Vec<Rfc822Metadata> {
    let mut paragraphs = Vec::new();
    let mut current_paragraph = String::new();

    for (i, line) in content.lines().enumerate() {
        if i >= MAX_ITERATION_COUNT {
            warn!(
                "RFC822 paragraph parser iteration limit reached ({MAX_ITERATION_COUNT}), truncating input"
            );
            break;
        }

        if line.is_empty() {
            if !current_paragraph.is_empty() {
                // Parse the accumulated paragraph as a single-paragraph RFC822
                // (no body separation - treat entire content as headers)
                let metadata = parse_paragraph_headers(&current_paragraph);
                paragraphs.push(metadata);
                current_paragraph.clear();
            }
        } else {
            if !current_paragraph.is_empty() {
                current_paragraph.push('\n');
            }
            current_paragraph.push_str(line);
        }
    }

    // Flush last paragraph
    if !current_paragraph.is_empty() {
        let metadata = parse_paragraph_headers(&current_paragraph);
        paragraphs.push(metadata);
    }

    paragraphs
}

/// Parses a single paragraph where all content is treated as headers (no body).
///
/// This is used for multi-paragraph parsing where blank lines separate paragraphs
/// rather than delimiting headers from body.
fn parse_paragraph_headers(content: &str) -> Rfc822Metadata {
    let mut headers: HashMap<String, Vec<String>> = HashMap::new();
    let mut current_name: Option<String> = None;
    let mut current_value = String::new();

    for (i, line) in content.lines().enumerate() {
        if i >= MAX_ITERATION_COUNT {
            warn!(
                "RFC822 paragraph headers parser iteration limit reached ({MAX_ITERATION_COUNT}), truncating input"
            );
            break;
        }

        // Continuation line
        if line.starts_with(' ') || line.starts_with('\t') {
            if current_name.is_some() {
                // For Debian-style multiline: preserve newlines and ` .` blank line markers
                current_value.push('\n');
                current_value.push_str(line);
            }
            continue;
        }

        // Flush previous header
        if let Some(name) = current_name.take() {
            current_value = truncate_field(current_value);
            add_header_value(&mut headers, &name, &current_value);
            current_value.clear();
        }

        // Parse new header line
        if let Some((name, value)) = line.split_once(':') {
            current_name = Some(name.trim().to_ascii_lowercase());
            current_value = value.trim_start().to_string();
        }
    }

    // Flush last header
    if let Some(name) = current_name.take() {
        current_value = truncate_field(current_value);
        add_header_value(&mut headers, &name, &current_value);
    }

    Rfc822Metadata {
        headers,
        body: String::new(),
    }
}

/// Adds a header value to the headers map, handling duplicate field names.
///
/// Values are trimmed at the end. Empty values are not added.
fn add_header_value(headers: &mut HashMap<String, Vec<String>>, name: &str, value: &str) {
    let entry = headers.entry(name.to_string()).or_default();
    let trimmed = value.trim_end();
    if !trimmed.is_empty() {
        entry.push(trimmed.to_string());
    }
}

/// Returns the first value for a header, or None if not present.
///
/// Header names are matched case-insensitively (keys are already lowercase).
///
/// # Arguments
///
/// * `headers` - The headers map from `Rfc822Metadata`
/// * `key` - The header name to look up (case-insensitive)
pub fn get_header_first(headers: &HashMap<String, Vec<String>>, key: &str) -> Option<String> {
    headers
        .get(&key.to_ascii_lowercase())
        .and_then(|values| values.first())
        .map(|value| value.trim().to_string())
}

/// Returns all values for a header, or an empty Vec if not present.
///
/// Header names are matched case-insensitively (keys are already lowercase).
/// Empty/whitespace-only values are filtered out.
///
/// # Arguments
///
/// * `headers` - The headers map from `Rfc822Metadata`
/// * `key` - The header name to look up (case-insensitive)
pub fn get_header_all(headers: &HashMap<String, Vec<String>>, key: &str) -> Vec<String> {
    headers
        .get(&key.to_ascii_lowercase())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ====== Basic parsing ======

    #[test]
    fn test_simple_headers() {
        let content = "Name: example\nVersion: 1.0\nSummary: A test package";
        let metadata = parse_rfc822_content(content);
        assert_eq!(
            get_header_first(&metadata.headers, "name"),
            Some("example".to_string())
        );
        assert_eq!(
            get_header_first(&metadata.headers, "version"),
            Some("1.0".to_string())
        );
        assert_eq!(
            get_header_first(&metadata.headers, "summary"),
            Some("A test package".to_string())
        );
        assert!(metadata.body.is_empty());
    }

    #[test]
    fn test_case_insensitive_keys() {
        let content = "NAME: upper\nName: mixed\nname: lower";
        let metadata = parse_rfc822_content(content);
        // All three should be stored under the same lowercase key
        let all = get_header_all(&metadata.headers, "name");
        assert_eq!(all.len(), 3);
        assert_eq!(all[0], "upper");
        assert_eq!(all[1], "mixed");
        assert_eq!(all[2], "lower");
    }

    #[test]
    fn test_case_insensitive_lookup() {
        let content = "Name: example";
        let metadata = parse_rfc822_content(content);
        assert_eq!(
            get_header_first(&metadata.headers, "NAME"),
            Some("example".to_string())
        );
        assert_eq!(
            get_header_first(&metadata.headers, "Name"),
            Some("example".to_string())
        );
        assert_eq!(
            get_header_first(&metadata.headers, "name"),
            Some("example".to_string())
        );
    }

    // ====== Continuation lines ======

    #[test]
    fn test_continuation_with_space() {
        let content = "Description: first line\n second line\n third line";
        let metadata = parse_rfc822_content(content);
        assert_eq!(
            get_header_first(&metadata.headers, "description"),
            Some("first line second line third line".to_string())
        );
    }

    #[test]
    fn test_continuation_with_tab() {
        let content = "Description: first line\n\tsecond line\n\tthird line";
        let metadata = parse_rfc822_content(content);
        assert_eq!(
            get_header_first(&metadata.headers, "description"),
            Some("first line second line third line".to_string())
        );
    }

    #[test]
    fn test_continuation_preserves_internal_whitespace() {
        let content = "Description: hello\n   world  test";
        let metadata = parse_rfc822_content(content);
        // trim_start removes leading whitespace, then space joins
        assert_eq!(
            get_header_first(&metadata.headers, "description"),
            Some("hello world  test".to_string())
        );
    }

    // ====== Body separation ======

    #[test]
    fn test_body_separation() {
        let content = "Name: example\nVersion: 1.0\n\nThis is the body\nWith multiple lines";
        let metadata = parse_rfc822_content(content);
        assert_eq!(
            get_header_first(&metadata.headers, "name"),
            Some("example".to_string())
        );
        assert_eq!(metadata.body, "This is the body\nWith multiple lines");
    }

    #[test]
    fn test_empty_body() {
        let content = "Name: example\n\n";
        let metadata = parse_rfc822_content(content);
        assert_eq!(
            get_header_first(&metadata.headers, "name"),
            Some("example".to_string())
        );
        assert!(metadata.body.is_empty());
    }

    #[test]
    fn test_no_body() {
        let content = "Name: example\nVersion: 1.0";
        let metadata = parse_rfc822_content(content);
        assert_eq!(
            get_header_first(&metadata.headers, "name"),
            Some("example".to_string())
        );
        assert!(metadata.body.is_empty());
    }

    #[test]
    fn test_body_with_trailing_newlines() {
        let content = "Name: test\n\nBody text\n\n\n";
        let metadata = parse_rfc822_content(content);
        assert_eq!(metadata.body, "Body text");
    }

    // ====== Duplicate fields ======

    #[test]
    fn test_duplicate_fields() {
        let content = "Classifier: License :: OSI Approved\nClassifier: Topic :: Software\nClassifier: Programming Language :: Python";
        let metadata = parse_rfc822_content(content);
        let classifiers = get_header_all(&metadata.headers, "classifier");
        assert_eq!(classifiers.len(), 3);
        assert_eq!(classifiers[0], "License :: OSI Approved");
        assert_eq!(classifiers[1], "Topic :: Software");
        assert_eq!(classifiers[2], "Programming Language :: Python");
    }

    #[test]
    fn test_get_header_first_with_duplicates() {
        let content = "Classifier: First\nClassifier: Second\nClassifier: Third";
        let metadata = parse_rfc822_content(content);
        assert_eq!(
            get_header_first(&metadata.headers, "classifier"),
            Some("First".to_string())
        );
    }

    // ====== Edge cases ======

    #[test]
    fn test_empty_content() {
        let metadata = parse_rfc822_content("");
        assert!(metadata.headers.is_empty());
        assert!(metadata.body.is_empty());
    }

    #[test]
    fn test_value_with_colon() {
        let content = "Homepage: https://example.com:8080/path";
        let metadata = parse_rfc822_content(content);
        assert_eq!(
            get_header_first(&metadata.headers, "homepage"),
            Some("https://example.com:8080/path".to_string())
        );
    }

    #[test]
    fn test_missing_header() {
        let content = "Name: example";
        let metadata = parse_rfc822_content(content);
        assert_eq!(get_header_first(&metadata.headers, "missing"), None);
        assert!(get_header_all(&metadata.headers, "missing").is_empty());
    }

    #[test]
    fn test_header_with_empty_value() {
        let content = "Name: \nVersion: 1.0";
        let metadata = parse_rfc822_content(content);
        // Empty value should not be added
        assert_eq!(get_header_first(&metadata.headers, "name"), None);
        assert_eq!(
            get_header_first(&metadata.headers, "version"),
            Some("1.0".to_string())
        );
    }

    #[test]
    fn test_whitespace_only_value() {
        let content = "Name:    \nVersion: 1.0";
        let metadata = parse_rfc822_content(content);
        // Whitespace-only value trimmed to empty => not added
        assert_eq!(get_header_first(&metadata.headers, "name"), None);
    }

    // ====== Multi-paragraph parsing ======

    #[test]
    fn test_multi_paragraph_two_paragraphs() {
        let content =
            "Source: example\nMaintainer: John Doe\n\nPackage: example-bin\nArchitecture: amd64";
        let paragraphs = parse_rfc822_paragraphs(content);
        assert_eq!(paragraphs.len(), 2);
        assert_eq!(
            get_header_first(&paragraphs[0].headers, "source"),
            Some("example".to_string())
        );
        assert_eq!(
            get_header_first(&paragraphs[1].headers, "package"),
            Some("example-bin".to_string())
        );
    }

    #[test]
    fn test_multi_paragraph_three_paragraphs() {
        let content = "Package: pkg1\nVersion: 1.0\n\nPackage: pkg2\nVersion: 2.0\n\nPackage: pkg3\nVersion: 3.0";
        let paragraphs = parse_rfc822_paragraphs(content);
        assert_eq!(paragraphs.len(), 3);
        assert_eq!(
            get_header_first(&paragraphs[0].headers, "package"),
            Some("pkg1".to_string())
        );
        assert_eq!(
            get_header_first(&paragraphs[1].headers, "package"),
            Some("pkg2".to_string())
        );
        assert_eq!(
            get_header_first(&paragraphs[2].headers, "package"),
            Some("pkg3".to_string())
        );
    }

    #[test]
    fn test_multi_paragraph_empty_content() {
        let paragraphs = parse_rfc822_paragraphs("");
        assert!(paragraphs.is_empty());
    }

    #[test]
    fn test_multi_paragraph_single_paragraph() {
        let content = "Name: test\nVersion: 1.0";
        let paragraphs = parse_rfc822_paragraphs(content);
        assert_eq!(paragraphs.len(), 1);
        assert_eq!(
            get_header_first(&paragraphs[0].headers, "name"),
            Some("test".to_string())
        );
    }

    #[test]
    fn test_multi_paragraph_with_continuation() {
        let content = "Package: example\nDescription: Short summary\n Long description\n continues here\n\nPackage: other\nVersion: 2.0";
        let paragraphs = parse_rfc822_paragraphs(content);
        assert_eq!(paragraphs.len(), 2);
        let desc = get_header_first(&paragraphs[0].headers, "description");
        assert!(desc.is_some());
        let desc_str = desc.unwrap();
        assert!(desc_str.starts_with("Short summary"));
        assert!(desc_str.contains("Long description"));
    }

    #[test]
    fn test_multi_paragraph_multiple_blank_lines() {
        let content = "Package: pkg1\nVersion: 1.0\n\n\n\nPackage: pkg2\nVersion: 2.0";
        let paragraphs = parse_rfc822_paragraphs(content);
        // Multiple blank lines should still result in just 2 paragraphs
        assert_eq!(paragraphs.len(), 2);
    }

    // ====== Debian-specific paragraph format ======

    #[test]
    fn test_paragraph_headers_debian_description() {
        // Debian Description has continuation lines with ` .` for blank line markers
        let content = "Package: example\nDescription: Short summary\n Long description.\n .\n Second paragraph.";
        let paragraphs = parse_rfc822_paragraphs(content);
        assert_eq!(paragraphs.len(), 1);
        let desc = get_header_first(&paragraphs[0].headers, "description");
        assert!(desc.is_some());
        let desc_str = desc.unwrap();
        assert!(desc_str.contains("Short summary"));
        assert!(desc_str.contains("."));
        assert!(desc_str.contains("Second paragraph."));
    }

    #[test]
    fn test_paragraph_headers_debian_depends() {
        let content =
            "Package: example\nDepends: libc6 (>= 2.17), libssl1.1 (>= 1.1.0)\nVersion: 1.0-1";
        let paragraphs = parse_rfc822_paragraphs(content);
        assert_eq!(paragraphs.len(), 1);
        assert_eq!(
            get_header_first(&paragraphs[0].headers, "depends"),
            Some("libc6 (>= 2.17), libssl1.1 (>= 1.1.0)".to_string())
        );
    }

    // ====== Real-world PKG-INFO format ======

    #[test]
    fn test_real_world_pkginfo() {
        let content = "\
Metadata-Version: 2.1
Name: requests
Version: 2.31.0
Summary: Python HTTP for Humans.
Home-page: https://requests.readthedocs.io
Author: Kenneth Reitz
Author-email: me@kennethreitz.org
License: Apache-2.0
Classifier: License :: OSI Approved :: Apache Software License
Classifier: Programming Language :: Python :: 3

Requests is an elegant and simple HTTP library for Python.";

        let metadata = parse_rfc822_content(content);
        assert_eq!(
            get_header_first(&metadata.headers, "name"),
            Some("requests".to_string())
        );
        assert_eq!(
            get_header_first(&metadata.headers, "version"),
            Some("2.31.0".to_string())
        );
        assert_eq!(
            get_header_first(&metadata.headers, "license"),
            Some("Apache-2.0".to_string())
        );
        let classifiers = get_header_all(&metadata.headers, "classifier");
        assert_eq!(classifiers.len(), 2);
        assert_eq!(
            metadata.body,
            "Requests is an elegant and simple HTTP library for Python."
        );
    }

    // ====== Real-world debian/control format ======

    #[test]
    fn test_real_world_debian_control() {
        let content = "\
Source: curl
Section: web
Priority: optional
Maintainer: Alessandro Ghedini <ghedo@debian.org>
Build-Depends: debhelper (>= 12), libssl-dev (>= 1.1.0)

Package: curl
Architecture: any
Depends: ${shlibs:Depends}, ${misc:Depends}, libcurl4 (= ${binary:Version})
Description: command line tool for transferring data with URL syntax
 curl is a command line tool for transferring data with URL syntax.
 .
 It supports multiple protocols including HTTP, HTTPS, FTP.

Package: libcurl4
Architecture: any
Multi-Arch: same
Depends: ${shlibs:Depends}, ${misc:Depends}
Description: easy-to-use client-side URL transfer library
 libcurl is an easy-to-use client-side URL transfer library.";

        let paragraphs = parse_rfc822_paragraphs(content);
        assert_eq!(paragraphs.len(), 3);

        // Source paragraph
        assert_eq!(
            get_header_first(&paragraphs[0].headers, "source"),
            Some("curl".to_string())
        );
        assert_eq!(
            get_header_first(&paragraphs[0].headers, "maintainer"),
            Some("Alessandro Ghedini <ghedo@debian.org>".to_string())
        );

        // First binary paragraph
        assert_eq!(
            get_header_first(&paragraphs[1].headers, "package"),
            Some("curl".to_string())
        );
        assert_eq!(
            get_header_first(&paragraphs[1].headers, "architecture"),
            Some("any".to_string())
        );

        // Second binary paragraph
        assert_eq!(
            get_header_first(&paragraphs[2].headers, "package"),
            Some("libcurl4".to_string())
        );
        assert_eq!(
            get_header_first(&paragraphs[2].headers, "multi-arch"),
            Some("same".to_string())
        );
    }
}
