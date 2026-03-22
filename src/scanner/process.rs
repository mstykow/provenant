use crate::license_detection::LicenseDetectionEngine;
use crate::parsers::try_parse_file;
use crate::utils::hash::{calculate_md5, calculate_sha1, calculate_sha256};
use crate::utils::language::detect_language;
use crate::utils::text::{is_source, remove_verbatim_escape_sequences};
use anyhow::Error;
use glob::Pattern;
use log::warn;
use mime_guess::from_path;
use rayon::prelude::*;
use std::fs::{self};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::cache::{CachedScanFindings, read_cached_findings, write_cached_findings};
use crate::copyright::{
    self, AuthorDetection, CopyrightDetection, CopyrightDetectionOptions, HolderDetection,
};
use crate::finder::{self, DetectionConfig};
use crate::models::{
    Author, Copyright, FileInfo, FileInfoBuilder, FileType, Holder, LicenseDetection, Match,
    OutputEmail, OutputURL,
};
use crate::progress::ScanProgress;
use crate::scanner::{ProcessResult, TextDetectionOptions};
use crate::utils::file::{
    ExtractedTextKind, extract_text_for_detection, get_creation_date, is_path_excluded,
};

const PEM_CERTIFICATE_HEADERS: &[(&str, &str)] = &[
    ("-----BEGIN CERTIFICATE-----", "-----END CERTIFICATE-----"),
    (
        "-----BEGIN TRUSTED CERTIFICATE-----",
        "-----END TRUSTED CERTIFICATE-----",
    ),
];

/// Scan a directory tree and produce [`ProcessResult`] entries.
///
/// This traverses files/directories up to `max_depth`, applies exclusion
/// patterns, extracts metadata, and performs license/copyright parsing.
pub fn process<P: AsRef<Path>>(
    path: P,
    max_depth: usize,
    progress: Arc<ScanProgress>,
    exclude_patterns: &[Pattern],
    license_engine: Option<Arc<LicenseDetectionEngine>>,
    include_text: bool,
) -> Result<ProcessResult, Error> {
    process_with_options(
        path,
        max_depth,
        progress,
        exclude_patterns,
        license_engine,
        include_text,
        &TextDetectionOptions::default(),
    )
}

pub fn process_with_options<P: AsRef<Path>>(
    path: P,
    max_depth: usize,
    progress: Arc<ScanProgress>,
    exclude_patterns: &[Pattern],
    license_engine: Option<Arc<LicenseDetectionEngine>>,
    include_text: bool,
    text_options: &TextDetectionOptions,
) -> Result<ProcessResult, Error> {
    let depth_limit = depth_limit_from_cli(max_depth);
    process_with_options_internal(
        path.as_ref(),
        depth_limit,
        progress,
        exclude_patterns,
        license_engine,
        include_text,
        text_options,
    )
}

fn depth_limit_from_cli(max_depth: usize) -> Option<usize> {
    if max_depth == 0 {
        None
    } else {
        Some(max_depth)
    }
}

fn process_with_options_internal(
    path: &Path,
    depth_limit: Option<usize>,
    progress: Arc<ScanProgress>,
    exclude_patterns: &[Pattern],
    license_engine: Option<Arc<LicenseDetectionEngine>>,
    include_text: bool,
    text_options: &TextDetectionOptions,
) -> Result<ProcessResult, Error> {
    if is_path_excluded(path, exclude_patterns) {
        return Ok(ProcessResult {
            files: Vec::new(),
            excluded_count: 1,
        });
    }

    let mut all_files = Vec::new();
    let mut total_excluded = 0;

    // Read directory entries and group by exclusion status and type
    let entries: Vec<_> = fs::read_dir(path)?.filter_map(Result::ok).collect();

    let mut file_entries = Vec::new();
    let mut dir_entries = Vec::new();

    for entry in entries {
        let path = entry.path();

        // Check exclusion only once per path
        if is_path_excluded(&path, exclude_patterns) {
            total_excluded += 1;
            continue;
        }

        match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => file_entries.push((path, metadata)),
            Ok(metadata) if path.is_dir() => dir_entries.push((path, metadata)),
            _ => continue,
        }
    }

    // Process files in parallel
    all_files.append(
        &mut file_entries
            .par_iter()
            .map(|(path, metadata)| {
                let file_entry = process_file(
                    path,
                    metadata,
                    license_engine.clone(),
                    include_text,
                    text_options,
                );
                progress.file_completed(path, metadata.len(), &file_entry.scan_errors);
                file_entry
            })
            .collect(),
    );

    // Process directories
    for (path, metadata) in dir_entries {
        all_files.push(process_directory(&path, &metadata));

        let should_recurse = match depth_limit {
            None => true,
            Some(remaining_depth) => remaining_depth > 0,
        };

        if should_recurse {
            let next_depth_limit = depth_limit.map(|remaining_depth| remaining_depth - 1);
            match process_with_options_internal(
                &path,
                next_depth_limit,
                progress.clone(),
                exclude_patterns,
                license_engine.clone(),
                include_text,
                text_options,
            ) {
                Ok(mut result) => {
                    all_files.append(&mut result.files);
                    total_excluded += result.excluded_count;
                }
                Err(e) => progress.record_runtime_error(&path, &e.to_string()),
            }
        }
    }

    Ok(ProcessResult {
        files: all_files,
        excluded_count: total_excluded,
    })
}

fn process_file(
    path: &Path,
    metadata: &fs::Metadata,
    license_engine: Option<Arc<LicenseDetectionEngine>>,
    include_text: bool,
    text_options: &TextDetectionOptions,
) -> FileInfo {
    let mut scan_errors: Vec<String> = vec![];
    let mut file_info_builder = FileInfoBuilder::default();

    let started = Instant::now();

    if let Err(e) = extract_information_from_content(
        &mut file_info_builder,
        path,
        license_engine,
        include_text,
        text_options,
    ) {
        scan_errors.push(e.to_string());
    };

    if is_timeout_exceeded(started, text_options.timeout_seconds) {
        scan_errors.push(format!(
            "Processing interrupted due to timeout after {:.2} seconds",
            text_options.timeout_seconds
        ));
    }

    let mut file_info = file_info_builder
        .name(path.file_name().unwrap().to_string_lossy().to_string())
        .base_name(
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
        )
        .extension(
            path.extension()
                .map_or("".to_string(), |ext| format!(".{}", ext.to_string_lossy())),
        )
        .path(path.to_string_lossy().to_string())
        .file_type(FileType::File)
        .mime_type(Some(
            from_path(path)
                .first_or_octet_stream()
                .essence_str()
                .to_string(),
        ))
        .size(metadata.len())
        .date(get_creation_date(metadata))
        .scan_errors(scan_errors)
        .build()
        .expect("FileInformationBuild not completely initialized");

    if file_info.programming_language.as_deref() == Some("Go")
        && is_go_non_production_source(path).unwrap_or(false)
    {
        file_info.is_source = Some(false);
    }

    if let (Some(scan_results_dir), Some(sha256)) = (
        text_options.scan_cache_dir.as_deref(),
        file_info.sha256.as_deref(),
    ) && file_info.scan_errors.is_empty()
    {
        let findings = CachedScanFindings::from_file_info(&file_info);
        let options_fingerprint = scan_cache_fingerprint(text_options);
        if let Err(err) =
            write_cached_findings(scan_results_dir, sha256, &options_fingerprint, &findings)
        {
            file_info
                .scan_errors
                .push(format!("Failed to write scan cache entry: {err}"));
        }
    }

    file_info
}

fn extract_information_from_content(
    file_info_builder: &mut FileInfoBuilder,
    path: &Path,
    license_engine: Option<Arc<LicenseDetectionEngine>>,
    include_text: bool,
    text_options: &TextDetectionOptions,
) -> Result<(), Error> {
    let started = Instant::now();
    let buffer = fs::read(path)?;

    if is_timeout_exceeded(started, text_options.timeout_seconds) {
        return Err(Error::msg(format!(
            "Timeout while reading file content (> {:.2}s)",
            text_options.timeout_seconds
        )));
    }

    let sha256 = calculate_sha256(&buffer);

    file_info_builder
        .sha1(Some(calculate_sha1(&buffer)))
        .md5(Some(calculate_md5(&buffer)))
        .sha256(Some(sha256.clone()))
        .programming_language(Some(detect_language(path, &buffer)));

    if should_skip_text_detection(path, &buffer) {
        return Ok(());
    }

    if let Some(scan_results_dir) = text_options.scan_cache_dir.as_deref() {
        let options_fingerprint = scan_cache_fingerprint(text_options);
        match read_cached_findings(scan_results_dir, &sha256, &options_fingerprint) {
            Ok(Some(findings)) => {
                file_info_builder
                    .package_data(findings.package_data)
                    .license_expression(findings.license_expression)
                    .license_detections(findings.license_detections)
                    .copyrights(findings.copyrights)
                    .holders(findings.holders)
                    .authors(findings.authors)
                    .emails(findings.emails)
                    .urls(findings.urls)
                    .programming_language(findings.programming_language);
                return Ok(());
            }
            Ok(None) => {}
            Err(err) => {
                warn!("Failed to read scan cache for {:?}: {}", path, err);
            }
        }
    }

    // Package parsing and text-based detection (copyright, license) are independent.
    // Python ScanCode runs all enabled plugins on every file, so we do the same.
    if let Some(package_data) = try_parse_file(path) {
        file_info_builder.package_data(package_data);
    }

    if is_timeout_exceeded(started, text_options.timeout_seconds) {
        return Err(Error::msg(format!(
            "Timeout while extracting package/text metadata (> {:.2}s)",
            text_options.timeout_seconds
        )));
    }

    let (text_content, text_kind) = extract_text_for_detection(path, &buffer);
    let from_binary_strings = matches!(text_kind, ExtractedTextKind::BinaryStrings);

    if is_timeout_exceeded(started, text_options.timeout_seconds) {
        return Err(Error::msg(format!(
            "Timeout while extracting text content (> {:.2}s)",
            text_options.timeout_seconds
        )));
    }

    if text_content.is_empty() {
        return Ok(());
    }

    if text_options.detect_copyrights {
        extract_copyright_information(
            file_info_builder,
            path,
            &text_content,
            text_options.timeout_seconds,
            from_binary_strings,
        );
    }
    extract_email_url_information(file_info_builder, &text_content, text_options);

    if is_timeout_exceeded(started, text_options.timeout_seconds) {
        return Err(Error::msg(format!(
            "Timeout before license scan (> {:.2}s)",
            text_options.timeout_seconds
        )));
    }
    // Handle source map files specially
    let text_content_for_license_detection = if crate::utils::sourcemap::is_sourcemap(path) {
        if let Some(sourcemap_content) =
            crate::utils::sourcemap::extract_sourcemap_content(&text_content)
        {
            sourcemap_content
        } else {
            text_content
        }
    } else if is_source(path) {
        remove_verbatim_escape_sequences(&text_content)
    } else {
        text_content
    };

    extract_license_information(
        file_info_builder,
        text_content_for_license_detection,
        license_engine,
        include_text,
        from_binary_strings,
    )
}

fn is_timeout_exceeded(started: Instant, timeout_seconds: f64) -> bool {
    timeout_seconds.is_finite()
        && timeout_seconds > 0.0
        && started.elapsed().as_secs_f64() > timeout_seconds
}

fn scan_cache_fingerprint(text_options: &TextDetectionOptions) -> String {
    format!(
        "copyrights={};emails={};urls={};max_emails={};max_urls={};timeout={:.6}",
        text_options.detect_copyrights,
        text_options.detect_emails,
        text_options.detect_urls,
        text_options.max_emails,
        text_options.max_urls,
        text_options.timeout_seconds,
    )
}

fn extract_copyright_information(
    file_info_builder: &mut FileInfoBuilder,
    path: &Path,
    text_content: &str,
    timeout_seconds: f64,
    from_binary_strings: bool,
) {
    // CREDITS files get special handling (Linux kernel style).
    if copyright::is_credits_file(path) {
        let author_detections = copyright::detect_credits_authors(text_content);
        if !author_detections.is_empty() {
            file_info_builder.authors(
                author_detections
                    .into_iter()
                    .map(|a| Author {
                        author: a.author,
                        start_line: a.start_line,
                        end_line: a.end_line,
                    })
                    .collect(),
            );
            return;
        }
    }

    let copyright_options = CopyrightDetectionOptions {
        max_runtime: if timeout_seconds.is_finite() && timeout_seconds > 0.0 {
            Some(Duration::from_secs_f64(timeout_seconds))
        } else {
            None
        },
        ..CopyrightDetectionOptions::default()
    };

    let (copyrights, holders, authors) =
        copyright::detect_copyrights_with_options(text_content, &copyright_options);
    let (copyrights, holders, authors) = if from_binary_strings {
        prune_binary_string_detections(copyrights, holders, authors)
    } else {
        (copyrights, holders, authors)
    };

    file_info_builder.copyrights(
        copyrights
            .into_iter()
            .map(|c| Copyright {
                copyright: c.copyright,
                start_line: c.start_line,
                end_line: c.end_line,
            })
            .collect::<Vec<Copyright>>(),
    );
    file_info_builder.holders(
        holders
            .into_iter()
            .map(|h| Holder {
                holder: h.holder,
                start_line: h.start_line,
                end_line: h.end_line,
            })
            .collect::<Vec<Holder>>(),
    );
    file_info_builder.authors(
        authors
            .into_iter()
            .map(|a| Author {
                author: a.author,
                start_line: a.start_line,
                end_line: a.end_line,
            })
            .collect::<Vec<Author>>(),
    );
}

fn prune_binary_string_detections(
    copyrights: Vec<CopyrightDetection>,
    holders: Vec<HolderDetection>,
    _authors: Vec<AuthorDetection>,
) -> (
    Vec<CopyrightDetection>,
    Vec<HolderDetection>,
    Vec<AuthorDetection>,
) {
    let kept_copyrights: Vec<CopyrightDetection> = copyrights
        .into_iter()
        .filter(|c| is_binary_string_copyright_candidate(&c.copyright))
        .collect();

    let kept_holders: Vec<HolderDetection> = holders
        .into_iter()
        .filter(|holder| {
            kept_copyrights.iter().any(|copyright| {
                ranges_overlap(
                    holder.start_line,
                    holder.end_line,
                    copyright.start_line,
                    copyright.end_line,
                )
            })
        })
        .collect();

    (kept_copyrights, kept_holders, Vec::new())
}

fn ranges_overlap(a_start: usize, a_end: usize, b_start: usize, b_end: usize) -> bool {
    a_start <= b_end && b_start <= a_end
}

fn is_binary_string_copyright_candidate(text: &str) -> bool {
    if has_explicit_copyright_marker(text) || contains_year(text) {
        return true;
    }

    let lower = text.to_ascii_lowercase();
    let Some(tail) = lower.strip_prefix("copyright") else {
        return true;
    };
    let tail = tail.trim();
    let alpha_tokens: Vec<&str> = tail
        .split_whitespace()
        .filter(|token| token.chars().any(|c| c.is_alphabetic()))
        .collect();

    if alpha_tokens.len() <= 1 {
        return true;
    }

    if tail.contains(',') || tail.contains(" and ") || tail.contains('&') {
        return true;
    }

    alpha_tokens
        .iter()
        .any(|token| is_company_like_suffix(token.trim_matches(|c: char| !c.is_alphanumeric())))
}

fn has_explicit_copyright_marker(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("(c)") || lower.contains('©') || lower.contains("copr")
}

fn contains_year(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.windows(4).any(|window| {
        window.iter().all(|b| b.is_ascii_digit())
            && matches!(window[0], b'1' | b'2')
            && matches!(window[1], b'9' | b'0')
    })
}

fn is_company_like_suffix(token: &str) -> bool {
    matches!(
        token.to_ascii_lowercase().as_str(),
        "inc"
            | "corp"
            | "corporation"
            | "co"
            | "company"
            | "ltd"
            | "llc"
            | "gmbh"
            | "foundation"
            | "project"
            | "systems"
            | "software"
            | "technologies"
            | "technology"
    )
}

fn extract_email_url_information(
    file_info_builder: &mut FileInfoBuilder,
    text_content: &str,
    text_options: &TextDetectionOptions,
) {
    if !text_options.detect_emails && !text_options.detect_urls {
        return;
    }

    if text_options.detect_emails {
        let config = DetectionConfig {
            max_emails: text_options.max_emails,
            max_urls: text_options.max_urls,
            unique: false,
        };
        let emails = finder::find_emails(text_content, &config)
            .into_iter()
            .map(|d| OutputEmail {
                email: d.email,
                start_line: d.start_line,
                end_line: d.end_line,
            })
            .collect::<Vec<_>>();
        file_info_builder.emails(emails);
    }

    if text_options.detect_urls {
        let config = DetectionConfig {
            max_emails: text_options.max_emails,
            max_urls: text_options.max_urls,
            unique: true,
        };
        let urls = finder::find_urls(text_content, &config)
            .into_iter()
            .map(|d| OutputURL {
                url: d.url,
                start_line: d.start_line,
                end_line: d.end_line,
            })
            .collect::<Vec<_>>();
        file_info_builder.urls(urls);
    }
}

fn extract_license_information(
    file_info_builder: &mut FileInfoBuilder,
    text_content: String,
    license_engine: Option<Arc<LicenseDetectionEngine>>,
    include_text: bool,
    from_binary_strings: bool,
) -> Result<(), Error> {
    let Some(engine) = license_engine else {
        return Ok(());
    };

    match engine.detect_with_kind(&text_content, false, from_binary_strings) {
        Ok(detections) => {
            let model_detections: Vec<LicenseDetection> = detections
                .into_iter()
                .filter_map(|d| convert_detection_to_model(d, include_text))
                .collect();

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
        }
        Err(e) => {
            warn!("License detection failed: {}", e);
        }
    }

    Ok(())
}

fn convert_detection_to_model(
    detection: crate::license_detection::LicenseDetection,
    include_text: bool,
) -> Option<LicenseDetection> {
    let license_expression = detection.license_expression?;
    let license_expression_spdx = detection.license_expression_spdx.unwrap_or_default();

    let matches: Vec<Match> = detection
        .matches
        .into_iter()
        .map(|m| Match {
            license_expression: m.license_expression,
            license_expression_spdx: m.license_expression_spdx.unwrap_or_default(),
            from_file: m.from_file,
            start_line: m.start_line,
            end_line: m.end_line,
            matcher: Some(m.matcher.to_string()),
            score: m.score as f64,
            matched_length: Some(m.matched_length),
            match_coverage: Some(m.match_coverage as f64),
            rule_relevance: Some(m.rule_relevance as usize),
            rule_identifier: Some(m.rule_identifier),
            rule_url: Some(m.rule_url),
            matched_text: if include_text { m.matched_text } else { None },
        })
        .collect();

    Some(LicenseDetection {
        license_expression,
        license_expression_spdx,
        matches,
        identifier: detection.identifier,
    })
}

fn should_skip_text_detection(path: &Path, buffer: &[u8]) -> bool {
    is_pem_certificate_file(path, buffer)
}

fn is_go_non_production_source(path: &Path) -> std::io::Result<bool> {
    if path.extension().and_then(|ext| ext.to_str()) != Some("go") {
        return Ok(false);
    }

    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with("_test.go"))
    {
        return Ok(true);
    }

    let content = fs::read_to_string(path)?;
    Ok(content.lines().take(10).any(|line| {
        let trimmed = line.trim();
        (trimmed.starts_with("//go:build") || trimmed.starts_with("// +build"))
            && trimmed.split_whitespace().any(|token| token == "test")
    }))
}

fn is_pem_certificate_file(_path: &Path, buffer: &[u8]) -> bool {
    let prefix_len = buffer.len().min(8192);
    let prefix = String::from_utf8_lossy(&buffer[..prefix_len]);
    let trimmed_lines: Vec<&str> = prefix
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(64)
        .collect();

    PEM_CERTIFICATE_HEADERS.iter().any(|(begin, end)| {
        trimmed_lines.iter().any(|line| line == begin)
            && trimmed_lines.iter().any(|line| line == end)
    })
}

fn process_directory(path: &Path, metadata: &fs::Metadata) -> FileInfo {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let base_name = name.clone(); // For directories, base_name is the same as name

    FileInfo {
        name,
        base_name,
        extension: "".to_string(),
        path: path.to_string_lossy().to_string(),
        file_type: FileType::Directory,
        mime_type: None,
        size: 0,
        date: get_creation_date(metadata),
        sha1: None,
        md5: None,
        sha256: None,
        programming_language: None,
        package_data: Vec::new(), // TODO: implement
        license_expression: None,
        copyrights: Vec::new(),         // TODO: implement
        holders: Vec::new(),            // TODO: implement
        authors: Vec::new(),            // TODO: implement
        emails: Vec::new(),             // TODO: implement
        license_detections: Vec::new(), // TODO: implement
        urls: Vec::new(),               // TODO: implement
        for_packages: Vec::new(),
        scan_errors: Vec::new(),
        is_source: None,
        source_count: None,
        is_legal: false,
        is_manifest: false,
        is_readme: false,
        is_top_level: false,
        is_key_file: false,
        tallies: None,
    }
}

#[cfg(test)]
mod tests {
    use super::is_go_non_production_source;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_is_go_non_production_source_for_test_filename() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().join("scanner_test.go");
        fs::write(&path, "package scanner\n").unwrap();

        assert!(is_go_non_production_source(&path).unwrap());
    }

    #[test]
    fn test_is_go_non_production_source_for_build_tag() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().join("scanner.go");
        fs::write(&path, "//go:build test\n\npackage scanner\n").unwrap();

        assert!(is_go_non_production_source(&path).unwrap());
    }

    #[test]
    fn test_is_go_non_production_source_for_regular_go_file() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().join("scanner.go");
        fs::write(&path, "package scanner\n").unwrap();

        assert!(!is_go_non_production_source(&path).unwrap());
    }
}
