use crate::license_detection::LicenseDetectionEngine;
use crate::parsers::compiled_binary::{
    is_supported_compiled_binary_format, try_parse_compiled_bytes,
};
use crate::parsers::try_parse_file;
use crate::parsers::windows_executable::try_parse_windows_executable_bytes;
use crate::utils::hash::{calculate_md5, calculate_sha1, calculate_sha1_git, calculate_sha256};
use crate::utils::text::{
    remove_verbatim_escape_sequences, should_remove_verbatim_escape_sequences,
};
use anyhow::Error;
use rayon::prelude::*;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::copyright::{
    self, AuthorDetection, CopyrightDetection, CopyrightDetectionOptions, HolderDetection,
};
use crate::finder::{self, DetectionConfig};
use crate::license_detection::PositionSet;
use crate::license_detection::models::LicenseMatch as InternalLicenseMatch;
use crate::license_detection::query::Query;
use crate::models::{
    Author, Copyright, DatasourceId, FileInfo, FileInfoBuilder, FileType, Holder, LicenseDetection,
    Match, OutputEmail, OutputURL, Sha256Digest,
};
use crate::parsers::utils::split_name_email;
use crate::progress::ScanProgress;
use crate::scanner::collect::CollectedPaths;
use crate::scanner::{LicenseScanOptions, ProcessResult, TextDetectionOptions};
use crate::utils::file::{
    ExtractedTextKind, augment_license_detection_text, classify_file_info,
    extract_text_for_detection_with_diagnostics, get_creation_date,
};
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
                progress.as_ref(),
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

pub fn process_collected_sequential(
    collected: &CollectedPaths,
    progress: Arc<ScanProgress>,
    license_engine: Option<Arc<LicenseDetectionEngine>>,
    license_options: LicenseScanOptions,
    text_options: &TextDetectionOptions,
) -> ProcessResult {
    let mut all_files: Vec<FileInfo> =
        Vec::with_capacity(collected.files.len() + collected.directories.len());

    for (path, metadata) in &collected.files {
        let file_entry = process_file(
            path,
            metadata,
            progress.as_ref(),
            license_engine.clone(),
            license_options,
            text_options,
        );
        progress.file_completed(path, metadata.len(), &file_entry.scan_errors);
        all_files.push(file_entry);
    }

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
                    progress.as_ref(),
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

pub fn process_collected_with_memory_limit_sequential(
    collected: &CollectedPaths,
    progress: Arc<ScanProgress>,
    license_engine: Option<Arc<LicenseDetectionEngine>>,
    license_options: LicenseScanOptions,
    text_options: &TextDetectionOptions,
    max_in_memory: i64,
) -> ProcessResult {
    if max_in_memory == 0 {
        return process_collected_sequential(
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
        let mut processed_chunk: Vec<FileInfo> = Vec::with_capacity(chunk.len());
        for (path, metadata) in chunk {
            let file_entry = process_file(
                path,
                metadata,
                progress.as_ref(),
                license_engine.clone(),
                license_options,
                text_options,
            );
            progress.file_completed(path, metadata.len(), &file_entry.scan_errors);
            processed_chunk.push(file_entry);
        }

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
    progress: &ScanProgress,
    license_engine: Option<Arc<LicenseDetectionEngine>>,
    license_options: LicenseScanOptions,
    text_options: &TextDetectionOptions,
) -> FileInfo {
    let mut scan_errors: Vec<String> = vec![];
    let mut file_info_builder = FileInfoBuilder::default();
    let license_enabled = license_engine.is_some();

    let started = Instant::now();

    let mut generated_flag = None;
    let mut is_source_file = false;
    match extract_information_from_content(
        &mut file_info_builder,
        &mut scan_errors,
        path,
        progress,
        license_engine,
        license_options,
        text_options,
    ) {
        Ok((is_generated, sha256, is_source)) => {
            generated_flag = is_generated;
            is_source_file = is_source;
            let _ = sha256;
        }
        Err(e) => scan_errors.push(e.to_string()),
    };

    maybe_record_processing_timeout(&mut scan_errors, started, text_options.timeout_seconds);

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
        file_info.is_source = Some(is_source_file);
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

    file_info
}

fn extract_information_from_content(
    file_info_builder: &mut FileInfoBuilder,
    scan_errors: &mut Vec<String>,
    path: &Path,
    progress: &ScanProgress,
    license_engine: Option<Arc<LicenseDetectionEngine>>,
    license_options: LicenseScanOptions,
    text_options: &TextDetectionOptions,
) -> Result<(Option<bool>, Sha256Digest, bool), Error> {
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
    let classification = classify_file_info(path, &buffer);

    if text_options.collect_info {
        file_info_builder
            .sha1(Some(calculate_sha1(&buffer)))
            .md5(Some(calculate_md5(&buffer)))
            .sha256(Some(sha256))
            .programming_language(classification.programming_language.clone())
            .mime_type(Some(classification.mime_type.clone()))
            .file_type_label(Some(classification.file_type.clone()))
            .sha1_git(Some(calculate_sha1_git(&buffer)))
            .is_binary(Some(classification.is_binary))
            .is_text(Some(classification.is_text))
            .is_archive(Some(classification.is_archive))
            .is_media(Some(classification.is_media))
            .is_source(Some(classification.is_source))
            .is_script(Some(classification.is_script))
            .files_count(Some(0))
            .dirs_count(Some(0))
            .size_count(Some(0));
    }

    if should_skip_text_detection(path, &buffer) {
        return Ok((is_generated, sha256, classification.is_source));
    }

    // Package parsing and text-based detection (copyright, license) are independent.
    // Python ScanCode runs all enabled plugins on every file, so we do the same.
    if text_options.detect_packages {
        let started = Instant::now();
        let parse_result = try_parse_file(path)
            .or_else(|| {
                text_options
                    .detect_application_packages
                    .then(|| try_parse_windows_executable_bytes(path, &buffer))
                    .flatten()
            })
            .or_else(|| {
                text_options
                    .detect_packages_in_compiled
                    .then(|| {
                        (classification.is_binary && is_supported_compiled_binary_format(&buffer))
                            .then(|| try_parse_compiled_bytes(&buffer))
                            .flatten()
                    })
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
        progress.record_detail_timing("scan:packages", started.elapsed().as_secs_f64());
    }

    if is_timeout_exceeded(started, text_options.timeout_seconds) {
        return Err(Error::msg(format!(
            "Timeout while extracting package/text metadata (> {:.2}s)",
            text_options.timeout_seconds
        )));
    }

    let (text_content, text_kind, text_scan_error) =
        extract_text_for_detection_with_diagnostics(path, &buffer);
    if let Some(text_scan_error) = text_scan_error {
        scan_errors.push(text_scan_error);
    }
    let from_binary_strings = matches!(text_kind, ExtractedTextKind::BinaryStrings);

    if is_timeout_exceeded(started, text_options.timeout_seconds) {
        return Err(Error::msg(format!(
            "Timeout while extracting text content (> {:.2}s)",
            text_options.timeout_seconds
        )));
    }

    if text_content.is_empty() {
        return Ok((is_generated, sha256, classification.is_source));
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
    extract_email_url_information(
        file_info_builder,
        &text_content,
        text_options,
        from_binary_strings,
    );

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
    } else if should_remove_verbatim_escape_sequences(path, classification.is_source) {
        remove_verbatim_escape_sequences(&text_content)
    } else {
        text_content
    };
    let text_content_for_license_detection =
        augment_license_detection_text(path, &text_content_for_license_detection);
    let text_content_for_license_detection = text_content_for_license_detection.into_owned();

    if license_enabled {
        let started = Instant::now();
        extract_license_information(
            file_info_builder,
            scan_errors,
            path,
            text_content_for_license_detection.clone(),
            license_engine,
            license_options,
            from_binary_strings,
        )?;
        progress.record_detail_timing("scan:licenses", started.elapsed().as_secs_f64());
    } else {
        extract_license_information(
            file_info_builder,
            scan_errors,
            path,
            text_content_for_license_detection,
            license_engine,
            license_options,
            from_binary_strings,
        )?;
    }

    Ok((is_generated, sha256, classification.is_source))
}

fn is_timeout_exceeded(started: Instant, timeout_seconds: f64) -> bool {
    timeout_seconds.is_finite()
        && timeout_seconds > 0.0
        && started.elapsed().as_secs_f64() > timeout_seconds
}

fn maybe_record_processing_timeout(
    scan_errors: &mut Vec<String>,
    started: Instant,
    timeout_seconds: f64,
) {
    if is_timeout_exceeded(started, timeout_seconds)
        && !scan_errors.iter().any(|error| is_timeout_scan_error(error))
    {
        scan_errors.push(format!(
            "Processing interrupted due to timeout after {:.2} seconds",
            timeout_seconds
        ));
    }
}

fn is_timeout_scan_error(error: &str) -> bool {
    error.contains("Timeout while ")
        || error.contains("Timeout before ")
        || error.contains("Processing interrupted due to timeout")
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
        prune_binary_string_detections(text_content, copyrights, holders, authors)
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
    text_content: &str,
    copyrights: Vec<CopyrightDetection>,
    holders: Vec<HolderDetection>,
    authors: Vec<AuthorDetection>,
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

    let kept_authors = authors
        .into_iter()
        .filter(|author| is_binary_string_author_candidate(&author.author))
        .chain(extract_binary_string_author_supplements(text_content))
        .filter({
            let mut seen = HashSet::new();
            move |author| seen.insert(author.author.clone())
        })
        .collect();

    (kept_copyrights, kept_holders, kept_authors)
}

fn ranges_overlap(a_start: usize, a_end: usize, b_start: usize, b_end: usize) -> bool {
    a_start <= b_end && b_start <= a_end
}

fn is_binary_string_copyright_candidate(text: &str) -> bool {
    if contains_year(text) {
        return true;
    }

    let trimmed = text.trim();
    let lower = trimmed.to_ascii_lowercase();
    let tail = if let Some(tail) = lower.strip_prefix("copyright") {
        tail.trim()
    } else {
        lower.trim()
    };
    let original_tail = if lower.starts_with("copyright") {
        trimmed["copyright".len()..].trim()
    } else {
        trimmed
    };

    if tail.is_empty() || !has_sufficient_alphabetic_content(tail) || has_excessive_at_noise(tail) {
        return false;
    }

    let alpha_tokens: Vec<&str> = tail
        .split_whitespace()
        .filter(|token| token.chars().any(|c| c.is_alphabetic()))
        .collect();

    if alpha_tokens.len() <= 1 {
        return has_explicit_copyright_marker(text)
            && alpha_tokens.iter().any(|token| {
                is_company_like_suffix(token.trim_matches(|c: char| !c.is_alphanumeric()))
            });
    }

    if !has_explicit_copyright_marker(text) {
        return false;
    }

    has_binary_name_like_shape(original_tail)
}

fn extract_binary_string_author_supplements(text_content: &str) -> Vec<AuthorDetection> {
    let mut authors = Vec::new();

    for (line_index, line) in text_content.lines().enumerate() {
        if let Some(author) = extract_named_author_from_binary_line(line) {
            authors.push(AuthorDetection {
                author,
                start_line: line_index + 1,
                end_line: line_index + 1,
            });
        }
    }

    authors
}

fn extract_named_author_from_binary_line(line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    let emails = finder::find_emails(
        line,
        &DetectionConfig {
            max_emails: 4,
            max_urls: 0,
            unique: false,
        },
    );
    let email = emails.first()?.email.as_str();
    if !is_binary_string_email_candidate(email) {
        return None;
    }

    let lower_line = line.to_ascii_lowercase();
    let email_start = lower_line.find(email)?;
    let raw_prefix = &line[..email_start];
    let has_author_marker = contains_binary_author_marker(raw_prefix);
    let prefix = take_suffix_after_last_author_marker(raw_prefix)?;
    let prefix = prefix
        .trim_start_matches(['*', '-', ':', ';', ',', '.', ' '])
        .trim_end_matches(['<', '(', '[', ' ', ':', '-'])
        .trim();

    let (name, _) = split_name_email(prefix);
    let name = name.or_else(|| {
        let trimmed = prefix.trim_matches(|c: char| c == '<' || c == '(' || c == '[' || c == ' ');
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    });

    let Some(name) = name.map(|name| name.trim().to_string()) else {
        if has_author_marker {
            return Some(email.to_string());
        }
        return None;
    };

    if name.is_empty() && has_author_marker {
        return Some(email.to_string());
    }

    if !has_binary_name_like_shape(&name) {
        return None;
    }

    if line.contains(&format!("<{email}>")) {
        Some(format!("{name} <{email}>"))
    } else if line.contains(&format!("({email})")) {
        Some(format!("{name} ({email})"))
    } else {
        Some(format!("{name} {email}"))
    }
}

fn take_suffix_after_last_ascii_marker<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    let lower = text.to_ascii_lowercase();
    let idx = lower.rfind(marker)?;
    Some(text[idx + marker.len()..].trim())
}

fn take_suffix_after_last_author_marker(text: &str) -> Option<&str> {
    const MARKERS: &[&str] = &[
        " patch author: ",
        " patch author ",
        " written by ",
        " contributed by ",
        " original work done by ",
        " work done by ",
        " thanks to ",
        " review by ",
        " by ",
        " from ",
    ];

    MARKERS
        .iter()
        .filter_map(|marker| take_suffix_after_last_ascii_marker(text, marker))
        .next()
}

fn contains_binary_author_marker(text: &str) -> bool {
    take_suffix_after_last_author_marker(text).is_some()
}

fn has_binary_name_like_shape(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.contains(" - ") || trimmed.chars().any(|c| c.is_ascii_digit())
    {
        return false;
    }

    let tokens: Vec<&str> = trimmed
        .split(|c: char| !c.is_ascii_alphabetic() && c != '.' && c != '\'')
        .filter(|segment| segment.chars().any(|c| c.is_ascii_alphabetic()))
        .collect();
    if tokens.is_empty() {
        return false;
    }

    let uppercase_like = tokens
        .iter()
        .filter(|token| {
            let token = token.trim_matches('.');
            token
                .chars()
                .find(|c| c.is_ascii_alphabetic())
                .is_some_and(|c| c.is_ascii_uppercase())
        })
        .count();

    uppercase_like >= 2 && uppercase_like * 2 >= tokens.len()
        || tokens
            .iter()
            .any(|token| is_company_like_suffix(token.trim_matches(|c: char| !c.is_alphanumeric())))
}

fn has_sufficient_alphabetic_content(text: &str) -> bool {
    let alnum_count = text.chars().filter(|c| c.is_ascii_alphanumeric()).count();
    if alnum_count == 0 {
        return false;
    }

    let alpha_count = text.chars().filter(|c| c.is_ascii_alphabetic()).count();
    alpha_count * 2 >= alnum_count
}

fn has_excessive_at_noise(text: &str) -> bool {
    text.chars().filter(|c| *c == '@').count() >= 3
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
    from_binary_strings: bool,
) {
    if !text_options.detect_emails && !text_options.detect_urls {
        return;
    }

    if text_options.detect_emails {
        let config = DetectionConfig {
            max_emails: text_options.max_emails,
            max_urls: text_options.max_urls,
            unique: from_binary_strings,
        };
        let emails = finder::find_emails(text_content, &config)
            .into_iter()
            .filter(|d| !from_binary_strings || is_binary_string_email_candidate(&d.email))
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
            .filter(|d| !from_binary_strings || is_binary_string_url_candidate(&d.url))
            .map(|d| OutputURL {
                url: d.url,
                start_line: d.start_line,
                end_line: d.end_line,
            })
            .collect::<Vec<_>>();
        file_info_builder.urls(urls);
    }
}

fn is_binary_string_email_candidate(email: &str) -> bool {
    let Some((local, domain)) = email.rsplit_once('@') else {
        return false;
    };

    if !has_strong_binary_local_part(local) {
        return false;
    }

    has_strong_binary_host_shape(domain)
}

fn is_binary_string_url_candidate(url: &str) -> bool {
    let parsed = url::Url::parse(url).ok();
    let Some(parsed) = parsed else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };

    has_strong_binary_host_shape(host) && has_meaningful_binary_url_context(&parsed)
}

fn is_binary_string_author_candidate(author: &str) -> bool {
    let trimmed = author.trim();
    if trimmed.is_empty()
        || !has_sufficient_alphabetic_content(trimmed)
        || has_excessive_at_noise(trimmed)
    {
        return false;
    }

    if trimmed.contains('@') {
        let emails = finder::find_emails(
            trimmed,
            &DetectionConfig {
                max_emails: 4,
                max_urls: 0,
                unique: true,
            },
        );
        if emails.len() > 1 {
            return false;
        }

        if let Some(extracted) = extract_named_author_from_binary_line(trimmed) {
            return !extracted.is_empty();
        }

        let Some(email) = emails.first().map(|d| d.email.as_str()) else {
            return false;
        };
        if !is_binary_string_email_candidate(email) {
            return false;
        }

        let (name, _) = split_name_email(trimmed);
        return name.as_deref().is_some_and(has_binary_name_like_shape);
    }

    has_binary_name_like_shape(trimmed)
}

fn has_meaningful_binary_url_context(parsed: &url::Url) -> bool {
    if parsed.path() != "/"
        && parsed
            .path()
            .split('/')
            .any(|segment| segment.chars().any(|c| c.is_ascii_alphabetic()) && segment.len() >= 2)
    {
        return true;
    }

    if parsed.query().is_some() || parsed.fragment().is_some() {
        return true;
    }

    let Some(host) = parsed.host_str() else {
        return false;
    };

    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() > 2 {
        return labels[..labels.len() - 1].iter().any(|label| {
            label.len() >= 3 && label.chars().filter(|c| c.is_ascii_alphabetic()).count() >= 3
        });
    }

    if matches!(labels.first(), Some(&"www")) {
        return true;
    }

    if labels.len() == 2 {
        let domain = labels[0];
        let tld = labels[1];
        if domain.len() >= 8 && matches!(tld, "org" | "edu" | "gov" | "mil" | "io" | "dev") {
            return true;
        }
    }

    labels
        .iter()
        .take(labels.len().saturating_sub(1))
        .any(|label| {
            label.contains('-') && label.chars().filter(|c| c.is_ascii_alphabetic()).count() >= 4
        })
}

fn has_strong_binary_local_part(local: &str) -> bool {
    local
        .split(|c: char| !c.is_ascii_alphabetic())
        .any(|segment| segment.len() >= 3)
}

fn has_strong_binary_host_shape(host: &str) -> bool {
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2 {
        return false;
    }

    let relevant = if matches!(labels.first(), Some(&"www" | &"ftp")) {
        &labels[1..]
    } else {
        &labels[..]
    };

    if relevant.len() < 2 {
        return false;
    }

    relevant[..relevant.len() - 1].iter().any(|label| {
        label.len() >= 3 && label.chars().filter(|c| c.is_ascii_alphabetic()).count() >= 3
    })
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
    let output_metric = |value: f32| ((value as f64) * 100.0).round() / 100.0;
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
        score: output_metric(m.score),
        matched_length: Some(m.matched_length),
        match_coverage: Some(output_metric(m.coverage())),
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
            license_match.start_line,
            license_match.end_line,
        );
    };
    let Some(end_pos) = matched_positions.iter().max() else {
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

    let Some(first_line) = trimmed_lines.first().copied() else {
        return false;
    };

    PEM_CERTIFICATE_HEADERS
        .iter()
        .any(|(begin, end)| first_line == *begin && trimmed_lines.iter().any(|line| line == end))
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
        extract_email_url_information, extract_named_author_from_binary_line,
        is_binary_string_author_candidate, is_binary_string_copyright_candidate,
        is_binary_string_email_candidate, is_binary_string_url_candidate,
        is_go_non_production_source, process_file,
    };
    use crate::license_detection::LicenseDetection as InternalLicenseDetection;
    use crate::license_detection::index::LicenseIndex;
    use crate::license_detection::index::dictionary::TokenDictionary;
    use crate::license_detection::models::position_span::PositionSpan;
    use crate::license_detection::models::{LicenseMatch, MatchCoordinates, MatcherKind, RuleKind};
    use crate::license_detection::query::Query;
    use crate::models::{FileInfoBuilder, FileType};
    use crate::progress::{ProgressMode, ScanProgress};
    use crate::scanner::scan_options_fingerprint;
    use crate::scanner::{LicenseScanOptions, TextDetectionOptions};
    use std::fs;
    use std::time::{Duration, Instant};
    use tempfile::tempdir;

    use super::maybe_record_processing_timeout;

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
            rule_start_token: 0,
            coordinates: MatchCoordinates::query_region(PositionSpan::empty()),
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
    fn test_convert_detection_to_model_rounds_match_coverage() {
        let mut detection = make_detection("");
        detection.matches[0].score = 81.82;
        detection.matches[0].match_coverage = 33.334;

        let (converted, clues) =
            convert_detection_to_model(&detection, LicenseScanOptions::default(), "", None);
        let converted = converted.expect("detection should convert");

        assert_eq!(converted.matches[0].score, 81.82);
        assert_eq!(converted.matches[0].match_coverage, Some(33.33));
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
        detection.matches[0].coordinates =
            MatchCoordinates::query_region(PositionSpan::from_positions(
                query
                    .tokens
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, _)| (idx != 9).then_some(idx))
                    .collect::<Vec<_>>(),
            ));
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
    fn test_extract_email_url_information_skips_binary_string_text() {
        let mut builder = FileInfoBuilder::default();
        let options = TextDetectionOptions {
            collect_info: false,
            detect_packages: false,
            detect_application_packages: false,
            detect_system_packages: false,
            detect_packages_in_compiled: false,
            detect_copyrights: false,
            detect_generated: false,
            detect_emails: true,
            detect_urls: true,
            max_emails: 50,
            max_urls: 50,
            timeout_seconds: 120.0,
        };

        extract_email_url_information(
            &mut builder,
            "contact 6h@fo.lwft and visit http://gmail.com/",
            &options,
            true,
        );

        let file = builder
            .name("binary.bin".to_string())
            .base_name("binary".to_string())
            .extension(".bin".to_string())
            .path("binary.bin".to_string())
            .file_type(FileType::File)
            .size(1)
            .build()
            .expect("builder should produce file info");

        assert!(file.emails.is_empty(), "emails: {:?}", file.emails);
        assert!(file.urls.is_empty(), "urls: {:?}", file.urls);
    }

    #[test]
    fn test_extract_email_url_information_keeps_good_binary_contacts() {
        let mut builder = FileInfoBuilder::default();
        let options = TextDetectionOptions {
            collect_info: false,
            detect_packages: false,
            detect_application_packages: false,
            detect_system_packages: false,
            detect_packages_in_compiled: false,
            detect_copyrights: false,
            detect_generated: false,
            detect_emails: true,
            detect_urls: true,
            max_emails: 50,
            max_urls: 50,
            timeout_seconds: 120.0,
        };

        extract_email_url_information(
            &mut builder,
            "report bugs to bug-coreutils@gnu.org and see https://www.gnu.org/software/coreutils/",
            &options,
            true,
        );

        let file = builder
            .name("binary.bin".to_string())
            .base_name("binary".to_string())
            .extension(".bin".to_string())
            .path("binary.bin".to_string())
            .file_type(FileType::File)
            .size(1)
            .build()
            .expect("builder should produce file info");

        assert_eq!(file.emails.len(), 1, "emails: {:?}", file.emails);
        assert_eq!(file.emails[0].email, "bug-coreutils@gnu.org");
        assert_eq!(file.urls.len(), 1, "urls: {:?}", file.urls);
        assert_eq!(file.urls[0].url, "https://www.gnu.org/software/coreutils/");
    }

    #[test]
    fn test_extract_email_url_information_deduplicates_binary_emails_before_cap() {
        let mut builder = FileInfoBuilder::default();
        let options = TextDetectionOptions {
            collect_info: false,
            detect_packages: false,
            detect_application_packages: false,
            detect_system_packages: false,
            detect_packages_in_compiled: false,
            detect_copyrights: false,
            detect_generated: false,
            detect_emails: true,
            detect_urls: false,
            max_emails: 2,
            max_urls: 50,
            timeout_seconds: 120.0,
        };

        extract_email_url_information(
            &mut builder,
            "first jakub@redhat.com second jakub@redhat.com third contyk@redhat.com",
            &options,
            true,
        );

        let file = builder
            .name("binary.bin".to_string())
            .base_name("binary".to_string())
            .extension(".bin".to_string())
            .path("binary.bin".to_string())
            .file_type(FileType::File)
            .size(1)
            .build()
            .expect("builder should produce file info");

        assert_eq!(file.emails.len(), 2, "emails: {:?}", file.emails);
        assert_eq!(file.emails[0].email, "jakub@redhat.com");
        assert_eq!(file.emails[1].email, "contyk@redhat.com");
    }

    #[test]
    fn test_binary_string_copyright_candidate_rejects_gibberish_holder_text() {
        let gibberish = "(c) S8@9 K @9 D @9 I,@9N(@ F@@9L,@ HD@9) M0@9s J'@y DH@9Ih@y";
        assert!(!is_binary_string_copyright_candidate(gibberish));
    }

    #[test]
    fn test_binary_string_copyright_candidate_keeps_real_notice() {
        let notice = "Copyright nexB and others (c) 2012";
        assert!(is_binary_string_copyright_candidate(notice));
    }

    #[test]
    fn test_binary_string_copyright_candidate_rejects_changelog_phrase() {
        assert!(!is_binary_string_copyright_candidate(
            "Copyright - split out libs"
        ));
    }

    #[test]
    fn test_binary_string_email_candidate_rejects_gibberish() {
        assert!(!is_binary_string_email_candidate("6h@fo.lwft"));
    }

    #[test]
    fn test_binary_string_email_candidate_keeps_gnu_bug_address() {
        assert!(is_binary_string_email_candidate("bug-coreutils@gnu.org"));
    }

    #[test]
    fn test_binary_string_url_candidate_rejects_short_fake_host() {
        assert!(!is_binary_string_url_candidate("http://ftp.so/"));
    }

    #[test]
    fn test_binary_string_url_candidate_keeps_gnu_help_url() {
        assert!(is_binary_string_url_candidate(
            "https://www.gnu.org/software/coreutils/"
        ));
    }

    #[test]
    fn test_binary_string_url_candidate_rejects_bare_root_domain() {
        assert!(!is_binary_string_url_candidate("http://gmail.com/"));
    }

    #[test]
    fn test_binary_string_url_candidate_keeps_project_subdomain_root() {
        assert!(is_binary_string_url_candidate("http://gcc.gnu.org"));
    }

    #[test]
    fn test_binary_string_url_candidate_keeps_long_org_root_domain() {
        assert!(is_binary_string_url_candidate("https://publicsuffix.org/"));
    }

    #[test]
    fn test_binary_string_url_candidate_keeps_short_project_path() {
        assert!(is_binary_string_url_candidate("http://tukaani.org/xz/"));
    }

    #[test]
    fn test_binary_string_author_candidate_keeps_named_author_with_email() {
        assert!(is_binary_string_author_candidate(
            "Andreas Schneider <asn@redhat.com>"
        ));
    }

    #[test]
    fn test_binary_string_author_candidate_rejects_gibberish() {
        assert!(!is_binary_string_author_candidate(
            "S8@9 K @9 D @9 I,@9N(@ F@@9L,@ HD@9"
        ));
    }

    #[test]
    fn test_binary_string_author_candidate_rejects_changelog_phrase() {
        assert!(!is_binary_string_author_candidate(
            "Developers can enable them. - revert news user back to"
        ));
    }

    #[test]
    fn test_extract_named_author_from_binary_line_recovers_by_prefix() {
        assert_eq!(
            extract_named_author_from_binary_line("Patch by Andreas Schneider <asn@redhat.com>"),
            Some("Andreas Schneider <asn@redhat.com>".to_string())
        );
    }

    #[test]
    fn test_extract_named_author_from_binary_line_recovers_parenthesized_email() {
        assert_eq!(
            extract_named_author_from_binary_line(
                "same for both OpenSSL and NSS by Rob Crittenden (rcritten@redhat.com)"
            ),
            Some("Rob Crittenden (rcritten@redhat.com)".to_string())
        );
    }

    #[test]
    fn test_extract_named_author_from_binary_line_rejects_plain_changelog_packager_line() {
        assert_eq!(
            extract_named_author_from_binary_line(
                "Rob Crittenden <rcritten@redhat.com> - 3.11.7-9"
            ),
            None
        );
    }

    #[test]
    fn test_extract_named_author_from_binary_line_keeps_email_only_review_author() {
        assert_eq!(
            extract_named_author_from_binary_line(
                "Changes as per initial review by panemade@gmail.com"
            ),
            Some("panemade@gmail.com".to_string())
        );
    }

    #[test]
    fn test_binary_string_author_candidate_rejects_multiple_emails_on_one_line() {
        assert!(!is_binary_string_author_candidate(
            "Rob Crittenden (rcritten@redhat.com) jakub@redhat.com"
        ));
    }

    #[test]
    fn test_compute_percentage_of_license_text_counts_unknown_tokens() {
        let index = create_test_index(&[("alpha", 0), ("mit", 1)], 2);
        let text = "alpha MIT omega";
        let query = Query::from_extracted_text(text, &index, false).expect("query should build");
        let mut detection = make_detection("");
        detection.matches[0].coordinates =
            MatchCoordinates::query_region(PositionSpan::from_positions(vec![1]));
        detection.matches[0].start_token = 1;
        detection.matches[0].end_token = 2;

        let percentage = compute_percentage_of_license_text(&query, &[detection]);

        assert_eq!(percentage, 33.33);
    }

    #[test]
    fn test_scan_options_fingerprint_changes_with_license_score() {
        let text_options = crate::scanner::TextDetectionOptions::default();
        let default_fingerprint = scan_options_fingerprint(
            &text_options,
            LicenseScanOptions {
                min_score: 0,
                ..LicenseScanOptions::default()
            },
            None,
        );
        let filtered_fingerprint = scan_options_fingerprint(
            &text_options,
            LicenseScanOptions {
                min_score: 70,
                ..LicenseScanOptions::default()
            },
            None,
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
