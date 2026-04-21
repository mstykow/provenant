// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::scan_test_utils::{assert_file_links_to_package, scan_and_assemble};
    use crate::models::{DatasourceId, PackageType};

    #[test]
    fn test_conan_manifest_scan_assembles_recipe_and_conandata() {
        let (files, result) =
            scan_and_assemble(Path::new("testdata/conan/recipes/libzip/manifest"));

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("libzip"))
            .expect("libzip conan package should be assembled");

        assert_eq!(package.package_type, Some(PackageType::Conan));
        assert_eq!(
            package.declared_license_expression_spdx.as_deref(),
            Some("BSD-3-Clause")
        );
        assert!(
            package
                .purl
                .as_deref()
                .is_some_and(|purl| purl.starts_with("pkg:conan/libzip@"))
        );
        assert_file_links_to_package(
            &files,
            "/conanfile.py",
            &package.package_uid,
            DatasourceId::ConanConanFilePy,
        );
        assert_file_links_to_package(
            &files,
            "/conandata.yml",
            &package.package_uid,
            DatasourceId::ConanConanDataYml,
        );
    }
}
