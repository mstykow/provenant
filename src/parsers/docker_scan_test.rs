// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::scan_test_utils::scan_and_assemble;
    use crate::models::{DatasourceId, PackageType};

    #[test]
    fn test_containerfile_scan_keeps_package_data_unassembled() {
        let (files, result) = scan_and_assemble(Path::new("testdata/docker-golden/pulp"));

        assert!(result.packages.is_empty());
        assert!(result.dependencies.is_empty());

        let containerfile = files
            .iter()
            .find(|file| file.path.ends_with("Containerfile"))
            .expect("Containerfile should be scanned");

        assert!(containerfile.for_packages.is_empty());
        assert_eq!(containerfile.package_data.len(), 1);

        let package = &containerfile.package_data[0];
        assert_eq!(package.package_type, Some(PackageType::Docker));
        assert_eq!(package.datasource_id, Some(DatasourceId::Dockerfile));
        assert_eq!(package.name.as_deref(), Some("Pulp OCI image"));
    }
}
