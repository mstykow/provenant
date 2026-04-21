// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::models::{FileInfo, FileType, Package};
use crate::utils::path::{parent_dir, parent_dir_for_lookup};

use super::classification::{
    FileClassification, is_community_file, is_legal_file, is_manifest_file, is_readme_file,
};
use super::{FileIx, PackageIx, lowest_common_parent_path, package_root};

#[derive(Clone, Copy, Debug, Default)]
struct FileFlags {
    is_manifest_candidate: bool,
    is_legal_candidate: bool,
    is_readme_candidate: bool,
    is_community_candidate: bool,
}

pub(super) struct PackageFileIndex {
    file_ix_by_path: HashMap<String, FileIx>,
    file_package_ixs: Vec<Vec<PackageIx>>,
    file_flags: Vec<FileFlags>,
    scan_top_level_flags: Vec<bool>,
    package_data_top_level_flags: Vec<bool>,
    referenced_flags: Vec<bool>,
    root_top_level_flags: Vec<bool>,
}

impl PackageFileIndex {
    pub(super) fn build(files: &[FileInfo], packages: &[Package]) -> Self {
        let package_ix_by_uid: HashMap<_, _> = packages
            .iter()
            .enumerate()
            .map(|(idx, package)| (package.package_uid.clone(), PackageIx(idx)))
            .collect();
        let file_ix_by_path: HashMap<_, _> = files
            .iter()
            .enumerate()
            .map(|(idx, file)| (file.path.clone(), FileIx(idx)))
            .collect();

        let file_package_ixs: Vec<Vec<PackageIx>> = files
            .iter()
            .map(|file| {
                file.for_packages
                    .iter()
                    .filter_map(|package_uid| package_ix_by_uid.get(package_uid).copied())
                    .collect()
            })
            .collect();

        let package_root_dir_by_ix: Vec<Option<String>> = packages
            .iter()
            .map(|package| package_root(package).map(|root| root.to_string_lossy().into_owned()))
            .collect();

        let mut package_referenced_file_ixs_by_ix = vec![HashSet::new(); packages.len()];
        for (file_ix, file) in files.iter().enumerate() {
            if file.package_data.is_empty() || file_package_ixs[file_ix].is_empty() {
                continue;
            }

            for package_ix in &file_package_ixs[file_ix] {
                let refs = &mut package_referenced_file_ixs_by_ix[package_ix.0];
                for package_data in &file.package_data {
                    for file_ref in &package_data.file_references {
                        if let Some(target_ix) = file_ix_by_path.get(&file_ref.path).copied() {
                            refs.insert(target_ix);
                        }
                    }
                }
            }
        }

        let scan_roots = build_scan_roots(files);
        let scan_root_ancestors = build_scan_root_ancestors(&scan_roots);
        let package_data_top_level_dirs = build_package_data_top_level_dirs(files);

        let mut file_flags = Vec::with_capacity(files.len());
        let mut scan_top_level_flags = Vec::with_capacity(files.len());
        let mut package_data_top_level_flags = Vec::with_capacity(files.len());
        let mut referenced_flags = Vec::with_capacity(files.len());
        let mut root_top_level_flags = Vec::with_capacity(files.len());

        for (idx, file) in files.iter().enumerate() {
            let file_ix = FileIx(idx);
            let is_manifest_candidate =
                !file.package_data.is_empty() || is_manifest_file(&file.path);
            let path = file.path.as_str();
            let path_parent = parent_dir(path);

            file_flags.push(FileFlags {
                is_manifest_candidate,
                is_legal_candidate: file.file_type == FileType::File && is_legal_file(file),
                is_readme_candidate: file.file_type == FileType::File && is_readme_file(file),
                is_community_candidate: file.file_type == FileType::File && is_community_file(file),
            });

            scan_top_level_flags.push(is_scan_top_level(path, &scan_roots, &scan_root_ancestors));

            package_data_top_level_flags.push(if file.file_type == FileType::Directory {
                package_data_top_level_dirs.contains(path)
            } else {
                (!file.package_data.is_empty()
                    && !file_package_ixs[idx].is_empty()
                    && is_manifest_candidate)
                    || package_data_top_level_dirs.contains(path_parent)
            });

            referenced_flags.push(file_package_ixs[idx].iter().any(|package_ix| {
                package_referenced_file_ixs_by_ix[package_ix.0].contains(&file_ix)
            }));

            root_top_level_flags.push(
                (file.file_type != FileType::File || file.package_data.is_empty())
                    && file_package_ixs[idx].iter().any(|package_ix| {
                        package_root_dir_by_ix[package_ix.0].as_deref() == Some(path_parent)
                    }),
            );
        }

        Self {
            file_ix_by_path,
            file_package_ixs,
            file_flags,
            scan_top_level_flags,
            package_data_top_level_flags,
            referenced_flags,
            root_top_level_flags,
        }
    }

    pub(super) fn is_key_file(
        &self,
        files: &[FileInfo],
        file_ix: FileIx,
        use_fallback_key_classification: bool,
    ) -> bool {
        if use_fallback_key_classification {
            self.classify_file(files, file_ix).is_key_file
        } else {
            files[file_ix.0].is_key_file
        }
    }

    pub(super) fn package_ixs_for_file(&self, file_ix: FileIx) -> &[PackageIx] {
        &self.file_package_ixs[file_ix.0]
    }

    pub(super) fn file_ix_by_path(&self, path: &str) -> Option<FileIx> {
        self.file_ix_by_path.get(path).copied()
    }

    pub(super) fn classify_file(&self, files: &[FileInfo], file_ix: FileIx) -> FileClassification {
        let file = &files[file_ix.0];
        let flags = self.file_flags[file_ix.0];
        let is_top_level = self.scan_top_level_flags[file_ix.0]
            || self.referenced_flags[file_ix.0]
            || self.root_top_level_flags[file_ix.0]
            || self.package_data_top_level_flags[file_ix.0];

        FileClassification {
            is_legal: flags.is_legal_candidate,
            is_manifest: file.file_type == FileType::File && flags.is_manifest_candidate,
            is_readme: flags.is_readme_candidate,
            is_top_level,
            is_key_file: file.file_type == FileType::File
                && is_top_level
                && (flags.is_legal_candidate
                    || flags.is_manifest_candidate
                    || flags.is_readme_candidate),
            is_community: flags.is_community_candidate,
        }
    }
}

fn build_package_data_top_level_dirs(files: &[FileInfo]) -> HashSet<String> {
    let mut top_level_dirs = HashSet::new();

    for file in files.iter().filter(|file| {
        file.file_type == FileType::File
            && !file.package_data.is_empty()
            && !file.for_packages.is_empty()
    }) {
        let parent = parent_dir(&file.path);
        if parent.is_empty() || !parent.contains('/') {
            continue;
        }

        top_level_dirs.insert(parent.to_string());
        insert_nonempty_ancestors(parent, &mut top_level_dirs);
    }

    top_level_dirs
}

fn build_scan_roots(files: &[FileInfo]) -> HashSet<String> {
    let parent_dirs: Vec<PathBuf> = files
        .iter()
        .filter(|file| file.file_type == FileType::File)
        .map(|file| {
            Path::new(&file.path)
                .parent()
                .unwrap_or_else(|| Path::new(""))
        })
        .map(Path::to_path_buf)
        .collect();

    let mut roots: Vec<PathBuf> = if parent_dirs.iter().any(|path| path.as_os_str().is_empty()) {
        vec![PathBuf::new()]
    } else {
        lowest_common_parent_path(&parent_dirs)
            .into_iter()
            .collect()
    };

    if roots.is_empty() {
        for file in files {
            let mut components = Path::new(&file.path).components();
            let Some(first) = components.next() else {
                continue;
            };

            let root = PathBuf::from(first.as_os_str());
            if !roots.contains(&root) {
                roots.push(root);
            }
        }
    }

    roots
        .into_iter()
        .map(|root| root.to_string_lossy().into_owned())
        .collect()
}

fn build_scan_root_ancestors(scan_roots: &HashSet<String>) -> HashSet<String> {
    let mut ancestors = HashSet::new();

    for root in scan_roots {
        insert_nonempty_ancestors(root, &mut ancestors);
    }

    ancestors
}

fn insert_nonempty_ancestors(path: &str, ancestors: &mut HashSet<String>) {
    let mut current = parent_dir_for_lookup(path);

    while let Some(candidate) = current {
        if candidate.is_empty() {
            break;
        }

        ancestors.insert(candidate.to_string());
        current = parent_dir_for_lookup(candidate);
    }
}

fn is_scan_top_level(
    path: &str,
    scan_roots: &HashSet<String>,
    scan_root_ancestors: &HashSet<String>,
) -> bool {
    if !path.contains('/') {
        return true;
    }

    scan_roots.contains(path)
        || scan_root_ancestors.contains(path)
        || scan_roots.contains(parent_dir(path))
}
