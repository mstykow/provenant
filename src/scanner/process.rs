use content_inspector::{ContentType, inspect};

use crate::license_detection::LicenseDetectionEngine;
use crate::parsers::{try_parse_compiled_file, try_parse_file};
use crate::utils::hash::{calculate_md5, calculate_sha1, calculate_sha1_git, calculate_sha256};
use crate::utils::language::detect_language;
use crate::utils::text::{is_source, remove_verbatim_escape_sequences};
use anyhow::Error;
use mime_guess::from_path;
use rayon::prelude::*;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::cache::{CachedScanFindings, read_cached_findings, write_cached_findings};
use crate::copyright::{
    self, AuthorDetection, CopyrightDetection, CopyrightDetectionOptions, HolderDetection,
};
use crate::finder::{self, DetectionConfig};
use crate::license_detection::models::LicenseMatch as InternalLicenseMatch;
use crate::license_detection::query::Query;
use crate::models::{
    Author, Copyright, DatasourceId, FileInfo, FileInfoBuilder, FileType, Holder, LicenseDetection,
    Match, OutputEmail, OutputURL,
};
use crate::progress::ScanProgress;
use crate::scanner::collect::CollectedPaths;
use crate::scanner::{LicenseScanOptions, ProcessResult, TextDetectionOptions};
use crate::utils::file::{ExtractedTextKind, extract_text_for_detection, get_creation_date};
use crate::utils::generated::generated_code_hints_from_bytes;
use tempfile::TempDir;

const PEM_CERTIFICATE_HEADERS: &[(&str, &str)] = &[
    ("-----BEGIN CERTIFICATE-----", "-----END CERTIFICATE-----"),
    (
        "-----BEGIN TRUSTED CERTIFICATE-----",
        "-----END TRUSTED CERTIFICATE-----",
    ),
];

pub fn process_collected(
    collected: &CollectedPaths,
    progress: Arc<ScanProgress>,
    license_engine: Option<Arc<LicenseDetectionEngine>>,
    license_options: LicenseScanOptions,
    text_options: &TextDetectionOptions,
) -> ProcessResult {
    let mut all_files: Vec<FileInfo> = collected
        .files
        .par_iter()
        .map(|(path, metadata)| {
            let file_entry = process_file(
                path,
                metadata,
                license_engine.clone(),
                license_options,
                text_options,
            );
            progress.file_completed(path, metadata.len(), &file_entry.scan_errors);
            file_entry
        })
        .collect();

    for (path, metadata) in &collected.directories {
        all_files.push(process_directory(
            path,
            metadata,
            text_options.collect_info,
            license_engine.is_some(),
        ));
    }

    ProcessResult {
        files: all_files,
        excluded_count: collected.excluded_count,
    }
}

pub fn process_collected_with_memory_limit(
    collected: &CollectedPaths,
    progress: Arc<ScanProgress>,
    license_engine: Option<Arc<LicenseDetectionEngine>>,
    license_options: LicenseScanOptions,
    text_options: &TextDetectionOptions,
    max_in_memory: i64,
) -> ProcessResult {
    if max_in_memory == 0 {
        return process_collected(
            collected,
            progress,
            license_engine,
            license_options,
            text_options,
        );
    }

    let memory_limit = if max_in_memory < 0 {
        0
    } else {
        max_in_memory as usize
    };
    let chunk_size = if max_in_memory < 0 {
        256
    } else {
        memory_limit.max(1)
    };

    let mut retained_files = Vec::new();
    let mut spill_store = None;

    for chunk in collected.files.chunks(chunk_size) {
        let processed_chunk: Vec<FileInfo> = chunk
            .par_iter()
            .map(|(path, metadata)| {
                let file_entry = process_file(
                    path,
                    metadata,
                    license_engine.clone(),
                    license_options,
                    text_options,
                );
                progress.file_completed(path, metadata.len(), &file_entry.scan_errors);
                file_entry
            })
            .collect();

        retain_or_spill_chunk(
            processed_chunk,
            &mut retained_files,
            &mut spill_store,
            memory_limit,
        );
    }

    for (path, metadata) in &collected.directories {
        let entry = process_directory(
            path,
            metadata,
            text_options.collect_info,
            license_engine.is_some(),
        );
        retain_or_spill_chunk(
            vec![entry],
            &mut retained_files,
            &mut spill_store,
            memory_limit,
        );
    }

    if let Some(spill_store) = spill_store {
        retained_files.extend(spill_store.load_all());
    }

    ProcessResult {
        files: retained_files,
        excluded_count: collected.excluded_count,
    }
}

fn retain_or_spill_chunk(
    chunk: Vec<FileInfo>,
    retained_files: &mut Vec<FileInfo>,
    spill_store: &mut Option<FileInfoSpillStore>,
    memory_limit: usize,
) {
    if memory_limit == 0 {
        spill_store
            .get_or_insert_with(FileInfoSpillStore::new)
            .spill(chunk);
        return;
    }

    let remaining_capacity = memory_limit.saturating_sub(retained_files.len());
    if remaining_capacity >= chunk.len() && spill_store.is_none() {
        retained_files.extend(chunk);
        return;
    }

    let mut chunk_iter = chunk.into_iter();
    retained_files.extend(chunk_iter.by_ref().take(remaining_capacity));
    let overflow: Vec<FileInfo> = chunk_iter.collect();
    if !overflow.is_empty() {
        spill_store
            .get_or_insert_with(FileInfoSpillStore::new)
            .spill(overflow);
    }
}

struct FileInfoSpillStore {
    temp_dir: TempDir,
    batch_index: usize,
}

impl FileInfoSpillStore {
    fn new() -> Self {
        Self {
            temp_dir: TempDir::new().expect("create spill dir"),
            batch_index: 0,
        }
    }

    fn spill(&mut self, files: Vec<FileInfo>) {
        let path = self
            .temp_dir
            .path()
            .join(format!("batch-{:06}.json.zst", self.batch_index));
        self.batch_index += 1;

        let payload = serde_json::to_vec(&files).expect("encode spilled file batch");
        let file = File::create(path).expect("create spill batch file");
        let mut encoder = zstd::Encoder::new(file, 3).expect("create spill encoder");
        encoder
            .write_all(&payload)
            .expect("write spilled file batch");
        encoder.finish().expect("finish spill encoder");
    }

    fn load_all(self) -> Vec<FileInfo> {
        let mut paths: Vec<_> = fs::read_dir(self.temp_dir.path())
            .expect("read spill dir")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        paths.sort();

        let mut files = Vec::new();
        for path in paths {
            let file = File::open(path).expect("open spill batch");
            let mut decoder = zstd::Decoder::new(file).expect("create spill decoder");
            let mut payload = Vec::new();
            decoder.read_to_end(&mut payload).expect("read spill batch");
            let mut batch: Vec<FileInfo> =
                serde_json::from_slice(&payload).expect("decode spilled file batch");
            files.append(&mut batch);
        }
        files
    }
}

fn process_file(
    path: &Path,
    metadata: &fs::Metadata,
    license_engine: Option<Arc<LicenseDetectionEngine>>,
    license_options: LicenseScanOptions,
    text_options: &TextDetectionOptions,
) -> FileInfo {
    let mut scan_errors: Vec<String> = vec![];
    let mut file_info_builder = FileInfoBuilder::default();
    let license_enabled = license_engine.is_some();

    let started = Instant::now();

    let mut generated_flag = None;
    match extract_information_from_content(
        &mut file_info_builder,
        &mut scan_errors,
        path,
        license_engine,
        license_options,
        text_options,
    ) {
        Ok(is_generated) => generated_flag = is_generated,
        Err(e) => scan_errors.push(e.to_string()),
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
        .mime_type(text_options.collect_info.then(|| {
            from_path(path)
                .first_or_octet_stream()
                .essence_str()
                .to_string()
        }))
        .file_type_label(None)
        .size(metadata.len())
        .date(
            text_options
                .collect_info
                .then(|| get_creation_date(metadata))
                .flatten(),
        )
        .scan_errors(scan_errors)
        .build()
        .expect("FileInformationBuild not completely initialized");

    if text_options.collect_info {
        file_info.is_source = Some(is_source(path));
    }

    if file_info.programming_language.as_deref() == Some("Go")
        && is_go_non_production_source(path).unwrap_or(false)
    {
        file_info.is_source = Some(false);
    }

    if text_options.detect_generated {
        file_info.is_generated = Some(generated_flag.unwrap_or(false));
    }

    if file_info.percentage_of_license_text.is_none() && license_enabled {
        file_info.percentage_of_license_text = Some(0.0);
    }

    if let (Some(scan_results_dir), Some(sha256)) = (
        text_options.scan_cache_dir.as_deref(),
        file_info.sha256.as_deref(),
    ) && file_info.scan_errors.is_empty()
    {
        let findings = CachedScanFindings::from_file_info(&file_info);
        let options_fingerprint =
            scan_cache_fingerprint(text_options, license_options, license_enabled);
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
    scan_errors: &mut Vec<String>,
    path: &Path,
    license_engine: Option<Arc<LicenseDetectionEngine>>,
    license_options: LicenseScanOptions,
    text_options: &TextDetectionOptions,
) -> Result<Option<bool>, Error> {
    let started = Instant::now();
    let buffer = fs::read(path)?;
    let license_enabled = license_engine.is_some();

    if is_timeout_exceeded(started, text_options.timeout_seconds) {
        return Err(Error::msg(format!(
            "Timeout while reading file content (> {:.2}s)",
            text_options.timeout_seconds
        )));
    }

    let sha256 = calculate_sha256(&buffer);
    let is_generated = text_options
        .detect_generated
        .then(|| !generated_code_hints_from_bytes(&buffer).is_empty());
    let programming_language = detect_language(path, &buffer);

    if text_options.collect_info {
        let mime_type = from_path(path)
            .first_or_octet_stream()
            .essence_str()
            .to_string();
        let (file_type_label, is_binary, is_text, is_archive, is_media, is_script) =
            detect_info_flags(
                path,
                &buffer,
                &mime_type,
                Some(programming_language.as_str()),
            );

        file_info_builder
            .sha1(Some(calculate_sha1(&buffer)))
            .md5(Some(calculate_md5(&buffer)))
            .sha256(Some(sha256.clone()))
            .programming_language(Some(programming_language.clone()))
            .mime_type(Some(mime_type))
            .file_type_label(Some(file_type_label))
            .sha1_git(Some(calculate_sha1_git(&buffer)))
            .is_binary(Some(is_binary))
            .is_text(Some(is_text))
            .is_archive(Some(is_archive))
            .is_media(Some(is_media))
            .is_script(Some(is_script))
            .files_count(Some(0))
            .dirs_count(Some(0))
            .size_count(Some(0));
    }

    if should_skip_text_detection(path, &buffer) {
        return Ok(is_generated);
    }

    if let Some(scan_results_dir) = text_options.scan_cache_dir.as_deref() {
        let options_fingerprint =
            scan_cache_fingerprint(text_options, license_options, license_enabled);
        match read_cached_findings(scan_results_dir, &sha256, &options_fingerprint) {
            Ok(Some(findings)) => {
                file_info_builder
                    .package_data(findings.package_data)
                    .license_expression(findings.license_expression)
                    .license_detections(findings.license_detections)
                    .license_clues(findings.license_clues)
                    .percentage_of_license_text(findings.percentage_of_license_text)
                    .copyrights(findings.copyrights)
                    .holders(findings.holders)
                    .authors(findings.authors)
                    .emails(findings.emails)
                    .urls(findings.urls)
                    .programming_language(findings.programming_language);
                return Ok(is_generated);
            }
            Ok(None) => {}
            Err(err) => {
                scan_errors.push(format!("Failed to read scan cache for {:?}: {}", path, err));
            }
        }
    }

    // Package parsing and text-based detection (copyright, license) are independent.
    // Python ScanCode runs all enabled plugins on every file, so we do the same.
    if text_options.detect_packages {
        let parse_result = try_parse_file(path).or_else(|| {
            text_options
                .detect_packages_in_compiled
                .then(|| try_parse_compiled_file(path))
                .flatten()
        });

        if let Some(parse_result) = parse_result {
            let packages = parse_result
                .packages
                .into_iter()
                .filter(|package| {
                    let is_compiled_package = package
                        .datasource_id
                        .as_ref()
                        .is_some_and(is_compiled_datasource);
                    let is_system_package = package
                        .datasource_id
                        .as_ref()
                        .is_some_and(is_system_datasource);
                    if is_compiled_package {
                        text_options.detect_packages_in_compiled
                    } else if is_system_package {
                        text_options.detect_system_packages
                    } else {
                        text_options.detect_application_packages
                    }
                })
                .collect();
            file_info_builder.package_data(packages);
            scan_errors.extend(parse_result.scan_errors);
        }
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
        return Ok(is_generated);
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
        scan_errors,
        path,
        text_content_for_license_detection,
        license_engine,
        license_options,
        from_binary_strings,
    )?;

    Ok(is_generated)
}

fn is_timeout_exceeded(started: Instant, timeout_seconds: f64) -> bool {
    timeout_seconds.is_finite()
        && timeout_seconds > 0.0
        && started.elapsed().as_secs_f64() > timeout_seconds
}

fn scan_cache_fingerprint(
    text_options: &TextDetectionOptions,
    license_options: LicenseScanOptions,
    license_enabled: bool,
) -> String {
    format!(
        "info={};packages={};app_packages={};system_packages={};compiled_packages={};copyrights={};emails={};urls={};max_emails={};max_urls={};timeout={:.6};license_enabled={};license_text={};license_text_diagnostics={};license_diagnostics={};unknown_licenses={};license_score={}",
        text_options.collect_info,
        text_options.detect_packages,
        text_options.detect_application_packages,
        text_options.detect_system_packages,
        text_options.detect_packages_in_compiled,
        text_options.detect_copyrights,
        text_options.detect_emails,
        text_options.detect_urls,
        text_options.max_emails,
        text_options.max_urls,
        text_options.timeout_seconds,
        license_enabled,
        license_options.include_text,
        license_options.include_text_diagnostics,
        license_options.include_diagnostics,
        license_options.unknown_licenses,
        license_options.min_score,
    )
}

fn detect_info_flags(
    path: &Path,
    buffer: &[u8],
    mime_type: &str,
    programming_language: Option<&str>,
) -> (String, bool, bool, bool, bool, bool) {
    let is_binary = matches!(inspect(buffer), ContentType::BINARY);
    let is_text = !is_binary;
    let is_media = mime_type.starts_with("image/")
        || mime_type.starts_with("audio/")
        || mime_type.starts_with("video/");
    let lower_extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase());
    let is_archive = matches!(
        lower_extension.as_deref(),
        Some(
            "zip"
                | "jar"
                | "war"
                | "ear"
                | "tar"
                | "gz"
                | "tgz"
                | "bz2"
                | "xz"
                | "7z"
                | "rar"
                | "apk"
                | "deb"
                | "rpm"
                | "whl"
                | "crate"
        )
    ) || mime_type.contains("zip")
        || mime_type.contains("compressed")
        || mime_type.contains("tar")
        || mime_type.contains("x-rpm")
        || mime_type.contains("debian");
    let is_script = buffer.starts_with(b"#!")
        || matches!(
            lower_extension.as_deref(),
            Some("sh" | "bash" | "zsh" | "fish" | "py" | "rb" | "pl" | "php" | "js" | "ts")
        )
        || matches!(
            programming_language,
            Some("Python" | "Ruby" | "PHP" | "JavaScript" | "TypeScript" | "Shell")
        );

    let file_type_label = if is_archive {
        "archive".to_string()
    } else if is_media {
        "media".to_string()
    } else if is_binary {
        "binary".to_string()
    } else if is_script {
        "script".to_string()
    } else if programming_language.is_some() {
        "source".to_string()
    } else if mime_type.starts_with("text/") {
        "text".to_string()
    } else {
        mime_type.to_string()
    };

    (
        file_type_label,
        is_binary,
        is_text,
        is_archive,
        is_media,
        is_script,
    )
}

fn is_system_datasource(datasource_id: &DatasourceId) -> bool {
    matches!(
        datasource_id,
        DatasourceId::AlpineInstalledDb
            | DatasourceId::DebianDistrolessInstalledDb
            | DatasourceId::DebianInstalledFilesList
            | DatasourceId::DebianInstalledMd5Sums
            | DatasourceId::DebianInstalledStatusDb
            | DatasourceId::FreebsdCompactManifest
            | DatasourceId::RpmInstalledDatabaseBdb
            | DatasourceId::RpmInstalledDatabaseNdb
            | DatasourceId::RpmInstalledDatabaseSqlite
            | DatasourceId::RpmYumdb
    )
}

fn is_compiled_datasource(datasource_id: &DatasourceId) -> bool {
    matches!(
        datasource_id,
        DatasourceId::GoBinary | DatasourceId::RustBinary
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
            license_options.min_score as f32,
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
    detection: &crate::license_detection::LicenseDetection,
    license_options: LicenseScanOptions,
    text_content: &str,
    query: Option<&Query<'_>>,
) -> (Option<LicenseDetection>, Vec<Match>) {
    let matches: Vec<Match> = detection
        .matches
        .iter()
        .map(|m| convert_match_to_model(m, license_options, text_content, query))
        .collect();

    if let Some(license_expression) = detection.license_expression.clone() {
        (
            Some(LicenseDetection {
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
                m.start_line,
                m.end_line,
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
        score: m.score as f64,
        matched_length: Some(m.matched_length),
        match_coverage: Some(m.match_coverage as f64),
        rule_relevance: Some(m.rule_relevance as usize),
        rule_identifier: Some(m.rule_identifier.clone()),
        rule_url,
        matched_text,
        referenced_filenames: m.referenced_filenames.clone(),
        matched_text_diagnostics,
    }
}

fn compute_percentage_of_license_text(
    query: &Query<'_>,
    detections: &[crate::license_detection::LicenseDetection],
) -> f64 {
    let matched_positions: std::collections::HashSet<usize> = detections
        .iter()
        .flat_map(|detection| detection.matches.iter())
        .flat_map(InternalLicenseMatch::qspan)
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
    let matched_positions: std::collections::HashSet<usize> =
        license_match.qspan().into_iter().collect();
    let Some(start_pos) = matched_positions.iter().min().copied() else {
        return crate::license_detection::query::matched_text_from_text(
            &query.text,
            license_match.start_line,
            license_match.end_line,
        );
    };
    let Some(end_pos) = matched_positions.iter().max().copied() else {
        return crate::license_detection::query::matched_text_from_text(
            &query.text,
            license_match.start_line,
            license_match.end_line,
        );
    };

    crate::license_detection::query::matched_text_diagnostics_from_text(
        &query.text,
        query,
        &matched_positions,
        start_pos,
        end_pos,
        license_match.start_line,
        license_match.end_line,
    )
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

fn process_directory(
    path: &Path,
    _metadata: &fs::Metadata,
    collect_info: bool,
    license_enabled: bool,
) -> FileInfo {
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
        file_type_label: None,
        size: 0,
        date: None,
        sha1: None,
        md5: None,
        sha256: None,
        sha1_git: None,
        programming_language: None,
        package_data: Vec::new(),
        license_expression: None,
        license_detections: Vec::new(),
        license_clues: Vec::new(),
        percentage_of_license_text: license_enabled.then_some(0.0),
        copyrights: Vec::new(),
        holders: Vec::new(),
        authors: Vec::new(),
        emails: Vec::new(),
        urls: Vec::new(),
        for_packages: Vec::new(),
        scan_errors: Vec::new(),
        license_policy: None,
        is_binary: collect_info.then_some(false),
        is_text: collect_info.then_some(false),
        is_archive: collect_info.then_some(false),
        is_media: collect_info.then_some(false),
        is_source: collect_info.then_some(false),
        is_script: collect_info.then_some(false),
        files_count: collect_info.then_some(0),
        dirs_count: collect_info.then_some(0),
        size_count: collect_info.then_some(0),
        source_count: None,
        is_legal: false,
        is_manifest: false,
        is_readme: false,
        is_top_level: false,
        is_key_file: false,
        is_community: false,
        is_generated: None,
        facets: vec![],
        tallies: None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        compute_percentage_of_license_text, convert_detection_to_model,
        is_go_non_production_source, scan_cache_fingerprint,
    };
    use crate::license_detection::LicenseDetection as InternalLicenseDetection;
    use crate::license_detection::index::LicenseIndex;
    use crate::license_detection::index::dictionary::TokenDictionary;
    use crate::license_detection::models::{LicenseMatch, MatcherKind, RuleKind};
    use crate::license_detection::query::Query;
    use crate::scanner::LicenseScanOptions;
    use std::fs;
    use tempfile::tempdir;

    fn make_internal_match(rule_url: &str) -> LicenseMatch {
        LicenseMatch {
            rid: 0,
            license_expression: "mit".to_string(),
            license_expression_spdx: Some("MIT".to_string()),
            from_file: None,
            start_line: 1,
            end_line: 1,
            start_token: 0,
            end_token: 1,
            matcher: MatcherKind::Hash,
            score: 1.0,
            matched_length: 3,
            rule_length: 3,
            match_coverage: 100.0,
            rule_relevance: 100,
            rule_identifier: "mit.LICENSE".to_string(),
            rule_url: rule_url.to_string(),
            matched_text: Some("MIT".to_string()),
            referenced_filenames: None,
            rule_kind: RuleKind::Text,
            is_from_license: true,
            matched_token_positions: None,
            hilen: 3,
            rule_start_token: 0,
            qspan_positions: None,
            ispan_positions: None,
            hispan_positions: None,
            candidate_resemblance: 0.0,
            candidate_containment: 0.0,
        }
    }

    fn make_detection(rule_url: &str) -> InternalLicenseDetection {
        InternalLicenseDetection {
            license_expression: Some("mit".to_string()),
            license_expression_spdx: Some("MIT".to_string()),
            matches: vec![make_internal_match(rule_url)],
            detection_log: vec![],
            identifier: Some("mit-test".to_string()),
            file_regions: Vec::new(),
        }
    }

    fn create_test_index(entries: &[(&str, u16)], len_legalese: usize) -> LicenseIndex {
        let dictionary = TokenDictionary::new_with_legalese(entries);
        let mut index = LicenseIndex::new(dictionary);
        index.len_legalese = len_legalese;
        index
    }

    #[test]
    fn test_convert_detection_to_model_preserves_rule_url() {
        let detection = make_detection(
            "https://github.com/nexB/scancode-toolkit/tree/develop/src/licensedcode/data/licenses/mit.LICENSE",
        );

        let (converted, clues) =
            convert_detection_to_model(&detection, LicenseScanOptions::default(), "", None);
        let converted = converted.expect("detection should convert");

        assert_eq!(
            converted.matches[0].rule_url.as_deref(),
            Some(
                "https://github.com/nexB/scancode-toolkit/tree/develop/src/licensedcode/data/licenses/mit.LICENSE"
            )
        );
        assert!(clues.is_empty());
    }

    #[test]
    fn test_convert_detection_to_model_emits_null_for_empty_rule_url() {
        let detection = make_detection("");

        let (converted, clues) =
            convert_detection_to_model(&detection, LicenseScanOptions::default(), "", None);
        let converted = converted.expect("detection should convert");

        assert_eq!(converted.matches[0].rule_url, None);
        assert!(clues.is_empty());
    }

    #[test]
    fn test_convert_detection_to_model_routes_expressionless_detection_to_license_clues() {
        let mut detection = make_detection(
            "https://github.com/nexB/scancode-toolkit/tree/develop/src/licensedcode/data/rules/license-clue_1.RULE",
        );
        detection.license_expression = None;
        detection.license_expression_spdx = None;
        detection.identifier = None;
        detection.matches[0].license_expression = "unknown-license-reference".to_string();
        detection.matches[0].license_expression_spdx =
            Some("LicenseRef-scancode-unknown-license-reference".to_string());
        detection.matches[0].rule_identifier = "license-clue_1.RULE".to_string();
        detection.matches[0].rule_kind = RuleKind::Clue;

        let (converted, clues) = convert_detection_to_model(
            &detection,
            LicenseScanOptions {
                include_text: true,
                min_score: 0,
                ..LicenseScanOptions::default()
            },
            "clue text",
            None,
        );

        assert!(converted.is_none());
        assert_eq!(clues.len(), 1);
        assert_eq!(clues[0].license_expression, "unknown-license-reference");
        assert_eq!(
            clues[0].license_expression_spdx,
            "LicenseRef-scancode-unknown-license-reference"
        );
        assert_eq!(
            clues[0].rule_identifier.as_deref(),
            Some("license-clue_1.RULE")
        );
        assert_eq!(clues[0].matched_text.as_deref(), Some("MIT"));
        assert_eq!(clues[0].matched_text_diagnostics, None);
    }

    #[test]
    fn test_convert_detection_to_model_includes_diagnostics_when_enabled() {
        let text = concat!(
            "Reproduction and distribution of this file, with or without modification, are\n",
            "permitted in any medium without royalties provided the copyright notice\n",
            "and this notice are preserved. This file is offered as-is, without any warranties.\n",
        );
        let index = create_test_index(
            &[
                ("reproduction", 0),
                ("distribution", 1),
                ("file", 2),
                ("without", 3),
                ("modification", 4),
                ("permitted", 5),
                ("medium", 6),
                ("royalties", 7),
                ("provided", 8),
                ("copyright", 9),
                ("notice", 10),
                ("preserved", 11),
                ("offered", 12),
                ("warranties", 13),
            ],
            14,
        );
        let query = Query::from_extracted_text(text, &index, false).expect("query should build");
        let mut detection = make_detection(
            "https://github.com/nexB/scancode-toolkit/tree/develop/src/licensedcode/data/licenses/fsf-ap.LICENSE",
        );
        detection.detection_log = vec!["imperfect-match-coverage".to_string()];
        detection.matches[0].license_expression = "fsf-ap".to_string();
        detection.matches[0].license_expression_spdx = Some("FSFAP".to_string());
        detection.matches[0].rule_identifier = "fsf-ap.LICENSE".to_string();
        detection.matches[0].matched_text = None;
        detection.matches[0].start_line = 1;
        detection.matches[0].end_line = 3;
        detection.matches[0].start_token = 0;
        detection.matches[0].end_token = query.tokens.len();
        detection.matches[0].qspan_positions = Some(
            query
                .tokens
                .iter()
                .enumerate()
                .filter_map(|(idx, _)| (idx != 9).then_some(idx))
                .collect(),
        );
        detection.identifier = Some("fsf_ap-test".to_string());

        let (converted, clues) = convert_detection_to_model(
            &detection,
            LicenseScanOptions {
                include_text: true,
                include_text_diagnostics: true,
                include_diagnostics: true,
                unknown_licenses: false,
                min_score: 0,
            },
            text,
            Some(&query),
        );
        let converted = converted.expect("detection should convert");

        assert!(clues.is_empty());
        assert_eq!(converted.detection_log, vec!["imperfect-match-coverage"]);
        assert_eq!(
            converted.matches[0].matched_text.as_deref(),
            Some(text.trim_end())
        );
        let diagnostics = converted.matches[0]
            .matched_text_diagnostics
            .as_deref()
            .expect("diagnostics should be present");
        assert!(diagnostics.contains('['));
        assert!(diagnostics.contains(']'));
        assert_ne!(diagnostics, text.trim_end());
    }

    #[test]
    fn test_compute_percentage_of_license_text_counts_unknown_tokens() {
        let index = create_test_index(&[("alpha", 0), ("mit", 1)], 2);
        let text = "alpha MIT omega";
        let query = Query::from_extracted_text(text, &index, false).expect("query should build");
        let mut detection = make_detection("");
        detection.matches[0].qspan_positions = Some(vec![1]);
        detection.matches[0].start_token = 1;
        detection.matches[0].end_token = 2;

        let percentage = compute_percentage_of_license_text(&query, &[detection]);

        assert_eq!(percentage, 33.33);
    }

    #[test]
    fn test_scan_cache_fingerprint_changes_with_license_score() {
        let text_options = crate::scanner::TextDetectionOptions::default();
        let default_fingerprint = scan_cache_fingerprint(
            &text_options,
            LicenseScanOptions {
                min_score: 0,
                ..LicenseScanOptions::default()
            },
            true,
        );
        let filtered_fingerprint = scan_cache_fingerprint(
            &text_options,
            LicenseScanOptions {
                min_score: 70,
                ..LicenseScanOptions::default()
            },
            true,
        );

        assert_ne!(default_fingerprint, filtered_fingerprint);
    }

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
