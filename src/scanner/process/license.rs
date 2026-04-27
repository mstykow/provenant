// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use crate::license_detection::LicenseDetection as InternalLicenseDetection;
use crate::license_detection::LicenseDetectionEngine;
use crate::license_detection::PositionSet;
use crate::license_detection::index::LicenseIndex;
use crate::license_detection::models::LicenseMatch as InternalLicenseMatch;
use crate::license_detection::query::Query;
use crate::models::{
    FileInfoBuilder, LicenseDetection as PublicLicenseDetection, Match, ScanDiagnostic,
};
use crate::scanner::LicenseScanOptions;
use anyhow::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

const MAX_OUTPUT_MATCHED_TEXT_LINE_LENGTH: usize = 10_000;
const MAX_OUTPUT_MATCHED_TEXT_BYTES: usize = 128 * 1024;
const MATCHED_TEXT_TRUNCATION_MARKER: &str = "… [truncated]";

pub(super) struct LicenseExtractionInput<'a> {
    pub(super) path: &'a Path,
    pub(super) text_content: String,
    pub(super) license_engine: Option<Arc<LicenseDetectionEngine>>,
    pub(super) license_options: LicenseScanOptions,
    pub(super) from_binary_strings: bool,
    pub(super) timeout_seconds: f64,
    pub(super) deadline: Option<Instant>,
}

pub(super) fn extract_license_information(
    file_info_builder: &mut FileInfoBuilder,
    scan_diagnostics: &mut Vec<ScanDiagnostic>,
    input: LicenseExtractionInput<'_>,
) -> Result<(), Error> {
    let LicenseExtractionInput {
        path,
        text_content,
        license_engine,
        license_options,
        from_binary_strings,
        timeout_seconds,
        deadline,
    } = input;

    let Some(engine) = license_engine else {
        return Ok(());
    };

    let detection_result = if deadline.is_some() {
        if license_options.min_score == 0 {
            engine.detect_with_kind_and_source_with_deadline(
                &text_content,
                license_options.unknown_licenses,
                from_binary_strings,
                &path.to_string_lossy(),
                deadline,
            )
        } else {
            engine.detect_with_kind_and_source_with_score_and_deadline(
                &text_content,
                license_options.unknown_licenses,
                from_binary_strings,
                &path.to_string_lossy(),
                f32::from(license_options.min_score),
                deadline,
            )
        }
    } else if license_options.min_score == 0 {
        engine.detect_with_kind_and_source(
            &text_content,
            license_options.unknown_licenses,
            from_binary_strings,
            &path.to_string_lossy(),
        )
    } else {
        engine.detect_with_kind_and_source_with_score(
            &text_content,
            license_options.unknown_licenses,
            from_binary_strings,
            &path.to_string_lossy(),
            f32::from(license_options.min_score),
        )
    };

    match detection_result {
        Ok(detections) => {
            let query = match if deadline.is_some() {
                Query::from_extracted_text_with_deadline(
                    &text_content,
                    engine.index(),
                    from_binary_strings,
                    deadline,
                )
            } else {
                Query::from_extracted_text(&text_content, engine.index(), from_binary_strings)
            } {
                Ok(query) => Some(query),
                Err(error) if is_license_detection_timeout_error(&error) => {
                    return Err(timeout_during_license_scan(timeout_seconds));
                }
                Err(_) => None,
            };
            let mut detections = detections;
            promote_legal_notice_low_quality_detections(&mut detections, path);

            let mut model_detections = Vec::new();
            let mut model_clues = Vec::new();

            for detection in &detections {
                let (public_detection, clue_matches) = convert_detection_to_model(
                    detection,
                    license_options,
                    &text_content,
                    query.as_ref(),
                    Some(engine.index()),
                );

                if let Some(public_detection) = public_detection {
                    model_detections.push(public_detection);
                }

                model_clues.extend(clue_matches);
            }

            if !model_detections.is_empty() {
                let expressions: Vec<String> = model_detections
                    .iter()
                    .filter(|d| !d.license_expression_spdx.is_empty())
                    .map(|d| d.license_expression_spdx.clone())
                    .collect();

                if !expressions.is_empty() {
                    let combined =
                        crate::utils::spdx::combine_license_expressions_preserving_structure(
                            expressions,
                        );
                    if let Some(expr) = combined {
                        file_info_builder.license_expression(Some(expr));
                    }
                }
            }

            file_info_builder.license_detections(model_detections);
            file_info_builder.license_clues(model_clues);
            file_info_builder.percentage_of_license_text(
                query
                    .as_ref()
                    .map(|query| compute_percentage_of_license_text(query, &detections)),
            );
        }
        Err(e) if is_license_detection_timeout_error(&e) => {
            return Err(timeout_during_license_scan(timeout_seconds));
        }
        Err(e) => {
            scan_diagnostics.push(ScanDiagnostic::error(format!(
                "License detection failed: {}",
                e
            )));
        }
    }

    Ok(())
}

fn is_license_detection_timeout_error(error: &Error) -> bool {
    error.to_string() == crate::license_detection::LICENSE_DETECTION_TIMEOUT_MESSAGE
}

fn timeout_during_license_scan(timeout_seconds: f64) -> Error {
    Error::msg(format!(
        "Timeout during license scan (> {:.2}s)",
        timeout_seconds
    ))
}

fn convert_detection_to_model(
    detection: &InternalLicenseDetection,
    license_options: LicenseScanOptions,
    text_content: &str,
    query: Option<&Query<'_>>,
    index: Option<&LicenseIndex>,
) -> (Option<PublicLicenseDetection>, Vec<Match>) {
    let matches: Vec<Match> = detection
        .matches
        .iter()
        .map(|m| convert_match_to_model(m, license_options, text_content, query))
        .collect();

    if let Some(license_expression) = detection.license_expression.clone() {
        (
            Some(PublicLicenseDetection {
                license_expression,
                license_expression_spdx: normalize_optional_spdx_expression(
                    detection.license_expression_spdx.as_deref(),
                ),
                matches,
                detection_log: if license_options.include_diagnostics {
                    detection.detection_log.clone()
                } else {
                    Vec::new()
                },
                identifier: detection.identifier.clone(),
            }),
            Vec::new(),
        )
    } else if let Some(public_detection) = index.and_then(|index| {
        promote_reference_url_clue_detection(detection, license_options, text_content, query, index)
    }) {
        (Some(public_detection), Vec::new())
    } else {
        (None, matches)
    }
}

fn promote_reference_url_clue_detection(
    detection: &InternalLicenseDetection,
    license_options: LicenseScanOptions,
    text_content: &str,
    query: Option<&Query<'_>>,
    index: &LicenseIndex,
) -> Option<PublicLicenseDetection> {
    let query = query?;

    let promoted_matches: Vec<&InternalLicenseMatch> = detection
        .matches
        .iter()
        .filter(|license_match| match_has_exact_reference_url(query, license_match, index))
        .collect();

    if promoted_matches.is_empty() {
        return None;
    }

    let license_expression = crate::utils::spdx::combine_license_expressions_preserving_structure(
        promoted_matches
            .iter()
            .map(|license_match| license_match.license_expression.clone()),
    )?;
    let license_expression_spdx =
        crate::utils::spdx::combine_license_expressions_preserving_structure(
            promoted_matches
                .iter()
                .filter_map(|license_match| license_match.license_expression_spdx.clone()),
        )
        .unwrap_or_default();
    let matches = promoted_matches
        .into_iter()
        .map(|license_match| {
            convert_match_to_model(license_match, license_options, text_content, Some(query))
        })
        .collect();

    Some(PublicLicenseDetection {
        license_expression,
        license_expression_spdx,
        matches,
        detection_log: if license_options.include_diagnostics {
            vec!["promoted-reference-url-license-clue".to_string()]
        } else {
            Vec::new()
        },
        identifier: detection.identifier.clone(),
    })
}

fn promote_legal_notice_low_quality_detections(
    detections: &mut [InternalLicenseDetection],
    path: &Path,
) {
    if !is_legal_notice_like_path(path) {
        return;
    }

    let has_concrete_detection = detections
        .iter()
        .any(|detection| detection.license_expression.is_some());
    if !has_concrete_detection {
        return;
    }

    for detection in detections {
        if detection.license_expression.is_some()
            || !detection
                .detection_log
                .iter()
                .any(|log| log == "low-quality-match-fragments")
            || detection.matches.is_empty()
        {
            continue;
        }

        if !detection.matches.iter().all(|license_match| {
            !license_match.is_license_clue()
                && !license_match.license_expression.is_empty()
                && !license_match.license_expression.contains("unknown")
        }) {
            continue;
        }

        let Some(license_expression) =
            crate::utils::spdx::combine_license_expressions_preserving_structure(
                detection
                    .matches
                    .iter()
                    .map(|license_match| license_match.license_expression.clone())
                    .collect::<Vec<_>>(),
            )
        else {
            continue;
        };
        let license_expression_spdx =
            crate::utils::spdx::combine_license_expressions_preserving_structure(
                detection
                    .matches
                    .iter()
                    .filter_map(|license_match| license_match.license_expression_spdx.clone())
                    .collect::<Vec<_>>(),
            );

        detection.license_expression = Some(license_expression);
        detection.license_expression_spdx = license_expression_spdx;
        detection
            .detection_log
            .push("promoted-low-quality-legal-notice".to_string());
    }
}

fn is_legal_notice_like_path(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(base_name) = path.file_stem().and_then(|stem| stem.to_str()) else {
        return false;
    };

    let name = name.to_ascii_lowercase();
    let base_name = base_name.to_ascii_lowercase();
    ["notice", "copyright", "copying", "license", "licence"]
        .iter()
        .any(|pattern| {
            name.starts_with(pattern)
                || name.ends_with(pattern)
                || base_name.starts_with(pattern)
                || base_name.ends_with(pattern)
        })
}

fn match_has_exact_reference_url(
    query: &Query<'_>,
    license_match: &InternalLicenseMatch,
    index: &LicenseIndex,
) -> bool {
    let Some(license) = index.licenses_by_key.get(&license_match.license_expression) else {
        return false;
    };

    if license.reference_urls.is_empty() {
        return false;
    }

    let matched_text = license_match.matched_text.clone().unwrap_or_else(|| {
        query.matched_text(license_match.start_line.get(), license_match.end_line.get())
    });
    let normalized_text = normalize_reference_url_candidate(&matched_text);
    if normalized_text.is_empty() {
        return false;
    }

    license.reference_urls.iter().any(|reference_url| {
        let normalized_reference = normalize_reference_url_candidate(reference_url);
        !normalized_reference.is_empty() && normalized_text.contains(&normalized_reference)
    })
}

fn normalize_reference_url_candidate(text: &str) -> String {
    text.trim().trim_end_matches('/').to_ascii_lowercase()
}

fn extract_output_matched_text(
    license_match: &InternalLicenseMatch,
    text_content: &str,
    query: Option<&Query<'_>>,
) -> String {
    if let Some(matched_text) = &license_match.matched_text {
        return cap_output_matched_text(matched_text.clone());
    }

    let start_line = license_match.start_line.get();
    let end_line = license_match.end_line.get();

    if line_range_has_oversized_line(
        text_content,
        start_line,
        end_line,
        MAX_OUTPUT_MATCHED_TEXT_LINE_LENGTH,
    ) {
        if let Some(compact_text) = compact_matched_text_from_query(query, license_match) {
            return cap_output_matched_text(compact_text);
        }

        return cap_output_matched_text(bounded_matched_text_from_text(
            text_content,
            start_line,
            end_line,
        ));
    }

    let whole_line =
        crate::license_detection::query::matched_text_from_text(text_content, start_line, end_line);

    if whole_line.len() > MAX_OUTPUT_MATCHED_TEXT_BYTES
        && let Some(compact_text) = compact_matched_text_from_query(query, license_match)
    {
        return cap_output_matched_text(compact_text);
    }

    cap_output_matched_text(whole_line)
}

fn compact_matched_text_from_query(
    query: Option<&Query<'_>>,
    license_match: &InternalLicenseMatch,
) -> Option<String> {
    let query = query?;
    let matched_positions: PositionSet = license_match.query_span().iter().collect();
    let start_pos = matched_positions.iter().min()?;
    let end_pos = matched_positions.iter().max()?;

    Some(crate::license_detection::query::matched_text_from_tokens(
        &query.text,
        query,
        &matched_positions,
        start_pos,
        end_pos,
        license_match.start_line.get(),
        license_match.end_line.get(),
    ))
}

fn line_range_has_oversized_line(
    text: &str,
    start_line: usize,
    end_line: usize,
    max_line_length: usize,
) -> bool {
    if start_line == 0 || end_line == 0 || start_line > end_line {
        return false;
    }

    text.lines().enumerate().any(|(idx, line)| {
        let line_num = idx + 1;
        line_num >= start_line && line_num <= end_line && line.len() > max_line_length
    })
}

fn bounded_matched_text_from_text(text: &str, start_line: usize, end_line: usize) -> String {
    matched_text_from_text_with_line_cap(
        text,
        start_line,
        end_line,
        MAX_OUTPUT_MATCHED_TEXT_LINE_LENGTH,
    )
}

fn matched_text_from_text_with_line_cap(
    text: &str,
    start_line: usize,
    end_line: usize,
    max_line_length: usize,
) -> String {
    if start_line == 0 || end_line == 0 || start_line > end_line {
        return String::new();
    }

    let mut selected_lines = Vec::new();

    for (idx, line) in text.split_inclusive('\n').enumerate() {
        let line_num = idx + 1;
        if line_num < start_line || line_num > end_line {
            continue;
        }

        let (line_text, line_ending) = split_line_ending(line);
        let capped_line = if line_text.len() > max_line_length {
            truncate_with_marker(line_text, max_line_length)
        } else {
            line_text.to_string()
        };

        selected_lines.push((capped_line, line_ending.to_string()));
    }

    let total_lines = selected_lines.len();
    let mut rendered = String::new();
    for (idx, (line_text, line_ending)) in selected_lines.into_iter().enumerate() {
        rendered.push_str(&line_text);
        if idx + 1 < total_lines {
            rendered.push_str(&line_ending);
        }
    }

    rendered
}

fn split_line_ending(line: &str) -> (&str, &str) {
    if let Some(line) = line.strip_suffix("\r\n") {
        (line, "\r\n")
    } else if let Some(line) = line.strip_suffix('\n') {
        (line, "\n")
    } else {
        (line, "")
    }
}

fn cap_output_matched_text(text: String) -> String {
    if text.len() <= MAX_OUTPUT_MATCHED_TEXT_BYTES {
        return text;
    }

    truncate_with_marker(&text, MAX_OUTPUT_MATCHED_TEXT_BYTES)
}

fn truncate_with_marker(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }

    if max_bytes <= MATCHED_TEXT_TRUNCATION_MARKER.len() {
        return truncate_at_char_boundary(MATCHED_TEXT_TRUNCATION_MARKER, max_bytes).to_string();
    }

    let prefix = truncate_at_char_boundary(
        text,
        max_bytes.saturating_sub(MATCHED_TEXT_TRUNCATION_MARKER.len()),
    );
    format!("{prefix}{MATCHED_TEXT_TRUNCATION_MARKER}")
}

fn truncate_at_char_boundary(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }

    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }

    &text[..end]
}

fn convert_match_to_model(
    m: &crate::license_detection::models::LicenseMatch,
    license_options: LicenseScanOptions,
    text_content: &str,
    query: Option<&Query<'_>>,
) -> Match {
    let rule_url = if m.rule_url.is_empty() {
        None
    } else {
        Some(m.rule_url.clone())
    };
    let matched_text = if license_options.include_text {
        Some(extract_output_matched_text(m, text_content, query))
    } else {
        None
    };
    let matched_text_diagnostics = if license_options.include_text_diagnostics {
        query.map(|query| matched_text_diagnostics_from_match(query, m))
    } else {
        None
    };
    Match {
        license_expression: m.license_expression.clone(),
        license_expression_spdx: normalize_optional_spdx_expression(
            m.license_expression_spdx.as_deref(),
        ),
        from_file: m.from_file.clone(),
        start_line: m.start_line,
        end_line: m.end_line,
        matcher: Some(m.matcher.to_string()),
        score: m.score,
        matched_length: Some(m.matched_length),
        match_coverage: Some((f64::from(m.coverage()) * 100.0).round() / 100.0),
        rule_relevance: Some(m.rule_relevance),
        rule_identifier: Some(m.rule_identifier.clone()),
        rule_url,
        matched_text,
        referenced_filenames: m.referenced_filenames.clone(),
        matched_text_diagnostics,
    }
}

fn normalize_optional_spdx_expression(expression: Option<&str>) -> String {
    let Some(expression) = expression
        .map(str::trim)
        .filter(|expression| !expression.is_empty())
    else {
        return String::new();
    };

    crate::utils::spdx::combine_license_expressions_preserving_structure(std::iter::once(
        expression.to_string(),
    ))
    .unwrap_or_else(|| expression.to_string())
}

fn compute_percentage_of_license_text(
    query: &Query<'_>,
    detections: &[InternalLicenseDetection],
) -> f64 {
    let matched_positions: std::collections::HashSet<usize> = detections
        .iter()
        .flat_map(|detection| detection.matches.iter())
        .flat_map(|m| m.query_span().iter())
        .collect();

    let query_tokens_length = query.tokens.len() + query.unknowns_by_pos.values().sum::<usize>();
    if query_tokens_length == 0 {
        return 0.0;
    }

    let percentage = (matched_positions.len() as f64 / query_tokens_length as f64) * 100.0;
    (percentage * 100.0).round() / 100.0
}

fn matched_text_diagnostics_from_match(
    query: &Query<'_>,
    license_match: &InternalLicenseMatch,
) -> String {
    let matched_positions: PositionSet = license_match.query_span().iter().collect();
    let Some(start_pos) = matched_positions.iter().min() else {
        return bounded_matched_text_from_text(
            &query.text,
            license_match.start_line.get(),
            license_match.end_line.get(),
        );
    };
    let Some(end_pos) = matched_positions.iter().max() else {
        return bounded_matched_text_from_text(
            &query.text,
            license_match.start_line.get(),
            license_match.end_line.get(),
        );
    };

    cap_output_matched_text(
        crate::license_detection::query::matched_text_diagnostics_from_text(
            &query.text,
            query,
            &matched_positions,
            start_pos,
            end_pos,
            license_match.start_line.get(),
            license_match.end_line.get(),
        ),
    )
}

#[cfg(test)]
#[path = "license_test.rs"]
mod tests;
