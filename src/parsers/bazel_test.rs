//! Tests for Bazel BUILD parser

use crate::models::PackageType;

use std::path::PathBuf;

use crate::parsers::PackageParser;
use crate::parsers::bazel::BazelBuildParser;

#[test]
fn test_parse_build_with_rules() {
    let path = PathBuf::from("testdata/bazel/parse/BUILD");
    let pkg = BazelBuildParser::extract_first_package(&path);

    assert_eq!(pkg.package_type, Some(PackageType::Bazel));
    assert_eq!(pkg.name, Some("hello-greet".to_string()));
    assert_eq!(pkg.purl.as_deref(), Some("pkg:bazel/hello-greet"));
    // No licenses field in this BUILD file
    assert_eq!(pkg.extracted_license_statement, None);
}

#[test]
fn test_parse_empty_build_fallback() {
    let path = PathBuf::from("testdata/bazel/end2end/BUILD");
    let pkg = BazelBuildParser::extract_first_package(&path);

    assert_eq!(pkg.package_type, Some(PackageType::Bazel));
    // Should use parent directory name
    assert_eq!(pkg.name, Some("end2end".to_string()));
}

#[test]
fn test_parse_empty_build_subdir_fallback() {
    let path = PathBuf::from("testdata/bazel/end2end/subdir2/BUILD");
    let pkg = BazelBuildParser::extract_first_package(&path);

    assert_eq!(pkg.package_type, Some(PackageType::Bazel));
    // Should use parent directory name
    assert_eq!(pkg.name, Some("subdir2".to_string()));
}

#[test]
fn test_extracts_cc_binary() {
    let content = r#"
cc_binary(
    name = "hello-world",
    srcs = ["hello-world.cc"],
)
"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let build_path = temp_dir.path().join("BUILD");
    std::fs::write(&build_path, content).unwrap();

    let pkg = BazelBuildParser::extract_first_package(&build_path);
    assert_eq!(pkg.package_type, Some(PackageType::Bazel));
    assert_eq!(pkg.name, Some("hello-world".to_string()));
    assert_eq!(pkg.purl.as_deref(), Some("pkg:bazel/hello-world"));
}

#[test]
fn test_extracts_cc_library() {
    let content = r#"
cc_library(
    name = "hello-greet",
    srcs = ["hello-greet.cc"],
    hdrs = ["hello-greet.h"],
)
"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let build_path = temp_dir.path().join("BUILD");
    std::fs::write(&build_path, content).unwrap();

    let pkg = BazelBuildParser::extract_first_package(&build_path);
    assert_eq!(pkg.package_type, Some(PackageType::Bazel));
    assert_eq!(pkg.name, Some("hello-greet".to_string()));
    assert_eq!(pkg.purl.as_deref(), Some("pkg:bazel/hello-greet"));
}

#[test]
fn test_extracts_java_binary() {
    let content = r#"
java_binary(
    name = "my-app",
    srcs = ["Main.java"],
    main_class = "com.example.Main",
)
"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let build_path = temp_dir.path().join("BUILD");
    std::fs::write(&build_path, content).unwrap();

    let pkg = BazelBuildParser::extract_first_package(&build_path);
    assert_eq!(pkg.package_type, Some(PackageType::Bazel));
    assert_eq!(pkg.name, Some("my-app".to_string()));
}

#[test]
fn test_extracts_py_library() {
    let content = r#"
py_library(
    name = "mylib",
    srcs = ["mylib.py"],
)
"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let build_path = temp_dir.path().join("BUILD");
    std::fs::write(&build_path, content).unwrap();

    let pkg = BazelBuildParser::extract_first_package(&build_path);
    assert_eq!(pkg.package_type, Some(PackageType::Bazel));
    assert_eq!(pkg.name, Some("mylib".to_string()));
}

#[test]
fn test_extracts_licenses() {
    let content = r#"
cc_binary(
    name = "hello-world",
    srcs = ["hello-world.cc"],
    licenses = ["notice", "reciprocal"],
)
"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let build_path = temp_dir.path().join("BUILD");
    std::fs::write(&build_path, content).unwrap();

    let pkg = BazelBuildParser::extract_first_package(&build_path);
    assert_eq!(pkg.package_type, Some(PackageType::Bazel));
    assert_eq!(pkg.name, Some("hello-world".to_string()));
    assert_eq!(
        pkg.extracted_license_statement,
        Some("notice, reciprocal".to_string())
    );
}

#[test]
fn test_ignores_non_binary_library_rules() {
    let content = r#"
filegroup(
    name = "package-srcs",
    srcs = glob(["**"]),
)

cc_test(
    name = "hello-test",
    srcs = ["hello-test.cc"],
)

cc_binary(
    name = "hello-world",
    srcs = ["hello-world.cc"],
)
"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let build_path = temp_dir.path().join("BUILD");
    std::fs::write(&build_path, content).unwrap();

    let pkg = BazelBuildParser::extract_first_package(&build_path);
    // Should only extract cc_binary, not filegroup or cc_test
    assert_eq!(pkg.package_type, Some(PackageType::Bazel));
    assert_eq!(pkg.name, Some("hello-world".to_string()));
}

#[test]
fn test_handles_load_statements() {
    let content = r#"
load("@rules_cc//cc:defs.bzl", "cc_binary", "cc_library")

cc_library(
    name = "hello-greet",
    srcs = ["hello-greet.cc"],
    hdrs = ["hello-greet.h"],
)
"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let build_path = temp_dir.path().join("BUILD");
    std::fs::write(&build_path, content).unwrap();

    let pkg = BazelBuildParser::extract_first_package(&build_path);
    assert_eq!(pkg.package_type, Some(PackageType::Bazel));
    assert_eq!(pkg.name, Some("hello-greet".to_string()));
}

#[test]
fn test_handles_assignment_to_rule() {
    let content = r#"
my_lib = cc_library(
    name = "assigned-lib",
    srcs = ["lib.cc"],
)
"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let build_path = temp_dir.path().join("BUILD");
    std::fs::write(&build_path, content).unwrap();

    let pkg = BazelBuildParser::extract_first_package(&build_path);
    assert_eq!(pkg.package_type, Some(PackageType::Bazel));
    assert_eq!(pkg.name, Some("assigned-lib".to_string()));
}

#[test]
fn test_handles_multiple_rules() {
    let content = r#"
cc_library(
    name = "first-lib",
    srcs = ["first.cc"],
)

cc_binary(
    name = "second-bin",
    srcs = ["second.cc"],
)
"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let build_path = temp_dir.path().join("BUILD");
    std::fs::write(&build_path, content).unwrap();

    let pkg = BazelBuildParser::extract_first_package(&build_path);
    // Should return the first rule found
    assert_eq!(pkg.package_type, Some(PackageType::Bazel));
    assert_eq!(pkg.name, Some("first-lib".to_string()));
}

#[test]
fn test_handles_rule_without_name() {
    let content = r#"
cc_binary(
    srcs = ["hello.cc"],
)
"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let build_path = temp_dir.path().join("BUILD");
    std::fs::write(&build_path, content).unwrap();

    let pkg = BazelBuildParser::extract_first_package(&build_path);
    // Should fall back to parent directory name
    assert_eq!(pkg.package_type, Some(PackageType::Bazel));
    assert!(pkg.name.is_some());
}

#[test]
fn test_handles_malformed_starlark() {
    let content = r#"
this is not valid starlark syntax {{{
"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let build_path = temp_dir.path().join("myproject");
    std::fs::create_dir_all(&build_path).unwrap();
    let build_file = build_path.join("BUILD");
    std::fs::write(&build_file, content).unwrap();

    let pkg = BazelBuildParser::extract_first_package(&build_file);
    // Should fall back to parent directory name
    assert_eq!(pkg.package_type, Some(PackageType::Bazel));
    assert_eq!(pkg.name, Some("myproject".to_string()));
}

#[test]
fn test_handles_empty_licenses_list() {
    let content = r#"
cc_binary(
    name = "hello-world",
    srcs = ["hello-world.cc"],
    licenses = [],
)
"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let build_path = temp_dir.path().join("BUILD");
    std::fs::write(&build_path, content).unwrap();

    let pkg = BazelBuildParser::extract_first_package(&build_path);
    assert_eq!(pkg.package_type, Some(PackageType::Bazel));
    assert_eq!(pkg.name, Some("hello-world".to_string()));
    assert_eq!(pkg.extracted_license_statement, None);
}

#[test]
fn test_extract_packages_returns_multiple() {
    let content = r#"
cc_library(
    name = "lib1",
    srcs = ["lib1.cc"],
)

cc_binary(
    name = "bin1",
    srcs = ["bin1.cc"],
)

cc_library(
    name = "lib2",
    srcs = ["lib2.cc"],
)
"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let build_path = temp_dir.path().join("BUILD");
    std::fs::write(&build_path, content).unwrap();

    let packages = BazelBuildParser::extract_packages(&build_path);
    assert_eq!(packages.len(), 3);
    assert_eq!(packages[0].name, Some("lib1".to_string()));
    assert_eq!(packages[0].purl.as_deref(), Some("pkg:bazel/lib1"));
    assert_eq!(packages[1].name, Some("bin1".to_string()));
    assert_eq!(packages[1].purl.as_deref(), Some("pkg:bazel/bin1"));
    assert_eq!(packages[2].name, Some("lib2".to_string()));
    assert_eq!(packages[2].purl.as_deref(), Some("pkg:bazel/lib2"));
}

#[test]
fn test_extract_packages_from_testdata() {
    let path = PathBuf::from("testdata/bazel/parse/BUILD");
    let packages = BazelBuildParser::extract_packages(&path);

    assert_eq!(packages.len(), 2);
    assert_eq!(packages[0].name, Some("hello-greet".to_string()));
    assert_eq!(packages[1].name, Some("hello-world".to_string()));
}
