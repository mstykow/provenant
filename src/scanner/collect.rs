// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use glob::Pattern;
use std::fs;
use std::path::{Path, PathBuf};

use crate::utils::file::is_path_excluded;

pub struct CollectedPaths {
    pub files: Vec<(PathBuf, fs::Metadata)>,
    pub directories: Vec<(PathBuf, fs::Metadata)>,
    pub excluded_count: usize,
    pub total_file_bytes: u64,
    pub collection_errors: Vec<(PathBuf, String)>,
}

impl CollectedPaths {
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn directory_count(&self) -> usize {
        self.directories.len()
    }
}

pub fn collect_paths<P: AsRef<Path>>(
    root: P,
    max_depth: usize,
    exclude_patterns: &[Pattern],
) -> CollectedPaths {
    let depth_limit = depth_limit_from_cli(max_depth);
    let root = root.as_ref();

    if is_path_excluded(root, exclude_patterns) {
        return CollectedPaths {
            files: Vec::new(),
            directories: Vec::new(),
            excluded_count: 1,
            total_file_bytes: 0,
            collection_errors: Vec::new(),
        };
    }

    let metadata = match fs::metadata(root) {
        Ok(metadata) => metadata,
        Err(error) => {
            return CollectedPaths {
                files: Vec::new(),
                directories: Vec::new(),
                excluded_count: 0,
                total_file_bytes: 0,
                collection_errors: vec![(root.to_path_buf(), error.to_string())],
            };
        }
    };

    if metadata.is_file() {
        return CollectedPaths {
            total_file_bytes: metadata.len(),
            files: vec![(root.to_path_buf(), metadata)],
            directories: Vec::new(),
            excluded_count: 0,
            collection_errors: Vec::new(),
        };
    }

    collect_all_paths(root, &metadata, depth_limit, exclude_patterns)
}

fn collect_all_paths(
    root: &Path,
    root_metadata: &fs::Metadata,
    depth_limit: Option<usize>,
    exclude_patterns: &[Pattern],
) -> CollectedPaths {
    let mut files = Vec::new();
    let mut directories = vec![(root.to_path_buf(), root_metadata.clone())];
    let mut excluded_count = 0;
    let mut total_file_bytes = 0_u64;
    let mut collection_errors = Vec::new();

    let mut pending_dirs: Vec<(PathBuf, Option<usize>)> = vec![(root.to_path_buf(), depth_limit)];

    while let Some((dir_path, current_depth)) = pending_dirs.pop() {
        let entries: Vec<_> = match fs::read_dir(&dir_path) {
            Ok(entries) => entries.filter_map(Result::ok).collect(),
            Err(e) => {
                collection_errors.push((dir_path.clone(), e.to_string()));
                continue;
            }
        };

        for entry in entries {
            let path = entry.path();

            if is_path_excluded(&path, exclude_patterns) {
                excluded_count += 1;
                continue;
            }

            match entry.metadata() {
                Ok(metadata) if metadata.is_file() => {
                    total_file_bytes += metadata.len();
                    files.push((path, metadata));
                }
                Ok(metadata) if metadata.is_dir() => {
                    directories.push((path.clone(), metadata));
                    let should_recurse = current_depth.is_none_or(|d| d > 0);
                    if should_recurse {
                        let next_depth = current_depth.map(|d| d - 1);
                        pending_dirs.push((path, next_depth));
                    }
                }
                _ => continue,
            }
        }
    }

    CollectedPaths {
        files,
        directories,
        excluded_count,
        total_file_bytes,
        collection_errors,
    }
}

fn depth_limit_from_cli(max_depth: usize) -> Option<usize> {
    if max_depth == 0 {
        None
    } else {
        Some(max_depth)
    }
}
