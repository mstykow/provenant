// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

/// Parser metadata for auto-generating documentation.
///
/// This module provides infrastructure for registering parser metadata
/// that is used to automatically generate `docs/SUPPORTED_FORMATS.md`.
///
/// Fields are used by `bin/generate_supported_formats.rs` but not in library code,
/// so we allow dead_code warnings for library builds.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ParserMetadata {
    /// Human-readable description (e.g., "npm package.json manifest")
    pub description: &'static str,
    /// File patterns this parser matches (e.g., ["**/package.json"])
    pub file_patterns: &'static [&'static str],
    /// Package type identifier (e.g., "npm", "pypi", "maven")
    pub package_type: &'static str,
    /// Primary programming language (e.g., "JavaScript", "Python")
    pub primary_language: &'static str,
    /// Optional documentation URL
    pub documentation_url: Option<&'static str>,
}

inventory::collect!(ParserMetadata);

/// Registers parser metadata for documentation generation.
///
/// # Example
///
/// ```no_run
/// use provenant::register_parser;
///
/// register_parser!(
///     "npm package.json manifest",
///     &["**/package.json"],
///     "npm",
///     "JavaScript",
///     Some("https://docs.npmjs.com/cli/v10/configuring-npm/package-json"),
/// );
/// ```
#[macro_export]
macro_rules! register_parser {
    ($description:expr, $patterns:expr, $package_type:expr, $language:expr, $docs_url:expr $(,)?) => {
        inventory::submit! {
            $crate::parsers::metadata::ParserMetadata {
                description: $description,
                file_patterns: $patterns,
                package_type: $package_type,
                primary_language: $language,
                documentation_url: $docs_url,
            }
        }
    };
}
