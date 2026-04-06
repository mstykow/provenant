use anyhow::{Result, anyhow};
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::assembly;
use crate::models::{
    FileInfo, FileType, LicenseReference, LicenseRuleReference, Package, TopLevelDependency,
    TopLevelLicenseDetection,
};
use crate::scanner::ProcessResult;

use super::{normalize_paths, normalize_top_level_output_paths};

#[cfg(test)]
#[path = "json_input_test.rs"]
mod json_input_test;

#[derive(Deserialize)]
pub(crate) struct JsonHeaderInput {
    #[serde(default)]
    errors: Vec<String>,
}

#[derive(Deserialize)]
pub(crate) struct JsonScanInput {
    #[serde(default)]
    pub(crate) headers: Vec<JsonHeaderInput>,
    #[serde(default)]
    pub(crate) files: Vec<FileInfo>,
    #[serde(default)]
    pub(crate) packages: Vec<Package>,
    #[serde(default)]
    pub(crate) dependencies: Vec<TopLevelDependency>,
    #[serde(default)]
    pub(crate) license_detections: Vec<TopLevelLicenseDetection>,
    #[serde(default)]
    pub(crate) license_references: Vec<LicenseReference>,
    #[serde(default)]
    pub(crate) license_rule_references: Vec<LicenseRuleReference>,
    #[serde(default)]
    pub(crate) excluded_count: usize,
}

impl JsonScanInput {
    pub(crate) fn directory_count(&self) -> usize {
        self.files
            .iter()
            .filter(|file| file.file_type == FileType::Directory)
            .count()
    }

    pub(crate) fn file_count(&self) -> usize {
        self.files
            .iter()
            .filter(|file| file.file_type == FileType::File)
            .count()
    }

    pub(crate) fn file_size_count(&self) -> u64 {
        self.files
            .iter()
            .filter(|file| file.file_type == FileType::File)
            .map(|file| file.size)
            .sum()
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        ProcessResult,
        assembly::AssemblyResult,
        Vec<TopLevelLicenseDetection>,
        Vec<LicenseReference>,
        Vec<LicenseRuleReference>,
        Vec<String>,
    ) {
        (
            ProcessResult {
                files: self.files,
                excluded_count: self.excluded_count,
            },
            assembly::AssemblyResult {
                packages: self.packages,
                dependencies: self.dependencies,
            },
            self.license_detections,
            self.license_references,
            self.license_rule_references,
            self.headers
                .into_iter()
                .flat_map(|header| header.errors)
                .collect(),
        )
    }
}

pub(crate) fn load_and_merge_json_inputs(
    input_paths: &[String],
    strip_root: bool,
    full_root: bool,
) -> Result<JsonScanInput> {
    let mut merged: Option<JsonScanInput> = None;
    for input_path in input_paths {
        let mut loaded = load_scan_from_json(input_path)?;
        if strip_root || full_root {
            normalize_loaded_json_scan(&mut loaded, strip_root, full_root);
        }

        if let Some(acc) = &mut merged {
            acc.files.append(&mut loaded.files);
            acc.packages.append(&mut loaded.packages);
            acc.dependencies.append(&mut loaded.dependencies);
            acc.license_detections
                .append(&mut loaded.license_detections);
            acc.license_references
                .append(&mut loaded.license_references);
            acc.license_rule_references
                .append(&mut loaded.license_rule_references);
            acc.headers.append(&mut loaded.headers);
            acc.excluded_count += loaded.excluded_count;
        } else {
            merged = Some(loaded);
        }
    }

    merged.ok_or_else(|| anyhow!("No input paths provided"))
}

pub(crate) fn load_scan_from_json(path: &str) -> Result<JsonScanInput> {
    let input_path = Path::new(path);
    if !input_path.is_file() {
        return Err(anyhow!("--from-json input must be a valid file: {}", path));
    }

    let content = fs::read_to_string(input_path)?;
    let parsed: JsonScanInput = serde_json::from_str(&content)
        .map_err(|e| anyhow!("Input JSON scan file is not valid JSON: {path}: {e}"))?;

    Ok(parsed)
}

pub(crate) fn normalize_loaded_json_scan(
    loaded: &mut JsonScanInput,
    strip_root: bool,
    full_root: bool,
) {
    let original_paths: Vec<String> = loaded.files.iter().map(|file| file.path.clone()).collect();

    if let Some(scan_root) = derive_json_scan_root(&loaded.files)
        && strip_root
    {
        normalize_paths(&mut loaded.files, &scan_root, true, false);
        normalize_loaded_top_level_detection_paths(loaded, &scan_root, true, false);
        normalize_top_level_output_paths(
            &mut loaded.packages,
            &mut loaded.dependencies,
            &scan_root,
            true,
        );
    }

    if full_root {
        trim_loaded_json_full_root_paths(loaded);
    }

    normalize_loaded_header_errors(loaded, &original_paths);
}

fn derive_json_scan_root(files: &[FileInfo]) -> Option<String> {
    let mut directories: Vec<&str> = files
        .iter()
        .filter(|file| file.file_type == FileType::Directory)
        .map(|file| file.path.as_str())
        .collect();
    directories.sort_by_key(|path| (path.matches('/').count(), path.len()));
    if let Some(root_dir) = directories.first() {
        return Some((*root_dir).to_string());
    }

    if files.len() == 1 {
        return files.first().map(|file| file.path.clone());
    }

    let paths: Vec<String> = files.iter().map(|file| file.path.clone()).collect();
    super::selection::common_path_prefix(&paths).map(|path| path.to_string_lossy().to_string())
}

fn trim_loaded_json_full_root_paths(loaded: &mut JsonScanInput) {
    for file in &mut loaded.files {
        trim_full_root_display_value(&mut file.path);
        for detection_match in &mut file.license_clues {
            if let Some(from_file) = detection_match.from_file.as_mut() {
                trim_full_root_display_value(from_file);
            }
        }
        for detection in &mut file.license_detections {
            for detection_match in &mut detection.matches {
                if let Some(from_file) = detection_match.from_file.as_mut() {
                    trim_full_root_display_value(from_file);
                }
            }
        }
        for package_data in &mut file.package_data {
            for file_reference in &mut package_data.file_references {
                trim_full_root_display_value(&mut file_reference.path);
            }
            for detection in &mut package_data.license_detections {
                for detection_match in &mut detection.matches {
                    if let Some(from_file) = detection_match.from_file.as_mut() {
                        trim_full_root_display_value(from_file);
                    }
                }
            }
            for detection in &mut package_data.other_license_detections {
                for detection_match in &mut detection.matches {
                    if let Some(from_file) = detection_match.from_file.as_mut() {
                        trim_full_root_display_value(from_file);
                    }
                }
            }
        }
    }

    for package in &mut loaded.packages {
        for datafile_path in &mut package.datafile_paths {
            trim_full_root_display_value(datafile_path);
        }
    }
    for dependency in &mut loaded.dependencies {
        trim_full_root_display_value(&mut dependency.datafile_path);
    }

    normalize_loaded_top_level_detection_paths(loaded, "", false, true);
}

fn trim_full_root_display_value(path: &mut String) {
    *path = path.replace('\\', "/").trim_matches('/').to_string();
}

fn normalize_loaded_top_level_detection_paths(
    loaded: &mut JsonScanInput,
    scan_root: &str,
    strip_root: bool,
    full_root: bool,
) {
    for detection in &mut loaded.license_detections {
        for detection_match in &mut detection.reference_matches {
            if let Some(from_file) = detection_match.from_file.as_mut() {
                if strip_root
                    && let Some(normalized) =
                        normalize_loaded_detection_path(from_file, scan_root, true, false)
                {
                    *from_file = normalized;
                }
                if full_root
                    && let Some(normalized) =
                        normalize_loaded_detection_path(from_file, scan_root, false, true)
                {
                    *from_file = normalized;
                }
            }
        }
    }
}

fn normalize_loaded_header_errors(loaded: &mut JsonScanInput, original_paths: &[String]) {
    let mut replacements: Vec<_> = original_paths
        .iter()
        .zip(loaded.files.iter().map(|file| file.path.as_str()))
        .filter(|(before, after)| before.as_str() != *after)
        .map(|(before, after)| (before.as_str(), after))
        .collect();
    replacements.sort_by(|left, right| right.0.len().cmp(&left.0.len()));

    for header in &mut loaded.headers {
        for error in &mut header.errors {
            for (before, after) in &replacements {
                if error.ends_with(before) {
                    let prefix_len = error.len() - before.len();
                    error.replace_range(prefix_len.., after);
                    break;
                }
            }
        }
    }
}

fn normalize_loaded_detection_path(
    path: &str,
    scan_root: &str,
    strip_root: bool,
    full_root: bool,
) -> Option<String> {
    let current_path = PathBuf::from(path);

    if full_root {
        let absolute_candidate = if current_path.is_absolute() {
            current_path.clone()
        } else {
            env::current_dir()
                .map(|cwd| cwd.join(&current_path))
                .unwrap_or(current_path.clone())
        };
        let absolute = absolute_candidate
            .canonicalize()
            .unwrap_or(absolute_candidate);
        return Some(
            absolute
                .to_string_lossy()
                .replace('\\', "/")
                .trim_matches('/')
                .to_string(),
        );
    }

    if strip_root {
        let scan_root_path = Path::new(scan_root);
        let strip_base = if scan_root_path.is_file() {
            scan_root_path.parent().unwrap_or_else(|| Path::new(""))
        } else {
            scan_root_path
        };

        if current_path == scan_root_path
            && let Some(file_name) = scan_root_path.file_name().and_then(|name| name.to_str())
        {
            return Some(file_name.to_string());
        }

        if let Ok(stripped) = current_path.strip_prefix(strip_base)
            && !stripped.as_os_str().is_empty()
        {
            return Some(stripped.to_string_lossy().to_string());
        }
    }

    None
}
