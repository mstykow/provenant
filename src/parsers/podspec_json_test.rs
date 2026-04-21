// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use super::super::PackageParser;
    use super::super::podspec_json::PodspecJsonParser;
    use crate::models::PackageType;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Helper to create a temporary .podspec.json file with the given content.
    fn create_temp_podspec_json(content: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("test.podspec.json");
        fs::write(&file_path, content).expect("Failed to write temp file");
        (temp_dir, file_path)
    }

    #[test]
    fn test_is_match_valid_podspec_json() {
        assert!(PodspecJsonParser::is_match(&PathBuf::from(
            "Test.podspec.json"
        )));
        assert!(PodspecJsonParser::is_match(&PathBuf::from(
            "FirebaseAnalytics.podspec.json"
        )));
        assert!(PodspecJsonParser::is_match(&PathBuf::from(
            "path/to/Package.podspec.json"
        )));
    }

    #[test]
    fn test_is_match_invalid_files() {
        assert!(!PodspecJsonParser::is_match(&PathBuf::from("package.json")));
        assert!(!PodspecJsonParser::is_match(&PathBuf::from("Test.podspec")));
        assert!(!PodspecJsonParser::is_match(&PathBuf::from("test.json")));
        assert!(!PodspecJsonParser::is_match(&PathBuf::from("podspec.json")));
    }

    #[test]
    fn test_extract_basic_fields() {
        let content = r#"{
            "name": "TestPod",
            "version": "1.0.0",
            "summary": "A test pod",
            "description": "A longer description of the test pod",
            "homepage": "https://example.com",
            "license": "MIT",
            "authors": "Test Author"
        }"#;

        let (_temp_dir, file_path) = create_temp_podspec_json(content);
        let package_data = PodspecJsonParser::extract_first_package(&file_path);

        assert_eq!(package_data.package_type, Some(PackageType::Cocoapods));
        assert_eq!(package_data.name, Some("TestPod".to_string()));
        assert_eq!(package_data.version, Some("1.0.0".to_string()));
        assert_eq!(
            package_data.description,
            Some("A test pod. A longer description of the test pod".to_string())
        );
        assert_eq!(
            package_data.homepage_url,
            Some("https://example.com".to_string())
        );
        assert_eq!(
            package_data.extracted_license_statement,
            Some("MIT".to_string())
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
        assert_eq!(
            package_data.primary_language,
            Some("Objective-C".to_string())
        );
        assert_eq!(package_data.parties.len(), 1);
        assert_eq!(
            package_data.parties[0].name,
            Some("Test Author".to_string())
        );
        assert_eq!(package_data.parties[0].role, Some("owner".to_string()));
    }

    #[test]
    fn test_extract_license_as_dict() {
        let content = r#"{
            "name": "TestPod",
            "version": "1.0.0",
            "license": {
                "type": "MIT",
                "text": "Copyright 2024 Test"
            }
        }"#;

        let (_temp_dir, file_path) = create_temp_podspec_json(content);
        let package_data = PodspecJsonParser::extract_first_package(&file_path);

        // License dict values should be joined with space
        let license_statement = package_data.extracted_license_statement.unwrap();
        assert!(license_statement.contains("MIT"));
        assert!(license_statement.contains("Copyright 2024 Test"));
        assert_eq!(
            package_data.declared_license_expression.as_deref(),
            Some("mit")
        );
        assert_eq!(
            package_data.declared_license_expression_spdx.as_deref(),
            Some("MIT")
        );
        assert_eq!(package_data.license_detections.len(), 1);
    }

    #[test]
    fn test_extract_license_file_reference_from_dict() {
        let content = r#"{
            "name": "TestPod",
            "version": "1.0.0",
            "license": {
                "type": "MIT",
                "file": "LICENSE.txt"
            }
        }"#;

        let (_temp_dir, file_path) = create_temp_podspec_json(content);
        let package_data = PodspecJsonParser::extract_first_package(&file_path);

        assert_eq!(package_data.license_detections.len(), 1);
        assert_eq!(
            package_data.license_detections[0].matches[0]
                .referenced_filenames
                .as_ref(),
            Some(&vec!["LICENSE.txt".to_string()])
        );
    }

    #[test]
    fn test_extract_source_git() {
        let content = r#"{
            "name": "TestPod",
            "version": "1.0.0",
            "source": {
                "git": "https://github.com/test/test.git",
                "tag": "1.0.0"
            }
        }"#;

        let (_temp_dir, file_path) = create_temp_podspec_json(content);
        let package_data = PodspecJsonParser::extract_first_package(&file_path);

        assert_eq!(
            package_data.vcs_url,
            Some("https://github.com/test/test.git".to_string())
        );
        assert_eq!(package_data.download_url, None);
    }

    #[test]
    fn test_extract_source_http() {
        let content = r#"{
            "name": "TestPod",
            "version": "1.0.0",
            "source": {
                "http": "https://example.com/test.zip"
            }
        }"#;

        let (_temp_dir, file_path) = create_temp_podspec_json(content);
        let package_data = PodspecJsonParser::extract_first_package(&file_path);

        assert_eq!(package_data.vcs_url, None);
        assert_eq!(
            package_data.download_url,
            Some("https://example.com/test.zip".to_string())
        );
    }

    #[test]
    fn test_extract_source_string() {
        let content = r#"{
            "name": "TestPod",
            "version": "1.0.0",
            "source": "https://github.com/test/test.git"
        }"#;

        let (_temp_dir, file_path) = create_temp_podspec_json(content);
        let package_data = PodspecJsonParser::extract_first_package(&file_path);

        assert_eq!(
            package_data.vcs_url,
            Some("https://github.com/test/test.git".to_string())
        );
    }

    #[test]
    fn test_extract_authors_dict() {
        let content = r#"{
            "name": "TestPod",
            "version": "1.0.0",
            "authors": {
                "Google": "google",
                "Apple": "apple.com"
            }
        }"#;

        let (_temp_dir, file_path) = create_temp_podspec_json(content);
        let package_data = PodspecJsonParser::extract_first_package(&file_path);

        assert_eq!(package_data.parties.len(), 2);

        // Find Google and Apple parties
        let google = package_data
            .parties
            .iter()
            .find(|p| p.name.as_ref() == Some(&"Google".to_string()));
        let apple = package_data
            .parties
            .iter()
            .find(|p| p.name.as_ref() == Some(&"Apple".to_string()));

        assert!(google.is_some());
        assert!(apple.is_some());

        // Check URLs - "google" should become "google.com"
        assert_eq!(google.unwrap().url, Some("google.com".to_string()));
        // "apple.com" should remain as is
        assert_eq!(apple.unwrap().url, Some("apple.com".to_string()));
    }

    #[test]
    fn test_extract_authors_string() {
        let content = r#"{
            "name": "TestPod",
            "version": "1.0.0",
            "authors": "Google, Inc."
        }"#;

        let (_temp_dir, file_path) = create_temp_podspec_json(content);
        let package_data = PodspecJsonParser::extract_first_package(&file_path);

        assert_eq!(package_data.parties.len(), 1);
        assert_eq!(
            package_data.parties[0].name,
            Some("Google, Inc.".to_string())
        );
        assert_eq!(package_data.parties[0].role, Some("owner".to_string()));
        assert_eq!(package_data.parties[0].url, None);
    }

    #[test]
    fn test_extract_dependencies() {
        let content = r#"{
            "name": "TestPod",
            "version": "1.0.0",
            "dependencies": {
                "FirebaseCore": "~> 8.0",
                "FirebaseInstallations": "~> 8.0",
                "GoogleUtilities/Network": "~> 7.4"
            }
        }"#;

        let (_temp_dir, file_path) = create_temp_podspec_json(content);
        let package_data = PodspecJsonParser::extract_first_package(&file_path);

        assert_eq!(package_data.dependencies.len(), 3);

        // Check first dependency
        let firebase_core = &package_data.dependencies[0];
        assert!(firebase_core.purl.is_some());
        assert!(
            firebase_core
                .purl
                .as_ref()
                .unwrap()
                .contains("FirebaseCore")
        );
        assert_eq!(
            firebase_core.extracted_requirement,
            Some("~> 8.0".to_string())
        );
        assert_eq!(firebase_core.scope, Some("runtime".to_string()));
        assert_eq!(firebase_core.is_runtime, Some(true));
        assert_eq!(firebase_core.is_optional, Some(false));
    }

    #[test]
    fn test_extra_data_contains_full_json() {
        let content = r#"{
            "name": "TestPod",
            "version": "1.0.0",
            "summary": "Test",
            "source": {
                "git": "https://github.com/test/test.git"
            },
            "dependencies": {
                "Dep1": "1.0"
            }
        }"#;

        let (_temp_dir, file_path) = create_temp_podspec_json(content);
        let package_data = PodspecJsonParser::extract_first_package(&file_path);

        assert!(package_data.extra_data.is_some());
        let extra_data = package_data.extra_data.unwrap();

        // Check that podspec.json key exists
        assert!(extra_data.contains_key("podspec.json"));

        // Check that source key exists
        assert!(extra_data.contains_key("source"));

        // Check that dependencies key exists
        assert!(extra_data.contains_key("dependencies"));
    }

    #[test]
    fn test_generate_urls() {
        let content = r#"{
            "name": "TestPod",
            "version": "1.0.0",
            "homepage": "https://example.com",
            "source": {
                "git": "https://github.com/test/TestPod.git"
            }
        }"#;

        let (_temp_dir, file_path) = create_temp_podspec_json(content);
        let package_data = PodspecJsonParser::extract_first_package(&file_path);

        // Check repository_homepage_url
        assert_eq!(
            package_data.repository_homepage_url,
            Some("https://cocoapods.org/pods/TestPod".to_string())
        );

        // Check repository_download_url
        assert_eq!(
            package_data.repository_download_url,
            Some("https://example.com/archive/1.0.0.zip".to_string())
        );

        // Check code_view_url
        assert_eq!(
            package_data.code_view_url,
            Some("https://github.com/test/TestPod/tree/1.0.0".to_string())
        );

        // Check bug_tracking_url
        assert_eq!(
            package_data.bug_tracking_url,
            Some("https://github.com/test/TestPod/issues/".to_string())
        );

        // Check api_data_url (uses hashed path)
        assert!(package_data.api_data_url.is_some());
        assert!(
            package_data
                .api_data_url
                .unwrap()
                .contains("https://raw.githubusercontent.com/CocoaPods/Specs")
        );
    }

    #[test]
    fn test_generate_purl() {
        let content = r#"{
            "name": "TestPod",
            "version": "1.0.0"
        }"#;

        let (_temp_dir, file_path) = create_temp_podspec_json(content);
        let package_data = PodspecJsonParser::extract_first_package(&file_path);

        assert!(package_data.purl.is_some());
        let purl = package_data.purl.unwrap();
        assert!(purl.contains("pkg:cocoapods/TestPod"));
        assert!(purl.contains("1.0.0"));
    }

    #[test]
    fn test_summary_only_becomes_description() {
        let content = r#"{
            "name": "TestPod",
            "version": "1.0.0",
            "summary": "Just a summary"
        }"#;

        let (_temp_dir, file_path) = create_temp_podspec_json(content);
        let package_data = PodspecJsonParser::extract_first_package(&file_path);

        assert_eq!(package_data.description, Some("Just a summary".to_string()));
    }

    #[test]
    fn test_description_starting_with_summary_not_duplicated() {
        let content = r#"{
            "name": "TestPod",
            "version": "1.0.0",
            "summary": "Short summary",
            "description": "Short summary with more details"
        }"#;

        let (_temp_dir, file_path) = create_temp_podspec_json(content);
        let package_data = PodspecJsonParser::extract_first_package(&file_path);

        // Should not prepend summary if description already starts with it
        assert_eq!(
            package_data.description,
            Some("Short summary with more details".to_string())
        );
    }

    #[test]
    fn test_malformed_json() {
        let content = r#"{ "name": "TestPod", invalid json }"#;

        let (_temp_dir, file_path) = create_temp_podspec_json(content);
        let package_data = PodspecJsonParser::extract_first_package(&file_path);

        // Should return default package data
        assert_eq!(package_data.package_type, Some(PackageType::Cocoapods));
        assert_eq!(package_data.name, None);
    }

    #[test]
    fn test_empty_fields_filtered() {
        let content = r#"{
            "name": "  ",
            "version": "",
            "summary": "   ",
            "description": ""
        }"#;

        let (_temp_dir, file_path) = create_temp_podspec_json(content);
        let package_data = PodspecJsonParser::extract_first_package(&file_path);

        // Empty/whitespace-only fields should be None
        assert_eq!(package_data.name, None);
        assert_eq!(package_data.version, None);
        assert_eq!(package_data.description, None);
    }

    #[test]
    fn test_extract_from_firebase_analytics() {
        let file_path = PathBuf::from("testdata/cocoapods/FirebaseAnalytics.podspec.json");

        let package_data = PodspecJsonParser::extract_first_package(&file_path);

        assert_eq!(package_data.name, Some("FirebaseAnalytics".to_string()));
        assert_eq!(package_data.version, Some("8.1.1".to_string()));
        assert_eq!(
            package_data.homepage_url,
            Some("https://firebase.google.com/features/analytics/".to_string())
        );
        assert!(package_data.extracted_license_statement.is_some());
        assert!(
            package_data
                .extracted_license_statement
                .unwrap()
                .contains("Copyright")
        );
        assert!(package_data.declared_license_expression.is_none());
        assert!(package_data.declared_license_expression_spdx.is_none());
        assert!(package_data.license_detections.is_empty());

        // Check dependencies
        assert!(!package_data.dependencies.is_empty());
        let firebase_core = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().unwrap().contains("FirebaseCore"));
        assert!(firebase_core.is_some());

        // Check parties
        assert_eq!(package_data.parties.len(), 1);
        assert_eq!(
            package_data.parties[0].name,
            Some("Google, Inc.".to_string())
        );

        // Check URLs
        assert_eq!(
            package_data.repository_homepage_url,
            Some("https://cocoapods.org/pods/FirebaseAnalytics".to_string())
        );
    }
}
