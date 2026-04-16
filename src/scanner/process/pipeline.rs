use super::contacts::extract_email_url_information;
use super::copyright::extract_copyright_information;
use super::license::extract_license_information;
use super::special_cases::{is_go_non_production_source, should_skip_text_detection};
use crate::license_detection::LicenseDetectionEngine;
use crate::models::{DatasourceId, FileInfo, FileInfoBuilder, FileType, Sha256Digest};
use crate::parsers::compiled_binary::{
    is_supported_compiled_binary_format, try_parse_compiled_bytes,
};
use crate::parsers::windows_executable::try_parse_windows_executable_bytes;
use crate::parsers::{try_parse_file, try_parse_file_with_license_engine};
use crate::progress::ScanProgress;
use crate::scanner::{LicenseScanOptions, TextDetectionOptions};
use crate::utils::file::{
    ExtractedTextKind, FileInfoClassification, augment_license_detection_text, classify_file_info,
    extract_text_for_detection_with_diagnostics, get_creation_date,
};
use crate::utils::generated::generated_code_hints_from_bytes;
use crate::utils::hash::{calculate_md5, calculate_sha1, calculate_sha1_git, calculate_sha256};
use crate::utils::text::{
    remove_verbatim_escape_sequences, should_remove_verbatim_escape_sequences,
};
use anyhow::Error;
use std::borrow::Cow;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

const LARGE_NON_SOURCE_JSON_LICENSE_TEXT_BYTES: usize = 128 * 1024;

pub(super) fn process_file(
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
    let filesystem_path = absolute_filesystem_path(path);
    let buffer = fs::read(&filesystem_path)?;
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
    let classification = classify_file_info(&filesystem_path, &buffer);

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

    if should_skip_text_detection(&filesystem_path, &buffer) {
        return Ok((is_generated, sha256, classification.is_source));
    }

    if text_options.detect_packages {
        let started = Instant::now();
        let parse_result = if let Some(engine) = license_engine.clone() {
            try_parse_file_with_license_engine(&filesystem_path, Some(engine))
        } else {
            try_parse_file(&filesystem_path)
        }
        .or_else(|| {
            text_options
                .detect_application_packages
                .then(|| try_parse_windows_executable_bytes(&filesystem_path, &buffer))
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
        extract_text_for_detection_with_diagnostics(&filesystem_path, &buffer);
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
        path,
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

    let text_content_for_license_detection =
        prepare_license_detection_text(path, &classification, text_content);

    if license_enabled {
        let started = Instant::now();
        extract_license_information(
            file_info_builder,
            scan_errors,
            &filesystem_path,
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
            &filesystem_path,
            text_content_for_license_detection,
            license_engine,
            license_options,
            from_binary_strings,
        )?;
    }

    if is_timeout_exceeded(started, text_options.timeout_seconds) {
        return Err(Error::msg(format!(
            "Timeout during license scan (> {:.2}s)",
            text_options.timeout_seconds
        )));
    }

    Ok((is_generated, sha256, classification.is_source))
}

fn prepare_license_detection_text(
    path: &Path,
    classification: &FileInfoClassification,
    text_content: String,
) -> String {
    let text_content = if crate::utils::sourcemap::is_sourcemap(path) {
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
    let text_content = augment_license_detection_text(path, &text_content);
    cap_non_source_json_license_text(path, classification, text_content.as_ref()).into_owned()
}

fn absolute_filesystem_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }

    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
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
        || error.contains("Timeout during ")
        || error.contains("Processing interrupted due to timeout")
}

fn cap_non_source_json_license_text<'a>(
    path: &Path,
    classification: &FileInfoClassification,
    text: &'a str,
) -> Cow<'a, str> {
    if classification.is_source
        || crate::utils::sourcemap::is_sourcemap(path)
        || is_npm_lockfile(path)
        || !is_json_like_text(classification, path)
        || text.len() <= LARGE_NON_SOURCE_JSON_LICENSE_TEXT_BYTES
    {
        return Cow::Borrowed(text);
    }

    Cow::Owned(
        truncate_at_char_boundary(text, LARGE_NON_SOURCE_JSON_LICENSE_TEXT_BYTES).to_string(),
    )
}

fn is_json_like_text(classification: &FileInfoClassification, path: &Path) -> bool {
    classification.file_type == "JSON text data"
        || classification.mime_type == "application/json"
        || classification.mime_type.ends_with("+json")
        || path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
}

fn is_npm_lockfile(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            matches!(
                name,
                "package-lock.json"
                    | ".package-lock.json"
                    | "npm-shrinkwrap.json"
                    | ".npm-shrinkwrap.json"
            )
        })
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

#[cfg(test)]
#[path = "pipeline_test.rs"]
mod tests;
