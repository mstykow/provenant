// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use crate::license_detection::expression::{
    combine_expressions_and, expression_to_string, parse_expression, simplify_expression,
};
use crate::models::{FileInfo, TallyEntry};
use crate::utils::spdx::combine_license_expressions;

pub(super) fn unique(values: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut unique_values = Vec::new();

    for value in values {
        if seen.insert(value.clone()) {
            unique_values.push(value.clone());
        }
    }

    unique_values
}

pub(super) fn remove_tally_value(value: Option<&str>, tallies: &[TallyEntry]) -> Vec<TallyEntry> {
    tallies
        .iter()
        .filter(|entry| {
            !entry
                .value
                .as_deref()
                .is_some_and(|entry_value| is_redundant_declared_license_tally(entry_value, value))
        })
        .cloned()
        .collect()
}

fn is_redundant_declared_license_tally(entry_value: &str, declared_value: Option<&str>) -> bool {
    let Some(declared_value) = declared_value else {
        return false;
    };

    if entry_value == declared_value {
        return true;
    }

    if declared_value.contains(" AND ")
        || declared_value.contains(" OR ")
        || declared_value.contains(" WITH ")
    {
        return false;
    }

    let normalized_declared = declared_value.trim().to_ascii_lowercase();
    let parts: Vec<String> = entry_value
        .replace(['(', ')'], " ")
        .split_whitespace()
        .filter(|part| !matches!(part.to_ascii_uppercase().as_str(), "AND" | "OR" | "WITH"))
        .map(|part| part.to_ascii_lowercase())
        .collect();

    !parts.is_empty() && parts.iter().all(|part| part == &normalized_declared)
}

pub(super) fn remove_tally_values(values: &[String], tallies: &[TallyEntry]) -> Vec<TallyEntry> {
    let normalized_values: HashSet<String> = values
        .iter()
        .map(|value| normalize_summary_holder_value(value))
        .collect();

    tallies
        .iter()
        .filter(|entry| {
            !entry.value.as_ref().is_some_and(|value| {
                values.contains(value)
                    || normalized_values.contains(&normalize_summary_holder_value(value))
            })
        })
        .cloned()
        .collect()
}

pub(super) fn canonicalize_summary_holder_display(value: &str) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");

    let key: String = normalized
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();

    match key.as_str() {
        "google" | "googlellc" | "googleinc" => "Google".to_string(),
        "microsoft" | "microsoftcorp" | "microsoftinc" | "microsoftcorporation" => {
            "Microsoft".to_string()
        }
        "sunmicrosystems" | "sunmicrosystemsinc" => "Sun Microsystems".to_string(),
        _ => normalized,
    }
}

pub(super) fn summary_holder_from_copyright(copyright: &str) -> Option<String> {
    let mut value = copyright.trim();
    if value.is_empty() {
        return None;
    }

    if value.len() >= "copyright".len()
        && value[.."copyright".len()].eq_ignore_ascii_case("copyright")
    {
        value = value["copyright".len()..].trim_start();
    }

    if let Some(stripped) = value.strip_prefix("(c)") {
        value = stripped.trim_start();
    }
    if let Some(stripped) = value.strip_prefix('©') {
        value = stripped.trim_start();
    }

    let cleaned = value.trim_matches(|ch: char| ch.is_whitespace() || ch == ',');
    if cleaned.is_empty() {
        return None;
    }

    if cleaned.starts_with("Holders ") || cleaned.contains("option either") {
        return None;
    }

    let cleaned = cleaned
        .strip_suffix(". Individual")
        .unwrap_or(cleaned)
        .trim();

    let cleaned = if cleaned.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        cleaned
            .trim_start_matches(|ch: char| {
                ch.is_ascii_digit() || ch == ' ' || ch == ',' || ch == '-'
            })
            .trim()
    } else {
        cleaned
    };

    let cleaned_without_email = cleaned
        .split_whitespace()
        .take_while(|token| !token.contains('@'))
        .collect::<Vec<_>>()
        .join(" ");
    let cleaned = if cleaned_without_email.is_empty() {
        cleaned
    } else {
        cleaned_without_email.as_str()
    };

    (!cleaned.is_empty()).then(|| cleaned.to_string())
}

pub(super) fn clean_legal_holder_candidate(holder: &str) -> Option<String> {
    let cleaned = holder.trim();
    if cleaned.is_empty()
        || cleaned.contains("option either")
        || cleaned.starts_with("messages,")
        || cleaned.starts_with("together with instructions")
    {
        return None;
    }

    let cleaned = cleaned
        .strip_suffix(". Individual")
        .unwrap_or(cleaned)
        .trim();

    (!cleaned.is_empty()).then(|| cleaned.to_string())
}

pub(super) fn canonicalize_summary_expression(expression: &str) -> String {
    let canonical = parse_expression(expression)
        .map(|parsed| expression_to_string(&simplify_expression(&parsed)))
        .or_else(|_| combine_expressions_and(&[expression], true))
        .unwrap_or_else(|_| expression.to_ascii_lowercase());

    if canonical.contains(" AND ") && !canonical.contains(" OR ") && !canonical.contains(" WITH ") {
        canonical
            .replace(['(', ')'], "")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        canonical
    }
}

pub(super) fn normalize_summary_holder_value(value: &str) -> String {
    let normalized = canonicalize_summary_holder_display(value)
        .trim_end_matches(['.', ',', ';', ':'])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();

    let key: String = normalized
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect();

    match key.as_str() {
        "google" | "googlellc" | "googleinc" => "google".to_string(),
        "microsoft" | "microsoftcorp" | "microsoftinc" | "microsoftcorporation" => {
            "microsoft".to_string()
        }
        _ => normalized,
    }
}

pub(super) fn summary_license_expression(file: &FileInfo) -> Option<String> {
    let mut detection_expressions: Vec<_> = file
        .license_detections
        .iter()
        .map(|detection| detection.license_expression.clone())
        .collect();

    if detection_expressions.is_empty() {
        detection_expressions.extend(
            file.package_data
                .iter()
                .flat_map(|package_data| package_data.license_detections.iter())
                .map(|detection| detection.license_expression.clone()),
        );
    }

    let detection_expressions = unique(&detection_expressions);

    if !detection_expressions.is_empty() {
        return if detection_expressions.len() == 1 {
            detection_expressions
                .into_iter()
                .next()
                .map(|expr| canonicalize_summary_expression(&expr))
        } else {
            combine_license_expressions(detection_expressions)
                .map(|expr| canonicalize_summary_expression(&expr))
        };
    }

    file.license_expression
        .as_deref()
        .map(canonicalize_summary_expression)
}

pub(super) fn package_primary_detected_license_values(
    file: &FileInfo,
    skip_unknown: bool,
) -> Vec<String> {
    if !file.license_detections.is_empty() {
        return Vec::new();
    }

    let mut values = file
        .package_data
        .iter()
        .flat_map(|package_data| {
            package_data
                .license_detections
                .iter()
                .map(|detection| canonicalize_summary_expression(&detection.license_expression))
                .chain(
                    package_data
                        .declared_license_expression
                        .as_deref()
                        .map(canonicalize_summary_expression),
                )
        })
        .collect::<Vec<_>>();

    if skip_unknown {
        values.retain(|expression| expression != "unknown-license-reference");
    }

    unique(&values)
}

pub(super) fn package_other_detected_license_values(
    file: &FileInfo,
    skip_unknown: bool,
) -> Vec<String> {
    let mut values = file
        .package_data
        .iter()
        .flat_map(|package_data| {
            package_data
                .other_license_detections
                .iter()
                .map(|detection| canonicalize_summary_expression(&detection.license_expression))
                .chain(
                    package_data
                        .other_license_expression
                        .as_deref()
                        .map(canonicalize_summary_expression),
                )
        })
        .collect::<Vec<_>>();

    if skip_unknown {
        values.retain(|expression| expression != "unknown-license-reference");
    }

    unique(&values)
}

pub(super) fn key_file_has_license_text(file: &FileInfo) -> bool {
    file.license_detections
        .iter()
        .chain(
            file.license_detections
                .is_empty()
                .then_some(())
                .into_iter()
                .flat_map(|_| {
                    file.package_data
                        .iter()
                        .flat_map(|package_data| package_data.license_detections.iter())
                }),
        )
        .flat_map(|detection| detection.matches.iter())
        .any(|license_match| {
            license_match.matched_length.unwrap_or_default() > 1
                || license_match.match_coverage.unwrap_or_default() > 1.0
        })
}
