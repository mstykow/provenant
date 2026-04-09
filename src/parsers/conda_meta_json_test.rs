#[cfg(test)]
mod tests {
    use super::super::PackageParser;
    use super::super::conda_meta_json::*;
    use crate::models::DatasourceId;
    use crate::models::Md5Digest;
    use crate::models::PackageType;
    use crate::models::Sha256Digest;
    use std::path::PathBuf;

    #[test]
    fn test_is_match() {
        assert!(CondaMetaJsonParser::is_match(&PathBuf::from(
            "/opt/conda/conda-meta/package.json"
        )));
        assert!(CondaMetaJsonParser::is_match(&PathBuf::from(
            "conda-meta/package.json"
        )));
        assert!(CondaMetaJsonParser::is_match(&PathBuf::from(
            "/some/path/conda-meta/requests-2.32.3-py312h06a4308_1.json"
        )));
        assert!(!CondaMetaJsonParser::is_match(&PathBuf::from(
            "package.json"
        )));
        assert!(!CondaMetaJsonParser::is_match(&PathBuf::from(
            "conda/package.json"
        )));
        assert!(!CondaMetaJsonParser::is_match(&PathBuf::from("meta.yaml")));
    }

    #[test]
    fn test_parse_basic_conda_meta() {
        let content = r#"{
  "name": "requests",
  "version": "2.32.3",
  "license": "Apache-2.0",
  "url": "https://conda.anaconda.org/conda-forge/noarch/requests-2.32.3-pyhd8ed1ab_0.conda",
  "size": 57820,
  "md5": "5ede4753180c7a550a443c430dc8ab52",
  "sha256": "f9a18bf43e59e60a28e45c94c8d2f9c7e9bb97e2c8bab08cf5d3f3e93bb10a18",
  "requested_spec": "requests",
  "channel": "https://conda.anaconda.org/conda-forge/noarch"
}"#;
        let pkg = parse_conda_meta_json(content);

        assert_eq!(pkg.name.as_deref(), Some("requests"));
        assert_eq!(pkg.version.as_deref(), Some("2.32.3"));
        assert_eq!(
            pkg.extracted_license_statement.as_deref(),
            Some("Apache-2.0")
        );
        assert_eq!(
            pkg.download_url.as_deref(),
            Some(
                "https://conda.anaconda.org/conda-forge/noarch/requests-2.32.3-pyhd8ed1ab_0.conda"
            )
        );
        assert_eq!(pkg.size, Some(57820));
        assert_eq!(
            pkg.md5,
            Some(Md5Digest::from_hex("5ede4753180c7a550a443c430dc8ab52").unwrap())
        );
        assert_eq!(
            pkg.sha256,
            Some(
                Sha256Digest::from_hex(
                    "f9a18bf43e59e60a28e45c94c8d2f9c7e9bb97e2c8bab08cf5d3f3e93bb10a18"
                )
                .unwrap()
            )
        );
        assert_eq!(pkg.package_type, Some(PackageType::Conda));
        assert_eq!(pkg.primary_language.as_deref(), Some("Python"));
        assert_eq!(pkg.datasource_id, Some(DatasourceId::CondaMetaJson));

        // Check extra_data
        assert!(pkg.extra_data.is_some());
        let extra = pkg.extra_data.as_ref().unwrap();
        assert_eq!(
            extra.get("requested_spec").and_then(|v| v.as_str()),
            Some("requests")
        );
        assert_eq!(
            extra.get("channel").and_then(|v| v.as_str()),
            Some("https://conda.anaconda.org/conda-forge/noarch")
        );
    }

    #[test]
    fn test_parse_with_files() {
        let content = r#"{
  "name": "python",
  "version": "3.12.0",
  "extracted_package_dir": "/opt/conda/pkgs/python-3.12.0-h06a4308_1",
  "files": [
    "bin/python",
    "bin/python3",
    "lib/libpython3.12.so"
  ],
  "package_tarball_full_path": "/opt/conda/pkgs/python-3.12.0-h06a4308_1.tar.bz2"
}"#;
        let pkg = parse_conda_meta_json(content);

        assert_eq!(pkg.name.as_deref(), Some("python"));
        assert_eq!(pkg.version.as_deref(), Some("3.12.0"));

        // Check extra_data has file fields
        assert!(pkg.extra_data.is_some());
        let extra = pkg.extra_data.as_ref().unwrap();
        assert_eq!(
            extra.get("extracted_package_dir").and_then(|v| v.as_str()),
            Some("/opt/conda/pkgs/python-3.12.0-h06a4308_1")
        );
        assert_eq!(
            extra
                .get("package_tarball_full_path")
                .and_then(|v| v.as_str()),
            Some("/opt/conda/pkgs/python-3.12.0-h06a4308_1.tar.bz2")
        );

        let files = extra.get("files").and_then(|v| v.as_array());
        assert!(files.is_some());
        assert_eq!(files.unwrap().len(), 3);
        assert_eq!(pkg.file_references.len(), 5);
    }

    #[test]
    fn test_parse_tzdata_fixture_and_preserve_channel_url() {
        let path = PathBuf::from("testdata/conda/conda-meta/tzdata-2024b-h04d1e81_0.json");
        let pkg = CondaMetaJsonParser::extract_first_package(&path);

        assert_eq!(pkg.name.as_deref(), Some("tzdata"));
        assert_eq!(pkg.version.as_deref(), Some("2024b"));
        assert_eq!(pkg.datasource_id, Some(DatasourceId::CondaMetaJson));
        assert_eq!(pkg.purl.as_deref(), Some("pkg:conda/tzdata@2024b"));
        let extra = pkg.extra_data.as_ref().unwrap();
        assert_eq!(
            extra.get("channel").and_then(|v| v.as_str()),
            Some("https://repo.anaconda.com/pkgs/main/noarch")
        );
        assert!(
            pkg.file_references
                .iter()
                .any(|r| r.path.contains("share/zoneinfo/zone.tab"))
        );
    }

    #[test]
    fn test_parse_minimal() {
        let content = r#"{
  "name": "package",
  "version": "1.0.0"
}"#;
        let pkg = parse_conda_meta_json(content);

        assert_eq!(pkg.name.as_deref(), Some("package"));
        assert_eq!(pkg.version.as_deref(), Some("1.0.0"));
        assert_eq!(pkg.extracted_license_statement, None);
        assert_eq!(pkg.download_url, None);
        // extra_data should be None when no optional fields present
        assert!(pkg.extra_data.is_none() || pkg.extra_data.as_ref().unwrap().is_empty());
    }

    #[test]
    fn test_parse_invalid_json() {
        let content = "this is not valid json {{{";
        let pkg = parse_conda_meta_json(content);

        // Should return default package on parse error
        assert_eq!(pkg.package_type, Some(PackageType::Conda));
        assert_eq!(pkg.datasource_id, Some(DatasourceId::CondaMetaJson));
    }
}
