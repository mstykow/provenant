// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use crate::models::DatasourceId;
    use crate::models::PackageType;
    use crate::parsers::PackageParser;
    use crate::parsers::readme::ReadmeParser;
    use std::path::PathBuf;

    #[test]
    fn test_is_match_all_variants() {
        let android = PathBuf::from("/path/README.android");
        let chromium = PathBuf::from("/path/README.chromium");
        let facebook = PathBuf::from("/path/README.facebook");
        let google = PathBuf::from("/path/README.google");
        let thirdparty = PathBuf::from("/path/README.thirdparty");

        assert!(ReadmeParser::is_match(&android));
        assert!(ReadmeParser::is_match(&chromium));
        assert!(ReadmeParser::is_match(&facebook));
        assert!(ReadmeParser::is_match(&google));
        assert!(ReadmeParser::is_match(&thirdparty));
    }

    #[test]
    fn test_is_match_case_insensitive() {
        let upper = PathBuf::from("/path/README.ANDROID");
        let mixed = PathBuf::from("/path/README.ChRoMiUm");
        let lower = PathBuf::from("/path/README.facebook");

        assert!(ReadmeParser::is_match(&upper));
        assert!(ReadmeParser::is_match(&mixed));
        assert!(ReadmeParser::is_match(&lower));
    }

    #[test]
    fn test_is_match_negative_cases() {
        let readme = PathBuf::from("/path/README");
        let readme_md = PathBuf::from("/path/README.md");
        let readme_txt = PathBuf::from("/path/README.txt");
        let readme_rst = PathBuf::from("/path/README.rst");
        let other = PathBuf::from("/path/INSTALL.txt");

        assert!(!ReadmeParser::is_match(&readme));
        assert!(!ReadmeParser::is_match(&readme_md));
        assert!(!ReadmeParser::is_match(&readme_txt));
        assert!(!ReadmeParser::is_match(&readme_rst));
        assert!(!ReadmeParser::is_match(&other));
    }

    #[test]
    fn test_extract_chromium_colon_separator() {
        let path = PathBuf::from("testdata/readme/chromium/third_party/example/README.chromium");
        let pkg = ReadmeParser::extract_first_package(&path);

        assert_eq!(pkg.package_type, Some(PackageType::Readme));
        assert_eq!(pkg.name, Some("Example Library".to_string()));
        assert_eq!(pkg.version, Some("2.1.0".to_string()));
        assert_eq!(pkg.homepage_url, Some("https://example.com".to_string()));
        assert_eq!(pkg.extracted_license_statement, Some("MIT".to_string()));
        assert_eq!(pkg.datasource_id, Some(DatasourceId::Readme));
    }

    #[test]
    fn test_extract_android_homepage_field() {
        let path = PathBuf::from("testdata/readme/android/third_party/example/README.android");
        let pkg = ReadmeParser::extract_first_package(&path);

        assert_eq!(pkg.name, Some("Android Example".to_string()));
        assert_eq!(pkg.version, Some("1.0".to_string()));
        assert_eq!(
            pkg.homepage_url,
            Some("https://android.example.com".to_string())
        );
        assert_eq!(pkg.copyright, Some("2024 Google Inc.".to_string()));
    }

    #[test]
    fn test_extract_facebook_downloaded_from() {
        let path = PathBuf::from("testdata/readme/facebook/third_party/example/README.facebook");
        let pkg = ReadmeParser::extract_first_package(&path);

        assert_eq!(pkg.name, Some("FB Library".to_string()));
        assert_eq!(
            pkg.download_url,
            Some("https://github.com/example/fb-lib".to_string())
        );
        assert_eq!(
            pkg.extracted_license_statement,
            Some("BSD-3-Clause".to_string())
        );
    }

    #[test]
    fn test_extract_parent_dir_fallback_no_name() {
        let path = PathBuf::from("testdata/readme/no-name/third_party/mylib/README.thirdparty");
        let pkg = ReadmeParser::extract_first_package(&path);

        // Should use parent directory name "mylib" as fallback
        assert_eq!(pkg.name, Some("mylib".to_string()));
        assert_eq!(pkg.homepage_url, Some("https://example.com".to_string()));
        assert_eq!(pkg.version, Some("3.0".to_string()));
    }

    #[test]
    fn test_extract_equals_separator() {
        let path =
            PathBuf::from("testdata/readme/equals-separator/third_party/eqlib/README.google");
        let pkg = ReadmeParser::extract_first_package(&path);

        assert_eq!(pkg.name, Some("Google Lib".to_string()));
        assert_eq!(
            pkg.homepage_url,
            Some("https://google.example.com".to_string())
        );
        assert_eq!(
            pkg.extracted_license_statement,
            Some("Apache-2.0".to_string())
        );
    }

    #[test]
    fn test_field_mapping_all_homepage_aliases() {
        // Test that all homepage URL aliases work
        let test_cases = vec![
            ("homepage", "https://example.com"),
            ("website", "https://example.com"),
            ("repo", "https://example.com"),
            ("source", "https://example.com"),
            ("upstream", "https://example.com"),
            ("url", "https://example.com"),
            ("project url", "https://example.com"),
        ];

        for (field, value) in test_cases {
            let content = format!("{}: {}", field, value);
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("README.chromium");
            std::fs::write(&path, content).unwrap();

            let pkg = ReadmeParser::extract_first_package(&path);
            assert_eq!(
                pkg.homepage_url,
                Some(value.to_string()),
                "Field '{}' should map to homepage_url",
                field
            );
        }
    }

    #[test]
    fn test_field_mapping_name_aliases() {
        // Test both "name" and "project" map to name
        let test_cases = vec![
            ("name", "Test Package"),
            ("project", "Test Package"),
            ("Name", "Test Package"), // Case insensitive
            ("PROJECT", "Test Package"),
        ];

        for (field, value) in test_cases {
            let content = format!("{}: {}", field, value);
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("README.chromium");
            std::fs::write(&path, content).unwrap();

            let pkg = ReadmeParser::extract_first_package(&path);
            assert_eq!(
                pkg.name,
                Some(value.to_string()),
                "Field '{}' should map to name",
                field
            );
        }
    }

    #[test]
    fn test_field_mapping_download_url_aliases() {
        let test_cases = vec![
            ("download link", "https://example.com/dl"),
            ("downloaded from", "https://example.com/dl"),
        ];

        for (field, value) in test_cases {
            let content = format!("{}: {}", field, value);
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("README.chromium");
            std::fs::write(&path, content).unwrap();

            let pkg = ReadmeParser::extract_first_package(&path);
            assert_eq!(
                pkg.download_url,
                Some(value.to_string()),
                "Field '{}' should map to download_url",
                field
            );
        }
    }

    #[test]
    fn test_field_mapping_license_aliases() {
        let test_cases = vec![("license", "MIT"), ("licence", "MIT")];

        for (field, value) in test_cases {
            let content = format!("{}: {}", field, value);
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("README.chromium");
            std::fs::write(&path, content).unwrap();

            let pkg = ReadmeParser::extract_first_package(&path);
            assert_eq!(
                pkg.extracted_license_statement,
                Some(value.to_string()),
                "Field '{}' should map to extracted_license_statement",
                field
            );
        }
    }

    #[test]
    fn test_skip_malformed_lines() {
        let content = "Name: Valid Package\n\
                       This line has no separator\n\
                       Version: 1.0\n\
                       : no key\n\
                       key with no value:\n\
                       = no key equals\n\
                       License: MIT";

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("README.chromium");
        std::fs::write(&path, content).unwrap();

        let pkg = ReadmeParser::extract_first_package(&path);

        // Only valid lines should be parsed
        assert_eq!(pkg.name, Some("Valid Package".to_string()));
        assert_eq!(pkg.version, Some("1.0".to_string()));
        assert_eq!(pkg.extracted_license_statement, Some("MIT".to_string()));
    }

    #[test]
    fn test_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("testdir").join("README.chromium");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "").unwrap();

        let pkg = ReadmeParser::extract_first_package(&path);

        // Should use parent dir name as fallback
        assert_eq!(pkg.name, Some("testdir".to_string()));
        assert_eq!(pkg.package_type, Some(PackageType::Readme));
        assert_eq!(pkg.datasource_id, Some(DatasourceId::Readme));
    }

    #[test]
    fn test_whitespace_handling() {
        let content = "  Name  :  Test Package  \n\
                       Version:1.0\n\
                       URL   =   https://example.com  ";

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("README.chromium");
        std::fs::write(&path, content).unwrap();

        let pkg = ReadmeParser::extract_first_package(&path);

        // Whitespace should be trimmed
        assert_eq!(pkg.name, Some("Test Package".to_string()));
        assert_eq!(pkg.version, Some("1.0".to_string()));
        assert_eq!(pkg.homepage_url, Some("https://example.com".to_string()));
    }

    #[test]
    fn test_nonexistent_file() {
        let path = PathBuf::from("testdata/readme/nonexistent/README.chromium");
        let pkg = ReadmeParser::extract_first_package(&path);

        // Should return default data with proper type and datasource
        assert_eq!(pkg.package_type, Some(PackageType::Readme));
        assert_eq!(pkg.datasource_id, Some(DatasourceId::Readme));
        assert!(pkg.name.is_none());
    }

    #[test]
    fn test_case_insensitive_field_matching() {
        let content = "NAME: Test\n\
                       VERSION: 1.0\n\
                       CoPyRiGhT: 2024\n\
                       HOMEPAGE: https://example.com";

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("README.chromium");
        std::fs::write(&path, content).unwrap();

        let pkg = ReadmeParser::extract_first_package(&path);

        // All fields should be recognized despite case variations
        assert_eq!(pkg.name, Some("Test".to_string()));
        assert_eq!(pkg.version, Some("1.0".to_string()));
        assert_eq!(pkg.copyright, Some("2024".to_string()));
        assert_eq!(pkg.homepage_url, Some("https://example.com".to_string()));
    }
}
