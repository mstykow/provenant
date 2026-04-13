mod tests {
    use crate::models::{Dependency, PackageType, Sha1Digest};
    use crate::parsers::{ComposerJsonParser, ComposerLockParser, PackageParser};
    use serde_json::Value;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_temp_file(file_name: &str, content: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join(file_name);
        fs::write(&file_path, content).expect("Failed to write temp file");
        (temp_dir, file_path)
    }

    fn find_dependency<'a>(dependencies: &'a [Dependency], purl: &str) -> &'a Dependency {
        dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some(purl))
            .unwrap_or_else(|| panic!("Dependency not found for purl: {}", purl))
    }

    fn sample_composer_lock() -> String {
        r#"
{
  "packages": [
    {
      "name": "acme/runtime",
      "version": "1.0.0",
      "type": "library",
      "require": {
        "php": "^8.2",
        "acme/shared": "^3.0"
      },
      "require-dev": {
        "acme/runtime-testkit": "^1.4"
      },
      "conflict": {
        "acme/legacy-runtime": "<1.0"
      },
      "suggest": {
        "ext-pcntl": "Improves worker support"
      },
      "source": {
        "type": "git",
        "url": "https://github.com/acme/runtime.git",
        "reference": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
      },
      "dist": {
        "type": "zip",
        "url": "https://example.com/runtime.zip",
        "reference": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "shasum": "cccccccccccccccccccccccccccccccccccccccc"
      }
    }
  ],
  "packages-dev": [
    {
      "name": "acme/devpkg",
      "version": "2.0.0",
      "type": "project"
    }
  ]
}
"#
        .to_string()
    }

    #[test]
    fn test_composer_json_is_match() {
        let valid_path = PathBuf::from("/some/path/composer.json");
        let alternate_prefix = PathBuf::from("/some/path/symfony.composer.json");
        let alternate_suffix = PathBuf::from("/some/path/composer.symfony.json");
        let invalid_path = PathBuf::from("/some/path/composer.lock");

        assert!(ComposerJsonParser::is_match(&valid_path));
        assert!(ComposerJsonParser::is_match(&alternate_prefix));
        assert!(ComposerJsonParser::is_match(&alternate_suffix));
        assert!(!ComposerJsonParser::is_match(&invalid_path));
    }

    #[test]
    fn test_composer_lock_is_match() {
        let valid_path = PathBuf::from("/some/path/composer.lock");
        let alternate_prefix = PathBuf::from("/some/path/php.composer.lock");
        let alternate_suffix = PathBuf::from("/some/path/composer.symfony.lock");
        let invalid_path = PathBuf::from("/some/path/composer.json");

        assert!(ComposerLockParser::is_match(&valid_path));
        assert!(ComposerLockParser::is_match(&alternate_prefix));
        assert!(ComposerLockParser::is_match(&alternate_suffix));
        assert!(!ComposerLockParser::is_match(&invalid_path));
    }

    #[test]
    fn test_extract_package_name() {
        let content = r#"
{
  "name": "acme/demo"
}
"#;

        let (_temp_dir, composer_path) = create_temp_file("composer.json", content);
        let package_data = ComposerJsonParser::extract_first_package(&composer_path);

        assert_eq!(package_data.namespace, Some("acme".to_string()));
        assert_eq!(package_data.name, Some("demo".to_string()));
        assert_eq!(
            package_data.purl,
            Some("pkg:composer/acme/demo".to_string())
        );
    }

    #[test]
    fn test_extract_dependencies() {
        let content = r#"
{
  "name": "acme/demo",
  "require": {
    "php": ">=8.0",
    "acme/runtime": "^1.0"
  },
  "require-dev": {
    "acme/devpkg": "~2.0"
  }
}
"#;

        let (_temp_dir, composer_path) = create_temp_file("composer.json", content);
        let package_data = ComposerJsonParser::extract_first_package(&composer_path);

        assert_eq!(package_data.dependencies.len(), 3);

        let runtime_dep = find_dependency(&package_data.dependencies, "pkg:composer/acme/runtime");
        assert_eq!(runtime_dep.scope.as_deref(), Some("require"));
        assert_eq!(runtime_dep.is_runtime, Some(true));

        let php_dep = find_dependency(&package_data.dependencies, "pkg:composer/php");
        assert_eq!(php_dep.extracted_requirement.as_deref(), Some(">=8.0"));

        let dev_dep = find_dependency(&package_data.dependencies, "pkg:composer/acme/devpkg");
        assert_eq!(dev_dep.scope.as_deref(), Some("require-dev"));
        assert_eq!(dev_dep.is_runtime, Some(false));
    }

    #[test]
    fn test_extract_dev_dependencies() {
        let content = r#"
{
  "name": "acme/demo",
  "require-dev": {
    "acme/devpkg": "~2.0"
  }
}
"#;

        let (_temp_dir, composer_path) = create_temp_file("composer.json", content);
        let package_data = ComposerJsonParser::extract_first_package(&composer_path);

        assert_eq!(package_data.dependencies.len(), 1);
        let dev_dep = find_dependency(&package_data.dependencies, "pkg:composer/acme/devpkg");
        assert_eq!(dev_dep.scope.as_deref(), Some("require-dev"));
        assert_eq!(dev_dep.is_runtime, Some(false));
        assert_eq!(dev_dep.is_optional, Some(true));
    }

    #[test]
    fn test_extract_version_constraints() {
        let content = r#"
{
  "name": "acme/demo",
  "require": {
    "acme/exact": "1.2.3",
    "acme/caret": "^1.2.3",
    "acme/tilde": "~1.2.3",
    "acme/range": ">=1.2.3"
  }
}
"#;

        let (_temp_dir, composer_path) = create_temp_file("composer.json", content);
        let package_data = ComposerJsonParser::extract_first_package(&composer_path);

        let exact_dep =
            find_dependency(&package_data.dependencies, "pkg:composer/acme/exact@1.2.3");
        assert_eq!(exact_dep.is_pinned, Some(true));

        let caret_dep = find_dependency(&package_data.dependencies, "pkg:composer/acme/caret");
        assert_eq!(caret_dep.is_pinned, Some(false));

        let tilde_dep = find_dependency(&package_data.dependencies, "pkg:composer/acme/tilde");
        assert_eq!(tilde_dep.is_pinned, Some(false));

        let range_dep = find_dependency(&package_data.dependencies, "pkg:composer/acme/range");
        assert_eq!(range_dep.is_pinned, Some(false));
    }

    #[test]
    fn test_extract_autoload_psr4() {
        let content = r#"
{
  "name": "acme/demo",
  "autoload": {
    "psr-4": {
      "Acme\\Demo\\": "src/"
    }
  }
}
"#;

        let (_temp_dir, composer_path) = create_temp_file("composer.json", content);
        let package_data = ComposerJsonParser::extract_first_package(&composer_path);
        let extra_data = package_data
            .extra_data
            .expect("Expected extra_data to be set");
        let psr4 = extra_data
            .get("autoload_psr4")
            .and_then(|value: &Value| value.as_object())
            .expect("Expected autoload_psr4 to be an object");

        assert_eq!(
            psr4.get("Acme\\Demo\\").and_then(|v: &Value| v.as_str()),
            Some("src/")
        );
    }

    #[test]
    fn test_extract_repositories() {
        let content = r#"
{
  "name": "acme/demo",
  "repositories": [
    {
      "type": "vcs",
      "url": "https://github.com/acme/demo"
    }
  ]
}
"#;

        let (_temp_dir, composer_path) = create_temp_file("composer.json", content);
        let package_data = ComposerJsonParser::extract_first_package(&composer_path);
        let extra_data = package_data
            .extra_data
            .expect("Expected extra_data to be set");
        let repos = extra_data
            .get("repositories")
            .and_then(|value: &Value| value.as_array())
            .expect("Expected repositories to be an array");

        assert_eq!(repos.len(), 1);
        assert_eq!(
            repos[0].get("url").and_then(|v: &Value| v.as_str()),
            Some("https://github.com/acme/demo")
        );
    }

    #[test]
    fn test_extract_manifest_source_dist_and_party_types() {
        let content = r#"
{
  "name": "acme/demo",
  "authors": [
    { "name": "Jane Doe", "email": "jane@example.com", "homepage": "https://example.com/jane" }
  ],
  "source": {
    "type": "git",
    "url": "https://github.com/acme/demo.git",
    "reference": "abc123"
  },
  "dist": {
    "type": "zip",
    "url": "https://example.com/demo.zip"
  }
}
"#;

        let (_temp_dir, composer_path) = create_temp_file("composer.json", content);
        let package_data = ComposerJsonParser::extract_first_package(&composer_path);

        assert_eq!(
            package_data.download_url.as_deref(),
            Some("https://example.com/demo.zip")
        );
        assert_eq!(
            package_data.vcs_url.as_deref(),
            Some("git+https://github.com/acme/demo.git@abc123")
        );
        assert_eq!(package_data.parties.len(), 2);
        assert_eq!(package_data.parties[0].r#type.as_deref(), Some("person"));
        assert_eq!(package_data.parties[1].r#type.as_deref(), Some("person"));
    }

    #[test]
    fn test_extract_spdx_license_id_into_declared_fields() {
        let content = r#"
{
  "name": "acme/demo",
  "license": "MIT"
}
"#;

        let (_temp_dir, composer_path) = create_temp_file("composer.json", content);
        let package_data = ComposerJsonParser::extract_first_package(&composer_path);

        assert_eq!(
            package_data.extracted_license_statement.as_deref(),
            Some("MIT")
        );
        assert_eq!(
            package_data.declared_license_expression.as_deref(),
            Some("mit")
        );
        assert_eq!(
            package_data.declared_license_expression_spdx.as_deref(),
            Some("MIT")
        );
        assert_eq!(package_data.license_detections.len(), 1);

        let detection = &package_data.license_detections[0];
        assert_eq!(detection.license_expression, "mit");
        assert_eq!(detection.license_expression_spdx, "MIT");
        assert_eq!(detection.matches.len(), 1);
        assert_eq!(detection.matches[0].matched_text.as_deref(), Some("MIT"));
    }

    #[test]
    fn test_extract_spdx_license_expression_into_declared_fields() {
        let content = r#"
{
  "name": "acme/demo",
  "license": "MIT OR Apache-2.0"
}
"#;

        let (_temp_dir, composer_path) = create_temp_file("composer.json", content);
        let package_data = ComposerJsonParser::extract_first_package(&composer_path);

        assert_eq!(
            package_data.extracted_license_statement.as_deref(),
            Some("MIT OR Apache-2.0")
        );
        assert_eq!(
            package_data.declared_license_expression.as_deref(),
            Some("mit OR apache-2.0")
        );
        assert_eq!(
            package_data.declared_license_expression_spdx.as_deref(),
            Some("MIT OR Apache-2.0")
        );
        assert_eq!(package_data.license_detections.len(), 1);
    }

    #[test]
    fn test_extract_spdx_with_expression_into_declared_fields() {
        let content = r#"
{
  "name": "acme/demo",
  "license": "GPL-2.0-only WITH Classpath-exception-2.0"
}
"#;

        let (_temp_dir, composer_path) = create_temp_file("composer.json", content);
        let package_data = ComposerJsonParser::extract_first_package(&composer_path);

        assert_eq!(
            package_data.extracted_license_statement.as_deref(),
            Some("GPL-2.0-only WITH Classpath-exception-2.0")
        );
        assert_eq!(
            package_data.declared_license_expression.as_deref(),
            Some("gpl-2.0-only WITH classpath-exception-2.0")
        );
        assert_eq!(
            package_data.declared_license_expression_spdx.as_deref(),
            Some("GPL-2.0-only WITH Classpath-exception-2.0")
        );
        assert_eq!(package_data.license_detections.len(), 1);
    }

    #[test]
    fn test_extract_spdx_license_list_into_declared_fields() {
        let content = r#"
{
  "name": "acme/demo",
  "license": ["MIT", "Apache-2.0"]
}
"#;

        let (_temp_dir, composer_path) = create_temp_file("composer.json", content);
        let package_data = ComposerJsonParser::extract_first_package(&composer_path);

        assert_eq!(
            package_data.extracted_license_statement.as_deref(),
            Some("MIT OR Apache-2.0")
        );
        assert_eq!(
            package_data.declared_license_expression.as_deref(),
            Some("mit OR apache-2.0")
        );
        assert_eq!(
            package_data.declared_license_expression_spdx.as_deref(),
            Some("MIT OR Apache-2.0")
        );
        assert_eq!(package_data.license_detections.len(), 1);
    }

    #[test]
    fn test_preserve_custom_license_as_extracted_only() {
        let content = r#"
{
  "name": "acme/demo",
  "license": "Acme Commercial License"
}
"#;

        let (_temp_dir, composer_path) = create_temp_file("composer.json", content);
        let package_data = ComposerJsonParser::extract_first_package(&composer_path);

        assert_eq!(
            package_data.extracted_license_statement.as_deref(),
            Some("Acme Commercial License")
        );
        assert_eq!(package_data.declared_license_expression, None);
        assert_eq!(package_data.declared_license_expression_spdx, None);
        assert!(package_data.license_detections.is_empty());
    }

    #[test]
    fn test_preserve_mixed_license_list_as_extracted_only() {
        let content = r#"
{
  "name": "acme/demo",
  "license": ["MIT", "Acme Commercial License"]
}
"#;

        let (_temp_dir, composer_path) = create_temp_file("composer.json", content);
        let package_data = ComposerJsonParser::extract_first_package(&composer_path);

        assert_eq!(
            package_data.extracted_license_statement.as_deref(),
            Some("MIT OR Acme Commercial License")
        );
        assert_eq!(package_data.declared_license_expression, None);
        assert_eq!(package_data.declared_license_expression_spdx, None);
        assert!(package_data.license_detections.is_empty());
    }

    #[test]
    fn test_normalize_proprietary_license() {
        let content = r#"
{
  "name": "acme/demo",
  "license": "proprietary"
}
"#;

        let (_temp_dir, composer_path) = create_temp_file("composer.json", content);
        let package_data = ComposerJsonParser::extract_first_package(&composer_path);

        assert_eq!(
            package_data.extracted_license_statement.as_deref(),
            Some("proprietary")
        );
        assert_eq!(
            package_data.declared_license_expression.as_deref(),
            Some("proprietary-license")
        );
        assert_eq!(
            package_data.declared_license_expression_spdx.as_deref(),
            Some("LicenseRef-scancode-proprietary-license")
        );
        assert_eq!(package_data.license_detections.len(), 1);
    }

    #[test]
    fn test_private_package_without_license_falls_back_to_proprietary() {
        let content = r#"
{
  "description": "Private package without name or license"
}
"#;

        let (_temp_dir, composer_path) = create_temp_file("composer.json", content);
        let package_data = ComposerJsonParser::extract_first_package(&composer_path);

        assert!(package_data.is_private);
        assert_eq!(
            package_data.extracted_license_statement.as_deref(),
            Some("proprietary-license")
        );
        assert_eq!(
            package_data.declared_license_expression.as_deref(),
            Some("proprietary-license")
        );
        assert_eq!(
            package_data.declared_license_expression_spdx.as_deref(),
            Some("LicenseRef-scancode-proprietary-license")
        );
        assert_eq!(package_data.license_detections.len(), 1);
    }

    #[test]
    fn test_extract_large_composer_lock() {
        let mut packages = String::new();
        let mut packages_dev = String::new();

        for i in 0..400 {
            if i > 0 {
                packages.push(',');
                packages_dev.push(',');
            }
            packages.push_str(&format!(
                r#"{{"name":"vendor/runtime-{i}","version":"1.{i}.0","type":"library"}}"#
            ));
            packages_dev.push_str(&format!(
                r#"{{"name":"vendor/dev-{i}","version":"2.{i}.0","type":"library"}}"#
            ));
        }

        let content = format!(r#"{{"packages":[{packages}],"packages-dev":[{packages_dev}]}}"#);

        let (_temp_dir, composer_path) = create_temp_file("composer.lock", &content);
        let package_data = ComposerLockParser::extract_first_package(&composer_path);

        assert_eq!(package_data.dependencies.len(), 800);
        assert!(
            package_data
                .dependencies
                .iter()
                .any(|dep| dep.purl.as_deref() == Some("pkg:composer/vendor/runtime-0@1.0.0"))
        );
        assert!(
            package_data
                .dependencies
                .iter()
                .any(|dep| dep.purl.as_deref() == Some("pkg:composer/vendor/dev-399@2.399.0"))
        );
    }

    #[test]
    fn test_extract_lock_package_relationships() {
        let content = sample_composer_lock();
        let (_temp_dir, composer_path) = create_temp_file("composer.lock", &content);
        let package_data = ComposerLockParser::extract_first_package(&composer_path);

        assert_eq!(package_data.dependencies.len(), 7);

        let runtime_dep = find_dependency(
            &package_data.dependencies,
            "pkg:composer/acme/runtime@1.0.0",
        );
        assert_eq!(runtime_dep.scope.as_deref(), Some("require"));
        assert_eq!(runtime_dep.is_runtime, Some(true));
        assert_eq!(runtime_dep.is_optional, Some(false));

        let php_dep = find_dependency(&package_data.dependencies, "pkg:composer/php");
        assert_eq!(php_dep.extracted_requirement.as_deref(), Some("^8.2"));
        assert_eq!(php_dep.scope.as_deref(), Some("require"));

        let shared_dep = find_dependency(&package_data.dependencies, "pkg:composer/acme/shared");
        assert_eq!(shared_dep.extracted_requirement.as_deref(), Some("^3.0"));
        assert_eq!(shared_dep.scope.as_deref(), Some("require"));

        let runtime_testkit_dep = find_dependency(
            &package_data.dependencies,
            "pkg:composer/acme/runtime-testkit",
        );
        assert_eq!(runtime_testkit_dep.scope.as_deref(), Some("require-dev"));
        assert_eq!(runtime_testkit_dep.is_runtime, Some(false));
        assert_eq!(runtime_testkit_dep.is_optional, Some(true));

        let conflict_dep = find_dependency(
            &package_data.dependencies,
            "pkg:composer/acme/legacy-runtime",
        );
        assert_eq!(conflict_dep.scope.as_deref(), Some("conflict"));
        assert_eq!(conflict_dep.is_runtime, Some(true));
        assert_eq!(conflict_dep.is_optional, Some(true));

        let suggest_dep = find_dependency(&package_data.dependencies, "pkg:composer/ext-pcntl");
        assert_eq!(suggest_dep.scope.as_deref(), Some("suggest"));
        assert_eq!(
            suggest_dep.extracted_requirement.as_deref(),
            Some("Improves worker support")
        );

        let dev_package_dep =
            find_dependency(&package_data.dependencies, "pkg:composer/acme/devpkg@2.0.0");
        assert_eq!(dev_package_dep.scope.as_deref(), Some("require-dev"));
        assert_eq!(dev_package_dep.is_runtime, Some(false));
        assert_eq!(dev_package_dep.is_optional, Some(true));
    }

    #[test]
    fn test_graceful_error_handling() {
        let content = r#"{ invalid-json }"#;

        let (_temp_dir, composer_path) = create_temp_file("composer.json", content);
        let package_data = ComposerJsonParser::extract_first_package(&composer_path);

        assert_eq!(package_data.package_type, Some(PackageType::Composer));
        assert!(package_data.name.is_none());
        assert!(package_data.dependencies.is_empty());
    }

    #[test]
    fn test_extract_hashes() {
        let content = sample_composer_lock();
        let (_temp_dir, composer_path) = create_temp_file("composer.lock", &content);
        let package_data = ComposerLockParser::extract_first_package(&composer_path);

        let runtime_dep = find_dependency(
            &package_data.dependencies,
            "pkg:composer/acme/runtime@1.0.0",
        );
        let resolved = runtime_dep
            .resolved_package
            .as_ref()
            .expect("Expected resolved package for lock dependency");
        assert_eq!(
            resolved.sha1,
            Some(Sha1Digest::from_hex("cccccccccccccccccccccccccccccccccccccccc").unwrap())
        );

        let extra_data = runtime_dep
            .extra_data
            .as_ref()
            .expect("Expected extra_data");
        assert_eq!(
            extra_data
                .get("source_reference")
                .and_then(|value| value.as_str()),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert_eq!(
            extra_data
                .get("dist_reference")
                .and_then(|value| value.as_str()),
            Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        );
    }

    #[test]
    fn test_extract_package_type() {
        let content = sample_composer_lock();
        let (_temp_dir, composer_path) = create_temp_file("composer.lock", &content);
        let package_data = ComposerLockParser::extract_first_package(&composer_path);

        let runtime_dep = find_dependency(
            &package_data.dependencies,
            "pkg:composer/acme/runtime@1.0.0",
        );
        let extra_data = runtime_dep
            .extra_data
            .as_ref()
            .expect("Expected extra_data");
        assert_eq!(
            extra_data.get("type").and_then(|value| value.as_str()),
            Some("library")
        );
    }

    #[test]
    fn test_no_unwrap_no_expect() {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("src/parsers/composer.rs");
        let content = fs::read_to_string(&path).expect("Failed to read composer.rs");

        assert!(!content.contains(".unwrap()"));
        assert!(!content.contains(".expect("));
    }

    #[test]
    fn test_extract_suggest_dependencies() {
        let content = r#"
{
  "name": "acme/demo",
  "suggest": {
    "ext-redis": "For better performance",
    "acme/optional-pkg": "^2.0"
  }
}
"#;

        let (_temp_dir, composer_path) = create_temp_file("composer.json", content);
        let package_data = ComposerJsonParser::extract_first_package(&composer_path);

        assert_eq!(package_data.dependencies.len(), 2);

        let redis_dep = find_dependency(&package_data.dependencies, "pkg:composer/ext-redis");
        assert_eq!(redis_dep.scope.as_deref(), Some("suggest"));
        assert_eq!(redis_dep.is_runtime, Some(true));
        assert_eq!(redis_dep.is_optional, Some(true));
        assert_eq!(
            redis_dep.extracted_requirement.as_deref(),
            Some("For better performance")
        );

        let optional_dep =
            find_dependency(&package_data.dependencies, "pkg:composer/acme/optional-pkg");
        assert_eq!(optional_dep.scope.as_deref(), Some("suggest"));
        assert_eq!(optional_dep.is_runtime, Some(true));
        assert_eq!(optional_dep.is_optional, Some(true));
    }

    #[test]
    fn test_extract_provide_dependencies() {
        let content = r#"
{
  "name": "acme/demo",
  "provide": {
    "acme/interface": "1.0",
    "virtual/package": "2.0"
  }
}
"#;

        let (_temp_dir, composer_path) = create_temp_file("composer.json", content);
        let package_data = ComposerJsonParser::extract_first_package(&composer_path);

        assert_eq!(package_data.dependencies.len(), 2);

        let interface_dep =
            find_dependency(&package_data.dependencies, "pkg:composer/acme/interface");
        assert_eq!(interface_dep.scope.as_deref(), Some("provide"));
        assert_eq!(interface_dep.is_runtime, Some(true));
        assert_eq!(interface_dep.is_optional, Some(false));
    }

    #[test]
    fn test_extract_conflict_dependencies() {
        let content = r#"
{
  "name": "acme/demo",
  "conflict": {
    "acme/incompatible": "1.0.*"
  }
}
"#;

        let (_temp_dir, composer_path) = create_temp_file("composer.json", content);
        let package_data = ComposerJsonParser::extract_first_package(&composer_path);

        assert_eq!(package_data.dependencies.len(), 1);

        let conflict_dep =
            find_dependency(&package_data.dependencies, "pkg:composer/acme/incompatible");
        assert_eq!(conflict_dep.scope.as_deref(), Some("conflict"));
        assert_eq!(conflict_dep.is_runtime, Some(true));
        assert_eq!(conflict_dep.is_optional, Some(true));
    }

    #[test]
    fn test_extract_replace_dependencies() {
        let content = r#"
{
  "name": "acme/demo",
  "replace": {
    "acme/old-package": "self.version"
  }
}
"#;

        let (_temp_dir, composer_path) = create_temp_file("composer.json", content);
        let package_data = ComposerJsonParser::extract_first_package(&composer_path);

        assert_eq!(package_data.dependencies.len(), 1);

        let replace_dep =
            find_dependency(&package_data.dependencies, "pkg:composer/acme/old-package");
        assert_eq!(replace_dep.scope.as_deref(), Some("replace"));
        assert_eq!(replace_dep.is_runtime, Some(true));
        assert_eq!(replace_dep.is_optional, Some(true));
    }

    #[test]
    fn test_extract_support() {
        let content = r#"
{
  "name": "acme/demo",
  "support": {
    "issues": "https://github.com/acme/demo/issues",
    "source": "https://github.com/acme/demo",
    "docs": "https://docs.acme.com",
    "forum": "https://forum.acme.com"
  }
}
"#;

        let (_temp_dir, composer_path) = create_temp_file("composer.json", content);
        let package_data = ComposerJsonParser::extract_first_package(&composer_path);

        assert_eq!(
            package_data.bug_tracking_url,
            Some("https://github.com/acme/demo/issues".to_string())
        );
        assert_eq!(
            package_data.code_view_url,
            Some("https://github.com/acme/demo".to_string())
        );
    }

    #[test]
    fn test_all_dependency_types_combined() {
        let content = r#"
{
  "name": "acme/demo",
  "require": {
    "php": ">=8.0"
  },
  "require-dev": {
    "phpunit/phpunit": "^9.0"
  },
  "suggest": {
    "ext-redis": "For caching"
  },
  "provide": {
    "psr/log-implementation": "1.0"
  },
  "conflict": {
    "acme/old": "1.0"
  },
  "replace": {
    "acme/deprecated": "self.version"
  }
}
"#;

        let (_temp_dir, composer_path) = create_temp_file("composer.json", content);
        let package_data = ComposerJsonParser::extract_first_package(&composer_path);

        assert_eq!(package_data.dependencies.len(), 6);

        assert!(
            package_data
                .dependencies
                .iter()
                .any(|d| d.scope.as_deref() == Some("require"))
        );
        assert!(
            package_data
                .dependencies
                .iter()
                .any(|d| d.scope.as_deref() == Some("require-dev"))
        );
        assert!(
            package_data
                .dependencies
                .iter()
                .any(|d| d.scope.as_deref() == Some("suggest"))
        );
        assert!(
            package_data
                .dependencies
                .iter()
                .any(|d| d.scope.as_deref() == Some("provide"))
        );
        assert!(
            package_data
                .dependencies
                .iter()
                .any(|d| d.scope.as_deref() == Some("conflict"))
        );
        assert!(
            package_data
                .dependencies
                .iter()
                .any(|d| d.scope.as_deref() == Some("replace"))
        );
    }
}
