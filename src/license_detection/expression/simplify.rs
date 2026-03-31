//! License expression simplification and utilities.

use std::collections::HashSet;

use super::{LicenseExpression, ParseError};

/// Simplify a license expression by deduplicating and reducing boolean clauses.
///
/// # Arguments
/// * `expr` - The expression to simplify
///
/// # Returns
/// Simplified expression with duplicate and subsumed licenses removed,
/// preserving order.
pub fn simplify_expression(expr: &LicenseExpression) -> LicenseExpression {
    match expr {
        LicenseExpression::License(key) => LicenseExpression::License(key.clone()),
        LicenseExpression::LicenseRef(key) => LicenseExpression::LicenseRef(key.clone()),
        LicenseExpression::With { left, right } => LicenseExpression::With {
            left: Box::new(simplify_expression(left)),
            right: Box::new(simplify_expression(right)),
        },
        LicenseExpression::And { .. } => {
            let mut unique = Vec::new();
            let mut seen = HashSet::new();
            collect_unique_and(expr, &mut unique, &mut seen);
            prune_subsumed_operands(&mut unique, true);
            build_expression_from_list(&unique, true)
        }
        LicenseExpression::Or { .. } => {
            let mut unique = Vec::new();
            let mut seen = HashSet::new();
            collect_unique_or(expr, &mut unique, &mut seen);
            prune_subsumed_operands(&mut unique, false);
            build_expression_from_list(&unique, false)
        }
    }
}

fn prune_subsumed_operands(operands: &mut Vec<LicenseExpression>, outer_is_and: bool) {
    let inner_is_and = !outer_is_and;
    let pruned: Vec<LicenseExpression> = operands
        .iter()
        .enumerate()
        .filter(|(candidate_idx, candidate)| {
            !operands.iter().enumerate().any(|(other_idx, other)| {
                candidate_idx != &other_idx && operand_subsumes(other, candidate, inner_is_and)
            })
        })
        .map(|(_, operand)| operand.clone())
        .collect();

    *operands = pruned;
}

fn operand_subsumes(
    other: &LicenseExpression,
    candidate: &LicenseExpression,
    inner_is_and: bool,
) -> bool {
    let other_args = get_flat_args(other);
    let candidate_args = get_flat_args(candidate);

    if other_args.len() >= candidate_args.len() {
        return false;
    }

    let relevant_operator = matches!(other, LicenseExpression::And { .. })
        || matches!(other, LicenseExpression::Or { .. })
        || matches!(candidate, LicenseExpression::And { .. })
        || matches!(candidate, LicenseExpression::Or { .. });

    if !relevant_operator {
        return false;
    }

    let operator_matches = if inner_is_and {
        matches!(candidate, LicenseExpression::And { .. })
            || matches!(other, LicenseExpression::And { .. })
    } else {
        matches!(candidate, LicenseExpression::Or { .. })
            || matches!(other, LicenseExpression::Or { .. })
    };

    if !operator_matches {
        return false;
    }

    other_args.iter().all(|other_arg| {
        candidate_args
            .iter()
            .any(|arg| expressions_equal(arg, other_arg))
    })
}

fn collect_unique_and(
    expr: &LicenseExpression,
    unique: &mut Vec<LicenseExpression>,
    seen: &mut HashSet<String>,
) {
    match expr {
        LicenseExpression::And { left, right } => {
            collect_unique_and(left, unique, seen);
            collect_unique_and(right, unique, seen);
        }
        LicenseExpression::Or { .. } => {
            let simplified = simplify_expression(expr);
            let key = expression_to_string(&simplified);
            if !seen.contains(&key) {
                seen.insert(key);
                unique.push(simplified);
            }
        }
        LicenseExpression::With { left, right } => {
            let simplified = LicenseExpression::With {
                left: Box::new(simplify_expression(left)),
                right: Box::new(simplify_expression(right)),
            };
            let key = expression_to_string(&simplified);
            if !seen.contains(&key) {
                seen.insert(key);
                unique.push(simplified);
            }
        }
        LicenseExpression::License(key) => {
            if !seen.contains(key) {
                seen.insert(key.clone());
                unique.push(LicenseExpression::License(key.clone()));
            }
        }
        LicenseExpression::LicenseRef(key) => {
            if !seen.contains(key) {
                seen.insert(key.clone());
                unique.push(LicenseExpression::LicenseRef(key.clone()));
            }
        }
    }
}

fn collect_unique_or(
    expr: &LicenseExpression,
    unique: &mut Vec<LicenseExpression>,
    seen: &mut HashSet<String>,
) {
    match expr {
        LicenseExpression::Or { left, right } => {
            collect_unique_or(left, unique, seen);
            collect_unique_or(right, unique, seen);
        }
        LicenseExpression::And { .. } => {
            let simplified = simplify_expression(expr);
            let key = expression_to_string(&simplified);
            if !seen.contains(&key) {
                seen.insert(key);
                unique.push(simplified);
            }
        }
        LicenseExpression::With { left, right } => {
            let simplified = LicenseExpression::With {
                left: Box::new(simplify_expression(left)),
                right: Box::new(simplify_expression(right)),
            };
            let key = expression_to_string(&simplified);
            if !seen.contains(&key) {
                seen.insert(key);
                unique.push(simplified);
            }
        }
        LicenseExpression::License(key) => {
            if !seen.contains(key) {
                seen.insert(key.clone());
                unique.push(LicenseExpression::License(key.clone()));
            }
        }
        LicenseExpression::LicenseRef(key) => {
            if !seen.contains(key) {
                seen.insert(key.clone());
                unique.push(LicenseExpression::LicenseRef(key.clone()));
            }
        }
    }
}

fn build_expression_from_list(unique: &[LicenseExpression], is_and: bool) -> LicenseExpression {
    match unique.len() {
        0 => panic!("build_expression_from_list called with empty list"),
        1 => unique[0].clone(),
        _ => {
            let mut iter = unique.iter();
            let mut result = iter.next().unwrap().clone();
            for expr in iter {
                result = if is_and {
                    LicenseExpression::And {
                        left: Box::new(result),
                        right: Box::new(expr.clone()),
                    }
                } else {
                    LicenseExpression::Or {
                        left: Box::new(result),
                        right: Box::new(expr.clone()),
                    }
                };
            }
            result
        }
    }
}

fn get_flat_args(expr: &LicenseExpression) -> Vec<LicenseExpression> {
    match expr {
        LicenseExpression::And { left, right } => {
            let mut args = Vec::new();
            collect_flat_and_args(left, &mut args);
            collect_flat_and_args(right, &mut args);
            args
        }
        LicenseExpression::Or { left, right } => {
            let mut args = Vec::new();
            collect_flat_or_args(left, &mut args);
            collect_flat_or_args(right, &mut args);
            args
        }
        _ => vec![expr.clone()],
    }
}

fn collect_flat_and_args(expr: &LicenseExpression, args: &mut Vec<LicenseExpression>) {
    match expr {
        LicenseExpression::And { left, right } => {
            collect_flat_and_args(left, args);
            collect_flat_and_args(right, args);
        }
        _ => args.push(expr.clone()),
    }
}

fn collect_flat_or_args(expr: &LicenseExpression, args: &mut Vec<LicenseExpression>) {
    match expr {
        LicenseExpression::Or { left, right } => {
            collect_flat_or_args(left, args);
            collect_flat_or_args(right, args);
        }
        _ => args.push(expr.clone()),
    }
}

fn decompose_expr(expr: &LicenseExpression) -> Vec<LicenseExpression> {
    match expr {
        LicenseExpression::With { left, right } => {
            let mut parts = decompose_expr(left);
            parts.extend(decompose_expr(right));
            parts
        }
        _ => vec![expr.clone()],
    }
}

fn expressions_equal(a: &LicenseExpression, b: &LicenseExpression) -> bool {
    match (a, b) {
        (LicenseExpression::License(ka), LicenseExpression::License(kb)) => ka == kb,
        (LicenseExpression::LicenseRef(ka), LicenseExpression::LicenseRef(kb)) => ka == kb,
        (
            LicenseExpression::With {
                left: l1,
                right: r1,
            },
            LicenseExpression::With {
                left: l2,
                right: r2,
            },
        ) => expressions_equal(l1, l2) && expressions_equal(r1, r2),
        (LicenseExpression::And { .. }, LicenseExpression::And { .. }) => {
            let args_a = get_flat_args(a);
            let args_b = get_flat_args(b);
            args_a.len() == args_b.len()
                && args_b
                    .iter()
                    .all(|b_arg| args_a.iter().any(|a_arg| expressions_equal(a_arg, b_arg)))
        }
        (LicenseExpression::Or { .. }, LicenseExpression::Or { .. }) => {
            let args_a = get_flat_args(a);
            let args_b = get_flat_args(b);
            args_a.len() == args_b.len()
                && args_b
                    .iter()
                    .all(|b_arg| args_a.iter().any(|a_arg| expressions_equal(a_arg, b_arg)))
        }
        _ => false,
    }
}

fn expr_in_args(expr: &LicenseExpression, args: &[LicenseExpression]) -> bool {
    if args.iter().any(|a| expressions_equal(a, expr)) {
        return true;
    }
    let decomposed = decompose_expr(expr);
    if decomposed.len() == 1 {
        return false;
    }
    decomposed
        .iter()
        .any(|d| args.iter().any(|a| expressions_equal(a, d)))
}

pub fn licensing_contains(container: &str, contained: &str) -> bool {
    let container = container.trim();
    let contained = contained.trim();
    if container.is_empty() || contained.is_empty() {
        return false;
    }

    if container == contained {
        return true;
    }

    let Ok(parsed_container) = super::parse::parse_expression(container) else {
        return false;
    };
    let Ok(parsed_contained) = super::parse::parse_expression(contained) else {
        return false;
    };

    let simplified_container = simplify_expression(&parsed_container);
    let simplified_contained = simplify_expression(&parsed_contained);

    match (&simplified_container, &simplified_contained) {
        (LicenseExpression::And { .. }, LicenseExpression::And { .. })
        | (LicenseExpression::Or { .. }, LicenseExpression::Or { .. }) => {
            let container_args = get_flat_args(&simplified_container);
            let contained_args = get_flat_args(&simplified_contained);
            contained_args
                .iter()
                .all(|c| container_args.iter().any(|ca| expressions_equal(ca, c)))
        }
        (
            LicenseExpression::And { .. } | LicenseExpression::Or { .. },
            LicenseExpression::License(_) | LicenseExpression::LicenseRef(_),
        ) => {
            let container_args = get_flat_args(&simplified_container);
            expr_in_args(&simplified_contained, &container_args)
        }
        (LicenseExpression::And { .. } | LicenseExpression::Or { .. }, _) => {
            let container_args = get_flat_args(&simplified_container);
            container_args
                .iter()
                .any(|ca| expressions_equal(ca, &simplified_contained))
        }
        (
            LicenseExpression::With { .. },
            LicenseExpression::License(_) | LicenseExpression::LicenseRef(_),
        ) => {
            let decomposed = decompose_expr(&simplified_container);
            decomposed
                .iter()
                .any(|d| expressions_equal(d, &simplified_contained))
        }
        (
            LicenseExpression::License(_) | LicenseExpression::LicenseRef(_),
            LicenseExpression::And { .. }
            | LicenseExpression::Or { .. }
            | LicenseExpression::With { .. },
        ) => false,
        (LicenseExpression::License(k1), LicenseExpression::License(k2)) => k1 == k2,
        (LicenseExpression::LicenseRef(k1), LicenseExpression::LicenseRef(k2)) => k1 == k2,
        _ => false,
    }
}

/// # Returns
/// String representation of the expression
///
/// # Parentheses
/// Parentheses are added when needed to preserve semantic meaning based on
/// operator precedence (WITH > AND > OR). This matches the Python
/// license-expression library behavior.
/// Convert a license expression to its string representation.
pub fn expression_to_string(expr: &LicenseExpression) -> String {
    match expr {
        LicenseExpression::License(key) => key.clone(),
        LicenseExpression::LicenseRef(key) => key.clone(),
        LicenseExpression::And { left, right } => {
            let left_str = expression_to_string_maybe_parens(left, true);
            let right_str = expression_to_string_maybe_parens(right, true);
            format!("{} AND {}", left_str, right_str)
        }
        LicenseExpression::Or { left, right } => {
            let left_str = expression_to_string_maybe_parens(left, true);
            let right_str = expression_to_string_maybe_parens(right, true);
            format!("{} OR {}", left_str, right_str)
        }
        LicenseExpression::With { left, right } => {
            let left_str = expression_to_string(left);
            let right_str = expression_to_string(right);
            format!("{} WITH {}", left_str, right_str)
        }
    }
}

fn expression_to_string_maybe_parens(expr: &LicenseExpression, parent_is_and_or: bool) -> String {
    match expr {
        LicenseExpression::License(key) => key.clone(),
        LicenseExpression::LicenseRef(key) => key.clone(),
        LicenseExpression::And { .. } | LicenseExpression::Or { .. } => {
            let result = expression_to_string(expr);
            if parent_is_and_or {
                format!("({})", result)
            } else {
                result
            }
        }
        LicenseExpression::With { left, right } => {
            let left_str = expression_to_string(left);
            let right_str = expression_to_string(right);
            format!("{} WITH {}", left_str, right_str)
        }
    }
}

fn combine_expressions_with(
    expressions: &[&str],
    unique: bool,
    combiner: fn(Vec<LicenseExpression>) -> Option<LicenseExpression>,
) -> Result<String, ParseError> {
    if expressions.is_empty() {
        return Ok(String::new());
    }
    if expressions.len() == 1 {
        let parsed = super::parse::parse_expression(expressions[0])?;
        return Ok(expression_to_string(&if unique {
            simplify_expression(&parsed)
        } else {
            parsed
        }));
    }

    let parsed_exprs: Vec<LicenseExpression> = expressions
        .iter()
        .map(|e| super::parse::parse_expression(e))
        .collect::<Result<Vec<_>, _>>()?;

    let combined = combiner(parsed_exprs);

    match combined {
        Some(expr) => {
            let final_expr = if unique {
                simplify_expression(&expr)
            } else {
                expr
            };
            Ok(expression_to_string(&final_expr))
        }
        None => Ok(String::new()),
    }
}

/// Combine multiple license expressions with `AND`.
///
/// This function parses each expression string, combines them with `AND`, and
/// optionally deduplicates license keys.
pub fn combine_expressions_and(expressions: &[&str], unique: bool) -> Result<String, ParseError> {
    combine_expressions_with(expressions, unique, LicenseExpression::and)
}

/// Combine multiple license expressions with `OR`.
///
/// This function parses each expression string, combines them with `OR`, and
/// optionally deduplicates license keys.
#[allow(dead_code)]
pub fn combine_expressions_or(expressions: &[&str], unique: bool) -> Result<String, ParseError> {
    combine_expressions_with(expressions, unique, LicenseExpression::or)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simplify_expression_no_change() {
        let expr = super::super::parse::parse_expression("MIT AND Apache-2.0").unwrap();
        let simplified = simplify_expression(&expr);
        assert_eq!(expression_to_string(&simplified), "mit AND apache-2.0");
    }

    #[test]
    fn test_simplify_expression_with_duplicates() {
        let expr = super::super::parse::parse_expression("MIT OR MIT").unwrap();
        let simplified = simplify_expression(&expr);
        assert_eq!(expression_to_string(&simplified), "mit");
    }

    #[test]
    fn test_simplify_and_duplicates() {
        let expr = super::super::parse::parse_expression("crapl-0.1 AND crapl-0.1").unwrap();
        let simplified = simplify_expression(&expr);
        assert_eq!(expression_to_string(&simplified), "crapl-0.1");
    }

    #[test]
    fn test_simplify_or_duplicates() {
        let expr = super::super::parse::parse_expression("mit OR mit").unwrap();
        let simplified = simplify_expression(&expr);
        assert_eq!(expression_to_string(&simplified), "mit");
    }

    #[test]
    fn test_simplify_preserves_different_licenses() {
        let expr = super::super::parse::parse_expression("mit AND apache-2.0").unwrap();
        let simplified = simplify_expression(&expr);
        assert_eq!(expression_to_string(&simplified), "mit AND apache-2.0");
    }

    #[test]
    fn test_simplify_complex_duplicates() {
        let expr = super::super::parse::parse_expression(
            "gpl-2.0-plus AND gpl-2.0-plus AND lgpl-2.0-plus",
        )
        .unwrap();
        let simplified = simplify_expression(&expr);
        assert_eq!(
            expression_to_string(&simplified),
            "gpl-2.0-plus AND lgpl-2.0-plus"
        );
    }

    #[test]
    fn test_simplify_three_duplicates() {
        let expr =
            super::super::parse::parse_expression("fsf-free AND fsf-free AND fsf-free").unwrap();
        let simplified = simplify_expression(&expr);
        assert_eq!(expression_to_string(&simplified), "fsf-free");
    }

    #[test]
    fn test_simplify_with_expression_dedup() {
        let expr = super::super::parse::parse_expression(
            "gpl-2.0 WITH classpath-exception-2.0 AND gpl-2.0 WITH classpath-exception-2.0",
        )
        .unwrap();
        let simplified = simplify_expression(&expr);
        assert_eq!(
            expression_to_string(&simplified),
            "gpl-2.0 WITH classpath-exception-2.0"
        );
    }

    #[test]
    fn test_simplify_nested_duplicates() {
        let expr =
            super::super::parse::parse_expression("(mit AND apache-2.0) OR (mit AND apache-2.0)")
                .unwrap();
        let simplified = simplify_expression(&expr);
        assert_eq!(expression_to_string(&simplified), "mit AND apache-2.0");
    }

    #[test]
    fn test_simplify_preserves_order() {
        let expr =
            super::super::parse::parse_expression("apache-2.0 AND mit AND apache-2.0").unwrap();
        let simplified = simplify_expression(&expr);
        assert_eq!(expression_to_string(&simplified), "apache-2.0 AND mit");
    }

    #[test]
    fn test_simplify_mit_and_mit_and_apache() {
        let expr = super::super::parse::parse_expression("mit AND mit AND apache-2.0").unwrap();
        let simplified = simplify_expression(&expr);
        assert_eq!(expression_to_string(&simplified), "mit AND apache-2.0");
    }

    #[test]
    fn test_simplify_and_absorption() {
        let expr = super::super::parse::parse_expression("mit AND (mit OR apache-2.0)").unwrap();
        let simplified = simplify_expression(&expr);

        assert_eq!(expression_to_string(&simplified), "mit");
    }

    #[test]
    fn test_simplify_or_absorption() {
        let expr = super::super::parse::parse_expression("mit OR (mit AND apache-2.0)").unwrap();
        let simplified = simplify_expression(&expr);

        assert_eq!(expression_to_string(&simplified), "mit");
    }

    #[test]
    fn test_simplify_or_subsumption() {
        let expr = super::super::parse::parse_expression(
            "(mit AND apache-2.0) OR (mit AND apache-2.0 AND bsd-new)",
        )
        .unwrap();
        let simplified = simplify_expression(&expr);

        assert_eq!(expression_to_string(&simplified), "mit AND apache-2.0");
    }

    #[test]
    fn test_simplify_and_subsumption() {
        let expr = super::super::parse::parse_expression(
            "(mit OR apache-2.0) AND (mit OR apache-2.0 OR bsd-new)",
        )
        .unwrap();
        let simplified = simplify_expression(&expr);

        assert_eq!(expression_to_string(&simplified), "mit OR apache-2.0");
    }

    #[test]
    fn test_expression_to_string_simple() {
        let expr = LicenseExpression::License("mit".to_string());
        assert_eq!(expression_to_string(&expr), "mit");
    }

    #[test]
    fn test_expression_to_string_and() {
        let expr = LicenseExpression::And {
            left: Box::new(LicenseExpression::License("mit".to_string())),
            right: Box::new(LicenseExpression::License("apache-2.0".to_string())),
        };
        assert_eq!(expression_to_string(&expr), "mit AND apache-2.0");
    }

    #[test]
    fn test_expression_to_string_or() {
        let expr = LicenseExpression::Or {
            left: Box::new(LicenseExpression::License("mit".to_string())),
            right: Box::new(LicenseExpression::License("apache-2.0".to_string())),
        };
        assert_eq!(expression_to_string(&expr), "mit OR apache-2.0");
    }

    #[test]
    fn test_expression_to_string_with() {
        let expr = LicenseExpression::With {
            left: Box::new(LicenseExpression::License("gpl-2.0".to_string())),
            right: Box::new(LicenseExpression::License(
                "classpath-exception-2.0".to_string(),
            )),
        };
        assert_eq!(
            expression_to_string(&expr),
            "gpl-2.0 WITH classpath-exception-2.0"
        );
    }

    #[test]
    fn test_expression_to_string_licenseref() {
        let expr = LicenseExpression::LicenseRef("licenseref-scancode-custom".to_string());
        assert_eq!(expression_to_string(&expr), "licenseref-scancode-custom");
    }

    #[test]
    fn test_expression_to_string_or_inside_and() {
        let or_expr = LicenseExpression::Or {
            left: Box::new(LicenseExpression::License("mit".to_string())),
            right: Box::new(LicenseExpression::License("apache-2.0".to_string())),
        };
        let and_expr = LicenseExpression::And {
            left: Box::new(or_expr),
            right: Box::new(LicenseExpression::License("gpl-2.0".to_string())),
        };
        assert_eq!(
            expression_to_string(&and_expr),
            "(mit OR apache-2.0) AND gpl-2.0"
        );
    }

    #[test]
    fn test_expression_to_string_and_inside_or() {
        let and_expr = LicenseExpression::And {
            left: Box::new(LicenseExpression::License("mit".to_string())),
            right: Box::new(LicenseExpression::License("apache-2.0".to_string())),
        };
        let or_expr = LicenseExpression::Or {
            left: Box::new(and_expr),
            right: Box::new(LicenseExpression::License("gpl-2.0".to_string())),
        };
        assert_eq!(
            expression_to_string(&or_expr),
            "(mit AND apache-2.0) OR gpl-2.0"
        );
    }

    #[test]
    fn test_expression_to_string_with_inside_or() {
        let with_expr = LicenseExpression::With {
            left: Box::new(LicenseExpression::License("gpl-2.0".to_string())),
            right: Box::new(LicenseExpression::License(
                "classpath-exception-2.0".to_string(),
            )),
        };
        let or_expr = LicenseExpression::Or {
            left: Box::new(with_expr),
            right: Box::new(LicenseExpression::License("mit".to_string())),
        };
        assert_eq!(
            expression_to_string(&or_expr),
            "gpl-2.0 WITH classpath-exception-2.0 OR mit"
        );
    }

    #[test]
    fn test_expression_to_string_with_inside_and() {
        let with_expr = LicenseExpression::With {
            left: Box::new(LicenseExpression::License("gpl-2.0".to_string())),
            right: Box::new(LicenseExpression::License(
                "classpath-exception-2.0".to_string(),
            )),
        };
        let and_expr = LicenseExpression::And {
            left: Box::new(with_expr),
            right: Box::new(LicenseExpression::License("mit".to_string())),
        };
        assert_eq!(
            expression_to_string(&and_expr),
            "gpl-2.0 WITH classpath-exception-2.0 AND mit"
        );
    }

    #[test]
    fn test_expression_to_string_nested_or_preserves_grouping() {
        // Manually constructed nested OR: (mit OR apache-2.0) OR gpl-2.0
        // When nested OR is constructed manually, it renders with parens
        // This matches Python license-expression behavior
        let or_expr = LicenseExpression::Or {
            left: Box::new(LicenseExpression::Or {
                left: Box::new(LicenseExpression::License("mit".to_string())),
                right: Box::new(LicenseExpression::License("apache-2.0".to_string())),
            }),
            right: Box::new(LicenseExpression::License("gpl-2.0".to_string())),
        };
        assert_eq!(
            expression_to_string(&or_expr),
            "(mit OR apache-2.0) OR gpl-2.0"
        );
    }

    #[test]
    fn test_expression_to_string_nested_and_preserves_grouping() {
        // Manually constructed nested AND: (mit AND apache-2.0) AND gpl-2.0
        // When nested AND is constructed manually, it renders with parens
        // This matches Python license-expression behavior
        let and_expr = LicenseExpression::And {
            left: Box::new(LicenseExpression::And {
                left: Box::new(LicenseExpression::License("mit".to_string())),
                right: Box::new(LicenseExpression::License("apache-2.0".to_string())),
            }),
            right: Box::new(LicenseExpression::License("gpl-2.0".to_string())),
        };
        assert_eq!(
            expression_to_string(&and_expr),
            "(mit AND apache-2.0) AND gpl-2.0"
        );
    }

    #[test]
    fn test_expression_to_string_roundtrip_or_and() {
        let input = "(mit OR apache-2.0) AND gpl-2.0";
        let expr = super::super::parse::parse_expression(input).unwrap();
        let output = expression_to_string(&expr);
        assert_eq!(output, "(mit OR apache-2.0) AND gpl-2.0");
    }

    #[test]
    fn test_expression_to_string_roundtrip_or_with() {
        let input = "gpl-2.0 WITH classpath-exception-2.0 OR mit";
        let expr = super::super::parse::parse_expression(input).unwrap();
        let output = expression_to_string(&expr);
        assert_eq!(output, "gpl-2.0 WITH classpath-exception-2.0 OR mit");
    }

    #[test]
    fn test_combine_expressions_empty() {
        let result = combine_expressions_and(&[], true).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_combine_expressions_single() {
        let result = combine_expressions_and(&["mit"], true).unwrap();
        assert_eq!(result, "mit");
    }

    #[test]
    fn test_combine_expressions_two_and() {
        let result = combine_expressions_and(&["mit", "gpl-2.0-plus"], true).unwrap();
        assert_eq!(result, "mit AND gpl-2.0-plus");
    }

    #[test]
    fn test_combine_expressions_two_or() {
        let result = combine_expressions_or(&["mit", "apache-2.0"], true).unwrap();
        assert_eq!(result, "mit OR apache-2.0");
    }

    #[test]
    fn test_combine_expressions_multiple_and() {
        let result = combine_expressions_and(&["mit", "apache-2.0", "gpl-2.0-plus"], true).unwrap();
        assert!(result.contains("mit"));
        assert!(result.contains("apache-2.0"));
        assert!(result.contains("gpl-2.0-plus"));
        assert_eq!(result.matches("AND").count(), 2);
    }

    #[test]
    fn test_combine_expressions_with_duplicates_unique() {
        let result = combine_expressions_or(&["mit", "mit", "apache-2.0"], true).unwrap();
        let expr = super::super::parse::parse_expression(&result).unwrap();
        let keys = expr.license_keys();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"mit".to_string()));
        assert!(keys.contains(&"apache-2.0".to_string()));
    }

    #[test]
    fn test_combine_expressions_with_duplicates_not_unique() {
        let result = combine_expressions_or(&["mit", "mit", "apache-2.0"], false).unwrap();
        let expr = super::super::parse::parse_expression(&result).unwrap();
        // combine_expressions creates nested structure, so we get parens
        assert_eq!(result, "(mit OR mit) OR apache-2.0");
        let keys = expr.license_keys();
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn test_combine_expressions_complex_with_simplification() {
        let result = combine_expressions_and(&["mit OR apache-2.0", "gpl-2.0-plus"], true).unwrap();
        assert_eq!(result, "(mit OR apache-2.0) AND gpl-2.0-plus");
        let expr = super::super::parse::parse_expression(&result).unwrap();
        assert!(matches!(expr, LicenseExpression::And { .. }));
        let keys = expr.license_keys();
        assert_eq!(keys.len(), 3);
    }

    #[test]
    fn test_combine_expressions_parse_error() {
        let result = combine_expressions_and(&["mit", "@invalid@"], true);
        assert!(result.is_err());
    }

    #[test]
    fn test_combine_expressions_with_existing_and() {
        let result = combine_expressions_and(&["mit AND apache-2.0", "gpl-2.0"], true).unwrap();
        assert!(result.contains("mit"));
        assert!(result.contains("apache-2.0"));
        assert!(result.contains("gpl-2.0"));
    }

    #[test]
    fn test_combine_expressions_with_existing_or() {
        let result = combine_expressions_or(&["mit OR apache-2.0", "gpl-2.0"], true).unwrap();
        assert!(result.contains("mit"));
        assert!(result.contains("apache-2.0"));
        assert!(result.contains("gpl-2.0"));
    }

    #[test]
    fn test_expression_to_string_with_no_outer_parens() {
        let with_expr = LicenseExpression::With {
            left: Box::new(LicenseExpression::License("gpl-2.0-plus".to_string())),
            right: Box::new(LicenseExpression::License(
                "classpath-exception-2.0".to_string(),
            )),
        };
        assert_eq!(
            expression_to_string(&with_expr),
            "gpl-2.0-plus WITH classpath-exception-2.0"
        );
    }

    #[test]
    fn test_expression_to_string_with_as_right_operand_of_or() {
        let with_expr = LicenseExpression::With {
            left: Box::new(LicenseExpression::License("gpl-2.0".to_string())),
            right: Box::new(LicenseExpression::License(
                "classpath-exception-2.0".to_string(),
            )),
        };
        let or_expr = LicenseExpression::Or {
            left: Box::new(LicenseExpression::License("mit".to_string())),
            right: Box::new(with_expr),
        };
        assert_eq!(
            expression_to_string(&or_expr),
            "mit OR gpl-2.0 WITH classpath-exception-2.0"
        );
    }

    #[test]
    fn test_expression_to_string_with_as_right_operand_of_and() {
        let with_expr = LicenseExpression::With {
            left: Box::new(LicenseExpression::License("gpl-2.0".to_string())),
            right: Box::new(LicenseExpression::License(
                "classpath-exception-2.0".to_string(),
            )),
        };
        let and_expr = LicenseExpression::And {
            left: Box::new(LicenseExpression::License("mit".to_string())),
            right: Box::new(with_expr),
        };
        assert_eq!(
            expression_to_string(&and_expr),
            "mit AND gpl-2.0 WITH classpath-exception-2.0"
        );
    }

    #[test]
    fn test_expression_to_string_complex_precedence() {
        let input = "mit OR apache-2.0 AND gpl-2.0";
        let expr = super::super::parse::parse_expression(input).unwrap();
        assert_eq!(
            expression_to_string(&expr),
            "mit OR (apache-2.0 AND gpl-2.0)"
        );
    }

    #[test]
    fn test_expression_to_string_with_no_outer_parens_in_complex_and() {
        // WITH has higher precedence than AND
        // Parsed as: (bsd-new AND mit) AND (gpl-3.0-plus WITH autoconf-simple-exception)
        // When rendered with our nested structure, we get parens on the left
        let input = "bsd-new AND mit AND gpl-3.0-plus WITH autoconf-simple-exception";
        let expr = super::super::parse::parse_expression(input).unwrap();
        // Our parser creates nested AND structure, so we get parens
        assert_eq!(
            expression_to_string(&expr),
            "(bsd-new AND mit) AND gpl-3.0-plus WITH autoconf-simple-exception"
        );
    }
}

#[cfg(test)]
mod contains_tests {
    use super::*;

    #[test]
    fn test_basic_containment() {
        assert!(licensing_contains("mit", "mit"));
        assert!(!licensing_contains("mit", "apache"));
    }

    #[test]
    fn test_or_containment() {
        assert!(licensing_contains("mit OR apache", "mit"));
        assert!(licensing_contains("mit OR apache", "apache"));
        assert!(!licensing_contains("mit OR apache", "gpl"));
    }

    #[test]
    fn test_and_containment() {
        assert!(licensing_contains("mit AND apache", "mit"));
        assert!(licensing_contains("mit AND apache", "apache"));
        assert!(!licensing_contains("mit", "mit AND apache"));
    }

    #[test]
    fn test_expression_subset() {
        assert!(licensing_contains(
            "mit AND apache AND bsd",
            "mit AND apache"
        ));
        assert!(!licensing_contains(
            "mit AND apache",
            "mit AND apache AND bsd"
        ));
        assert!(licensing_contains("mit OR apache OR bsd", "mit OR apache"));
        assert!(!licensing_contains("mit OR apache", "mit OR apache OR bsd"));
    }

    #[test]
    fn test_order_independence() {
        assert!(licensing_contains("mit AND apache", "apache AND mit"));
        assert!(licensing_contains("mit OR apache", "apache OR mit"));
    }

    #[test]
    fn test_plus_suffix_no_containment() {
        assert!(!licensing_contains("gpl-2.0-plus", "gpl-2.0"));
        assert!(!licensing_contains("gpl-2.0", "gpl-2.0-plus"));
    }

    #[test]
    fn test_with_decomposition() {
        assert!(licensing_contains(
            "gpl-2.0 WITH classpath-exception",
            "gpl-2.0"
        ));
        assert!(licensing_contains(
            "gpl-2.0 WITH classpath-exception",
            "classpath-exception"
        ));
        assert!(!licensing_contains(
            "gpl-2.0",
            "gpl-2.0 WITH classpath-exception"
        ));
    }

    #[test]
    fn test_mixed_operators() {
        assert!(!licensing_contains("mit OR apache", "mit AND apache"));
        assert!(!licensing_contains("mit AND apache", "mit OR apache"));
    }

    #[test]
    fn test_nested_expressions() {
        assert!(!licensing_contains("(mit OR apache) AND bsd", "mit"));
        assert!(licensing_contains(
            "(mit OR apache) AND bsd",
            "mit OR apache"
        ));
        assert!(licensing_contains("(mit OR apache) AND bsd", "bsd"));
    }

    #[test]
    fn test_empty_expressions() {
        assert!(!licensing_contains("", "mit"));
        assert!(!licensing_contains("mit", ""));
        assert!(!licensing_contains("", ""));
        assert!(!licensing_contains("   ", "mit"));
    }

    #[test]
    fn test_invalid_expressions() {
        assert!(!licensing_contains("mit AND", "mit"));
        assert!(!licensing_contains("mit", "AND apache"));
    }
}
