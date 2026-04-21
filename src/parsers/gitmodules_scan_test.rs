// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::scan_test_utils::{assert_dependency_present, scan_and_assemble};
    use crate::models::DatasourceId;

    #[test]
    fn test_gitmodules_scan_keeps_manifest_unassembled_and_hoists_known_dependencies() {
        let (files, result) = scan_and_assemble(Path::new("testdata/gitmodules"));

        assert!(result.packages.is_empty());
        assert_eq!(result.dependencies.len(), 3);
        assert_dependency_present(
            &result.dependencies,
            "pkg:github/example/dep1",
            ".gitmodules",
        );
        assert_dependency_present(&result.dependencies, "pkg:github/org/lib2", ".gitmodules");
        assert_dependency_present(
            &result.dependencies,
            "pkg:gitlab/company/project",
            ".gitmodules",
        );
        assert!(
            result
                .dependencies
                .iter()
                .all(|dep| dep.for_package_uid.is_none())
        );

        let gitmodules = files
            .iter()
            .find(|file| file.path.ends_with("/.gitmodules"))
            .expect(".gitmodules should be scanned");
        assert!(gitmodules.for_packages.is_empty());
        assert!(
            gitmodules
                .package_data
                .iter()
                .any(|pkg_data| pkg_data.datasource_id == Some(DatasourceId::Gitmodules))
        );
    }
}
