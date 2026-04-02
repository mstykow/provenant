use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::license_detection::{LicenseDetectionEngine, LicenseMatch};
use crate::utils::file::{ExtractedTextKind, classify_file_info, extract_text_for_detection};
use crate::utils::sourcemap::{extract_sourcemap_content, is_sourcemap};
use crate::utils::text::{
    remove_verbatim_escape_sequences, should_remove_verbatim_escape_sequences,
};

pub fn read_golden_input_content(test_file: &Path) -> Result<Option<(String, ExtractedTextKind)>> {
    let bytes =
        fs::read(test_file).with_context(|| format!("Failed to read {}", test_file.display()))?;
    let (text, text_kind) = extract_text_for_detection(test_file, &bytes);
    let classification = classify_file_info(test_file, &bytes);

    if text.is_empty() {
        return Ok(None);
    }

    if is_sourcemap(test_file) {
        Ok(Some((
            extract_sourcemap_content(&text).unwrap_or(text),
            text_kind,
        )))
    } else if should_remove_verbatim_escape_sequences(test_file, classification.is_source) {
        Ok(Some((remove_verbatim_escape_sequences(&text), text_kind)))
    } else {
        Ok(Some((text, text_kind)))
    }
}

pub fn detect_matches_for_golden(
    engine: &LicenseDetectionEngine,
    test_file: &Path,
    unknown_licenses: bool,
) -> Result<Vec<LicenseMatch>> {
    let Some((text, text_kind)) = read_golden_input_content(test_file)? else {
        return Ok(Vec::new());
    };

    engine
        .detect_matches_with_kind(
            &text,
            unknown_licenses,
            matches!(text_kind, ExtractedTextKind::BinaryStrings),
        )
        .with_context(|| format!("Detection failed for {}", test_file.display()))
}

pub fn detect_license_expressions_for_golden(
    engine: &LicenseDetectionEngine,
    test_file: &Path,
    unknown_licenses: bool,
) -> Result<Vec<String>> {
    Ok(
        detect_matches_for_golden(engine, test_file, unknown_licenses)?
            .into_iter()
            .map(|m| m.license_expression)
            .collect(),
    )
}
