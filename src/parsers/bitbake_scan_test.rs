// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::scan_test_utils::{assert_dependency_present, scan_and_assemble};
    use crate::models::{DatasourceId, PackageType};

    #[test]
    fn test_bitbake_scan_assembles_recipe_and_bbappend_and_links_local_files() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let recipe_path = temp_dir.path().join("example_1.2.3.bb");
        let append_path = temp_dir.path().join("example_1.2.3.bbappend");
        let license_path = temp_dir.path().join("LICENSE");
        let patch_path = temp_dir.path().join("fix-build.patch");
        let append_patch_path = temp_dir.path().join("append.patch");

        fs::write(
            &recipe_path,
            r#"
SUMMARY = "Example recipe"
LICENSE = "MIT"
LIC_FILES_CHKSUM = "file://LICENSE;md5=d41d8cd98f00b204e9800998ecf8427e"
SRC_URI = "https://example.com/example-${PV}.tar.gz file://fix-build.patch"
DEPENDS = "zlib"
RDEPENDS:${PN} = "libz"
"#,
        )
        .expect("write recipe");
        fs::write(
            &append_path,
            r#"
DEPENDS:append = " openssl"
SRC_URI:append = " file://append.patch"
"#,
        )
        .expect("write bbappend");
        fs::write(&license_path, "").expect("write license");
        fs::write(&patch_path, "patch").expect("write patch");
        fs::write(&append_patch_path, "patch").expect("write append patch");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("example"))
            .expect("bitbake package should be assembled");

        assert_eq!(package.package_type, Some(PackageType::Bitbake));
        assert_eq!(package.version.as_deref(), Some("1.2.3"));
        assert_eq!(package.purl.as_deref(), Some("pkg:bitbake/example@1.2.3"));
        assert!(
            package
                .datafile_paths
                .iter()
                .any(|path| path.ends_with("/example_1.2.3.bb"))
        );
        assert!(
            package
                .datafile_paths
                .iter()
                .any(|path| path.ends_with("/example_1.2.3.bbappend"))
        );
        assert!(
            package
                .datasource_ids
                .contains(&DatasourceId::BitbakeRecipe)
        );
        assert!(
            package
                .datasource_ids
                .contains(&DatasourceId::BitbakeRecipeAppend)
        );

        assert_dependency_present(&result.dependencies, "pkg:bitbake/zlib", "example_1.2.3.bb");
        assert_dependency_present(
            &result.dependencies,
            "pkg:bitbake/openssl",
            "example_1.2.3.bbappend",
        );

        let recipe_file = files
            .iter()
            .find(|file| file.path.ends_with("/example_1.2.3.bb"))
            .expect("recipe should be scanned");
        assert!(
            recipe_file
                .package_data
                .iter()
                .any(|pkg_data| pkg_data.datasource_id == Some(DatasourceId::BitbakeRecipe))
        );
        assert!(recipe_file.for_packages.contains(&package.package_uid));

        let append_file = files
            .iter()
            .find(|file| file.path.ends_with("/example_1.2.3.bbappend"))
            .expect("append file should be scanned");
        assert!(
            append_file.package_data.iter().any(|pkg_data| {
                pkg_data.datasource_id == Some(DatasourceId::BitbakeRecipeAppend)
            })
        );
        assert!(append_file.for_packages.contains(&package.package_uid));

        for suffix in ["/LICENSE", "/fix-build.patch", "/append.patch"] {
            let file = files
                .iter()
                .find(|file| file.path.ends_with(suffix))
                .unwrap_or_else(|| panic!("{suffix} should be scanned"));
            assert!(
                file.for_packages.contains(&package.package_uid),
                "{suffix} should link to {} but had {:?}",
                package.package_uid,
                file.for_packages
            );
        }
    }
}
