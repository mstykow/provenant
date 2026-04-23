// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use glob::Pattern;
use std::collections::HashSet;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionFrontier {
    pub path: PathBuf,
    pub recurse: bool,
}

struct CollectionAccumulator {
    files: Vec<(PathBuf, fs::Metadata)>,
    directories: Vec<(PathBuf, fs::Metadata)>,
    file_seen: HashSet<PathBuf>,
    dir_seen: HashSet<PathBuf>,
    excluded_count: usize,
    total_file_bytes: u64,
    collection_errors: Vec<(PathBuf, String)>,
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

pub fn collect_selected_paths(
    root: &Path,
    selected: &[CollectionFrontier],
    max_depth: usize,
    exclude_patterns: &[Pattern],
) -> CollectedPaths {
    let depth_limit = depth_limit_from_cli(max_depth);

    if is_path_excluded(root, exclude_patterns) {
        return CollectedPaths {
            files: Vec::new(),
            directories: Vec::new(),
            excluded_count: 1,
            total_file_bytes: 0,
            collection_errors: Vec::new(),
        };
    }

    let root_metadata = match fs::metadata(root) {
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

    let mut accumulator = CollectionAccumulator {
        files: Vec::new(),
        directories: vec![(root.to_path_buf(), root_metadata)],
        file_seen: HashSet::new(),
        dir_seen: HashSet::from([root.to_path_buf()]),
        excluded_count: 0,
        total_file_bytes: 0,
        collection_errors: Vec::new(),
    };

    for frontier in minimize_frontier(selected) {
        let relative_depth = frontier.path.components().count();
        if depth_limit.is_some_and(|limit| relative_depth > limit) {
            continue;
        }

        let absolute = root.join(&frontier.path);
        if is_path_or_any_ancestor_excluded(root, &absolute, exclude_patterns) {
            accumulator.excluded_count += 1;
            continue;
        }

        let metadata = match fs::metadata(&absolute) {
            Ok(metadata) => metadata,
            Err(error) => {
                accumulator
                    .collection_errors
                    .push((absolute, error.to_string()));
                continue;
            }
        };

        add_ancestor_directories(root, &absolute, &mut accumulator);

        if metadata.is_file() {
            insert_file(&mut accumulator, absolute, metadata);
            continue;
        }

        if !metadata.is_dir() {
            continue;
        }

        let subtree_depth_limit = depth_limit.map(|limit| limit.saturating_sub(relative_depth));
        let collected = if frontier.recurse {
            collect_all_paths(&absolute, &metadata, subtree_depth_limit, exclude_patterns)
        } else {
            CollectedPaths {
                files: Vec::new(),
                directories: vec![(absolute, metadata)],
                excluded_count: 0,
                total_file_bytes: 0,
                collection_errors: Vec::new(),
            }
        };
        merge_collected(&mut accumulator, collected);
    }

    CollectedPaths {
        files: accumulator.files,
        directories: accumulator.directories,
        excluded_count: accumulator.excluded_count,
        total_file_bytes: accumulator.total_file_bytes,
        collection_errors: accumulator.collection_errors,
    }
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

fn is_path_or_any_ancestor_excluded(
    path_root: &Path,
    path: &Path,
    exclude_patterns: &[Pattern],
) -> bool {
    let mut current = Some(path);
    while let Some(candidate) = current {
        if is_path_excluded(candidate, exclude_patterns) {
            return true;
        }
        if candidate == path_root {
            break;
        }
        current = candidate.parent();
    }
    false
}

fn minimize_frontier(selected: &[CollectionFrontier]) -> Vec<CollectionFrontier> {
    let mut ordered = selected.to_vec();
    ordered.sort_by_key(|entry| (entry.path.components().count(), !entry.recurse));

    let mut minimized = Vec::new();
    for entry in ordered {
        let covered = minimized.iter().any(|existing: &CollectionFrontier| {
            existing.recurse
                && (entry.path == existing.path || entry.path.starts_with(&existing.path))
        });
        if !covered {
            minimized.push(entry);
        }
    }
    minimized
}

fn add_ancestor_directories(root: &Path, path: &Path, accumulator: &mut CollectionAccumulator) {
    let mut current = path.parent();
    while let Some(dir) = current {
        if dir == root {
            break;
        }
        if accumulator.dir_seen.insert(dir.to_path_buf()) {
            match fs::metadata(dir) {
                Ok(metadata) => accumulator.directories.push((dir.to_path_buf(), metadata)),
                Err(error) => accumulator
                    .collection_errors
                    .push((dir.to_path_buf(), error.to_string())),
            }
        }
        current = dir.parent();
    }
}

fn insert_file(accumulator: &mut CollectionAccumulator, path: PathBuf, metadata: fs::Metadata) {
    if accumulator.file_seen.insert(path.clone()) {
        accumulator.total_file_bytes += metadata.len();
        accumulator.files.push((path, metadata));
    }
}

fn merge_collected(accumulator: &mut CollectionAccumulator, collected: CollectedPaths) {
    accumulator.excluded_count += collected.excluded_count;
    accumulator
        .collection_errors
        .extend(collected.collection_errors);

    for (path, metadata) in collected.files {
        insert_file(accumulator, path, metadata);
    }
    for (path, metadata) in collected.directories {
        if accumulator.dir_seen.insert(path.clone()) {
            accumulator.directories.push((path, metadata));
        }
    }
}
