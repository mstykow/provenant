use crate::models::{FileInfo, FileType};
use std::fs;
use std::path::Path;

const PEM_CERTIFICATE_HEADERS: &[(&str, &str)] = &[
    ("-----BEGIN CERTIFICATE-----", "-----END CERTIFICATE-----"),
    (
        "-----BEGIN TRUSTED CERTIFICATE-----",
        "-----END TRUSTED CERTIFICATE-----",
    ),
];

pub(super) fn should_skip_text_detection(path: &Path, buffer: &[u8]) -> bool {
    is_pem_certificate_file(path, buffer)
}

pub(super) fn is_go_non_production_source(path: &Path) -> std::io::Result<bool> {
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

pub(super) fn process_directory(
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
    let base_name = name.clone();

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
        scan_diagnostics: Vec::new(),
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
#[path = "special_cases_test.rs"]
mod tests;
