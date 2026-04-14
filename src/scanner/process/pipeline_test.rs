use super::{
    LARGE_NON_SOURCE_JSON_LICENSE_TEXT_BYTES, cap_non_source_json_license_text,
    maybe_record_processing_timeout, process_file,
};
use crate::progress::{ProgressMode, ScanProgress};
use crate::scanner::{LicenseScanOptions, TextDetectionOptions};
use crate::utils::file::FileInfoClassification;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};
use tempfile::tempdir;

#[test]
fn test_process_file_suppresses_non_actionable_pdf_extraction_failure() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("broken.pdf");
    fs::write(&path, b"%PDF-1.7\nthis is not a valid pdf object graph\n")
        .expect("write malformed pdf");
    let metadata = fs::metadata(&path).expect("metadata");
    let progress = ScanProgress::new(ProgressMode::Quiet);

    let file_info = process_file(
        &path,
        &metadata,
        &progress,
        None,
        LicenseScanOptions::default(),
        &TextDetectionOptions::default(),
    );

    assert!(file_info.scan_errors.is_empty());
}

#[test]
fn test_processing_timeout_is_not_duplicated_after_stage_specific_timeout() {
    let started = Instant::now() - Duration::from_secs(2);
    let mut scan_errors = vec!["Timeout before license scan (> 1.00s)".to_string()];

    maybe_record_processing_timeout(&mut scan_errors, started, 1.0);

    assert_eq!(scan_errors, vec!["Timeout before license scan (> 1.00s)"]);
}

#[test]
fn test_processing_timeout_is_recorded_when_no_timeout_error_exists() {
    let started = Instant::now() - Duration::from_secs(2);
    let mut scan_errors = Vec::new();

    maybe_record_processing_timeout(&mut scan_errors, started, 1.0);

    assert_eq!(
        scan_errors,
        vec!["Processing interrupted due to timeout after 1.00 seconds"]
    );
}

#[test]
fn test_cap_non_source_json_license_text_truncates_large_json() {
    let classification = FileInfoClassification {
        mime_type: "application/json".to_string(),
        file_type: "JSON text data".to_string(),
        programming_language: None,
        is_binary: false,
        is_text: true,
        is_archive: false,
        is_media: false,
        is_source: false,
        is_script: false,
    };
    let large_json = format!("{{\"items\":\"{}\"}}", "x".repeat(200_000));

    let capped = cap_non_source_json_license_text(
        Path::new("resolution.json"),
        &classification,
        &large_json,
    );

    assert!(capped.len() <= LARGE_NON_SOURCE_JSON_LICENSE_TEXT_BYTES);
    assert!(capped.len() < large_json.len());
}

#[test]
fn test_cap_non_source_json_license_text_keeps_sourcemaps_intact() {
    let classification = FileInfoClassification {
        mime_type: "application/json".to_string(),
        file_type: "JSON text data".to_string(),
        programming_language: None,
        is_binary: false,
        is_text: true,
        is_archive: false,
        is_media: false,
        is_source: false,
        is_script: false,
    };
    let large_json = format!("{{\"mappings\":\"{}\"}}", "x".repeat(200_000));

    let capped =
        cap_non_source_json_license_text(Path::new("bundle.js.map"), &classification, &large_json);

    assert_eq!(capped.as_ref(), large_json);
}

#[test]
fn test_cap_non_source_json_license_text_keeps_package_locks_intact() {
    let classification = FileInfoClassification {
        mime_type: "application/json".to_string(),
        file_type: "JSON text data".to_string(),
        programming_language: None,
        is_binary: false,
        is_text: true,
        is_archive: false,
        is_media: false,
        is_source: false,
        is_script: false,
    };
    let large_json = format!("{{\"packages\":\"{}\"}}", "x".repeat(200_000));

    let capped = cap_non_source_json_license_text(
        Path::new("package-lock.json"),
        &classification,
        &large_json,
    );

    assert_eq!(capped.as_ref(), large_json);
}

#[test]
fn test_cap_non_source_json_license_text_keeps_npm_shrinkwrap_intact() {
    let classification = FileInfoClassification {
        mime_type: "application/json".to_string(),
        file_type: "JSON text data".to_string(),
        programming_language: None,
        is_binary: false,
        is_text: true,
        is_archive: false,
        is_media: false,
        is_source: false,
        is_script: false,
    };
    let large_json = format!("{{\"packages\":\"{}\"}}", "x".repeat(200_000));

    let capped = cap_non_source_json_license_text(
        Path::new("npm-shrinkwrap.json"),
        &classification,
        &large_json,
    );

    assert_eq!(capped.as_ref(), large_json);
}
