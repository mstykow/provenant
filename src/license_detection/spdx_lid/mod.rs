//! SPDX-License-Identifier detection and parsing.
//!
//! This module handles detection of SPDX license identifier tags in source code,
//! such as "SPDX-License-Identifier: MIT" or variations with different comment
//! styles and casing.
//!
//! Based on Python implementation at:
//! reference/scancode-toolkit/src/licensedcode/match_spdx_lid.py
//!
//! ## Signature
//!
//! The `spdx_lid_match()` function takes `(index, query)` where query contains
//! pre-computed SPDX lines with token positions tracked during tokenization.
//! This enables correct `start_token` and `end_token` values in LicenseMatches.

use regex::Regex;

use crate::license_detection::expression::{
    LicenseExpression, expression_to_string, parse_expression,
};
use crate::license_detection::index::LicenseIndex;
use crate::license_detection::models::{LicenseMatch, MatcherKind};
use crate::license_detection::query::Query;

pub const MATCH_SPDX_ID: MatcherKind = MatcherKind::SpdxId;

#[derive(Debug, Clone, PartialEq)]
enum RecoveryToken {
    LicenseKey(String),
    Keyword(SpdxKeyword),
    Ignored,
}

#[derive(Debug, Clone, PartialEq)]
enum SpdxKeyword {
    And,
    Or,
    With,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BooleanOperator {
    And,
    Or,
}

lazy_static::lazy_static! {
    static ref SPDX_LID_PATTERN: Regex = Regex::new(
        r"(?i)(spd[xz][\-\s]+lin?[cs]en?[sc]es?[\-\s]+identifi?er\s*:? *)"
    ).expect("Invalid SPDX-LID regex");

    static ref NUGET_SPDX_PATTERN: Regex = Regex::new(
        r"(?i)(https?://licenses\.nuget\.org/?)\s*:? *"
    ).expect("Invalid NuGet SPDX regex");
}

pub fn split_spdx_lid(text: &str) -> (Option<String>, String) {
    // Try SPDX pattern first
    if let Some(captures) = SPDX_LID_PATTERN.captures(text)
        && let Some(matched) = captures.get(1)
    {
        let prefix = matched.as_str().to_string();
        let expression = &text[matched.end()..];
        return (Some(prefix), expression.to_string());
    }

    // Try NuGet pattern
    if let Some(captures) = NUGET_SPDX_PATTERN.captures(text)
        && let Some(full_match) = captures.get(0)
    {
        let prefix = &text[..full_match.end()];
        let expression = &text[full_match.end()..];
        return (Some(prefix.to_string()), expression.to_string());
    }

    (None, text.to_string())
}

pub fn clean_spdx_text(text: &str) -> String {
    let mut text = text.to_string();

    text = text.replace("</a>", "");
    text = text.replace("</p>", "");
    text = text.replace("</div>", "");
    text = text.replace("</licenseUrl>", "");

    normalize_spaces(&mut text);

    strip_punctuation(&mut text);
    fix_unbalanced_parens(&mut text);

    if text.contains("\">") {
        let parts: Vec<&str> = text.split("\">").collect();
        if parts.len() > 1 && parts[1].contains(parts[0]) {
            text = parts[0].to_string();
        }
    }

    normalize_spaces(&mut text);

    text
}

fn normalize_spaces(text: &mut String) {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    *text = normalized;
}

fn strip_punctuation(text: &mut String) {
    let punctuation = "!\"#$%&\'*,-./:;<=>?@[\\]^_`{|}~ \t\r\n ";

    while !text.is_empty()
        && text
            .chars()
            .next()
            .map(|c| punctuation.contains(c) || c == ')')
            .unwrap_or(false)
    {
        text.remove(0);
    }

    while !text.is_empty()
        && text
            .chars()
            .last()
            .map(|c| punctuation.contains(c) || c == '(')
            .unwrap_or(false)
    {
        text.pop();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParenBalanceResult {
    Balanced,
    FixedSingleUnbalanced,
    Mismatched,
}

fn fix_unbalanced_parens(text: &mut String) -> ParenBalanceResult {
    let open_count = text.matches('(').count();
    let close_count = text.matches(')').count();

    if open_count == close_count {
        return ParenBalanceResult::Balanced;
    }

    if open_count == 1 && close_count == 0 {
        *text = text.replace('(', " ");
        return ParenBalanceResult::FixedSingleUnbalanced;
    }

    if close_count == 1 && open_count == 0 {
        *text = text.replace(')', " ");
        return ParenBalanceResult::FixedSingleUnbalanced;
    }

    ParenBalanceResult::Mismatched
}

pub(crate) fn normalize_spdx_key(key: &str) -> String {
    key.to_lowercase().replace("_", "-")
}

const DEPRECATED_SPDX_EXPRESSION_SUBS: &[(&str, &str)] = &[
    ("ecos-2.0", "gpl-2.0-plus with ecos-exception-2.0"),
    (
        "gpl-2.0-with-classpath-exception",
        "gpl-2.0-only with classpath-exception-2.0",
    ),
    (
        "gpl-2.0-with-gcc-exception",
        "gpl-2.0-only with gcc-exception-2.0",
    ),
    ("wxwindows", "lgpl-2.0-plus with wxwindows-exception-3.1"),
    (
        "gpl-2.0-with-autoconf-exception",
        "gpl-2.0-only with autoconf-exception-2.0",
    ),
    (
        "gpl-2.0-with-bison-exception",
        "gpl-2.0-only with bison-exception-2.2",
    ),
    (
        "gpl-2.0-with-font-exception",
        "gpl-2.0-only with font-exception-2.0",
    ),
    (
        "gpl-3.0-with-autoconf-exception",
        "gpl-3.0-only with autoconf-exception-3.0",
    ),
    (
        "gpl-3.0-with-gcc-exception",
        "gpl-3.0-only with gcc-exception-3.1",
    ),
];

fn get_deprecated_substitution(spdx_key: &str) -> Option<&'static str> {
    let normalized = normalize_spdx_key(spdx_key);
    for (deprecated, replacement) in DEPRECATED_SPDX_EXPRESSION_SUBS {
        if *deprecated == normalized {
            return Some(*replacement);
        }
    }
    None
}

pub(crate) fn split_license_expression(license_expression: &str) -> Vec<String> {
    let normalized = license_expression.replace(['(', ')'], " ");
    let mut tokens: Vec<String> = Vec::new();

    let mut current = String::new();
    for c in normalized.chars() {
        if c == ' ' {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
        } else {
            current.push(c);
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
        .into_iter()
        .filter(|t| {
            let t_lower = t.to_lowercase();
            !matches!(t_lower.as_str(), "and" | "or" | "with")
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn extract_matched_text_from_lines(
    text: &str,
    start_line: usize,
    end_line: usize,
) -> String {
    if start_line == 0 || end_line == 0 || start_line > end_line {
        return String::new();
    }

    text.lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let line_num = idx + 1;
            if line_num >= start_line && line_num <= end_line {
                Some(line)
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn spdx_lid_match<'a>(query: &Query<'a>) -> Vec<LicenseMatch<'a>> {
    let mut matches = Vec::new();

    for (spdx_text, start_token, end_token) in &query.spdx_lines {
        let (_, expression) = split_spdx_lid(spdx_text);
        let spdx_expression = clean_spdx_text(&expression);

        if spdx_expression.is_empty() {
            continue;
        }

        let lowered = spdx_expression.to_lowercase();
        let resolved_expression = if let Some(sub) = get_deprecated_substitution(&lowered) {
            sub.to_string()
        } else {
            lowered.clone()
        };

        if let Some(license_expression) =
            find_matching_rule_for_expression(query.index, &resolved_expression)
        {
            let matched_length = spdx_expression.len();
            let match_coverage = 100.0;

            let start_line = query.line_for_pos(*start_token).unwrap_or(1);
            let end_line = query.line_for_pos(*end_token).unwrap_or(start_line);

            let rule = query
                .index
                .rid_by_spdx_key
                .get(&license_expression)
                .map(|&rid| &query.index.rules_by_rid[rid])
                .or_else(|| {
                    query
                        .index
                        .unknown_spdx_rid
                        .map(|rid| &query.index.rules_by_rid[rid])
                });

            let rule = match rule {
                Some(r) => r,
                None => continue,
            };

            let score = rule.relevance as f32 / 100.0;

            let license_match = LicenseMatch {
                rule,
                license_expression_spdx: None,
                from_file: None,
                start_line,
                end_line,
                start_token: *start_token,
                end_token: *end_token,
                matcher: MATCH_SPDX_ID,
                score,
                matched_length,
                match_coverage,
                matched_text: None,
                is_from_license: false,
                matched_token_positions: None,
                hilen: 0,
                rule_start_token: 0,
                qspan_positions: None,
                ispan_positions: None,
                hispan_positions: None,
                candidate_resemblance: 0.0,
                candidate_containment: 0.0,
            };

            matches.push(license_match);
        }
    }

    matches
}

pub(crate) fn is_bare_license_list(expression: &str) -> bool {
    let lowered = expression.to_lowercase();
    !lowered.contains(" and ")
        && !lowered.contains(" or ")
        && !lowered.contains(" with ")
        && !expression.contains('(')
        && !expression.contains(')')
}

fn has_invalid_spdx_chars(text: &str) -> bool {
    for c in text.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' => {}
            ' ' | '\t' | '\n' | '\r' => {}
            '(' | ')' | '-' | '.' | '_' | '+' | ':' => {}
            _ => return true,
        }
    }
    false
}

fn tokenize_for_recovery(text: &str) -> Vec<RecoveryToken> {
    // If text contains invalid characters, skip recovery
    if has_invalid_spdx_chars(text) {
        return Vec::new();
    }

    let mut tokens = Vec::new();
    let mut current = String::new();

    for c in text.to_lowercase().chars() {
        match c {
            ' ' | '\t' | '(' | ')' | '\n' | '\r' => {
                if !current.is_empty() {
                    let token = classify_recovery_token(&current);
                    if !matches!(token, RecoveryToken::Ignored) {
                        tokens.push(token);
                    }
                    current.clear();
                }
            }
            _ => current.push(c),
        }
    }

    if !current.is_empty() {
        let token = classify_recovery_token(&current);
        if !matches!(token, RecoveryToken::Ignored) {
            tokens.push(token);
        }
    }

    tokens
}

fn classify_recovery_token(text: &str) -> RecoveryToken {
    match text {
        "and" => RecoveryToken::Keyword(SpdxKeyword::And),
        "or" => RecoveryToken::Keyword(SpdxKeyword::Or),
        "with" => RecoveryToken::Keyword(SpdxKeyword::With),
        _ => {
            if is_likely_license_key(text) {
                RecoveryToken::LicenseKey(text.to_string())
            } else {
                RecoveryToken::Ignored
            }
        }
    }
}

fn is_likely_license_key(text: &str) -> bool {
    if text.len() < 2 {
        return false;
    }

    let has_valid_chars = text
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '.' || c == '+' || c == '_');

    if !has_valid_chars {
        return false;
    }

    let lower = text.to_lowercase();
    if matches!(lower.as_str(), "and" | "or" | "with") {
        return false;
    }

    true
}

fn is_spdx_exception(text: &str) -> bool {
    let lowered = text.to_lowercase();
    if lowered.ends_with("-exception")
        || lowered.contains("-exception-")
        || lowered.ends_with("exception")
    {
        return true;
    }
    matches!(
        lowered.as_str(),
        "linux-syscall-note" | "gpl-cc-1.0" | "llgpr" | "llgpl" | "shl-2.0" | "shl-2.1"
    )
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecoveryRenderMode {
    Canonical,
    PreserveMalformedGrouping,
}

fn reparse_invalid_expression(text: &str) -> Option<(LicenseExpression, RecoveryRenderMode)> {
    let tokens = tokenize_for_recovery(text);

    let mut has_keywords = false;
    let mut license_keys: Vec<String> = Vec::new();

    for token in &tokens {
        match token {
            RecoveryToken::Keyword(_) => has_keywords = true,
            RecoveryToken::LicenseKey(key) => license_keys.push(key.clone()),
            RecoveryToken::Ignored => {}
        }
    }

    if license_keys.is_empty() {
        return Some((
            LicenseExpression::License("unknown-spdx".to_string()),
            RecoveryRenderMode::Canonical,
        ));
    }

    let all_exceptions = license_keys.iter().all(|key| is_spdx_exception(key));
    if all_exceptions {
        return Some((
            LicenseExpression::License("unknown-spdx".to_string()),
            RecoveryRenderMode::Canonical,
        ));
    }

    let expressions: Vec<LicenseExpression> = license_keys
        .into_iter()
        .map(LicenseExpression::License)
        .collect();

    let mut result = if has_keywords {
        LicenseExpression::and(expressions)
            .unwrap_or(LicenseExpression::License("unknown-spdx".to_string()))
    } else {
        LicenseExpression::or(expressions)
            .unwrap_or(LicenseExpression::License("unknown-spdx".to_string()))
    };

    if has_keywords {
        result = LicenseExpression::And {
            left: Box::new(result),
            right: Box::new(LicenseExpression::License("unknown-spdx".to_string())),
        };
    }

    let render_mode = if has_keywords {
        RecoveryRenderMode::PreserveMalformedGrouping
    } else {
        RecoveryRenderMode::Canonical
    };

    Some((result, render_mode))
}

fn convert_recovered_expression_to_scancode(
    expr: &LicenseExpression,
    index: &LicenseIndex,
) -> LicenseExpression {
    match expr {
        LicenseExpression::License(key) => {
            let lookup_key = key.to_lowercase();
            if let Some(&rid) = index.rid_by_spdx_key.get(&lookup_key) {
                LicenseExpression::License(index.rules_by_rid[rid].license_expression.clone())
            } else {
                LicenseExpression::License("unknown-spdx".to_string())
            }
        }
        LicenseExpression::LicenseRef(key) => {
            let lookup_key = key.to_lowercase();
            if let Some(&rid) = index.rid_by_spdx_key.get(&lookup_key) {
                LicenseExpression::License(index.rules_by_rid[rid].license_expression.clone())
            } else {
                LicenseExpression::License("unknown-spdx".to_string())
            }
        }
        LicenseExpression::And { left, right } => LicenseExpression::And {
            left: Box::new(convert_recovered_expression_to_scancode(left, index)),
            right: Box::new(convert_recovered_expression_to_scancode(right, index)),
        },
        LicenseExpression::Or { left, right } => LicenseExpression::Or {
            left: Box::new(convert_recovered_expression_to_scancode(left, index)),
            right: Box::new(convert_recovered_expression_to_scancode(right, index)),
        },
        LicenseExpression::With { left, right } => LicenseExpression::With {
            left: Box::new(convert_recovered_expression_to_scancode(left, index)),
            right: Box::new(convert_recovered_expression_to_scancode(right, index)),
        },
    }
}

fn render_valid_scancode_expression(expr: &LicenseExpression) -> String {
    render_canonical_boolean_expression(expr)
}

fn render_recovered_scancode_expression(
    expr: &LicenseExpression,
    render_mode: RecoveryRenderMode,
) -> String {
    match render_mode {
        RecoveryRenderMode::Canonical => render_canonical_boolean_expression(expr),
        RecoveryRenderMode::PreserveMalformedGrouping => expression_to_string(expr),
    }
}

fn render_canonical_boolean_expression(expr: &LicenseExpression) -> String {
    match expr {
        LicenseExpression::License(key) => key.clone(),
        LicenseExpression::LicenseRef(key) => key.clone(),
        LicenseExpression::With { left, right } => {
            let left_str = render_canonical_boolean_expression(left);
            let right_str = render_canonical_boolean_expression(right);
            format!("{} WITH {}", left_str, right_str)
        }
        LicenseExpression::And { .. } => render_flat_boolean_chain(expr, BooleanOperator::And),
        LicenseExpression::Or { .. } => render_flat_boolean_chain(expr, BooleanOperator::Or),
    }
}

fn render_flat_boolean_chain(expr: &LicenseExpression, operator: BooleanOperator) -> String {
    let mut parts = Vec::new();
    collect_boolean_chain(expr, operator, &mut parts);

    let separator = match operator {
        BooleanOperator::And => " AND ",
        BooleanOperator::Or => " OR ",
    };

    parts
        .into_iter()
        .map(|part| render_boolean_operand(part, operator))
        .collect::<Vec<_>>()
        .join(separator)
}

fn collect_boolean_chain<'a>(
    expr: &'a LicenseExpression,
    operator: BooleanOperator,
    parts: &mut Vec<&'a LicenseExpression>,
) {
    match (operator, expr) {
        (BooleanOperator::And, LicenseExpression::And { left, right })
        | (BooleanOperator::Or, LicenseExpression::Or { left, right }) => {
            collect_boolean_chain(left, operator, parts);
            collect_boolean_chain(right, operator, parts);
        }
        _ => parts.push(expr),
    }
}

fn render_boolean_operand(expr: &LicenseExpression, parent_operator: BooleanOperator) -> String {
    match expr {
        LicenseExpression::And { .. } => match parent_operator {
            BooleanOperator::And => render_canonical_boolean_expression(expr),
            BooleanOperator::Or => format!("({})", render_canonical_boolean_expression(expr)),
        },
        LicenseExpression::Or { .. } => match parent_operator {
            BooleanOperator::Or => render_canonical_boolean_expression(expr),
            BooleanOperator::And => format!("({})", render_canonical_boolean_expression(expr)),
        },
        _ => render_canonical_boolean_expression(expr),
    }
}

pub(crate) fn find_matching_rule_for_expression(
    index: &LicenseIndex,
    expression: &str,
) -> Option<String> {
    if let Some(&rid) = index.rid_by_spdx_key.get(expression) {
        let rule = &index.rules_by_rid[rid];
        return Some(rule.license_expression.clone());
    }

    for rule in &index.rules_by_rid {
        let normalized = normalize_spdx_key(&rule.license_expression);
        if normalized == expression {
            return Some(rule.license_expression.clone());
        }
    }

    if let Ok(parsed) = parse_expression(expression)
        && let Some(converted) = convert_spdx_expression_to_scancode(&parsed, index)
    {
        let result = render_valid_scancode_expression(&converted);
        if !result.is_empty() {
            return Some(result);
        }
    }

    if is_bare_license_list(expression) {
        let license_keys = split_license_expression(expression);
        if license_keys.len() > 1 {
            let or_expression = license_keys.join(" OR ");
            if let Ok(parsed) = parse_expression(&or_expression)
                && let Some(converted) = convert_spdx_expression_to_scancode(&parsed, index)
            {
                let result = render_valid_scancode_expression(&converted);
                if !result.is_empty() {
                    return Some(result);
                }
            }
        }
    }

    if let Some((recovered, render_mode)) = reparse_invalid_expression(expression) {
        let converted = convert_recovered_expression_to_scancode(&recovered, index);
        let result = render_recovered_scancode_expression(&converted, render_mode);
        if !result.is_empty() {
            return Some(result);
        }
    }

    index
        .unknown_spdx_rid
        .map(|rid| index.rules_by_rid[rid].license_expression.clone())
}

fn convert_spdx_expression_to_scancode(
    expr: &LicenseExpression,
    index: &LicenseIndex,
) -> Option<LicenseExpression> {
    match expr {
        LicenseExpression::License(key) => {
            let lookup_key = key.to_lowercase();
            if let Some(&rid) = index.rid_by_spdx_key.get(&lookup_key) {
                Some(LicenseExpression::License(
                    index.rules_by_rid[rid].license_expression.clone(),
                ))
            } else {
                None
            }
        }
        LicenseExpression::LicenseRef(key) => {
            let lookup_key = key.to_lowercase();
            if let Some(&rid) = index.rid_by_spdx_key.get(&lookup_key) {
                Some(LicenseExpression::License(
                    index.rules_by_rid[rid].license_expression.clone(),
                ))
            } else {
                None
            }
        }
        LicenseExpression::And { left, right } => {
            let left_converted = convert_spdx_expression_to_scancode(left, index);
            let right_converted = convert_spdx_expression_to_scancode(right, index);
            match (left_converted, right_converted) {
                (Some(l), Some(r)) => Some(LicenseExpression::And {
                    left: Box::new(l),
                    right: Box::new(r),
                }),
                _ => None,
            }
        }
        LicenseExpression::Or { left, right } => {
            let left_converted = convert_spdx_expression_to_scancode(left, index);
            let right_converted = convert_spdx_expression_to_scancode(right, index);
            match (left_converted, right_converted) {
                (Some(l), Some(r)) => Some(LicenseExpression::Or {
                    left: Box::new(l),
                    right: Box::new(r),
                }),
                _ => None,
            }
        }
        LicenseExpression::With { left, right } => {
            let left_converted = convert_spdx_expression_to_scancode(left, index);
            let right_converted = convert_spdx_expression_to_scancode(right, index);
            match (left_converted, right_converted) {
                (Some(l), Some(r)) => Some(LicenseExpression::With {
                    left: Box::new(l),
                    right: Box::new(r),
                }),
                _ => None,
            }
        }
    }
}

#[cfg(test)]
mod test;
