use glob::Pattern;
use provenant::progress::{ProgressMode, ScanProgress};
use provenant::scanner::LicenseScanOptions;
use provenant::{FileType, TextDetectionOptions, collect_paths, process_collected};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct ExpectedAuthor {
    author: String,
    start_line: usize,
    end_line: usize,
}

fn hidden_progress() -> Arc<ScanProgress> {
    Arc::new(ScanProgress::new(ProgressMode::Quiet))
}

#[test]
fn scanner_matches_structured_credits_fixture() {
    let fixture_dir = PathBuf::from("testdata/scanner-copyright/credits");
    let fixture_path = fixture_dir.join("CREDITS");
    let expected_path = fixture_dir.join("CREDITS.expected-authors.json");

    let expected: Vec<ExpectedAuthor> = serde_json::from_str(
        &fs::read_to_string(&expected_path).expect("read expected authors fixture"),
    )
    .expect("parse expected authors fixture");

    let progress = hidden_progress();
    let patterns: Vec<Pattern> = vec![];
    let options = TextDetectionOptions {
        collect_info: false,
        detect_packages: false,
        detect_application_packages: false,
        detect_system_packages: false,
        detect_packages_in_compiled: false,
        detect_copyrights: true,
        detect_generated: false,
        detect_emails: false,
        detect_urls: false,
        max_emails: 50,
        max_urls: 50,
        timeout_seconds: 120.0,
        scan_cache_dir: None,
    };

    let collected = collect_paths(&fixture_dir, 0, &patterns);
    let result = process_collected(
        &collected,
        progress,
        None,
        LicenseScanOptions::default(),
        &options,
    );

    let scanned = result
        .files
        .into_iter()
        .find(|entry| {
            entry.file_type == FileType::File && entry.path == fixture_path.to_string_lossy()
        })
        .expect("fixture file should be present in scan result");

    let actual: Vec<ExpectedAuthor> = scanned
        .authors
        .into_iter()
        .map(|author| ExpectedAuthor {
            author: author.author,
            start_line: author.start_line,
            end_line: author.end_line,
        })
        .collect();

    assert_eq!(actual, expected);
    assert!(scanned.copyrights.is_empty());
    assert!(scanned.holders.is_empty());
}
