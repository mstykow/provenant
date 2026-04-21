// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Loader-stage rule type.
//!
//! This module defines `LoadedRule`, which represents a parsed and normalized
//! rule file (.RULE or .LICENSE) before it is converted to a runtime `Rule`.
//!
//! Loader-stage responsibilities include:
//! - Text trimming and normalization
//! - Fallback/default handling derived only from one file
//! - Empty-vector to `None` cleanup
//! - File-local validation
//! - False-positive handling for missing `license_expression`

use serde::{Deserialize, Serialize};

use super::RuleKind;

/// Loader-stage representation of a rule.
///
/// This struct contains parsed and normalized data from a .RULE or .LICENSE file.
/// It is serialized at build time and deserialized at runtime, then converted
/// to a runtime `Rule` during the build stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadedRule {
    /// Unique identifier derived from the filename (e.g., "mit.LICENSE").
    pub identifier: String,

    /// License expression string using SPDX syntax and ScanCode license keys.
    /// For false-positive rules with no source expression, this is set to "unknown".
    pub license_expression: String,

    /// Pattern text to match, trimmed and normalized.
    pub text: String,

    /// Classification of this rule, derived from source rule-kind booleans.
    pub rule_kind: RuleKind,

    /// True if exact matches to this rule are false positives.
    pub is_false_positive: bool,

    /// True if this rule text is a required phrase.
    pub is_required_phrase: bool,

    #[serde(default)]
    pub skip_for_required_phrase_generation: bool,

    /// Relevance score 0-100 (100 is most relevant).
    /// Stored as Option to distinguish between explicit 100 and default 100.
    pub relevance: Option<u8>,

    /// Minimum match coverage percentage (0-100) if specified.
    pub minimum_coverage: Option<u8>,

    /// True if minimum_coverage was explicitly stored in source frontmatter.
    pub has_stored_minimum_coverage: bool,

    /// Tokens must appear in order if true.
    pub is_continuous: bool,

    /// Filenames where this rule should be considered.
    pub referenced_filenames: Option<Vec<String>>,

    /// URLs that should be ignored when found in this rule text.
    pub ignorable_urls: Option<Vec<String>>,

    /// Emails that should be ignored when found in this rule text.
    pub ignorable_emails: Option<Vec<String>>,

    /// Copyrights that should be ignored when found in this rule text.
    pub ignorable_copyrights: Option<Vec<String>>,

    /// Holder names that should be ignored when found in this rule text.
    pub ignorable_holders: Option<Vec<String>>,

    /// Author names that should be ignored when found in this rule text.
    pub ignorable_authors: Option<Vec<String>>,

    /// Programming language for the rule if specified.
    pub language: Option<String>,

    /// Free text notes.
    pub notes: Option<String>,

    /// Whether this rule is deprecated.
    pub is_deprecated: bool,

    #[serde(default)]
    pub replaced_by: Vec<String>,
}

/// Loader-stage normalization functions for rule data.
impl LoadedRule {
    /// Derive identifier from filename.
    ///
    /// Returns the filename as-is, which serves as the unique identifier.
    pub fn derive_identifier(filename: &str) -> String {
        filename.to_string()
    }

    /// Derive rule kind from source rule-kind booleans.
    ///
    /// Returns an error if multiple flags are set.
    pub fn derive_rule_kind(
        is_license_text: bool,
        is_license_notice: bool,
        is_license_reference: bool,
        is_license_tag: bool,
        is_license_intro: bool,
        is_license_clue: bool,
    ) -> Result<RuleKind, RuleKindError> {
        RuleKind::from_rule_flags(
            is_license_text,
            is_license_notice,
            is_license_reference,
            is_license_tag,
            is_license_intro,
            is_license_clue,
        )
        .map_err(|_| RuleKindError::MultipleFlagsSet)
    }

    /// Normalize license expression.
    ///
    /// - Strips trivial outer parentheses
    /// - For false-positive rules with no expression, returns "unknown"
    /// - For non-false-positive rules with no expression, returns an error
    pub fn normalize_license_expression(
        expression: Option<&str>,
        is_false_positive: bool,
    ) -> Result<String, LicenseExpressionError> {
        match expression {
            Some(expr) if !expr.trim().is_empty() => {
                Ok(normalize_trivial_outer_parens(expr.trim()))
            }
            Some(_) => {
                if is_false_positive {
                    Ok("unknown".to_string())
                } else {
                    Err(LicenseExpressionError::EmptyExpression)
                }
            }
            None => {
                if is_false_positive {
                    Ok("unknown".to_string())
                } else {
                    Err(LicenseExpressionError::MissingExpression)
                }
            }
        }
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

    /// Validate rule-kind flags against false_positive flag.
    ///
    /// - False-positive rules must NOT have any is_license_* flags set
    /// - Non-false-positive rules MUST have exactly one is_license_* flag set
    pub fn validate_rule_kind_flags(
        rule_kind: RuleKind,
        is_false_positive: bool,
    ) -> Result<(), RuleKindError> {
        if is_false_positive && rule_kind != RuleKind::None {
            return Err(RuleKindError::FalsePositiveWithFlags);
        }
        if !is_false_positive && rule_kind == RuleKind::None {
            return Err(RuleKindError::NoFlagsSet);
        }
        Ok(())
    }
}

/// Error type for rule-kind validation failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleKindError {
    MultipleFlagsSet,
    NoFlagsSet,
    FalsePositiveWithFlags,
}

impl std::fmt::Display for RuleKindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MultipleFlagsSet => write!(f, "rule has multiple is_license_* flags set"),
            Self::NoFlagsSet => write!(f, "non-false-positive rule has no is_license_* flags set"),
            Self::FalsePositiveWithFlags => {
                write!(f, "false-positive rule cannot have is_license_* flags set")
            }
        }
    }
}

impl std::error::Error for RuleKindError {}

/// Error type for license expression validation failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LicenseExpressionError {
    MissingExpression,
    EmptyExpression,
}

impl std::fmt::Display for LicenseExpressionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingExpression => write!(
                f,
                "license_expression is required for non-false-positive rules"
            ),
            Self::EmptyExpression => write!(
                f,
                "license_expression cannot be empty for non-false-positive rules"
            ),
        }
    }
}

impl std::error::Error for LicenseExpressionError {}

/// Check if a string has trivial outer parentheses.
///
/// Trivial outer parentheses are a single pair of parens that wrap the entire
/// expression without any other top-level parens.
fn has_trivial_outer_parens(s: &str) -> bool {
    let trimmed = s.trim();
    if !trimmed.starts_with('(') || !trimmed.ends_with(')') {
        return false;
    }
    let mut depth = 0;
    let chars: Vec<char> = trimmed.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if *c == '(' {
            depth += 1;
        } else if *c == ')' {
            depth -= 1;
            if depth == 0 && i < chars.len() - 1 {
                return false;
            }
        }
    }
    depth == 0
}

/// Normalize license expression by removing trivial outer parentheses.
///
/// This recursively strips outer parens that wrap the entire expression.
fn normalize_trivial_outer_parens(expr: &str) -> String {
    let trimmed = expr.trim();
    if has_trivial_outer_parens(trimmed) {
        let inner = &trimmed[1..trimmed.len() - 1];
        normalize_trivial_outer_parens(inner)
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_identifier() {
        assert_eq!(LoadedRule::derive_identifier("mit.LICENSE"), "mit.LICENSE");
        assert_eq!(
            LoadedRule::derive_identifier("gpl-2.0_12.RULE"),
            "gpl-2.0_12.RULE"
        );
    }

    #[test]
    fn test_derive_rule_kind_single_flag() {
        assert_eq!(
            LoadedRule::derive_rule_kind(true, false, false, false, false, false),
            Ok(RuleKind::Text)
        );
        assert_eq!(
            LoadedRule::derive_rule_kind(false, true, false, false, false, false),
            Ok(RuleKind::Notice)
        );
        assert_eq!(
            LoadedRule::derive_rule_kind(false, false, true, false, false, false),
            Ok(RuleKind::Reference)
        );
        assert_eq!(
            LoadedRule::derive_rule_kind(false, false, false, true, false, false),
            Ok(RuleKind::Tag)
        );
        assert_eq!(
            LoadedRule::derive_rule_kind(false, false, false, false, true, false),
            Ok(RuleKind::Intro)
        );
        assert_eq!(
            LoadedRule::derive_rule_kind(false, false, false, false, false, true),
            Ok(RuleKind::Clue)
        );
    }

    #[test]
    fn test_derive_rule_kind_none() {
        assert_eq!(
            LoadedRule::derive_rule_kind(false, false, false, false, false, false),
            Ok(RuleKind::None)
        );
    }

    #[test]
    fn test_derive_rule_kind_multiple_flags() {
        assert_eq!(
            LoadedRule::derive_rule_kind(true, true, false, false, false, false),
            Err(RuleKindError::MultipleFlagsSet)
        );
    }

    #[test]
    fn test_normalize_license_expression_with_value() {
        assert_eq!(
            LoadedRule::normalize_license_expression(Some("mit"), false),
            Ok("mit".to_string())
        );
    }

    #[test]
    fn test_normalize_license_expression_false_positive_fallback() {
        assert_eq!(
            LoadedRule::normalize_license_expression(None, true),
            Ok("unknown".to_string())
        );
        assert_eq!(
            LoadedRule::normalize_license_expression(Some(""), true),
            Ok("unknown".to_string())
        );
        assert_eq!(
            LoadedRule::normalize_license_expression(Some("   "), true),
            Ok("unknown".to_string())
        );
    }

    #[test]
    fn test_normalize_license_expression_missing_error() {
        assert_eq!(
            LoadedRule::normalize_license_expression(None, false),
            Err(LicenseExpressionError::MissingExpression)
        );
    }

    #[test]
    fn test_normalize_license_expression_empty_error() {
        assert_eq!(
            LoadedRule::normalize_license_expression(Some(""), false),
            Err(LicenseExpressionError::EmptyExpression)
        );
    }

    #[test]
    fn test_normalize_trivial_outer_parens() {
        assert_eq!(normalize_trivial_outer_parens("mit"), "mit");
        assert_eq!(normalize_trivial_outer_parens("(mit)"), "mit");
        assert_eq!(normalize_trivial_outer_parens("((mit))"), "mit");
        assert_eq!(
            normalize_trivial_outer_parens("(mit OR apache-2.0)"),
            "mit OR apache-2.0"
        );
        assert_eq!(
            normalize_trivial_outer_parens("(mit) OR (apache-2.0)"),
            "(mit) OR (apache-2.0)"
        );
    }

    #[test]
    fn test_normalize_optional_string() {
        assert_eq!(LoadedRule::normalize_optional_string(None), None);
        assert_eq!(LoadedRule::normalize_optional_string(Some("")), None);
        assert_eq!(LoadedRule::normalize_optional_string(Some("   ")), None);
        assert_eq!(
            LoadedRule::normalize_optional_string(Some("hello")),
            Some("hello".to_string())
        );
        assert_eq!(
            LoadedRule::normalize_optional_string(Some("  hello  ")),
            Some("hello".to_string())
        );
    }

    #[test]
    fn test_normalize_optional_list() {
        assert_eq!(LoadedRule::normalize_optional_list(None), None);
        assert_eq!(LoadedRule::normalize_optional_list(Some(&[])), None);
        assert_eq!(
            LoadedRule::normalize_optional_list(Some(&["a".to_string(), "b".to_string()])),
            Some(vec!["a".to_string(), "b".to_string()])
        );
        assert_eq!(
            LoadedRule::normalize_optional_list(Some(&["  a  ".to_string(), "  b  ".to_string()])),
            Some(vec!["a".to_string(), "b".to_string()])
        );
        assert_eq!(
            LoadedRule::normalize_optional_list(Some(&["".to_string(), "  ".to_string()])),
            None
        );
    }

    #[test]
    fn test_validate_rule_kind_flags() {
        assert!(LoadedRule::validate_rule_kind_flags(RuleKind::Text, false).is_ok());
        assert_eq!(
            LoadedRule::validate_rule_kind_flags(RuleKind::None, false),
            Err(RuleKindError::NoFlagsSet)
        );
        assert!(LoadedRule::validate_rule_kind_flags(RuleKind::None, true).is_ok());
        assert_eq!(
            LoadedRule::validate_rule_kind_flags(RuleKind::Text, true),
            Err(RuleKindError::FalsePositiveWithFlags)
        );
    }

    #[test]
    fn test_serde_roundtrip() {
        let rule = LoadedRule {
            identifier: "mit.LICENSE".to_string(),
            license_expression: "mit".to_string(),
            text: "MIT License".to_string(),
            rule_kind: RuleKind::Text,
            is_false_positive: false,
            is_required_phrase: false,
            skip_for_required_phrase_generation: false,
            relevance: Some(100),
            minimum_coverage: Some(90),
            has_stored_minimum_coverage: true,
            is_continuous: false,
            referenced_filenames: Some(vec!["MIT.txt".to_string()]),
            ignorable_urls: None,
            ignorable_emails: None,
            ignorable_copyrights: None,
            ignorable_holders: None,
            ignorable_authors: None,
            language: None,
            notes: Some("Test note".to_string()),
            is_deprecated: false,
            replaced_by: vec![],
        };

        let json = serde_json::to_string(&rule).unwrap();
        let deserialized: LoadedRule = serde_json::from_str(&json).unwrap();
        assert_eq!(rule, deserialized);
    }
}
