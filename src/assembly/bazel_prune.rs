// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use crate::models::{FileInfo, Package, PackageType, PackageUid, TopLevelDependency};

pub(super) fn prune_unused_bazel_packages(
    files: &[FileInfo],
    packages: &mut Vec<Package>,
    dependencies: &mut Vec<TopLevelDependency>,
) {
    let used_package_uids: HashSet<&str> = files
        .iter()
        .flat_map(|file| file.for_packages.iter().map(|uid| uid.as_str()))
        .collect();

    let removed_package_uids: HashSet<PackageUid> = packages
        .iter()
        .filter(|package| package.package_type == Some(PackageType::Bazel))
        .filter(|package| !used_package_uids.contains(package.package_uid.as_str()))
        .map(|package| package.package_uid.clone())
        .collect();

    if removed_package_uids.is_empty() {
        return;
    }

    packages.retain(|package| !removed_package_uids.contains(&package.package_uid));
    dependencies.retain(|dependency| {
        dependency
            .for_package_uid
            .as_ref()
            .is_none_or(|package_uid| !removed_package_uids.contains(package_uid))
    });
}
