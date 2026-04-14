#[cfg(test)]
mod tests {
    use crate::models::PackageType;
    use crate::parsers::{CargoParser, PackageParser};
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // Helper function to create a temporary Cargo.toml file with the given content
    fn create_temp_cargo_toml(content: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cargo_path = temp_dir.path().join("Cargo.toml");
        fs::write(&cargo_path, content).expect("Failed to write Cargo.toml");

        (temp_dir, cargo_path)
    }

    #[test]
    fn test_is_match() {
        let valid_path = PathBuf::from("/some/path/Cargo.toml");
        let lowercase_path = PathBuf::from("/some/path/cargo.toml");
        let invalid_path = PathBuf::from("/some/path/not_cargo.toml");

        assert!(CargoParser::is_match(&valid_path));
        assert!(CargoParser::is_match(&lowercase_path));
        assert!(!CargoParser::is_match(&invalid_path));
    }

    #[test]
    fn test_extract_from_testdata() {
        let cargo_path = PathBuf::from("testdata/cargo/Cargo.toml");
        let package_data = CargoParser::extract_first_package(&cargo_path);

        assert_eq!(package_data.package_type, Some(PackageType::Cargo));
        assert_eq!(package_data.name, Some("test-cargo".to_string()));
        assert_eq!(package_data.version, Some("1.2.3".to_string()));
        assert_eq!(
            package_data.homepage_url,
            Some("https://example.com".to_string())
        );
        assert_eq!(package_data.download_url, None);

        assert_eq!(
            package_data.declared_license_expression.as_deref(),
            Some("apache-2.0 OR mit")
        );
        assert_eq!(
            package_data.declared_license_expression_spdx.as_deref(),
            Some("Apache-2.0 OR MIT")
        );
        assert_eq!(package_data.license_detections.len(), 1);
        assert_eq!(
            package_data.extracted_license_statement,
            Some("MIT OR Apache-2.0".to_string())
        );

        // Check purl
        assert_eq!(
            package_data.purl,
            Some("pkg:cargo/test-cargo@1.2.3".to_string())
        );

        // Check authors extraction
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
        assert!(purls.iter().any(|p| p.contains("serde")));
        assert!(purls.iter().any(|p| p.contains("tokio")));
        assert!(purls.iter().any(|p| p.contains("mockito")));
    }

    #[test]
    fn test_extract_basic_package_info() {
        let content = r#"
[package]
name = "test-package"
version = "0.1.0"
license = "MIT"
repository = "https://github.com/user/test-package"
homepage = "https://example.com"
authors = ["Test User <test@example.com>"]
        "#;

        let (_temp_file, cargo_path) = create_temp_cargo_toml(content);
        let package_data = CargoParser::extract_first_package(&cargo_path);

        assert_eq!(package_data.package_type, Some(PackageType::Cargo));
        assert_eq!(package_data.name, Some("test-package".to_string()));
        assert_eq!(package_data.version, Some("0.1.0".to_string()));
        assert_eq!(
            package_data.homepage_url,
            Some("https://example.com".to_string())
        );
        assert_eq!(package_data.download_url, None);

        assert_eq!(
            package_data.declared_license_expression.as_deref(),
            Some("mit")
        );
        assert_eq!(
            package_data.declared_license_expression_spdx.as_deref(),
            Some("MIT")
        );
        assert_eq!(package_data.license_detections.len(), 1);
        assert_eq!(
            package_data.extracted_license_statement,
            Some("MIT".to_string())
        );

        // Check purl
        assert_eq!(
            package_data.purl,
            Some("pkg:cargo/test-package@0.1.0".to_string())
        );

        // Check authors extraction
        assert_eq!(package_data.parties.len(), 1);
        assert_eq!(
            package_data.parties[0].email,
            Some("test@example.com".to_string())
        );
    }

    #[test]
    fn test_extract_dependencies() {
        let content = r#"
[package]
name = "test-package"
version = "0.1.0"
license = "MIT"

[dependencies]
serde = "1.0"
log = { version = "0.4", features = ["std"] }

[dev-dependencies]
tokio = { version = "1.0", features = ["full"] }
"#;

        let (_temp_file, cargo_path) = create_temp_cargo_toml(content);
        let package_data = CargoParser::extract_first_package(&cargo_path);

        // We should have 3 dependencies in total (2 regular, 1 dev)
        assert_eq!(package_data.dependencies.len(), 3);

        let serde_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_ref().unwrap().contains("serde"))
            .expect("Should find serde dependency");

        assert_eq!(serde_dep.purl, Some("pkg:cargo/serde".to_string()));
        assert_eq!(serde_dep.extracted_requirement, Some("1.0".to_string()));
        assert_eq!(serde_dep.scope, Some("dependencies".to_string()));
        assert_eq!(serde_dep.is_runtime, Some(true));
        assert_eq!(serde_dep.is_optional, Some(false));

        let tokio_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_ref().unwrap().contains("tokio"))
            .expect("Should find tokio dependency");

        assert_eq!(tokio_dep.purl, Some("pkg:cargo/tokio".to_string()));
        assert_eq!(tokio_dep.extracted_requirement, Some("1.0".to_string()));
        assert_eq!(tokio_dep.scope, Some("dev-dependencies".to_string()));
        assert_eq!(tokio_dep.is_runtime, Some(false));
        assert_eq!(tokio_dep.is_optional, Some(false));
    }

    #[test]
    fn test_extract_legacy_underscore_dependency_sections() {
        let content = r#"
[package]
name = "test-package"
version = "0.1.0"
license = "MIT"

[dev_dependencies]
my-package = "99999.0.0"

[build_dependencies]
my-package = "99999.0.0"
"#;

        let (_temp_file, cargo_path) = create_temp_cargo_toml(content);
        let package_data = CargoParser::extract_first_package(&cargo_path);

        let dependency_purls: Vec<_> = package_data
            .dependencies
            .iter()
            .filter_map(|dep| dep.purl.as_deref())
            .collect();

        assert_eq!(dependency_purls.len(), 2);
        assert!(
            dependency_purls
                .iter()
                .all(|purl| *purl == "pkg:cargo/my-package")
        );
    }

    #[test]
    fn test_extract_short_composite_declared_license_preserves_mit_zero_and_or() {
        let content = r#"
[package]
name = "constant_time_eq"
version = "0.3.1"
license = "CC0-1.0 OR MIT-0 OR Apache-2.0"
        "#;

        let (_temp_file, cargo_path) = create_temp_cargo_toml(content);
        let package_data = CargoParser::extract_first_package(&cargo_path);

        assert_eq!(
            package_data.declared_license_expression.as_deref(),
            Some("cc0-1.0 OR mit-0 OR apache-2.0")
        );
        assert_eq!(
            package_data.declared_license_expression_spdx.as_deref(),
            Some("CC0-1.0 OR MIT-0 OR Apache-2.0")
        );
        assert_eq!(package_data.license_detections.len(), 1);
        assert_eq!(
            package_data.license_detections[0].license_expression,
            "cc0-1.0 OR mit-0 OR apache-2.0"
        );
        assert_eq!(
            package_data.license_detections[0].license_expression_spdx,
            "CC0-1.0 OR MIT-0 OR Apache-2.0"
        );
        assert_eq!(
            package_data.extracted_license_statement.as_deref(),
            Some("CC0-1.0 OR MIT-0 OR Apache-2.0")
        );
    }

    #[test]
    fn test_extract_with_complex_dependencies() {
        let content = r#"
[package]
name = "complex-package"
version = "0.2.0"
license = "Apache-2.0"

[dependencies]
regex = "1.5.4"
serde = { version = "1.0.136", features = ["derive"] }
reqwest = { version = "0.11", optional = true }

[dev-dependencies]
mockito = "0.31.0"
"#;

        let (_temp_file, cargo_path) = create_temp_cargo_toml(content);
        let package_data = CargoParser::extract_first_package(&cargo_path);

        // Check we have all dependencies extracted (3 regular + 1 dev)
        assert_eq!(package_data.dependencies.len(), 4);

        let regex_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_ref().unwrap().contains("regex"))
            .expect("Should find regex dependency");

        assert_eq!(regex_dep.purl, Some("pkg:cargo/regex".to_string()));
        assert_eq!(regex_dep.extracted_requirement, Some("1.5.4".to_string()));

        let serde_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_ref().unwrap().contains("serde"))
            .expect("Should find serde dependency");

        assert_eq!(serde_dep.purl, Some("pkg:cargo/serde".to_string()));
        assert_eq!(serde_dep.extracted_requirement, Some("1.0.136".to_string()));
    }

    #[test]
    fn test_empty_or_invalid_cargo_toml() {
        // Test with empty content
        let content = "";
        let (_temp_file, cargo_path) = create_temp_cargo_toml(content);
        let package_data = CargoParser::extract_first_package(&cargo_path);

        // Should return default/empty package data
        assert_eq!(package_data.name, None);
        assert_eq!(package_data.version, None);
        assert!(package_data.dependencies.is_empty());

        // Test with invalid TOML
        let content = "this is not valid TOML";
        let (_temp_file, cargo_path) = create_temp_cargo_toml(content);
        let package_data = CargoParser::extract_first_package(&cargo_path);

        // Should return default/empty package data
        assert_eq!(package_data.name, None);
        assert_eq!(package_data.version, None);
        assert!(package_data.dependencies.is_empty());
    }

    #[test]
    fn test_invalid_cargo_toml_returns_no_packages() {
        let content = r#"
[package]
name = "manifest-invalid-test-fixture"
version = "0.1.0"

[invalid-section]
key = invalid-value
        "#;

        let (_temp_file, cargo_path) = create_temp_cargo_toml(content);
        let packages = CargoParser::extract_packages(&cargo_path);

        assert!(packages.is_empty());
    }

    #[test]
    fn test_extract_api_url_basic() {
        // Given: A package.toml with name and version
        let cargo_path = PathBuf::from("testdata/cargo/Cargo-api-url-basic.toml");
        let package_data = CargoParser::extract_first_package(&cargo_path);

        // Then: API data URL should be generated
        assert_eq!(
            package_data.api_data_url,
            Some("https://crates.io/api/v1/crates/serde".to_string())
        );

        // Then: Homepage URL should fall back to crates.io
        assert_eq!(
            package_data.homepage_url,
            Some("https://crates.io/crates/serde".to_string())
        );

        // Then: Download URL should be None (download URL goes in repository_download_url)
        assert_eq!(package_data.download_url, None);

        // Then: Repository download URL should point to crates.io download API
        assert_eq!(
            package_data.repository_download_url,
            Some("https://crates.io/api/v1/crates/serde/1.0.228/download".to_string())
        );
    }

    #[test]
    fn test_extract_api_url_no_version() {
        // Given: A package.toml with name but no version
        let cargo_path = PathBuf::from("testdata/cargo/Cargo-api-url-no-version.toml");
        let package_data = CargoParser::extract_first_package(&cargo_path);

        // Then: API data URL should still be generated (without version)
        assert_eq!(
            package_data.api_data_url,
            Some("https://crates.io/api/v1/crates/tokio".to_string())
        );

        // Then: Homepage URL should fall back to crates.io
        assert_eq!(
            package_data.homepage_url,
            Some("https://crates.io/crates/tokio".to_string())
        );

        // Then: Download URL should not be generated (no version)
        assert_eq!(package_data.download_url, None);
    }

    #[test]
    fn test_extract_vcs_url_no_repository() {
        // Given: A minimal Cargo.toml without a repository field
        let cargo_path = PathBuf::from("testdata/cargo/Cargo-minimal.toml");
        let package_data = CargoParser::extract_first_package(&cargo_path);

        // Then: vcs_url should be None
        assert_eq!(package_data.vcs_url, None);
    }

    #[test]
    fn test_extract_license_file() {
        use serde_json::json;

        let content = r#"
[package]
name = "test-package"
version = "0.1.0"
license = "MIT"
license-file = "LICENSE.txt"
"#;

        let (_temp_file, cargo_path) = create_temp_cargo_toml(content);
        let package_data = CargoParser::extract_first_package(&cargo_path);

        // Then: license-file should be extracted to extra_data
        assert!(package_data.extra_data.is_some());
        let extra_data = package_data.extra_data.unwrap();
        assert_eq!(extra_data.get("license_file"), Some(&json!("LICENSE.txt")));
    }

    #[test]
    fn test_extract_readme_and_publish() {
        use serde_json::json;

        let content = r#"
[package]
name = "test-package"
version = "0.1.0"
license = "MIT"
readme = "README.md"
publish = false
"#;

        let (_temp_file, cargo_path) = create_temp_cargo_toml(content);
        let package_data = CargoParser::extract_first_package(&cargo_path);

        let extra_data = package_data.extra_data.unwrap();
        assert_eq!(extra_data.get("readme_file"), Some(&json!("README.md")));
        assert_eq!(extra_data.get("publish"), Some(&json!(false)));
        assert!(package_data.is_private);
    }

    #[test]
    fn test_extract_manifest_file_references() {
        let content = r#"
[package]
name = "test-package"
version = "0.1.0"
license = "MIT"
license-file = "LICENSE.txt"
readme = "README.md"
"#;

        let (_temp_file, cargo_path) = create_temp_cargo_toml(content);
        let package_data = CargoParser::extract_first_package(&cargo_path);

        let file_reference_paths: Vec<_> = package_data
            .file_references
            .iter()
            .map(|reference| reference.path.as_str())
            .collect();

        assert_eq!(file_reference_paths, vec!["LICENSE.txt", "README.md"]);
    }

    #[test]
    fn test_extract_manifest_file_references_dedupes_same_path() {
        let content = r#"
[package]
name = "test-package"
version = "0.1.0"
license = "MIT"
license-file = "README.md"
readme = "README.md"
"#;

        let (_temp_file, cargo_path) = create_temp_cargo_toml(content);
        let package_data = CargoParser::extract_first_package(&cargo_path);

        let file_reference_paths: Vec<_> = package_data
            .file_references
            .iter()
            .map(|reference| reference.path.as_str())
            .collect();

        assert_eq!(file_reference_paths, vec!["README.md"]);
    }

    #[test]
    fn test_declared_license_detection_preserves_license_file_reference() {
        let content = r#"
[package]
name = "test-package"
version = "0.1.0"
license = "MIT"
license-file = "LICENSE.txt"
"#;

        let (_temp_file, cargo_path) = create_temp_cargo_toml(content);
        let package_data = CargoParser::extract_first_package(&cargo_path);

        assert_eq!(package_data.license_detections.len(), 1);
        let referenced_filenames = package_data.license_detections[0].matches[0]
            .referenced_filenames
            .as_ref()
            .expect("referenced filenames should be present");
        assert_eq!(referenced_filenames, &vec!["LICENSE.txt".to_string()]);
    }

    #[test]
    fn test_extract_build_dependencies() {
        let content = r#"
[package]
name = "test-package"
version = "0.1.0"
license = "MIT"

[dependencies]
serde = "1.0"

[dev-dependencies]
tokio = "1.0"

[build-dependencies]
cc = "1.0"
"#;

        let (_temp_file, cargo_path) = create_temp_cargo_toml(content);
        let package_data = CargoParser::extract_first_package(&cargo_path);

        // We should have 3 dependencies in total (1 regular, 1 dev, 1 build)
        assert_eq!(package_data.dependencies.len(), 3);

        let cc_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_ref().unwrap().contains("cc"))
            .expect("Should find cc build dependency");

        assert_eq!(cc_dep.purl, Some("pkg:cargo/cc".to_string()));
        assert_eq!(cc_dep.extracted_requirement, Some("1.0".to_string()));
        assert_eq!(cc_dep.scope, Some("build-dependencies".to_string()));
        assert_eq!(cc_dep.is_runtime, Some(false));
        assert_eq!(cc_dep.is_optional, Some(false));
    }

    #[test]
    fn test_cargo_git_path_dependencies() {
        let path = PathBuf::from("testdata/cargo/git-path-deps/Cargo.toml");
        let result = CargoParser::extract_first_package(&path);

        assert_eq!(result.dependencies.len(), 3);

        // Verify git dependency
        let git_dep = result
            .dependencies
            .iter()
            .find(|d| {
                d.purl
                    .as_ref()
                    .map(|p| p.contains("remote-crate"))
                    .unwrap_or(false)
            })
            .expect("Should find git dependency");
        assert_eq!(git_dep.extracted_requirement, None); // No version for git deps

        // Verify path dependency
        let path_dep = result
            .dependencies
            .iter()
            .find(|d| {
                d.purl
                    .as_ref()
                    .map(|p| p.contains("local-crate"))
                    .unwrap_or(false)
            })
            .expect("Should find path dependency");
        assert_eq!(path_dep.extracted_requirement, None); // No version for path deps

        // Verify registry dependency has version
        let registry_dep = result
            .dependencies
            .iter()
            .find(|d| {
                d.purl
                    .as_ref()
                    .map(|p| p.contains("registry-crate"))
                    .unwrap_or(false)
            })
            .expect("Should find registry dependency");
        assert_eq!(
            registry_dep.extracted_requirement,
            Some("1.0.0".to_string())
        );
    }
}
