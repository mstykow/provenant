// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use crate::models::FileInfo;

use super::{AssemblerConfig, DirectoryMergeOutput, sibling_merge};

pub(super) fn assemble_windows_update_packages(
    config: &AssemblerConfig,
    files: &[FileInfo],
    file_indices: &[usize],
) -> Vec<DirectoryMergeOutput> {
    let mut ordered_indices = file_indices.to_vec();
    if let Some(update_mum_position) = ordered_indices.iter().position(|&idx| {
        Path::new(&files[idx].path)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("update.mum"))
    }) {
        let update_mum_idx = ordered_indices.remove(update_mum_position);
        ordered_indices.insert(0, update_mum_idx);
    }

    sibling_merge::assemble_siblings(config, files, &ordered_indices)
}
