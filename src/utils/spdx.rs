// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};

use crate::license_detection::expression::{
    LicenseExpression, parse_expression, simplify_expression,
    simplify_expression_preserving_structure,
};

#[derive(Clone, Copy)]
pub(crate) enum ExpressionRelation {
    And,
    Or,
}

#[derive(Clone, Copy)]
enum BooleanOperator {
    And,
    Or,
}

pub fn combine_license_expressions(
    expressions: impl IntoIterator<Item = String>,
) -> Option<String> {
    combine_license_expressions_with_relation(expressions, ExpressionRelation::And)
}

pub fn combine_license_expressions_preserving_structure(
    expressions: impl IntoIterator<Item = String>,
) -> Option<String> {
    combine_license_expressions_with_relation_and_mode(expressions, ExpressionRelation::And, true)
}

pub(crate) fn combine_license_expressions_with_relation_preserving_structure(
    expressions: impl IntoIterator<Item = String>,
    relation: ExpressionRelation,
) -> Option<String> {
    combine_license_expressions_with_relation_and_mode(expressions, relation, true)
}

pub(crate) fn combine_license_expressions_with_relation(
    expressions: impl IntoIterator<Item = String>,
    relation: ExpressionRelation,
) -> Option<String> {
    combine_license_expressions_with_relation_and_mode(expressions, relation, false)
}

fn combine_license_expressions_with_relation_and_mode(
    expressions: impl IntoIterator<Item = String>,
    relation: ExpressionRelation,
    preserve_structure: bool,
) -> Option<String> {
    let expressions: Vec<String> = expressions
        .into_iter()
        .map(|expression| expression.trim().to_string())
        .filter(|expression| !expression.is_empty())
        .collect();

    if expressions.is_empty() {
        return None;
    }

    combine_parsed_expressions(&expressions, relation, preserve_structure)
        .or_else(|| combine_license_expressions_fallback(&expressions, relation))
}

fn combine_parsed_expressions(
    expressions: &[String],
    relation: ExpressionRelation,
    preserve_structure: bool,
) -> Option<String> {
    let mut case_map = HashMap::new();
    let parsed_expressions: Vec<LicenseExpression> = expressions
        .iter()
        .map(|expression| {
            collect_term_case(expression, &mut case_map);
            parse_expression(expression).ok()
        })
        .collect::<Option<Vec<_>>>()?;

    let combined = match relation {
        ExpressionRelation::And => LicenseExpression::and(parsed_expressions),
        ExpressionRelation::Or => LicenseExpression::or(parsed_expressions),
    }?;

    let combined = if preserve_structure {
        simplify_expression_preserving_structure(&combined)
    } else {
        simplify_expression(&combined)
    };
    Some(render_expression_with_case_map(&combined, &case_map))
}

fn combine_license_expressions_fallback(
    expressions: &[String],
    relation: ExpressionRelation,
) -> Option<String> {
    let unique_expressions: HashSet<String> = expressions.iter().cloned().collect();
    if unique_expressions.is_empty() {
        return None;
    }

    let mut sorted_expressions: Vec<String> = unique_expressions.into_iter().collect();
    sorted_expressions.sort();

    let separator = match relation {
        ExpressionRelation::And => " AND ",
        ExpressionRelation::Or => " OR ",
    };

    Some(
        sorted_expressions
            .iter()
            .map(|expr| wrap_compound_expression(expr))
            .collect::<Vec<_>>()
            .join(separator),
    )
}

fn collect_term_case(expression: &str, case_map: &mut HashMap<String, String>) {
    let chars: Vec<char> = expression.chars().collect();
    let mut pos = 0;

    while pos < chars.len() {
        let ch = chars[pos];
        if !(ch.is_alphanumeric() || ch == '-' || ch == '.' || ch == '_' || ch == '+') {
            pos += 1;
            continue;
        }

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

        let term: String = chars[start..pos].iter().collect();
        let upper = term.to_ascii_uppercase();
        if matches!(upper.as_str(), "AND" | "OR" | "WITH") {
            continue;
        }

        case_map.entry(term.to_ascii_lowercase()).or_insert(term);
    }
}

fn render_expression_with_case_map(
    expression: &LicenseExpression,
    case_map: &HashMap<String, String>,
) -> String {
    match expression {
        LicenseExpression::License(key) | LicenseExpression::LicenseRef(key) => {
            case_map.get(key).cloned().unwrap_or_else(|| key.clone())
        }
        LicenseExpression::And { .. } => {
            render_flat_boolean_chain(expression, BooleanOperator::And, case_map)
        }
        LicenseExpression::Or { .. } => {
            render_flat_boolean_chain(expression, BooleanOperator::Or, case_map)
        }
        LicenseExpression::With { left, right } => format!(
            "{} WITH {}",
            render_expression_with_case_map(left, case_map),
            render_expression_with_case_map(right, case_map)
        ),
    }
}

fn render_flat_boolean_chain(
    expression: &LicenseExpression,
    operator: BooleanOperator,
    case_map: &HashMap<String, String>,
) -> String {
    let mut parts = Vec::new();
    collect_boolean_chain(expression, operator, &mut parts);

    let separator = match operator {
        BooleanOperator::And => " AND ",
        BooleanOperator::Or => " OR ",
    };

    parts
        .into_iter()
        .map(|part| render_boolean_operand(part, operator, case_map))
        .collect::<Vec<_>>()
        .join(separator)
}

fn collect_boolean_chain<'a>(
    expression: &'a LicenseExpression,
    operator: BooleanOperator,
    parts: &mut Vec<&'a LicenseExpression>,
) {
    match (operator, expression) {
        (BooleanOperator::And, LicenseExpression::And { left, right })
        | (BooleanOperator::Or, LicenseExpression::Or { left, right }) => {
            collect_boolean_chain(left, operator, parts);
            collect_boolean_chain(right, operator, parts);
        }
        _ => parts.push(expression),
    }
}

fn render_boolean_operand(
    expression: &LicenseExpression,
    parent_operator: BooleanOperator,
    case_map: &HashMap<String, String>,
) -> String {
    match expression {
        LicenseExpression::And { .. } => match parent_operator {
            BooleanOperator::And => render_expression_with_case_map(expression, case_map),
            BooleanOperator::Or => format!(
                "({})",
                render_expression_with_case_map(expression, case_map)
            ),
        },
        LicenseExpression::Or { .. } => match parent_operator {
            BooleanOperator::Or => render_expression_with_case_map(expression, case_map),
            BooleanOperator::And => format!(
                "({})",
                render_expression_with_case_map(expression, case_map)
            ),
        },
        _ => render_expression_with_case_map(expression, case_map),
    }
}

fn wrap_compound_expression(expression: &str) -> String {
    if expression.contains(' ') && !(expression.starts_with('(') && expression.ends_with(')')) {
        format!("({})", expression)
    } else {
        expression.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combine_license_expressions_preserves_spdx_case() {
        let result = combine_license_expressions(vec!["MIT".to_string(), "Apache-2.0".to_string()]);

        assert_eq!(result.as_deref(), Some("Apache-2.0 AND MIT"));
    }

    #[test]
    fn combine_license_expressions_flattens_same_operator_parentheses() {
        let result = combine_license_expressions(vec![
            "MIT".to_string(),
            "ICU".to_string(),
            "Unicode-TOU".to_string(),
        ]);

        assert_eq!(result.as_deref(), Some("ICU AND MIT AND Unicode-TOU"));
    }

    #[test]
    fn combine_license_expressions_does_not_absorb_with_expressions() {
        let result = combine_license_expressions(vec![
            "GPL-2.0 WITH Classpath-exception-2.0".to_string(),
            "GPL-2.0".to_string(),
        ]);

        assert_eq!(
            result.as_deref(),
            Some("GPL-2.0 AND GPL-2.0 WITH Classpath-exception-2.0")
        );
    }

    #[test]
    fn combine_license_expressions_simplifies_absorbed_and_expression() {
        let result = combine_license_expressions(vec![
            "Apache-2.0 OR MIT".to_string(),
            "Apache-2.0".to_string(),
        ]);

        assert_eq!(result.as_deref(), Some("Apache-2.0"));
    }

    #[test]
    fn combine_license_expressions_preserving_structure_keeps_distinct_nested_operands() {
        let result = combine_license_expressions_preserving_structure(vec![
            "MIT".to_string(),
            "Apache-2.0 OR MIT".to_string(),
        ]);

        assert_eq!(result.as_deref(), Some("MIT AND (Apache-2.0 OR MIT)"));
    }

    #[test]
    fn combine_license_expressions_with_relation_simplifies_absorbed_or_expression() {
        let result = combine_license_expressions_with_relation(
            vec!["MIT AND Apache-2.0".to_string(), "MIT".to_string()],
            ExpressionRelation::Or,
        );

        assert_eq!(result.as_deref(), Some("MIT"));
    }
}
