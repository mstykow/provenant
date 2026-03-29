#[cfg(test)]
mod tests {
    use super::super::PackageParser;
    use super::super::swift_show_dependencies::*;
    use crate::models::DatasourceId;
    use crate::models::PackageType;
    use std::path::PathBuf;

    #[test]
    fn test_is_match() {
        assert!(SwiftShowDependenciesParser::is_match(&PathBuf::from(
            "/path/to/swift-show-dependencies.deplock"
        )));
        assert!(SwiftShowDependenciesParser::is_match(&PathBuf::from(
            "some/dir/swift-show-dependencies.deplock"
        )));
        assert!(SwiftShowDependenciesParser::is_match(&PathBuf::from(
            "swift-show-dependencies.deplock"
        )));
        assert!(!SwiftShowDependenciesParser::is_match(&PathBuf::from(
            "Package.swift"
        )));
        assert!(!SwiftShowDependenciesParser::is_match(&PathBuf::from(
            "dependencies.json"
        )));
    }

    #[test]
    fn test_parse_basic() {
        let content = r#"{"name": "MyPackage"}"#;
        let pkg = parse_swift_show_dependencies(content);

        assert_eq!(pkg.name.as_deref(), Some("MyPackage"));
        assert_eq!(pkg.package_type, Some(PackageType::Swift));
        assert_eq!(pkg.primary_language.as_deref(), Some("Swift"));
        assert_eq!(
            pkg.datasource_id,
            Some(DatasourceId::SwiftPackageShowDependencies)
        );
    }

    #[test]
    fn test_parse_invalid_json() {
        let content = "not json";
        let pkg = parse_swift_show_dependencies(content);

        assert_eq!(pkg.package_type, Some(PackageType::Swift));
        assert_eq!(
            pkg.datasource_id,
            Some(DatasourceId::SwiftPackageShowDependencies)
        );
    }

    #[test]
    fn test_parse_with_dependencies() {
        let content = r#"{
  "name": "VercelUI",
  "version": "1.0.0",
  "url": "https://github.com/vercel/VercelUI",
  "dependencies": [
    {
      "identity": "vercel",
      "name": "Vercel",
      "url": "https://github.com/swift-cloud/Vercel",
      "version": "1.15.2",
      "dependencies": [
        {
          "identity": "vapor",
          "name": "vapor",
          "url": "https://github.com/vapor/vapor",
          "version": "4.79.0",
          "dependencies": []
        }
      ]
    },
    {
      "identity": "swift-nio",
      "name": "swift-nio",
      "url": "https://github.com/apple/swift-nio.git",
      "version": "2.58.0",
      "dependencies": []
    }
  ]
}"#;
        let pkg = parse_swift_show_dependencies(content);

        assert_eq!(pkg.name.as_deref(), Some("VercelUI"));
        assert_eq!(pkg.version.as_deref(), Some("1.0.0"));
        assert_eq!(
            pkg.homepage_url.as_deref(),
            Some("https://github.com/vercel/VercelUI")
        );

        assert_eq!(pkg.dependencies.len(), 3);

        let vercel_dep = pkg
            .dependencies
            .iter()
            .find(|d| d.purl.as_deref() == Some("pkg:swift/github.com/swift-cloud/Vercel@1.15.2"));
        assert!(vercel_dep.is_some());
        let vercel = vercel_dep.unwrap();
        assert_eq!(vercel.extracted_requirement.as_deref(), Some("1.15.2"));
        assert_eq!(vercel.is_direct, Some(true));
        assert_eq!(vercel.is_runtime, None);

        let vapor_dep = pkg
            .dependencies
            .iter()
            .find(|d| d.purl.as_deref() == Some("pkg:swift/github.com/vapor/vapor@4.79.0"));
        assert!(vapor_dep.is_some());
        let vapor = vapor_dep.unwrap();
        assert_eq!(vapor.extracted_requirement.as_deref(), Some("4.79.0"));
        assert_eq!(vapor.is_direct, Some(false));
        assert_eq!(vapor.is_runtime, None);

        let nio_dep = pkg
            .dependencies
            .iter()
            .find(|d| d.purl.as_deref() == Some("pkg:swift/github.com/apple/swift-nio@2.58.0"));
        assert!(nio_dep.is_some());
        let nio = nio_dep.unwrap();
        assert_eq!(nio.extracted_requirement.as_deref(), Some("2.58.0"));
        assert_eq!(nio.is_direct, Some(true));
        assert_eq!(nio.is_runtime, None);
    }

    #[test]
    fn test_parse_no_dependencies() {
        let content = r#"{
  "name": "SimplePackage",
  "version": "1.0.0",
  "dependencies": []
}"#;
        let pkg = parse_swift_show_dependencies(content);

        assert_eq!(pkg.name.as_deref(), Some("SimplePackage"));
        assert_eq!(pkg.version.as_deref(), Some("1.0.0"));
        assert!(pkg.dependencies.is_empty());
    }
}
