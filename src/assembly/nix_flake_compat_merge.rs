use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::models::{DatasourceId, FileInfo, Package, PackageData};

const FLAKE_COMPAT_WRAPPER_KIND: &str = "flake_compat";

pub fn attach_flake_compat_default_files(files: &mut [FileInfo], packages: &mut Vec<Package>) {
    let flake_packages_by_dir = collect_flake_packages_by_directory(packages);
    let wrapper_packages_by_path = collect_wrapper_packages_by_path(packages);

    let mut attachments = Vec::new();
    for (file_idx, file) in files.iter().enumerate() {
        let Some(package_data) = file
            .package_data
            .iter()
            .find(|package_data| is_flake_compat_wrapper(package_data))
            .cloned()
        else {
            continue;
        };

        let Some(directory) = Path::new(&file.path).parent().map(Path::to_path_buf) else {
            continue;
        };
        let Some(&package_idx) = flake_packages_by_dir.get(&directory) else {
            continue;
        };

        attachments.push((
            file_idx,
            package_idx,
            wrapper_packages_by_path.get(&file.path).copied(),
            package_data,
            file.path.clone(),
        ));
    }

    let mut removal_indices = Vec::new();

    for (file_idx, package_idx, wrapper_package_idx, package_data, file_path) in attachments {
        let package_uid = packages[package_idx].package_uid.clone();
        if !packages[package_idx].datafile_paths.contains(&file_path) {
            packages[package_idx].update(&package_data, file_path.clone());
        }

        files[file_idx]
            .for_packages
            .retain(|existing_uid| existing_uid == &package_uid);
        if !files[file_idx].for_packages.contains(&package_uid) {
            files[file_idx].for_packages.push(package_uid.clone());
        }

        if let Some(wrapper_package_idx) = wrapper_package_idx {
            removal_indices.push(wrapper_package_idx);
        }
    }

    removal_indices.sort_unstable();
    removal_indices.dedup();
    for removal_idx in removal_indices.into_iter().rev() {
        packages.remove(removal_idx);
    }
}

fn collect_flake_packages_by_directory(packages: &[Package]) -> HashMap<PathBuf, usize> {
    let mut flake_packages_by_dir = HashMap::new();

    for (idx, package) in packages.iter().enumerate() {
        if !package.datasource_ids.iter().any(|datasource_id| {
            matches!(
                datasource_id,
                DatasourceId::NixFlakeNix | DatasourceId::NixFlakeLock
            )
        }) {
            continue;
        }

        let Some(directory) = package
            .datafile_paths
            .iter()
            .find_map(|path| Path::new(path).parent().map(Path::to_path_buf))
        else {
            continue;
        };

        flake_packages_by_dir.entry(directory).or_insert(idx);
    }

    flake_packages_by_dir
}

fn collect_wrapper_packages_by_path(packages: &[Package]) -> HashMap<String, usize> {
    packages
        .iter()
        .enumerate()
        .filter(|(_, package)| is_flake_compat_wrapper_package(package))
        .filter_map(|(idx, package)| {
            package
                .datafile_paths
                .first()
                .cloned()
                .map(|path| (path, idx))
        })
        .collect()
}

fn is_flake_compat_wrapper(package_data: &PackageData) -> bool {
    package_data.datasource_id == Some(DatasourceId::NixDefaultNix)
        && package_data
            .extra_data
            .as_ref()
            .and_then(|extra_data| extra_data.get("nix_wrapper_kind"))
            .and_then(|kind| kind.as_str())
            == Some(FLAKE_COMPAT_WRAPPER_KIND)
}

fn is_flake_compat_wrapper_package(package: &Package) -> bool {
    package
        .datasource_ids
        .contains(&DatasourceId::NixDefaultNix)
        && package
            .extra_data
            .as_ref()
            .and_then(|extra_data| extra_data.get("nix_wrapper_kind"))
            .and_then(|kind| kind.as_str())
            == Some(FLAKE_COMPAT_WRAPPER_KIND)
}
