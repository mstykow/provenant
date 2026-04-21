// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::scan_test_utils::scan_and_assemble;
    use crate::models::{DatasourceId, PackageType};

    #[test]
    fn test_cran_description_scan_assembles_package_and_hoists_dependencies() {
        let (files, result) = scan_and_assemble(Path::new("testdata/cran/geometry"));

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("geometry"))
            .expect("geometry package should be assembled");

        assert_eq!(package.package_type, Some(PackageType::Cran));
        assert_eq!(package.version.as_deref(), Some("0.4.2"));
        assert_eq!(package.purl.as_deref(), Some("pkg:cran/geometry@0.4.2"));
        assert!(result.dependencies.iter().any(|dep| {
            dep.purl.as_deref() == Some("pkg:cran/magic")
                && dep.for_package_uid.as_deref() == Some(package.package_uid.as_str())
        }));
        let desc = files
            .iter()
            .find(|file| file.path.ends_with("/DESCRIPTION"))
            .expect("DESCRIPTION should be scanned");
        assert!(desc.for_packages.contains(&package.package_uid));
        assert!(
            desc.package_data
                .iter()
                .any(|pkg_data| pkg_data.datasource_id == Some(DatasourceId::CranDescription))
        );
    }
}
