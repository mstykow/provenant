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
    fn test_composer_basic_scan_assembles_manifest_and_lockfile() {
        let (files, result) =
            scan_and_assemble(Path::new("testdata/assembly-golden/composer-basic"));

        let package = result
            .packages
            .iter()
            .find(|package| {
                package.namespace.as_deref() == Some("test")
                    && package.name.as_deref() == Some("package")
            })
            .expect("composer package should be assembled");

        assert_eq!(package.package_type, Some(PackageType::Composer));
        assert_eq!(package.version.as_deref(), Some("1.0.0"));
        assert_eq!(
            package.purl.as_deref(),
            Some("pkg:composer/test/package@1.0.0")
        );
        assert_dependency_present(
            &result.dependencies,
            "pkg:composer/phpunit/phpunit",
            "composer.json",
        );
        assert_dependency_present(
            &result.dependencies,
            "pkg:composer/phpunit/phpunit@10.0.0",
            "composer.lock",
        );
        assert_file_links_to_package(
            &files,
            "/composer.json",
            &package.package_uid,
            DatasourceId::PhpComposerJson,
        );
        assert_file_links_to_package(
            &files,
            "/composer.lock",
            &package.package_uid,
            DatasourceId::PhpComposerLock,
        );

        let lock_file = files
            .iter()
            .find(|file| file.path.ends_with("/composer.lock"))
            .expect("composer.lock should be scanned");
        assert!(
            lock_file
                .package_data
                .iter()
                .any(|pkg| pkg.purl.as_deref() == Some("pkg:composer/phpunit/phpunit@10.0.0"))
        );
    }
}
