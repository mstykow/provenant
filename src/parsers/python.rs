//! Parser for Python package manifests and metadata files.
//!
//! Comprehensive parser supporting multiple Python packaging formats including
//! modern (pyproject.toml) and legacy (setup.py, setup.cfg) standards.
//!
//! # Supported Formats
//! - pyproject.toml (PEP 621)
//! - setup.py (AST parsing, no code execution)
//! - setup.cfg (INI format)
//! - PKG-INFO / METADATA (RFC 822 format)
//! - .whl archives (wheel format)
//! - .egg archives (legacy egg format)
//! - requirements.txt
//!
//! # Key Features
//! - Archive safety checks (size limits, compression ratio validation)
//! - AST-based setup.py parsing (no code execution)
//! - RFC 822 metadata parsing for wheels/eggs
//! - Dependency extraction with PEP 508 markers
//! - Party information (authors, maintainers)
//!
//! # Security Features
//! - Archive size limit: 100MB
//! - Per-file size limit: 50MB
//! - Compression ratio limit: 100:1
//! - Total extracted size tracking
//! - No code execution from setup.py or .egg files
//!
//! # Implementation Notes
//! - Uses multiple parsers for different formats
//! - Direct dependencies: all manifest dependencies are direct
//! - Graceful fallback on parse errors with warning logs

use crate::models::{
    DatasourceId, Dependency, FileReference, PackageData, PackageType, Party, Sha256Digest,
};
use crate::parser_warn as warn;
use crate::parsers::utils::{read_file_to_string, split_name_email};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use bzip2::read::BzDecoder;
use csv::ReaderBuilder;
use flate2::read::GzDecoder;
use liblzma::read::XzDecoder;
use packageurl::PackageUrl;
use regex::Regex;
use ruff_python_ast as ast;
use ruff_python_parser::parse_module;
use serde_json::{Map as JsonMap, Value as JsonValue};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use tar::Archive;
use toml::Value as TomlValue;
use toml::map::Map as TomlMap;
use zip::ZipArchive;

use super::PackageParser;
use super::license_normalization::{
    DeclaredLicenseMatchMetadata, build_declared_license_data, normalize_spdx_declared_license,
    normalize_spdx_expression,
};
use super::pep508::parse_pep508_requirement;

// Field constants for pyproject.toml
const FIELD_PROJECT: &str = "project";
const FIELD_NAME: &str = "name";
const FIELD_VERSION: &str = "version";
const FIELD_LICENSE: &str = "license";
const FIELD_AUTHORS: &str = "authors";
const FIELD_MAINTAINERS: &str = "maintainers";
const FIELD_URLS: &str = "urls";
const FIELD_HOMEPAGE: &str = "homepage";
const FIELD_REPOSITORY: &str = "repository";
const FIELD_DEPENDENCIES: &str = "dependencies";
const FIELD_OPTIONAL_DEPENDENCIES: &str = "optional-dependencies";
const FIELD_DEPENDENCY_GROUPS: &str = "dependency-groups";
const FIELD_DEV_DEPENDENCIES: &str = "dev-dependencies";
const MAX_SETUP_PY_BYTES: usize = 1_048_576;
const MAX_SETUP_PY_AST_NODES: usize = 10_000;
const MAX_SETUP_PY_AST_DEPTH: usize = 50;
const MAX_ARCHIVE_SIZE: u64 = 100 * 1024 * 1024; // 100MB uncompressed
const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024; // 50MB per file
const MAX_COMPRESSION_RATIO: f64 = 100.0; // 100:1 ratio

/// Python package parser supporting 11 manifest formats.
///
/// Extracts metadata from Python package files including pyproject.toml, setup.py,
/// setup.cfg, PKG-INFO, METADATA, pip-inspect lockfiles, and .whl/.egg archives.
///
/// # Security
///
/// setup.py files are parsed using AST analysis rather than code execution to prevent
/// arbitrary code execution during scanning. See `extract_from_setup_py_ast` for details.
pub struct PythonParser;

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

impl PackageParser for PythonParser {
    const PACKAGE_TYPE: PackageType = PackageType::Pypi;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        vec![
            if path.file_name().unwrap_or_default() == "pyproject.toml" {
                extract_from_pyproject_toml(path)
            } else if path.file_name().unwrap_or_default() == "setup.cfg" {
                extract_from_setup_cfg(path)
            } else if is_setup_py_like_path(path) {
                return extract_setup_py_packages(path);
            } else if path.file_name().unwrap_or_default() == "PKG-INFO" {
                extract_from_rfc822_metadata(path, detect_pkg_info_datasource_id(path))
            } else if is_installed_wheel_metadata_path(path) {
                extract_from_rfc822_metadata(path, DatasourceId::PypiWheelMetadata)
            } else if is_pip_cache_origin_json(path) {
                extract_from_pip_origin_json(path)
            } else if path.file_name().unwrap_or_default() == "pypi.json" {
                extract_from_pypi_json(path)
            } else if path.file_name().unwrap_or_default() == "pip-inspect.deplock" {
                extract_from_pip_inspect(path)
            } else if is_python_sdist_archive_path(path) {
                extract_from_sdist_archive(path)
            } else if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("whl"))
            {
                extract_from_wheel_archive(path)
            } else if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("egg"))
            {
                extract_from_egg_archive(path)
            } else {
                default_package_data(path)
            },
        ]
    }

    fn is_match(path: &Path) -> bool {
        if let Some(filename) = path.file_name()
            && (filename == "pyproject.toml"
                || filename == "setup.cfg"
                || is_setup_py_like_path(path)
                || filename == "PKG-INFO"
                || (filename == "METADATA" && is_installed_wheel_metadata_path(path))
                || filename == "pypi.json"
                || filename == "pip-inspect.deplock"
                || is_pip_cache_origin_json(path))
        {
            return true;
        }

        if let Some(extension) = path.extension() {
            let ext = extension.to_string_lossy().to_lowercase();
            if ext == "whl" || ext == "egg" || is_python_sdist_archive_path(path) {
                return true;
            }
        }

        false
    }
}

fn is_setup_py_like_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "setup.py" || name.ends_with("_setup.py"))
}

fn is_installed_wheel_metadata_path(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some("METADATA")
        && path
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".dist-info"))
}

#[derive(Debug, Clone)]
struct InstalledWheelMetadata {
    wheel_tags: Vec<String>,
    wheel_version: Option<String>,
    wheel_generator: Option<String>,
    root_is_purelib: Option<bool>,
    compressed_tag: Option<String>,
}

fn merge_sibling_wheel_metadata(path: &Path, package_data: &mut PackageData) {
    let Some(parent) = path.parent() else {
        return;
    };

    if !parent
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".dist-info"))
    {
        return;
    }

    let wheel_path = parent.join("WHEEL");
    if !wheel_path.exists() {
        return;
    }

    let Ok(content) = read_file_to_string(&wheel_path) else {
        warn!("Failed to read sibling WHEEL file at {:?}", wheel_path);
        return;
    };

    let Some(wheel_metadata) = parse_installed_wheel_metadata(&content) else {
        return;
    };

    apply_installed_wheel_metadata(package_data, &wheel_metadata);
}

fn parse_installed_wheel_metadata(content: &str) -> Option<InstalledWheelMetadata> {
    use super::rfc822::{get_header_all, get_header_first};

    let metadata = super::rfc822::parse_rfc822_content(content);
    let wheel_tags = get_header_all(&metadata.headers, "tag");
    if wheel_tags.is_empty() {
        return None;
    }

    let wheel_version = get_header_first(&metadata.headers, "wheel-version");
    let wheel_generator = get_header_first(&metadata.headers, "generator");
    let root_is_purelib =
        get_header_first(&metadata.headers, "root-is-purelib").and_then(|value| {
            match value.to_ascii_lowercase().as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            }
        });

    let compressed_tag = compress_wheel_tags(&wheel_tags);

    Some(InstalledWheelMetadata {
        wheel_tags,
        wheel_version,
        wheel_generator,
        root_is_purelib,
        compressed_tag,
    })
}

fn compress_wheel_tags(tags: &[String]) -> Option<String> {
    if tags.is_empty() {
        return None;
    }

    if tags.len() == 1 {
        return Some(tags[0].clone());
    }

    let mut python_tags = Vec::new();
    let mut abi_tag: Option<&str> = None;
    let mut platform_tag: Option<&str> = None;

    for tag in tags {
        let mut parts = tag.splitn(3, '-');
        let python = parts.next()?;
        let abi = parts.next()?;
        let platform = parts.next()?;

        if abi_tag.is_some_and(|existing| existing != abi)
            || platform_tag.is_some_and(|existing| existing != platform)
        {
            return None;
        }

        abi_tag = Some(abi);
        platform_tag = Some(platform);
        python_tags.push(python.to_string());
    }

    Some(format!(
        "{}-{}-{}",
        python_tags.join("."),
        abi_tag?,
        platform_tag?
    ))
}

fn apply_installed_wheel_metadata(
    package_data: &mut PackageData,
    wheel_metadata: &InstalledWheelMetadata,
) {
    let extra_data = package_data.extra_data.get_or_insert_with(HashMap::new);
    extra_data.insert(
        "wheel_tags".to_string(),
        JsonValue::Array(
            wheel_metadata
                .wheel_tags
                .iter()
                .cloned()
                .map(JsonValue::String)
                .collect(),
        ),
    );

    if let Some(wheel_version) = &wheel_metadata.wheel_version {
        extra_data.insert(
            "wheel_version".to_string(),
            JsonValue::String(wheel_version.clone()),
        );
    }

    if let Some(wheel_generator) = &wheel_metadata.wheel_generator {
        extra_data.insert(
            "wheel_generator".to_string(),
            JsonValue::String(wheel_generator.clone()),
        );
    }

    if let Some(root_is_purelib) = wheel_metadata.root_is_purelib {
        extra_data.insert(
            "root_is_purelib".to_string(),
            JsonValue::Bool(root_is_purelib),
        );
    }

    if let (Some(name), Some(version), Some(extension)) = (
        package_data.name.as_deref(),
        package_data.version.as_deref(),
        wheel_metadata.compressed_tag.as_deref(),
    ) {
        package_data.purl = build_pypi_purl_with_extension(name, Some(version), extension);
    }
}

fn is_pip_cache_origin_json(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some("origin.json")
        && path.ancestors().skip(1).any(|ancestor| {
            ancestor
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("wheels"))
        })
}

fn extract_from_pip_origin_json(path: &Path) -> PackageData {
    let content = match read_file_to_string(path) {
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
        name: Some(name),
        version: Some(version),
        datasource_id: Some(DatasourceId::PypiPipOriginJson),
        download_url: Some(download_url.to_string()),
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

fn strip_python_archive_extension(file_name: &str) -> Option<&str> {
    [".tar.gz", ".tar.bz2", ".tar.xz", ".tgz", ".zip", ".whl"]
        .iter()
        .find_map(|suffix| file_name.strip_suffix(suffix))
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

fn extract_from_rfc822_metadata(path: &Path, datasource_id: DatasourceId) -> PackageData {
    let content = match read_file_to_string(path) {
        Ok(content) => content,
        Err(e) => {
            warn!("Failed to read metadata at {:?}: {}", path, e);
            return default_package_data(path);
        }
    };

    let metadata = super::rfc822::parse_rfc822_content(&content);
    let mut package_data = build_package_data_from_rfc822(&metadata, datasource_id);
    merge_sibling_metadata_dependencies(path, &mut package_data);
    merge_sibling_metadata_file_references(path, &mut package_data);
    if datasource_id == DatasourceId::PypiWheelMetadata {
        merge_sibling_wheel_metadata(path, &mut package_data);
    }
    package_data
}

fn merge_sibling_metadata_dependencies(path: &Path, package_data: &mut PackageData) {
    let mut extra_dependencies = Vec::new();

    if let Some(parent) = path.parent() {
        let direct_requires = parent.join("requires.txt");
        if direct_requires.exists()
            && let Ok(content) = read_file_to_string(&direct_requires)
        {
            extra_dependencies.extend(parse_requires_txt(&content));
        }

        let sibling_egg_info_requires = parent
            .read_dir()
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .find_map(|entry| {
                let child_path = entry.path();
                if child_path.is_dir()
                    && child_path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.ends_with(".egg-info"))
                {
                    let requires = child_path.join("requires.txt");
                    requires.exists().then_some(requires)
                } else {
                    None
                }
            });

        if let Some(requires_path) = sibling_egg_info_requires
            && let Ok(content) = read_file_to_string(&requires_path)
        {
            extra_dependencies.extend(parse_requires_txt(&content));
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

fn merge_sibling_metadata_file_references(path: &Path, package_data: &mut PackageData) {
    let mut extra_refs = Vec::new();

    if let Some(parent) = path.parent() {
        let record_path = parent.join("RECORD");
        if record_path.exists()
            && let Ok(content) = read_file_to_string(&record_path)
        {
            extra_refs.extend(parse_record_csv(&content));
        }

        let installed_files_path = parent.join("installed-files.txt");
        if installed_files_path.exists()
            && let Ok(content) = read_file_to_string(&installed_files_path)
        {
            extra_refs.extend(parse_installed_files_txt(&content));
        }

        let sources_path = parent.join("SOURCES.txt");
        if sources_path.exists()
            && let Ok(content) = read_file_to_string(&sources_path)
        {
            extra_refs.extend(parse_sources_txt(&content));
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

fn collect_validated_zip_entries<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    path: &Path,
    archive_type: &str,
) -> Result<Vec<ValidatedZipEntry>, String> {
    let mut total_extracted = 0u64;
    let mut entries = Vec::new();

    for i in 0..archive.len() {
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

fn is_python_sdist_archive_path(path: &Path) -> bool {
    detect_python_sdist_archive_format(path).is_some()
}

fn detect_python_sdist_archive_format(path: &Path) -> Option<PythonSdistArchiveFormat> {
    let file_name = path.file_name()?.to_str()?.to_ascii_lowercase();

    if !is_likely_python_sdist_filename(&file_name) {
        return None;
    }

    if file_name.ends_with(".tar.gz") {
        Some(PythonSdistArchiveFormat::TarGz)
    } else if file_name.ends_with(".tgz") {
        tgz_sdist_contains_pkg_info(path).then_some(PythonSdistArchiveFormat::Tgz)
    } else if file_name.ends_with(".tar.bz2") {
        Some(PythonSdistArchiveFormat::TarBz2)
    } else if file_name.ends_with(".tar.xz") {
        Some(PythonSdistArchiveFormat::TarXz)
    } else if file_name.ends_with(".zip") {
        zip_sdist_contains_pkg_info(path).then_some(PythonSdistArchiveFormat::Zip)
    } else {
        None
    }
}

fn tgz_sdist_contains_pkg_info(path: &Path) -> bool {
    if !path.is_file() {
        return true;
    }

    let compressed_size = match std::fs::metadata(path) {
        Ok(metadata) => metadata.len(),
        Err(_) => return false,
    };
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let decoder = GzDecoder::new(file);
    let Some(entries) = collect_tar_sdist_entries(path, decoder, "tgz", compressed_size) else {
        return false;
    };

    select_sdist_pkginfo_entry(path, &entries).is_some()
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

fn is_likely_python_sdist_filename(file_name: &str) -> bool {
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

fn extract_from_sdist_archive(path: &Path) -> PackageData {
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

    for entry_result in archive_entries {
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

    let mut package_data =
        python_parse_rfc822_content(&metadata_content, DatasourceId::PypiSdistPkginfo);
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
    let metadata = super::rfc822::parse_rfc822_content(content);
    super::rfc822::get_header_first(&metadata.headers, "name")
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

fn normalize_python_package_name(name: &str) -> String {
    name.to_ascii_lowercase().replace('_', "-")
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

fn extract_from_wheel_archive(path: &Path) -> PackageData {
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

    let mut package_data = python_parse_rfc822_content(&content, DatasourceId::PypiWheel);

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

fn extract_from_egg_archive(path: &Path) -> PackageData {
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

    let mut package_data = python_parse_rfc822_content(&content, DatasourceId::PypiEgg);

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

    String::from_utf8(bytes).map_err(|e| format!("{} is not valid UTF-8: {}", context, e))
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

/// Parses RECORD CSV format from wheel archives (PEP 427).
/// Format: path,hash,size (3 columns, no header)
/// Hash format: sha256=urlsafe_base64_hash or empty
/// Size: bytes as u64 or empty
pub fn parse_record_csv(content: &str) -> Vec<FileReference> {
    let mut reader = ReaderBuilder::new()
        .has_headers(false)
        .from_reader(content.as_bytes());

    let mut file_references = Vec::new();

    for result in reader.records() {
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

                // Parse hash: format is "algorithm=value"
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

                // Parse size
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

/// Parses installed-files.txt format from egg archives (PEP 376).
/// Format: one file path per line, no headers, no hash, no size
pub fn parse_installed_files_txt(content: &str) -> Vec<FileReference> {
    content
        .lines()
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

pub fn parse_sources_txt(content: &str) -> Vec<FileReference> {
    content
        .lines()
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

struct WheelInfo {
    name: String,
    version: String,
    python_tag: String,
    abi_tag: String,
    platform_tag: String,
}

fn parse_wheel_filename(path: &Path) -> Option<WheelInfo> {
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

fn build_wheel_purl(
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

fn python_parse_rfc822_content(content: &str, datasource_id: DatasourceId) -> PackageData {
    let metadata = super::rfc822::parse_rfc822_content(content);
    build_package_data_from_rfc822(&metadata, datasource_id)
}

/// Builds PackageData from parsed RFC822 metadata.
///
/// This is the shared implementation for both `extract_from_rfc822_metadata` (file-based)
/// and `python_parse_rfc822_content` (content-based) functions.
fn build_package_data_from_rfc822(
    metadata: &super::rfc822::Rfc822Metadata,
    datasource_id: DatasourceId,
) -> PackageData {
    use super::rfc822::{get_header_all, get_header_first};

    let name = get_header_first(&metadata.headers, "name");
    let version = get_header_first(&metadata.headers, "version");
    let summary = get_header_first(&metadata.headers, "summary");
    let mut homepage_url = get_header_first(&metadata.headers, "home-page");
    let author = get_header_first(&metadata.headers, "author");
    let author_email = get_header_first(&metadata.headers, "author-email");
    let license = get_header_first(&metadata.headers, "license");
    let license_expression = get_header_first(&metadata.headers, "license-expression");
    let download_url = get_header_first(&metadata.headers, "download-url");
    let platform = get_header_first(&metadata.headers, "platform");
    let requires_python = get_header_first(&metadata.headers, "requires-python");
    let classifiers = get_header_all(&metadata.headers, "classifier");
    let license_files = get_header_all(&metadata.headers, "license-file");

    let description_body = if metadata.body.is_empty() {
        get_header_first(&metadata.headers, "description").unwrap_or_default()
    } else {
        metadata.body.clone()
    };

    let description = build_description(summary.as_deref(), &description_body);

    let mut parties = Vec::new();
    if author.is_some() || author_email.is_some() {
        parties.push(Party {
            r#type: Some("person".to_string()),
            role: Some("author".to_string()),
            name: author,
            email: author_email,
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }

    let (keywords, license_classifiers) = split_classifiers(&classifiers);
    let referenced_license_files: Vec<&str> = license_files.iter().map(String::as_str).collect();
    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        license_expression
            .as_deref()
            .and_then(normalize_spdx_expression)
            .map(|normalized| {
                build_declared_license_data(
                    normalized,
                    DeclaredLicenseMatchMetadata::single_line(
                        license_expression.as_deref().unwrap_or_default(),
                    )
                    .with_referenced_filenames(&referenced_license_files),
                )
            })
            .unwrap_or_else(|| normalize_spdx_declared_license(license_expression.as_deref()));

    let extracted_license_statement = license_expression
        .clone()
        .or_else(|| build_extracted_license_statement(license.as_deref(), &license_classifiers));

    let mut extra_data = HashMap::new();
    if let Some(platform_value) = platform
        && !platform_value.eq_ignore_ascii_case("unknown")
        && !platform_value.is_empty()
    {
        extra_data.insert(
            "platform".to_string(),
            serde_json::Value::String(platform_value),
        );
    }

    if let Some(requires_python_value) = requires_python
        && !requires_python_value.is_empty()
    {
        extra_data.insert(
            "requires_python".to_string(),
            serde_json::Value::String(requires_python_value),
        );
    }

    if !license_files.is_empty() {
        extra_data.insert(
            "license_files".to_string(),
            serde_json::Value::Array(
                license_files
                    .iter()
                    .cloned()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
    }

    let file_references = license_files
        .iter()
        .map(|path| FileReference {
            path: path.clone(),
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            extra_data: None,
        })
        .collect();

    let project_urls = get_header_all(&metadata.headers, "project-url");
    let dependencies = extract_rfc822_dependencies(&metadata.headers);
    let (mut bug_tracking_url, mut code_view_url, mut vcs_url) = (None, None, None);

    if !project_urls.is_empty() {
        let parsed_urls = parse_project_urls(&project_urls);

        for (label, url) in &parsed_urls {
            let label_lower = label.to_lowercase();

            if bug_tracking_url.is_none()
                && matches!(
                    label_lower.as_str(),
                    "tracker"
                        | "bug reports"
                        | "bug tracker"
                        | "issues"
                        | "issue tracker"
                        | "github: issues"
                )
            {
                bug_tracking_url = Some(url.clone());
            } else if code_view_url.is_none()
                && matches!(label_lower.as_str(), "source" | "source code" | "code")
            {
                code_view_url = Some(url.clone());
            } else if vcs_url.is_none()
                && matches!(
                    label_lower.as_str(),
                    "github" | "gitlab" | "github: repo" | "repository"
                )
            {
                vcs_url = Some(url.clone());
            } else if homepage_url.is_none()
                && matches!(label_lower.as_str(), "website" | "homepage" | "home")
            {
                homepage_url = Some(url.clone());
            } else if label_lower == "changelog" {
                extra_data.insert(
                    "changelog_url".to_string(),
                    serde_json::Value::String(url.clone()),
                );
            }
        }

        let project_urls_json: serde_json::Map<String, serde_json::Value> = parsed_urls
            .iter()
            .map(|(label, url)| (label.clone(), serde_json::Value::String(url.clone())))
            .collect();

        if !project_urls_json.is_empty() {
            extra_data.insert(
                "project_urls".to_string(),
                serde_json::Value::Object(project_urls_json),
            );
        }
    }

    let extra_data = if extra_data.is_empty() {
        None
    } else {
        Some(extra_data)
    };

    let (repository_homepage_url, repository_download_url, api_data_url, purl) =
        build_pypi_urls(name.as_deref(), version.as_deref());

    PackageData {
        package_type: Some(PythonParser::PACKAGE_TYPE),
        namespace: None,
        name,
        version,
        qualifiers: None,
        subpath: None,
        primary_language: Some("Python".to_string()),
        description,
        release_date: None,
        parties,
        keywords,
        homepage_url,
        download_url,
        size: None,
        sha1: None,
        md5: None,
        sha256: None,
        sha512: None,
        bug_tracking_url,
        code_view_url,
        vcs_url,
        copyright: None,
        holder: None,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        other_license_expression: None,
        other_license_expression_spdx: None,
        other_license_detections: Vec::new(),
        extracted_license_statement,
        notice_text: None,
        source_packages: Vec::new(),
        file_references,
        is_private: false,
        is_virtual: false,
        extra_data,
        dependencies,
        repository_homepage_url,
        repository_download_url,
        api_data_url,
        datasource_id: Some(datasource_id),
        purl,
    }
}

fn parse_project_urls(project_urls: &[String]) -> Vec<(String, String)> {
    project_urls
        .iter()
        .filter_map(|url_entry| {
            if let Some((label, url)) = url_entry.split_once(", ") {
                let label_trimmed = label.trim();
                let url_trimmed = url.trim();
                if !label_trimmed.is_empty() && !url_trimmed.is_empty() {
                    return Some((label_trimmed.to_string(), url_trimmed.to_string()));
                }
            }
            None
        })
        .collect()
}

fn build_description(summary: Option<&str>, body: &str) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(summary_value) = summary
        && !summary_value.trim().is_empty()
    {
        parts.push(summary_value.trim().to_string());
    }

    if !body.trim().is_empty() {
        parts.push(body.trim().to_string());
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

fn split_classifiers(classifiers: &[String]) -> (Vec<String>, Vec<String>) {
    let mut keywords = Vec::new();
    let mut license_classifiers = Vec::new();

    for classifier in classifiers {
        if classifier.starts_with("License ::") {
            license_classifiers.push(classifier.to_string());
        } else {
            keywords.push(classifier.to_string());
        }
    }

    (keywords, license_classifiers)
}

fn build_extracted_license_statement(
    license: Option<&str>,
    license_classifiers: &[String],
) -> Option<String> {
    let mut lines = Vec::new();

    if let Some(value) = license
        && !value.trim().is_empty()
    {
        lines.push(format!("license: {}", value.trim()));
    }

    if !license_classifiers.is_empty() {
        lines.push("classifiers:".to_string());
        for classifier in license_classifiers {
            lines.push(format!("  - '{}'", classifier));
        }
    }

    if lines.is_empty() {
        None
    } else {
        Some(format!("{}\n", lines.join("\n")))
    }
}

pub(crate) fn build_pypi_urls(
    name: Option<&str>,
    version: Option<&str>,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    let repository_homepage_url = name.map(|value| format!("https://pypi.org/project/{}", value));

    let repository_download_url = name.and_then(|value| {
        version.map(|ver| {
            format!(
                "https://pypi.org/packages/source/{}/{}/{}-{}.tar.gz",
                &value[..1.min(value.len())],
                value,
                value,
                ver
            )
        })
    });

    let api_data_url = name.map(|value| {
        if let Some(ver) = version {
            format!("https://pypi.org/pypi/{}/{}/json", value, ver)
        } else {
            format!("https://pypi.org/pypi/{}/json", value)
        }
    });

    let purl = name.and_then(|value| {
        let mut package_url = PackageUrl::new(PythonParser::PACKAGE_TYPE.as_str(), value).ok()?;
        if let Some(ver) = version {
            package_url.with_version(ver).ok()?;
        }
        Some(package_url.to_string())
    });

    (
        repository_homepage_url,
        repository_download_url,
        api_data_url,
        purl,
    )
}

fn build_pypi_purl_with_extension(
    name: &str,
    version: Option<&str>,
    extension: &str,
) -> Option<String> {
    let mut package_url = PackageUrl::new(PythonParser::PACKAGE_TYPE.as_str(), name).ok()?;
    if let Some(ver) = version {
        package_url.with_version(ver).ok()?;
    }
    package_url.add_qualifier("extension", extension).ok()?;
    Some(package_url.to_string())
}

fn extract_from_pyproject_toml(path: &Path) -> PackageData {
    let toml_content = match read_toml_file(path) {
        Ok(content) => content,
        Err(e) => {
            warn!(
                "Failed to read or parse pyproject.toml at {:?}: {}",
                path, e
            );
            return default_package_data(path);
        }
    };

    let tool_table = toml_content.get("tool").and_then(|v| v.as_table());
    let is_poetry_pyproject = tool_table
        .and_then(|tool| tool.get("poetry"))
        .and_then(|value| value.as_table())
        .is_some();

    // Handle both PEP 621 (project table) and poetry formats
    let project_table =
        if let Some(project) = toml_content.get(FIELD_PROJECT).and_then(|v| v.as_table()) {
            // Standard PEP 621 format with [project] table
            project.clone()
        } else if let Some(tool) = tool_table {
            if let Some(poetry) = tool.get("poetry").and_then(|v| v.as_table()) {
                // Poetry format with [tool.poetry] table
                poetry.clone()
            } else {
                return default_package_data(path);
            }
        } else if toml_content.get(FIELD_NAME).is_some() {
            // Other format with top-level fields
            match toml_content.as_table() {
                Some(table) => table.clone(),
                None => {
                    warn!("Failed to convert TOML content to table in {:?}", path);
                    return default_package_data(path);
                }
            }
        } else {
            return default_package_data(path);
        };

    let name = project_table
        .get(FIELD_NAME)
        .and_then(|v| v.as_str())
        .map(String::from);

    let version = project_table
        .get(FIELD_VERSION)
        .and_then(|v| v.as_str())
        .map(String::from);
    let classifiers = project_table
        .get("classifiers")
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let extracted_license_statement = extract_raw_license_string(&project_table);
    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        normalize_spdx_declared_license(extract_license_expression_candidate(&project_table));

    // URLs can be in different formats depending on the tool (poetry, flit, etc.)
    let (homepage_url, repository_url) = extract_urls(&project_table);

    let (dependencies, optional_dependencies) = extract_dependencies(&project_table, &toml_content);
    let extra_data = extract_pyproject_extra_data(&toml_content);

    // Create package URL
    let purl = name.as_ref().and_then(|n| {
        let mut package_url = match PackageUrl::new(PythonParser::PACKAGE_TYPE.as_str(), n) {
            Ok(p) => p,
            Err(e) => {
                warn!(
                    "Failed to create PackageUrl for Python package '{}': {}",
                    n, e
                );
                return None;
            }
        };

        if let Some(v) = &version
            && let Err(e) = package_url.with_version(v)
        {
            warn!(
                "Failed to set version '{}' for Python package '{}': {}",
                v, n, e
            );
            return None;
        }

        Some(package_url.to_string())
    });

    let api_data_url = name.as_ref().map(|n| {
        if let Some(v) = &version {
            format!("https://pypi.org/pypi/{}/{}/json", n, v)
        } else {
            format!("https://pypi.org/pypi/{}/json", n)
        }
    });

    let pypi_homepage_url = name
        .as_ref()
        .map(|n| format!("https://pypi.org/project/{}", n));

    let pypi_download_url = name.as_ref().and_then(|n| {
        version.as_ref().map(|v| {
            format!(
                "https://pypi.org/packages/source/{}/{}/{}-{}.tar.gz",
                &n[..1.min(n.len())],
                n,
                n,
                v
            )
        })
    });

    PackageData {
        package_type: Some(PythonParser::PACKAGE_TYPE),
        namespace: None,
        name,
        version,
        qualifiers: None,
        subpath: None,
        primary_language: None,
        description: None,
        release_date: None,
        parties: extract_parties(&project_table),
        keywords: Vec::new(),
        homepage_url: homepage_url.or(pypi_homepage_url),
        download_url: repository_url.clone().or(pypi_download_url),
        size: None,
        sha1: None,
        md5: None,
        sha256: None,
        sha512: None,
        bug_tracking_url: None,
        code_view_url: None,
        vcs_url: repository_url,
        copyright: None,
        holder: None,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        other_license_expression: None,
        other_license_expression_spdx: None,
        other_license_detections: Vec::new(),
        extracted_license_statement,
        notice_text: None,
        source_packages: Vec::new(),
        file_references: Vec::new(),
        is_private: has_private_classifier(&classifiers),
        is_virtual: false,
        extra_data,
        dependencies: [dependencies, optional_dependencies].concat(),
        repository_homepage_url: None,
        repository_download_url: None,
        api_data_url,
        datasource_id: Some(if is_poetry_pyproject {
            DatasourceId::PypiPoetryPyprojectToml
        } else {
            DatasourceId::PypiPyprojectToml
        }),
        purl,
    }
}

fn detect_pkg_info_datasource_id(path: &Path) -> DatasourceId {
    let path_str = path.to_string_lossy().replace('\\', "/");
    if path_str.contains("/EGG-INFO/PKG-INFO") {
        DatasourceId::PypiEggPkginfo
    } else if path_str.ends_with(".egg-info/PKG-INFO") {
        DatasourceId::PypiEditableEggPkginfo
    } else {
        DatasourceId::PypiSdistPkginfo
    }
}

fn extract_raw_license_string(project: &TomlMap<String, TomlValue>) -> Option<String> {
    project
        .get(FIELD_LICENSE)
        .and_then(|license_value| match license_value {
            TomlValue::String(license_str) => Some(license_str.clone()),
            TomlValue::Table(license_table) => license_table
                .get("text")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| {
                    license_table
                        .get("expression")
                        .and_then(|v| v.as_str())
                        .map(|expr| expr.to_string())
                }),
            _ => None,
        })
}

fn extract_license_expression_candidate(project: &TomlMap<String, TomlValue>) -> Option<&str> {
    match project.get(FIELD_LICENSE) {
        Some(TomlValue::String(license_str)) => Some(license_str.as_str()),
        Some(TomlValue::Table(license_table)) => license_table
            .get("expression")
            .and_then(|value| value.as_str()),
        _ => None,
    }
}

fn extract_urls(project: &TomlMap<String, TomlValue>) -> (Option<String>, Option<String>) {
    let mut homepage_url = None;
    let mut repository_url = None;

    // Check for URLs table
    if let Some(urls) = project.get(FIELD_URLS).and_then(|v| v.as_table()) {
        homepage_url = urls
            .get(FIELD_HOMEPAGE)
            .and_then(|v| v.as_str())
            .map(String::from);
        repository_url = urls
            .get(FIELD_REPOSITORY)
            .and_then(|v| v.as_str())
            .map(String::from);
    }

    // If not found in URLs table, check for top-level keys
    if homepage_url.is_none() {
        homepage_url = project
            .get(FIELD_HOMEPAGE)
            .and_then(|v| v.as_str())
            .map(String::from);
    }

    if repository_url.is_none() {
        repository_url = project
            .get(FIELD_REPOSITORY)
            .and_then(|v| v.as_str())
            .map(String::from);
    }

    (homepage_url, repository_url)
}

fn extract_parties(project: &TomlMap<String, TomlValue>) -> Vec<Party> {
    let mut parties = Vec::new();

    if let Some(authors) = project.get(FIELD_AUTHORS).and_then(|v| v.as_array()) {
        for author in authors {
            if let Some(author_str) = author.as_str() {
                let (name, email) = split_name_email(author_str);
                parties.push(Party {
                    r#type: None,
                    role: Some("author".to_string()),
                    name,
                    email,
                    url: None,
                    organization: None,
                    organization_url: None,
                    timezone: None,
                });
            }
        }
    }

    if let Some(maintainers) = project.get(FIELD_MAINTAINERS).and_then(|v| v.as_array()) {
        for maintainer in maintainers {
            if let Some(maintainer_str) = maintainer.as_str() {
                let (name, email) = split_name_email(maintainer_str);
                parties.push(Party {
                    r#type: None,
                    role: Some("maintainer".to_string()),
                    name,
                    email,
                    url: None,
                    organization: None,
                    organization_url: None,
                    timezone: None,
                });
            }
        }
    }

    parties
}

fn extract_dependencies(
    project: &TomlMap<String, TomlValue>,
    toml_content: &TomlValue,
) -> (Vec<Dependency>, Vec<Dependency>) {
    let mut dependencies = Vec::new();
    let mut optional_dependencies = Vec::new();

    // Handle dependencies - can be array or table format
    if let Some(deps_value) = project.get(FIELD_DEPENDENCIES) {
        match deps_value {
            TomlValue::Array(arr) => {
                dependencies = parse_dependency_array(arr, false, None);
            }
            TomlValue::Table(table) => {
                dependencies = parse_dependency_table(table, false, None);
            }
            _ => {}
        }
    }

    // Handle PEP 621 optional-dependencies with scope
    if let Some(opt_deps_table) = project
        .get(FIELD_OPTIONAL_DEPENDENCIES)
        .and_then(|v| v.as_table())
    {
        for (extra_name, deps) in opt_deps_table {
            match deps {
                TomlValue::Array(arr) => {
                    optional_dependencies.extend(parse_dependency_array(
                        arr,
                        true,
                        Some(extra_name),
                    ));
                }
                TomlValue::Table(table) => {
                    optional_dependencies.extend(parse_dependency_table(
                        table,
                        true,
                        Some(extra_name),
                    ));
                }
                _ => {}
            }
        }
    }

    // Handle Poetry dev-dependencies
    if let Some(dev_deps_value) = project.get(FIELD_DEV_DEPENDENCIES) {
        match dev_deps_value {
            TomlValue::Array(arr) => {
                optional_dependencies.extend(parse_dependency_array(
                    arr,
                    true,
                    Some(FIELD_DEV_DEPENDENCIES),
                ));
            }
            TomlValue::Table(table) => {
                optional_dependencies.extend(parse_dependency_table(
                    table,
                    true,
                    Some(FIELD_DEV_DEPENDENCIES),
                ));
            }
            _ => {}
        }
    }

    // Handle Poetry dependency groups: [tool.poetry.group.<name>]
    if let Some(groups_table) = project.get("group").and_then(|v| v.as_table()) {
        for (group_name, group_data) in groups_table {
            if let Some(group_deps) = group_data.as_table().and_then(|t| t.get("dependencies")) {
                match group_deps {
                    TomlValue::Array(arr) => {
                        optional_dependencies.extend(parse_dependency_array(
                            arr,
                            true,
                            Some(group_name),
                        ));
                    }
                    TomlValue::Table(table) => {
                        optional_dependencies.extend(parse_dependency_table(
                            table,
                            true,
                            Some(group_name),
                        ));
                    }
                    _ => {}
                }
            }
        }
    }

    if let Some(groups_table) = toml_content
        .get(FIELD_DEPENDENCY_GROUPS)
        .and_then(|value| value.as_table())
    {
        for (group_name, deps) in groups_table {
            match deps {
                TomlValue::Array(arr) => {
                    optional_dependencies.extend(parse_dependency_array(
                        arr,
                        true,
                        Some(group_name),
                    ));
                }
                TomlValue::Table(table) => {
                    optional_dependencies.extend(parse_dependency_table(
                        table,
                        true,
                        Some(group_name),
                    ));
                }
                _ => {}
            }
        }
    }

    if let Some(dev_deps_value) = toml_content
        .get("tool")
        .and_then(|value| value.as_table())
        .and_then(|tool| tool.get("uv"))
        .and_then(|value| value.as_table())
        .and_then(|uv| uv.get(FIELD_DEV_DEPENDENCIES))
    {
        match dev_deps_value {
            TomlValue::Array(arr) => {
                optional_dependencies.extend(parse_dependency_array(arr, true, Some("dev")));
            }
            TomlValue::Table(table) => {
                optional_dependencies.extend(parse_dependency_table(table, true, Some("dev")));
            }
            _ => {}
        }
    }

    (dependencies, optional_dependencies)
}

fn extract_pyproject_extra_data(toml_content: &TomlValue) -> Option<HashMap<String, JsonValue>> {
    let mut extra_data = HashMap::new();

    if let Some(tool_uv) = toml_content
        .get("tool")
        .and_then(|value| value.as_table())
        .and_then(|tool| tool.get("uv"))
    {
        extra_data.insert("tool_uv".to_string(), toml_value_to_json(tool_uv));
    }

    if extra_data.is_empty() {
        None
    } else {
        Some(extra_data)
    }
}

fn toml_value_to_json(value: &TomlValue) -> JsonValue {
    match value {
        TomlValue::String(value) => JsonValue::String(value.clone()),
        TomlValue::Integer(value) => JsonValue::String(value.to_string()),
        TomlValue::Float(value) => JsonValue::String(value.to_string()),
        TomlValue::Boolean(value) => JsonValue::Bool(*value),
        TomlValue::Datetime(value) => JsonValue::String(value.to_string()),
        TomlValue::Array(values) => {
            JsonValue::Array(values.iter().map(toml_value_to_json).collect())
        }
        TomlValue::Table(values) => JsonValue::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), toml_value_to_json(value)))
                .collect::<JsonMap<String, JsonValue>>(),
        ),
    }
}

fn parse_dependency_table(
    table: &TomlMap<String, TomlValue>,
    is_optional: bool,
    scope: Option<&str>,
) -> Vec<Dependency> {
    table
        .iter()
        .filter_map(|(name, version)| {
            let version_str = version.as_str().map(|s| s.to_string());
            let mut package_url =
                PackageUrl::new(PythonParser::PACKAGE_TYPE.as_str(), name).ok()?;

            if let Some(v) = &version_str {
                package_url.with_version(v).ok()?;
            }

            Some(Dependency {
                purl: Some(package_url.to_string()),
                extracted_requirement: None,
                scope: scope.map(|s| s.to_string()),
                is_runtime: Some(!is_optional),
                is_optional: Some(is_optional),
                is_pinned: None,
                is_direct: Some(true),
                resolved_package: None,
                extra_data: None,
            })
        })
        .collect()
}

fn parse_dependency_array(
    array: &[TomlValue],
    is_optional: bool,
    scope: Option<&str>,
) -> Vec<Dependency> {
    array
        .iter()
        .filter_map(|dep| {
            let dep_str = dep.as_str()?;
            build_pyproject_array_dependency(dep_str, is_optional, scope)
        })
        .collect()
}

fn build_pyproject_array_dependency(
    dep_str: &str,
    is_optional: bool,
    scope: Option<&str>,
) -> Option<Dependency> {
    let parsed = parse_pep508_requirement(dep_str)?;
    let name = normalize_python_package_name(&parsed.name);
    let pinned_version = parsed
        .specifiers
        .as_deref()
        .and_then(extract_exact_pinned_version);

    let mut package_url = PackageUrl::new(PythonParser::PACKAGE_TYPE.as_str(), &name).ok()?;
    if let Some(version) = pinned_version.as_deref() {
        package_url.with_version(version).ok()?;
    }

    let mut extra_data = HashMap::new();
    if let Some(marker) = parsed.marker {
        extra_data.insert("marker".to_string(), JsonValue::String(marker));
    }
    if !parsed.extras.is_empty() {
        extra_data.insert(
            "extras".to_string(),
            JsonValue::Array(parsed.extras.into_iter().map(JsonValue::String).collect()),
        );
    }

    let extracted_requirement = parsed.specifiers.or(parsed.url);

    Some(Dependency {
        purl: Some(package_url.to_string()),
        extracted_requirement: extracted_requirement.clone(),
        scope: scope.map(|s| s.to_string()),
        is_runtime: Some(!is_optional),
        is_optional: Some(is_optional),
        is_pinned: Some(pinned_version.is_some()),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: if extra_data.is_empty() {
            None
        } else {
            Some(extra_data)
        },
    })
}

fn extract_exact_pinned_version(specifiers: &str) -> Option<String> {
    let trimmed = specifiers.trim();
    if trimmed.contains(',') {
        return None;
    }

    let stripped = if let Some(version) = trimmed.strip_prefix("===") {
        version
    } else if let Some(version) = trimmed.strip_prefix("==") {
        version
    } else {
        return None;
    };

    let version = stripped.trim();
    if version.is_empty() || version.contains('*') {
        None
    } else {
        Some(version.to_string())
    }
}

#[derive(Debug, Clone)]
enum Value {
    String(String),
    Number(f64),
    Bool(bool),
    None,
    List(Vec<Value>),
    Tuple(Vec<Value>),
    Dict(HashMap<String, Value>),
}

struct LiteralEvaluator {
    constants: HashMap<String, Value>,
    max_depth: usize,
    max_nodes: usize,
    nodes_visited: usize,
}

impl LiteralEvaluator {
    fn new(constants: HashMap<String, Value>) -> Self {
        Self {
            constants,
            max_depth: MAX_SETUP_PY_AST_DEPTH,
            max_nodes: MAX_SETUP_PY_AST_NODES,
            nodes_visited: 0,
        }
    }

    fn insert_constant(&mut self, name: String, value: Value) {
        self.constants.insert(name, value);
    }

    fn evaluate_expr(&mut self, expr: &ast::Expr, depth: usize) -> Option<Value> {
        if depth >= self.max_depth || self.nodes_visited >= self.max_nodes {
            return None;
        }
        self.nodes_visited += 1;

        match expr {
            ast::Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => {
                Some(Value::String(value.to_str().to_string()))
            }
            ast::Expr::BooleanLiteral(ast::ExprBooleanLiteral { value, .. }) => {
                Some(Value::Bool(*value))
            }
            ast::Expr::NumberLiteral(ast::ExprNumberLiteral { value, .. }) => {
                self.evaluate_number(value)
            }
            ast::Expr::NoneLiteral(_) => Some(Value::None),
            ast::Expr::Name(ast::ExprName { id, .. }) => self.constants.get(id.as_str()).cloned(),
            ast::Expr::List(ast::ExprList { elts, .. }) => {
                let mut values = Vec::new();
                for elt in elts {
                    values.push(self.evaluate_expr(elt, depth + 1)?);
                }
                Some(Value::List(values))
            }
            ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                let mut values = Vec::new();
                for elt in elts {
                    values.push(self.evaluate_expr(elt, depth + 1)?);
                }
                Some(Value::Tuple(values))
            }
            ast::Expr::Dict(ast::ExprDict { items, .. }) => {
                let mut dict = HashMap::new();
                for item in items {
                    let key_expr = item.key.as_ref()?;
                    let key_value = self.evaluate_expr(key_expr, depth + 1)?;
                    let key = value_to_string(&key_value)?;
                    let value = self.evaluate_expr(&item.value, depth + 1)?;
                    dict.insert(key, value);
                }
                Some(Value::Dict(dict))
            }
            ast::Expr::Call(ast::ExprCall {
                func, arguments, ..
            }) => {
                let args = arguments.args.as_ref();
                let keywords = arguments.keywords.as_ref();
                if keywords.is_empty()
                    && let Some(name) = dotted_name(func.as_ref(), depth + 1)
                    && matches!(name.as_str(), "OrderedDict" | "collections.OrderedDict")
                {
                    return self.evaluate_ordered_dict(args, depth + 1);
                }

                if !args.is_empty() {
                    return None;
                }

                if let ast::Expr::Name(ast::ExprName { id, .. }) = func.as_ref()
                    && id == "dict"
                {
                    let mut dict = HashMap::new();
                    for keyword in keywords {
                        let key = keyword.arg.as_ref().map(ast::Identifier::as_str)?;
                        let value = self.evaluate_expr(&keyword.value, depth + 1)?;
                        dict.insert(key.to_string(), value);
                    }
                    return Some(Value::Dict(dict));
                }

                None
            }
            _ => None,
        }
    }

    fn evaluate_number(&self, number: &ast::Number) -> Option<Value> {
        match number {
            ast::Number::Int(value) => value.to_string().parse::<f64>().ok().map(Value::Number),
            ast::Number::Float(value) => Some(Value::Number(*value)),
            ast::Number::Complex { .. } => None,
        }
    }

    fn evaluate_ordered_dict(&mut self, args: &[ast::Expr], depth: usize) -> Option<Value> {
        if args.len() != 1 {
            return None;
        }

        let items = match self.evaluate_expr(&args[0], depth)? {
            Value::List(items) | Value::Tuple(items) => items,
            _ => return None,
        };

        let mut dict = HashMap::new();
        for item in items {
            let Value::Tuple(values) = item else {
                return None;
            };
            if values.len() != 2 {
                return None;
            }
            let key = value_to_string(&values[0])?;
            dict.insert(key, values[1].clone());
        }

        Some(Value::Dict(dict))
    }
}

#[derive(Default)]
struct SetupAliases {
    setup_names: HashSet<String>,
    module_aliases: HashMap<String, String>,
}

fn extract_setup_py_packages(path: &Path) -> Vec<PackageData> {
    extract_from_setup_py(path).into_iter().collect()
}

fn extract_from_setup_py(path: &Path) -> Option<PackageData> {
    let content = match read_file_to_string(path) {
        Ok(content) => content,
        Err(e) => {
            warn!("Failed to read setup.py at {:?}: {}", path, e);
            return Some(default_package_data(path));
        }
    };

    if content.len() > MAX_SETUP_PY_BYTES {
        warn!("setup.py too large at {:?}: {} bytes", path, content.len());
        let package_data = extract_from_setup_py_regex(&content);
        return should_emit_setup_py_package(&package_data).then_some(package_data);
    }

    let mut package_data = match extract_from_setup_py_ast(&content) {
        Ok(Some(data)) => data,
        Ok(None) => return Some(default_package_data(path)),
        Err(e) => {
            warn!("Failed to parse setup.py AST at {:?}: {}", path, e);
            extract_from_setup_py_regex(&content)
        }
    };

    if package_data.name.is_none() {
        package_data.name = extract_setup_value(&content, "name");
    }

    if package_data.version.is_none() {
        package_data.version = extract_setup_value(&content, "version");
    }

    fill_from_sibling_dunder_metadata(path, &content, &mut package_data);

    if package_data.purl.is_none() {
        package_data.purl = build_setup_py_purl(
            package_data.name.as_deref(),
            package_data.version.as_deref(),
        );
    }

    if should_emit_setup_py_package(&package_data) {
        Some(package_data)
    } else {
        Some(default_package_data(path))
    }
}

fn should_emit_setup_py_package(package_data: &PackageData) -> bool {
    package_data.name.is_some()
        || package_data.version.is_some()
        || package_data.purl.is_some()
        || !package_data.dependencies.is_empty()
        || package_data.extracted_license_statement.is_some()
        || !package_data.license_detections.is_empty()
        || !package_data.parties.is_empty()
        || package_data.description.is_some()
        || package_data.homepage_url.is_some()
        || package_data.bug_tracking_url.is_some()
        || package_data.code_view_url.is_some()
        || package_data.vcs_url.is_some()
}

fn fill_from_sibling_dunder_metadata(path: &Path, content: &str, package_data: &mut PackageData) {
    if package_data.version.is_some()
        && package_data.extracted_license_statement.is_some()
        && package_data
            .parties
            .iter()
            .any(|party| party.role.as_deref() == Some("author") && party.name.is_some())
    {
        return;
    }

    let Some(root) = path.parent() else {
        return;
    };

    let dunder_metadata = collect_sibling_dunder_metadata(root, content);

    if package_data.version.is_none() {
        package_data.version = dunder_metadata.version;
    }

    if package_data.extracted_license_statement.is_none() {
        package_data.extracted_license_statement = dunder_metadata.license;
    }

    let has_author = package_data
        .parties
        .iter()
        .any(|party| party.role.as_deref() == Some("author") && party.name.is_some());

    if !has_author && let Some(author) = dunder_metadata.author {
        package_data.parties.push(Party {
            r#type: Some("person".to_string()),
            role: Some("author".to_string()),
            name: Some(author),
            email: None,
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }
}

#[derive(Default)]
struct DunderMetadata {
    version: Option<String>,
    author: Option<String>,
    license: Option<String>,
}

fn collect_sibling_dunder_metadata(root: &Path, content: &str) -> DunderMetadata {
    let statements = match parse_module(content) {
        Ok(parsed) => parsed.into_suite(),
        Err(_) => return DunderMetadata::default(),
    };

    let version_re = Regex::new(r#"(?m)^\s*__version__\s*=\s*['\"]([^'\"]+)['\"]"#).ok();
    let author_re = Regex::new(r#"(?m)^\s*__author__\s*=\s*['\"]([^'\"]+)['\"]"#).ok();
    let license_re = Regex::new(r#"(?m)^\s*__license__\s*=\s*['\"]([^'\"]+)['\"]"#).ok();
    let mut metadata = DunderMetadata::default();

    for module in imported_dunder_modules(&statements) {
        let Some(path) = resolve_imported_module_path(root, &module) else {
            continue;
        };
        let Ok(module_content) = read_file_to_string(&path) else {
            continue;
        };

        if metadata.version.is_none() {
            metadata.version = version_re
                .as_ref()
                .and_then(|regex| regex.captures(&module_content))
                .and_then(|captures| captures.get(1))
                .map(|match_| match_.as_str().to_string());
        }

        if metadata.author.is_none() {
            metadata.author = author_re
                .as_ref()
                .and_then(|regex| regex.captures(&module_content))
                .and_then(|captures| captures.get(1))
                .map(|match_| match_.as_str().to_string());
        }

        if metadata.license.is_none() {
            metadata.license = license_re
                .as_ref()
                .and_then(|regex| regex.captures(&module_content))
                .and_then(|captures| captures.get(1))
                .map(|match_| match_.as_str().to_string());
        }

        if metadata.version.is_some() && metadata.author.is_some() && metadata.license.is_some() {
            return metadata;
        }
    }

    metadata
}

fn imported_dunder_modules(statements: &[ast::Stmt]) -> Vec<String> {
    let mut modules = Vec::new();

    for statement in statements {
        let ast::Stmt::ImportFrom(ast::StmtImportFrom { module, names, .. }) = statement else {
            continue;
        };
        let Some(module) = module.as_ref().map(|name| name.as_str()) else {
            continue;
        };
        let imports_dunder = names.iter().any(|alias| {
            matches!(
                alias.name.as_str(),
                "__version__" | "__author__" | "__license__"
            )
        });
        if imports_dunder {
            modules.push(module.to_string());
        }
    }

    modules
}

fn resolve_imported_module_path(root: &Path, module: &str) -> Option<PathBuf> {
    let relative = PathBuf::from_iter(module.split('.'));
    let candidates = [
        root.join(relative.with_extension("py")),
        root.join(&relative).join("__init__.py"),
        root.join("src").join(relative.with_extension("py")),
        root.join("src").join(relative).join("__init__.py"),
    ];

    candidates.into_iter().find(|candidate| candidate.exists())
}

/// Extracts package metadata from setup.py using AST parsing (NO CODE EXECUTION).
///
/// # Security Model
///
/// This function parses setup.py as a Python AST and evaluates only literal values
/// (strings, numbers, lists, dicts). It does NOT execute Python code, preventing
/// arbitrary code execution during scanning.
///
/// # DoS Prevention
///
/// - `MAX_SETUP_PY_BYTES`: Limits file size to 1MB
/// - `MAX_SETUP_PY_AST_DEPTH`: Limits recursion depth (50 levels)
/// - `MAX_SETUP_PY_AST_NODES`: Limits total nodes visited (10,000)
///
/// These limits prevent stack overflow and infinite loops on malformed/malicious inputs.
fn extract_from_setup_py_ast(content: &str) -> Result<Option<PackageData>, String> {
    let statements = parse_module(content)
        .map(|parsed| parsed.into_suite())
        .map_err(|e| e.to_string())?;
    let aliases = collect_setup_aliases(&statements);
    let mut evaluator = LiteralEvaluator::new(HashMap::new());
    build_setup_py_constants(&statements, &mut evaluator);

    let setup_call = find_setup_call(&statements, &aliases);
    let Some(call_expr) = setup_call else {
        return Ok(None);
    };

    let setup_values = extract_setup_keywords(call_expr, &mut evaluator);
    Ok(Some(build_setup_py_package_data(&setup_values)))
}

fn build_setup_py_constants(statements: &[ast::Stmt], evaluator: &mut LiteralEvaluator) {
    for stmt in statements {
        if let ast::Stmt::Assign(ast::StmtAssign { targets, value, .. }) = stmt {
            if targets.len() != 1 {
                continue;
            }

            let Some(name) = extract_assign_name(&targets[0]) else {
                continue;
            };

            if let Some(value) = evaluator.evaluate_expr(value.as_ref(), 0) {
                evaluator.insert_constant(name, value);
            }
        }
    }
}

fn extract_assign_name(target: &ast::Expr) -> Option<String> {
    match target {
        ast::Expr::Name(ast::ExprName { id, .. }) => Some(id.as_str().to_string()),
        _ => None,
    }
}

fn collect_setup_aliases(statements: &[ast::Stmt]) -> SetupAliases {
    let mut aliases = SetupAliases::default();
    aliases.setup_names.insert("setup".to_string());

    for stmt in statements {
        match stmt {
            ast::Stmt::Import(ast::StmtImport { names, .. }) => {
                for alias in names {
                    let module_name = alias.name.as_str();
                    if !is_setup_module(module_name) {
                        continue;
                    }
                    let alias_name = alias
                        .asname
                        .as_ref()
                        .map(|name| name.as_str())
                        .unwrap_or(module_name);
                    aliases
                        .module_aliases
                        .insert(alias_name.to_string(), module_name.to_string());
                }
            }
            ast::Stmt::ImportFrom(ast::StmtImportFrom { module, names, .. }) => {
                let Some(module_name) = module.as_ref().map(|name| name.as_str()) else {
                    continue;
                };
                if !is_setup_module(module_name) {
                    continue;
                }
                for alias in names {
                    if alias.name.as_str() != "setup" {
                        continue;
                    }
                    let alias_name = alias
                        .asname
                        .as_ref()
                        .map(|name| name.as_str())
                        .unwrap_or("setup");
                    aliases.setup_names.insert(alias_name.to_string());
                }
            }
            _ => {}
        }
    }

    aliases
}

fn is_setup_module(module_name: &str) -> bool {
    matches!(module_name, "setuptools" | "distutils" | "distutils.core")
}

fn find_setup_call<'a>(
    statements: &'a [ast::Stmt],
    aliases: &'a SetupAliases,
) -> Option<&'a ast::Expr> {
    let mut finder = SetupCallFinder {
        aliases,
        called_function_names: collect_top_level_called_function_names(statements),
        nodes_visited: 0,
    };
    finder.find_in_statements(statements)
}

fn collect_top_level_called_function_names(statements: &[ast::Stmt]) -> HashSet<String> {
    let mut called = HashSet::new();
    collect_called_function_names_in_statements(statements, &mut called);
    called
}

fn collect_called_function_names_in_statements(
    statements: &[ast::Stmt],
    called: &mut HashSet<String>,
) {
    for stmt in statements {
        match stmt {
            ast::Stmt::Expr(ast::StmtExpr { value, .. })
            | ast::Stmt::Assign(ast::StmtAssign { value, .. }) => {
                collect_called_function_names_in_expr(value.as_ref(), called);
            }
            ast::Stmt::If(ast::StmtIf {
                body,
                elif_else_clauses,
                ..
            }) => {
                collect_called_function_names_in_statements(body, called);
                for clause in elif_else_clauses {
                    collect_called_function_names_in_statements(&clause.body, called);
                }
            }
            ast::Stmt::For(ast::StmtFor { body, orelse, .. })
            | ast::Stmt::While(ast::StmtWhile { body, orelse, .. }) => {
                collect_called_function_names_in_statements(body, called);
                collect_called_function_names_in_statements(orelse, called);
            }
            ast::Stmt::With(ast::StmtWith { body, .. }) => {
                collect_called_function_names_in_statements(body, called);
            }
            ast::Stmt::Try(ast::StmtTry {
                body,
                orelse,
                finalbody,
                handlers,
                ..
            }) => {
                collect_called_function_names_in_statements(body, called);
                collect_called_function_names_in_statements(orelse, called);
                collect_called_function_names_in_statements(finalbody, called);
                for handler in handlers {
                    let ast::ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                        body,
                        ..
                    }) = handler;
                    collect_called_function_names_in_statements(body, called);
                }
            }
            _ => {}
        }
    }
}

fn collect_called_function_names_in_expr(expr: &ast::Expr, called: &mut HashSet<String>) {
    if let ast::Expr::Call(ast::ExprCall {
        func, arguments, ..
    }) = expr
    {
        if let ast::Expr::Name(ast::ExprName { id, .. }) = func.as_ref() {
            called.insert(id.as_str().to_string());
        }

        for arg in arguments.args.iter() {
            collect_called_function_names_in_expr(arg, called);
        }
        for keyword in arguments.keywords.iter() {
            collect_called_function_names_in_expr(&keyword.value, called);
        }
    }
}

struct SetupCallFinder<'a> {
    aliases: &'a SetupAliases,
    called_function_names: HashSet<String>,
    nodes_visited: usize,
}

impl<'a> SetupCallFinder<'a> {
    fn find_in_statements(&mut self, statements: &'a [ast::Stmt]) -> Option<&'a ast::Expr> {
        for stmt in statements {
            if self.nodes_visited >= MAX_SETUP_PY_AST_NODES {
                return None;
            }
            self.nodes_visited += 1;

            let found = match stmt {
                ast::Stmt::Expr(ast::StmtExpr { value, .. }) => self.visit_expr(value.as_ref()),
                ast::Stmt::Assign(ast::StmtAssign { value, .. }) => self.visit_expr(value.as_ref()),
                ast::Stmt::If(ast::StmtIf {
                    body,
                    elif_else_clauses,
                    ..
                }) => self.find_in_statements(body).or_else(|| {
                    for clause in elif_else_clauses {
                        if let Some(found) = self.find_in_statements(&clause.body) {
                            return Some(found);
                        }
                    }
                    None
                }),
                ast::Stmt::For(ast::StmtFor { body, orelse, .. })
                | ast::Stmt::While(ast::StmtWhile { body, orelse, .. }) => self
                    .find_in_statements(body)
                    .or_else(|| self.find_in_statements(orelse)),
                ast::Stmt::FunctionDef(ast::StmtFunctionDef { name, body, .. }) => self
                    .called_function_names
                    .contains(name.as_str())
                    .then(|| self.find_in_statements(body))
                    .flatten(),
                ast::Stmt::With(ast::StmtWith { body, .. }) => self.find_in_statements(body),
                ast::Stmt::Try(ast::StmtTry {
                    body,
                    orelse,
                    finalbody,
                    handlers,
                    ..
                }) => self
                    .find_in_statements(body)
                    .or_else(|| self.find_in_statements(orelse))
                    .or_else(|| self.find_in_statements(finalbody))
                    .or_else(|| {
                        for handler in handlers {
                            let ast::ExceptHandler::ExceptHandler(
                                ast::ExceptHandlerExceptHandler { body, .. },
                            ) = handler;
                            if let Some(found) = self.find_in_statements(body) {
                                return Some(found);
                            }
                        }
                        None
                    }),
                _ => None,
            };

            if found.is_some() {
                return found;
            }
        }

        None
    }

    fn visit_expr(&mut self, expr: &'a ast::Expr) -> Option<&'a ast::Expr> {
        if self.nodes_visited >= MAX_SETUP_PY_AST_NODES {
            return None;
        }
        self.nodes_visited += 1;

        match expr {
            ast::Expr::Call(ast::ExprCall { func, .. })
                if is_setup_call(func.as_ref(), self.aliases) =>
            {
                Some(expr)
            }
            _ => None,
        }
    }
}

fn is_setup_call(func: &ast::Expr, aliases: &SetupAliases) -> bool {
    let Some(dotted) = dotted_name(func, 0) else {
        return false;
    };

    if aliases.setup_names.contains(&dotted) {
        return true;
    }

    let Some(module) = dotted.strip_suffix(".setup") else {
        return false;
    };

    let resolved = resolve_module_alias(module, aliases);
    is_setup_module(&resolved)
}

fn dotted_name(expr: &ast::Expr, depth: usize) -> Option<String> {
    if depth >= MAX_SETUP_PY_AST_DEPTH {
        return None;
    }

    match expr {
        ast::Expr::Name(ast::ExprName { id, .. }) => Some(id.as_str().to_string()),
        ast::Expr::Attribute(ast::ExprAttribute { value, attr, .. }) => {
            let base = dotted_name(value.as_ref(), depth + 1)?;
            Some(format!("{}.{}", base, attr.as_str()))
        }
        _ => None,
    }
}

fn resolve_module_alias(module: &str, aliases: &SetupAliases) -> String {
    if let Some(mapped) = aliases.module_aliases.get(module) {
        return mapped.clone();
    }

    let Some((base, rest)) = module.split_once('.') else {
        return module.to_string();
    };

    if let Some(mapped) = aliases.module_aliases.get(base) {
        return format!("{}.{}", mapped, rest);
    }

    module.to_string()
}

fn extract_setup_keywords(
    call_expr: &ast::Expr,
    evaluator: &mut LiteralEvaluator,
) -> HashMap<String, Value> {
    let mut values = HashMap::new();
    let ast::Expr::Call(ast::ExprCall { arguments, .. }) = call_expr else {
        return values;
    };

    for keyword in arguments.keywords.iter() {
        if let Some(arg) = keyword.arg.as_ref().map(ast::Identifier::as_str) {
            if let Some(value) = evaluator.evaluate_expr(&keyword.value, 0) {
                values.insert(arg.to_string(), value);
            }
        } else if let Some(Value::Dict(dict)) = evaluator.evaluate_expr(&keyword.value, 0) {
            for (key, value) in dict {
                values.insert(key, value);
            }
        }
    }

    values
}

fn build_setup_py_package_data(values: &HashMap<String, Value>) -> PackageData {
    let name = get_value_string(values, "name");
    let version = get_value_string(values, "version");
    let description =
        get_value_string(values, "description").or_else(|| get_value_string(values, "summary"));
    let homepage_url =
        get_value_string(values, "url").or_else(|| get_value_string(values, "home_page"));
    let author = get_value_string(values, "author");
    let author_email = get_value_string(values, "author_email");
    let maintainer = get_value_string(values, "maintainer");
    let maintainer_email = get_value_string(values, "maintainer_email");
    let license = get_value_string(values, "license");
    let classifiers = values
        .get("classifiers")
        .and_then(value_to_string_list)
        .unwrap_or_default();

    let mut parties = Vec::new();
    if author.is_some() || author_email.is_some() {
        parties.push(Party {
            r#type: Some("person".to_string()),
            role: Some("author".to_string()),
            name: author,
            email: author_email,
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }

    if maintainer.is_some() || maintainer_email.is_some() {
        parties.push(Party {
            r#type: Some("person".to_string()),
            role: Some("maintainer".to_string()),
            name: maintainer,
            email: maintainer_email,
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }

    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        normalize_spdx_declared_license(license.as_deref());
    let extracted_license_statement = license.clone();

    let dependencies = build_setup_py_dependencies(values);
    let purl = build_setup_py_purl(name.as_deref(), version.as_deref());
    let mut homepage_from_project_urls = None;
    let (mut bug_tracking_url, mut code_view_url, mut vcs_url) = (None, None, None);
    let mut extra_data = HashMap::new();

    if let Some(parsed_project_urls) = values.get("project_urls").and_then(value_to_string_pairs) {
        apply_project_url_mappings(
            &parsed_project_urls,
            &mut homepage_from_project_urls,
            &mut bug_tracking_url,
            &mut code_view_url,
            &mut vcs_url,
            &mut extra_data,
        );
    }

    let extra_data = if extra_data.is_empty() {
        None
    } else {
        Some(extra_data)
    };

    PackageData {
        package_type: Some(PythonParser::PACKAGE_TYPE),
        namespace: None,
        name,
        version,
        qualifiers: None,
        subpath: None,
        primary_language: Some("Python".to_string()),
        description,
        release_date: None,
        parties,
        keywords: Vec::new(),
        homepage_url: homepage_url.or(homepage_from_project_urls),
        download_url: None,
        size: None,
        sha1: None,
        md5: None,
        sha256: None,
        sha512: None,
        bug_tracking_url,
        code_view_url,
        vcs_url,
        copyright: None,
        holder: None,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        other_license_expression: None,
        other_license_expression_spdx: None,
        other_license_detections: Vec::new(),
        extracted_license_statement,
        notice_text: None,
        source_packages: Vec::new(),
        file_references: Vec::new(),
        is_private: has_private_classifier(&classifiers),
        is_virtual: false,
        extra_data,
        dependencies,
        repository_homepage_url: None,
        repository_download_url: None,
        api_data_url: None,
        datasource_id: Some(DatasourceId::PypiSetupPy),
        purl,
    }
}

fn build_setup_py_dependencies(values: &HashMap<String, Value>) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    if let Some(reqs) = values
        .get("install_requires")
        .and_then(value_to_string_list)
    {
        dependencies.extend(build_setup_py_dependency_list(&reqs, "install", false));
    }

    if let Some(reqs) = values.get("tests_require").and_then(value_to_string_list) {
        dependencies.extend(build_setup_py_dependency_list(&reqs, "test", true));
    }

    if let Some(Value::Dict(extras)) = values.get("extras_require") {
        let mut extra_items: Vec<_> = extras.iter().collect();
        extra_items.sort_by_key(|(name, _)| *name);
        for (extra_name, extra_value) in extra_items {
            if let Some(reqs) = value_to_string_list(extra_value) {
                dependencies.extend(build_setup_py_dependency_list(
                    reqs.as_slice(),
                    extra_name,
                    true,
                ));
            }
        }
    }

    dependencies
}

fn build_setup_py_dependency_list(
    reqs: &[String],
    scope: &str,
    is_optional: bool,
) -> Vec<Dependency> {
    reqs.iter()
        .filter_map(|req| build_setup_cfg_dependency(req, scope, is_optional))
        .collect()
}

fn get_value_string(values: &HashMap<String, Value>, key: &str) -> Option<String> {
    values.get(key).and_then(value_to_string)
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn value_to_string_list(value: &Value) -> Option<Vec<String>> {
    match value {
        Value::String(value) => Some(vec![value.clone()]),
        Value::List(values) | Value::Tuple(values) => {
            let mut items = Vec::new();
            for item in values {
                items.push(value_to_string(item)?);
            }
            Some(items)
        }
        _ => None,
    }
}

fn value_to_string_pairs(value: &Value) -> Option<Vec<(String, String)>> {
    let Value::Dict(dict) = value else {
        return None;
    };

    let mut pairs: Vec<(String, String)> = dict
        .iter()
        .map(|(key, value)| Some((key.clone(), value_to_string(value)?)))
        .collect::<Option<Vec<_>>>()?;
    pairs.sort_by(|left, right| left.0.cmp(&right.0));
    Some(pairs)
}

fn extract_rfc822_dependencies(headers: &HashMap<String, Vec<String>>) -> Vec<Dependency> {
    let requires_dist = super::rfc822::get_header_all(headers, "requires-dist");
    extract_requires_dist_dependencies(&requires_dist)
}

pub(crate) fn extract_requires_dist_dependencies(requires_dist: &[String]) -> Vec<Dependency> {
    requires_dist
        .iter()
        .filter_map(|entry| build_rfc822_dependency(entry))
        .collect()
}

fn build_rfc822_dependency(entry: &str) -> Option<Dependency> {
    build_python_dependency(entry, "install", false, None)
}

fn build_python_dependency(
    entry: &str,
    default_scope: &str,
    default_optional: bool,
    marker_override: Option<&str>,
) -> Option<Dependency> {
    let (requirement_part, marker_part) = entry
        .split_once(';')
        .map(|(req, marker)| (req.trim(), Some(marker.trim())))
        .unwrap_or((entry.trim(), None));

    let name = extract_setup_cfg_dependency_name(requirement_part)?;
    let requirement = normalize_rfc822_requirement(requirement_part);
    let (scope, is_optional, marker, marker_data) = parse_rfc822_marker(
        marker_part.or(marker_override),
        default_scope,
        default_optional,
    );
    let mut purl = PackageUrl::new(PythonParser::PACKAGE_TYPE.as_str(), &name).ok()?;

    let is_pinned = requirement
        .as_deref()
        .is_some_and(|req| req.starts_with("==") || req.starts_with("==="));
    if is_pinned
        && let Some(version) = requirement
            .as_deref()
            .map(|req| req.trim_start_matches('='))
    {
        purl.with_version(version).ok()?;
    }

    let mut extra_data = HashMap::new();
    extra_data.extend(marker_data);
    if let Some(marker) = marker {
        extra_data.insert("marker".to_string(), serde_json::Value::String(marker));
    }

    Some(Dependency {
        purl: Some(purl.to_string()),
        extracted_requirement: requirement,
        scope: Some(scope),
        is_runtime: Some(true),
        is_optional: Some(is_optional),
        is_pinned: Some(is_pinned),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: if extra_data.is_empty() {
            None
        } else {
            Some(extra_data)
        },
    })
}

fn normalize_rfc822_requirement(requirement_part: &str) -> Option<String> {
    let name = extract_setup_cfg_dependency_name(requirement_part)?;
    let trimmed = requirement_part.trim();
    let mut remainder = trimmed[name.len()..].trim();

    if let Some(stripped) = remainder.strip_prefix('[')
        && let Some(end_idx) = stripped.find(']')
    {
        remainder = stripped[end_idx + 1..].trim();
    }

    let remainder = remainder
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
        .unwrap_or(remainder)
        .trim();

    if remainder.is_empty() {
        return None;
    }

    let mut specifiers: Vec<String> = remainder
        .split(',')
        .map(|specifier| specifier.trim().replace(' ', ""))
        .filter(|specifier| !specifier.is_empty())
        .collect();
    specifiers.sort();
    Some(specifiers.join(","))
}

fn parse_rfc822_marker(
    marker_part: Option<&str>,
    default_scope: &str,
    default_optional: bool,
) -> (
    String,
    bool,
    Option<String>,
    HashMap<String, serde_json::Value>,
) {
    let Some(marker) = marker_part.filter(|marker| !marker.trim().is_empty()) else {
        return (
            default_scope.to_string(),
            default_optional,
            None,
            HashMap::new(),
        );
    };

    let extra_re = Regex::new(r#"extra\s*==\s*['\"]([^'\"]+)['\"]"#)
        .expect("extra marker regex should compile");
    let mut extra_data = HashMap::new();

    if let Some(python_version) = extract_marker_field(marker, "python_version") {
        extra_data.insert(
            "python_version".to_string(),
            serde_json::Value::String(python_version),
        );
    }
    if let Some(sys_platform) = extract_marker_field(marker, "sys_platform") {
        extra_data.insert(
            "sys_platform".to_string(),
            serde_json::Value::String(sys_platform),
        );
    }

    if let Some(captures) = extra_re.captures(marker)
        && let Some(scope) = captures.get(1)
    {
        return (
            scope.as_str().to_string(),
            true,
            Some(marker.trim().to_string()),
            extra_data,
        );
    }

    (
        default_scope.to_string(),
        default_optional,
        Some(marker.trim().to_string()),
        extra_data,
    )
}

fn extract_marker_field(marker: &str, field: &str) -> Option<String> {
    let re = Regex::new(&format!(
        r#"{}\s*(==|!=|<=|>=|<|>)\s*['\"]([^'\"]+)['\"]"#,
        field
    ))
    .ok()?;
    let captures = re.captures(marker)?;
    let operator = captures.get(1)?.as_str();
    let value = captures.get(2)?.as_str();
    Some(format!("{} {}", operator, value))
}

fn parse_requires_txt(content: &str) -> Vec<Dependency> {
    let mut dependencies = Vec::new();
    let mut current_scope = "install".to_string();
    let mut current_optional = false;
    let mut current_marker: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let inner = &trimmed[1..trimmed.len() - 1];
            if let Some(rest) = inner.strip_prefix(':') {
                current_scope = "install".to_string();
                current_optional = false;
                current_marker = Some(rest.trim().to_string());
            } else if let Some((scope, marker)) = inner.split_once(':') {
                current_scope = scope.trim().to_string();
                current_optional = true;
                current_marker = Some(marker.trim().to_string());
            } else {
                current_scope = inner.trim().to_string();
                current_optional = true;
                current_marker = None;
            }
            continue;
        }

        if let Some(dependency) = build_python_dependency(
            trimmed,
            &current_scope,
            current_optional,
            current_marker.as_deref(),
        ) {
            dependencies.push(dependency);
        }
    }

    dependencies
}

fn has_private_classifier(classifiers: &[String]) -> bool {
    classifiers
        .iter()
        .any(|classifier| classifier.eq_ignore_ascii_case("Private :: Do Not Upload"))
}

fn build_setup_py_purl(name: Option<&str>, version: Option<&str>) -> Option<String> {
    let name = name?;
    let mut package_url = PackageUrl::new(PythonParser::PACKAGE_TYPE.as_str(), name).ok()?;
    if let Some(version) = version {
        package_url.with_version(version).ok()?;
    }
    Some(package_url.to_string())
}

fn extract_from_setup_py_regex(content: &str) -> PackageData {
    let name = extract_setup_value(content, "name");
    let version = extract_setup_value(content, "version");
    let license_expression = extract_setup_value(content, "license");

    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        normalize_spdx_declared_license(license_expression.as_deref());
    let extracted_license_statement = license_expression.clone();

    let dependencies = extract_setup_py_dependencies(content);
    let homepage_url = extract_setup_value(content, "url");
    let purl = build_setup_py_purl(name.as_deref(), version.as_deref());

    PackageData {
        package_type: Some(PythonParser::PACKAGE_TYPE),
        namespace: None,
        name,
        version,
        qualifiers: None,
        subpath: None,
        primary_language: Some("Python".to_string()),
        description: None,
        release_date: None,
        parties: Vec::new(),
        keywords: Vec::new(),
        homepage_url,
        download_url: None,
        size: None,
        sha1: None,
        md5: None,
        sha256: None,
        sha512: None,
        bug_tracking_url: None,
        code_view_url: None,
        vcs_url: None,
        copyright: None,
        holder: None,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        other_license_expression: None,
        other_license_expression_spdx: None,
        other_license_detections: Vec::new(),
        extracted_license_statement,
        notice_text: None,
        source_packages: Vec::new(),
        file_references: Vec::new(),
        is_private: false,
        is_virtual: false,
        extra_data: None,
        dependencies,
        repository_homepage_url: None,
        repository_download_url: None,
        api_data_url: None,
        datasource_id: Some(DatasourceId::PypiSetupPy),
        purl,
    }
}

fn package_data_to_resolved(pkg: &PackageData) -> crate::models::ResolvedPackage {
    crate::models::ResolvedPackage::from_package_data(pkg, PackageType::Pypi)
}

fn extract_from_pypi_json(path: &Path) -> PackageData {
    let default = PackageData {
        package_type: Some(PythonParser::PACKAGE_TYPE),
        datasource_id: Some(DatasourceId::PypiJson),
        ..Default::default()
    };

    let content = match read_file_to_string(path) {
        Ok(content) => content,
        Err(error) => {
            warn!("Failed to read pypi.json at {:?}: {}", path, error);
            return default;
        }
    };

    let root: serde_json::Value = match serde_json::from_str(&content) {
        Ok(value) => value,
        Err(error) => {
            warn!("Failed to parse pypi.json at {:?}: {}", path, error);
            return default;
        }
    };

    let Some(info) = root.get("info").and_then(|value| value.as_object()) else {
        warn!("No info object found in pypi.json at {:?}", path);
        return default;
    };

    let name = info
        .get("name")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned);
    let version = info
        .get("version")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned);
    let summary = info
        .get("summary")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned);
    let description = info
        .get("description")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .or(summary);
    let mut homepage_url = info
        .get("home_page")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned);
    let author = info
        .get("author")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned);
    let author_email = info
        .get("author_email")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned);
    let license = info
        .get("license")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned);
    let keywords = parse_setup_cfg_keywords(
        info.get("keywords")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
    );
    let classifiers = info
        .get("classifiers")
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut parties = Vec::new();
    if author.is_some() || author_email.is_some() {
        parties.push(Party {
            r#type: Some("person".to_string()),
            role: Some("author".to_string()),
            name: author,
            email: author_email,
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }

    let mut bug_tracking_url = None;
    let mut code_view_url = None;
    let mut vcs_url = None;
    let mut extra_data = HashMap::new();

    let parsed_project_urls = info
        .get("project_urls")
        .and_then(|value| value.as_object())
        .map(|map| {
            let mut pairs: Vec<(String, String)> = map
                .iter()
                .filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_string())))
                .collect();
            pairs.sort_by(|left, right| left.0.cmp(&right.0));
            pairs
        })
        .unwrap_or_default();

    apply_project_url_mappings(
        &parsed_project_urls,
        &mut homepage_url,
        &mut bug_tracking_url,
        &mut code_view_url,
        &mut vcs_url,
        &mut extra_data,
    );

    let (download_url, size, sha256) = root
        .get("urls")
        .and_then(|value| value.as_array())
        .map(|urls| select_pypi_json_artifact(urls))
        .unwrap_or((None, None, None));

    let sha256 = sha256.and_then(|h| Sha256Digest::from_hex(&h).ok());

    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        normalize_spdx_declared_license(license.as_deref());
    let dependencies = info
        .get("requires_dist")
        .and_then(|value| value.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.as_str().map(ToOwned::to_owned))
                .collect::<Vec<_>>()
        })
        .map(|entries| extract_requires_dist_dependencies(&entries))
        .unwrap_or_default();

    let (repository_homepage_url, repository_download_url, api_data_url, purl) =
        build_pypi_urls(name.as_deref(), version.as_deref());

    PackageData {
        package_type: Some(PythonParser::PACKAGE_TYPE),
        namespace: None,
        name,
        version,
        qualifiers: None,
        subpath: None,
        primary_language: None,
        description,
        release_date: None,
        parties,
        keywords,
        homepage_url: homepage_url.or(repository_homepage_url.clone()),
        download_url,
        size,
        sha1: None,
        md5: None,
        sha256,
        sha512: None,
        bug_tracking_url,
        code_view_url,
        vcs_url,
        copyright: None,
        holder: None,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        other_license_expression: None,
        other_license_expression_spdx: None,
        other_license_detections: Vec::new(),
        extracted_license_statement: license,
        notice_text: None,
        source_packages: Vec::new(),
        file_references: Vec::new(),
        is_private: has_private_classifier(&classifiers),
        is_virtual: false,
        extra_data: if extra_data.is_empty() {
            None
        } else {
            Some(extra_data)
        },
        dependencies,
        repository_homepage_url,
        repository_download_url,
        api_data_url,
        datasource_id: Some(DatasourceId::PypiJson),
        purl,
    }
}

fn select_pypi_json_artifact(
    urls: &[serde_json::Value],
) -> (Option<String>, Option<u64>, Option<String>) {
    let selected = urls
        .iter()
        .find(|entry| entry.get("packagetype").and_then(|value| value.as_str()) == Some("sdist"))
        .or_else(|| urls.first());

    let Some(entry) = selected else {
        return (None, None, None);
    };

    let download_url = entry
        .get("url")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned);
    let size = entry.get("size").and_then(|value| value.as_u64());
    let sha256 = entry
        .get("digests")
        .and_then(|value| value.as_object())
        .and_then(|digests| digests.get("sha256"))
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned);

    (download_url, size, sha256)
}

fn extract_from_pip_inspect(path: &Path) -> PackageData {
    let content = match read_file_to_string(path) {
        Ok(content) => content,
        Err(e) => {
            warn!("Failed to read pip-inspect.deplock at {:?}: {}", path, e);
            return default_package_data(path);
        }
    };

    let root: serde_json::Value = match serde_json::from_str(&content) {
        Ok(value) => value,
        Err(e) => {
            warn!(
                "Failed to parse pip-inspect.deplock JSON at {:?}: {}",
                path, e
            );
            return default_package_data(path);
        }
    };

    let installed = match root.get("installed").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => {
            warn!(
                "No 'installed' array found in pip-inspect.deplock at {:?}",
                path
            );
            return default_package_data(path);
        }
    };

    let pip_version = root
        .get("pip_version")
        .and_then(|v| v.as_str())
        .map(String::from);
    let inspect_version = root
        .get("version")
        .and_then(|v| v.as_str())
        .map(String::from);

    let mut main_package: Option<PackageData> = None;
    let mut dependencies: Vec<Dependency> = Vec::new();

    for package_entry in installed {
        let metadata = match package_entry.get("metadata") {
            Some(m) => m,
            None => continue,
        };

        let is_requested = package_entry
            .get("requested")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let has_direct_url = package_entry.get("direct_url").is_some();

        let name = metadata
            .get("name")
            .and_then(|v| v.as_str())
            .map(String::from);
        let version = metadata
            .get("version")
            .and_then(|v| v.as_str())
            .map(String::from);
        let summary = metadata
            .get("summary")
            .and_then(|v| v.as_str())
            .map(String::from);
        let home_page = metadata
            .get("home_page")
            .and_then(|v| v.as_str())
            .map(String::from);
        let author = metadata
            .get("author")
            .and_then(|v| v.as_str())
            .map(String::from);
        let author_email = metadata
            .get("author_email")
            .and_then(|v| v.as_str())
            .map(String::from);
        let license = metadata
            .get("license")
            .and_then(|v| v.as_str())
            .map(String::from);
        let description = metadata
            .get("description")
            .and_then(|v| v.as_str())
            .map(String::from);
        let keywords = metadata
            .get("keywords")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|k| k.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let mut parties = Vec::new();
        if author.is_some() || author_email.is_some() {
            parties.push(Party {
                r#type: Some("person".to_string()),
                role: Some("author".to_string()),
                name: author,
                email: author_email,
                url: None,
                organization: None,
                organization_url: None,
                timezone: None,
            });
        }

        let (declared_license_expression, declared_license_expression_spdx, license_detections) =
            normalize_spdx_declared_license(license.as_deref());
        let extracted_license_statement = license.clone();
        let requires_dist = metadata
            .get("requires_dist")
            .and_then(|v| v.as_array())
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| entry.as_str().map(ToOwned::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let parsed_dependencies = extract_requires_dist_dependencies(&requires_dist);

        let purl = name.as_ref().and_then(|n| {
            let mut package_url = PackageUrl::new(PythonParser::PACKAGE_TYPE.as_str(), n).ok()?;
            if let Some(v) = &version {
                package_url.with_version(v).ok()?;
            }
            Some(package_url.to_string())
        });

        if is_requested && has_direct_url {
            let mut extra_data = HashMap::new();
            if let Some(pv) = &pip_version {
                extra_data.insert(
                    "pip_version".to_string(),
                    serde_json::Value::String(pv.clone()),
                );
            }
            if let Some(iv) = &inspect_version {
                extra_data.insert(
                    "inspect_version".to_string(),
                    serde_json::Value::String(iv.clone()),
                );
            }

            main_package = Some(PackageData {
                package_type: Some(PythonParser::PACKAGE_TYPE),
                namespace: None,
                name,
                version,
                qualifiers: None,
                subpath: None,
                primary_language: Some("Python".to_string()),
                description: description.or(summary),
                release_date: None,
                parties,
                keywords,
                homepage_url: home_page,
                download_url: None,
                size: None,
                sha1: None,
                md5: None,
                sha256: None,
                sha512: None,
                bug_tracking_url: None,
                code_view_url: None,
                vcs_url: None,
                copyright: None,
                holder: None,
                declared_license_expression,
                declared_license_expression_spdx,
                license_detections,
                other_license_expression: None,
                other_license_expression_spdx: None,
                other_license_detections: Vec::new(),
                extracted_license_statement,
                notice_text: None,
                source_packages: Vec::new(),
                file_references: Vec::new(),
                is_private: false,
                is_virtual: true,
                extra_data: if extra_data.is_empty() {
                    None
                } else {
                    Some(extra_data)
                },
                dependencies: parsed_dependencies,
                repository_homepage_url: None,
                repository_download_url: None,
                api_data_url: None,
                datasource_id: Some(DatasourceId::PypiInspectDeplock),
                purl,
            });
        } else {
            let resolved_package = PackageData {
                package_type: Some(PythonParser::PACKAGE_TYPE),
                namespace: None,
                name: name.clone(),
                version: version.clone(),
                qualifiers: None,
                subpath: None,
                primary_language: Some("Python".to_string()),
                description: description.or(summary),
                release_date: None,
                parties,
                keywords,
                homepage_url: home_page,
                download_url: None,
                size: None,
                sha1: None,
                md5: None,
                sha256: None,
                sha512: None,
                bug_tracking_url: None,
                code_view_url: None,
                vcs_url: None,
                copyright: None,
                holder: None,
                declared_license_expression,
                declared_license_expression_spdx,
                license_detections,
                other_license_expression: None,
                other_license_expression_spdx: None,
                other_license_detections: Vec::new(),
                extracted_license_statement,
                notice_text: None,
                source_packages: Vec::new(),
                file_references: Vec::new(),
                is_private: false,
                is_virtual: true,
                extra_data: None,
                dependencies: parsed_dependencies,
                repository_homepage_url: None,
                repository_download_url: None,
                api_data_url: None,
                datasource_id: Some(DatasourceId::PypiInspectDeplock),
                purl: purl.clone(),
            };

            let resolved = package_data_to_resolved(&resolved_package);
            dependencies.push(Dependency {
                purl,
                extracted_requirement: None,
                scope: None,
                is_runtime: Some(true),
                is_optional: Some(false),
                is_pinned: Some(true),
                is_direct: Some(is_requested),
                resolved_package: Some(Box::new(resolved)),
                extra_data: None,
            });
        }
    }

    if let Some(mut main_pkg) = main_package {
        let direct_requirement_purls: HashSet<String> = main_pkg
            .dependencies
            .iter()
            .filter_map(|dep| dep.purl.as_deref().map(base_dependency_purl))
            .collect();

        let resolved_requirement_purls: HashSet<String> = dependencies
            .iter()
            .filter_map(|dep| dep.purl.as_deref().map(base_dependency_purl))
            .collect();

        let unresolved_dependencies = main_pkg
            .dependencies
            .iter()
            .filter(|dep| {
                dep.purl.as_ref().is_some_and(|purl| {
                    !resolved_requirement_purls.contains(&base_dependency_purl(purl))
                })
            })
            .cloned()
            .collect::<Vec<_>>();

        for dependency in &mut dependencies {
            if dependency
                .purl
                .as_ref()
                .is_some_and(|purl| direct_requirement_purls.contains(&base_dependency_purl(purl)))
            {
                dependency.is_direct = Some(true);
            }
        }

        main_pkg.dependencies = dependencies;
        main_pkg.dependencies.extend(unresolved_dependencies);
        main_pkg
    } else {
        default_package_data(path)
    }
}

fn base_dependency_purl(purl: &str) -> String {
    purl.split_once('@')
        .map(|(base, _)| base.to_string())
        .unwrap_or_else(|| purl.to_string())
}

type IniSections = HashMap<String, HashMap<String, Vec<String>>>;

fn extract_from_setup_cfg(path: &Path) -> PackageData {
    let content = match read_file_to_string(path) {
        Ok(content) => content,
        Err(e) => {
            warn!("Failed to read setup.cfg at {:?}: {}", path, e);
            return default_package_data(path);
        }
    };

    let sections = parse_setup_cfg(&content);
    let name = get_ini_value(&sections, "metadata", "name");
    let version = get_ini_value(&sections, "metadata", "version");
    let description = get_ini_value(&sections, "metadata", "description");
    let author = get_ini_value(&sections, "metadata", "author");
    let author_email = get_ini_value(&sections, "metadata", "author_email");
    let maintainer = get_ini_value(&sections, "metadata", "maintainer");
    let maintainer_email = get_ini_value(&sections, "metadata", "maintainer_email");
    let license = get_ini_value(&sections, "metadata", "license");
    let mut homepage_url = get_ini_value(&sections, "metadata", "url");
    let classifiers = get_ini_values(&sections, "metadata", "classifiers");
    let keywords = parse_setup_cfg_keywords(get_ini_value(&sections, "metadata", "keywords"));
    let python_requires = get_ini_value(&sections, "options", "python_requires");
    let parsed_project_urls =
        parse_setup_cfg_project_urls(&get_ini_values(&sections, "metadata", "project_urls"));
    let (mut bug_tracking_url, mut code_view_url, mut vcs_url) = (None, None, None);
    let mut extra_data = HashMap::new();

    let mut parties = Vec::new();
    if author.is_some() || author_email.is_some() {
        parties.push(Party {
            r#type: Some("person".to_string()),
            role: Some("author".to_string()),
            name: author,
            email: author_email,
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }

    if maintainer.is_some() || maintainer_email.is_some() {
        parties.push(Party {
            r#type: Some("person".to_string()),
            role: Some("maintainer".to_string()),
            name: maintainer,
            email: maintainer_email,
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }

    let declared_license_expression = None;
    let declared_license_expression_spdx = None;
    let license_detections = Vec::new();
    let extracted_license_statement = license.clone();

    let dependencies = extract_setup_cfg_dependencies(&sections);

    if let Some(value) = python_requires {
        extra_data.insert(
            "python_requires".to_string(),
            serde_json::Value::String(value),
        );
    }

    apply_project_url_mappings(
        &parsed_project_urls,
        &mut homepage_url,
        &mut bug_tracking_url,
        &mut code_view_url,
        &mut vcs_url,
        &mut extra_data,
    );

    let extra_data = if extra_data.is_empty() {
        None
    } else {
        Some(extra_data)
    };

    let purl = name.as_ref().and_then(|n| {
        let mut package_url = PackageUrl::new(PythonParser::PACKAGE_TYPE.as_str(), n).ok()?;
        if let Some(v) = &version {
            package_url.with_version(v).ok()?;
        }
        Some(package_url.to_string())
    });

    PackageData {
        package_type: Some(PythonParser::PACKAGE_TYPE),
        namespace: None,
        name,
        version,
        qualifiers: None,
        subpath: None,
        primary_language: Some("Python".to_string()),
        description,
        release_date: None,
        parties,
        keywords,
        homepage_url,
        download_url: None,
        size: None,
        sha1: None,
        md5: None,
        sha256: None,
        sha512: None,
        bug_tracking_url,
        code_view_url,
        vcs_url,
        copyright: None,
        holder: None,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        other_license_expression: None,
        other_license_expression_spdx: None,
        other_license_detections: Vec::new(),
        extracted_license_statement,
        notice_text: None,
        source_packages: Vec::new(),
        file_references: Vec::new(),
        is_private: has_private_classifier(&classifiers),
        is_virtual: false,
        extra_data,
        dependencies,
        repository_homepage_url: None,
        repository_download_url: None,
        api_data_url: None,
        datasource_id: Some(DatasourceId::PypiSetupCfg),
        purl,
    }
}

fn parse_setup_cfg_keywords(value: Option<String>) -> Vec<String> {
    let Some(keywords) = value else {
        return Vec::new();
    };

    keywords
        .split(',')
        .map(str::trim)
        .filter(|keyword| !keyword.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn parse_setup_cfg_project_urls(entries: &[String]) -> Vec<(String, String)> {
    entries
        .iter()
        .filter_map(|entry| {
            let (label, url) = entry.split_once('=')?;
            let label = label.trim();
            let url = url.trim();
            if label.is_empty() || url.is_empty() {
                None
            } else {
                Some((label.to_string(), url.to_string()))
            }
        })
        .collect()
}

fn apply_project_url_mappings(
    parsed_urls: &[(String, String)],
    homepage_url: &mut Option<String>,
    bug_tracking_url: &mut Option<String>,
    code_view_url: &mut Option<String>,
    vcs_url: &mut Option<String>,
    extra_data: &mut HashMap<String, serde_json::Value>,
) {
    for (label, url) in parsed_urls {
        let label_lower = label.to_lowercase();

        if bug_tracking_url.is_none()
            && matches!(
                label_lower.as_str(),
                "tracker"
                    | "bug reports"
                    | "bug tracker"
                    | "issues"
                    | "issue tracker"
                    | "github: issues"
            )
        {
            *bug_tracking_url = Some(url.clone());
        } else if code_view_url.is_none()
            && matches!(label_lower.as_str(), "source" | "source code" | "code")
        {
            *code_view_url = Some(url.clone());
        } else if vcs_url.is_none()
            && matches!(
                label_lower.as_str(),
                "github" | "gitlab" | "github: repo" | "repository"
            )
        {
            *vcs_url = Some(url.clone());
        } else if homepage_url.is_none()
            && matches!(label_lower.as_str(), "website" | "homepage" | "home")
        {
            *homepage_url = Some(url.clone());
        } else if label_lower == "changelog" {
            extra_data.insert(
                "changelog_url".to_string(),
                serde_json::Value::String(url.clone()),
            );
        }
    }

    let project_urls_json: serde_json::Map<String, serde_json::Value> = parsed_urls
        .iter()
        .map(|(label, url)| (label.clone(), serde_json::Value::String(url.clone())))
        .collect();

    if !project_urls_json.is_empty() {
        extra_data.insert(
            "project_urls".to_string(),
            serde_json::Value::Object(project_urls_json),
        );
    }
}

fn parse_setup_cfg(content: &str) -> IniSections {
    let mut sections: IniSections = HashMap::new();
    let mut current_section: Option<String> = None;
    let mut current_key: Option<String> = None;

    for raw_line in content.lines() {
        let line = raw_line.trim_end_matches('\r');
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let stripped = line.trim_start();
        if stripped.starts_with('#') || stripped.starts_with(';') {
            continue;
        }

        if stripped.starts_with('[') && stripped.ends_with(']') {
            let section_name = stripped
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim()
                .to_ascii_lowercase();
            current_section = if section_name.is_empty() {
                None
            } else {
                Some(section_name)
            };
            current_key = None;
            continue;
        }

        if (line.starts_with(' ') || line.starts_with('\t')) && current_key.is_some() {
            if let (Some(section), Some(key)) = (current_section.as_ref(), current_key.as_ref()) {
                let value = stripped.trim();
                if !value.is_empty() {
                    sections
                        .entry(section.clone())
                        .or_default()
                        .entry(key.clone())
                        .or_default()
                        .push(value.to_string());
                }
            }
            continue;
        }

        if let Some((key, value)) = stripped.split_once('=')
            && let Some(section) = current_section.as_ref()
        {
            let key_name = key.trim().to_ascii_lowercase();
            let value_trimmed = value.trim();
            let entry = sections
                .entry(section.clone())
                .or_default()
                .entry(key_name.clone())
                .or_default();
            if !value_trimmed.is_empty() {
                entry.push(value_trimmed.to_string());
            }
            current_key = Some(key_name);
        }
    }

    sections
}

fn get_ini_value(sections: &IniSections, section: &str, key: &str) -> Option<String> {
    sections
        .get(&section.to_ascii_lowercase())
        .and_then(|values| values.get(&key.to_ascii_lowercase()))
        .and_then(|entries| entries.first())
        .map(|value| value.trim().to_string())
}

fn get_ini_values(sections: &IniSections, section: &str, key: &str) -> Vec<String> {
    sections
        .get(&section.to_ascii_lowercase())
        .and_then(|values| values.get(&key.to_ascii_lowercase()))
        .cloned()
        .unwrap_or_default()
}

fn extract_setup_cfg_dependencies(sections: &IniSections) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    for (sub_section, scope) in [
        ("install_requires", "install"),
        ("tests_require", "test"),
        ("setup_requires", "setup"),
    ] {
        let reqs = get_ini_values(sections, "options", sub_section);
        dependencies.extend(parse_setup_cfg_requirements(&reqs, scope, false));
    }

    if let Some(extras) = sections.get("options.extras_require") {
        let mut extra_items: Vec<_> = extras.iter().collect();
        extra_items.sort_by_key(|(name, _)| *name);
        for (extra_name, reqs) in extra_items {
            dependencies.extend(parse_setup_cfg_requirements(reqs, extra_name, true));
        }
    }

    dependencies
}

fn parse_setup_cfg_requirements(
    reqs: &[String],
    scope: &str,
    is_optional: bool,
) -> Vec<Dependency> {
    reqs.iter()
        .filter_map(|req| build_setup_cfg_dependency(req, scope, is_optional))
        .collect()
}

fn build_setup_cfg_dependency(req: &str, scope: &str, is_optional: bool) -> Option<Dependency> {
    let trimmed = req.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }

    let name = extract_setup_cfg_dependency_name(trimmed)?;
    let purl = PackageUrl::new(PythonParser::PACKAGE_TYPE.as_str(), &name).ok()?;

    Some(Dependency {
        purl: Some(purl.to_string()),
        extracted_requirement: Some(normalize_setup_cfg_requirement(trimmed)),
        scope: Some(scope.to_string()),
        is_runtime: Some(true),
        is_optional: Some(is_optional),
        is_pinned: Some(false),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: None,
    })
}

fn extract_setup_cfg_dependency_name(req: &str) -> Option<String> {
    let trimmed = req.trim();
    if trimmed.is_empty() {
        return None;
    }

    let end = trimmed
        .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '=' | '!' | '~' | ';' | '['))
        .unwrap_or(trimmed.len());
    let name = trimmed[..end].trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn normalize_setup_cfg_requirement(req: &str) -> String {
    req.chars().filter(|c| !c.is_whitespace()).collect()
}

fn extract_setup_value(content: &str, key: &str) -> Option<String> {
    let patterns = vec![
        format!("{}=\"", key),   // name="value"
        format!("{} =\"", key),  // name ="value"
        format!("{}= \"", key),  // name= "value"
        format!("{} = \"", key), // name = "value"
        format!("{}='", key),    // name='value'
        format!("{} ='", key),   // name ='value'
        format!("{}= '", key),   // name= 'value'
        format!("{} = '", key),  // name = 'value'
    ];

    for pattern in patterns {
        if let Some(start_idx) = content.find(&pattern) {
            let value_start = start_idx + pattern.len();
            let remaining = &content[value_start..];

            if let Some(end_idx) = remaining.find(['"', '\'']) {
                return Some(remaining[..end_idx].to_string());
            }
        }
    }

    None
}

fn extract_setup_py_dependencies(content: &str) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    if let Some(tests_deps) = extract_tests_require(content) {
        dependencies.extend(tests_deps);
    }

    if let Some(extras_deps) = extract_extras_require(content) {
        dependencies.extend(extras_deps);
    }

    dependencies
}

fn extract_tests_require(content: &str) -> Option<Vec<Dependency>> {
    let pattern = r"tests_require\s*=\s*\[([^\]]+)\]";
    let re = Regex::new(pattern).ok()?;
    let captures = re.captures(content)?;
    let deps_str = captures.get(1)?.as_str();

    let deps = parse_setup_py_dep_list(deps_str, "test", true);
    if deps.is_empty() { None } else { Some(deps) }
}

fn extract_extras_require(content: &str) -> Option<Vec<Dependency>> {
    let pattern = r"extras_require\s*=\s*\{([^}]+)\}";
    let re = Regex::new(pattern).ok()?;
    let captures = re.captures(content)?;
    let dict_content = captures.get(1)?.as_str();

    let mut all_deps = Vec::new();

    let entry_pattern = r#"['"]([^'"]+)['"]\s*:\s*\[([^\]]+)\]"#;
    let entry_re = Regex::new(entry_pattern).ok()?;

    for entry_cap in entry_re.captures_iter(dict_content) {
        if let (Some(extra_name), Some(deps_str)) = (entry_cap.get(1), entry_cap.get(2)) {
            let deps = parse_setup_py_dep_list(deps_str.as_str(), extra_name.as_str(), true);
            all_deps.extend(deps);
        }
    }

    if all_deps.is_empty() {
        None
    } else {
        Some(all_deps)
    }
}

fn parse_setup_py_dep_list(deps_str: &str, scope: &str, is_optional: bool) -> Vec<Dependency> {
    let dep_pattern = r#"['"]([^'"]+)['"]"#;
    let re = match Regex::new(dep_pattern) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    re.captures_iter(deps_str)
        .filter_map(|cap| {
            let dep_str = cap.get(1)?.as_str().trim();
            if dep_str.is_empty() {
                return None;
            }

            let name = extract_setup_cfg_dependency_name(dep_str)?;
            let purl = PackageUrl::new(PythonParser::PACKAGE_TYPE.as_str(), &name).ok()?;

            Some(Dependency {
                purl: Some(purl.to_string()),
                extracted_requirement: Some(dep_str.to_string()),
                scope: Some(scope.to_string()),
                is_runtime: Some(true),
                is_optional: Some(is_optional),
                is_pinned: Some(false),
                is_direct: Some(true),
                resolved_package: None,
                extra_data: None,
            })
        })
        .collect()
}

/// Reads and parses a TOML file
pub(crate) fn read_toml_file(path: &Path) -> Result<TomlValue, String> {
    let content = read_file_to_string(path).map_err(|e| e.to_string())?;
    toml::from_str(&content).map_err(|e| format!("Failed to parse TOML: {}", e))
}

/// Calculates file size and SHA256 checksum for integrity verification in SBOMs.
///
/// Used for .whl and .egg archives to populate `size` and `sha256` fields in PackageData.
/// Essential for SBOM compliance and package integrity verification.
///
/// # Returns
///
/// - `(Some(size), Some(hash))` on success
/// - `(None, None)` if file cannot be opened
/// - `(Some(size), None)` if hash calculation fails during read
fn calculate_file_checksums(path: &Path) -> (Option<u64>, Option<Sha256Digest>) {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return (None, None),
    };

    let metadata = match file.metadata() {
        Ok(m) => m,
        Err(_) => return (None, None),
    };
    let size = metadata.len();

    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 8192];

    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => hasher.update(&buffer[..n]),
            Err(_) => return (Some(size), None),
        }
    }

    let hash = Sha256Digest::from_bytes(hasher.finalize().into());
    (Some(size), Some(hash))
}

fn default_package_data(path: &Path) -> PackageData {
    PackageData {
        package_type: Some(PythonParser::PACKAGE_TYPE),
        primary_language: Some("Python".to_string()),
        datasource_id: infer_python_datasource_id(path),
        ..Default::default()
    }
}

fn infer_python_datasource_id(path: &Path) -> Option<DatasourceId> {
    let file_name = path.file_name().and_then(|name| name.to_str());

    match file_name {
        Some("pyproject.toml") => {
            if read_toml_file(path)
                .ok()
                .and_then(|content| content.get("tool").and_then(|v| v.as_table()).cloned())
                .and_then(|tool| tool.get("poetry").and_then(|v| v.as_table()).cloned())
                .is_some()
            {
                Some(DatasourceId::PypiPoetryPyprojectToml)
            } else {
                Some(DatasourceId::PypiPyprojectToml)
            }
        }
        Some(name) if name == "setup.py" || name.ends_with("_setup.py") => {
            Some(DatasourceId::PypiSetupPy)
        }
        Some("setup.cfg") => Some(DatasourceId::PypiSetupCfg),
        Some("PKG-INFO") => Some(detect_pkg_info_datasource_id(path)),
        Some("METADATA") if is_installed_wheel_metadata_path(path) => {
            Some(DatasourceId::PypiWheelMetadata)
        }
        Some("pypi.json") => Some(DatasourceId::PypiJson),
        Some("pip-inspect.deplock") => Some(DatasourceId::PypiInspectDeplock),
        Some("origin.json") if is_pip_cache_origin_json(path) => {
            Some(DatasourceId::PypiPipOriginJson)
        }
        _ if file_name.is_some_and(is_likely_python_sdist_filename) => {
            Some(DatasourceId::PypiSdist)
        }
        _ if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("whl")) =>
        {
            Some(DatasourceId::PypiWheel)
        }
        _ if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("egg")) =>
        {
            Some(DatasourceId::PypiEgg)
        }
        _ => None,
    }
}

crate::register_parser!(
    "Python package manifests (pyproject.toml, setup.py, *_setup.py, setup.cfg, pypi.json, PKG-INFO, .dist-info/METADATA, pip cache origin.json, sdist archives, .whl, .egg)",
    &[
        "**/pyproject.toml",
        "**/setup.py",
        "**/*_setup.py",
        "**/setup.cfg",
        "**/pypi.json",
        "**/PKG-INFO",
        "**/*.dist-info/METADATA",
        "**/origin.json",
        "**/*.tar.gz",
        "**/*.tgz",
        "**/*.tar.bz2",
        "**/*.tar.xz",
        "**/*.zip",
        "**/*.whl",
        "**/*.egg"
    ],
    "pypi",
    "Python",
    Some("https://packaging.python.org/"),
);
