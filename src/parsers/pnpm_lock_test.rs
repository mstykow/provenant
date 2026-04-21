// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::pnpm_lock::*;
use crate::models::PackageType;
use crate::parsers::PackageParser;
use std::path::PathBuf;

#[test]
fn test_is_match_pnpm_lock() {
    assert!(PnpmLockParser::is_match(&PathBuf::from("pnpm-lock.yaml")));
    assert!(PnpmLockParser::is_match(&PathBuf::from(
        "some/path/to/pnpm-lock.yaml"
    )));
    assert!(!PnpmLockParser::is_match(&PathBuf::from("package.json")));
    assert!(!PnpmLockParser::is_match(&PathBuf::from("yarn.lock")));
}

#[test]
fn test_is_match_shrinkwrap_yaml() {
    assert!(PnpmLockParser::is_match(&PathBuf::from("shrinkwrap.yaml")));
    assert!(PnpmLockParser::is_match(&PathBuf::from(
        "some/path/to/shrinkwrap.yaml"
    )));
    assert!(!PnpmLockParser::is_match(&PathBuf::from("README.md")));
}

#[test]
fn test_extract_from_testdata_v5() {
    let test_data_path = PathBuf::from("testdata/pnpm/pnpm-v5.yaml");
    assert!(
        test_data_path.exists(),
        "missing fixture: {}",
        test_data_path.display()
    );

    let data = PnpmLockParser::extract_first_package(&test_data_path);

    assert_eq!(data.package_type, Some(PackageType::PnpmLock));
    assert!(
        !data.dependencies.is_empty(),
        "Should extract packages from v5 lockfile"
    );
}

#[test]
fn test_extract_from_testdata_v6() {
    let test_data_path = PathBuf::from("testdata/pnpm/pnpm-v6.yaml");
    assert!(
        test_data_path.exists(),
        "missing fixture: {}",
        test_data_path.display()
    );

    let data = PnpmLockParser::extract_first_package(&test_data_path);

    assert_eq!(data.package_type, Some(PackageType::PnpmLock));
    assert!(
        !data.dependencies.is_empty(),
        "Should extract packages from v6 lockfile"
    );
}

#[test]
fn test_extract_from_testdata_v9() {
    let test_data_path = PathBuf::from("testdata/pnpm/pnpm-v9.yaml");
    assert!(
        test_data_path.exists(),
        "missing fixture: {}",
        test_data_path.display()
    );

    let data = PnpmLockParser::extract_first_package(&test_data_path);

    assert_eq!(data.package_type, Some(PackageType::PnpmLock));
    assert!(
        !data.dependencies.is_empty(),
        "Should extract packages from v9 lockfile"
    );
}

#[test]
fn test_parse_purl_fields_v6_complex() {
    let (namespace, name, version) = parse_purl_fields("@headlessui/react@1.6.6", "6.0").unwrap();
    assert_eq!(namespace, Some("@headlessui".to_string()));
    assert_eq!(name, "react".to_string());
    assert_eq!(version, "1.6.6".to_string());
}

#[test]
fn test_parse_purl_fields_v5_complex() {
    let (namespace, name, version) =
        parse_purl_fields("@napi-rs/simple-git-android-arm-eabi/0.1.8", "5.0").unwrap();
    assert_eq!(namespace, Some("@napi-rs".to_string()));
    assert_eq!(name, "simple-git-android-arm-eabi".to_string());
    assert_eq!(version, "0.1.8".to_string());
}

#[test]
fn test_parse_purl_fields_v5_non_scoped() {
    let (namespace, name, version) =
        parse_purl_fields("regenerator-runtime/0.13.9", "5.0").unwrap();
    assert_eq!(namespace, None);
    assert_eq!(name, "regenerator-runtime".to_string());
    assert_eq!(version, "0.13.9".to_string());
}

#[test]
fn test_extract_dependency_with_resolution() {
    let yaml = r#"
resolution:
  integrity: sha512-lkqXDcvlFT5rvEjiu6+QYO+1GXrEHRo2LOtS7E4GtX5ESIZOgepqsZBVIj6Pv+a6zqsya9VCgiK1KAK4BvJDAw==
"#;

    let data: yaml_serde::Value = yaml_serde::from_str(yaml).unwrap();

    let dep = extract_dependency("regenerator-runtime@0.13.9", &data, "9.0", false);
    assert!(dep.is_some());

    let dep = dep.unwrap();
    assert_eq!(dep.extracted_requirement, Some("0.13.9".to_string()));
    assert!(dep.resolved_package.is_some());

    let resolved = dep.resolved_package.unwrap();
    assert_eq!(resolved.name, "regenerator-runtime".to_string());
    assert_eq!(resolved.version, "0.13.9".to_string());
}

#[test]
fn test_extract_dependency_with_flags() {
    let yaml = r#"
resolution:
  integrity: sha512-example
hasBin: true
requiresBuild: true
dev: false
"#;

    let data: yaml_serde::Value = yaml_serde::from_str(yaml).unwrap();

    let dep = extract_dependency("babel-cli@7.0.0", &data, "9.0", false);
    assert!(dep.is_some());

    let dep = dep.unwrap();
    assert!(dep.resolved_package.is_some());

    // Note: Extra data is not directly testable via Dependency struct
    // but should be present in the resolved_package
}

#[test]
fn test_extract_dependency_invalid_input() {
    let yaml = r#"
resolution:
  integrity: sha512-example
"#;

    let data: yaml_serde::Value = yaml_serde::from_str(yaml).unwrap();

    // Invalid purl_fields should return None
    let dep = extract_dependency("", &data, "9.0", false);
    assert!(dep.is_none());

    let dep = extract_dependency("invalid-format", &data, "9.0", false);
    assert!(dep.is_none());
}

#[test]
fn test_detect_pnpm_version_shrinkwrap() {
    let yaml = r#"
shrinkwrapVersion: 4
shrinkwrapMinorVersion: 0
"#;

    let data: yaml_serde::Value = yaml_serde::from_str(yaml).unwrap();
    assert_eq!(detect_pnpm_version(&data), "4.0");
}

#[test]
fn test_detect_pnpm_version_default() {
    let yaml = "settings:\n  autoInstallPeers: true\n";

    let data: yaml_serde::Value = yaml_serde::from_str(yaml).unwrap();
    assert_eq!(detect_pnpm_version(&data), "5.0");
}

#[test]
fn test_clean_purl_fields_v9() {
    let purl_fields = "@babel/runtime@7.18.9";
    assert_eq!(
        clean_purl_fields(purl_fields, "9.0"),
        "@babel/runtime@7.18.9"
    );

    let purl_fields = "anve-upload-upyun@1.0.8";
    assert_eq!(
        clean_purl_fields(purl_fields, "9.0"),
        "anve-upload-upyun@1.0.8"
    );
}

#[test]
fn test_parse_purl_fields_v9_multiple_at_symbols() {
    let (namespace, name, version) =
        parse_purl_fields("@babel/helper-validator-identifier@7.24.7", "9.0").unwrap();
    assert_eq!(namespace, Some("babel".to_string()));
    assert_eq!(name, "helper-validator-identifier".to_string());
    assert_eq!(version, "7.24.7".to_string());
}

#[test]
fn test_create_purl_scoped() {
    let purl = create_purl(&Some("@babel".to_string()), "runtime", "7.18.9");
    assert!(purl.contains("pkg:npm"));
    assert!(purl.contains("%40babel"));
    assert!(purl.contains("runtime"));
    assert!(purl.contains("7.18.9"));
}

#[test]
fn test_create_purl_non_scoped() {
    let purl = create_purl(&None, "express", "4.18.2");
    assert!(purl.contains("pkg:npm"));
    assert!(purl.contains("express"));
    assert!(purl.contains("4.18.2"));
}

#[test]
fn test_pnpm_dev_dependencies_v6() {
    let test_data_path = std::path::PathBuf::from("testdata/pnpm/pnpm-v6.yaml");
    assert!(
        test_data_path.exists(),
        "missing fixture: {}",
        test_data_path.display()
    );

    let data = PnpmLockParser::extract_first_package(&test_data_path);

    assert_eq!(data.package_type, Some(PackageType::PnpmLock));
    assert!(!data.dependencies.is_empty());

    let dev_deps: Vec<_> = data
        .dependencies
        .iter()
        .filter(|d| d.scope.as_deref() == Some("dev"))
        .collect();

    let runtime_deps: Vec<_> = data
        .dependencies
        .iter()
        .filter(|d| d.scope.is_none() && d.is_runtime == Some(true))
        .collect();

    assert!(
        !dev_deps.is_empty(),
        "Should have dev dependencies from pnpm-v6.yaml"
    );

    assert_eq!(
        dev_deps.len(),
        19,
        "pnpm-v6.yaml contains 19 dev dependencies"
    );

    assert_eq!(
        runtime_deps.len(),
        0,
        "pnpm-v6.yaml contains only dev dependencies (no runtime deps)"
    );

    for dep in &dev_deps {
        assert_eq!(
            dep.is_runtime,
            Some(false),
            "Dev dependencies should have is_runtime=false"
        );
        assert_eq!(
            dep.scope,
            Some("dev".to_string()),
            "Dev dependencies should have scope='dev'"
        );
    }
}

#[test]
fn test_extract_dependency_with_dev_flag() {
    let yaml = r#"
resolution:
  integrity: sha512-example
dev: true
"#;

    let data: yaml_serde::Value = yaml_serde::from_str(yaml).unwrap();

    let dep = extract_dependency("@babel/core@7.24.5", &data, "6.0", false);
    assert!(dep.is_some());

    let dep = dep.unwrap();
    assert_eq!(dep.scope, Some("dev".to_string()));
    assert_eq!(dep.is_runtime, Some(false));
    assert_eq!(dep.is_optional, Some(false));
}

#[test]
fn test_extract_dependency_with_optional_flag() {
    let yaml = r#"
resolution:
  integrity: sha512-example
optional: true
"#;

    let data: yaml_serde::Value = yaml_serde::from_str(yaml).unwrap();

    let dep = extract_dependency("prettier@2.8.8", &data, "9.0", false);
    assert!(dep.is_some());

    let dep = dep.unwrap();
    assert_eq!(dep.scope, Some("optional".to_string()));
    assert_eq!(dep.is_runtime, Some(true));
    assert_eq!(dep.is_optional, Some(true));
}

#[test]
fn test_extract_dependency_runtime_default() {
    let yaml = r#"
resolution:
  integrity: sha512-example
"#;

    let data: yaml_serde::Value = yaml_serde::from_str(yaml).unwrap();

    let dep = extract_dependency("express@4.18.2", &data, "9.0", false);
    assert!(dep.is_some());

    let dep = dep.unwrap();
    assert_eq!(dep.scope, None);
    assert_eq!(dep.is_runtime, Some(true));
    assert_eq!(dep.is_optional, Some(false));
}

#[test]
fn test_extract_dependency_dev_v9_from_graph() {
    let yaml = r#"
resolution:
  integrity: sha512-example
"#;

    let data: yaml_serde::Value = yaml_serde::from_str(yaml).unwrap();

    let dep = extract_dependency("@types/node@20.2.1", &data, "9.0", true);
    assert!(dep.is_some());

    let dep = dep.unwrap();
    assert_eq!(dep.scope, Some("dev".to_string()));
    assert_eq!(dep.is_runtime, Some(false));
    assert_eq!(dep.is_optional, Some(false));
}

#[test]
fn test_clean_purl_fields_v5_package_with_underscore() {
    let purl_fields = "/string_decoder/1.3.0";
    let result = clean_purl_fields(purl_fields, "5.0");
    assert_eq!(result, "string_decoder/1.3.0");
}

#[test]
fn test_clean_purl_fields_v5_safe_buffer_with_underscore() {
    let purl_fields = "/safe_buffer/5.2.1_xyz789abc";
    let result = clean_purl_fields(purl_fields, "5.0");
    assert_eq!(result, "safe_buffer/5.2.1");
}

#[test]
fn test_clean_purl_fields_v5_scoped_package_with_underscore() {
    let purl_fields = "/@babel/helper_string_parser/7.24.8_abc123";
    let result = clean_purl_fields(purl_fields, "5.0");
    assert_eq!(result, "@babel/helper_string_parser/7.24.8");
}

#[test]
fn test_parse_purl_fields_v5_string_decoder_with_underscore() {
    let (namespace, name, version) = parse_purl_fields("string_decoder/1.3.0", "5.0").unwrap();
    assert_eq!(namespace, None);
    assert_eq!(name, "string_decoder".to_string());
    assert_eq!(version, "1.3.0".to_string());
}

#[test]
fn test_parse_purl_fields_v5_scoped_with_underscore() {
    let (namespace, name, version) =
        parse_purl_fields("@babel/helper_string_parser/7.24.8", "5.0").unwrap();
    assert_eq!(namespace, Some("@babel".to_string()));
    assert_eq!(name, "helper_string_parser".to_string());
    assert_eq!(version, "7.24.8".to_string());
}

#[test]
fn test_parse_purl_fields_v6_package_with_underscore() {
    let (namespace, name, version) = parse_purl_fields("string_decoder@1.3.0", "6.0").unwrap();
    assert_eq!(namespace, None);
    assert_eq!(name, "string_decoder".to_string());
    assert_eq!(version, "1.3.0".to_string());
}

#[test]
fn test_parse_purl_fields_v6_scoped_with_underscore() {
    let (namespace, name, version) =
        parse_purl_fields("@babel/helper_string_parser@7.24.8", "6.0").unwrap();
    assert_eq!(namespace, Some("@babel".to_string()));
    assert_eq!(name, "helper_string_parser".to_string());
    assert_eq!(version, "7.24.8".to_string());
}
