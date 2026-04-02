#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use tempfile::TempDir;

    use crate::models::{DatasourceId, PackageType};
    use crate::parsers::{CitationCffParser, PackageParser};

    fn create_temp_citation(content: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("tempdir");
        let path = temp_dir.path().join("CITATION.cff");
        fs::write(&path, content).expect("write CITATION.cff");
        (temp_dir, path)
    }

    #[test]
    fn test_is_match() {
        assert!(CitationCffParser::is_match(
            PathBuf::from("CITATION.cff").as_path()
        ));
        assert!(!CitationCffParser::is_match(
            PathBuf::from("citation.yml").as_path()
        ));
    }

    #[test]
    fn test_extract_basic_citation_metadata() {
        let (_temp_dir, path) = create_temp_citation(
            "cff-version: 1.2.0\ntitle: Demo Project\nversion: 1.0.0\nabstract: Demo abstract\nlicense: MIT\nurl: https://example.com\nrepository-code: https://github.com/example/demo\nauthors:\n  - given-names: Ada\n    family-names: Lovelace\n    email: ada@example.com\n",
        );

        let package = CitationCffParser::extract_first_package(&path);
        assert_eq!(package.package_type, Some(PackageType::Generic));
        assert_eq!(package.datasource_id, Some(DatasourceId::CitationCff));
        assert_eq!(package.name.as_deref(), Some("Demo Project"));
        assert_eq!(package.version.as_deref(), Some("1.0.0"));
        assert_eq!(package.description.as_deref(), Some("Demo abstract"));
        assert_eq!(package.declared_license_expression.as_deref(), Some("mit"));
        assert_eq!(package.homepage_url.as_deref(), Some("https://example.com"));
        assert_eq!(
            package.vcs_url.as_deref(),
            Some("https://github.com/example/demo")
        );
        assert_eq!(package.parties.len(), 1);
        assert_eq!(package.parties[0].name.as_deref(), Some("Ada Lovelace"));
    }

    #[test]
    fn test_missing_cff_version_returns_default_package() {
        let (_temp_dir, path) = create_temp_citation("title: Missing Version\n");
        let package = CitationCffParser::extract_first_package(&path);
        assert_eq!(package.package_type, Some(PackageType::Generic));
        assert_eq!(package.datasource_id, Some(DatasourceId::CitationCff));
        assert!(package.name.is_none());
    }
}
