// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Loader-stage license type.
//!
//! This module defines `LoadedLicense`, which represents a parsed and normalized
//! license file (.LICENSE) before it is converted to a runtime `License`.
//!
//! Loader-stage responsibilities include:
//! - Key derivation from filename
//! - Name fallback chain resolution
//! - URL merging from multiple source fields
//! - Text trimming and normalization
//! - Deprecation metadata preservation (without filtering)

use serde::{Deserialize, Serialize};

/// Loader-stage representation of a license.
///
/// This struct contains parsed and normalized data from a .LICENSE file.
/// It is serialized at build time and deserialized at runtime, then converted
/// to a runtime `License` during the build stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadedLicense {
    pub key: String,
    pub short_name: Option<String>,
    pub name: String,
    pub language: Option<String>,
    pub spdx_license_key: Option<String>,
    pub other_spdx_license_keys: Vec<String>,
    pub category: Option<String>,
    pub owner: Option<String>,
    pub homepage_url: Option<String>,
    pub text: String,
    pub reference_urls: Vec<String>,
    pub osi_license_key: Option<String>,
    pub text_urls: Vec<String>,
    pub osi_url: Option<String>,
    pub faq_url: Option<String>,
    pub other_urls: Vec<String>,
    pub notes: Option<String>,
    pub is_deprecated: bool,
    pub is_exception: bool,
    pub is_unknown: bool,
    pub is_generic: bool,
    pub replaced_by: Vec<String>,
    pub minimum_coverage: Option<u8>,
    pub standard_notice: Option<String>,
    pub ignorable_copyrights: Option<Vec<String>>,
    pub ignorable_holders: Option<Vec<String>>,
    pub ignorable_authors: Option<Vec<String>>,
    pub ignorable_urls: Option<Vec<String>>,
    pub ignorable_emails: Option<Vec<String>>,
}

/// Loader-stage normalization functions for license data.
impl LoadedLicense {
    /// Derive key from filename.
    ///
    /// Returns the file stem (filename without extension) as the key.
    /// This should match the `key` field in the frontmatter.
    pub fn derive_key(path: &std::path::Path) -> Result<String, LicenseKeyError> {
        path.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .ok_or(LicenseKeyError::CannotExtractKey)
    }

    /// Validate that the frontmatter key matches the filename key.
    pub fn validate_key_match(
        filename_key: &str,
        frontmatter_key: Option<&str>,
    ) -> Result<(), LicenseKeyError> {
        match frontmatter_key {
            Some(fm_key) if fm_key != filename_key => Err(LicenseKeyError::KeyMismatch {
                filename: filename_key.to_string(),
                frontmatter: fm_key.to_string(),
            }),
            _ => Ok(()),
        }
    }

    /// Derive name using the fallback chain.
    ///
    /// Priority order:
    /// 1. `name` field
    /// 2. `short_name` field
    /// 3. `key` as fallback
    pub fn derive_name(name: Option<&str>, short_name: Option<&str>, key: &str) -> String {
        name.map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                short_name
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_else(|| key.to_string())
    }

    /// Merge reference URLs from multiple source fields.
    ///
    /// Collects URLs in this order:
    /// 1. text_urls
    /// 2. other_urls
    /// 3. osi_url
    /// 4. faq_url
    /// 5. homepage_url
    pub fn merge_reference_urls(
        text_urls: Option<&[String]>,
        other_urls: Option<&[String]>,
        osi_url: Option<&str>,
        faq_url: Option<&str>,
        homepage_url: Option<&str>,
    ) -> Vec<String> {
        let mut urls = Vec::new();

        if let Some(u) = text_urls {
            urls.extend(u.iter().cloned());
        }
        if let Some(u) = other_urls {
            urls.extend(u.iter().cloned());
        }
        if let Some(u) = osi_url {
            let u = u.trim();
            if !u.is_empty() {
                urls.push(u.to_string());
            }
        }
        if let Some(u) = faq_url {
            let u = u.trim();
            if !u.is_empty() {
                urls.push(u.to_string());
            }
        }
        if let Some(u) = homepage_url {
            let u = u.trim();
            if !u.is_empty() {
                urls.push(u.to_string());
            }
        }

        urls
    }

    /// Normalize optional string field.
    ///
    /// Returns `None` for empty strings, `Some(trimmed)` otherwise.
    pub fn normalize_optional_string(s: Option<&str>) -> Option<String> {
        s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    }

    /// Normalize optional string list.
    ///
    /// Returns `None` for empty lists, `Some(list)` with trimmed strings otherwise.
    pub fn normalize_optional_list(list: Option<&[String]>) -> Option<Vec<String>> {
        list.map(|l| {
            l.iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|l: &Vec<String>| !l.is_empty())
    }

    /// Validate that a non-deprecated, non-unknown, non-generic license has text content.
    pub fn validate_text_content(
        text: &str,
        is_deprecated: bool,
        is_unknown: bool,
        is_generic: bool,
    ) -> Result<(), LicenseTextError> {
        if text.trim().is_empty() && !is_deprecated && !is_unknown && !is_generic {
            Err(LicenseTextError::EmptyText)
        } else {
            Ok(())
        }
    }
}

/// Error type for license key validation failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LicenseKeyError {
    CannotExtractKey,
    KeyMismatch {
        filename: String,
        frontmatter: String,
    },
}

impl std::fmt::Display for LicenseKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CannotExtractKey => write!(f, "cannot extract key from license file path"),
            Self::KeyMismatch {
                filename,
                frontmatter,
            } => {
                write!(
                    f,
                    "license key mismatch: filename '{}' vs frontmatter '{}'",
                    filename, frontmatter
                )
            }
        }
    }
}

impl std::error::Error for LicenseKeyError {}

/// Error type for license text validation failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LicenseTextError {
    EmptyText,
}

impl std::fmt::Display for LicenseTextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyText => write!(
                f,
                "license file has empty text content and is not deprecated/unknown/generic"
            ),
        }
    }
}

impl std::error::Error for LicenseTextError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_derive_key() {
        assert_eq!(
            LoadedLicense::derive_key(&PathBuf::from("licenses/mit.LICENSE")),
            Ok("mit".to_string())
        );
        assert_eq!(
            LoadedLicense::derive_key(&PathBuf::from("/path/to/apache-2.0.LICENSE")),
            Ok("apache-2.0".to_string())
        );
        assert_eq!(
            LoadedLicense::derive_key(&PathBuf::from("no-extension")),
            Ok("no-extension".to_string())
        );
        assert_eq!(
            LoadedLicense::derive_key(&PathBuf::from("/")),
            Err(LicenseKeyError::CannotExtractKey)
        );
    }

    #[test]
    fn test_validate_key_match() {
        assert!(LoadedLicense::validate_key_match("mit", Some("mit")).is_ok());
        assert!(LoadedLicense::validate_key_match("mit", None).is_ok());
        assert_eq!(
            LoadedLicense::validate_key_match("mit", Some("apache")),
            Err(LicenseKeyError::KeyMismatch {
                filename: "mit".to_string(),
                frontmatter: "apache".to_string()
            })
        );
    }

    #[test]
    fn test_derive_name() {
        assert_eq!(
            LoadedLicense::derive_name(Some("MIT License"), None, "mit"),
            "MIT License"
        );
        assert_eq!(LoadedLicense::derive_name(None, Some("MIT"), "mit"), "MIT");
        assert_eq!(
            LoadedLicense::derive_name(Some("  MIT License  "), None, "mit"),
            "MIT License"
        );
        assert_eq!(LoadedLicense::derive_name(None, None, "mit"), "mit");
        assert_eq!(
            LoadedLicense::derive_name(Some(""), Some("Short"), "key"),
            "Short"
        );
        assert_eq!(LoadedLicense::derive_name(Some("   "), None, "key"), "key");
    }

    #[test]
    fn test_merge_reference_urls() {
        let text_urls = vec!["https://example.com/text".to_string()];
        let other_urls = vec!["https://example.com/other".to_string()];

        let urls = LoadedLicense::merge_reference_urls(
            Some(&text_urls),
            Some(&other_urls),
            Some("https://opensource.org/licenses/MIT"),
            Some("https://example.com/faq"),
            Some("https://example.com/home"),
        );
        assert_eq!(urls.len(), 5);
        assert_eq!(urls[0], "https://example.com/text");
        assert_eq!(urls[1], "https://example.com/other");
        assert_eq!(urls[2], "https://opensource.org/licenses/MIT");
        assert_eq!(urls[3], "https://example.com/faq");
        assert_eq!(urls[4], "https://example.com/home");
    }

    #[test]
    fn test_merge_reference_urls_empty() {
        let urls = LoadedLicense::merge_reference_urls(None, None, None, None, None);
        assert!(urls.is_empty());
    }

    #[test]
    fn test_merge_reference_urls_trims_whitespace() {
        let urls = LoadedLicense::merge_reference_urls(
            None,
            None,
            Some("  https://example.com  "),
            None,
            None,
        );
        assert_eq!(urls, vec!["https://example.com"]);
    }

    #[test]
    fn test_normalize_optional_string() {
        assert_eq!(LoadedLicense::normalize_optional_string(None), None);
        assert_eq!(LoadedLicense::normalize_optional_string(Some("")), None);
        assert_eq!(LoadedLicense::normalize_optional_string(Some("   ")), None);
        assert_eq!(
            LoadedLicense::normalize_optional_string(Some("hello")),
            Some("hello".to_string())
        );
        assert_eq!(
            LoadedLicense::normalize_optional_string(Some("  hello  ")),
            Some("hello".to_string())
        );
    }

    #[test]
    fn test_normalize_optional_list() {
        assert_eq!(LoadedLicense::normalize_optional_list(None), None);
        assert_eq!(LoadedLicense::normalize_optional_list(Some(&[])), None);
        assert_eq!(
            LoadedLicense::normalize_optional_list(Some(&["a".to_string(), "b".to_string()])),
            Some(vec!["a".to_string(), "b".to_string()])
        );
    }

    #[test]
    fn test_validate_text_content() {
        assert!(LoadedLicense::validate_text_content("some text", false, false, false).is_ok());
        assert!(LoadedLicense::validate_text_content("", true, false, false).is_ok());
        assert!(LoadedLicense::validate_text_content("", false, true, false).is_ok());
        assert!(LoadedLicense::validate_text_content("", false, false, true).is_ok());
        assert_eq!(
            LoadedLicense::validate_text_content("", false, false, false),
            Err(LicenseTextError::EmptyText)
        );
        assert_eq!(
            LoadedLicense::validate_text_content("   ", false, false, false),
            Err(LicenseTextError::EmptyText)
        );
    }

    #[test]
    fn test_serde_roundtrip() {
        let license = LoadedLicense {
            key: "mit".to_string(),
            short_name: Some("MIT".to_string()),
            name: "MIT License".to_string(),
            language: Some("en".to_string()),
            spdx_license_key: Some("MIT".to_string()),
            other_spdx_license_keys: vec![],
            category: Some("Permissive".to_string()),
            owner: Some("Open Source Initiative".to_string()),
            homepage_url: Some("https://opensource.org/licenses/MIT".to_string()),
            text: "MIT License text".to_string(),
            reference_urls: vec!["https://opensource.org/licenses/MIT".to_string()],
            osi_license_key: Some("MIT".to_string()),
            text_urls: vec!["https://opensource.org/licenses/MIT".to_string()],
            osi_url: Some("https://opensource.org/licenses/MIT".to_string()),
            faq_url: None,
            other_urls: vec![],
            notes: Some("Test note".to_string()),
            is_deprecated: false,
            is_exception: false,
            is_unknown: false,
            is_generic: false,
            replaced_by: vec![],
            minimum_coverage: None,
            standard_notice: None,
            ignorable_copyrights: None,
            ignorable_holders: None,
            ignorable_authors: None,
            ignorable_urls: None,
            ignorable_emails: None,
        };

        let json = serde_json::to_string(&license).unwrap();
        let deserialized: LoadedLicense = serde_json::from_str(&json).unwrap();
        assert_eq!(license, deserialized);
    }
}
