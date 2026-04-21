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
    fn test_chef_basic_scan_assembles_metadata_json_and_rb() {
        let (files, result) = scan_and_assemble(Path::new("testdata/chef/basic"));

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("301"))
            .expect("chef package should be assembled");

        assert_eq!(package.package_type, Some(PackageType::Chef));
        assert_eq!(package.version.as_deref(), Some("0.1.0"));
        assert_eq!(package.purl.as_deref(), Some("pkg:chef/301@0.1.0"));
        assert_dependency_present(&result.dependencies, "pkg:chef/nodejs", "metadata.rb");
        assert_file_links_to_package(
            &files,
            "/metadata.json",
            &package.package_uid,
            DatasourceId::ChefCookbookMetadataJson,
        );
        assert_file_links_to_package(
            &files,
            "/metadata.rb",
            &package.package_uid,
            DatasourceId::ChefCookbookMetadataRb,
        );
    }
}
