// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::scan_test_utils::scan_and_assemble;
    use crate::models::{DatasourceId, PackageType};

    #[test]
    fn test_haxe_scan_assembles_package_and_hoists_dependencies() {
        let (files, result) = scan_and_assemble(Path::new("testdata/haxe/deps"));

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("selecthxml"))
            .expect("haxe package should be assembled");

        assert_eq!(package.package_type, Some(PackageType::Haxe));
        assert_eq!(package.version.as_deref(), Some("0.5.1"));
        assert!(result.dependencies.iter().any(|dep| {
            dep.purl.as_deref() == Some("pkg:haxe/tink_core")
                && dep.for_package_uid.as_deref() == Some(package.package_uid.as_str())
        }));
        assert!(result.dependencies.iter().any(|dep| {
            dep.purl.as_deref() == Some("pkg:haxe/tink_macro@3.23")
                && dep.for_package_uid.as_deref() == Some(package.package_uid.as_str())
        }));
        let haxelib = files
            .iter()
            .find(|file| file.path.ends_with("/haxelib.json"))
            .expect("haxelib.json should be scanned");
        assert!(haxelib.for_packages.contains(&package.package_uid));
        assert!(
            haxelib
                .package_data
                .iter()
                .any(|pkg_data| pkg_data.datasource_id == Some(DatasourceId::HaxelibJson))
        );
    }
}
