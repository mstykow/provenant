use std::collections::HashMap;

use rayon::prelude::*;

use crate::models::FileInfo;

use super::package_file_index::{FileIx, PackageFileIndex, PackageIx};

#[derive(Default)]
pub(crate) struct OutputIndexes {
    first_file_index_by_path: HashMap<String, FileIx>,
    key_file_indices_by_package_ix: HashMap<PackageIx, Vec<FileIx>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputIndexMode {
    KeyFilesOnly,
    Full,
}

impl OutputIndexMode {
    fn includes_path_index(self) -> bool {
        matches!(self, Self::Full)
    }
}

impl OutputIndexes {
    pub(crate) fn build(
        files: &[FileInfo],
        package_file_index: Option<&PackageFileIndex>,
        use_fallback_key_classification: bool,
        mode: OutputIndexMode,
    ) -> Self {
        let mut indexes = files
            .par_iter()
            .enumerate()
            .fold(Self::default, |mut indexes, (idx, file)| {
                let file_ix = FileIx(idx);

                if mode.includes_path_index() {
                    indexes
                        .first_file_index_by_path
                        .entry(file.path.clone())
                        .or_insert(file_ix);
                }

                let is_key_file = package_file_index.is_some_and(|index| {
                    index.is_key_file(files, file_ix, use_fallback_key_classification)
                });

                if is_key_file {
                    for package_ix in package_file_index
                        .into_iter()
                        .flat_map(|index| index.package_ixs_for_file(file_ix))
                    {
                        indexes
                            .key_file_indices_by_package_ix
                            .entry(*package_ix)
                            .or_default()
                            .push(file_ix);
                    }
                }

                indexes
            })
            .reduce(Self::default, |mut left, mut right| {
                for (path, file_ix) in right.first_file_index_by_path.drain() {
                    left.first_file_index_by_path
                        .entry(path)
                        .and_modify(|existing| {
                            if file_ix.0 < existing.0 {
                                *existing = file_ix;
                            }
                        })
                        .or_insert(file_ix);
                }

                for (package_ix, mut file_indices) in right.key_file_indices_by_package_ix.drain() {
                    left.key_file_indices_by_package_ix
                        .entry(package_ix)
                        .or_default()
                        .append(&mut file_indices);
                }

                left
            });

        for file_indices in indexes.key_file_indices_by_package_ix.values_mut() {
            file_indices.sort_by_key(|file_ix| file_ix.0);
        }

        indexes
    }

    pub(crate) fn file_ix_by_path(&self, path: &str) -> Option<FileIx> {
        self.first_file_index_by_path.get(path).copied()
    }

    pub(crate) fn key_file_indices_for_package(&self, package_ix: PackageIx) -> Option<&[FileIx]> {
        self.key_file_indices_by_package_ix
            .get(&package_ix)
            .map(Vec::as_slice)
    }
}
