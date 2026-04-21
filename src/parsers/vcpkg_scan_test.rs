// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::scan_test_utils::{assert_dependency_present, scan_and_assemble};
    use crate::models::DatasourceId;

    #[test]
    fn test_vcpkg_scan_remains_unassembled_and_hoists_dependencies() {
        let (files, result) = scan_and_assemble(Path::new("testdata/vcpkg/project"));

        assert!(result.packages.is_empty());
        assert_dependency_present(&result.dependencies, "pkg:generic/vcpkg/fmt", "vcpkg.json");
        assert_dependency_present(
            &result.dependencies,
            "pkg:generic/vcpkg/cpprestsdk",
            "vcpkg.json",
        );
        assert!(
            result
                .dependencies
                .iter()
                .all(|dep| dep.for_package_uid.is_none())
        );
        let manifest = files
            .iter()
            .find(|file| file.path.ends_with("/vcpkg.json"))
            .expect("vcpkg.json should be scanned");
        assert!(manifest.for_packages.is_empty());
        assert!(
            manifest
                .package_data
                .iter()
                .any(|pkg_data| pkg_data.datasource_id == Some(DatasourceId::VcpkgJson))
        );
    }
}
