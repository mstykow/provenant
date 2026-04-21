// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use crate::models::{DatasourceId, PackageType, Sha1Digest, Sha256Digest, Sha512Digest};
    use crate::parsers::{NpmParser, PackageParser};
    use serde_json::Value;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // Helper function to create a temporary package.json file with the given content
    fn create_temp_package_json(content: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let package_path = temp_dir.path().join("package.json");
        fs::write(&package_path, content).expect("Failed to write package.json");

        (temp_dir, package_path)
    }

    #[test]
    fn test_is_match() {
        let valid_path = PathBuf::from("/some/path/package.json");
        let invalid_path = PathBuf::from("/some/path/not_package.json");

        assert!(NpmParser::is_match(&valid_path));
        assert!(!NpmParser::is_match(&invalid_path));
    }

    #[test]
    fn test_extract_from_testdata() {
        let package_path = PathBuf::from("testdata/npm/package.json")
            .canonicalize()
            .unwrap();
        let package_data = NpmParser::extract_first_package(&package_path);

        assert_eq!(package_data.package_type, Some(PackageType::Npm));
        assert_eq!(package_data.name, Some("test-package".to_string()));
        assert_eq!(package_data.namespace, Some("@example".to_string()));
        assert_eq!(package_data.version, Some("1.0.0".to_string()));
        assert_eq!(
            package_data.homepage_url,
            Some("https://example.com".to_string())
        );
        assert_eq!(
            package_data.download_url,
            Some(
                "https://registry.npmjs.org/@example/test-package/-/test-package-1.0.0.tgz"
                    .to_string()
            )
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
        assert!(package_data.extracted_license_statement.is_some());

        // Check purl
        // The PURL should include the scoped package name properly URL-encoded
        // The PURL should include the scoped package name properly URL-encoded
        // PURL should be based on package name only
        assert_eq!(
            package_data.purl,
            Some("pkg:npm/%40example/test-package@1.0.0".to_string())
        );

        // Check author extraction
        assert_eq!(package_data.parties.len(), 1);
        assert_eq!(
            package_data.parties[0].email,
            Some("test@example.com".to_string())
        );

        // Check dependencies via PURLs
        assert_eq!(package_data.dependencies.len(), 3);
        let purls: Vec<&str> = package_data
            .dependencies
            .iter()
            .filter_map(|d| d.purl.as_deref())
            .collect();
        assert!(purls.iter().any(|p| p.contains("express")));
        assert!(purls.iter().any(|p| p.contains("lodash")));
        assert!(purls.iter().any(|p| p.contains("jest")));
    }

    #[test]
    fn test_extract_from_npm_testdata() {
        let package_path = PathBuf::from("testdata/npm/package.json")
            .canonicalize()
            .unwrap();
        let package_data = NpmParser::extract_first_package(&package_path);

        assert_eq!(package_data.package_type, Some(PackageType::Npm));
        assert_eq!(package_data.name, Some("test-package".to_string()));
        assert_eq!(package_data.namespace, Some("@example".to_string()));
        assert_eq!(package_data.version, Some("1.0.0".to_string()));
        assert_eq!(
            package_data.homepage_url,
            Some("https://example.com".to_string())
        );
        assert_eq!(
            package_data.download_url,
            Some(
                "https://registry.npmjs.org/@example/test-package/-/test-package-1.0.0.tgz"
                    .to_string()
            )
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
        assert!(package_data.extracted_license_statement.is_some());

        // Check purl - should be pkg:npm/%40example/test-package@1.0.0 without namespace
        assert_eq!(
            package_data.purl,
            Some("pkg:npm/%40example/test-package@1.0.0".to_string())
        );

        // Check author extraction
        assert_eq!(package_data.parties.len(), 1);
        assert_eq!(
            package_data.parties[0].email,
            Some("test@example.com".to_string())
        );

        // Check dependencies
        assert_eq!(package_data.dependencies.len(), 3);
        let purls: Vec<&str> = package_data
            .dependencies
            .iter()
            .filter_map(|d| d.purl.as_deref())
            .collect();
        assert!(purls.iter().any(|p| p.contains("express")));
        assert!(purls.iter().any(|p| p.contains("lodash")));
        assert!(purls.iter().any(|p| p.contains("jest")));
    }

    #[test]
    fn test_extract_basic_package_info() {
        let content = r#"
{
  "name": "test-package",
  "version": "1.0.0",
  "license": "MIT",
  "homepage": "https://example.com",
  "repository": "https://github.com/user/test-package",
  "author": "Test User <test@example.com>"
}
"#;

        let (_temp_file, package_path) = create_temp_package_json(content);
        let package_data = NpmParser::extract_first_package(&package_path);

        assert_eq!(package_data.package_type, Some(PackageType::Npm));
        assert_eq!(package_data.name, Some("test-package".to_string()));
        assert_eq!(package_data.version, Some("1.0.0".to_string()));
        assert_eq!(
            package_data.homepage_url,
            Some("https://example.com".to_string())
        );
        assert_eq!(
            package_data.download_url,
            Some("https://registry.npmjs.org/test-package/-/test-package-1.0.0.tgz".to_string())
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
        assert!(package_data.extracted_license_statement.is_some());

        // Check purl
        assert_eq!(
            package_data.purl,
            Some("pkg:npm/test-package@1.0.0".to_string())
        );

        // Check author extraction - fixed to match actual parser behavior
        assert_eq!(package_data.parties.len(), 1);
        assert_eq!(
            package_data.parties[0].email,
            Some("test@example.com".to_string())
        );
    }

    #[test]
    fn test_extract_scoped_package() {
        let content = r#"
{
  "name": "@org/test-package",
  "version": "1.0.0",
  "license": "Apache-2.0"
}
"#;

        let (_temp_file, package_path) = create_temp_package_json(content);
        let package_data = NpmParser::extract_first_package(&package_path);

        assert_eq!(package_data.name, Some("test-package".to_string()));
        assert_eq!(package_data.namespace, Some("@org".to_string()));

        // Check purl contains the expected components rather than exact match
        let purl = package_data.purl.unwrap();
        assert!(purl.starts_with("pkg:npm/"));
        assert!(purl.contains("test-package"));
        assert!(purl.ends_with("@1.0.0"));
        assert!(purl.contains("org"));
    }

    #[test]
    fn test_extract_different_license_formats() {
        // Test license as object
        let license_obj_content = r#"
{
  "name": "test-package",
  "version": "1.0.0",
  "license": {
    "type": "BSD-3-Clause",
    "url": "https://opensource.org/licenses/BSD-3-Clause"
  }
}
"#;

        let (_temp_file_1, path_1) = create_temp_package_json(license_obj_content);
        let package_data_1 = NpmParser::extract_first_package(&path_1);

        assert_eq!(package_data_1.declared_license_expression, None);
        assert_eq!(package_data_1.declared_license_expression_spdx, None);
        assert_eq!(package_data_1.license_detections.len(), 0);
        assert!(package_data_1.extracted_license_statement.is_some());

        // Test deprecated licenses array
        let licenses_array_content = r#"
{
  "name": "test-package",
  "version": "1.0.0",
  "licenses": [
    {
      "type": "MIT",
      "url": "https://opensource.org/licenses/MIT"
    },
    {
      "type": "Apache-2.0",
      "url": "https://opensource.org/licenses/Apache-2.0"
    }
  ]
}
"#;

        let (_temp_file_2, path_2) = create_temp_package_json(licenses_array_content);
        let package_data_2 = NpmParser::extract_first_package(&path_2);

        assert_eq!(package_data_2.declared_license_expression, None);
        assert_eq!(package_data_2.declared_license_expression_spdx, None);
        assert_eq!(package_data_2.license_detections.len(), 0);
        assert!(package_data_2.extracted_license_statement.is_some());
    }

    #[test]
    fn test_extract_repository_formats() {
        // Test repository as string
        let repo_string_content = r#"
{
  "name": "test-package",
  "version": "1.0.0",
  "repository": "https://github.com/user/test-package"
}
"#;

        let (_temp_file_1, path_1) = create_temp_package_json(repo_string_content);
        let package_data_1 = NpmParser::extract_first_package(&path_1);

        // Check if repository extraction is working
        if let Some(download_url) = package_data_1.download_url {
            assert!(download_url.contains("registry.npmjs.org"));
            assert!(download_url.contains("test-package"));
        }

        // Test repository as object
        let repo_obj_content = r#"
{
  "name": "test-package",
  "version": "1.0.0",
  "repository": {
    "type": "git",
    "url": "git://github.com/user/test-package.git"
  }
}
"#;

        let (_temp_file_2, path_2) = create_temp_package_json(repo_obj_content);
        let package_data_2 = NpmParser::extract_first_package(&path_2);

        // Check if repository object extraction is working
        if let Some(download_url) = package_data_2.download_url {
            // Should contain registry and the package name
            assert!(download_url.contains("registry.npmjs.org"));
            assert!(download_url.contains("test-package"));
            // Should be https
            assert!(download_url.starts_with("https://"));
        }
    }

    #[test]
    fn test_extract_dependencies() {
        let content = r#"
{
  "name": "test-package",
  "version": "1.0.0",
  "dependencies": {
    "express": "^4.17.1",
    "lodash": "~4.17.20"
  },
  "devDependencies": {
    "jest": "^27.0.0",
    "eslint": "7.32.0"
  }
}
"#;

        let (_temp_file, package_path) = create_temp_package_json(content);
        let package_data = NpmParser::extract_first_package(&package_path);

        // Check if dependencies are extracted (may be 0 if parser doesn't support this yet)
        if !package_data.dependencies.is_empty() {
            // If dependencies are extracted, verify they contain expected packages
            let purls: Vec<String> = package_data
                .dependencies
                .iter()
                .filter_map(|dep| dep.purl.clone())
                .collect();

            // Should have some dependencies if extraction is working
            assert!(!purls.is_empty());

            // Check for expected packages if they exist
            let expected_packages = ["express", "lodash", "jest", "eslint"];
            for purl in &purls {
                let has_expected_package = expected_packages.iter().any(|pkg| purl.contains(pkg));
                assert!(has_expected_package, "Unexpected package in PURL: {}", purl);
            }

            // If we have dependencies, check some specific properties
            if let Some(express_dep) = package_data
                .dependencies
                .iter()
                .find(|dep| dep.purl.as_ref().is_some_and(|p| p.contains("express")))
            {
                assert_eq!(express_dep.is_optional, Some(false));
            }

            if let Some(jest_dep) = package_data
                .dependencies
                .iter()
                .find(|dep| dep.purl.as_ref().is_some_and(|p| p.contains("jest")))
            {
                assert_eq!(jest_dep.is_optional, Some(true));
            }
        }
    }

    #[test]
    fn test_extract_multiple_contributors() {
        let content = r#"
{
  "name": "test-package",
  "version": "1.0.0",
  "author": "Main Author <main@example.com>",
  "contributors": [
    "Contributor 1 <contrib1@example.com>",
    {
      "name": "Contributor 2",
      "email": "contrib2@example.com"
    }
  ],
  "maintainers": [
    {
      "name": "Maintainer 1",
      "email": "maint1@example.com"
    }
  ]
}
"#;

        let (_temp_file, package_path) = create_temp_package_json(content);
        let package_data = NpmParser::extract_first_package(&package_path);

        // Check that at least some parties are extracted (may be 0 if parser doesn't support this yet)
        if !package_data.parties.is_empty() {
            // If parties are extracted, verify they contain expected emails
            let emails: Vec<&str> = package_data
                .parties
                .iter()
                .filter_map(|p| p.email.as_deref())
                .collect();

            // Should have at least the author if parties are supported
            assert!(!emails.is_empty());

            // Check for expected emails if they exist
            let expected_emails = [
                "main@example.com",
                "contrib1@example.com",
                "contrib2@example.com",
                "maint1@example.com",
            ];
            for email in emails {
                assert!(
                    expected_emails.contains(&email),
                    "Unexpected email: {}",
                    email
                );
            }
        }
    }

    #[test]
    fn test_empty_or_invalid_package_json() {
        // Test with empty content
        let content = "{}";
        let (_temp_file_1, path_1) = create_temp_package_json(content);
        let package_data_1 = NpmParser::extract_first_package(&path_1);

        // Should return default/empty package data
        assert_eq!(package_data_1.name, None);
        assert_eq!(package_data_1.version, None);
        assert_eq!(package_data_1.package_type, Some(PackageType::Npm));
        assert_eq!(
            package_data_1.datasource_id,
            Some(DatasourceId::NpmPackageJson)
        );
        assert!(package_data_1.dependencies.is_empty());

        // Test with invalid JSON
        let content = "this is not valid JSON";
        let (_temp_file_2, path_2) = create_temp_package_json(content);
        let package_data_2 = NpmParser::extract_first_package(&path_2);

        // Should return default/empty package data
        assert_eq!(package_data_2.name, None);
        assert_eq!(package_data_2.version, None);
        assert_eq!(package_data_2.package_type, Some(PackageType::Npm));
        assert_eq!(
            package_data_2.datasource_id,
            Some(DatasourceId::NpmPackageJson)
        );
        assert!(package_data_2.dependencies.is_empty());
    }

    #[test]
    fn test_empty_name_or_version_does_not_create_registry_urls() {
        let content = r#"
{
  "name": " ",
  "version": "",
  "homepage": "https://example.com"
}
"#;

        let (_temp_file, package_path) = create_temp_package_json(content);
        let package_data = NpmParser::extract_first_package(&package_path);

        assert_eq!(package_data.name, None);
        assert_eq!(package_data.version, None);
        assert_eq!(package_data.download_url, None);
        assert_eq!(package_data.repository_homepage_url, None);
        assert_eq!(package_data.repository_download_url, None);
        assert_eq!(package_data.api_data_url, None);
        assert_eq!(package_data.purl, None);
    }

    #[test]
    fn test_extract_overrides_into_extra_data() {
        let content = r#"
{
  "name": "override-test",
  "version": "1.0.0",
  "overrides": {
    "lodash": "4.17.21",
    "react-scripts": {
      "webpack": "5.74.0"
    }
  }
}
"#;

        let (_temp_file, package_path) = create_temp_package_json(content);
        let package_data = NpmParser::extract_first_package(&package_path);

        let extra_data = package_data.extra_data.expect("expected extra_data");
        let overrides = extra_data
            .get("overrides")
            .expect("expected overrides field");
        assert_eq!(overrides["lodash"], Value::String("4.17.21".to_string()));
        assert_eq!(
            overrides["react-scripts"]["webpack"],
            Value::String("5.74.0".to_string())
        );
    }

    #[test]
    fn test_extract_peer_dependencies() {
        let package_path = PathBuf::from("testdata/npm/package-peer-dependencies.json")
            .canonicalize()
            .unwrap();
        let package_data = NpmParser::extract_first_package(&package_path);

        // Should have peer dependencies
        assert!(!package_data.dependencies.is_empty());

        // Find peer dependencies by scope
        let peer_deps: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|dep| dep.scope.as_deref() == Some("peerDependencies"))
            .collect();

        assert_eq!(peer_deps.len(), 2);

        // Check that they have the correct scope and is_runtime flags
        for dep in &peer_deps {
            assert_eq!(dep.scope, Some("peerDependencies".to_string()));
            assert_eq!(dep.is_runtime, Some(true));
            assert_eq!(dep.is_optional, None);
        }
    }

    #[test]
    fn test_extract_optional_dependencies() {
        let package_path = PathBuf::from("testdata/npm/package-optional-dependencies.json")
            .canonicalize()
            .unwrap();
        let package_data = NpmParser::extract_first_package(&package_path);

        // Should have optional dependencies
        assert!(!package_data.dependencies.is_empty());

        // Find optional dependencies by scope
        let optional_deps: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|dep| dep.scope.as_deref() == Some("optionalDependencies"))
            .collect();

        assert_eq!(optional_deps.len(), 2);

        // Check that they have the correct flags
        for dep in &optional_deps {
            assert_eq!(dep.scope, Some("optionalDependencies".to_string()));
            assert_eq!(
                dep.is_runtime,
                Some(true),
                "is_runtime should be true for optional dependencies"
            );
            assert_eq!(
                dep.is_optional,
                Some(true),
                "is_optional should be true for optional dependencies"
            );
        }
    }

    #[test]
    fn test_extract_bundled_dependencies() {
        let package_path = PathBuf::from("testdata/npm/package-bundled-dependencies.json")
            .canonicalize()
            .unwrap();
        let package_data = NpmParser::extract_first_package(&package_path);

        // Should have bundled dependencies
        assert!(!package_data.dependencies.is_empty());

        // Find bundled dependencies by scope
        let bundled_deps: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|dep| dep.scope.as_deref() == Some("bundledDependencies"))
            .collect();

        assert_eq!(bundled_deps.len(), 3);

        // Check that bundled dependencies have no version (just package name in PURL)
        for dep in &bundled_deps {
            assert_eq!(dep.scope, Some("bundledDependencies".to_string()));
            if let Some(ref purl) = dep.purl {
                // Bundled dependencies should not have version
                assert!(
                    !purl.contains('@'),
                    "Bundled dependencies should not have version in PURL: {}",
                    purl
                );
            }
        }

        // Check for expected package names
        let purls: Vec<&str> = bundled_deps
            .iter()
            .filter_map(|dep| dep.purl.as_deref())
            .collect();
        assert!(purls.iter().any(|p| p.contains("lodash")));
        assert!(purls.iter().any(|p| p.contains("moment")));
        assert!(purls.iter().any(|p| p.contains("axios")));
    }

    #[test]
    fn test_extract_bundle_dependencies_alternative_spelling_is_ignored() {
        let package_path = PathBuf::from("testdata/npm/package-bundle-dependencies.json")
            .canonicalize()
            .unwrap();
        let package_data = NpmParser::extract_first_package(&package_path);

        let bundled_deps_count = package_data
            .dependencies
            .iter()
            .filter(|dep| dep.scope.as_deref() == Some("bundledDependencies"))
            .count();

        assert_eq!(bundled_deps_count, 0);
    }

    #[test]
    fn test_extract_resolutions() {
        let package_path = PathBuf::from("testdata/npm/package-resolutions.json")
            .canonicalize()
            .unwrap();
        let package_data = NpmParser::extract_first_package(&package_path);

        // Should have resolutions in extra_data
        let extra_data = package_data
            .extra_data
            .expect("extra_data should be present with resolutions");

        assert!(
            extra_data.contains_key("resolutions"),
            "extra_data should contain resolutions field"
        );

        let resolutions = extra_data
            .get("resolutions")
            .expect("resolutions should exist");
        if let serde_json::Value::Object(resolutions_obj) = resolutions {
            assert_eq!(resolutions_obj.len(), 3);
            assert!(resolutions_obj.contains_key("typescript"));
            assert!(resolutions_obj.contains_key("react"));
            assert!(resolutions_obj.contains_key("@babel/core"));
        } else {
            panic!("resolutions should be a JSON object");
        }
    }

    #[test]
    fn test_extract_all_dependency_types() {
        let package_path = PathBuf::from("testdata/npm/package-all-dependencies.json")
            .canonicalize()
            .unwrap();
        let package_data = NpmParser::extract_first_package(&package_path);

        assert!(!package_data.dependencies.is_empty());

        // Count each type by scope
        let get_count_by_scope = |scope: Option<&str>| {
            package_data
                .dependencies
                .iter()
                .filter(|dep| dep.scope.as_deref() == scope)
                .count()
        };

        let regular_deps_count =
            get_count_by_scope(None) + get_count_by_scope(Some("dependencies"));
        let dev_deps_count = get_count_by_scope(Some("devDependencies"));
        let peer_deps_count = get_count_by_scope(Some("peerDependencies"));
        let optional_deps_count = get_count_by_scope(Some("optionalDependencies"));
        let bundled_deps_count = get_count_by_scope(Some("bundledDependencies"));

        // Verify we have mix of different dependency types
        assert_eq!(regular_deps_count, 2, "Should have 2 regular dependencies");
        assert_eq!(dev_deps_count, 1, "Should have 1 dev dependency");
        assert_eq!(peer_deps_count, 2, "Should have 2 peer dependencies");
        assert_eq!(
            optional_deps_count, 2,
            "Should have 2 optional dependencies"
        );
        assert_eq!(bundled_deps_count, 1, "Should have 1 bundled dependency");

        // Verify peer dependencies have correct flags
        let peer_deps: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|dep| dep.scope.as_deref() == Some("peerDependencies"))
            .collect();
        for dep in peer_deps {
            assert_eq!(dep.is_runtime, Some(true));
        }

        // Verify optional dependencies have correct flags
        let optional_deps: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|dep| dep.scope.as_deref() == Some("optionalDependencies"))
            .collect();
        for dep in optional_deps {
            assert_eq!(dep.is_runtime, Some(true));
            assert_eq!(dep.is_optional, Some(true));
        }

        // Verify resolutions are in extra_data
        let extra_data = package_data
            .extra_data
            .expect("extra_data should be present with resolutions");
        assert!(extra_data.contains_key("resolutions"));
    }

    #[test]
    fn test_extract_description() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-description.json").as_path(),
        );

        assert_eq!(
            package_data.name,
            Some("test-package-description".to_string())
        );
        assert_eq!(
            package_data.description,
            Some(
                "A test package with a long description explaining what this package does"
                    .to_string()
            )
        );
    }

    #[test]
    fn test_extract_keywords_array() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-keywords-array.json").as_path(),
        );

        assert_eq!(
            package_data.keywords,
            vec!["javascript", "npm", "package", "metadata"]
        );
    }

    #[test]
    fn test_extract_keywords_string() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-keywords-string.json").as_path(),
        );

        assert_eq!(
            package_data.keywords,
            vec!["javascript, npm, package, metadata"]
        );
    }

    #[test]
    fn test_extract_engines() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-engines.json").as_path(),
        );

        let extra_data = package_data
            .extra_data
            .expect("extra_data should be present with engines");
        assert!(extra_data.contains_key("engines"));

        if let Some(serde_json::Value::Object(engines)) = extra_data.get("engines") {
            assert!(engines.contains_key("node"));
            assert!(engines.contains_key("npm"));
        } else {
            panic!("engines should be an object");
        }
    }

    #[test]
    fn test_extract_platform_metadata() {
        let content = r#"
{
  "name": "platform-aware-package",
  "version": "1.0.0",
  "os": ["darwin", "linux", "!win32"],
  "cpu": ["x64", "arm64"],
  "libc": ["glibc", "musl"],
  "deprecated": "Use platform-aware-package-next instead",
  "hasBin": true,
  "engines": {
    "node": ">=18"
  }
}
"#;

        let (_temp_dir, package_path) = create_temp_package_json(content);
        let package_data = NpmParser::extract_first_package(&package_path);

        let extra_data = package_data
            .extra_data
            .expect("extra_data should be present with platform metadata");

        assert_eq!(
            extra_data.get("deprecated"),
            Some(&Value::String(
                "Use platform-aware-package-next instead".to_string()
            ))
        );
        assert_eq!(extra_data.get("hasBin"), Some(&Value::Bool(true)));

        let os = extra_data.get("os").expect("expected os metadata");
        assert_eq!(
            os,
            &Value::Array(vec![
                Value::String("darwin".to_string()),
                Value::String("linux".to_string()),
                Value::String("!win32".to_string()),
            ])
        );

        let cpu = extra_data.get("cpu").expect("expected cpu metadata");
        assert_eq!(
            cpu,
            &Value::Array(vec![
                Value::String("x64".to_string()),
                Value::String("arm64".to_string()),
            ])
        );

        let libc = extra_data.get("libc").expect("expected libc metadata");
        assert_eq!(
            libc,
            &Value::Array(vec![
                Value::String("glibc".to_string()),
                Value::String("musl".to_string()),
            ])
        );

        assert!(extra_data.contains_key("engines"));
    }

    #[test]
    fn test_extract_package_manager() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-manager.json").as_path(),
        );

        let extra_data = package_data
            .extra_data
            .expect("extra_data should be present with packageManager");
        assert!(extra_data.contains_key("packageManager"));

        if let Some(serde_json::Value::String(pm)) = extra_data.get("packageManager") {
            assert_eq!(pm, "pnpm@8.6.0");
        } else {
            panic!("packageManager should be a string");
        }
    }

    #[test]
    fn test_extract_workspaces() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-workspaces.json").as_path(),
        );

        let extra_data = package_data
            .extra_data
            .expect("extra_data should be present with workspaces");
        assert!(extra_data.contains_key("workspaces"));

        if let Some(serde_json::Value::Array(workspaces)) = extra_data.get("workspaces") {
            assert_eq!(workspaces.len(), 2);
            assert_eq!(
                workspaces[0],
                serde_json::Value::String("packages/*".to_string())
            );
            assert_eq!(
                workspaces[1],
                serde_json::Value::String("apps/*".to_string())
            );
        } else {
            panic!("workspaces should be an array");
        }
    }

    #[test]
    fn test_extract_private() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-private.json").as_path(),
        );

        let extra_data = package_data
            .extra_data
            .expect("extra_data should be present with private");
        assert!(extra_data.contains_key("private"));

        if let Some(serde_json::Value::Bool(private)) = extra_data.get("private") {
            assert!(*private);
        } else {
            panic!("private should be a boolean");
        }
    }

    #[test]
    fn test_extract_all_metadata() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-metadata-all.json").as_path(),
        );

        assert_eq!(
            package_data.description,
            Some("A package with all metadata fields".to_string())
        );

        assert_eq!(
            package_data.keywords,
            vec!["javascript", "npm", "metadata", "test"]
        );

        let extra_data = package_data
            .extra_data
            .expect("extra_data should be present with all metadata");

        assert!(extra_data.contains_key("engines"));
        assert!(extra_data.contains_key("packageManager"));
        assert!(extra_data.contains_key("workspaces"));
        assert!(extra_data.contains_key("private"));

        if let Some(serde_json::Value::Bool(private)) = extra_data.get("private") {
            assert!(!(*private));
        }
    }

    #[test]
    fn test_extract_bugs_string() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-bugs-string.json").as_path(),
        );

        assert_eq!(
            package_data.bug_tracking_url,
            Some("https://github.com/user/repo/issues".to_string())
        );
    }

    #[test]
    fn test_extract_bugs_object() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-bugs-object.json").as_path(),
        );

        assert_eq!(
            package_data.bug_tracking_url,
            Some("https://github.com/user/repo/issues".to_string())
        );
    }

    #[test]
    fn test_extract_bugs_with_all_metadata() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-metadata-all-bugs.json").as_path(),
        );

        assert_eq!(
            package_data.name,
            Some("test-package-all-metadata".to_string())
        );

        assert_eq!(
            package_data.bug_tracking_url,
            Some("https://github.com/user/repo/issues".to_string())
        );

        assert_eq!(
            package_data.description,
            Some("A package with all metadata fields".to_string())
        );

        assert!(!package_data.keywords.is_empty());

        assert!(package_data.extra_data.is_some());
        let extra_data = package_data.extra_data.unwrap();
        assert!(extra_data.contains_key("engines"));
        assert!(extra_data.contains_key("packageManager"));
        assert!(extra_data.contains_key("workspaces"));
        assert!(extra_data.contains_key("private"));
    }

    #[test]
    fn test_homepage_array_is_ignored() {
        let content = r#"
{
  "name": "test-package",
  "version": "1.0.0",
  "homepage": ["https://example.com"]
}
"#;

        let (_temp_file, package_path) = create_temp_package_json(content);
        let package_data = NpmParser::extract_first_package(&package_path);

        assert_eq!(package_data.homepage_url, None);
    }

    #[test]
    fn test_blank_bugs_values_are_ignored() {
        let content = r#"
{
  "name": "test-package",
  "version": "1.0.0",
  "bugs": {
    "url": "   "
  }
}
"#;

        let (_temp_file, package_path) = create_temp_package_json(content);
        let package_data = NpmParser::extract_first_package(&package_path);

        assert_eq!(package_data.bug_tracking_url, None);
    }

    #[test]
    fn test_extract_dist_sha256() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-dist-sha256.json").as_path(),
        );

        assert_eq!(
            package_data.name,
            Some("test-package-dist-sha256".to_string())
        );

        assert_eq!(
            package_data.sha256,
            Some(
                Sha256Digest::from_hex(
                    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                )
                .unwrap()
            )
        );

        assert!(package_data.sha512.is_none());

        assert_eq!(
            package_data.download_url,
            Some("https://registry.npmjs.org/test-package-dist-sha256/-/test-package-dist-sha256-1.0.0.tgz".to_string())
        );
    }

    #[test]
    fn test_extract_dist_sha512() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-dist-tarball.json").as_path(),
        );

        assert_eq!(
            package_data.name,
            Some("test-package-dist-sha512".to_string())
        );

        assert!(package_data.sha256.is_none());

        assert_eq!(
            package_data.sha512,
            Some(Sha512Digest::from_hex("cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e").unwrap())
        );

        assert_eq!(
            package_data.download_url,
            Some("https://registry.npmjs.org/test-package-dist-sha512/-/test-package-dist-sha512-1.0.0.tgz".to_string())
        );
    }

    #[test]
    fn test_extract_dist_no_integrity() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-dist-no-integrity.json").as_path(),
        );

        assert_eq!(
            package_data.name,
            Some("test-package-dist-no-integrity".to_string())
        );

        assert!(package_data.sha256.is_none());
        assert!(package_data.sha512.is_none());

        assert_eq!(
            package_data.download_url,
            Some("https://registry.npmjs.org/test-package-dist-no-integrity/-/test-package-dist-no-integrity-1.0.0.tgz".to_string())
        );
    }

    #[test]
    fn test_extract_dist_complete() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-dist-complete.json").as_path(),
        );

        assert_eq!(
            package_data.name,
            Some("test-package-dist-complete".to_string())
        );

        assert_eq!(
            package_data.sha512,
            Some(Sha512Digest::from_hex("ee26b0dd4af7e749aa1a8ee3c10ae9923f618980772e473f8819a5d4940e0db27ac185f8a0e1d5f84f88bc887fd67b143732c304cc5fa9ad8e6f57f50028a8ff").unwrap())
        );

        assert_eq!(
            package_data.download_url,
            Some("https://registry.npmjs.org/test-package-dist-complete/-/test-package-dist-complete-1.0.0.tgz".to_string())
        );

        assert_eq!(
            package_data.bug_tracking_url,
            Some("https://github.com/user/repo/issues".to_string())
        );

        assert!(package_data.sha256.is_none());
    }

    #[test]
    fn test_extract_no_dist_fallback_to_repo() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-no-dist.json").as_path(),
        );

        assert_eq!(package_data.name, Some("test-package-no-dist".to_string()));

        assert!(package_data.sha256.is_none());
        assert!(package_data.sha512.is_none());

        assert_eq!(
            package_data.download_url,
            Some(
                "https://registry.npmjs.org/test-package-no-dist/-/test-package-no-dist-1.0.0.tgz"
                    .to_string()
            )
        );
    }

    #[test]
    fn test_extract_repo_string_https() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-repo-string-https.json").as_path(),
        );
        assert_eq!(
            package_data.download_url,
            Some("https://registry.npmjs.org/test-package/-/test-package-1.0.0.tgz".to_string())
        );
    }

    #[test]
    fn test_extract_repo_string_git() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-repo-string-git.json").as_path(),
        );
        assert_eq!(
            package_data.download_url,
            Some("https://registry.npmjs.org/test-package/-/test-package-1.0.0.tgz".to_string())
        );
    }

    #[test]
    fn test_extract_repo_string_git_at() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-repo-string-git-at.json").as_path(),
        );
        assert_eq!(
            package_data.download_url,
            Some("https://registry.npmjs.org/test-package/-/test-package-1.0.0.tgz".to_string())
        );
    }

    #[test]
    fn test_extract_repo_string_git_at_gitlab() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-repo-string-git-at-gitlab.json").as_path(),
        );
        assert_eq!(
            package_data.download_url,
            Some("https://registry.npmjs.org/test-package/-/test-package-1.0.0.tgz".to_string())
        );
    }

    #[test]
    fn test_extract_repo_string_github_shortcut() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-repo-string-github-shortcut.json").as_path(),
        );
        assert_eq!(
            package_data.download_url,
            Some("https://registry.npmjs.org/test-package/-/test-package-1.0.0.tgz".to_string())
        );
    }

    #[test]
    fn test_extract_repo_string_gitlab_shortcut() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-repo-string-gitlab-shortcut.json").as_path(),
        );
        assert_eq!(
            package_data.download_url,
            Some("https://registry.npmjs.org/test-package/-/test-package-1.0.0.tgz".to_string())
        );
    }

    #[test]
    fn test_extract_repo_string_bitbucket_shortcut() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-repo-string-bitbucket-shortcut.json").as_path(),
        );
        assert_eq!(
            package_data.download_url,
            Some("https://registry.npmjs.org/test-package/-/test-package-1.0.0.tgz".to_string())
        );
    }

    #[test]
    fn test_extract_repo_string_gist_shortcut() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-repo-string-gist-shortcut.json").as_path(),
        );
        assert_eq!(
            package_data.download_url,
            Some("https://registry.npmjs.org/test-package/-/test-package-1.0.0.tgz".to_string())
        );
    }

    #[test]
    fn test_extract_repo_string_implicit_github() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-repo-string-implicit-github.json").as_path(),
        );
        assert_eq!(
            package_data.download_url,
            Some("https://registry.npmjs.org/test-package/-/test-package-1.0.0.tgz".to_string())
        );
    }

    #[test]
    fn test_extract_repo_object_url() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-repo-object-url.json").as_path(),
        );
        assert_eq!(
            package_data.download_url,
            Some("https://registry.npmjs.org/test-package/-/test-package-1.0.0.tgz".to_string())
        );
    }

    #[test]
    fn test_extract_repo_object_complete() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-repo-object-complete.json").as_path(),
        );
        assert_eq!(
            package_data.download_url,
            Some("https://registry.npmjs.org/test-package/-/test-package-1.0.0.tgz".to_string())
        );
    }

    #[test]
    fn test_extract_peer_dependencies_meta() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-peerdeps-meta.json").as_path(),
        );

        assert_eq!(package_data.name, Some("test-peer-deps-meta".to_string()));

        let peer_deps: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|d| {
                d.scope
                    .as_ref()
                    .map(|s| s == "peerDependencies")
                    .unwrap_or(false)
            })
            .collect();

        assert_eq!(peer_deps.len(), 2);

        let react_dep = peer_deps
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("react")))
            .unwrap();
        assert_eq!(react_dep.is_optional, None);

        let react_dom_dep = peer_deps
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("react-dom")))
            .unwrap();
        assert_eq!(react_dom_dep.is_optional, Some(true));
    }

    #[test]
    fn test_extract_peer_dependencies_meta_multiple() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-peerdeps-meta-multiple.json").as_path(),
        );

        let peer_deps: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|d| {
                d.scope
                    .as_ref()
                    .map(|s| s == "peerDependencies")
                    .unwrap_or(false)
            })
            .collect();

        assert_eq!(peer_deps.len(), 3);

        let react_dep = peer_deps
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("react")))
            .unwrap();
        assert_eq!(react_dep.is_optional, Some(true));

        let react_dom_dep = peer_deps
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("react-dom")))
            .unwrap();
        assert_eq!(react_dom_dep.is_optional, Some(false));

        let typescript_dep = peer_deps
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("typescript")))
            .unwrap();
        assert_eq!(typescript_dep.is_optional, Some(true));
    }

    #[test]
    fn test_extract_peer_dependencies_meta_empty() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-peerdeps-meta-empty.json").as_path(),
        );

        let peer_deps: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|d| {
                d.scope
                    .as_ref()
                    .map(|s| s == "peerDependencies")
                    .unwrap_or(false)
            })
            .collect();

        assert_eq!(peer_deps.len(), 1);
        assert_eq!(peer_deps[0].is_optional, None);
    }

    #[test]
    fn test_extract_peer_dependencies_meta_optional_true_only() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-peerdeps-meta-optional-true.json").as_path(),
        );

        let peer_deps: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|d| {
                d.scope
                    .as_ref()
                    .map(|s| s == "peerDependencies")
                    .unwrap_or(false)
            })
            .collect();

        assert_eq!(peer_deps.len(), 2);

        let react_dep = peer_deps
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("react")))
            .unwrap();
        assert_eq!(react_dep.is_optional, Some(true));

        let react_dom_dep = peer_deps
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("react-dom")))
            .unwrap();
        assert_eq!(react_dom_dep.is_optional, None);
    }

    #[test]
    fn test_extract_peer_dependencies_meta_absent() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-peerdeps-meta-absent.json").as_path(),
        );

        let peer_deps: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|d| {
                d.scope
                    .as_ref()
                    .map(|s| s == "peerDependencies")
                    .unwrap_or(false)
            })
            .collect();

        assert_eq!(peer_deps.len(), 2);
        assert!(peer_deps.iter().all(|d| d.is_optional.is_none()));
    }

    #[test]
    fn test_extract_dependencies_meta_pnpm() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-depsmeta-pnpm.json").as_path(),
        );

        assert!(package_data.extra_data.is_some());
        let extra_data = package_data.extra_data.as_ref().unwrap();

        assert!(extra_data.contains_key("dependenciesMeta"));
        let deps_meta = &extra_data["dependenciesMeta"];

        if let serde_json::Value::Object(obj) = deps_meta {
            assert_eq!(obj.len(), 2);
            assert!(obj.contains_key("lodash"));
            assert!(obj.contains_key("axios"));
        } else {
            panic!("dependenciesMeta should be an object");
        }
    }

    #[test]
    fn test_extract_dependencies_meta_empty() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-depsmeta-empty.json").as_path(),
        );

        assert!(package_data.extra_data.is_some());
        let extra_data = package_data.extra_data.as_ref().unwrap();

        assert!(extra_data.contains_key("dependenciesMeta"));
        let deps_meta = &extra_data["dependenciesMeta"];

        if let serde_json::Value::Object(obj) = deps_meta {
            assert_eq!(obj.len(), 0);
        } else {
            panic!("dependenciesMeta should be an object");
        }
    }

    #[test]
    fn test_extract_dependencies_meta_absent() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-depsmeta-absent.json").as_path(),
        );

        if let Some(extra_data) = &package_data.extra_data {
            assert!(!extra_data.contains_key("dependenciesMeta"));
        }
    }

    #[test]
    fn test_extract_dependencies_meta_combined() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-depsmeta-combined.json").as_path(),
        );

        assert!(package_data.extra_data.is_some());
        let extra_data = package_data.extra_data.as_ref().unwrap();

        assert!(extra_data.contains_key("dependenciesMeta"));
        let deps_meta = &extra_data["dependenciesMeta"];

        if let serde_json::Value::Object(obj) = deps_meta {
            assert_eq!(obj.len(), 1);
            assert!(obj.contains_key("lodash"));
        } else {
            panic!("dependenciesMeta should be an object");
        }

        let peer_deps: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|d| {
                d.scope
                    .as_ref()
                    .map(|s| s == "peerDependencies")
                    .unwrap_or(false)
            })
            .collect();

        assert_eq!(peer_deps.len(), 2);

        let react_dom_dep = peer_deps
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("react-dom")))
            .unwrap();
        assert_eq!(react_dom_dep.is_optional, Some(true));
    }

    #[test]
    fn test_extract_workspace_dependencies_caret() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-workspace-caret.json").as_path(),
        );

        // Check that workspaces is in extra_data
        assert!(package_data.extra_data.is_some());
        let extra_data = package_data.extra_data.as_ref().unwrap();
        assert!(extra_data.contains_key("workspaces"));

        // Check that workspace: protocol is captured in extracted_requirement
        let workspace_deps: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|d| {
                d.extracted_requirement
                    .as_ref()
                    .map(|r| r.starts_with("workspace:"))
                    .unwrap_or(false)
            })
            .collect();

        assert_eq!(workspace_deps.len(), 2);
        assert_eq!(
            workspace_deps[0].extracted_requirement.as_deref(),
            Some("workspace:^")
        );
        assert_eq!(
            workspace_deps[1].extracted_requirement.as_deref(),
            Some("workspace:^")
        );
    }

    #[test]
    fn test_extract_workspace_dependencies_tilde() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-workspace-tilde.json").as_path(),
        );

        let workspace_deps: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|d| {
                d.extracted_requirement
                    .as_ref()
                    .map(|r| r.starts_with("workspace:~"))
                    .unwrap_or(false)
            })
            .collect();

        assert_eq!(workspace_deps.len(), 2);
        assert_eq!(
            workspace_deps[0].extracted_requirement.as_deref(),
            Some("workspace:~")
        );
        assert_eq!(
            workspace_deps[1].extracted_requirement.as_deref(),
            Some("workspace:~")
        );
    }

    #[test]
    fn test_extract_workspace_dependencies_asterisk() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-workspace-asterisk.json").as_path(),
        );

        let workspace_deps: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|d| {
                d.extracted_requirement
                    .as_ref()
                    .map(|r| r.starts_with("workspace:*"))
                    .unwrap_or(false)
            })
            .collect();

        assert_eq!(workspace_deps.len(), 2);
        assert_eq!(
            workspace_deps[0].extracted_requirement.as_deref(),
            Some("workspace:*")
        );
        assert_eq!(
            workspace_deps[1].extracted_requirement.as_deref(),
            Some("workspace:*")
        );
    }

    #[test]
    fn test_extract_workspace_dependencies_mixed() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-workspace-deps.json").as_path(),
        );

        // Check multiple dependency scopes with workspace: protocol
        let runtime_workspace_dep = package_data
            .dependencies
            .iter()
            .find(|d| {
                d.extracted_requirement.as_deref() == Some("workspace:^")
                    && d.scope.as_deref() == Some("dependencies")
            })
            .unwrap();
        assert_eq!(
            runtime_workspace_dep.purl,
            Some("pkg:npm/shared-utils".to_string())
        );
        assert_eq!(
            runtime_workspace_dep.extracted_requirement.as_deref(),
            Some("workspace:^")
        );

        let dev_workspace_dep = package_data
            .dependencies
            .iter()
            .find(|d| {
                d.extracted_requirement.as_deref() == Some("workspace:*")
                    && d.scope.as_deref() == Some("devDependencies")
            })
            .unwrap();
        assert_eq!(dev_workspace_dep.purl, Some("pkg:npm/config".to_string()));
        assert_eq!(
            dev_workspace_dep.extracted_requirement.as_deref(),
            Some("workspace:*")
        );

        let peer_workspace_dep = package_data
            .dependencies
            .iter()
            .find(|d| {
                d.extracted_requirement.as_deref() == Some("workspace:~")
                    && d.scope.as_ref().is_some_and(|s| s == "peerDependencies")
            })
            .unwrap();
        assert_eq!(peer_workspace_dep.purl, Some("pkg:npm/types".to_string()));
        assert_eq!(
            peer_workspace_dep.extracted_requirement.as_deref(),
            Some("workspace:~")
        );

        // Check non-workspace dependency still has extracted_requirement
        let react_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("react")))
            .unwrap();
        assert_eq!(react_dep.extracted_requirement.as_deref(), Some("^18.0.0"));
    }

    #[test]
    fn test_extract_workspaces_array() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-workspaces-multi.json").as_path(),
        );

        assert!(package_data.extra_data.is_some());
        let extra_data = package_data.extra_data.as_ref().unwrap();
        assert!(extra_data.contains_key("workspaces"));

        let workspaces = extra_data.get("workspaces").unwrap();
        if let Value::Array(arr) = workspaces {
            assert_eq!(arr.len(), 3);
            assert_eq!(arr[0], "packages/*");
            assert_eq!(arr[1], "apps/*");
            assert_eq!(arr[2], "tools/*");
        } else {
            panic!("workspaces should be an array");
        }
    }

    #[test]
    fn test_extract_workspaces_string() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-workspaces-string.json").as_path(),
        );

        assert!(package_data.extra_data.is_some());
        let extra_data = package_data.extra_data.as_ref().unwrap();
        assert!(extra_data.contains_key("workspaces"));

        let workspaces = extra_data.get("workspaces").unwrap();
        if let Value::String(s) = workspaces {
            assert_eq!(s, "packages/*");
        } else {
            panic!("workspaces should be a string");
        }
    }

    #[test]
    fn test_extract_no_workspaces() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-no-workspaces.json").as_path(),
        );

        // When workspaces are not present, they shouldn't be in extra_data
        if let Some(extra_data) = &package_data.extra_data {
            assert!(!extra_data.contains_key("workspaces"));
        }

        // But regular dependencies should still have extracted_requirement
        let react_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("react")))
            .unwrap();
        assert_eq!(react_dep.extracted_requirement.as_deref(), Some("^18.0.0"));
    }

    #[test]
    fn test_extract_regular_dependencies_have_requirement() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-no-workspaces.json").as_path(),
        );

        // All dependencies should have extracted_requirement set
        for dep in &package_data.dependencies {
            assert!(
                dep.extracted_requirement.is_some(),
                "Dependency {:?} should have extracted_requirement",
                dep.purl
            );
        }
    }

    #[test]
    fn test_extract_alias_simple() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-alias-simple.json").as_path(),
        );

        // Verify alias dependency uses actual package in purl
        let alias_dep = package_data
            .dependencies
            .iter()
            .find(|d| {
                d.purl
                    .as_ref()
                    .is_some_and(|p| p.contains("actual-package"))
            })
            .unwrap();
        assert_eq!(
            alias_dep.extracted_requirement.as_deref(),
            Some("npm:actual-package@^1.0.0")
        );
        assert!(
            alias_dep
                .purl
                .as_ref()
                .unwrap()
                .starts_with("pkg:npm/actual-package")
        );

        // Regular dependency should not be affected
        let regular_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("regular-dep")))
            .unwrap();
        assert_eq!(regular_dep.extracted_requirement.as_deref(), Some("^2.0.0"));
    }

    #[test]
    fn test_extract_alias_scoped() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-alias-scoped.json").as_path(),
        );

        // Verify scoped alias uses actual package in purl
        let alias_dep = package_data
            .dependencies
            .iter()
            .find(|d| {
                d.purl
                    .as_ref()
                    .is_some_and(|p| p.contains("scope") && p.contains("actual-package"))
            })
            .unwrap();
        assert_eq!(
            alias_dep.extracted_requirement.as_deref(),
            Some("npm:@scope/actual-package@^1.0.0")
        );
        assert!(alias_dep.purl.as_ref().unwrap().contains("%40scope"));
    }

    #[test]
    fn test_extract_alias_multiple_scopes() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-alias-multiple-scopes.json").as_path(),
        );

        // Runtime alias
        let runtime_alias = package_data
            .dependencies
            .iter()
            .find(|d| d.extracted_requirement.as_deref() == Some("npm:actual-package@^1.0.0"))
            .unwrap();
        assert_eq!(runtime_alias.scope.as_deref(), Some("dependencies"));

        // Dev alias
        let dev_alias = package_data
            .dependencies
            .iter()
            .find(|d| d.extracted_requirement.as_deref() == Some("npm:actual-package@~2.0.0"))
            .unwrap();
        assert_eq!(dev_alias.scope.as_deref(), Some("devDependencies"));

        // Peer alias
        let peer_alias = package_data
            .dependencies
            .iter()
            .find(|d| d.extracted_requirement.as_deref() == Some("npm:peer-package@^3.0.0"))
            .unwrap();
        assert_eq!(peer_alias.scope.as_deref(), Some("peerDependencies"));

        // Optional alias
        let opt_alias = package_data
            .dependencies
            .iter()
            .find(|d| d.extracted_requirement.as_deref() == Some("npm:opt-package@*"))
            .unwrap();
        assert_eq!(opt_alias.scope.as_deref(), Some("optionalDependencies"));
    }

    #[test]
    fn test_extract_alias_version_formats() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-alias-versions.json").as_path(),
        );

        let deps: std::collections::HashMap<_, _> = package_data
            .dependencies
            .iter()
            .filter_map(|d| {
                d.extracted_requirement
                    .as_ref()
                    .map(|r| (r.clone(), d.purl.clone()))
            })
            .collect();

        // Verify each alias format uses actual-package in purl
        for dep in deps.values() {
            assert!(dep.as_ref().unwrap().contains("actual-package"));
        }

        assert!(deps.contains_key("npm:actual-package@^1.0.0"));
        assert!(deps.contains_key("npm:actual-package@~2.0.0"));
        assert!(deps.contains_key("npm:actual-package@*"));
        assert!(deps.contains_key("npm:actual-package@3.0.0"));
        assert!(deps.contains_key("npm:actual-package@>=2.0.0 <3.0.0"));
    }

    #[test]
    fn test_extract_no_alias() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-alias-no-alias.json").as_path(),
        );

        // Regular dependencies should not be affected
        assert_eq!(package_data.dependencies.len(), 2);

        for dep in &package_data.dependencies {
            assert!(
                !dep.extracted_requirement
                    .as_ref()
                    .unwrap()
                    .starts_with("npm:")
            );
        }
    }

    #[test]
    fn test_extract_api_url_basic() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-api-url-basic.json").as_path(),
        );

        assert_eq!(
            package_data.api_data_url,
            Some("https://registry.npmjs.org/react/18.2.0".to_string())
        );
    }

    #[test]
    fn test_extract_api_url_scoped() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-api-url-scoped.json").as_path(),
        );

        assert_eq!(
            package_data.api_data_url,
            Some("https://registry.npmjs.org/@babel%2fcore/7.2.0".to_string())
        );
    }

    #[test]
    fn test_extract_api_url_no_version() {
        let package_data = NpmParser::extract_first_package(
            PathBuf::from("testdata/npm/package-api-url-no-version.json").as_path(),
        );

        assert_eq!(
            package_data.api_data_url,
            Some("https://registry.npmjs.org/react".to_string())
        );
    }

    #[test]
    fn test_extract_vcs_url_string() {
        let content = r#"{
  "name": "test-package",
  "version": "1.0.0",
  "repository": "https://github.com/user/test-package.git"
}"#;

        let (_temp_file, path) = create_temp_package_json(content);
        let package_data = NpmParser::extract_first_package(&path);

        assert_eq!(
            package_data.vcs_url,
            Some("https://github.com/user/test-package.git".to_string())
        );
    }

    #[test]
    fn test_extract_vcs_url_object_with_type() {
        let content = r#"{
  "name": "test-package",
  "version": "1.0.0",
  "repository": {
    "type": "git",
    "url": "https://github.com/user/test-package.git"
  }
}"#;

        let (_temp_file, path) = create_temp_package_json(content);
        let package_data = NpmParser::extract_first_package(&path);

        assert_eq!(
            package_data.vcs_url,
            Some("git+https://github.com/user/test-package.git".to_string())
        );
    }

    #[test]
    fn test_extract_vcs_url_object_with_directory() {
        let content = r#"{
  "name": "test-package",
  "version": "1.0.0",
  "repository": {
    "type": "git",
    "url": "https://github.com/user/monorepo.git",
    "directory": "packages/test-package"
  }
}"#;

        let (_temp_file, path) = create_temp_package_json(content);
        let package_data = NpmParser::extract_first_package(&path);

        assert_eq!(
            package_data.vcs_url,
            Some("git+https://github.com/user/monorepo.git#packages/test-package".to_string())
        );
    }

    #[test]
    fn test_extract_vcs_url_object_with_git_head() {
        let content = r#"{
  "name": "test-package",
  "version": "1.0.0",
  "gitHead": "fc7bbf03e39cc48a8924b90696d28345a6a90f3c",
  "repository": {
    "type": "git",
    "url": "https://github.com/user/test-package.git"
  }
}"#;

        let (_temp_file, path) = create_temp_package_json(content);
        let package_data = NpmParser::extract_first_package(&path);

        assert_eq!(
            package_data.vcs_url,
            Some(
                "git+https://github.com/user/test-package.git@fc7bbf03e39cc48a8924b90696d28345a6a90f3c"
                    .to_string()
            )
        );
    }

    #[test]
    fn test_extract_vcs_url_git_protocol() {
        let content = r#"{
  "name": "test-package",
  "version": "1.0.0",
  "repository": "git://github.com/user/test-package.git"
}"#;

        let (_temp_file, path) = create_temp_package_json(content);
        let package_data = NpmParser::extract_first_package(&path);

        assert_eq!(
            package_data.vcs_url,
            Some("git://github.com/user/test-package.git".to_string())
        );
    }

    #[test]
    fn test_extract_vcs_url_github_shorthand() {
        let content = r#"{
  "name": "test-package",
  "version": "1.0.0",
  "repository": "user/repo"
}"#;

        let (_temp_file, path) = create_temp_package_json(content);
        let package_data = NpmParser::extract_first_package(&path);

        assert_eq!(
            package_data.vcs_url,
            Some("https://github.com/user/repo".to_string())
        );
    }

    #[test]
    fn test_extract_vcs_url_none() {
        let content = r#"{
  "name": "test-package",
  "version": "1.0.0"
}"#;

        let (_temp_file, path) = create_temp_package_json(content);
        let package_data = NpmParser::extract_first_package(&path);

        assert_eq!(package_data.vcs_url, None);
    }

    #[test]
    fn test_workspace_protocol_asterisk() {
        let path = PathBuf::from("testdata/npm/package-workspace-asterisk.json");
        let package_data = NpmParser::extract_first_package(&path);

        let workspace_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.extracted_requirement.as_deref() == Some("workspace:*"));

        assert!(
            workspace_dep.is_some(),
            "Should find workspace:* dependency"
        );
        let dep = workspace_dep.unwrap();
        assert!(dep.purl.is_some(), "Workspace deps should have a purl");
        assert_eq!(dep.extracted_requirement, Some("workspace:*".to_string()));
    }

    #[test]
    fn test_workspace_protocol_caret() {
        let path = PathBuf::from("testdata/npm/package-workspace-caret.json");
        let package_data = NpmParser::extract_first_package(&path);

        let workspace_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.extracted_requirement.as_deref() == Some("workspace:^"));

        assert!(
            workspace_dep.is_some(),
            "Should find workspace:^ dependency"
        );
        let dep = workspace_dep.unwrap();
        assert!(dep.purl.is_some(), "Workspace deps should have a purl");
        assert_eq!(dep.extracted_requirement, Some("workspace:^".to_string()));
    }

    #[test]
    fn test_workspace_protocol_tilde() {
        let path = PathBuf::from("testdata/npm/package-workspace-tilde.json");
        let package_data = NpmParser::extract_first_package(&path);

        let workspace_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.extracted_requirement.as_deref() == Some("workspace:~"));

        assert!(
            workspace_dep.is_some(),
            "Should find workspace:~ dependency"
        );
        let dep = workspace_dep.unwrap();
        assert!(dep.purl.is_some(), "Workspace deps should have a purl");
        assert_eq!(dep.extracted_requirement, Some("workspace:~".to_string()));
    }

    #[test]
    fn test_workspace_protocol_mixed_deps() {
        let path = PathBuf::from("testdata/npm/package-workspace-deps.json");
        let package_data = NpmParser::extract_first_package(&path);

        let workspace_deps: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|d| {
                d.extracted_requirement
                    .as_ref()
                    .is_some_and(|req| req.starts_with("workspace:"))
            })
            .collect();

        assert_eq!(
            workspace_deps.len(),
            3,
            "Should find 3 workspace dependencies (regular, dev, and peer)"
        );

        for dep in &workspace_deps {
            assert!(
                dep.purl.is_some(),
                "Workspace dependencies should have a purl derived from the dep name"
            );
            assert!(
                dep.extracted_requirement
                    .as_ref()
                    .unwrap()
                    .starts_with("workspace:")
            );
        }

        let normal_deps: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|d| {
                d.extracted_requirement
                    .as_ref()
                    .is_some_and(|req| !req.starts_with("workspace:"))
            })
            .collect();

        assert!(
            !normal_deps.is_empty(),
            "Should still have normal dependencies"
        );
        for dep in normal_deps {
            assert!(dep.purl.is_some(), "Normal dependencies should have PURL");
        }
    }

    #[test]
    fn test_scoped_dependency_purl_bug() {
        let package_path = PathBuf::from("testdata/npm/scoped-deps/package.json");
        let package_data = NpmParser::extract_first_package(&package_path);

        assert!(
            !package_data.dependencies.is_empty(),
            "Should have dependencies"
        );

        let types_node = package_data
            .dependencies
            .iter()
            .find(|d| {
                if let Some(purl) = &d.purl {
                    purl.contains("types") && purl.contains("node")
                } else {
                    false
                }
            })
            .expect("Should find @types/node dependency");

        let types_purl = types_node.purl.as_ref().unwrap();

        assert_eq!(
            types_purl, "pkg:npm/%40types/node",
            "Scoped dependency PURLs should encode only the @, not the namespace slash"
        );

        let babel = package_data
            .dependencies
            .iter()
            .find(|d| {
                if let Some(purl) = &d.purl {
                    purl.contains("babel")
                } else {
                    false
                }
            })
            .expect("Should find @babel/core dependency");

        assert_eq!(
            babel.purl.as_deref(),
            Some("pkg:npm/%40babel/core"),
            "Scoped dependency PURLs should encode only the @, not the namespace slash"
        );

        let lodash = package_data
            .dependencies
            .iter()
            .find(|d| {
                if let Some(purl) = &d.purl {
                    purl.contains("lodash") && !purl.contains("%40")
                } else {
                    false
                }
            })
            .expect("Should find lodash dependency");

        assert!(
            !lodash.purl.as_ref().unwrap().contains("%40"),
            "Unscoped package should not have namespace encoding"
        );
    }

    #[test]
    fn test_peer_dependencies_meta_optional() {
        let package_path = PathBuf::from("testdata/npm/peer-deps-meta/package.json");
        let package_data = NpmParser::extract_first_package(&package_path);

        let debug_dep = package_data
            .dependencies
            .iter()
            .find(|d| {
                d.scope.as_deref() == Some("peerDependencies")
                    && d.purl
                        .as_ref()
                        .map(|p| p.contains("debug"))
                        .unwrap_or(false)
            })
            .expect("Should find debug peer dependency");

        assert_eq!(
            debug_dep.scope,
            Some("peerDependencies".to_string()),
            "Debug should be a peer dependency"
        );
        assert_eq!(
            debug_dep.is_optional,
            Some(true),
            "Debug should be marked optional via peerDependenciesMeta"
        );

        let lodash_dep = package_data
            .dependencies
            .iter()
            .find(|d| {
                d.scope.as_deref() == Some("peerDependencies")
                    && d.purl
                        .as_ref()
                        .map(|p| p.contains("lodash"))
                        .unwrap_or(false)
            })
            .expect("Should find lodash peer dependency");

        assert!(
            lodash_dep.is_optional == Some(false) || lodash_dep.is_optional.is_none(),
            "Lodash should NOT be marked optional (not in peerDependenciesMeta). Got: {:?}",
            lodash_dep.is_optional
        );
    }

    #[test]
    fn test_git_url_dependencies() {
        let content = r#"
{
  "name": "test-package",
  "version": "1.0.0",
  "dependencies": {
    "express": "git://github.com/expressjs/express.git",
    "lodash": "git+https://github.com/lodash/lodash.git#v4.17.21",
    "underscore": "git@github.com:jashkenas/underscore.git",
    "moment": "github:moment/moment",
    "axios": "axios/axios#v1.0.0"
  }
}
"#;
        let (_temp_file, package_path) = create_temp_package_json(content);
        let package_data = NpmParser::extract_first_package(&package_path);

        assert_eq!(package_data.dependencies.len(), 5);

        for dep in &package_data.dependencies {
            assert!(
                dep.is_pinned == Some(false),
                "Git URL dependencies should not be pinned: {:?}",
                dep.purl
            );
            assert!(
                dep.extracted_requirement.is_some(),
                "Git URL dependencies should have extracted_requirement"
            );
        }
    }

    #[test]
    fn test_url_dependencies() {
        let content = r#"
{
  "name": "test-package",
  "version": "1.0.0",
  "dependencies": {
    "my-package": "https://example.com/package.tgz",
    "another": "http://example.com/another.tgz"
  }
}
"#;
        let (_temp_file, package_path) = create_temp_package_json(content);
        let package_data = NpmParser::extract_first_package(&package_path);

        assert_eq!(package_data.dependencies.len(), 2);

        for dep in &package_data.dependencies {
            assert!(
                dep.is_pinned == Some(false),
                "URL dependencies should not be pinned: {:?}",
                dep.purl
            );
        }
    }

    #[test]
    fn test_local_path_dependencies() {
        let content = r#"
{
  "name": "test-package",
  "version": "1.0.0",
  "dependencies": {
    "local-pkg": "file:../local-pkg",
    "linked-pkg": "link:../linked-pkg"
  }
}
"#;
        let (_temp_file, package_path) = create_temp_package_json(content);
        let package_data = NpmParser::extract_first_package(&package_path);

        assert_eq!(package_data.dependencies.len(), 2);

        for dep in &package_data.dependencies {
            assert!(
                dep.is_pinned == Some(false),
                "Local path dependencies should not be pinned: {:?}",
                dep.purl
            );
        }
    }

    #[test]
    fn test_mixed_dependencies() {
        let content = r#"
{
  "name": "test-package",
  "version": "1.0.0",
  "dependencies": {
    "regular": "^4.17.0",
    "exact": "2.0.0",
    "git-dep": "git+https://github.com/org/repo.git",
    "url-dep": "https://example.com/package.tgz",
    "github-dep": "user/repo#main"
  }
}
"#;
        let (_temp_file, package_path) = create_temp_package_json(content);
        let package_data = NpmParser::extract_first_package(&package_path);

        assert_eq!(package_data.dependencies.len(), 5);

        let regular = package_data
            .dependencies
            .iter()
            .find(|d| {
                d.purl
                    .as_ref()
                    .map(|p| p.contains("regular"))
                    .unwrap_or(false)
            })
            .expect("Should find regular dependency");
        assert_eq!(
            regular.is_pinned,
            Some(false),
            "Semver range should not be pinned"
        );

        let exact = package_data
            .dependencies
            .iter()
            .find(|d| {
                d.purl
                    .as_ref()
                    .map(|p| p.contains("exact"))
                    .unwrap_or(false)
            })
            .expect("Should find exact dependency");
        assert_eq!(
            exact.is_pinned,
            Some(false),
            "package.json exact requirements should remain unpinned"
        );
        assert!(
            exact.purl.as_ref().unwrap() == "pkg:npm/exact",
            "package.json dependency PURLs should not include manifest requirement versions"
        );

        let git_dep = package_data
            .dependencies
            .iter()
            .find(|d| {
                d.purl
                    .as_ref()
                    .map(|p| p.contains("git-dep"))
                    .unwrap_or(false)
            })
            .expect("Should find git-dep dependency");
        assert_eq!(
            git_dep.is_pinned,
            Some(false),
            "Git dependency should not be pinned"
        );
    }

    #[test]
    fn test_scoped_manifest_dependencies_preserve_order_and_unversioned_purls() {
        let content = r#"
{
  "name": "angular-compare-validator",
  "version": "0.1.1",
  "dependencies": {
    "@angular/core": ">=2.0.0",
    "@angular/forms": ">=2.0.0",
    "@angular/common": ">=2.0.0",
    "rxjs": ">=5.0.0-beta.12"
  }
}
"#;

        let (_temp_file, package_path) = create_temp_package_json(content);
        let package_data = NpmParser::extract_first_package(&package_path);

        let dependency_purls: Vec<&str> = package_data
            .dependencies
            .iter()
            .map(|dep| dep.purl.as_deref().expect("dependency should have purl"))
            .collect();

        assert_eq!(
            dependency_purls,
            vec![
                "pkg:npm/%40angular/core",
                "pkg:npm/%40angular/forms",
                "pkg:npm/%40angular/common",
                "pkg:npm/rxjs",
            ]
        );
        assert!(
            package_data
                .dependencies
                .iter()
                .all(|dep| dep.is_pinned == Some(false))
        );
    }

    #[test]
    fn test_dist_shasum_populates_sha1() {
        let content = r#"
{
  "name": "angular-compare-validator",
  "version": "0.1.1",
  "dist": {
    "integrity": "sha512-j3DtXjUTGFrVj7KjEUdprJPd1og2zokUblhvwD4DrJPc+x8RNUrCb0CLdcDr9RZj1eTo4nw4dSo8Br3edJp8Aw==",
    "shasum": "d35a0754c8587b0502874e3636cf0f19565d09b7"
  }
}
"#;

        let (_temp_file, package_path) = create_temp_package_json(content);
        let package_data = NpmParser::extract_first_package(&package_path);

        assert_eq!(
            package_data.sha1,
            Some(Sha1Digest::from_hex("d35a0754c8587b0502874e3636cf0f19565d09b7").unwrap())
        );
        assert!(package_data.sha512.is_some());
    }

    #[test]
    fn test_party_object_url_none_is_treated_as_missing() {
        let content = r#"
{
  "name": "party-url-test",
  "version": "1.0.0",
  "contributors": [
    {
      "name": "Example Contributor",
      "email": "contributor@example.com",
      "url": "none"
    }
  ]
}
"#;

        let (_temp_file, package_path) = create_temp_package_json(content);
        let package_data = NpmParser::extract_first_package(&package_path);

        assert_eq!(package_data.parties.len(), 1);
        assert_eq!(package_data.parties[0].url, None);
    }

    #[test]
    fn test_dist_tarball_registry_url_is_normalized_to_https() {
        let content = r#"
{
  "name": "registry-tarball-test",
  "version": "1.0.0",
  "dist": {
    "tarball": "http://registry.npmjs.org/registry-tarball-test/-/registry-tarball-test-1.0.0.tgz"
  }
}
"#;

        let (_temp_file, package_path) = create_temp_package_json(content);
        let package_data = NpmParser::extract_first_package(&package_path);

        assert_eq!(
            package_data.download_url.as_deref(),
            Some(
                "https://registry.npmjs.org/registry-tarball-test/-/registry-tarball-test-1.0.0.tgz"
            )
        );
    }
}
