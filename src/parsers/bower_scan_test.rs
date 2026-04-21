// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::scan_test_utils::scan_and_assemble;
    use crate::models::{DatasourceId, PackageType};

    #[test]
    fn test_bower_scan_assembles_package_and_hoists_dependencies() {
        let (files, result) = scan_and_assemble(Path::new("testdata/bower/basic"));

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("blue-leaf"))
            .expect("bower package should be assembled");

        assert_eq!(package.package_type, Some(PackageType::Bower));
        assert!(result.dependencies.iter().any(|dep| {
            dep.purl.as_deref() == Some("pkg:bower/get-size")
                && dep.for_package_uid.as_deref() == Some(package.package_uid.as_str())
        }));
        assert!(result.dependencies.iter().any(|dep| {
            dep.purl.as_deref() == Some("pkg:bower/qunit")
                && dep.for_package_uid.as_deref() == Some(package.package_uid.as_str())
        }));
        let bower_json = files
            .iter()
            .find(|file| file.path.ends_with("/bower.json"))
            .expect("bower.json should be scanned");
        assert!(bower_json.for_packages.contains(&package.package_uid));
        assert!(
            bower_json
                .package_data
                .iter()
                .any(|pkg_data| pkg_data.datasource_id == Some(DatasourceId::BowerJson))
        );
    }
}
