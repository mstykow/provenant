// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use crate::models::{DatasourceId, PackageType};
use std::path::PathBuf;

use super::PackageParser;
use super::vcpkg::VcpkgManifestParser;

#[test]
fn test_vcpkg_manifest_is_match() {
    assert!(VcpkgManifestParser::is_match(&PathBuf::from(
        "/tmp/vcpkg.json"
    )));
    assert!(!VcpkgManifestParser::is_match(&PathBuf::from(
        "/tmp/vcpkg-configuration.json"
    )));
}

#[test]
fn test_parse_vcpkg_project_manifest() {
    let path = PathBuf::from("testdata/vcpkg/project/vcpkg.json");
    let pkg = VcpkgManifestParser::extract_first_package(&path);

    assert_eq!(pkg.package_type, Some(PackageType::Vcpkg));
    assert_eq!(pkg.datasource_id, Some(DatasourceId::VcpkgJson));
    assert_eq!(pkg.name.as_deref(), Some("sample-project"));
    assert_eq!(pkg.version.as_deref(), Some("1.0.0"));
    assert_eq!(
        pkg.description.as_deref(),
        Some("A sample vcpkg project manifest")
    );
    assert_eq!(
        pkg.homepage_url.as_deref(),
        Some("https://example.com/sample-project")
    );
    assert_eq!(pkg.extracted_license_statement.as_deref(), Some("MIT"));
    assert_eq!(
        pkg.purl.as_deref(),
        Some("pkg:generic/vcpkg/sample-project@1.0.0")
    );

    let extra = pkg.extra_data.as_ref().expect("extra_data should exist");
    assert_eq!(
        extra.get("builtin-baseline"),
        Some(&serde_json::json!(
            "3426db05b996481ca31e95fff3734cf23e0f51bc"
        ))
    );
    assert_eq!(
        extra.get("supports"),
        Some(&serde_json::json!("windows | linux"))
    );
    assert!(extra.get("overrides").is_some());
    assert!(extra.get("configuration").is_some());

    assert_eq!(pkg.dependencies.len(), 3);
    let fmt = pkg
        .dependencies
        .iter()
        .find(|dep| dep.purl.as_deref() == Some("pkg:generic/vcpkg/fmt"))
        .expect("expected fmt dependency");
    assert_eq!(fmt.scope.as_deref(), Some("dependencies"));
    assert_eq!(fmt.extracted_requirement.as_deref(), Some("fmt"));
    assert_eq!(fmt.is_runtime, Some(true));
    assert_eq!(fmt.is_optional, Some(false));
    assert_eq!(fmt.is_direct, Some(true));
    assert_eq!(fmt.is_pinned, Some(false));

    let cpprestsdk = pkg
        .dependencies
        .iter()
        .find(|dep| dep.purl.as_deref() == Some("pkg:generic/vcpkg/cpprestsdk"))
        .expect("expected cpprestsdk dependency");
    assert_eq!(
        cpprestsdk.extracted_requirement.as_deref(),
        Some("2.10.18#1")
    );
    assert_eq!(cpprestsdk.is_runtime, Some(false));
    assert_eq!(cpprestsdk.is_optional, Some(false));
    assert_eq!(cpprestsdk.is_direct, Some(true));
    assert_eq!(cpprestsdk.is_pinned, Some(false));
    let cpprestsdk_extra = cpprestsdk.extra_data.as_ref().expect("expected extra_data");
    assert_eq!(
        cpprestsdk_extra.get("features"),
        Some(&serde_json::json!(["websockets"]))
    );
    assert_eq!(
        cpprestsdk_extra.get("default-features"),
        Some(&serde_json::json!(false))
    );
    assert_eq!(cpprestsdk_extra.get("host"), Some(&serde_json::json!(true)));
    assert_eq!(
        cpprestsdk_extra.get("platform"),
        Some(&serde_json::json!("windows"))
    );

    let zlib = pkg
        .dependencies
        .iter()
        .find(|dep| dep.purl.as_deref() == Some("pkg:generic/vcpkg/zlib"))
        .expect("expected zlib dependency");
    assert_eq!(zlib.extracted_requirement.as_deref(), Some("1.3.1#2"));
}

#[test]
fn test_parse_vcpkg_port_manifest() {
    let path = PathBuf::from("testdata/vcpkg/port/vcpkg.json");
    let pkg = VcpkgManifestParser::extract_first_package(&path);

    assert_eq!(pkg.package_type, Some(PackageType::Vcpkg));
    assert_eq!(pkg.datasource_id, Some(DatasourceId::VcpkgJson));
    assert_eq!(pkg.name.as_deref(), Some("fmt"));
    assert_eq!(pkg.version.as_deref(), Some("10.1.1#7"));
    assert_eq!(
        pkg.description.as_deref(),
        Some("Formatting library for C++.")
    );
    assert_eq!(
        pkg.homepage_url.as_deref(),
        Some("https://github.com/fmtlib/fmt")
    );
    assert_eq!(pkg.extracted_license_statement.as_deref(), Some("MIT"));
    assert_eq!(
        pkg.purl.as_deref(),
        Some("pkg:generic/vcpkg/fmt@10.1.1%237")
    );
    assert_eq!(pkg.parties.len(), 1);
    assert_eq!(pkg.parties[0].role.as_deref(), Some("maintainer"));
    assert_eq!(pkg.parties[0].name.as_deref(), Some("fmt maintainers"));
    assert_eq!(pkg.parties[0].email.as_deref(), Some("fmt@example.com"));

    let extra = pkg.extra_data.as_ref().expect("extra_data should exist");
    assert_eq!(
        extra.get("default-features"),
        Some(&serde_json::json!(["unicode"]))
    );
    assert!(extra.get("features").is_some());

    assert_eq!(pkg.dependencies.len(), 3);
    assert!(
        pkg.dependencies
            .iter()
            .all(|dep| dep.scope.as_deref() == Some("dependencies"))
    );

    assert!(
        pkg.dependencies
            .iter()
            .filter(|dep| dep.purl.as_deref() != Some("pkg:generic/vcpkg/icu"))
            .all(|dep| dep.is_runtime == Some(false))
    );

    let icu = pkg
        .dependencies
        .iter()
        .find(|dep| dep.purl.as_deref() == Some("pkg:generic/vcpkg/icu"))
        .expect("expected icu feature dependency");
    assert_eq!(icu.is_runtime, Some(true));
    assert_eq!(icu.is_direct, Some(true));
    assert_eq!(
        icu.extra_data
            .as_ref()
            .and_then(|extra| extra.get("feature"))
            .and_then(|value| value.as_str()),
        Some("unicode")
    );
}

#[test]
fn test_invalid_vcpkg_manifest_returns_default_package() {
    let temp_dir = tempfile::TempDir::new().expect("Failed to create temp dir");
    let path = temp_dir.path().join("vcpkg.json");
    std::fs::write(&path, "{ invalid json }").expect("Failed to write invalid vcpkg.json");

    let pkg = VcpkgManifestParser::extract_first_package(&path);

    assert_eq!(pkg.package_type, Some(PackageType::Vcpkg));
    assert_eq!(pkg.datasource_id, Some(DatasourceId::VcpkgJson));
    assert!(pkg.name.is_none());
    assert!(pkg.dependencies.is_empty());
}

#[test]
fn test_parse_vcpkg_manifest_reads_sibling_configuration_when_not_embedded() {
    let temp_dir = tempfile::TempDir::new().expect("Failed to create temp dir");
    let manifest_path = temp_dir.path().join("vcpkg.json");
    let config_path = temp_dir.path().join("vcpkg-configuration.json");

    std::fs::write(
        &manifest_path,
        r#"{
            "name": "cfg-project",
            "version-string": "0.1.0",
            "dependencies": ["fmt"]
        }"#,
    )
    .expect("Failed to write manifest");
    std::fs::write(
        &config_path,
        r#"{
            "default-registry": {
                "kind": "git",
                "repository": "https://github.com/microsoft/vcpkg",
                "baseline": "0123456789abcdef0123456789abcdef01234567"
            }
        }"#,
    )
    .expect("Failed to write config");

    let pkg = VcpkgManifestParser::extract_first_package(&manifest_path);
    let extra = pkg.extra_data.as_ref().expect("extra_data should exist");
    let configuration = extra
        .get("configuration")
        .expect("expected sibling configuration metadata");

    assert_eq!(
        configuration["default-registry"]["repository"],
        serde_json::json!("https://github.com/microsoft/vcpkg")
    );
}

#[test]
fn test_parse_vcpkg_project_manifest_without_identity() {
    let temp_dir = tempfile::TempDir::new().expect("Failed to create temp dir");
    let manifest_path = temp_dir.path().join("vcpkg.json");
    std::fs::write(
        &manifest_path,
        r#"{
            "dependencies": [
                "fmt",
                { "name": "zlib", "version>=": "1.3.1#2" }
            ],
            "builtin-baseline": "3426db05b996481ca31e95fff3734cf23e0f51bc"
        }"#,
    )
    .expect("Failed to write manifest");

    let pkg = VcpkgManifestParser::extract_first_package(&manifest_path);

    assert_eq!(pkg.package_type, Some(PackageType::Vcpkg));
    assert_eq!(pkg.datasource_id, Some(DatasourceId::VcpkgJson));
    assert!(pkg.name.is_none());
    assert!(pkg.version.is_none());
    assert!(pkg.purl.is_none());
    assert_eq!(pkg.dependencies.len(), 2);
    assert!(
        pkg.dependencies
            .iter()
            .any(|dep| dep.purl.as_deref() == Some("pkg:generic/vcpkg/fmt"))
    );
    assert!(
        pkg.dependencies
            .iter()
            .any(|dep| dep.purl.as_deref() == Some("pkg:generic/vcpkg/zlib"))
    );
}
