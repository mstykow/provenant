use crate::license_detection::LicenseDetection as InternalLicenseDetection;
use crate::license_detection::LicenseDetectionEngine;
use crate::license_detection::PositionSet;
use crate::license_detection::models::LicenseMatch as InternalLicenseMatch;
use crate::license_detection::query::Query;
use crate::models::{FileInfoBuilder, LicenseDetection as PublicLicenseDetection, Match};
use crate::scanner::LicenseScanOptions;
use anyhow::Error;
use std::path::Path;
use std::sync::Arc;

pub(super) fn extract_license_information(
    file_info_builder: &mut FileInfoBuilder,
    scan_errors: &mut Vec<String>,
    path: &Path,
    text_content: String,
    license_engine: Option<Arc<LicenseDetectionEngine>>,
    license_options: LicenseScanOptions,
    from_binary_strings: bool,
) -> Result<(), Error> {
    let Some(engine) = license_engine else {
        return Ok(());
    };

    let detection_result = if license_options.min_score == 0 {
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
            let query =
                Query::from_extracted_text(&text_content, engine.index(), from_binary_strings).ok();
            let mut model_detections = Vec::new();
            let mut model_clues = Vec::new();

            for detection in &detections {
                let (public_detection, clue_matches) = convert_detection_to_model(
                    detection,
                    license_options,
                    &text_content,
                    query.as_ref(),
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
                    let combined = crate::utils::spdx::combine_license_expressions(expressions);
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
        Err(e) => {
            scan_errors.push(format!("License detection failed: {}", e));
        }
    }

    Ok(())
}

fn convert_detection_to_model(
    detection: &InternalLicenseDetection,
    license_options: LicenseScanOptions,
    text_content: &str,
    query: Option<&Query<'_>>,
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
                license_expression_spdx: detection
                    .license_expression_spdx
                    .clone()
                    .unwrap_or_default(),
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
    } else {
        (None, matches)
    }
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
        m.matched_text.clone().or_else(|| {
            Some(crate::license_detection::query::matched_text_from_text(
                text_content,
                m.start_line.get(),
                m.end_line.get(),
            ))
        })
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
        license_expression_spdx: m.license_expression_spdx.clone().unwrap_or_default(),
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
        return crate::license_detection::query::matched_text_from_text(
            &query.text,
            license_match.start_line.get(),
            license_match.end_line.get(),
        );
    };
    let Some(end_pos) = matched_positions.iter().max() else {
        return crate::license_detection::query::matched_text_from_text(
            &query.text,
            license_match.start_line.get(),
            license_match.end_line.get(),
        );
    };

    crate::license_detection::query::matched_text_diagnostics_from_text(
        &query.text,
        query,
        &matched_positions,
        start_pos,
        end_pos,
        license_match.start_line.get(),
        license_match.end_line.get(),
    )
}

#[cfg(test)]
#[path = "tests/license.rs"]
mod tests;
