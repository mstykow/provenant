// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Source map file processing for license detection.
//!
//! Source map files (.js.map, .css.map) are JSON files containing embedded
//! source code in a `sourcesContent` array. This module extracts that content
//! for license detection.

use std::path::Path;

/// Check if a file is a source map file based on extension.
pub fn is_sourcemap(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| {
            let name_lower = name.to_lowercase();
            name_lower.ends_with(".js.map") || name_lower.ends_with(".css.map")
        })
        .unwrap_or(false)
}

/// Extract source content from a source map JSON file.
///
/// Parses the JSON and extracts all entries from `sourcesContent`,
/// combining them with newlines for license detection.
///
/// Returns `Some(combined_text)` if successfully parsed with content.
/// Returns `None` if JSON parsing fails or no sourcesContent exists.
pub fn extract_sourcemap_content(json_text: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(json_text).ok()?;
    let sources = json.get("sourcesContent")?.as_array()?;

    let combined: String = sources
        .iter()
        .filter_map(|v| v.as_str())
        .map(replace_verbatim_cr_lf_chars)
        .collect::<Vec<_>>()
        .join("\n");

    if combined.is_empty() {
        None
    } else {
        Some(combined)
    }
}

/// Replace verbatim escaped CR/LF characters with actual newlines.
///
/// This matches Python's `replace_verbatim_cr_lf_chars()` behavior exactly:
/// - Double-escaped (e.g., source had literal `\r` that was escaped again):
///   - `\\r\\n` (backslash-backslash-r-backslash-backslash-n) → newline
///   - `\\r` (backslash-backslash-r) → newline
///   - `\\n` (backslash-backslash-n) → newline
/// - Single-escaped (e.g., JSON-escaped newlines):
///   - `\r\n` (backslash-r-backslash-n) → newline
///   - `\r` (backslash-r) → newline
///   - `\n` (backslash-n) → newline
fn replace_verbatim_cr_lf_chars(s: &str) -> String {
    s.replace("\\\\r\\\\n", "\n")
        .replace("\\r\\n", "\n")
        .replace("\\\\r", "\n")
        .replace("\\\\n", "\n")
        .replace("\\r", "\n")
        .replace("\\n", "\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_is_sourcemap_js_map() {
        assert!(is_sourcemap(&PathBuf::from("app.js.map")));
        assert!(is_sourcemap(&PathBuf::from("APP.JS.MAP")));
    }

    #[test]
    fn test_is_sourcemap_css_map() {
        assert!(is_sourcemap(&PathBuf::from("style.css.map")));
        assert!(is_sourcemap(&PathBuf::from("STYLE.CSS.MAP")));
    }

    #[test]
    fn test_is_sourcemap_not_map() {
        assert!(!is_sourcemap(&PathBuf::from("app.js")));
        assert!(!is_sourcemap(&PathBuf::from("data.json")));
        assert!(!is_sourcemap(&PathBuf::from("other.map")));
    }

    #[test]
    fn test_extract_sourcemap_content_basic() {
        let json = r#"{"version":3,"sourcesContent":["hello\nworld"]}"#;
        let result = extract_sourcemap_content(json);
        assert!(result.is_some());
        let content = result.unwrap();
        assert!(content.contains("hello"));
        assert!(content.contains("world"));
    }

    #[test]
    fn test_extract_sourcemap_content_mit_license() {
        let json = r#"{"version":3,"sourcesContent":["Use of this source code is governed by an MIT-style license\nthat can be found in the LICENSE file"]}"#;
        let result = extract_sourcemap_content(json);
        assert!(result.is_some());
        let content = result.unwrap();
        assert!(content.contains("MIT-style license"));
        assert!(content.contains("LICENSE file"));
        assert!(content.contains("\n"));
    }

    #[test]
    fn test_extract_sourcemap_content_multiple_entries() {
        let json = r#"{"version":3,"sourcesContent":["first\nfile","second\nfile"]}"#;
        let result = extract_sourcemap_content(json);
        assert!(result.is_some());
        let content = result.unwrap();
        assert!(content.contains("first"));
        assert!(content.contains("second"));
    }

    #[test]
    fn test_extract_sourcemap_content_no_sources() {
        let json = r#"{"version":3,"sources":[]}"#;
        let result = extract_sourcemap_content(json);
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_sourcemap_content_invalid_json() {
        let json = r#"not valid json"#;
        let result = extract_sourcemap_content(json);
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_sourcemap_content_null_entries() {
        let json = r#"{"version":3,"sourcesContent":[null,"actual\ncontent"]}"#;
        let result = extract_sourcemap_content(json);
        assert!(result.is_some());
        let content = result.unwrap();
        assert!(content.contains("actual"));
    }

    #[test]
    fn test_replace_verbatim_cr_lf_chars() {
        // Single-escaped (backslash-n, backslash-r in the string)
        assert_eq!(replace_verbatim_cr_lf_chars("a\\nb"), "a\nb");
        assert_eq!(replace_verbatim_cr_lf_chars("a\\rb"), "a\nb");
        assert_eq!(replace_verbatim_cr_lf_chars("a\\r\\nb"), "a\nb");
        // Double-escaped (literal backslash-backslash-n in the string)
        assert_eq!(replace_verbatim_cr_lf_chars("a\\\\nb"), "a\nb");
        assert_eq!(replace_verbatim_cr_lf_chars("a\\\\rb"), "a\nb");
        assert_eq!(replace_verbatim_cr_lf_chars("a\\\\r\\\\nb"), "a\nb");
    }

    #[test]
    fn test_ar_er_js_map_detection() {
        let path = PathBuf::from("testdata/license-golden/datadriven/lic2/ar-ER.js.map");
        if !path.exists() {
            eprintln!("Skipping test: test file not found");
            return;
        }

        let text = std::fs::read_to_string(&path).expect("Failed to read file");
        eprintln!("Raw text length: {}", text.len());

        let json: serde_json::Value = serde_json::from_str(&text).expect("JSON parse failed");
        let sources = json
            .get("sourcesContent")
            .expect("No sourcesContent")
            .as_array()
            .expect("Not array");
        eprintln!("Sources array length: {}", sources.len());

        if let Some(first) = sources.first().and_then(|v| v.as_str()) {
            eprintln!("First source length: {}", first.len());
            eprintln!("First 100 chars: {:?}", &first[..100.min(first.len())]);
        }

        let result = extract_sourcemap_content(&text);
        assert!(result.is_some(), "Should extract content from ar-ER.js.map");

        let content = result.unwrap();
        eprintln!("Extracted content length: {}", content.len());
        assert!(
            content.contains("MIT-style license"),
            "Should contain MIT license text"
        );
    }
}
