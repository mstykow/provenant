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
    fn test_helm_basic_scan_assembles_chart_and_lockfile() {
        let (files, result) = scan_and_assemble(Path::new("testdata/assembly-golden/helm-basic"));

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("nginx"))
            .expect("helm chart should be assembled");

        assert_eq!(package.package_type, Some(PackageType::Helm));
        assert_eq!(package.version.as_deref(), Some("22.1.1"));
        assert_eq!(package.purl.as_deref(), Some("pkg:helm/nginx@22.1.1"));
        assert_dependency_present(&result.dependencies, "pkg:helm/common", "Chart.yaml");
        assert_dependency_present(&result.dependencies, "pkg:helm/common@2.31.4", "Chart.lock");
        assert_file_links_to_package(
            &files,
            "/Chart.yaml",
            &package.package_uid,
            DatasourceId::HelmChartYaml,
        );
        assert_file_links_to_package(
            &files,
            "/Chart.lock",
            &package.package_uid,
            DatasourceId::HelmChartLock,
        );
    }
}
