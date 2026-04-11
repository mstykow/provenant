#[cfg(test)]
mod tests {
    use super::super::PackageParser;
    use super::super::pip_inspect_deplock::*;
    use crate::models::DatasourceId;
    use crate::models::PackageType;
    use std::path::PathBuf;

    #[test]
    fn test_is_match() {
        assert!(PipInspectDeplockParser::is_match(&PathBuf::from(
            "/path/to/pip-inspect.deplock"
        )));
        assert!(PipInspectDeplockParser::is_match(&PathBuf::from(
            "some/dir/pip-inspect.deplock"
        )));
        assert!(!PipInspectDeplockParser::is_match(&PathBuf::from(
            "pip-inspect.json"
        )));
        assert!(!PipInspectDeplockParser::is_match(&PathBuf::from(
            "package.json"
        )));
    }

    #[test]
    fn test_parse_basic_pip_inspect() {
        let content = r#"{
  "version": "1",
  "pip_version": "23.0.1",
  "installed": [
    {
      "metadata": {
        "name": "requests",
        "version": "2.31.0",
        "license": "Apache 2.0",
        "description": "Python HTTP for Humans.",
        "keywords": "http,requests"
      },
      "requested": true,
      "direct_url": {
        "url": "file:///path/to/requests"
      }
    },
    {
      "metadata": {
        "name": "urllib3",
        "version": "2.0.0",
        "license": "MIT"
      },
      "requested": false
    }
  ]
}"#;
        let pkg = parse_pip_inspect_deplock(content);

        assert_eq!(pkg.name.as_deref(), Some("requests"));
        assert_eq!(pkg.version.as_deref(), Some("2.31.0"));
        assert_eq!(
            pkg.extracted_license_statement.as_deref(),
            Some("Apache 2.0")
        );
        assert_eq!(pkg.description.as_deref(), Some("Python HTTP for Humans."));
        assert_eq!(pkg.keywords, vec!["http,requests".to_string()]);
        assert_eq!(pkg.package_type, Some(PackageType::Pypi));
        assert_eq!(pkg.primary_language.as_deref(), Some("Python"));
        assert_eq!(pkg.datasource_id, Some(DatasourceId::PypiInspectDeplock));
        assert!(pkg.is_virtual);
        assert!(pkg.dependencies.is_empty());

        // Check extra_data
        assert!(pkg.extra_data.is_some());
        let extra = pkg.extra_data.as_ref().unwrap();
        assert_eq!(
            extra.get("pip_version").and_then(|v| v.as_str()),
            Some("23.0.1")
        );
        assert_eq!(
            extra.get("inspect_version").and_then(|v| v.as_str()),
            Some("1")
        );
    }

    #[test]
    fn test_parse_no_direct_url() {
        let content = r#"{
  "version": "1",
  "installed": [
    {
      "metadata": {
        "name": "my-package",
        "version": "1.0.0",
        "license": "MIT"
      },
      "requested": true
    }
  ]
}"#;
        let pkg = parse_pip_inspect_deplock(content);

        // Should find the requested package even without direct_url
        assert_eq!(pkg.name.as_deref(), Some("my-package"));
        assert_eq!(pkg.version.as_deref(), Some("1.0.0"));
        assert_eq!(pkg.extracted_license_statement.as_deref(), Some("MIT"));
    }

    #[test]
    fn test_parse_no_installed_packages() {
        let content = r#"{
  "version": "1",
  "pip_version": "23.0.1",
  "installed": []
}"#;
        let pkg = parse_pip_inspect_deplock(content);

        // Should return default package
        assert_eq!(pkg.package_type, Some(PackageType::Pypi));
        assert_eq!(pkg.datasource_id, Some(DatasourceId::PypiInspectDeplock));
    }

    #[test]
    fn test_parse_invalid_json() {
        let content = "this is not valid json {{{";
        let pkg = parse_pip_inspect_deplock(content);

        // Should return default package on parse error
        assert_eq!(pkg.package_type, Some(PackageType::Pypi));
        assert_eq!(pkg.datasource_id, Some(DatasourceId::PypiInspectDeplock));
    }

    #[test]
    fn test_parse_requires_dist_dependencies() {
        let content = r#"{
  "version": "1",
  "installed": [
    {
      "metadata": {
        "name": "demo",
        "version": "1.0.0",
        "license": "MIT",
        "requires_dist": [
          "typing-extensions>=4.0.0",
          "importlib-metadata; python_version < \"3.10\""
        ]
      },
      "requested": true,
      "direct_url": {
        "url": "file:///path/to/demo"
      }
    }
  ]
}"#;
        let pkg = parse_pip_inspect_deplock(content);

        assert_eq!(pkg.declared_license_expression.as_deref(), Some("mit"));
        assert_eq!(pkg.declared_license_expression_spdx.as_deref(), Some("MIT"));
        assert_eq!(pkg.license_detections.len(), 1);
        assert_eq!(pkg.dependencies.len(), 2);
        assert!(
            pkg.dependencies
                .iter()
                .any(|dep| dep.purl.as_deref() == Some("pkg:pypi/typing-extensions"))
        );
        assert!(pkg.dependencies.iter().any(|dep| {
            dep.purl.as_deref() == Some("pkg:pypi/importlib-metadata")
                && dep
                    .extra_data
                    .as_ref()
                    .and_then(|extra| extra.get("python_version"))
                    .and_then(|value| value.as_str())
                    == Some("< 3.10")
        }));
    }

    #[test]
    fn test_extract_fixture_uses_python_parser_compat_path() {
        let test_file = PathBuf::from("testdata/python/pip-inspect/pip-inspect.deplock");
        let pkg = PipInspectDeplockParser::extract_first_package(&test_file);

        assert_eq!(pkg.package_type, Some(PackageType::Pypi));
        assert_eq!(pkg.datasource_id, Some(DatasourceId::PypiInspectDeplock));
        assert_eq!(pkg.name.as_deref(), Some("univers"));
        assert_eq!(pkg.version.as_deref(), Some("0.0.0"));
        assert_eq!(pkg.primary_language.as_deref(), Some("Python"));
        assert!(pkg.is_virtual);
        assert_eq!(pkg.dependencies.len(), 5);
    }
}
