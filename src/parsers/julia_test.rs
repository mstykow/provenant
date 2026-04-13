use crate::models::{DatasourceId, PackageType};
use crate::parsers::PackageParser;
use crate::parsers::julia::{JuliaManifestTomlParser, JuliaProjectTomlParser};
use std::path::PathBuf;

fn test_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/julia")
}

#[test]
fn test_project_toml_is_match() {
    assert!(JuliaProjectTomlParser::is_match(
        PathBuf::from("Project.toml").as_path()
    ));
    assert!(JuliaProjectTomlParser::is_match(
        PathBuf::from("project.toml").as_path()
    ));
    assert!(!JuliaProjectTomlParser::is_match(
        PathBuf::from("Manifest.toml").as_path()
    ));
    assert!(!JuliaProjectTomlParser::is_match(
        PathBuf::from("Cargo.toml").as_path()
    ));
}

#[test]
fn test_manifest_toml_is_match() {
    assert!(JuliaManifestTomlParser::is_match(
        PathBuf::from("Manifest.toml").as_path()
    ));
    assert!(JuliaManifestTomlParser::is_match(
        PathBuf::from("manifest.toml").as_path()
    ));
    assert!(!JuliaManifestTomlParser::is_match(
        PathBuf::from("Project.toml").as_path()
    ));
}

#[test]
fn test_project_toml_basic_extraction() {
    let test_file = test_data_dir().join("basic/Project.toml");
    if !test_file.exists() {
        return;
    }
    let packages = JuliaProjectTomlParser::extract_packages(&test_file);
    assert_eq!(packages.len(), 1);
    let pkg = &packages[0];
    assert_eq!(pkg.package_type, Some(PackageType::Julia));
    assert_eq!(pkg.datasource_id, Some(DatasourceId::JuliaProjectToml));
    assert_eq!(pkg.primary_language, Some("Julia".to_string()));
    assert_eq!(pkg.name.as_deref(), Some("MyPackage"));
    assert_eq!(pkg.version.as_deref(), Some("1.0.0"));
}

#[test]
fn test_manifest_toml_basic_extraction() {
    let test_file = test_data_dir().join("basic/Manifest.toml");
    if !test_file.exists() {
        return;
    }
    let packages = JuliaManifestTomlParser::extract_packages(&test_file);
    assert!(!packages.is_empty());
    for pkg in &packages {
        assert_eq!(pkg.package_type, Some(PackageType::Julia));
        assert_eq!(pkg.datasource_id, Some(DatasourceId::JuliaManifestToml));
    }
}

#[test]
fn test_project_toml_dependencies() {
    let test_file = test_data_dir().join("basic/Project.toml");
    if !test_file.exists() {
        return;
    }
    let packages = JuliaProjectTomlParser::extract_packages(&test_file);
    assert_eq!(packages.len(), 1);
    let pkg = &packages[0];
    assert_eq!(pkg.dependencies.len(), 2);

    let json_dep = pkg
        .dependencies
        .iter()
        .find(|d| d.purl.as_deref().unwrap_or_default().contains("/JSON"))
        .expect("JSON dep");
    assert!(json_dep.is_pinned.unwrap_or(false));

    let http_dep = pkg
        .dependencies
        .iter()
        .find(|d| d.purl.as_deref().unwrap_or_default().contains("/HTTP"))
        .expect("HTTP dep");
    assert!(!http_dep.is_pinned.unwrap_or(true));
}

#[test]
fn test_project_toml_license() {
    let test_file = test_data_dir().join("basic/Project.toml");
    if !test_file.exists() {
        return;
    }
    let packages = JuliaProjectTomlParser::extract_packages(&test_file);
    assert_eq!(packages.len(), 1);
    let pkg = &packages[0];
    assert_eq!(pkg.extracted_license_statement.as_deref(), Some("MIT"));
    assert!(pkg.declared_license_expression_spdx.is_some());
}
