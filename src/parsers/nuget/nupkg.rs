use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::models::{DatasourceId, PackageData, PackageType};
use crate::parser_warn as warn;

use super::super::PackageParser;
use super::default_package_data;
use super::nuspec::parse_nuspec_content;

const MAX_ARCHIVE_SIZE: u64 = 100 * 1024 * 1024;
const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;
const MAX_COMPRESSION_RATIO: f64 = 100.0;
const MAX_UNCOMPRESSED_SIZE: u64 = 1024 * 1024 * 1024;

pub struct NupkgParser;

impl PackageParser for NupkgParser {
    const PACKAGE_TYPE: PackageType = PackageType::Nuget;

    fn is_match(path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext == "nupkg")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        vec![match extract_nupkg_archive(path) {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to extract .nupkg at {:?}: {}", path, e);
                default_package_data(Some(DatasourceId::NugetNupkg))
            }
        }]
    }
}

fn extract_nupkg_archive(path: &Path) -> Result<PackageData, String> {
    use std::fs;
    use zip::ZipArchive;

    let file_metadata =
        fs::metadata(path).map_err(|e| format!("Failed to read file metadata: {}", e))?;
    let archive_size = file_metadata.len();

    if archive_size > MAX_ARCHIVE_SIZE {
        return Err(format!(
            "Archive too large: {} bytes (limit: {} bytes)",
            archive_size, MAX_ARCHIVE_SIZE
        ));
    }

    let file = File::open(path).map_err(|e| format!("Failed to open archive: {}", e))?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| format!("Failed to read ZIP archive: {}", e))?;

    let mut total_uncompressed: u64 = 0;

    for i in 0..archive.len() {
        let content = {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| format!("Failed to read ZIP entry: {}", e))?;

            let entry_name = entry.name().to_string();
            let entry_size = entry.size();

            total_uncompressed += entry_size;
            if total_uncompressed > MAX_UNCOMPRESSED_SIZE {
                warn!(
                    "NuGet: total uncompressed size exceeds {} bytes for {:?}",
                    MAX_UNCOMPRESSED_SIZE, path
                );
                return Err(format!(
                    "Total uncompressed size exceeds limit: {} bytes (limit: {} bytes)",
                    total_uncompressed, MAX_UNCOMPRESSED_SIZE
                ));
            }

            if !entry_name.ends_with(".nuspec") {
                continue;
            }

            if entry_size > MAX_FILE_SIZE {
                return Err(format!(
                    ".nuspec too large: {} bytes (limit: {} bytes)",
                    entry_size, MAX_FILE_SIZE
                ));
            }

            let compressed_size = entry.compressed_size();
            if compressed_size > 0 {
                let ratio = entry_size as f64 / compressed_size as f64;
                if ratio > MAX_COMPRESSION_RATIO {
                    return Err(format!(
                        "Suspicious compression ratio: {:.2}:1 (limit: {:.0}:1)",
                        ratio, MAX_COMPRESSION_RATIO
                    ));
                }
            }

            let mut content = String::new();
            entry
                .read_to_string(&mut content)
                .map_err(|e| format!("Failed to read .nuspec: {}", e))?;
            content
        };

        let mut package_data = parse_nuspec_content(&content)?;

        let license_file = package_data.extra_data.as_ref().and_then(|extra| {
            extra
                .get("license_file")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
        });

        if let Some(license_file) = license_file
            && let Some(license_text) = read_nupkg_license_file(&mut archive, &license_file)?
        {
            package_data.extracted_license_statement = Some(license_text);
        }

        return Ok(package_data);
    }

    Err("No .nuspec file found in archive".to_string())
}

fn read_nupkg_license_file(
    archive: &mut zip::ZipArchive<File>,
    license_file: &str,
) -> Result<Option<String>, String> {
    if license_file.split('/').any(|c| c == "..") || license_file.split('\\').any(|c| c == "..") {
        warn!(
            "NuGet: path traversal detected in license file path: {}",
            license_file
        );
        return Ok(None);
    }

    let normalized_target = license_file.replace('\\', "/");

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read ZIP entry: {}", e))?;
        let entry_name = entry.name().replace('\\', "/");

        if entry_name != normalized_target
            && !entry_name.ends_with(&format!("/{}", normalized_target))
        {
            continue;
        }

        let entry_size = entry.size();
        if entry_size > MAX_FILE_SIZE {
            return Err(format!(
                "License file too large: {} bytes (limit: {} bytes)",
                entry_size, MAX_FILE_SIZE
            ));
        }

        let mut content = Vec::new();
        entry
            .read_to_end(&mut content)
            .map_err(|e| format!("Failed to read license file from archive: {}", e))?;

        return Ok(Some(String::from_utf8_lossy(&content).to_string()));
    }

    Ok(None)
}

crate::register_parser!(
    ".NET .nupkg package archive",
    &["**/*.nupkg"],
    "nuget",
    "C#",
    Some("https://learn.microsoft.com/en-us/nuget/create-packages/creating-a-package"),
);
