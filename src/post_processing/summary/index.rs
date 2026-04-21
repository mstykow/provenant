// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use crate::models::{FileInfo, FileType, Package};
use crate::utils::path::{parent_dir, parent_dir_for_lookup};

use super::super::output_indexes::OutputIndexes;
use super::super::{FileIx, PackageIx, is_score_key_file, package_root};

pub(super) struct SummaryIndex {
    summary_origin_package_ixs: Vec<PackageIx>,
    selected_package_ixs: Vec<PackageIx>,
    file_matches_summary_origin_package: Vec<bool>,
    file_matches_selected_package: Vec<bool>,
    file_under_nested_root: Vec<bool>,
    summary_score_key_file_flags: Vec<bool>,
}

impl SummaryIndex {
    pub(super) fn build(files: &[FileInfo], packages: &[Package], indexes: &OutputIndexes) -> Self {
        let package_roots: Vec<Option<String>> = packages
            .iter()
            .map(|package| package_root(package).map(|root| root.to_string_lossy().into_owned()))
            .collect();
        let top_level_roots = build_top_level_root_lookup(&package_roots);
        let summary_origin_package_ixs =
            build_summary_origin_package_ixs(packages, files, &package_roots, &top_level_roots);
        let selected_package_ixs =
            build_selected_package_ixs(packages, files, indexes, &summary_origin_package_ixs);
        let summary_origin_package_uids: HashSet<&str> = summary_origin_package_ixs
            .iter()
            .map(|package_ix| packages[package_ix.0].package_uid.as_str())
            .collect();
        let selected_package_uids: HashSet<&str> = selected_package_ixs
            .iter()
            .map(|package_ix| packages[package_ix.0].package_uid.as_str())
            .collect();
        let nested_roots = build_nested_root_lookup(files, &package_roots, &top_level_roots);
        let file_matches_summary_origin_package: Vec<bool> = files
            .iter()
            .map(|file| file_matches_package_uids(file, &summary_origin_package_uids))
            .collect();
        let file_matches_selected_package: Vec<bool> = files
            .iter()
            .map(|file| file_matches_package_uids(file, &selected_package_uids))
            .collect();
        let file_under_nested_root: Vec<bool> = files
            .iter()
            .map(|file| path_is_within_any_root(file.path.as_str(), &nested_roots))
            .collect();
        let summary_score_key_file_flags: Vec<bool> = files
            .iter()
            .zip(file_under_nested_root.iter())
            .map(|(file, under_nested_root)| {
                file.file_type == FileType::File
                    && file.is_top_level
                    && is_score_key_file(file)
                    && !*under_nested_root
            })
            .collect();

        Self {
            summary_origin_package_ixs,
            selected_package_ixs,
            file_matches_summary_origin_package,
            file_matches_selected_package,
            file_under_nested_root,
            summary_score_key_file_flags,
        }
    }

    pub(super) fn selected_packages<'a>(
        &'a self,
        packages: &'a [Package],
    ) -> impl Iterator<Item = &'a Package> + 'a {
        self.selected_package_ixs
            .iter()
            .filter_map(|package_ix| packages.get(package_ix.0))
    }

    pub(super) fn summary_origin_packages<'a>(
        &'a self,
        packages: &'a [Package],
    ) -> impl Iterator<Item = &'a Package> + 'a {
        self.summary_origin_package_ixs
            .iter()
            .filter_map(|package_ix| packages.get(package_ix.0))
    }

    pub(super) fn matches_summary_origin_package(&self, file_ix: FileIx) -> bool {
        self.file_matches_summary_origin_package[file_ix.0]
    }

    pub(super) fn matches_selected_package(&self, file_ix: FileIx) -> bool {
        self.file_matches_selected_package[file_ix.0]
    }

    pub(super) fn is_under_nested_root(&self, file_ix: FileIx) -> bool {
        self.file_under_nested_root[file_ix.0]
    }

    pub(super) fn is_summary_score_key_file(&self, file_ix: FileIx) -> bool {
        self.summary_score_key_file_flags[file_ix.0]
    }
}

fn build_summary_origin_package_ixs(
    packages: &[Package],
    files: &[FileInfo],
    package_roots: &[Option<String>],
    top_level_roots: &HashSet<String>,
) -> Vec<PackageIx> {
    if packages.is_empty() {
        return Vec::new();
    }

    if top_level_roots.is_empty() {
        return (0..packages.len()).map(PackageIx).collect();
    }

    let summary_origin_package_ixs: Vec<PackageIx> = package_roots
        .iter()
        .enumerate()
        .filter_map(|(package_ix, root)| {
            root.as_ref()
                .is_some_and(|root| top_level_roots.contains(root))
                .then_some(PackageIx(package_ix))
        })
        .collect();

    if summary_origin_package_ixs.is_empty() && !files.is_empty() {
        (0..packages.len()).map(PackageIx).collect()
    } else {
        summary_origin_package_ixs
    }
}

fn build_selected_package_ixs(
    packages: &[Package],
    files: &[FileInfo],
    indexes: &OutputIndexes,
    summary_origin_package_ixs: &[PackageIx],
) -> Vec<PackageIx> {
    let selected_package_ixs: Vec<PackageIx> = summary_origin_package_ixs
        .iter()
        .copied()
        .filter(|package_ix| {
            packages[package_ix.0]
                .datafile_paths
                .iter()
                .any(|datafile_path| {
                    indexes
                        .file_ix_by_path(datafile_path)
                        .and_then(|index| files.get(index.0))
                        .is_some_and(|file| file.file_type == FileType::File)
                })
        })
        .collect();

    if selected_package_ixs.is_empty() {
        summary_origin_package_ixs.to_vec()
    } else {
        selected_package_ixs
    }
}

fn build_top_level_root_lookup(package_roots: &[Option<String>]) -> HashSet<String> {
    let mut roots: Vec<String> = package_roots.iter().flatten().cloned().collect();
    roots.sort_by(|left, right| {
        path_component_count(left)
            .cmp(&path_component_count(right))
            .then_with(|| left.cmp(right))
    });
    roots.dedup();

    let mut top_level_roots = HashSet::new();
    for root in roots {
        if path_has_parent_root(&root, &top_level_roots) {
            continue;
        }
        top_level_roots.insert(root);
    }

    top_level_roots
}

fn build_nested_root_lookup(
    files: &[FileInfo],
    package_roots: &[Option<String>],
    top_level_roots: &HashSet<String>,
) -> HashSet<String> {
    let mut nested_roots: HashSet<String> = package_roots
        .iter()
        .flatten()
        .filter(|root| {
            !top_level_roots.contains(root.as_str()) && path_has_parent_root(root, top_level_roots)
        })
        .cloned()
        .collect();

    nested_roots.extend(
        files
            .iter()
            .filter(|file| {
                file.file_type == FileType::File && file.is_manifest && !file.is_top_level
            })
            .map(|file| parent_dir(&file.path).to_string()),
    );

    nested_roots
}

fn file_matches_package_uids(file: &FileInfo, package_uids: &HashSet<&str>) -> bool {
    file.for_packages.is_empty()
        || package_uids.is_empty()
        || file
            .for_packages
            .iter()
            .any(|package_uid| package_uids.contains(package_uid.as_str()))
}

fn path_component_count(path: &str) -> usize {
    if path.is_empty() {
        0
    } else {
        path.bytes().filter(|byte| *byte == b'/').count() + 1
    }
}

fn path_has_parent_root(path: &str, roots: &HashSet<String>) -> bool {
    if !path.is_empty() && roots.contains("") {
        return true;
    }

    let mut current = parent_dir_for_lookup(path);
    while let Some(candidate) = current {
        if roots.contains(candidate) {
            return true;
        }
        current = parent_dir_for_lookup(candidate);
    }

    false
}

fn path_is_within_any_root(path: &str, roots: &HashSet<String>) -> bool {
    if roots.contains("") {
        return true;
    }

    let mut current = Some(path);
    while let Some(candidate) = current {
        if roots.contains(candidate) {
            return true;
        }
        current = parent_dir_for_lookup(candidate);
    }

    false
}
