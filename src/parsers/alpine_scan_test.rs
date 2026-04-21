// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::super::scan_test_utils::scan_and_assemble;
    use crate::models::DatasourceId;

    #[test]
    fn test_alpine_installed_db_scan_assigns_referenced_rootfs_files() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let installed_path = temp_dir.path().join("lib/apk/db/installed");
        let etc_dir = temp_dir.path().join("etc");

        fs::create_dir_all(installed_path.parent().unwrap()).expect("create installed db parent");
        fs::create_dir_all(&etc_dir).expect("create etc dir");
        fs::copy(
            Path::new("testdata/alpine/lib/apk/db/installed"),
            &installed_path,
        )
        .expect("copy alpine installed fixture");
        fs::write(etc_dir.join("fstab"), "# generated during test\n").expect("write fstab");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("alpine-baselayout-data"))
            .expect("alpine-baselayout-data package should be assembled");
        let fstab = files
            .iter()
            .find(|file| file.path.ends_with("/etc/fstab"))
            .expect("fstab should be scanned");
        let installed = files
            .iter()
            .find(|file| file.path.ends_with("/lib/apk/db/installed"))
            .expect("installed db should be scanned");

        assert!(fstab.for_packages.contains(&package.package_uid));
        assert!(
            installed.package_data.iter().any(|pkg_data| {
                pkg_data.datasource_id == Some(DatasourceId::AlpineInstalledDb)
            })
        );
    }
}
