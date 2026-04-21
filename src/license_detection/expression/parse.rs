// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! License expression parsing implementation.

use super::{LicenseExpression, ParseError};

/// Token in a license expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum Token {
    /// License key
    License(String),

    /// Operator: AND
    And,

    /// Operator: OR
    Or,

    /// Operator: WITH
    With,

    /// Opening parenthesis
    LeftParen,

    /// Closing parenthesis
    RightParen,
}

/// Parse a license expression string into a structured expression.
///
/// # Arguments
/// * `expr` - The license expression string to parse
///
/// # Returns
/// Ok with parsed LicenseExpression, or Err with ParseError
///
/// # Examples
/// ```
/// use provenant::license_detection::expression::parse_expression;
///
/// let expr = parse_expression("MIT AND Apache-2.0").unwrap();
/// ```
pub fn parse_expression(expr: &str) -> Result<LicenseExpression, ParseError> {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return Err(ParseError::EmptyExpression);
    }

    let tokens = tokenize(trimmed)?;
    parse_tokens(&tokens)
}

/// Tokenize a license expression string into tokens.
pub(super) fn tokenize(expr: &str) -> Result<Vec<Token>, ParseError> {
    let mut tokens = Vec::new();
    let mut pos = 0;
    let chars: Vec<char> = expr.chars().collect();

    while pos < chars.len() {
        let c = chars[pos];

        if c.is_whitespace() {
            pos += 1;
            continue;
        }

        match c {
            '(' => {
                tokens.push(Token::LeftParen);
                pos += 1;
            }
            ')' => {
                tokens.push(Token::RightParen);
                pos += 1;
            }
            _ => {
                if c.is_alphanumeric() || c == '-' || c == '.' || c == '_' || c == '+' {
                    let start = pos;
                    while pos < chars.len()
                        && (chars[pos].is_alphanumeric()
                            || chars[pos] == '-'
                            || chars[pos] == '.'
                            || chars[pos] == '_'
                            || chars[pos] == '+')
                    {
                        pos += 1;
                    }
                    let text: String = chars[start..pos].iter().collect();
                    let token = match_text_to_token(&text);
                    tokens.push(token);
                } else {
                    return Err(ParseError::UnexpectedToken {
                        token: c.to_string(),
                        position: pos,
                    });
                }
            }
        }
    }

    Ok(tokens)
}

/// Match text to appropriate token.
fn match_text_to_token(text: &str) -> Token {
    let text_upper = text.to_uppercase();
    match text_upper.as_str() {
        "AND" => Token::And,
        "OR" => Token::Or,
        "WITH" => Token::With,
        _ => Token::License(text.to_lowercase()),
    }
}

/// Parse tokens into a LicenseExpression using recursive descent.
pub(super) fn parse_tokens(tokens: &[Token]) -> Result<LicenseExpression, ParseError> {
    if tokens.is_empty() {
        return Err(ParseError::EmptyExpression);
    }

    let (expr, remaining) = parse_or(tokens)?;
    if !remaining.is_empty() {
        return Err(ParseError::ParseError(format!(
            "Unexpected tokens after parsing: {:?}",
            remaining
        )));
    }

    Ok(expr)
}

/// Parse OR expressions (lowest precedence).
pub(super) fn parse_or(tokens: &[Token]) -> Result<(LicenseExpression, &[Token]), ParseError> {
    let (mut expr, mut remaining) = parse_and(tokens)?;

    while let Some(Token::Or) = remaining.first() {
        remaining = &remaining[1..];
        let (right, rest) = parse_and(remaining)?;
        expr = LicenseExpression::Or {
            left: Box::new(expr),
            right: Box::new(right),
        };
        remaining = rest;
    }

    Ok((expr, remaining))
}

/// Parse AND expressions (medium precedence).
pub(super) fn parse_and(tokens: &[Token]) -> Result<(LicenseExpression, &[Token]), ParseError> {
    let (mut expr, mut remaining) = parse_with(tokens)?;

    while let Some(Token::And) = remaining.first() {
        remaining = &remaining[1..];
        let (right, rest) = parse_with(remaining)?;
        expr = LicenseExpression::And {
            left: Box::new(expr),
            right: Box::new(right),
        };
        remaining = rest;
    }

    Ok((expr, remaining))
}

/// Parse WITH expressions (highest precedence for operators).
pub(super) fn parse_with(tokens: &[Token]) -> Result<(LicenseExpression, &[Token]), ParseError> {
    let (mut expr, mut remaining) = parse_primary(tokens)?;

    while let Some(Token::With) = remaining.first() {
        remaining = &remaining[1..];
        let (right, rest) = parse_primary(remaining)?;
        expr = LicenseExpression::With {
            left: Box::new(expr),
            right: Box::new(right),
        };
        remaining = rest;
    }

    Ok((expr, remaining))
}

/// Parse primary expressions (license keys or parenthesized expressions).
pub(super) fn parse_primary(tokens: &[Token]) -> Result<(LicenseExpression, &[Token]), ParseError> {
    if tokens.is_empty() {
        return Err(ParseError::EmptyExpression);
    }

    match &tokens[0] {
        Token::LeftParen => {
            if tokens.len() < 2 {
                return Err(ParseError::MismatchedParentheses);
            }
            let (expr, remaining) = parse_or(&tokens[1..])?;
            if remaining.is_empty() || remaining[0] != Token::RightParen {
                return Err(ParseError::MismatchedParentheses);
            }
            Ok((expr, &remaining[1..]))
        }
        Token::License(key) => {
            let expr = if key.starts_with("licenseref-") {
                LicenseExpression::LicenseRef(key.clone())
            } else {
                LicenseExpression::License(key.clone())
            };
            Ok((expr, &tokens[1..]))
        }
        Token::RightParen => Err(ParseError::MismatchedParentheses),
        Token::And | Token::Or | Token::With => Err(ParseError::ParseError(format!(
            "Unexpected operator at start: {:?}",
            tokens[0]
        ))),
    }
}

#[cfg(test)]
#[path = "parse_test.rs"]
mod tests;
