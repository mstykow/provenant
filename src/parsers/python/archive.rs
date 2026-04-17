use super::PythonParser;
use super::utils::{
    build_pypi_urls, calculate_file_checksums, default_package_data, normalize_python_package_name,
    parse_requires_txt, strip_python_archive_extension,
};
use crate::models::{DatasourceId, FileReference, PackageData, Sha256Digest};
use crate::parser_warn as warn;
use crate::parsers::PackageParser;
use crate::parsers::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use bzip2::read::BzDecoder;
use csv::ReaderBuilder;
use flate2::read::GzDecoder;
use liblzma::read::XzDecoder;
use packageurl::PackageUrl;
use serde_json::Value as JsonValue;
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path};
use tar::Archive;
use zip::ZipArchive;

#[derive(Clone, Copy, Debug)]
enum PythonSdistArchiveFormat {
    TarGz,
    Tgz,
    TarBz2,
    TarXz,
    Zip,
}

#[derive(Clone, Debug)]
struct ValidatedZipEntry {
    index: usize,
    name: String,
}

pub(super) const MAX_ARCHIVE_SIZE: u64 = 100 * 1024 * 1024;
pub(super) const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;
pub(super) const MAX_COMPRESSION_RATIO: f64 = 100.0;

fn collect_validated_zip_entries<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    path: &Path,
    archive_type: &str,
) -> Result<Vec<ValidatedZipEntry>, String> {
    let mut total_extracted = 0u64;
    let mut entries = Vec::new();
    let mut entry_count = 0usize;

    for i in 0..archive.len() {
        entry_count += 1;
        if entry_count > MAX_ITERATION_COUNT {
            warn!(
                "Exceeded max entry count in {} {:?}; stopping at {} entries",
                archive_type, path, MAX_ITERATION_COUNT
            );
            break;
        }
        if let Ok(file) = archive.by_index_raw(i) {
            let compressed_size = file.compressed_size();
            let uncompressed_size = file.size();
            let Some(entry_name) = normalize_archive_entry_path(file.name()) else {
                warn!(
                    "Skipping unsafe path in {} {:?}: {}",
                    archive_type,
                    path,
                    file.name()
                );
                continue;
            };

            if compressed_size > 0 {
                let ratio = uncompressed_size as f64 / compressed_size as f64;
                if ratio > MAX_COMPRESSION_RATIO {
                    warn!(
                        "Suspicious compression ratio in {} {:?}: {:.2}:1",
                        archive_type, path, ratio
                    );
                    continue;
                }
            }

            if uncompressed_size > MAX_FILE_SIZE {
                warn!(
                    "File too large in {} {:?}: {} bytes (limit: {} bytes)",
                    archive_type, path, uncompressed_size, MAX_FILE_SIZE
                );
                continue;
            }

            total_extracted += uncompressed_size;
            if total_extracted > MAX_ARCHIVE_SIZE {
                let msg = format!(
                    "Total extracted size exceeds limit for {} {:?}",
                    archive_type, path
                );
                warn!("{}", msg);
                return Err(msg);
            }

            entries.push(ValidatedZipEntry {
                index: i,
                name: entry_name,
            });
        }
    }

    Ok(entries)
}

pub(super) fn is_python_sdist_archive_path(path: &Path) -> bool {
    detect_python_sdist_archive_format(path).is_some()
}

pub(super) fn is_valid_wheel_archive_path(path: &Path) -> bool {
    if !path.is_file() {
        return true;
    }

    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let mut archive = match ZipArchive::new(file) {
        Ok(archive) => archive,
        Err(_) => return false,
    };

    let validated_entries = match collect_validated_zip_entries(&mut archive, path, "wheel") {
        Ok(entries) => entries,
        Err(_) => return false,
    };

    find_validated_zip_entry_by_suffix(&validated_entries, ".dist-info/METADATA").is_some()
}

fn detect_python_sdist_archive_format(path: &Path) -> Option<PythonSdistArchiveFormat> {
    let file_name = path.file_name()?.to_str()?.to_ascii_lowercase();

    if !is_likely_python_sdist_filename(&file_name) {
        return None;
    }

    if file_name.ends_with(".tar.gz") {
        tar_gz_sdist_contains_pkg_info(path).then_some(PythonSdistArchiveFormat::TarGz)
    } else if file_name.ends_with(".tgz") {
        tgz_sdist_contains_pkg_info(path).then_some(PythonSdistArchiveFormat::Tgz)
    } else if file_name.ends_with(".tar.bz2") {
        tar_bz2_sdist_contains_pkg_info(path).then_some(PythonSdistArchiveFormat::TarBz2)
    } else if file_name.ends_with(".tar.xz") {
        tar_xz_sdist_contains_pkg_info(path).then_some(PythonSdistArchiveFormat::TarXz)
    } else if file_name.ends_with(".zip") {
        zip_sdist_contains_pkg_info(path).then_some(PythonSdistArchiveFormat::Zip)
    } else {
        None
    }
}

fn tar_gz_sdist_contains_pkg_info(path: &Path) -> bool {
    let Some(compressed_size) = compressed_archive_size(path) else {
        return false;
    };
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let decoder = GzDecoder::new(file);
    tar_sdist_contains_pkg_info(path, decoder, "tar.gz", compressed_size)
}

fn tar_bz2_sdist_contains_pkg_info(path: &Path) -> bool {
    let Some(compressed_size) = compressed_archive_size(path) else {
        return false;
    };
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let decoder = BzDecoder::new(file);
    tar_sdist_contains_pkg_info(path, decoder, "tar.bz2", compressed_size)
}

fn tar_xz_sdist_contains_pkg_info(path: &Path) -> bool {
    let Some(compressed_size) = compressed_archive_size(path) else {
        return false;
    };
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let decoder = XzDecoder::new(file);
    tar_sdist_contains_pkg_info(path, decoder, "tar.xz", compressed_size)
}

fn compressed_archive_size(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|metadata| metadata.len())
}

fn tar_sdist_contains_pkg_info<R: Read>(
    path: &Path,
    reader: R,
    archive_type: &str,
    compressed_size: u64,
) -> bool {
    let Some(entries) = collect_tar_sdist_entries(path, reader, archive_type, compressed_size)
    else {
        return false;
    };

    select_sdist_pkginfo_entry(path, &entries).is_some()
}

fn tgz_sdist_contains_pkg_info(path: &Path) -> bool {
    if !path.is_file() {
        return true;
    }

    let Some(compressed_size) = compressed_archive_size(path) else {
        return false;
    };
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let decoder = GzDecoder::new(file);
    tar_sdist_contains_pkg_info(path, decoder, "tgz", compressed_size)
}

fn zip_sdist_contains_pkg_info(path: &Path) -> bool {
    if !path.is_file() {
        return true;
    }

    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let mut archive = match ZipArchive::new(file) {
        Ok(archive) => archive,
        Err(_) => return false,
    };

    let validated_entries = match collect_validated_zip_entries(&mut archive, path, "sdist zip") {
        Ok(entries) => entries,
        Err(_) => return false,
    };
    let metadata_entries: Vec<_> = validated_entries
        .iter()
        .filter(|entry| entry.name.ends_with("/PKG-INFO"))
        .filter_map(|entry| {
            read_validated_zip_entry(&mut archive, entry, path, "sdist zip")
                .ok()
                .map(|content| (entry.name.clone(), content))
        })
        .collect();

    has_matching_sdist_pkginfo_candidate(path, &metadata_entries)
}

pub(super) fn is_likely_python_sdist_filename(file_name: &str) -> bool {
    let Some(stem) = strip_python_archive_extension(file_name) else {
        return false;
    };

    let Some((name, version)) = stem.rsplit_once('-') else {
        return false;
    };

    !name.is_empty()
        && !version.is_empty()
        && version.chars().any(|ch| ch.is_ascii_digit())
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

pub(super) fn extract_from_sdist_archive(path: &Path) -> PackageData {
    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            warn!(
                "Failed to read metadata for sdist archive {:?}: {}",
                path, e
            );
            return default_package_data(path);
        }
    };

    if metadata.len() > MAX_ARCHIVE_SIZE {
        warn!(
            "sdist archive too large: {} bytes (limit: {} bytes)",
            metadata.len(),
            MAX_ARCHIVE_SIZE
        );
        return default_package_data(path);
    }

    let Some(format) = detect_python_sdist_archive_format(path) else {
        return default_package_data(path);
    };

    let mut package_data = match format {
        PythonSdistArchiveFormat::TarGz | PythonSdistArchiveFormat::Tgz => {
            let file = match File::open(path) {
                Ok(file) => file,
                Err(e) => {
                    warn!("Failed to open sdist archive {:?}: {}", path, e);
                    return default_package_data(path);
                }
            };
            let decoder = GzDecoder::new(file);
            extract_from_tar_sdist_archive(path, decoder, "tar.gz", metadata.len())
        }
        PythonSdistArchiveFormat::TarBz2 => {
            let file = match File::open(path) {
                Ok(file) => file,
                Err(e) => {
                    warn!("Failed to open sdist archive {:?}: {}", path, e);
                    return default_package_data(path);
                }
            };
            let decoder = BzDecoder::new(file);
            extract_from_tar_sdist_archive(path, decoder, "tar.bz2", metadata.len())
        }
        PythonSdistArchiveFormat::TarXz => {
            let file = match File::open(path) {
                Ok(file) => file,
                Err(e) => {
                    warn!("Failed to open sdist archive {:?}: {}", path, e);
                    return default_package_data(path);
                }
            };
            let decoder = XzDecoder::new(file);
            extract_from_tar_sdist_archive(path, decoder, "tar.xz", metadata.len())
        }
        PythonSdistArchiveFormat::Zip => extract_from_zip_sdist_archive(path),
    };

    if package_data.package_type.is_some() {
        let (size, sha256) = calculate_file_checksums(path);
        package_data.size = size;
        package_data.sha256 = sha256;
    }

    package_data
}

fn extract_from_tar_sdist_archive<R: Read>(
    path: &Path,
    reader: R,
    archive_type: &str,
    compressed_size: u64,
) -> PackageData {
    let Some(entries) = collect_tar_sdist_entries(path, reader, archive_type, compressed_size)
    else {
        return default_package_data(path);
    };

    build_sdist_package_data(path, entries)
}

fn collect_tar_sdist_entries<R: Read>(
    path: &Path,
    reader: R,
    archive_type: &str,
    compressed_size: u64,
) -> Option<Vec<(String, String)>> {
    let mut archive = Archive::new(reader);
    let archive_entries = match archive.entries() {
        Ok(entries) => entries,
        Err(e) => {
            warn!(
                "Failed to read {} sdist archive {:?}: {}",
                archive_type, path, e
            );
            return None;
        }
    };

    let mut total_extracted = 0u64;
    let mut entries = Vec::new();
    let mut entry_count = 0usize;

    for entry_result in archive_entries {
        entry_count += 1;
        if entry_count > MAX_ITERATION_COUNT {
            warn!(
                "Exceeded max entry count in {} sdist {:?}; stopping at {} entries",
                archive_type, path, MAX_ITERATION_COUNT
            );
            break;
        }

        let mut entry = match entry_result {
            Ok(entry) => entry,
            Err(e) => {
                warn!(
                    "Failed to read {} sdist entry from {:?}: {}",
                    archive_type, path, e
                );
                continue;
            }
        };

        let entry_size = entry.size();
        if entry_size > MAX_FILE_SIZE {
            warn!(
                "File too large in {} sdist {:?}: {} bytes (limit: {} bytes)",
                archive_type, path, entry_size, MAX_FILE_SIZE
            );
            continue;
        }

        total_extracted += entry_size;
        if total_extracted > MAX_ARCHIVE_SIZE {
            warn!(
                "Total extracted size exceeds limit for {} sdist {:?}",
                archive_type, path
            );
            return None;
        }

        if compressed_size > 0 {
            let ratio = total_extracted as f64 / compressed_size as f64;
            if ratio > MAX_COMPRESSION_RATIO {
                warn!(
                    "Suspicious compression ratio in {} sdist {:?}: {:.2}:1",
                    archive_type, path, ratio
                );
                return None;
            }
        }

        let entry_path = match entry.path() {
            Ok(path) => path.to_string_lossy().replace('\\', "/"),
            Err(e) => {
                warn!(
                    "Failed to get {} sdist entry path from {:?}: {}",
                    archive_type, path, e
                );
                continue;
            }
        };

        let Some(entry_path) = normalize_archive_entry_path(&entry_path) else {
            warn!("Skipping unsafe {} sdist path in {:?}", archive_type, path);
            continue;
        };

        if !is_relevant_sdist_text_entry(&entry_path) {
            continue;
        }

        if let Ok(content) = read_limited_utf8(
            &mut entry,
            MAX_FILE_SIZE,
            &format!("{} entry {}", archive_type, entry_path),
        ) {
            entries.push((entry_path, content));
        }
    }

    Some(entries)
}

fn extract_from_zip_sdist_archive(path: &Path) -> PackageData {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) => {
            warn!("Failed to open zip sdist archive {:?}: {}", path, e);
            return default_package_data(path);
        }
    };

    let mut archive = match ZipArchive::new(file) {
        Ok(archive) => archive,
        Err(e) => {
            warn!("Failed to read zip sdist archive {:?}: {}", path, e);
            return default_package_data(path);
        }
    };

    let validated_entries = match collect_validated_zip_entries(&mut archive, path, "sdist zip") {
        Ok(entries) => entries,
        Err(_) => return default_package_data(path),
    };

    let mut entries = Vec::new();
    for entry in validated_entries.iter() {
        if !is_relevant_sdist_text_entry(&entry.name) {
            continue;
        }

        if let Ok(content) = read_validated_zip_entry(&mut archive, entry, path, "sdist zip") {
            entries.push((entry.name.clone(), content));
        }
    }

    build_sdist_package_data(path, entries)
}

fn is_relevant_sdist_text_entry(entry_path: &str) -> bool {
    entry_path.ends_with("/PKG-INFO")
        || entry_path.ends_with("/requires.txt")
        || entry_path.ends_with("/SOURCES.txt")
}

fn build_sdist_package_data(path: &Path, entries: Vec<(String, String)>) -> PackageData {
    let Some((metadata_path, metadata_content)) = select_sdist_pkginfo_entry(path, &entries) else {
        warn!("No PKG-INFO file found in sdist archive {:?}", path);
        return default_package_data(path);
    };

    let mut package_data = super::rfc822_meta::python_parse_rfc822_content(
        &metadata_content,
        DatasourceId::PypiSdistPkginfo,
    );
    merge_sdist_archive_dependencies(&entries, &metadata_path, &mut package_data);
    merge_sdist_archive_file_references(&entries, &metadata_path, &mut package_data);
    apply_sdist_name_version_fallback(path, &mut package_data);
    package_data.datasource_id = Some(DatasourceId::PypiSdist);
    package_data
}

fn select_sdist_pkginfo_entry(
    archive_path: &Path,
    entries: &[(String, String)],
) -> Option<(String, String)> {
    let expected_name = sdist_archive_expected_name(archive_path);

    entries
        .iter()
        .filter(|(entry_path, _)| entry_path.ends_with("/PKG-INFO"))
        .min_by_key(|(entry_path, content)| {
            let components: Vec<_> = entry_path
                .split('/')
                .filter(|part| !part.is_empty())
                .collect();
            let candidate_name = sdist_pkginfo_candidate_name(content);
            let name_rank = if candidate_name == expected_name {
                0
            } else {
                1
            };
            let kind_rank = sdist_pkginfo_kind_rank(entry_path);

            (name_rank, kind_rank, components.len(), entry_path.clone())
        })
        .map(|(entry_path, content)| (entry_path.clone(), content.clone()))
}

fn has_matching_sdist_pkginfo_candidate(archive_path: &Path, entries: &[(String, String)]) -> bool {
    let Some(expected_name) = sdist_archive_expected_name(archive_path) else {
        return false;
    };

    entries.iter().any(|(entry_path, content)| {
        sdist_pkginfo_kind_rank(entry_path) < 3
            && sdist_pkginfo_candidate_name(content).as_deref() == Some(expected_name.as_str())
    })
}

fn sdist_archive_expected_name(archive_path: &Path) -> Option<String> {
    archive_path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(strip_python_archive_extension)
        .and_then(|stem| {
            stem.rsplit_once('-')
                .map(|(name, _)| normalize_python_package_name(name))
        })
}

fn sdist_pkginfo_candidate_name(content: &str) -> Option<String> {
    let metadata = super::super::rfc822::parse_rfc822_content(content);
    super::super::rfc822::get_header_first(&metadata.headers, "name")
        .map(|name| normalize_python_package_name(&name))
}

fn sdist_pkginfo_kind_rank(entry_path: &str) -> usize {
    let components: Vec<_> = entry_path
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();

    if components.len() == 3 && components[1].ends_with(".egg-info") && components[2] == "PKG-INFO"
    {
        0
    } else if components.len() == 2 && components[1] == "PKG-INFO" {
        1
    } else if entry_path.ends_with(".egg-info/PKG-INFO") {
        2
    } else {
        3
    }
}

fn merge_sdist_archive_dependencies(
    entries: &[(String, String)],
    metadata_path: &str,
    package_data: &mut PackageData,
) {
    let metadata_dir = metadata_path
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .unwrap_or("");
    let archive_root = metadata_path.split('/').next().unwrap_or("");
    let matched_egg_info_dir =
        select_matching_sdist_egg_info_dir(entries, archive_root, package_data.name.as_deref());
    let mut extra_dependencies = Vec::new();

    for (entry_path, content) in entries {
        let is_direct_requires =
            !metadata_dir.is_empty() && entry_path == &format!("{metadata_dir}/requires.txt");
        let is_egg_info_requires = matched_egg_info_dir.as_ref().is_some_and(|egg_info_dir| {
            entry_path == &format!("{archive_root}/{egg_info_dir}/requires.txt")
        });

        if is_direct_requires || is_egg_info_requires {
            extra_dependencies.extend(parse_requires_txt(content));
        }
    }

    for dependency in extra_dependencies {
        if !package_data.dependencies.iter().any(|existing| {
            existing.purl == dependency.purl
                && existing.scope == dependency.scope
                && existing.extracted_requirement == dependency.extracted_requirement
                && existing.extra_data == dependency.extra_data
        }) {
            package_data.dependencies.push(dependency);
        }
    }
}

fn merge_sdist_archive_file_references(
    entries: &[(String, String)],
    metadata_path: &str,
    package_data: &mut PackageData,
) {
    let metadata_dir = metadata_path
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .unwrap_or("");
    let archive_root = metadata_path.split('/').next().unwrap_or("");
    let matched_egg_info_dir =
        select_matching_sdist_egg_info_dir(entries, archive_root, package_data.name.as_deref());
    let mut extra_refs = Vec::new();

    for (entry_path, content) in entries {
        let is_direct_sources =
            !metadata_dir.is_empty() && entry_path == &format!("{metadata_dir}/SOURCES.txt");
        let is_egg_info_sources = matched_egg_info_dir.as_ref().is_some_and(|egg_info_dir| {
            entry_path == &format!("{archive_root}/{egg_info_dir}/SOURCES.txt")
        });

        if is_direct_sources || is_egg_info_sources {
            extra_refs.extend(parse_sources_txt(content));
        }
    }

    for file_ref in extra_refs {
        if !package_data
            .file_references
            .iter()
            .any(|existing| existing.path == file_ref.path)
        {
            package_data.file_references.push(file_ref);
        }
    }
}

fn select_matching_sdist_egg_info_dir(
    entries: &[(String, String)],
    archive_root: &str,
    package_name: Option<&str>,
) -> Option<String> {
    let normalized_package_name = package_name.map(normalize_python_package_name);

    entries
        .iter()
        .filter_map(|(entry_path, _)| {
            let components: Vec<_> = entry_path
                .split('/')
                .filter(|part| !part.is_empty())
                .collect();
            if components.len() == 3
                && components[0] == archive_root
                && components[1].ends_with(".egg-info")
            {
                Some(components[1].to_string())
            } else {
                None
            }
        })
        .min_by_key(|egg_info_dir| {
            let normalized_dir_name =
                normalize_python_package_name(egg_info_dir.trim_end_matches(".egg-info"));
            let name_rank = if Some(normalized_dir_name.clone()) == normalized_package_name {
                0
            } else {
                1
            };

            (name_rank, egg_info_dir.clone())
        })
}

fn apply_sdist_name_version_fallback(path: &Path, package_data: &mut PackageData) {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return;
    };

    let Some(stem) = strip_python_archive_extension(file_name) else {
        return;
    };

    let Some((name, version)) = stem.rsplit_once('-') else {
        return;
    };

    if package_data.name.is_none() {
        package_data.name = Some(name.replace('_', "-"));
    }
    if package_data.version.is_none() {
        package_data.version = Some(version.to_string());
    }

    if package_data.purl.is_none()
        || package_data.repository_homepage_url.is_none()
        || package_data.repository_download_url.is_none()
        || package_data.api_data_url.is_none()
    {
        let (repository_homepage_url, repository_download_url, api_data_url, purl) =
            build_pypi_urls(
                package_data.name.as_deref(),
                package_data.version.as_deref(),
            );

        if package_data.repository_homepage_url.is_none() {
            package_data.repository_homepage_url = repository_homepage_url;
        }
        if package_data.repository_download_url.is_none() {
            package_data.repository_download_url = repository_download_url;
        }
        if package_data.api_data_url.is_none() {
            package_data.api_data_url = api_data_url;
        }
        if package_data.purl.is_none() {
            package_data.purl = purl;
        }
    }
}

pub(super) fn extract_from_wheel_archive(path: &Path) -> PackageData {
    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            warn!(
                "Failed to read metadata for wheel archive {:?}: {}",
                path, e
            );
            return default_package_data(path);
        }
    };

    if metadata.len() > MAX_ARCHIVE_SIZE {
        warn!(
            "Wheel archive too large: {} bytes (limit: {} bytes)",
            metadata.len(),
            MAX_ARCHIVE_SIZE
        );
        return default_package_data(path);
    }

    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            warn!("Failed to open wheel archive {:?}: {}", path, e);
            return default_package_data(path);
        }
    };

    let mut archive = match ZipArchive::new(file) {
        Ok(a) => a,
        Err(e) => {
            warn!("Failed to read wheel archive {:?}: {}", path, e);
            return default_package_data(path);
        }
    };

    let validated_entries = match collect_validated_zip_entries(&mut archive, path, "wheel") {
        Ok(entries) => entries,
        Err(_) => return default_package_data(path),
    };

    let metadata_entry =
        match find_validated_zip_entry_by_suffix(&validated_entries, ".dist-info/METADATA") {
            Some(entry) => entry,
            None => {
                warn!("No METADATA file found in wheel archive {:?}", path);
                return default_package_data(path);
            }
        };

    let content = match read_validated_zip_entry(&mut archive, metadata_entry, path, "wheel") {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to read METADATA from {:?}: {}", path, e);
            return default_package_data(path);
        }
    };

    let mut package_data =
        super::rfc822_meta::python_parse_rfc822_content(&content, DatasourceId::PypiWheel);

    let (size, sha256) = calculate_file_checksums(path);
    package_data.size = size;
    package_data.sha256 = sha256;

    if let Some(record_entry) =
        find_validated_zip_entry_by_suffix(&validated_entries, ".dist-info/RECORD")
        && let Ok(record_content) =
            read_validated_zip_entry(&mut archive, record_entry, path, "wheel")
    {
        package_data.file_references = parse_record_csv(&record_content);
    }

    if let Some(wheel_info) = parse_wheel_filename(path) {
        if package_data.name.is_none() {
            package_data.name = Some(wheel_info.name.clone());
        }
        if package_data.version.is_none() {
            package_data.version = Some(wheel_info.version.clone());
        }

        package_data.qualifiers = Some(std::collections::HashMap::from([(
            "extension".to_string(),
            format!(
                "{}-{}-{}",
                wheel_info.python_tag, wheel_info.abi_tag, wheel_info.platform_tag
            ),
        )]));

        package_data.purl = build_wheel_purl(
            package_data.name.as_deref(),
            package_data.version.as_deref(),
            &wheel_info,
        );

        let mut extra_data = package_data.extra_data.unwrap_or_default();
        extra_data.insert(
            "python_requires".to_string(),
            serde_json::Value::String(wheel_info.python_tag.clone()),
        );
        extra_data.insert(
            "abi_tag".to_string(),
            serde_json::Value::String(wheel_info.abi_tag.clone()),
        );
        extra_data.insert(
            "platform_tag".to_string(),
            serde_json::Value::String(wheel_info.platform_tag.clone()),
        );
        package_data.extra_data = Some(extra_data);
    }

    package_data
}

pub(super) fn extract_from_egg_archive(path: &Path) -> PackageData {
    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            warn!("Failed to read metadata for egg archive {:?}: {}", path, e);
            return default_package_data(path);
        }
    };

    if metadata.len() > MAX_ARCHIVE_SIZE {
        warn!(
            "Egg archive too large: {} bytes (limit: {} bytes)",
            metadata.len(),
            MAX_ARCHIVE_SIZE
        );
        return default_package_data(path);
    }

    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            warn!("Failed to open egg archive {:?}: {}", path, e);
            return default_package_data(path);
        }
    };

    let mut archive = match ZipArchive::new(file) {
        Ok(a) => a,
        Err(e) => {
            warn!("Failed to read egg archive {:?}: {}", path, e);
            return default_package_data(path);
        }
    };

    let validated_entries = match collect_validated_zip_entries(&mut archive, path, "egg") {
        Ok(entries) => entries,
        Err(_) => return default_package_data(path),
    };

    let pkginfo_entry = match find_validated_zip_entry_by_any_suffix(
        &validated_entries,
        &["EGG-INFO/PKG-INFO", ".egg-info/PKG-INFO"],
    ) {
        Some(entry) => entry,
        None => {
            warn!("No PKG-INFO file found in egg archive {:?}", path);
            return default_package_data(path);
        }
    };

    let content = match read_validated_zip_entry(&mut archive, pkginfo_entry, path, "egg") {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to read PKG-INFO from {:?}: {}", path, e);
            return default_package_data(path);
        }
    };

    let mut package_data =
        super::rfc822_meta::python_parse_rfc822_content(&content, DatasourceId::PypiEgg);

    let (size, sha256) = calculate_file_checksums(path);
    package_data.size = size;
    package_data.sha256 = sha256;

    if let Some(installed_files_entry) = find_validated_zip_entry_by_any_suffix(
        &validated_entries,
        &[
            "EGG-INFO/installed-files.txt",
            ".egg-info/installed-files.txt",
        ],
    ) && let Ok(installed_files_content) =
        read_validated_zip_entry(&mut archive, installed_files_entry, path, "egg")
    {
        package_data.file_references = parse_installed_files_txt(&installed_files_content);
    }

    if let Some(egg_info) = parse_egg_filename(path) {
        if package_data.name.is_none() {
            package_data.name = Some(egg_info.name.clone());
        }
        if package_data.version.is_none() {
            package_data.version = Some(egg_info.version.clone());
        }

        if let Some(python_version) = &egg_info.python_version {
            let mut extra_data = package_data.extra_data.unwrap_or_default();
            extra_data.insert(
                "python_version".to_string(),
                serde_json::Value::String(python_version.clone()),
            );
            package_data.extra_data = Some(extra_data);
        }
    }

    package_data.purl = build_egg_purl(
        package_data.name.as_deref(),
        package_data.version.as_deref(),
    );

    package_data
}

fn find_validated_zip_entry_by_suffix<'a>(
    entries: &'a [ValidatedZipEntry],
    suffix: &str,
) -> Option<&'a ValidatedZipEntry> {
    entries.iter().find(|entry| entry.name.ends_with(suffix))
}

fn find_validated_zip_entry_by_any_suffix<'a>(
    entries: &'a [ValidatedZipEntry],
    suffixes: &[&str],
) -> Option<&'a ValidatedZipEntry> {
    entries
        .iter()
        .find(|entry| suffixes.iter().any(|suffix| entry.name.ends_with(suffix)))
}

fn read_validated_zip_entry<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    entry: &ValidatedZipEntry,
    path: &Path,
    archive_type: &str,
) -> Result<String, String> {
    let mut file = archive
        .by_index(entry.index)
        .map_err(|e| format!("Failed to find entry {}: {}", entry.name, e))?;

    let compressed_size = file.compressed_size();
    let uncompressed_size = file.size();

    if compressed_size > 0 {
        let ratio = uncompressed_size as f64 / compressed_size as f64;
        if ratio > MAX_COMPRESSION_RATIO {
            return Err(format!(
                "Rejected suspicious compression ratio in {} {:?}: {:.2}:1",
                archive_type, path, ratio
            ));
        }
    }

    if uncompressed_size > MAX_FILE_SIZE {
        return Err(format!(
            "Rejected oversized entry in {} {:?}: {} bytes",
            archive_type, path, uncompressed_size
        ));
    }

    read_limited_utf8(
        &mut file,
        MAX_FILE_SIZE,
        &format!("{} entry {}", archive_type, entry.name),
    )
}

fn read_limited_utf8<R: Read>(
    reader: &mut R,
    max_bytes: u64,
    context: &str,
) -> Result<String, String> {
    let mut limited = reader.take(max_bytes + 1);
    let mut bytes = Vec::new();
    limited
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read {}: {}", context, e))?;

    if bytes.len() as u64 > max_bytes {
        return Err(format!(
            "{} exceeded {} byte limit while reading",
            context, max_bytes
        ));
    }

    match String::from_utf8(bytes) {
        Ok(s) => Ok(s),
        Err(err) => {
            let bytes = err.into_bytes();
            warn!("Invalid UTF-8 in archive entry; using lossy conversion");
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        }
    }
}

fn normalize_archive_entry_path(entry_path: &str) -> Option<String> {
    let normalized = entry_path.replace('\\', "/");
    if normalized.len() >= 3 {
        let bytes = normalized.as_bytes();
        if bytes[1] == b':' && bytes[2] == b'/' && bytes[0].is_ascii_alphabetic() {
            return None;
        }
    }
    let path = Path::new(&normalized);
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            Component::Normal(segment) => components.push(segment.to_string_lossy().to_string()),
            Component::CurDir => {}
            Component::RootDir | Component::ParentDir | Component::Prefix(_) => return None,
        }
    }

    (!components.is_empty()).then_some(components.join("/"))
}

pub(super) fn parse_record_csv(content: &str) -> Vec<FileReference> {
    let mut reader = ReaderBuilder::new()
        .has_headers(false)
        .from_reader(content.as_bytes());

    let mut file_references = Vec::new();
    let mut record_count = 0usize;

    for result in reader.records() {
        record_count += 1;
        if record_count > MAX_ITERATION_COUNT {
            warn!(
                "Exceeded max record count in RECORD CSV; stopping at {} records",
                MAX_ITERATION_COUNT
            );
            break;
        }
        match result {
            Ok(record) => {
                if record.len() < 3 {
                    continue;
                }

                let path = record.get(0).unwrap_or("").trim().to_string();
                if path.is_empty() {
                    continue;
                }

                let hash_field = record.get(1).unwrap_or("").trim();
                let size_field = record.get(2).unwrap_or("").trim();

                let sha256 = if !hash_field.is_empty() && hash_field.contains('=') {
                    let parts: Vec<&str> = hash_field.split('=').collect();
                    if parts.len() == 2 && parts[0] == "sha256" {
                        match URL_SAFE_NO_PAD.decode(parts[1]) {
                            Ok(decoded) => {
                                let hex = decoded
                                    .iter()
                                    .map(|b| format!("{:02x}", b))
                                    .collect::<String>();
                                Sha256Digest::from_hex(&hex).ok()
                            }
                            Err(_) => None,
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                let size = if !size_field.is_empty() && size_field != "-" {
                    size_field.parse::<u64>().ok()
                } else {
                    None
                };

                file_references.push(FileReference {
                    path,
                    size,
                    sha1: None,
                    md5: None,
                    sha256,
                    sha512: None,
                    extra_data: None,
                });
            }
            Err(e) => {
                warn!("Failed to parse RECORD CSV row: {}", e);
                continue;
            }
        }
    }

    file_references
}

pub(super) fn parse_installed_files_txt(content: &str) -> Vec<FileReference> {
    content
        .lines()
        .take(MAX_ITERATION_COUNT)
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(|path| FileReference {
            path: path.to_string(),
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            extra_data: None,
        })
        .collect()
}

pub(super) fn parse_sources_txt(content: &str) -> Vec<FileReference> {
    content
        .lines()
        .take(MAX_ITERATION_COUNT)
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|path| FileReference {
            path: path.to_string(),
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            extra_data: None,
        })
        .collect()
}

pub(super) struct WheelInfo {
    name: String,
    version: String,
    python_tag: String,
    abi_tag: String,
    platform_tag: String,
}

pub(super) fn parse_wheel_filename(path: &Path) -> Option<WheelInfo> {
    let stem = path.file_stem()?.to_string_lossy();
    let parts: Vec<&str> = stem.split('-').collect();

    if parts.len() >= 5 {
        Some(WheelInfo {
            name: parts[0].replace('_', "-"),
            version: parts[1].to_string(),
            python_tag: parts[2].to_string(),
            abi_tag: parts[3].to_string(),
            platform_tag: parts[4..].join("-"),
        })
    } else {
        None
    }
}

struct EggInfo {
    name: String,
    version: String,
    python_version: Option<String>,
}

fn parse_egg_filename(path: &Path) -> Option<EggInfo> {
    let stem = path.file_stem()?.to_string_lossy();
    let parts: Vec<&str> = stem.split('-').collect();

    if parts.len() >= 2 {
        Some(EggInfo {
            name: parts[0].replace('_', "-"),
            version: parts[1].to_string(),
            python_version: parts.get(2).map(|s| s.to_string()),
        })
    } else {
        None
    }
}

pub(super) fn build_wheel_purl(
    name: Option<&str>,
    version: Option<&str>,
    wheel_info: &WheelInfo,
) -> Option<String> {
    let name = name?;
    let mut package_url = PackageUrl::new(PythonParser::PACKAGE_TYPE.as_str(), name).ok()?;

    if let Some(ver) = version {
        package_url.with_version(ver).ok()?;
    }

    let extension = format!(
        "{}-{}-{}",
        wheel_info.python_tag, wheel_info.abi_tag, wheel_info.platform_tag
    );
    package_url.add_qualifier("extension", extension).ok()?;

    Some(package_url.to_string())
}

fn build_egg_purl(name: Option<&str>, version: Option<&str>) -> Option<String> {
    let name = name?;
    let mut package_url = PackageUrl::new(PythonParser::PACKAGE_TYPE.as_str(), name).ok()?;

    if let Some(ver) = version {
        package_url.with_version(ver).ok()?;
    }

    package_url.add_qualifier("type", "egg").ok()?;

    Some(package_url.to_string())
}

pub(super) fn is_pip_cache_origin_json(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some("origin.json")
        && path.ancestors().skip(1).any(|ancestor| {
            ancestor
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("wheels"))
        })
}

pub(super) fn extract_from_pip_origin_json(path: &Path) -> PackageData {
    let content = match read_file_to_string(path, None) {
        Ok(content) => content,
        Err(e) => {
            warn!("Failed to read pip cache origin.json at {:?}: {}", path, e);
            return default_package_data(path);
        }
    };

    let root: JsonValue = match serde_json::from_str(&content) {
        Ok(root) => root,
        Err(e) => {
            warn!("Failed to parse pip cache origin.json at {:?}: {}", path, e);
            return default_package_data(path);
        }
    };

    let Some(download_url) = root.get("url").and_then(|value| value.as_str()) else {
        warn!("No url found in pip cache origin.json at {:?}", path);
        return default_package_data(path);
    };

    let sibling_wheel = find_sibling_cached_wheel(path);
    let name_version = parse_name_version_from_origin_url(download_url).or_else(|| {
        sibling_wheel
            .as_ref()
            .map(|wheel_info| (wheel_info.name.clone(), wheel_info.version.clone()))
    });

    let Some((name, version)) = name_version else {
        warn!(
            "Failed to infer package name/version from pip cache origin.json at {:?}",
            path
        );
        return default_package_data(path);
    };

    let (repository_homepage_url, repository_download_url, api_data_url, plain_purl) =
        build_pypi_urls(Some(&name), Some(&version));
    let purl = sibling_wheel
        .as_ref()
        .and_then(|wheel_info| build_wheel_purl(Some(&name), Some(&version), wheel_info))
        .or(plain_purl);

    PackageData {
        package_type: Some(PythonParser::PACKAGE_TYPE),
        primary_language: Some("Python".to_string()),
        name: Some(truncate_field(name)),
        version: Some(version),
        datasource_id: Some(DatasourceId::PypiPipOriginJson),
        download_url: Some(truncate_field(download_url.to_string())),
        sha256: extract_sha256_from_origin_json(&root)
            .and_then(|h| Sha256Digest::from_hex(&h).ok()),
        repository_homepage_url,
        repository_download_url,
        api_data_url,
        purl,
        ..Default::default()
    }
}

fn find_sibling_cached_wheel(path: &Path) -> Option<WheelInfo> {
    let parent = path.parent()?;
    let entries = parent.read_dir().ok()?;

    for entry in entries.flatten() {
        let sibling_path = entry.path();
        if sibling_path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("whl"))
            && let Some(wheel_info) = parse_wheel_filename(&sibling_path)
        {
            return Some(wheel_info);
        }
    }

    None
}

fn parse_name_version_from_origin_url(url: &str) -> Option<(String, String)> {
    let file_name = url.rsplit('/').next()?;

    if file_name.ends_with(".whl") {
        return parse_wheel_filename(Path::new(file_name))
            .map(|wheel_info| (wheel_info.name, wheel_info.version));
    }

    let stem = strip_python_archive_extension(file_name)?;
    let (name, version) = stem.rsplit_once('-')?;
    if name.is_empty() || version.is_empty() {
        return None;
    }

    Some((name.replace('_', "-"), version.to_string()))
}

fn extract_sha256_from_origin_json(root: &JsonValue) -> Option<String> {
    root.pointer("/archive_info/hashes/sha256")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
        .or_else(|| {
            root.pointer("/archive_info/hash")
                .and_then(|value| value.as_str())
                .and_then(normalize_origin_hash)
        })
}

fn normalize_origin_hash(hash: &str) -> Option<String> {
    if let Some(value) = hash.strip_prefix("sha256=") {
        return Some(value.to_string());
    }
    if let Some(value) = hash.strip_prefix("sha256:") {
        return Some(value.to_string());
    }
    if hash.len() == 64 && hash.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Some(hash.to_string());
    }
    None
}
