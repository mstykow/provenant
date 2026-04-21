// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::scan_test_utils::{
        assert_dependency_present, assert_file_links_to_package, scan_and_assemble,
    };
    use crate::models::{DatasourceId, PackageType};

    #[test]
    fn test_hackage_basic_scan_assembles_multi_file_package() {
        let (files, result) =
            scan_and_assemble(Path::new("testdata/assembly-golden/hackage-basic"));

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("aaa-example-hackage"))
            .expect("hackage package should be assembled");

        assert_eq!(package.package_type, Some(PackageType::Hackage));
        assert_eq!(package.version.as_deref(), Some("0.1.0.0"));
        assert_eq!(
            package.purl.as_deref(),
            Some("pkg:hackage/aaa-example-hackage@0.1.0.0")
        );
        assert_dependency_present(
            &result.dependencies,
            "pkg:hackage/base",
            "aaa-example-hackage.cabal",
        );
        assert_dependency_present(
            &result.dependencies,
            "pkg:hackage/aeson@2.2.1.0",
            "stack.yaml",
        );
        assert_file_links_to_package(
            &files,
            "/aaa-example-hackage.cabal",
            &package.package_uid,
            DatasourceId::HackageCabal,
        );
        assert_file_links_to_package(
            &files,
            "/cabal.project",
            &package.package_uid,
            DatasourceId::HackageCabalProject,
        );
        assert_file_links_to_package(
            &files,
            "/stack.yaml",
            &package.package_uid,
            DatasourceId::HackageStackYaml,
        );
    }
}
