//! Tests for Buck BUILD and METADATA.bzl parsers

use crate::models::{PackageType, Sha1Digest};

use std::path::PathBuf;

use crate::parsers::PackageParser;
use crate::parsers::buck::{BuckBuildParser, BuckMetadataBzlParser};

// ============================================================================
// BuckBuildParser Tests
// ============================================================================

#[test]
fn test_parse_buck_with_rules() {
    let path = PathBuf::from("testdata/buck/parse/BUCK");
    let pkg = BuckBuildParser::extract_first_package(&path);

    assert_eq!(pkg.package_type, Some(PackageType::Buck));
    assert_eq!(pkg.name, Some("app".to_string()));
}

#[test]
fn test_parse_empty_buck_fallback() {
    let path = PathBuf::from("testdata/buck/end2end/BUCK");
    let pkg = BuckBuildParser::extract_first_package(&path);

    assert_eq!(pkg.package_type, Some(PackageType::Buck));
    assert_eq!(pkg.name, Some("end2end".to_string()));
}

#[test]
fn test_parse_buck_subdir_with_rule() {
    let path = PathBuf::from("testdata/buck/end2end/subdir2/BUCK");
    let pkg = BuckBuildParser::extract_first_package(&path);

    assert_eq!(pkg.package_type, Some(PackageType::Buck));
    // This BUCK file has a cxx_binary rule with name="bin"
    assert_eq!(pkg.name, Some("bin".to_string()));
    assert_eq!(pkg.extracted_license_statement, Some("LICENSE".to_string()));
    let extra = pkg.extra_data.as_ref().expect("extra_data should exist");
    assert_eq!(
        extra.get("license_file").and_then(|v| v.as_str()),
        Some("LICENSE")
    );
    let referenced_filenames = pkg.license_detections[0].matches[0]
        .referenced_filenames
        .as_ref()
        .expect("referenced_filenames should be present");
    assert_eq!(referenced_filenames, &vec!["LICENSE".to_string()]);
}

#[test]
fn test_extracts_android_binary() {
    let content = r#"
android_binary(
    name = "my-app",
    manifest = "AndroidManifest.xml",
)
"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let buck_path = temp_dir.path().join("BUCK");
    std::fs::write(&buck_path, content).unwrap();

    let pkg = BuckBuildParser::extract_first_package(&buck_path);
    assert_eq!(pkg.package_type, Some(PackageType::Buck));
    assert_eq!(pkg.name, Some("my-app".to_string()));
}

#[test]
fn test_extracts_android_library() {
    let content = r#"
android_library(
    name = "my-lib",
    srcs = glob(["**/*.java"]),
)
"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let buck_path = temp_dir.path().join("BUCK");
    std::fs::write(&buck_path, content).unwrap();

    let pkg = BuckBuildParser::extract_first_package(&buck_path);
    assert_eq!(pkg.package_type, Some(PackageType::Buck));
    assert_eq!(pkg.name, Some("my-lib".to_string()));
}

#[test]
fn test_extracts_java_binary() {
    let content = r#"
java_binary(
    name = "my-app",
    main_class = "com.example.Main",
)
"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let buck_path = temp_dir.path().join("BUCK");
    std::fs::write(&buck_path, content).unwrap();

    let pkg = BuckBuildParser::extract_first_package(&buck_path);
    assert_eq!(pkg.package_type, Some(PackageType::Buck));
    assert_eq!(pkg.name, Some("my-app".to_string()));
}

#[test]
fn test_buck_ignores_non_binary_library_rules() {
    let content = r#"
filegroup(
    name = "resources",
    srcs = glob(["res/**"]),
)

android_binary(
    name = "app",
    manifest = "AndroidManifest.xml",
)
"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let buck_path = temp_dir.path().join("BUCK");
    std::fs::write(&buck_path, content).unwrap();

    let pkg = BuckBuildParser::extract_first_package(&buck_path);
    assert_eq!(pkg.package_type, Some(PackageType::Buck));
    assert_eq!(pkg.name, Some("app".to_string()));
}

#[test]
fn test_buck_handles_multiple_rules() {
    let content = r#"
android_binary(
    name = "app1",
    manifest = "AndroidManifest.xml",
)

android_binary(
    name = "app2",
    manifest = "AndroidManifest.xml",
)
"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let buck_path = temp_dir.path().join("BUCK");
    std::fs::write(&buck_path, content).unwrap();

    let pkg = BuckBuildParser::extract_first_package(&buck_path);
    // Should return first rule
    assert_eq!(pkg.package_type, Some(PackageType::Buck));
    assert_eq!(pkg.name, Some("app1".to_string()));
}

// ============================================================================
// BuckMetadataBzlParser Tests
// ============================================================================

#[test]
fn test_parse_metadata_bzl_basic() {
    let path = PathBuf::from("testdata/buck/metadata/METADATA.bzl");
    let pkg = BuckMetadataBzlParser::extract_first_package(&path);

    assert_eq!(pkg.package_type, Some(PackageType::Github));
    assert_eq!(pkg.name, Some("example".to_string()));
    assert_eq!(pkg.version, Some("0.0.1".to_string()));
    assert_eq!(
        pkg.extracted_license_statement,
        Some("BSD-3-Clause".to_string())
    );
    assert_eq!(
        pkg.homepage_url,
        Some("https://github.com/example/example".to_string())
    );

    // Check maintainers
    assert_eq!(pkg.parties.len(), 1);
    assert_eq!(pkg.parties[0].name, Some("oss_foundation".to_string()));
    assert_eq!(pkg.parties[0].role, Some("maintainer".to_string()));
    assert_eq!(pkg.parties[0].r#type, Some("organization".to_string()));

    // Check extra_data
    assert!(pkg.extra_data.is_some());
    let extra = pkg.extra_data.as_ref().unwrap();
    assert_eq!(
        extra.get("upstream_hash"),
        Some(&serde_json::Value::String("deadbeef".to_string()))
    );
}

#[test]
fn test_parse_metadata_bzl_new_format() {
    let path = PathBuf::from("testdata/buck/metadata/new-format-METADATA.bzl");
    let pkg = BuckMetadataBzlParser::extract_first_package(&path);

    assert_eq!(pkg.package_type, Some(PackageType::Github));
    assert_eq!(pkg.name, Some("example/example".to_string()));
    assert_eq!(pkg.version, Some("0.0.1".to_string()));
    assert_eq!(
        pkg.extracted_license_statement,
        Some("BSD-3-Clause".to_string())
    );
    assert_eq!(
        pkg.homepage_url,
        Some("https://github.com/example/example".to_string())
    );
    assert_eq!(
        pkg.vcs_url,
        Some("https://github.com/example/example.git".to_string())
    );

    // Check maintainers
    assert_eq!(pkg.parties.len(), 1);
    assert_eq!(pkg.parties[0].name, Some("example_org".to_string()));

    // Check extra_data
    assert!(pkg.extra_data.is_some());
    let extra = pkg.extra_data.as_ref().unwrap();
    assert_eq!(
        extra.get("vcs_commit_hash"),
        Some(&serde_json::Value::String("deadbeef".to_string()))
    );
}

#[test]
fn test_metadata_bzl_with_licenses_list() {
    let content = r#"
METADATA = {
    "name": "example",
    "version": "1.0.0",
    "licenses": ["MIT", "Apache-2.0"],
}
"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let metadata_path = temp_dir.path().join("METADATA.bzl");
    std::fs::write(&metadata_path, content).unwrap();

    let pkg = BuckMetadataBzlParser::extract_first_package(&metadata_path);
    assert_eq!(pkg.name, Some("example".to_string()));
    assert_eq!(pkg.version, Some("1.0.0".to_string()));
    assert_eq!(
        pkg.extracted_license_statement,
        Some("MIT, Apache-2.0".to_string())
    );
}

#[test]
fn test_metadata_bzl_splits_license_references_from_license_statements() {
    let content = r#"
METADATA = {
    "name": "example",
    "version": "1.0.0",
    "licenses": ["MIT", "LICENSE.txt"],
}
"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let metadata_path = temp_dir.path().join("METADATA.bzl");
    std::fs::write(&metadata_path, content).unwrap();

    let pkg = BuckMetadataBzlParser::extract_first_package(&metadata_path);
    assert_eq!(pkg.extracted_license_statement.as_deref(), Some("MIT"));
    let extra = pkg.extra_data.as_ref().expect("extra_data should exist");
    assert_eq!(
        extra.get("license_file").and_then(|value| value.as_str()),
        Some("LICENSE.txt")
    );
    let referenced_filenames = pkg.license_detections[0].matches[0]
        .referenced_filenames
        .as_ref()
        .expect("referenced_filenames should be present");
    assert_eq!(referenced_filenames, &vec!["LICENSE.txt".to_string()]);
}

#[test]
fn test_metadata_bzl_with_multiple_maintainers() {
    let content = r#"
METADATA = {
    "name": "example",
    "maintainers": ["org1", "org2", "org3"],
}
"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let metadata_path = temp_dir.path().join("METADATA.bzl");
    std::fs::write(&metadata_path, content).unwrap();

    let pkg = BuckMetadataBzlParser::extract_first_package(&metadata_path);
    assert_eq!(pkg.parties.len(), 3);
    assert_eq!(pkg.parties[0].name, Some("org1".to_string()));
    assert_eq!(pkg.parties[1].name, Some("org2".to_string()));
    assert_eq!(pkg.parties[2].name, Some("org3".to_string()));
}

#[test]
fn test_metadata_bzl_with_download_url() {
    let content = r#"
METADATA = {
    "name": "example",
    "download_url": "https://example.com/download.tar.gz",
    "download_archive_sha1": "abc123def456789012345678901234567890abcd",
}
"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let metadata_path = temp_dir.path().join("METADATA.bzl");
    std::fs::write(&metadata_path, content).unwrap();

    let pkg = BuckMetadataBzlParser::extract_first_package(&metadata_path);
    assert_eq!(
        pkg.download_url,
        Some("https://example.com/download.tar.gz".to_string())
    );
    assert_eq!(
        pkg.sha1,
        Some(Sha1Digest::from_hex("abc123def456789012345678901234567890abcd").unwrap())
    );
}

#[test]
fn test_metadata_bzl_empty_file() {
    let content = r#"
# Just a comment
"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let metadata_path = temp_dir.path().join("METADATA.bzl");
    std::fs::write(&metadata_path, content).unwrap();

    let pkg = BuckMetadataBzlParser::extract_first_package(&metadata_path);
    assert_eq!(pkg.package_type, Some(PackageType::Buck));
    assert_eq!(pkg.name, None);
}

#[test]
fn test_metadata_bzl_no_metadata_variable() {
    let content = r#"
OTHER_VAR = {
    "name": "example",
}
"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let metadata_path = temp_dir.path().join("METADATA.bzl");
    std::fs::write(&metadata_path, content).unwrap();

    let pkg = BuckMetadataBzlParser::extract_first_package(&metadata_path);
    assert_eq!(pkg.package_type, Some(PackageType::Buck));
    assert_eq!(pkg.name, None);
}

#[test]
fn test_metadata_bzl_malformed_syntax() {
    let content = r#"
METADATA = {{{
"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let metadata_path = temp_dir.path().join("METADATA.bzl");
    std::fs::write(&metadata_path, content).unwrap();

    let pkg = BuckMetadataBzlParser::extract_first_package(&metadata_path);
    assert_eq!(pkg.package_type, Some(PackageType::Buck));
}

#[test]
fn test_metadata_bzl_with_package_url() {
    let path = PathBuf::from("testdata/buck/metadata/with-package-url-METADATA.bzl");
    let pkg = BuckMetadataBzlParser::extract_first_package(&path);

    // package_url should override type, namespace, name, version
    assert_eq!(pkg.package_type, Some(PackageType::Maven));
    assert_eq!(
        pkg.namespace,
        Some("androidx.compose.animation".to_string())
    );
    assert_eq!(pkg.name, Some("animation".to_string()));
    assert_eq!(pkg.version, Some("0.0.1".to_string()));

    // Other fields should still be extracted
    assert_eq!(
        pkg.extracted_license_statement,
        Some("BSD-3-Clause".to_string())
    );
    assert_eq!(
        pkg.homepage_url,
        Some(
            "https://developer.android.com/jetpack/androidx/releases/compose-animation#0.0.1"
                .to_string()
        )
    );
    assert_eq!(pkg.parties.len(), 1);
    assert_eq!(pkg.parties[0].name, Some("oss_foundation".to_string()));
}
