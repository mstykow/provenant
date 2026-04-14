/// Shared utility functions for package parsers
///
/// This module provides common file I/O and parsing utilities
/// used across multiple parser implementations.
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use anyhow::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use packageurl::PackageUrl;

/// Default maximum file size for non-archive manifest files (100 MB).
pub const MAX_MANIFEST_SIZE: u64 = 100 * 1024 * 1024;

/// Default maximum length for individual string field values (10 MB).
pub const MAX_FIELD_LENGTH: usize = 10 * 1024 * 1024;

/// Default maximum iteration count for loops processing items (100,000).
pub const MAX_ITERATION_COUNT: usize = 100_000;

/// Truncates a string field value to [`MAX_FIELD_LENGTH`] bytes if it exceeds
/// the limit, returning the truncated string. Returns the original string if
/// within limits.
pub fn truncate_field(value: String) -> String {
    if value.len() <= MAX_FIELD_LENGTH {
        return value;
    }
    let truncated = &value[..value.floor_char_boundary(MAX_FIELD_LENGTH)];
    crate::parser_warn!(
        "Truncated field value from {} bytes to {} bytes (MAX_FIELD_LENGTH)",
        value.len(),
        truncated.len()
    );
    truncated.to_string()
}

/// Reads a file's entire contents into a String with ADR 0004 security checks.
///
/// Performs the following validations before reading:
/// 1. **File existence**: checks `fs::metadata()` before opening
/// 2. **File size**: rejects files exceeding `max_size` (default 100 MB)
/// 3. **UTF-8 encoding**: on UTF-8 failure, falls back to lossy conversion with a warning
///
/// # Arguments
///
/// * `path` - Path to the file to read
/// * `max_size` - Maximum allowed file size in bytes (defaults to [`MAX_MANIFEST_SIZE`])
///
/// # Returns
///
/// * `Ok(String)` - File contents as UTF-8 string (lossy if non-UTF-8 bytes found)
/// * `Err` - File doesn't exist, is too large, or cannot be read
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use provenant::parsers::utils::read_file_to_string;
///
/// let content = read_file_to_string(Path::new("path/to/file.txt"), None)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn read_file_to_string(path: &Path, max_size: Option<u64>) -> Result<String> {
    let limit = max_size.unwrap_or(MAX_MANIFEST_SIZE);

    let metadata =
        fs::metadata(path).map_err(|e| anyhow::anyhow!("Cannot stat file {:?}: {}", path, e))?;

    if metadata.len() > limit {
        anyhow::bail!(
            "File {:?} is {} bytes, exceeding the {} byte limit",
            path,
            metadata.len(),
            limit
        );
    }

    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    let mut file = File::open(path)?;
    file.read_to_end(&mut bytes)?;

    match String::from_utf8(bytes) {
        Ok(s) => Ok(s),
        Err(err) => {
            let bytes = err.into_bytes();
            crate::parser_warn!(
                "File {:?} contains invalid UTF-8; using lossy conversion",
                path
            );
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        }
    }
}

/// Creates a correctly-formatted npm Package URL for scoped or regular packages.
///
/// Handles namespace encoding for scoped packages (e.g., `@babel/core`) and ensures
/// the slash between namespace and package name is NOT encoded as `%2F`.
pub fn npm_purl(full_name: &str, version: Option<&str>) -> Option<String> {
    let (namespace, name) = if full_name.starts_with('@') {
        let parts: Vec<&str> = full_name.splitn(2, '/').collect();
        if parts.len() == 2 {
            (Some(parts[0]), parts[1])
        } else {
            (None, full_name)
        }
    } else {
        (None, full_name)
    };

    let mut purl = PackageUrl::new("npm", name).ok()?;

    if let Some(ns) = namespace {
        purl.with_namespace(ns).ok()?;
    }

    if let Some(ver) = version {
        purl.with_version(ver).ok()?;
    }

    Some(purl.to_string())
}

/// Parses Subresource Integrity (SRI) format and returns hash as hex string.
///
/// SRI format: "algorithm-base64string" (e.g., "sha512-9NET910DNaIPng...")
///
/// Returns the algorithm name and hex-encoded hash digest.
pub fn parse_sri(integrity: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = integrity.splitn(2, '-').collect();
    if parts.len() != 2 {
        return None;
    }

    let algorithm = parts[0];
    let base64_str = parts[1];

    let bytes = BASE64_STANDARD.decode(base64_str).ok()?;

    let hex_string = bytes
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();

    Some((algorithm.to_string(), hex_string))
}

/// Parses "Name <email@domain.com>" format into separate components.
///
/// This utility handles common author/maintainer strings found in package manifests
/// where the format combines a human-readable name with an email address in angle brackets.
///
/// # Arguments
///
/// * `s` - A string potentially containing name and email in "Name \<email\>" format
///
/// # Returns
///
/// A tuple of `(Option<String>, Option<String>)` representing `(name, email)`:
/// - If `\<email\>` pattern found: name (trimmed, or None if empty) and email
/// - If no pattern: trimmed input as name, None for email
///
/// # Examples
///
/// ```
/// use provenant::parsers::utils::split_name_email;
///
/// // Full format
/// let (name, email) = split_name_email("John Doe <john@example.com>");
/// assert_eq!(name, Some("John Doe".to_string()));
/// assert_eq!(email, Some("john@example.com".to_string()));
///
/// // Email only in angle brackets
/// let (name, email) = split_name_email("<john@example.com>");
/// assert_eq!(name, None);
/// assert_eq!(email, Some("john@example.com".to_string()));
///
/// // Name only (no angle brackets)
/// let (name, email) = split_name_email("John Doe");
/// assert_eq!(name, Some("John Doe".to_string()));
/// assert_eq!(email, None);
/// ```
pub fn split_name_email(s: &str) -> (Option<String>, Option<String>) {
    if let Some(email_start) = s.find('<')
        && let Some(email_end) = s.find('>')
        && email_start < email_end
    {
        let name = s[..email_start].trim();
        let email = &s[email_start + 1..email_end];
        (
            if name.is_empty() {
                None
            } else {
                Some(name.to_string())
            },
            Some(email.to_string()),
        )
    } else {
        (Some(s.trim().to_string()), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_read_file_to_string_success() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"test content").unwrap();

        let content = read_file_to_string(&file_path, None).unwrap();
        assert_eq!(content, "test content");
    }

    #[test]
    fn test_read_file_to_string_nonexistent() {
        let path = Path::new("/nonexistent/file.txt");
        let result = read_file_to_string(path, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_file_to_string_empty() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("empty.txt");
        File::create(&file_path).unwrap();

        let content = read_file_to_string(&file_path, None).unwrap();
        assert_eq!(content, "");
    }

    #[test]
    fn test_npm_purl_scoped_with_version() {
        let purl = npm_purl("@babel/core", Some("7.0.0")).unwrap();
        assert_eq!(purl, "pkg:npm/%40babel/core@7.0.0");
    }

    #[test]
    fn test_npm_purl_scoped_without_version() {
        let purl = npm_purl("@babel/core", None).unwrap();
        assert_eq!(purl, "pkg:npm/%40babel/core");
    }

    #[test]
    fn test_npm_purl_unscoped_with_version() {
        let purl = npm_purl("lodash", Some("4.17.21")).unwrap();
        assert_eq!(purl, "pkg:npm/lodash@4.17.21");
    }

    #[test]
    fn test_npm_purl_unscoped_without_version() {
        let purl = npm_purl("lodash", None).unwrap();
        assert_eq!(purl, "pkg:npm/lodash");
    }

    #[test]
    fn test_npm_purl_scoped_slash_not_encoded() {
        let purl = npm_purl("@types/node", Some("18.0.0")).unwrap();
        assert!(purl.contains("/%40types/node"));
        assert!(!purl.contains("%2F"));
    }

    #[test]
    fn test_parse_sri_sha512() {
        let (algo, hash) = parse_sri("sha512-9NET910DNaIPngYnLLPeg+Ogzqsi9uM4mSboU5y6p8S5DzMTVEsJZrawi+BoDNUVBa2DhJqQYUFvMDfgU062LQ==").unwrap();
        assert_eq!(algo, "sha512");
        assert_eq!(hash.len(), 128);
    }

    #[test]
    fn test_parse_sri_sha1() {
        let (algo, hash) = parse_sri("sha1-w7M6te42DYbg5ijwRorn7yfWVN8=").unwrap();
        assert_eq!(algo, "sha1");
        assert_eq!(hash.len(), 40);
    }

    #[test]
    fn test_parse_sri_sha256() {
        let (algo, hash) =
            parse_sri("sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=").unwrap();
        assert_eq!(algo, "sha256");
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn test_parse_sri_invalid_format() {
        assert!(parse_sri("invalid").is_none());
        assert!(parse_sri("sha512").is_none());
        assert!(parse_sri("").is_none());
    }

    #[test]
    fn test_parse_sri_invalid_base64() {
        assert!(parse_sri("sha512-!!!invalid!!!").is_none());
    }

    #[test]
    fn test_split_name_email_full_format() {
        let (name, email) = split_name_email("John Doe <john@example.com>");
        assert_eq!(name, Some("John Doe".to_string()));
        assert_eq!(email, Some("john@example.com".to_string()));
    }

    #[test]
    fn test_split_name_email_name_only() {
        let (name, email) = split_name_email("John Doe");
        assert_eq!(name, Some("John Doe".to_string()));
        assert_eq!(email, None);
    }

    #[test]
    fn test_split_name_email_email_only_plain() {
        let (name, email) = split_name_email("john@example.com");
        assert_eq!(name, Some("john@example.com".to_string()));
        assert_eq!(email, None);
    }

    #[test]
    fn test_split_name_email_email_only_brackets() {
        let (name, email) = split_name_email("<john@example.com>");
        assert_eq!(name, None);
        assert_eq!(email, Some("john@example.com".to_string()));
    }

    #[test]
    fn test_split_name_email_whitespace_trimming() {
        let (name, email) = split_name_email("  John Doe  <  john@example.com  >  ");
        assert_eq!(name, Some("John Doe".to_string()));
        assert_eq!(email, Some("  john@example.com  ".to_string()));
    }

    #[test]
    fn test_split_name_email_empty_string() {
        let (name, email) = split_name_email("");
        assert_eq!(name, Some("".to_string()));
        assert_eq!(email, None);
    }

    #[test]
    fn test_split_name_email_whitespace_only() {
        let (name, email) = split_name_email("   ");
        assert_eq!(name, Some("".to_string()));
        assert_eq!(email, None);
    }

    #[test]
    fn test_split_name_email_invalid_bracket_order() {
        let (name, email) = split_name_email("John >email< Doe");
        assert_eq!(name, Some("John >email< Doe".to_string()));
        assert_eq!(email, None);
    }

    #[test]
    fn test_split_name_email_missing_close_bracket() {
        let (name, email) = split_name_email("John Doe <email@example.com");
        assert_eq!(name, Some("John Doe <email@example.com".to_string()));
        assert_eq!(email, None);
    }

    #[test]
    fn test_split_name_email_missing_open_bracket() {
        let (name, email) = split_name_email("John Doe email@example.com>");
        assert_eq!(name, Some("John Doe email@example.com>".to_string()));
        assert_eq!(email, None);
    }

    #[test]
    fn test_read_file_to_string_oversized() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("big.txt");
        fs::write(&file_path, "x").unwrap();

        let result = read_file_to_string(&file_path, Some(0));
        assert!(result.is_err());
    }

    #[test]
    fn test_read_file_to_string_lossy_utf8() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("bad_utf8.txt");
        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"hello\xffworld").unwrap();

        let content = read_file_to_string(&file_path, None).unwrap();
        assert!(content.contains("hello"));
        assert!(content.contains("world"));
    }

    #[test]
    fn test_truncate_field_within_limit() {
        let s = "short value".to_string();
        assert_eq!(truncate_field(s.clone()), s);
    }

    #[test]
    fn test_truncate_field_exceeds_limit() {
        let long = "x".repeat(MAX_FIELD_LENGTH + 100);
        let truncated = truncate_field(long);
        assert!(truncated.len() <= MAX_FIELD_LENGTH);
    }
}
