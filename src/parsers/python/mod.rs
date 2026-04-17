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

enum PythonFileKind {
    PyprojectToml,
    SetupCfg,
    SetupPy,
    PkgInfo,
    WheelMetadata,
    PipOriginJson,
    PypiJson,
    PipInspectDeplock,
    SdistArchive,
    WheelArchive,
    EggArchive,
}

fn classify_python_file(path: &Path) -> Option<PythonFileKind> {
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    Some(match filename {
        "pyproject.toml" => PythonFileKind::PyprojectToml,
        "setup.cfg" => PythonFileKind::SetupCfg,
        _ if is_setup_py_like_path(path) => PythonFileKind::SetupPy,
        "PKG-INFO" => PythonFileKind::PkgInfo,
        "METADATA" if is_installed_wheel_metadata_path(path) => PythonFileKind::WheelMetadata,
        "pypi.json" => PythonFileKind::PypiJson,
        "pip-inspect.deplock" => PythonFileKind::PipInspectDeplock,
        _ => {
            if archive::is_pip_cache_origin_json(path) {
                PythonFileKind::PipOriginJson
            } else if archive::is_python_sdist_archive_path(path) {
                PythonFileKind::SdistArchive
            } else if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("whl"))
                && archive::is_valid_wheel_archive_path(path)
            {
                PythonFileKind::WheelArchive
            } else if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("egg"))
            {
                PythonFileKind::EggArchive
            } else {
                return None;
            }
        }
    })
}

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
        match classify_python_file(path) {
            Some(PythonFileKind::SetupPy) => setup_py::extract_setup_py_packages(path),
            Some(kind) => vec![match kind {
                PythonFileKind::PyprojectToml => pyproject::extract_from_pyproject_toml(path),
                PythonFileKind::SetupCfg => setup_cfg::extract_from_setup_cfg(path),
                PythonFileKind::PkgInfo => rfc822_meta::extract_from_rfc822_metadata(
                    path,
                    utils::detect_pkg_info_datasource_id(path),
                ),
                PythonFileKind::WheelMetadata => {
                    rfc822_meta::extract_from_rfc822_metadata(path, DatasourceId::PypiWheelMetadata)
                }
                PythonFileKind::PipOriginJson => archive::extract_from_pip_origin_json(path),
                PythonFileKind::PypiJson => pypi_json::extract_from_pypi_json(path),
                PythonFileKind::PipInspectDeplock => pypi_json::extract_from_pip_inspect(path),
                PythonFileKind::SdistArchive => archive::extract_from_sdist_archive(path),
                PythonFileKind::WheelArchive => archive::extract_from_wheel_archive(path),
                PythonFileKind::EggArchive => archive::extract_from_egg_archive(path),
                PythonFileKind::SetupPy => unreachable!(),
            }],
            None => vec![utils::default_package_data(path)],
        }
    }

    fn is_match(path: &Path) -> bool {
        classify_python_file(path).is_some()
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
