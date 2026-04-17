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

mod archive;
mod pypi_json;
mod pyproject;
mod rfc822_meta;
mod setup_cfg;
mod setup_py;
mod utils;

#[cfg(test)]
mod scan_test;
#[cfg(test)]
mod test;

use super::PackageParser;
use crate::models::{DatasourceId, PackageData, PackageType};
use std::path::Path;

pub(crate) use self::utils::build_pypi_urls;
#[cfg(test)]
pub(crate) use self::utils::extract_requires_dist_dependencies;
pub(crate) use self::utils::read_toml_file;

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

impl PackageParser for PythonParser {
    const PACKAGE_TYPE: PackageType = PackageType::Pypi;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        vec![
            if path.file_name().unwrap_or_default() == "pyproject.toml" {
                pyproject::extract_from_pyproject_toml(path)
            } else if path.file_name().unwrap_or_default() == "setup.cfg" {
                setup_cfg::extract_from_setup_cfg(path)
            } else if is_setup_py_like_path(path) {
                return setup_py::extract_setup_py_packages(path);
            } else if path.file_name().unwrap_or_default() == "PKG-INFO" {
                rfc822_meta::extract_from_rfc822_metadata(
                    path,
                    utils::detect_pkg_info_datasource_id(path),
                )
            } else if is_installed_wheel_metadata_path(path) {
                rfc822_meta::extract_from_rfc822_metadata(path, DatasourceId::PypiWheelMetadata)
            } else if archive::is_pip_cache_origin_json(path) {
                archive::extract_from_pip_origin_json(path)
            } else if path.file_name().unwrap_or_default() == "pypi.json" {
                pypi_json::extract_from_pypi_json(path)
            } else if path.file_name().unwrap_or_default() == "pip-inspect.deplock" {
                pypi_json::extract_from_pip_inspect(path)
            } else if archive::is_python_sdist_archive_path(path) {
                archive::extract_from_sdist_archive(path)
            } else if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("whl"))
            {
                archive::extract_from_wheel_archive(path)
            } else if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("egg"))
            {
                archive::extract_from_egg_archive(path)
            } else {
                utils::default_package_data(path)
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
                || archive::is_pip_cache_origin_json(path))
        {
            return true;
        }

        if let Some(extension) = path.extension() {
            let ext = extension.to_string_lossy().to_lowercase();
            if (ext == "whl" && archive::is_valid_wheel_archive_path(path))
                || ext == "egg"
                || archive::is_python_sdist_archive_path(path)
            {
                return true;
            }
        }

        false
    }
}

fn is_setup_py_like_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name == "setup.py" || name.ends_with("_setup.py") || name.ends_with("-setup.py")
        })
}

pub(super) fn is_installed_wheel_metadata_path(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some("METADATA")
        && path
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".dist-info"))
}

crate::register_parser!(
    "Python package manifests (pyproject.toml, setup.py, suffixed setup.py variants, setup.cfg, pypi.json, PKG-INFO, .dist-info/METADATA, pip cache origin.json, sdist archives, .whl, .egg)",
    &[
        "**/pyproject.toml",
        "**/setup.py",
        "**/*_setup.py",
        "**/*-setup.py",
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
