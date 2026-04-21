// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::scan_test_utils::{
        assert_dependency_present, assert_file_links_to_package, scan_and_assemble,
    };
    use crate::models::{DatasourceId, PackageType};

    #[test]
    fn test_dart_pubspec_scan_assembles_manifest_and_lockfile() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        fs::copy(
            "testdata/dart-golden/publish-pubspec/pubspec.yaml",
            temp_dir.path().join("pubspec.yaml"),
        )
        .expect("copy pubspec fixture");
        fs::copy(
            "testdata/dart-golden/stock-lock/pubspec.lock",
            temp_dir.path().join("pubspec.lock"),
        )
        .expect("copy pubspec.lock fixture");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("mock_name"))
            .expect("dart package should be assembled");

        assert_eq!(package.package_type, Some(PackageType::Dart));
        assert_eq!(package.version.as_deref(), Some("1.1.0"));
        assert_eq!(package.purl.as_deref(), Some("pkg:dart/mock_name@1.1.0"));
        assert_dependency_present(&result.dependencies, "pkg:pubspec/yaml", "pubspec.yaml");
        assert_dependency_present(
            &result.dependencies,
            "pkg:pubspec/async@2.6.1",
            "pubspec.lock",
        );
        assert_file_links_to_package(
            &files,
            "/pubspec.yaml",
            &package.package_uid,
            DatasourceId::PubspecYaml,
        );
        assert_file_links_to_package(
            &files,
            "/pubspec.lock",
            &package.package_uid,
            DatasourceId::PubspecLock,
        );
    }
}
