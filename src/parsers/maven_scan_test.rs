// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::scan_test_utils::scan_and_assemble;
    use crate::models::{DatasourceId, PackageType};

    #[test]
    fn test_maven_repository_pom_scan_assembles_package_from_repo_style_filename() {
        let (files, result) = scan_and_assemble(Path::new(
            "testdata/summarycode-golden/tallies/packages/scan/aopalliance",
        ));

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("aopalliance"))
            .expect("aopalliance package should be assembled");

        assert_eq!(package.package_type, Some(PackageType::Maven));
        assert_eq!(package.namespace.as_deref(), Some("aopalliance"));
        assert_eq!(package.version.as_deref(), Some("1.0"));
        assert_eq!(
            package.declared_license_expression.as_deref(),
            Some("public-domain")
        );

        let pom = files
            .iter()
            .find(|file| file.path.ends_with("/aopalliance-1.0.pom"))
            .expect("repository pom should be scanned");
        assert!(pom.for_packages.contains(&package.package_uid));
        assert!(
            pom.package_data
                .iter()
                .any(|pkg_data| pkg_data.datasource_id == Some(DatasourceId::MavenPom))
        );
    }
}
