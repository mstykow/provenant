// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use crate::models::DatasourceId;
    use crate::models::PackageType;
    use crate::parsers::{HaxeParser, PackageParser};
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // Helper function to create a temporary haxelib.json file with the given content
    fn create_temp_haxelib_json(content: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let haxelib_path = temp_dir.path().join("haxelib.json");
        fs::write(&haxelib_path, content).expect("Failed to write haxelib.json");

        (temp_dir, haxelib_path)
    }

    #[test]
    fn test_is_match() {
        let valid_path = PathBuf::from("/some/path/haxelib.json");
        let invalid_path = PathBuf::from("/some/path/not_haxelib.json");

        assert!(HaxeParser::is_match(&valid_path));
        assert!(!HaxeParser::is_match(&invalid_path));
    }

    #[test]
    fn test_extract_from_testdata() {
        let haxelib_path = PathBuf::from("testdata/haxe/basic/haxelib.json");
        let package_data = HaxeParser::extract_first_package(&haxelib_path);

        assert_eq!(package_data.package_type, Some(PackageType::Haxe));
        assert_eq!(package_data.name, Some("haxelib".to_string()));
        assert_eq!(package_data.version, Some("3.4.0".to_string()));
        assert_eq!(
            package_data.homepage_url,
            Some("https://lib.haxe.org/documentation/".to_string())
        );
        assert_eq!(
            package_data.download_url,
            Some("https://lib.haxe.org/p/haxelib/3.4.0/download/".to_string())
        );

        assert_eq!(
            package_data.extracted_license_statement,
            Some("GPL".to_string())
        );
        assert!(package_data.declared_license_expression.is_none());
        assert!(package_data.declared_license_expression_spdx.is_none());
        assert!(package_data.license_detections.is_empty());

        // Check purl
        assert_eq!(
            package_data.purl,
            Some("pkg:haxe/haxelib@3.4.0".to_string())
        );

        // Check contributor extraction
        assert_eq!(package_data.parties.len(), 6);
        let names: Vec<&str> = package_data
            .parties
            .iter()
            .filter_map(|p| p.name.as_deref())
            .collect();
        assert!(names.contains(&"back2dos"));
        assert!(names.contains(&"ncannasse"));

        // Verify all contributors have proper URLs
        for party in &package_data.parties {
            assert!(
                party
                    .url
                    .as_ref()
                    .unwrap()
                    .contains("https://lib.haxe.org/u/")
            );
            assert_eq!(party.role, Some("contributor".to_string()));
        }
    }

    #[test]
    fn test_dependencies() {
        let haxelib_path = PathBuf::from("testdata/haxe/deps/haxelib.json");
        let package_data = HaxeParser::extract_first_package(&haxelib_path);

        assert_eq!(package_data.dependencies.len(), 2);

        // Check for unpinned dependency (empty version string)
        let unpinned = package_data
            .dependencies
            .iter()
            .find(|d| d.is_pinned == Some(false))
            .expect("Should have unpinned dependency");
        assert_eq!(unpinned.extracted_requirement, None);
        assert!(unpinned.purl.as_ref().unwrap().contains("tink_core"));

        // Check for pinned dependency
        let pinned = package_data
            .dependencies
            .iter()
            .find(|d| d.is_pinned == Some(true))
            .expect("Should have pinned dependency");
        assert_eq!(pinned.extracted_requirement, None);
        assert!(pinned.purl.as_ref().unwrap().contains("tink_macro@3.23"));
    }

    #[test]
    fn test_validation_empty_json() {
        let (_temp_dir, haxelib_path) = create_temp_haxelib_json("{}");
        let package_data = HaxeParser::extract_first_package(&haxelib_path);

        // Should have proper type and datasource even for empty/invalid data
        assert_eq!(package_data.package_type, Some(PackageType::Haxe));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::HaxelibJson));
        assert!(package_data.name.is_none());
    }

    #[test]
    fn test_minimal_valid_json() {
        let minimal_json = r#"
        {
            "name": "minimal",
            "version": "1.0.0",
            "license": "MIT",
            "contributors": []
        }
        "#;

        let (_temp_dir, haxelib_path) = create_temp_haxelib_json(minimal_json);
        let package_data = HaxeParser::extract_first_package(&haxelib_path);

        assert_eq!(package_data.name, Some("minimal".to_string()));
        assert_eq!(package_data.version, Some("1.0.0".to_string()));
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
            package_data.purl,
            Some("pkg:haxe/minimal@1.0.0".to_string())
        );
    }

    #[test]
    fn test_url_generation() {
        let haxelib_path = PathBuf::from("testdata/haxe/basic/haxelib.json");
        let package_data = HaxeParser::extract_first_package(&haxelib_path);

        // Repository homepage URL should be generated
        assert_eq!(
            package_data.repository_homepage_url,
            Some("https://lib.haxe.org/p/haxelib".to_string())
        );

        // Download URL should be generated from version
        assert_eq!(
            package_data.download_url,
            Some("https://lib.haxe.org/p/haxelib/3.4.0/download/".to_string())
        );

        // Repository download URL should match download URL
        assert_eq!(
            package_data.repository_download_url,
            Some("https://lib.haxe.org/p/haxelib/3.4.0/download/".to_string())
        );
    }

    #[test]
    fn test_with_tags() {
        let haxelib_path = PathBuf::from("testdata/haxe/tags/haxelib.json");
        let package_data = HaxeParser::extract_first_package(&haxelib_path);

        assert_eq!(package_data.name, Some("tink_core".to_string()));

        // Keywords should be extracted from tags
        assert_eq!(
            package_data.keywords,
            vec![
                "tink".to_string(),
                "cross".to_string(),
                "utility".to_string(),
                "reactive".to_string(),
                "functional".to_string(),
                "async".to_string(),
                "lazy".to_string(),
                "signal".to_string(),
                "event".to_string(),
            ]
        );
    }

    #[test]
    fn test_invalid_json_format() {
        let invalid_json = "{ invalid json }";

        let (_temp_dir, haxelib_path) = create_temp_haxelib_json(invalid_json);
        let package_data = HaxeParser::extract_first_package(&haxelib_path);

        // Should gracefully handle invalid JSON
        assert_eq!(package_data.package_type, Some(PackageType::Haxe));
        assert!(package_data.name.is_none());
    }
}
