//! License expression parsing and manipulation.
//!
//! This module provides a parser for ScanCode license expressions, supporting:
//! - ScanCode license keys (e.g., `mit`, `gpl-2.0-plus`, `apache-2.0`)
//! - SPDX operators: `AND`, `OR`, `WITH` (case-insensitive)
//! - Parenthetical grouping
//! - The `LicenseRef-scancode-*` format for non-SPDX licenses
//!
//! The parser converts license expression strings into an AST (Abstract Syntax Tree)
//! and provides functions for validation and simplification.

mod parse;
mod simplify;

pub use parse::parse_expression;
pub use simplify::{
    combine_expressions_and, combine_expressions_or, expression_to_string, licensing_contains,
    simplify_expression,
};

/// Error type for license expression parsing.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::enum_variant_names)]
pub enum ParseError {
    /// Empty expression
    EmptyExpression,

    /// Unexpected token at position
    UnexpectedToken { token: String, position: usize },

    /// Mismatched parentheses
    MismatchedParentheses,

    /// Generic parse error with message
    ParseError(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyExpression => write!(f, "Empty license expression"),
            Self::UnexpectedToken { token, position } => {
                write!(f, "Unexpected token '{}' at position {}", token, position)
            }
            Self::MismatchedParentheses => write!(f, "Mismatched parentheses"),
            Self::ParseError(msg) => write!(f, "Parse error: {}", msg),
        }
    }
}

impl std::error::Error for ParseError {}

/// A parsed license expression represented as an AST.
#[derive(Debug, Clone, PartialEq)]
pub enum LicenseExpression {
    /// A single license key
    License(String),

    /// A LicenseRef-scancode-* reference
    LicenseRef(String),

    /// AND operation: left AND right
    And {
        left: Box<LicenseExpression>,
        right: Box<LicenseExpression>,
    },

    /// OR operation: left OR right
    Or {
        left: Box<LicenseExpression>,
        right: Box<LicenseExpression>,
    },

    /// WITH operation: left WITH right (exception)
    With {
        left: Box<LicenseExpression>,
        right: Box<LicenseExpression>,
    },
}

impl LicenseExpression {
    /// Extract all license keys from the expression.
    #[allow(dead_code)]
    pub fn license_keys(&self) -> Vec<String> {
        let mut keys = Vec::new();
        self.collect_keys(&mut keys);
        keys.sort();
        keys.dedup();
        keys
    }

    #[allow(dead_code)]
    fn collect_keys(&self, keys: &mut Vec<String>) {
        match self {
            Self::License(key) => keys.push(key.clone()),
            Self::LicenseRef(key) => keys.push(key.clone()),
            Self::And { left, right } | Self::Or { left, right } | Self::With { left, right } => {
                left.collect_keys(keys);
                right.collect_keys(keys);
            }
        }
    }

    /// Create an AND expression combining multiple expressions.
    pub fn and(expressions: Vec<LicenseExpression>) -> Option<LicenseExpression> {
        if expressions.is_empty() {
            None
        } else if expressions.len() == 1 {
            Some(expressions.into_iter().next().unwrap())
        } else {
            let mut iter = expressions.into_iter();
            let mut result = iter.next().unwrap();
            for expr in iter {
                result = LicenseExpression::And {
                    left: Box::new(result),
                    right: Box::new(expr),
                };
            }
            Some(result)
        }
    }

    /// Create an OR expression combining multiple expressions.
    pub fn or(expressions: Vec<LicenseExpression>) -> Option<LicenseExpression> {
        if expressions.is_empty() {
            None
        } else if expressions.len() == 1 {
            Some(expressions.into_iter().next().unwrap())
        } else {
            let mut iter = expressions.into_iter();
            let mut result = iter.next().unwrap();
            for expr in iter {
                result = LicenseExpression::Or {
                    left: Box::new(result),
                    right: Box::new(expr),
                };
            }
            Some(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_and_helper_empty() {
        let result = LicenseExpression::and(vec![]);
        assert!(result.is_none());
    }

    #[test]
    fn test_and_helper_single() {
        let expr = LicenseExpression::License("mit".to_string());
        let result = LicenseExpression::and(vec![expr.clone()]).unwrap();
        assert_eq!(result, expr);
    }

    #[test]
    fn test_and_helper_multiple() {
        let exprs = vec![
            LicenseExpression::License("mit".to_string()),
            LicenseExpression::License("apache-2.0".to_string()),
        ];
        let result = LicenseExpression::and(exprs).unwrap();
        assert!(matches!(result, LicenseExpression::And { .. }));
    }

    #[test]
    fn test_or_helper_empty() {
        let result = LicenseExpression::or(vec![]);
        assert!(result.is_none());
    }

    #[test]
    fn test_or_helper_single() {
        let expr = LicenseExpression::License("mit".to_string());
        let result = LicenseExpression::or(vec![expr.clone()]).unwrap();
        assert_eq!(result, expr);
    }

    #[test]
    fn test_or_helper_multiple() {
        let exprs = vec![
            LicenseExpression::License("mit".to_string()),
            LicenseExpression::License("apache-2.0".to_string()),
        ];
        let result = LicenseExpression::or(exprs).unwrap();
        assert!(matches!(result, LicenseExpression::Or { .. }));
    }

    #[test]
    fn test_validate_expression_valid() {
        let expr = parse_expression("MIT AND Apache-2.0").unwrap();
        let mut known = HashSet::new();
        known.insert("mit".to_string());
        known.insert("apache-2.0".to_string());

        let unknown: Vec<_> = expr
            .license_keys()
            .into_iter()
            .filter(|key| !known.contains(key))
            .collect();
        assert!(unknown.is_empty());
    }

    #[test]
    fn test_validate_expression_unknown_keys() {
        let expr = parse_expression("MIT AND UnknownKey").unwrap();
        let mut known = HashSet::new();
        known.insert("mit".to_string());

        let unknown: Vec<_> = expr
            .license_keys()
            .into_iter()
            .filter(|key| !known.contains(key))
            .collect();
        assert_eq!(unknown, vec!["unknownkey".to_string()]);
    }

    #[test]
    fn test_validate_expression_empty_known_keys() {
        let expr = parse_expression("MIT AND Apache-2.0").unwrap();
        let known: HashSet<String> = HashSet::new();

        let unknown: Vec<_> = expr
            .license_keys()
            .into_iter()
            .filter(|key| !known.contains(key))
            .collect();
        assert_eq!(unknown.len(), 2);
    }
}
