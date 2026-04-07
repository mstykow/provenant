mod tests {
    use std::path::PathBuf;

    use crate::parsers::pep508::parse_pep508_requirement;
    use crate::parsers::{PackageParser, RequirementsTxtParser};

    #[test]
    fn test_pep508_parsing_variants() {
        let requirement = "package[extra1,extra2]>=1.0,<2.0; python_version >= '3.8'";
        let parsed = parse_pep508_requirement(requirement).expect("parse pep508");
        assert_eq!(parsed.name, "package");
        assert_eq!(
            parsed.extras,
            vec!["extra1".to_string(), "extra2".to_string()]
        );
        assert_eq!(parsed.specifiers.as_deref(), Some(">=1.0,<2.0"));
        assert_eq!(parsed.marker.as_deref(), Some("python_version >= '3.8'"));

        let requirement = "lib @ https://example.com/lib-1.0.tar.gz; os_name == 'posix'";
        let parsed = parse_pep508_requirement(requirement).expect("parse pep508");
        assert_eq!(parsed.name, "lib");
        assert!(parsed.is_name_at_url);
        assert_eq!(
            parsed.url.as_deref(),
            Some("https://example.com/lib-1.0.tar.gz")
        );
        assert_eq!(parsed.marker.as_deref(), Some("os_name == 'posix'"));
    }

    #[test]
    fn test_requirements_single_level_include() {
        let test_file = PathBuf::from("testdata/python/requirements-includes/requirements.txt");
        let package_data = RequirementsTxtParser::extract_first_package(&test_file);

        assert_eq!(package_data.dependencies.len(), 3);

        let purls: Vec<&str> = package_data
            .dependencies
            .iter()
            .filter_map(|d| d.purl.as_deref())
            .collect();

        assert!(
            purls.iter().any(|p| p.contains("pkg:pypi/requests")),
            "Should contain requests from main file"
        );
        assert!(
            purls.iter().any(|p| p.contains("pkg:pypi/pytest")),
            "Should contain pytest from included file"
        );
        assert!(
            purls.iter().any(|p| p.contains("pkg:pypi/black")),
            "Should contain black from included file"
        );

        assert!(package_data.extra_data.is_some());
        let extra_data = package_data.extra_data.unwrap();
        assert!(extra_data.contains_key("requirements_includes"));
    }

    #[test]
    fn test_requirements_nested_includes() {
        let test_file = PathBuf::from("testdata/python/requirements-nested/requirements.txt");
        let package_data = RequirementsTxtParser::extract_first_package(&test_file);

        assert_eq!(package_data.dependencies.len(), 4);

        let purls: Vec<&str> = package_data
            .dependencies
            .iter()
            .filter_map(|d| d.purl.as_deref())
            .collect();

        assert!(
            purls.iter().any(|p| p.contains("pkg:pypi/requests")),
            "Should contain requests from main file"
        );
        assert!(
            purls.iter().any(|p| p.contains("pkg:pypi/pytest")),
            "Should contain pytest from first include"
        );
        assert!(
            purls.iter().any(|p| p.contains("pkg:pypi/coverage")),
            "Should contain coverage from nested include"
        );
        assert!(
            purls.iter().any(|p| p.contains("pkg:pypi/black")),
            "Should contain black from nested include"
        );
    }

    #[test]
    fn test_requirements_circular_include_detection() {
        let test_file = PathBuf::from("testdata/python/requirements-circular/requirements-a.txt");
        let package_data = RequirementsTxtParser::extract_first_package(&test_file);

        assert_eq!(package_data.dependencies.len(), 2);

        let purls: Vec<&str> = package_data
            .dependencies
            .iter()
            .filter_map(|d| d.purl.as_deref())
            .collect();

        assert!(
            purls.iter().any(|p| p.contains("pkg:pypi/requests")),
            "Should contain requests from A"
        );
        assert!(
            purls.iter().any(|p| p.contains("pkg:pypi/pytest")),
            "Should contain pytest from B"
        );
    }

    #[test]
    fn test_requirements_constraints_file() {
        let test_file = PathBuf::from("testdata/python/requirements-constraints/requirements.txt");
        let package_data = RequirementsTxtParser::extract_first_package(&test_file);

        assert_eq!(package_data.dependencies.len(), 3);

        let purls: Vec<&str> = package_data
            .dependencies
            .iter()
            .filter_map(|d| d.purl.as_deref())
            .collect();

        assert!(
            purls.iter().any(|p| p.contains("pkg:pypi/requests")),
            "Should contain requests from main file"
        );
        assert!(
            purls.iter().any(|p| p.contains("pkg:pypi/urllib3")),
            "Should contain urllib3 from constraints file"
        );

        assert!(package_data.extra_data.is_some());
        let extra_data = package_data.extra_data.unwrap();
        assert!(extra_data.contains_key("constraints"));
    }

    #[test]
    fn test_is_match_supports_underscore_and_lockfile_names() {
        assert!(RequirementsTxtParser::is_match(&PathBuf::from(
            "/tmp/requirements_lock_3_11.txt"
        )));
        assert!(RequirementsTxtParser::is_match(&PathBuf::from(
            "/tmp/requirements_build.txt"
        )));
        assert!(!RequirementsTxtParser::is_match(&PathBuf::from(
            "/tmp/key-requirements-expected.txt"
        )));
    }
}
