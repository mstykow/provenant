// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Result, anyhow};
use glob::Pattern;
use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::FileInfo;
use crate::scanner::CollectedPaths;

use super::apply_path_selection_filter;

#[cfg(test)]
#[path = "selection_test.rs"]
mod selection_test;

pub(crate) fn resolve_native_scan_inputs(inputs: &[String]) -> Result<(String, Vec<String>)> {
    if inputs.is_empty() {
        return Err(anyhow!("No directory input path provided"));
    }

    if inputs.len() == 1 {
        return Ok((inputs[0].clone(), Vec::new()));
    }

    if inputs.iter().any(|path| Path::new(path).is_absolute()) {
        return Err(anyhow!(
            "Invalid inputs: all input paths must be relative when using multiple inputs"
        ));
    }

    let common_prefix = common_path_prefix(inputs)
        .unwrap_or_else(|| PathBuf::from("."))
        .to_string_lossy()
        .to_string();
    if common_prefix != "." && !Path::new(&common_prefix).is_dir() {
        return Err(anyhow!(
            "Invalid inputs: all input paths must share a common single parent directory"
        ));
    }

    let synthetic_includes = inputs
        .iter()
        .map(|path| path.replace('\\', "/").trim_end_matches('/').to_string())
        .collect();

    Ok((common_prefix, synthetic_includes))
}

#[derive(Debug)]
pub(crate) struct ResolvedPathsFileEntries {
    pub includes: Vec<String>,
    pub missing_entries: Vec<String>,
}

pub(crate) fn resolve_paths_file_entries(
    scan_root: &Path,
    entries: &[String],
) -> Result<ResolvedPathsFileEntries> {
    let root_metadata = fs::metadata(scan_root).map_err(|err| {
        anyhow!(
            "Failed to access scan root {:?} for --paths-file: {err}",
            scan_root
        )
    })?;
    if !root_metadata.is_dir() {
        return Err(anyhow!(
            "--paths-file requires the positional scan root to be a directory: {:?}",
            scan_root
        ));
    }

    let mut includes = Vec::new();
    let mut missing_entries = Vec::new();
    let mut seen = HashSet::new();

    for entry in entries {
        let Some(normalized) = normalize_paths_file_entry(entry)? else {
            continue;
        };

        if scan_root.join(&normalized).exists() {
            if seen.insert(normalized.clone()) {
                includes.push(normalized);
            }
        } else if seen.insert(format!("missing:{normalized}")) {
            missing_entries.push(normalized);
        }
    }

    Ok(ResolvedPathsFileEntries {
        includes,
        missing_entries,
    })
}

fn normalize_paths_file_entry(entry: &str) -> Result<Option<String>> {
    let entry = entry.trim_end_matches('\r');
    if entry.trim().is_empty() {
        return Ok(None);
    }

    let path = Path::new(entry);
    if path.is_absolute() {
        return Err(anyhow!(
            "--paths-file entries must be relative to the declared scan root: {entry:?}"
        ));
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::Normal(segment) => normalized.push(segment),
            std::path::Component::ParentDir => {
                if !normalized.pop() {
                    return Err(anyhow!(
                        "--paths-file entry escapes the declared scan root: {entry:?}"
                    ));
                }
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err(anyhow!(
                    "--paths-file entries must be relative to the declared scan root: {entry:?}"
                ));
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        return Err(anyhow!(
            "--paths-file entries must name a file or directory under the declared scan root: {entry:?}"
        ));
    }

    let normalized = normalized
        .components()
        .map(|component| OsString::from(component.as_os_str()))
        .collect::<PathBuf>()
        .to_string_lossy()
        .replace('\\', "/");

    Ok(Some(normalized))
}

pub(crate) fn common_path_prefix(inputs: &[String]) -> Option<PathBuf> {
    let first = inputs.first()?;
    let mut shared_components: Vec<_> = Path::new(first).components().collect();

    for input in &inputs[1..] {
        let components: Vec<_> = Path::new(input).components().collect();
        let shared_len = shared_components
            .iter()
            .zip(components.iter())
            .take_while(|(left, right)| left == right)
            .count();
        shared_components.truncate(shared_len);
        if shared_components.is_empty() {
            break;
        }
    }

    if shared_components.is_empty() {
        None
    } else {
        let mut prefix = PathBuf::new();
        for component in shared_components {
            prefix.push(component.as_os_str());
        }
        Some(prefix)
    }
}

pub(crate) fn apply_user_path_filters_to_collected(
    collected: &mut CollectedPaths,
    scan_root: &Path,
    include_patterns: &[String],
    exclude_patterns: &[String],
) -> usize {
    let before_files = collected.files.len();
    let before_dirs = collected.directories.len();
    collected.files.retain(|(path, _)| {
        let relative_path = normalize_scan_relative_path(path, scan_root);
        is_included_path(&relative_path, include_patterns, exclude_patterns)
    });

    let kept_file_paths: HashSet<_> = collected
        .files
        .iter()
        .map(|(path, _)| path.clone())
        .collect();
    collected.directories.retain(|(path, _)| {
        let relative_path = normalize_scan_relative_path(path, scan_root);
        is_included_path(&relative_path, include_patterns, exclude_patterns)
            || kept_file_paths
                .iter()
                .any(|file_path| file_path.starts_with(path))
    });

    (before_files - collected.files.len()) + (before_dirs - collected.directories.len())
}

pub(crate) fn apply_cli_path_selection_filter(
    files: &mut Vec<FileInfo>,
    include_patterns: &[String],
    exclude_patterns: &[String],
) {
    apply_path_selection_filter(files, |file| {
        is_included_path(&file.path, include_patterns, exclude_patterns)
    });
}

pub(crate) fn normalize_scan_relative_path(path: &Path, scan_root: &Path) -> String {
    let normalized = path
        .strip_prefix(scan_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");

    if normalized.is_empty() && path == scan_root {
        if scan_root.is_file() || (!scan_root.exists() && scan_root.extension().is_some()) {
            scan_root
                .file_name()
                .map(|name| name.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default()
        } else {
            String::new()
        }
    } else {
        normalized
    }
}

pub(crate) fn is_included_path(
    path: &str,
    include_patterns: &[String],
    exclude_patterns: &[String],
) -> bool {
    if path.trim().is_empty() {
        return false;
    }

    let normalized_path = path.replace('\\', "/").to_ascii_lowercase();
    let stripped_path = normalized_path.trim_start_matches(['/', '0']).to_string();

    if !include_patterns.is_empty()
        && !include_patterns
            .iter()
            .filter(|pattern| !pattern.trim().is_empty())
            .any(|pattern| path_matches_scancode_pattern(pattern, &normalized_path, &stripped_path))
    {
        return false;
    }

    !exclude_patterns
        .iter()
        .filter(|pattern| !pattern.trim().is_empty())
        .any(|pattern| path_matches_scancode_pattern(pattern, &normalized_path, &stripped_path))
}

fn path_matches_scancode_pattern(
    pattern: &str,
    normalized_path: &str,
    stripped_path: &str,
) -> bool {
    let normalized_pattern = pattern.trim_start_matches('/').to_ascii_lowercase();
    let Ok(compiled) = Pattern::new(&normalized_pattern) else {
        return false;
    };

    if !normalized_pattern.contains('/') {
        stripped_path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .any(|segment| compiled.matches(segment))
    } else {
        matching_path_candidates(normalized_path, stripped_path)
            .iter()
            .any(|candidate| compiled.matches(candidate))
    }
}

fn matching_path_candidates<'a>(normalized_path: &'a str, stripped_path: &'a str) -> Vec<&'a str> {
    let mut candidates = Vec::new();

    for path in [normalized_path, stripped_path] {
        if path.is_empty() {
            continue;
        }

        candidates.push(path);
        let mut current = path;
        while let Some((parent, _)) = current.rsplit_once('/') {
            if parent.is_empty() {
                break;
            }
            candidates.push(parent);
            current = parent;
        }
    }

    candidates
}
