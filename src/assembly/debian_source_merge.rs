// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use crate::models::{DatasourceId, FileInfo, Package, PackageData, TopLevelDependency};

use super::{AssemblerConfig, DirectoryMergeOutput};

pub(super) fn assemble_debian_source_packages(
    config: &AssemblerConfig,
    files: &[FileInfo],
    file_indices: &[usize],
) -> Vec<DirectoryMergeOutput> {
    let mut control_entries: Vec<(usize, String, PackageData)> = Vec::new();
    let mut copyright_entries: Vec<(usize, String, PackageData)> = Vec::new();
    let mut affected_indices = BTreeSet::new();

    for &idx in file_indices {
        let file = &files[idx];

        for pkg_data in &file.package_data {
            let Some(datasource_id) = pkg_data.datasource_id else {
                continue;
            };

            if !config.datasource_ids.contains(&datasource_id) {
                continue;
            }

            match datasource_id {
                DatasourceId::DebianControlInSource => {
                    control_entries.push((idx, file.path.clone(), pkg_data.clone()));
                    affected_indices.insert(idx);
                }
                DatasourceId::DebianCopyrightInSource => {
                    copyright_entries.push((idx, file.path.clone(), pkg_data.clone()));
                    affected_indices.insert(idx);
                }
                _ => {}
            }
        }
    }

    if control_entries.is_empty() {
        return Vec::new();
    }

    let affected_indices: Vec<usize> = affected_indices.into_iter().collect();

    control_entries
        .into_iter()
        .filter_map(|(_, datafile_path, pkg_data)| {
            let datasource_id = pkg_data.datasource_id?;
            pkg_data.purl.as_ref()?;

            let mut package = Package::from_package_data(&pkg_data, datafile_path.clone());
            for (_, copyright_path, copyright_pkg_data) in &copyright_entries {
                package.update(copyright_pkg_data, copyright_path.clone());
            }

            let for_package_uid = Some(package.package_uid.clone());
            let dependencies = pkg_data
                .dependencies
                .iter()
                .filter(|dependency| dependency.purl.is_some())
                .map(|dependency| {
                    TopLevelDependency::from_dependency(
                        dependency,
                        datafile_path.clone(),
                        datasource_id,
                        for_package_uid.clone(),
                    )
                })
                .collect();

            Some((Some(package), dependencies, affected_indices.clone()))
        })
        .collect()
}
