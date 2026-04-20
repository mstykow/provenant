#[cfg(test)]
mod tests {
    use super::super::PackageParser;
    use super::super::conan_data::*;
    use crate::models::DatasourceId;
    use crate::models::PackageType;
    use crate::models::Sha256Digest;
    use std::path::PathBuf;

    #[test]
    fn test_is_match() {
        assert!(ConanDataParser::is_match(&PathBuf::from(
            "/path/to/conandata.yml"
        )));
        assert!(ConanDataParser::is_match(&PathBuf::from(
            "some/dir/conandata.yml"
        )));
        assert!(!ConanDataParser::is_match(&PathBuf::from("conanfile.py")));
        assert!(!ConanDataParser::is_match(&PathBuf::from("conandata.yaml")));
        assert!(!ConanDataParser::is_match(&PathBuf::from("package.json")));
    }

    #[test]
    fn test_parse_basic_conandata() {
        let content = r#"
sources:
  "1.0.0":
    url: "https://example.com/package-1.0.0.tar.gz"
    sha256: "abc1230000000000000000000000000000000000000000000000000000000000"
  "2.0.0":
    url: "https://example.com/package-2.0.0.tar.gz"
    sha256: "def4560000000000000000000000000000000000000000000000000000000000"
"#;
        let packages = parse_conandata_yml(content);
        assert_eq!(packages.len(), 2);

        // Check first package
        let pkg1 = packages
            .iter()
            .find(|p| p.version.as_deref() == Some("1.0.0"));
        assert!(pkg1.is_some());
        let pkg1 = pkg1.unwrap();
        assert_eq!(pkg1.package_type, Some(PackageType::Conan));
        assert_eq!(pkg1.primary_language.as_deref(), Some("C++"));
        assert_eq!(
            pkg1.download_url.as_deref(),
            Some("https://example.com/package-1.0.0.tar.gz")
        );
        assert_eq!(
            pkg1.sha256,
            Some(
                Sha256Digest::from_hex(
                    "abc1230000000000000000000000000000000000000000000000000000000000"
                )
                .unwrap()
            )
        );
        assert_eq!(pkg1.datasource_id, Some(DatasourceId::ConanConanDataYml));

        // Check second package
        let pkg2 = packages
            .iter()
            .find(|p| p.version.as_deref() == Some("2.0.0"));
        assert!(pkg2.is_some());
        let pkg2 = pkg2.unwrap();
        assert_eq!(
            pkg2.download_url.as_deref(),
            Some("https://example.com/package-2.0.0.tar.gz")
        );
        assert_eq!(
            pkg2.sha256,
            Some(
                Sha256Digest::from_hex(
                    "def4560000000000000000000000000000000000000000000000000000000000"
                )
                .unwrap()
            )
        );
    }

    #[test]
    fn test_parse_multiple_urls() {
        let content = r#"
sources:
  "1.5.0":
    url:
      - "https://mirror1.com/package-1.5.0.tar.gz"
      - "https://mirror2.com/package-1.5.0.tar.gz"
    sha256: "abc7890000000000000000000000000000000000000000000000000000000000"
"#;
        let packages = parse_conandata_yml(content);
        assert_eq!(packages.len(), 1);

        let pkg = &packages[0];
        // Should use first URL from list
        assert_eq!(
            pkg.download_url.as_deref(),
            Some("https://mirror1.com/package-1.5.0.tar.gz")
        );
        assert_eq!(
            pkg.sha256,
            Some(
                Sha256Digest::from_hex(
                    "abc7890000000000000000000000000000000000000000000000000000000000"
                )
                .unwrap()
            )
        );
    }

    #[test]
    fn test_parse_multiple_source_entries_per_version() {
        let content = r#"
sources:
  "3.6":
    - url: "https://example.com/libselinux-3.6.tar.gz"
      sha256: "abc1230000000000000000000000000000000000000000000000000000000000"
    - url: "https://example.com/libsepol-3.6.tar.gz"
      sha256: "def4560000000000000000000000000000000000000000000000000000000000"
patches:
  "3.6":
    - patch_file: "patches/fix.patch"
      patch_description: "Fix packaging"
      patch_type: "portability"
"#;

        let packages = parse_conandata_yml(content);
        assert_eq!(packages.len(), 1);

        let pkg = &packages[0];
        assert_eq!(pkg.version.as_deref(), Some("3.6"));
        assert_eq!(
            pkg.download_url.as_deref(),
            Some("https://example.com/libselinux-3.6.tar.gz")
        );
        assert_eq!(
            pkg.sha256,
            Some(
                Sha256Digest::from_hex(
                    "abc1230000000000000000000000000000000000000000000000000000000000"
                )
                .unwrap()
            )
        );

        let extra = pkg.extra_data.as_ref().expect("extra_data should exist");
        let additional_sources = extra
            .get("additional_sources")
            .and_then(|value| value.as_array())
            .expect("additional_sources should be preserved");
        assert_eq!(additional_sources.len(), 1);
        assert_eq!(
            additional_sources[0]
                .get("url")
                .and_then(|value| value.as_str()),
            Some("https://example.com/libsepol-3.6.tar.gz")
        );
        assert_eq!(
            additional_sources[0]
                .get("sha256")
                .and_then(|value| value.as_str()),
            Some("def4560000000000000000000000000000000000000000000000000000000000")
        );
        assert!(extra.contains_key("patches"));
    }

    #[test]
    fn test_parse_missing_fields() {
        let content = r#"
sources:
  "3.0.0":
    url: "https://example.com/package-3.0.0.tar.gz"
  "4.0.0":
    sha256: "onlyhash"
"#;
        let packages = parse_conandata_yml(content);
        assert_eq!(packages.len(), 2);

        let pkg1 = packages
            .iter()
            .find(|p| p.version.as_deref() == Some("3.0.0"));
        assert!(pkg1.is_some());
        let pkg1 = pkg1.unwrap();
        assert_eq!(
            pkg1.download_url.as_deref(),
            Some("https://example.com/package-3.0.0.tar.gz")
        );
        assert_eq!(pkg1.sha256, None);

        let pkg2 = packages
            .iter()
            .find(|p| p.version.as_deref() == Some("4.0.0"));
        assert!(pkg2.is_some());
        let pkg2 = pkg2.unwrap();
        assert_eq!(pkg2.download_url, None);
        assert_eq!(pkg2.sha256, None);
    }

    #[test]
    fn test_parse_empty_sources() {
        let content = r#"
sources: {}
"#;
        let packages = parse_conandata_yml(content);
        // Should return default package when sources is empty
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].package_type, Some(PackageType::Conan));
    }

    #[test]
    fn test_parse_invalid_yaml() {
        let content = "this is not valid yaml: [[[";
        let packages = parse_conandata_yml(content);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].package_type, Some(PackageType::Conan));
    }

    #[test]
    fn test_parse_with_patches() {
        let content = r#"
sources:
  "1.12.0":
    url: "https://github.com/libcpr/cpr/archive/refs/tags/1.12.0.tar.gz"
    sha256: "f64b501de66e163d6a278fbb6a95f395ee873b7a66c905dd785eae107266a709"
patches:
  "1.12.0":
    - patch_file: "patches/008-1.12.0-remove-warning-flags.patch"
      patch_description: "disable warning flags and warning as error"
      patch_type: "portability"
    - patch_file: "patches/009-1.12.0-windows-msvc-runtime.patch"
      patch_description: "dont hardcode value of CMAKE_MSVC_RUNTIME_LIBRARY"
      patch_type: "conan"
"#;
        let packages = parse_conandata_yml(content);
        assert_eq!(packages.len(), 1);

        let pkg = &packages[0];
        assert_eq!(pkg.version.as_deref(), Some("1.12.0"));
        assert_eq!(
            pkg.download_url.as_deref(),
            Some("https://github.com/libcpr/cpr/archive/refs/tags/1.12.0.tar.gz")
        );

        assert!(pkg.extra_data.is_some());
        let extra = pkg.extra_data.as_ref().unwrap();
        assert!(extra.contains_key("patches"));

        let patches = extra.get("patches").unwrap();
        assert!(patches.is_array());
        let patches_array = patches.as_array().unwrap();
        assert_eq!(patches_array.len(), 2);

        let patch1 = &patches_array[0];
        assert_eq!(
            patch1.get("patch_file").and_then(|v| v.as_str()),
            Some("patches/008-1.12.0-remove-warning-flags.patch")
        );
        assert_eq!(
            patch1.get("patch_description").and_then(|v| v.as_str()),
            Some("disable warning flags and warning as error")
        );
        assert_eq!(
            patch1.get("patch_type").and_then(|v| v.as_str()),
            Some("portability")
        );
    }

    #[test]
    fn test_parse_with_mirror_urls() {
        let content = r#"
sources:
  "1.0.0":
    url:
      - "https://mirror1.com/package.tar.gz"
      - "https://mirror2.com/package.tar.gz"
      - "https://mirror3.com/package.tar.gz"
    sha256: "abc123"
"#;
        let packages = parse_conandata_yml(content);
        assert_eq!(packages.len(), 1);

        let pkg = &packages[0];
        assert_eq!(
            pkg.download_url.as_deref(),
            Some("https://mirror1.com/package.tar.gz")
        );

        assert!(pkg.extra_data.is_some());
        let extra = pkg.extra_data.as_ref().unwrap();
        assert!(extra.contains_key("mirror_urls"));

        let mirrors = extra.get("mirror_urls").unwrap();
        assert!(mirrors.is_array());
        let mirrors_array = mirrors.as_array().unwrap();
        assert_eq!(mirrors_array.len(), 3);
    }

    #[test]
    fn test_real_boost_fixture_preserves_all_source_urls() {
        let path = PathBuf::from("testdata/conan/recipes/boost/manifest/conandata.yml");
        let packages = ConanDataParser::extract_packages(&path);

        let pkg = packages
            .iter()
            .find(|package| package.version.as_deref() == Some("1.84.0"))
            .expect("boost 1.84.0 package should exist");

        assert_eq!(
            pkg.download_url.as_deref(),
            Some(
                "https://boostorg.jfrog.io/artifactory/main/release/1.84.0/source/boost_1_84_0.tar.bz2"
            )
        );

        let mirrors = pkg
            .extra_data
            .as_ref()
            .and_then(|extra| extra.get("mirror_urls"))
            .and_then(|value| value.as_array())
            .expect("boost mirror_urls should exist");

        assert_eq!(mirrors.len(), 2);
        assert_eq!(
            mirrors[0].as_str(),
            Some(
                "https://boostorg.jfrog.io/artifactory/main/release/1.84.0/source/boost_1_84_0.tar.bz2"
            )
        );
        assert_eq!(
            mirrors[1].as_str(),
            Some("https://sourceforge.net/projects/boost/files/boost/1.84.0/boost_1_84_0.tar.bz2")
        );
    }

    #[test]
    fn test_real_libzip_fixture_preserves_all_source_urls() {
        let path = PathBuf::from("testdata/conan/recipes/libzip/manifest/conandata.yml");
        let packages = ConanDataParser::extract_packages(&path);

        let pkg = packages
            .iter()
            .find(|package| package.version.as_deref() == Some("1.10.1"))
            .expect("libzip 1.10.1 package should exist");

        assert_eq!(
            pkg.download_url.as_deref(),
            Some("https://libzip.org/download/libzip-1.10.1.tar.gz")
        );

        let mirrors = pkg
            .extra_data
            .as_ref()
            .and_then(|extra| extra.get("mirror_urls"))
            .and_then(|value| value.as_array())
            .expect("libzip mirror_urls should exist");

        assert_eq!(mirrors.len(), 2);
        assert_eq!(
            mirrors[0].as_str(),
            Some("https://libzip.org/download/libzip-1.10.1.tar.gz")
        );
        assert_eq!(
            mirrors[1].as_str(),
            Some("https://github.com/nih-at/libzip/releases/download/v1.10.1/libzip-1.10.1.tar.gz")
        );
    }

    #[test]
    fn test_parse_patches_without_matching_source() {
        let content = r#"
sources:
  "1.0.0":
    url: "https://example.com/package.tar.gz"
patches:
  "2.0.0":
    - patch_file: "some.patch"
"#;
        let packages = parse_conandata_yml(content);
        assert_eq!(packages.len(), 1);

        let pkg = &packages[0];
        assert_eq!(pkg.version.as_deref(), Some("1.0.0"));
        assert!(
            pkg.extra_data.is_none() || !pkg.extra_data.as_ref().unwrap().contains_key("patches")
        );
    }
}
