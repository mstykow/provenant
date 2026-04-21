// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use crate::models::{FileInfo, Package, TopLevelDependency};

use super::{AssemblerConfig, DirectoryMergeOutput, sibling_merge};

const SCANCODE_SIMPLE_TOP_LEVEL_KEY: &str = "scancode_simple_top_level";

pub(super) fn assemble_bazel_packages(
    config: &AssemblerConfig,
    files: &[FileInfo],
    file_indices: &[usize],
) -> Vec<DirectoryMergeOutput> {
    if should_emit_one_package_per_target(files, file_indices) {
        let mut results = Vec::new();

        for &idx in file_indices {
            let file = &files[idx];
            for pkg_data in &file.package_data {
                let dsid_matches = pkg_data
                    .datasource_id
                    .is_some_and(|dsid| config.datasource_ids.contains(&dsid));
                if !dsid_matches || pkg_data.purl.is_none() {
                    continue;
                }

                let datafile_path = file.path.clone();
                let datasource_id = pkg_data.datasource_id.expect("datasource_id must be Some");
                let pkg = Package::from_package_data(pkg_data, datafile_path.clone());
                let for_package_uid = Some(pkg.package_uid.clone());
                let deps = pkg_data
                    .dependencies
                    .iter()
                    .filter(|dep| dep.purl.is_some())
                    .map(|dep| {
                        TopLevelDependency::from_dependency(
                            dep,
                            datafile_path.clone(),
                            datasource_id,
                            for_package_uid.clone(),
                        )
                    })
                    .collect();
                results.push((Some(pkg), deps, vec![idx]));
            }
        }

        results
    } else {
        sibling_merge::assemble_siblings(config, files, file_indices)
    }
}

fn should_emit_one_package_per_target(files: &[FileInfo], file_indices: &[usize]) -> bool {
    file_indices.iter().any(|&idx| {
        files[idx].package_data.iter().any(|pkg_data| {
            pkg_data.extra_data.as_ref().is_some_and(|extra_data| {
                extra_data
                    .get(SCANCODE_SIMPLE_TOP_LEVEL_KEY)
                    .and_then(|value| value.as_bool())
                    == Some(true)
            })
        })
    })
}
