// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Parser for third-party attribution README files.
//!
//! Extracts package metadata from semi-structured README files used to document
//! third-party dependencies in Android, Chromium, Facebook, Google, and similar codebases.
//!
//! # Supported Formats
//! - README.android
//! - README.chromium
//! - README.facebook
//! - README.google
//! - README.thirdparty
//!
//! # Key Features
//! - Key:value pair extraction (both `:` and `=` separators)
//! - Parent directory name fallback for packages without explicit names
//! - Field name mapping to standardized PackageData fields
//!
//! # Implementation Notes
//! - Keys are matched case-insensitively
//! - Lines without valid separators are skipped
//! - Multiple URL-related keys map to homepage_url (repo, source, upstream, etc.)
//! - Separator precedence: the first separator (`:` or `=`) on each line is used

use crate::models::PackageData;
use crate::models::{DatasourceId, PackageType};
use crate::parser_warn as warn;
use crate::parsers::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};
use std::path::Path;

use super::PackageParser;

/// README attribution file parser.
///
/// Extracts package metadata from semi-structured README files commonly used
/// to document third-party dependencies in large codebases.
pub struct ReadmeParser;

impl PackageParser for ReadmeParser {
    const PACKAGE_TYPE: PackageType = PackageType::Readme;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| {
            let name = name.to_string_lossy().to_lowercase();
            matches!(
                name.as_str(),
                "readme.android"
                    | "readme.chromium"
                    | "readme.facebook"
                    | "readme.google"
                    | "readme.thirdparty"
            )
        })
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read README file at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        let mut pkg = default_package_data();

        // Parse key:value pairs
        for line in content.lines().take(MAX_ITERATION_COUNT) {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let split_colon = line.split_once(':');
            let split_equals = line.split_once('=');

            let (key, value) = match (split_colon, split_equals) {
                (Some((ck, cv)), Some((ek, _))) if ck.len() <= ek.len() => (ck.trim(), cv.trim()),
                (_, Some((ek, ev))) => (ek.trim(), ev.trim()),
                (Some((ck, cv)), None) => (ck.trim(), cv.trim()),
                (None, None) => continue,
            };

            if key.is_empty() || value.is_empty() {
                continue;
            }

            // Map README field to PackageData field (case-insensitive)
            let key_lower = key.to_lowercase();
            match key_lower.as_str() {
                "name" | "project" => {
                    pkg.name = Some(truncate_field(value.to_string()));
                }
                "version" => {
                    pkg.version = Some(truncate_field(value.to_string()));
                }
                "copyright" => {
                    pkg.copyright = Some(truncate_field(value.to_string()));
                }
                "download link" | "downloaded from" => {
                    pkg.download_url = Some(truncate_field(value.to_string()));
                }
                "homepage" | "website" | "repo" | "source" | "upstream" | "url" | "project url" => {
                    pkg.homepage_url = Some(truncate_field(value.to_string()));
                }
                "licence" | "license" => {
                    pkg.extracted_license_statement = Some(truncate_field(value.to_string()));
                }
                _ => {
                    // Unrecognized field, skip
                }
            }
        }

        // Fallback: use parent directory name if no name was found
        if pkg.name.is_none()
            && let Some(parent) = path.parent()
            && let Some(parent_name) = parent.file_name()
        {
            pkg.name = Some(truncate_field(parent_name.to_string_lossy().to_string()));
        }

        vec![pkg]
    }
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(ReadmeParser::PACKAGE_TYPE),
        datasource_id: Some(DatasourceId::Readme),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_is_match_android() {
        let valid = PathBuf::from("/some/path/README.android");
        assert!(ReadmeParser::is_match(&valid));
    }

    #[test]
    fn test_is_match_chromium() {
        let valid = PathBuf::from("/some/path/README.chromium");
        assert!(ReadmeParser::is_match(&valid));
    }

    #[test]
    fn test_is_match_facebook() {
        let valid = PathBuf::from("/some/path/README.facebook");
        assert!(ReadmeParser::is_match(&valid));
    }

    #[test]
    fn test_is_match_google() {
        let valid = PathBuf::from("/some/path/README.google");
        assert!(ReadmeParser::is_match(&valid));
    }

    #[test]
    fn test_is_match_thirdparty() {
        let valid = PathBuf::from("/some/path/README.thirdparty");
        assert!(ReadmeParser::is_match(&valid));
    }

    #[test]
    fn test_is_match_case_insensitive() {
        let upper = PathBuf::from("/some/path/README.CHROMIUM");
        let mixed = PathBuf::from("/some/path/README.ChRoMiUm");
        assert!(ReadmeParser::is_match(&upper));
        assert!(ReadmeParser::is_match(&mixed));
    }

    #[test]
    fn test_is_match_negative_cases() {
        let readme_md = PathBuf::from("/some/path/README.md");
        let readme_txt = PathBuf::from("/some/path/README.txt");
        let readme = PathBuf::from("/some/path/README");
        let other = PathBuf::from("/some/path/INSTALL.txt");

        assert!(!ReadmeParser::is_match(&readme_md));
        assert!(!ReadmeParser::is_match(&readme_txt));
        assert!(!ReadmeParser::is_match(&readme));
        assert!(!ReadmeParser::is_match(&other));
    }

    #[test]
    fn test_extract_chromium_format() {
        let path = PathBuf::from("testdata/readme/chromium/third_party/example/README.chromium");
        let pkg = ReadmeParser::extract_first_package(&path);

        assert_eq!(pkg.package_type, Some(PackageType::Readme));
        assert_eq!(pkg.name, Some("Example Library".to_string()));
        assert_eq!(pkg.version, Some("2.1.0".to_string()));
        assert_eq!(pkg.homepage_url, Some("https://example.com".to_string()));
        assert_eq!(pkg.extracted_license_statement, Some("MIT".to_string()));
        assert_eq!(pkg.datasource_id, Some(DatasourceId::Readme));
    }

    #[test]
    fn test_extract_android_format() {
        let path = PathBuf::from("testdata/readme/android/third_party/example/README.android");
        let pkg = ReadmeParser::extract_first_package(&path);

        assert_eq!(pkg.name, Some("Android Example".to_string()));
        assert_eq!(pkg.version, Some("1.0".to_string()));
        assert_eq!(
            pkg.homepage_url,
            Some("https://android.example.com".to_string())
        );
        assert_eq!(pkg.copyright, Some("2024 Google Inc.".to_string()));
    }

    #[test]
    fn test_extract_facebook_format() {
        let path = PathBuf::from("testdata/readme/facebook/third_party/example/README.facebook");
        let pkg = ReadmeParser::extract_first_package(&path);

        assert_eq!(pkg.name, Some("FB Library".to_string()));
        assert_eq!(
            pkg.download_url,
            Some("https://github.com/example/fb-lib".to_string())
        );
        assert_eq!(
            pkg.extracted_license_statement,
            Some("BSD-3-Clause".to_string())
        );
    }

    #[test]
    fn test_extract_parent_dir_fallback() {
        let path = PathBuf::from("testdata/readme/no-name/third_party/mylib/README.thirdparty");
        let pkg = ReadmeParser::extract_first_package(&path);

        // Should use parent directory name "mylib" since no name field in file
        assert_eq!(pkg.name, Some("mylib".to_string()));
        assert_eq!(pkg.homepage_url, Some("https://example.com".to_string()));
        assert_eq!(pkg.version, Some("3.0".to_string()));
    }

    #[test]
    fn test_extract_equals_separator() {
        let path =
            PathBuf::from("testdata/readme/equals-separator/third_party/eqlib/README.google");
        let pkg = ReadmeParser::extract_first_package(&path);

        assert_eq!(pkg.name, Some("Google Lib".to_string()));
        assert_eq!(
            pkg.homepage_url,
            Some("https://google.example.com".to_string())
        );
        assert_eq!(
            pkg.extracted_license_statement,
            Some("Apache-2.0".to_string())
        );
    }

    #[test]
    fn test_case_insensitive_field_names() {
        let path = PathBuf::from("testdata/readme/chromium/third_party/example/README.chromium");
        let pkg = ReadmeParser::extract_first_package(&path);

        // The test file uses "Name:", "URL:", "Version:", "License:"
        // All should be recognized despite capitalization
        assert!(pkg.name.is_some());
        assert!(pkg.version.is_some());
        assert!(pkg.homepage_url.is_some());
        assert!(pkg.extracted_license_statement.is_some());
    }

    #[test]
    fn test_invalid_file() {
        let nonexistent = PathBuf::from("testdata/readme/nonexistent/README.chromium");
        let pkg = ReadmeParser::extract_first_package(&nonexistent);

        // Should return default data with proper type and datasource
        assert_eq!(pkg.package_type, Some(PackageType::Readme));
        assert_eq!(pkg.datasource_id, Some(DatasourceId::Readme));
    }
}

crate::register_parser!(
    "Third-party attribution README files",
    &[
        "**/README.android",
        "**/README.chromium",
        "**/README.facebook",
        "**/README.google",
        "**/README.thirdparty"
    ],
    "readme",
    "",
    Some(
        "https://github.com/chromium/chromium/blob/main/docs/contributing.md#third_party-components"
    ),
);
