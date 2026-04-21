// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::scan_test_utils::{assert_dependency_present, scan_and_assemble};
    use crate::models::DatasourceId;

    #[test]
    fn test_carthage_scan_hoists_purl_dependencies_without_assembling_root_package() {
        let (files, result) = scan_and_assemble(Path::new("testdata/carthage"));

        assert!(result.packages.is_empty());
        assert_eq!(result.dependencies.len(), 10);
        assert!(
            result
                .dependencies
                .iter()
                .all(|dependency| dependency.purl.is_some())
        );
        assert!(
            result
                .dependencies
                .iter()
                .all(|dependency| dependency.for_package_uid.is_none())
        );

        assert_dependency_present(
            &result.dependencies,
            "pkg:github/reactivecocoa/reactivecocoa",
            "Cartfile",
        );
        assert_dependency_present(
            &result.dependencies,
            "pkg:github/quick/quick",
            "Cartfile.private",
        );
        assert_dependency_present(
            &result.dependencies,
            "pkg:github/reactivecocoa/reactivecocoa@v2.3.1",
            "Cartfile.resolved",
        );

        let cartfile = files
            .iter()
            .find(|file| file.path.ends_with("/Cartfile"))
            .expect("Cartfile should be scanned");
        let cartfile_private = files
            .iter()
            .find(|file| file.path.ends_with("/Cartfile.private"))
            .expect("Cartfile.private should be scanned");
        let cartfile_resolved = files
            .iter()
            .find(|file| file.path.ends_with("/Cartfile.resolved"))
            .expect("Cartfile.resolved should be scanned");

        assert!(cartfile.for_packages.is_empty());
        assert!(cartfile_private.for_packages.is_empty());
        assert!(cartfile_resolved.for_packages.is_empty());

        assert!(cartfile.package_data.iter().any(|package_data| {
            package_data.datasource_id == Some(DatasourceId::CarthageCartfile)
                && !package_data.is_private
        }));
        assert!(cartfile_private.package_data.iter().any(|package_data| {
            package_data.datasource_id == Some(DatasourceId::CarthageCartfile)
                && package_data.is_private
        }));
        assert!(cartfile_resolved.package_data.iter().any(|package_data| {
            package_data.datasource_id == Some(DatasourceId::CarthageCartfileResolved)
                && !package_data.is_private
        }));
    }
}
