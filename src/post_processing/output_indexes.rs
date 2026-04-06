use std::collections::HashMap;

use crate::models::FileInfo;

use super::package_file_index::PackageFileIndex;
use super::{FileIx, PackageIx};

#[derive(Default)]
pub(super) struct OutputIndexes {
    first_file_index_by_path: HashMap<String, FileIx>,
    key_file_indices_by_package_ix: HashMap<PackageIx, Vec<FileIx>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum OutputIndexMode {
    KeyFilesOnly,
    Full,
}

impl OutputIndexMode {
    fn includes_path_index(self) -> bool {
        matches!(self, Self::Full)
    }
}

impl OutputIndexes {
    pub(super) fn build(
        files: &[FileInfo],
        package_file_index: Option<&PackageFileIndex>,
        use_fallback_key_classification: bool,
        mode: OutputIndexMode,
    ) -> Self {
        let mut indexes = Self::default();

        for (idx, file) in files.iter().enumerate() {
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
        }

        indexes
    }

    pub(super) fn file_ix_by_path(&self, path: &str) -> Option<FileIx> {
        self.first_file_index_by_path.get(path).copied()
    }

    pub(super) fn key_file_indices_for_package(&self, package_ix: PackageIx) -> Option<&[FileIx]> {
        self.key_file_indices_by_package_ix
            .get(&package_ix)
            .map(Vec::as_slice)
    }
}
