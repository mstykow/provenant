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
    fn test_pixi_basic_scan_assembles_manifest_and_lockfile() {
        let (files, result) = scan_and_assemble(Path::new("testdata/assembly-golden/pixi-basic"));

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("pixi-demo"))
            .expect("pixi package should be assembled");

        assert_eq!(package.package_type, Some(PackageType::Pixi));
        assert_eq!(package.version.as_deref(), Some("1.2.3"));
        assert_eq!(package.purl.as_deref(), Some("pkg:pixi/pixi-demo@1.2.3"));
        assert_dependency_present(&result.dependencies, "pkg:conda/python", "pixi.toml");
        assert_dependency_present(&result.dependencies, "pkg:conda/python@3.12.7", "pixi.lock");
        assert_file_links_to_package(
            &files,
            "/pixi.toml",
            &package.package_uid,
            DatasourceId::PixiToml,
        );
        assert_file_links_to_package(
            &files,
            "/pixi.lock",
            &package.package_uid,
            DatasourceId::PixiLock,
        );
    }
}
