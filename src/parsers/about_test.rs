// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use crate::models::DatasourceId;
    use crate::models::PackageType;
    use std::path::PathBuf;

    use crate::parsers::PackageParser;
    use crate::parsers::about::AboutFileParser;

    #[test]
    fn test_is_match() {
        // Should match uppercase .ABOUT extension
        assert!(AboutFileParser::is_match(&PathBuf::from(
            "testdata/about/apipkg.ABOUT"
        )));
        assert!(AboutFileParser::is_match(&PathBuf::from(
            "/path/to/file.ABOUT"
        )));

        // Should NOT match lowercase .about
        assert!(!AboutFileParser::is_match(&PathBuf::from(
            "testdata/about/file.about"
        )));

        // Should NOT match other extensions
        assert!(!AboutFileParser::is_match(&PathBuf::from(
            "testdata/about/file.txt"
        )));
        assert!(!AboutFileParser::is_match(&PathBuf::from(
            "testdata/about/package.json"
        )));
        assert!(!AboutFileParser::is_match(&PathBuf::from(
            "testdata/about/README.md"
        )));
    }

    #[test]
    fn test_basic_extraction() {
        let path = PathBuf::from("testdata/about/appdirs.ABOUT");
        let result = AboutFileParser::extract_first_package(&path);

        assert_eq!(result.package_type, Some(PackageType::Pypi));
        assert_eq!(result.name, Some("appdirs".to_string()));
        assert_eq!(result.version, Some("1.4.3".to_string()));
        assert_eq!(
            result.homepage_url,
            Some("https://pypi.python.org/pypi/appdirs".to_string())
        );
        assert_eq!(
            result.copyright,
            Some("Copyright (c) 2010 ActiveState Software Inc.".to_string())
        );
        assert_eq!(result.extracted_license_statement, Some("mit".to_string()));
        assert_eq!(result.declared_license_expression, Some("mit".to_string()));
        assert_eq!(
            result.declared_license_expression_spdx,
            Some("MIT".to_string())
        );
        assert_eq!(result.license_detections.len(), 1);
        assert_eq!(
            result.download_url,
            Some("https://pypi.python.org/packages/56/eb/810e700ed1349edde4cbdc1b2a21e28cdf115f9faf263f6bbf8447c1abf3/appdirs-1.4.3-py2.py3-none-any.whl#md5=9ed4b51c9611775c3078b3831072e153".to_string())
        );
        assert_eq!(result.datasource_id, Some(DatasourceId::AboutFile));
        assert_eq!(result.purl, Some("pkg:pypi/appdirs@1.4.3".to_string()));
        assert_eq!(
            result.vcs_url,
            Some("https://github.com/ActiveState/appdirs.git".to_string())
        );
    }

    #[test]
    fn test_owner_party() {
        let path = PathBuf::from("testdata/about/appdirs.ABOUT");
        let result = AboutFileParser::extract_first_package(&path);

        assert_eq!(result.parties.len(), 1);
        let party = &result.parties[0];
        assert_eq!(party.r#type, Some("person".to_string()));
        assert_eq!(party.role, Some("owner".to_string()));
        assert_eq!(party.name, Some("ActiveState".to_string()));
    }

    #[test]
    fn test_file_references() {
        let path = PathBuf::from("testdata/about/appdirs.ABOUT");
        let result = AboutFileParser::extract_first_package(&path);

        assert_eq!(result.file_references.len(), 2);
        assert_eq!(
            result.file_references[0].path,
            "appdirs-1.4.3-py2.py3-none-any.whl"
        );
        assert_eq!(result.file_references[1].path, "appdirs.LICENSE");
    }

    #[test]
    fn test_missing_fields() {
        // Create a minimal ABOUT file
        let test_content = r#"
name: test-package
version: 1.0.0
"#;
        use std::io::Write;
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("minimal.ABOUT");
        let mut file = std::fs::File::create(&file_path).unwrap();
        file.write_all(test_content.as_bytes()).unwrap();

        let result = AboutFileParser::extract_first_package(&file_path);

        assert_eq!(result.package_type, Some(PackageType::About));
        assert_eq!(result.datasource_id, Some(DatasourceId::AboutFile));
        assert_eq!(result.name, Some("test-package".to_string()));
        assert_eq!(result.version, Some("1.0.0".to_string()));
        assert_eq!(result.homepage_url, None);
        assert_eq!(result.copyright, None);
        assert_eq!(result.extracted_license_statement, None);
        assert_eq!(result.parties.len(), 0);
        assert_eq!(result.file_references.len(), 0);
    }

    #[test]
    fn test_purl_type_override() {
        // Create an ABOUT file with purl
        let test_content = r#"
name: django
version: 3.2.0
purl: pkg:pypi/django@3.2.0
"#;
        use std::io::Write;
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("purl.ABOUT");
        let mut file = std::fs::File::create(&file_path).unwrap();
        file.write_all(test_content.as_bytes()).unwrap();

        let result = AboutFileParser::extract_first_package(&file_path);

        // Type should be extracted from purl
        assert_eq!(result.package_type, Some(PackageType::Pypi));
        assert_eq!(result.name, Some("django".to_string()));
        assert_eq!(result.version, Some("3.2.0".to_string()));
        assert_eq!(result.purl, Some("pkg:pypi/django@3.2.0".to_string()));
    }

    #[test]
    fn test_explicit_type_override() {
        // Create an ABOUT file with explicit type field
        let test_content = r#"
type: custom-type
name: mypackage
version: 2.0.0
"#;
        use std::io::Write;
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("typed.ABOUT");
        let mut file = std::fs::File::create(&file_path).unwrap();
        file.write_all(test_content.as_bytes()).unwrap();

        let result = AboutFileParser::extract_first_package(&file_path);

        // Unknown type falls back to default "about" since "custom-type" is not a valid PackageType
        assert_eq!(result.package_type, Some(PackageType::About));
        assert_eq!(result.name, Some("mypackage".to_string()));
        assert_eq!(result.version, Some("2.0.0".to_string()));
    }

    #[test]
    fn test_purl_with_namespace() {
        // Create an ABOUT file with namespaced purl
        let test_content = r#"
purl: pkg:npm/%40babel/core@7.0.0
"#;
        use std::io::Write;
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("namespaced.ABOUT");
        let mut file = std::fs::File::create(&file_path).unwrap();
        file.write_all(test_content.as_bytes()).unwrap();

        let result = AboutFileParser::extract_first_package(&file_path);

        // Should extract namespace from purl
        assert_eq!(result.package_type, Some(PackageType::Npm));
        assert_eq!(result.namespace, Some("@babel".to_string()));
        assert_eq!(result.name, Some("core".to_string()));
        assert_eq!(result.version, Some("7.0.0".to_string()));
    }

    #[test]
    fn test_home_url_vs_homepage_url() {
        // Test home_url field
        let test_content1 = r#"
name: pkg1
home_url: https://example.com/home
"#;
        use std::io::Write;
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path1 = temp_dir.path().join("home1.ABOUT");
        let mut file1 = std::fs::File::create(&file_path1).unwrap();
        file1.write_all(test_content1.as_bytes()).unwrap();

        let result1 = AboutFileParser::extract_first_package(&file_path1);
        assert_eq!(
            result1.homepage_url,
            Some("https://example.com/home".to_string())
        );

        // Test homepage_url field
        let test_content2 = r#"
name: pkg2
homepage_url: https://example.com/homepage
"#;
        let file_path2 = temp_dir.path().join("home2.ABOUT");
        let mut file2 = std::fs::File::create(&file_path2).unwrap();
        file2.write_all(test_content2.as_bytes()).unwrap();

        let result2 = AboutFileParser::extract_first_package(&file_path2);
        assert_eq!(
            result2.homepage_url,
            Some("https://example.com/homepage".to_string())
        );
    }

    #[test]
    fn test_apipkg_about() {
        let path = PathBuf::from("testdata/about/apipkg.ABOUT");
        let result = AboutFileParser::extract_first_package(&path);

        assert_eq!(result.package_type, Some(PackageType::Pypi));
        assert_eq!(result.name, Some("apipkg".to_string()));
        assert_eq!(result.version, Some("1.4".to_string()));
        assert_eq!(
            result.homepage_url,
            Some("https://bitbucket.org/hpk42/apipkg".to_string())
        );
        assert_eq!(
            result.copyright,
            Some("Copyright (c) 2009 holger krekel".to_string())
        );
        assert_eq!(result.extracted_license_statement, Some("mit".to_string()));

        // Owner party
        assert_eq!(result.parties.len(), 1);
        assert_eq!(result.parties[0].name, Some("Holger Krekel".to_string()));

        // File reference
        assert_eq!(result.file_references.len(), 2);
        assert_eq!(
            result.file_references[0].path,
            "apipkg-1.4-py2.py3-none-any.whl"
        );
        assert_eq!(result.file_references[1].path, "apipkg.LICENSE");
        assert_eq!(result.file_references[1].path, "apipkg.LICENSE");
        assert_eq!(result.purl, Some("pkg:pypi/apipkg@1.4".to_string()));
        let referenced_filenames = result.license_detections[0].matches[0]
            .referenced_filenames
            .as_ref()
            .expect("referenced filenames should be present");
        assert_eq!(referenced_filenames, &vec!["apipkg.LICENSE".to_string()]);
    }

    #[test]
    fn test_download_url_infers_github_purl_without_reporting_pkg_about() {
        let test_content = r#"
download_url: https://raw.githubusercontent.com/docker/docker/ff2de8dace1ba1c1f5e8542790ef5cd564375934/image/spec/v1.1.md
"#;
        use std::io::Write;
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("docker.ABOUT");
        let mut file = std::fs::File::create(&file_path).unwrap();
        file.write_all(test_content.as_bytes()).unwrap();

        let result = AboutFileParser::extract_first_package(&file_path);

        assert_eq!(result.package_type, Some(PackageType::Github));
        assert_eq!(result.namespace, Some("docker".to_string()));
        assert_eq!(result.name, Some("docker".to_string()));
        assert_eq!(result.purl, Some("pkg:github/docker/docker".to_string()));
    }

    #[test]
    fn test_partial_about_file_is_graceful_with_datasource() {
        let test_content = r#"
download_url: https://raw.githubusercontent.com/docker/docker/ff2de8dace1ba1c1f5e8542790ef5cd564375934/image/spec/v1.1.md
"#;
        use std::io::Write;
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("partial.ABOUT");
        let mut file = std::fs::File::create(&file_path).unwrap();
        file.write_all(test_content.as_bytes()).unwrap();

        let result = AboutFileParser::extract_first_package(&file_path);

        assert_eq!(result.datasource_id, Some(DatasourceId::AboutFile));
        assert_eq!(result.package_type, Some(PackageType::Github));
        assert_eq!(result.name, Some("docker".to_string()));
        assert_eq!(result.purl, Some("pkg:github/docker/docker".to_string()));
    }

    #[test]
    fn test_invalid_yaml_returns_about_defaults() {
        use std::io::Write;
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("broken.ABOUT");
        let mut file = std::fs::File::create(&file_path).unwrap();
        file.write_all(b"::not yaml::\n[").unwrap();

        let result = AboutFileParser::extract_first_package(&file_path);

        assert_eq!(result.package_type, Some(PackageType::About));
        assert_eq!(result.datasource_id, Some(DatasourceId::AboutFile));
        assert_eq!(result.purl, None);
    }

    #[test]
    fn test_extra_data_preserves_notice_and_notes() {
        let path =
            PathBuf::from("testdata/copyright-golden/copyrights/misco2/regexhq/regexhq.ABOUT");
        let result = AboutFileParser::extract_first_package(&path);

        let extra = result.extra_data.expect("extra data should exist");
        assert!(extra.contains_key("notice_file"));
        assert!(extra.contains_key("notes"));
    }
}
