// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use super::super::PackageParser;
    use super::super::bower::BowerJsonParser;
    use crate::models::DatasourceId;
    use crate::models::PackageType;
    use std::path::PathBuf;

    #[test]
    fn test_is_match() {
        assert!(BowerJsonParser::is_match(&PathBuf::from("bower.json")));
        assert!(BowerJsonParser::is_match(&PathBuf::from(".bower.json")));
        assert!(BowerJsonParser::is_match(&PathBuf::from(
            "/path/to/project/bower.json"
        )));
        assert!(!BowerJsonParser::is_match(&PathBuf::from("package.json")));
        assert!(!BowerJsonParser::is_match(&PathBuf::from("composer.json")));
        assert!(!BowerJsonParser::is_match(&PathBuf::from("bower.lock")));
    }

    #[test]
    fn test_basic_extraction() {
        let path = PathBuf::from("testdata/bower/basic/bower.json");
        let package_data = BowerJsonParser::extract_first_package(&path);

        assert_eq!(package_data.package_type, Some(PackageType::Bower));
        assert_eq!(package_data.name, Some("blue-leaf".to_string()));
        assert_eq!(
            package_data.description,
            Some("Physics-like animations for pretty particles".to_string())
        );
        assert_eq!(
            package_data.primary_language,
            Some("JavaScript".to_string())
        );
        assert_eq!(package_data.datasource_id, Some(DatasourceId::BowerJson));
        assert!(package_data.is_private);
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
    fn test_dependencies() {
        let path = PathBuf::from("testdata/bower/basic/bower.json");
        let package_data = BowerJsonParser::extract_first_package(&path);

        assert_eq!(package_data.dependencies.len(), 3);

        // Check runtime dependencies
        let runtime_deps: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|d| d.scope == Some("dependencies".to_string()))
            .collect();
        assert_eq!(runtime_deps.len(), 2);

        let get_size = runtime_deps
            .iter()
            .find(|d| d.purl == Some("pkg:bower/get-size".to_string()));
        assert!(get_size.is_some());
        assert_eq!(
            get_size.unwrap().extracted_requirement,
            Some("~1.2.2".to_string())
        );
        assert_eq!(get_size.unwrap().is_runtime, Some(true));
        assert_eq!(get_size.unwrap().is_optional, Some(false));

        // Check dev dependencies
        let dev_deps: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|d| d.scope == Some("devDependencies".to_string()))
            .collect();
        assert_eq!(dev_deps.len(), 1);

        let qunit = dev_deps
            .iter()
            .find(|d| d.purl == Some("pkg:bower/qunit".to_string()));
        assert!(qunit.is_some());
        assert_eq!(
            qunit.unwrap().extracted_requirement,
            Some("~1.16.0".to_string())
        );
        assert_eq!(qunit.unwrap().is_runtime, Some(false));
        assert_eq!(qunit.unwrap().is_optional, Some(true));
    }

    #[test]
    fn test_author_string() {
        let path = PathBuf::from("testdata/bower/basic/bower.json");
        let package_data = BowerJsonParser::extract_first_package(&path);

        assert_eq!(package_data.parties.len(), 1);

        let author = &package_data.parties[0];
        assert_eq!(author.name, Some("Betty Beta".to_string()));
        assert_eq!(author.email, Some("bbeta@example.com".to_string()));
        assert_eq!(author.role, Some("author".to_string()));
        assert_eq!(author.r#type, Some("person".to_string()));
    }

    #[test]
    fn test_author_object() {
        let path = PathBuf::from("testdata/bower/author-objects/bower.json");
        let package_data = BowerJsonParser::extract_first_package(&path);

        assert_eq!(package_data.parties.len(), 2);

        // First author (string format)
        let author1 = &package_data.parties[0];
        assert_eq!(author1.name, Some("Betty Beta".to_string()));
        assert_eq!(author1.email, Some("bbeta@example.com".to_string()));
        assert_eq!(author1.role, Some("author".to_string()));

        // Second author (object format)
        let author2 = &package_data.parties[1];
        assert_eq!(author2.name, Some("John Doe".to_string()));
        assert_eq!(author2.email, Some("john@doe.com".to_string()));
        assert_eq!(author2.url, Some("http://johndoe.com".to_string()));
        assert_eq!(author2.role, Some("author".to_string()));
    }

    #[test]
    fn test_license_array() {
        let path = PathBuf::from("testdata/bower/list-of-licenses/bower.json");
        let package_data = BowerJsonParser::extract_first_package(&path);

        assert_eq!(
            package_data.extracted_license_statement,
            Some("MIT AND Apache 2.0 AND BSD-3-Clause".to_string())
        );
        assert!(package_data.declared_license_expression.is_none());
        assert!(package_data.declared_license_expression_spdx.is_none());
        assert!(package_data.license_detections.is_empty());
    }

    #[test]
    fn test_keywords() {
        let path = PathBuf::from("testdata/bower/basic/bower.json");
        let package_data = BowerJsonParser::extract_first_package(&path);

        assert_eq!(package_data.keywords.len(), 3);
        assert!(package_data.keywords.contains(&"motion".to_string()));
        assert!(package_data.keywords.contains(&"physics".to_string()));
        assert!(package_data.keywords.contains(&"particles".to_string()));
    }

    #[test]
    fn test_private_package() {
        let path = PathBuf::from("testdata/bower/basic/bower.json");
        let package_data = BowerJsonParser::extract_first_package(&path);

        assert!(package_data.is_private);
    }

    #[test]
    fn test_malformed_json() {
        // Create a temporary file with malformed JSON
        use std::fs;
        use std::io::Write;
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("malformed_bower.json");
        let mut file = fs::File::create(&temp_file).unwrap();
        file.write_all(b"{ invalid json }").unwrap();

        let package_data = BowerJsonParser::extract_first_package(&temp_file);

        // Should return default package data on error
        assert_eq!(package_data.name, None);
        assert_eq!(package_data.package_type, Some(PackageType::Bower));
        assert_eq!(
            package_data.primary_language,
            Some("JavaScript".to_string())
        );
        assert_eq!(package_data.datasource_id, Some(DatasourceId::BowerJson));

        // Cleanup
        fs::remove_file(temp_file).ok();
    }
}
