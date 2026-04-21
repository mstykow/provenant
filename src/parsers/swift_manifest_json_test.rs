// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use crate::models::DatasourceId;
    use crate::models::PackageType;
    use crate::parsers::PackageParser;
    use crate::parsers::swift_manifest_json::{SwiftManifestJsonParser, get_namespace_and_name};
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_is_match() {
        assert!(SwiftManifestJsonParser::is_match(&PathBuf::from(
            "/some/path/Package.swift.json"
        )));
        assert!(SwiftManifestJsonParser::is_match(&PathBuf::from(
            "/some/path/Package.swift.deplock"
        )));
        assert!(!SwiftManifestJsonParser::is_match(&PathBuf::from(
            "/some/path/Package.swift"
        )));
        assert!(!SwiftManifestJsonParser::is_match(&PathBuf::from(
            "/some/path/Package.resolved"
        )));
        assert!(!SwiftManifestJsonParser::is_match(&PathBuf::from(
            "/some/path/package.json"
        )));
    }

    #[test]
    fn test_extract_mapbox_manifest() {
        let path = PathBuf::from("testdata/swift/Package.swift.json");
        let data = SwiftManifestJsonParser::extract_first_package(&path);

        assert_eq!(data.package_type, Some(PackageType::Swift));
        assert_eq!(data.name, Some("MapboxMaps".to_string()));
        assert_eq!(data.namespace, None);
        assert_eq!(data.version, None);
        assert_eq!(data.primary_language, Some("Swift".to_string()));
        assert_eq!(
            data.datasource_id,
            Some(DatasourceId::SwiftPackageManifestJson)
        );
        assert_eq!(data.purl, Some("pkg:swift/MapboxMaps".to_string()));

        let extra = data.extra_data.as_ref().expect("extra_data should exist");
        assert!(extra.contains_key("platforms"));
        let platforms = extra.get("platforms").expect("platforms should exist");
        let platforms_arr = platforms.as_array().expect("platforms should be array");
        assert_eq!(platforms_arr.len(), 3);

        assert_eq!(data.dependencies.len(), 3);

        let turf = &data.dependencies[0];
        assert_eq!(
            turf.purl.as_deref(),
            Some("pkg:swift/github.com/mapbox/turf-swift")
        );
        assert_eq!(
            turf.extracted_requirement.as_deref(),
            Some("vers:swift/>=2.8.0|<3.0.0")
        );
        assert_eq!(turf.is_pinned, Some(false));
        assert_eq!(turf.is_runtime, None);
        assert_eq!(turf.is_optional, Some(false));
        assert_eq!(turf.is_direct, Some(true));
        assert_eq!(turf.scope, Some("dependencies".to_string()));
        assert_eq!(
            turf.extra_data
                .as_ref()
                .and_then(|extra| extra.get("requirement_kind"))
                .and_then(|v| v.as_str()),
            Some("range")
        );

        let core_maps = &data.dependencies[1];
        assert_eq!(
            core_maps.purl.as_deref(),
            Some("pkg:swift/github.com/mapbox/mapbox-core-maps-ios@11.4.0-rc.2")
        );
        assert_eq!(
            core_maps.extracted_requirement.as_deref(),
            Some("11.4.0-rc.2")
        );
        assert_eq!(core_maps.is_pinned, Some(true));

        let common = &data.dependencies[2];
        assert_eq!(
            common.purl.as_deref(),
            Some("pkg:swift/github.com/mapbox/mapbox-common-ios@24.4.0-rc.2")
        );
        assert_eq!(common.extracted_requirement.as_deref(), Some("24.4.0-rc.2"));
        assert_eq!(common.is_pinned, Some(true));
    }

    #[test]
    fn test_extract_vercelui_deplock() {
        let path = PathBuf::from("testdata/swift/Package.swift.deplock");
        let data = SwiftManifestJsonParser::extract_first_package(&path);

        assert_eq!(data.package_type, Some(PackageType::Swift));
        assert_eq!(data.name, Some("VercelUI".to_string()));
        assert_eq!(data.purl, Some("pkg:swift/VercelUI".to_string()));

        assert_eq!(data.dependencies.len(), 1);
        let dep = &data.dependencies[0];
        assert_eq!(
            dep.purl.as_deref(),
            Some("pkg:swift/github.com/swift-cloud/Vercel")
        );
        assert_eq!(
            dep.extracted_requirement.as_deref(),
            Some("vers:swift/>=1.15.2|<2.0.0")
        );
        assert_eq!(dep.is_pinned, Some(false));
    }

    #[test]
    fn test_all_requirement_types() {
        let path = PathBuf::from("testdata/swift/Package-all-requirements.swift.json");
        let data = SwiftManifestJsonParser::extract_first_package(&path);

        assert_eq!(data.name, Some("TestPackage".to_string()));
        assert_eq!(data.dependencies.len(), 4);

        let exact_dep = &data.dependencies[0];
        assert_eq!(
            exact_dep.purl.as_deref(),
            Some("pkg:swift/github.com/apple/swift-argument-parser@1.3.0")
        );
        assert_eq!(exact_dep.extracted_requirement.as_deref(), Some("1.3.0"));
        assert_eq!(exact_dep.is_pinned, Some(true));

        let range_dep = &data.dependencies[1];
        assert_eq!(
            range_dep.purl.as_deref(),
            Some("pkg:swift/github.com/apple/swift-nio")
        );
        assert_eq!(
            range_dep.extracted_requirement.as_deref(),
            Some("vers:swift/>=2.0.0|<3.0.0")
        );
        assert_eq!(range_dep.is_pinned, Some(false));

        let branch_dep = &data.dependencies[2];
        assert_eq!(
            branch_dep.purl.as_deref(),
            Some("pkg:swift/github.com/vapor/vapor")
        );
        assert_eq!(branch_dep.extracted_requirement.as_deref(), Some("main"));
        assert_eq!(branch_dep.is_pinned, Some(false));

        let rev_dep = &data.dependencies[3];
        assert_eq!(
            rev_dep.purl.as_deref(),
            Some("pkg:swift/github.com/apple/swift-log@abcdef1234567890")
        );
        assert_eq!(
            rev_dep.extracted_requirement.as_deref(),
            Some("abcdef1234567890")
        );
        assert_eq!(rev_dep.is_pinned, Some(true));
    }

    #[test]
    fn test_tools_version_in_extra_data() {
        let path = PathBuf::from("testdata/swift/Package.swift.json");
        let data = SwiftManifestJsonParser::extract_first_package(&path);

        let extra = data.extra_data.as_ref().expect("extra_data should exist");
        assert_eq!(
            extra.get("swift_tools_version"),
            Some(&serde_json::Value::String("5.9.0".to_string()))
        );
    }

    #[test]
    fn test_get_namespace_and_name_https() {
        let (ns, name) =
            get_namespace_and_name("https://github.com/apple/swift-argument-parser.git");
        assert_eq!(ns, Some("github.com/apple".to_string()));
        assert_eq!(name, "swift-argument-parser");
    }

    #[test]
    fn test_get_namespace_and_name_no_git_suffix() {
        let (ns, name) = get_namespace_and_name("https://github.com/vapor/vapor");
        assert_eq!(ns, Some("github.com/vapor".to_string()));
        assert_eq!(name, "vapor");
    }

    #[test]
    fn test_get_namespace_and_name_trailing_slash() {
        let (ns, name) = get_namespace_and_name("https://github.com/apple/swift-nio/");
        assert_eq!(ns, Some("github.com/apple".to_string()));
        assert_eq!(name, "swift-nio");
    }

    #[test]
    fn test_get_namespace_and_name_deep_path() {
        let (ns, name) = get_namespace_and_name("https://gitlab.com/org/group/repo.git");
        assert_eq!(ns, Some("gitlab.com/org/group".to_string()));
        assert_eq!(name, "repo");
    }

    #[test]
    fn test_empty_dependencies() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let path = temp_dir.path().join("Package.swift.json");
        fs::write(
            &path,
            r#"{"name": "EmptyPkg", "dependencies": [], "toolsVersion": {"_version": "5.5.0"}}"#,
        )
        .expect("Failed to write");

        let data = SwiftManifestJsonParser::extract_first_package(&path);
        assert_eq!(data.name, Some("EmptyPkg".to_string()));
        assert!(data.dependencies.is_empty());
    }

    #[test]
    fn test_no_dependencies_key() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let path = temp_dir.path().join("Package.swift.json");
        fs::write(&path, r#"{"name": "MinimalPkg"}"#).expect("Failed to write");

        let data = SwiftManifestJsonParser::extract_first_package(&path);
        assert_eq!(data.name, Some("MinimalPkg".to_string()));
        assert!(data.dependencies.is_empty());
    }

    #[test]
    fn test_invalid_json_returns_default() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let path = temp_dir.path().join("Package.swift.json");
        fs::write(&path, "not valid json").expect("Failed to write");

        let data = SwiftManifestJsonParser::extract_first_package(&path);
        assert_eq!(data.package_type, Some(PackageType::Swift));
        assert_eq!(
            data.datasource_id,
            Some(DatasourceId::SwiftPackageManifestJson)
        );
        assert_eq!(data.name, None);
    }

    #[test]
    fn test_nonexistent_file_returns_default() {
        let path = PathBuf::from("/nonexistent/Package.swift.json");
        let data = SwiftManifestJsonParser::extract_first_package(&path);
        assert_eq!(data.package_type, Some(PackageType::Swift));
        assert_eq!(
            data.datasource_id,
            Some(DatasourceId::SwiftPackageManifestJson)
        );
        assert_eq!(data.name, None);
    }

    #[test]
    fn test_file_system_dependency_is_preserved() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let path = temp_dir.path().join("Package.swift.json");
        fs::write(
            &path,
            r#"{
                "name": "TestPkg",
                "dependencies": [
                    {"fileSystem": [{"identity": "local-pkg", "path": "/local"}]},
                    {
                        "sourceControl": [{
                            "identity": "real-dep",
                            "location": {"remote": [{"urlString": "https://github.com/org/repo.git"}]},
                            "requirement": {"exact": ["1.0.0"]}
                        }]
                    }
                ]
            }"#,
        )
        .expect("Failed to write");

        let data = SwiftManifestJsonParser::extract_first_package(&path);
        assert_eq!(data.dependencies.len(), 2);
        assert_eq!(
            data.dependencies[0].purl.as_deref(),
            Some("pkg:swift/local-pkg")
        );
        assert_eq!(data.dependencies[0].is_runtime, None);
        assert_eq!(
            data.dependencies[0]
                .extra_data
                .as_ref()
                .and_then(|extra| extra.get("dependency_kind"))
                .and_then(|v| v.as_str()),
            Some("fileSystem")
        );
        assert_eq!(
            data.dependencies[0]
                .extra_data
                .as_ref()
                .and_then(|extra| extra.get("path"))
                .and_then(|v| v.as_str()),
            Some("/local")
        );
        assert_eq!(
            data.dependencies[1].purl.as_deref(),
            Some("pkg:swift/github.com/org/repo@1.0.0")
        );
    }

    #[test]
    fn test_no_remote_url_uses_identity() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let path = temp_dir.path().join("Package.swift.json");
        fs::write(
            &path,
            r#"{
                "name": "TestPkg",
                "dependencies": [{
                    "sourceControl": [{
                        "identity": "local-dependency",
                        "location": {"local": ["/some/path"]},
                        "requirement": {"exact": ["2.0.0"]}
                    }]
                }]
            }"#,
        )
        .expect("Failed to write");

        let data = SwiftManifestJsonParser::extract_first_package(&path);
        assert_eq!(data.dependencies.len(), 1);
        let dep = &data.dependencies[0];
        assert_eq!(
            dep.purl.as_deref(),
            Some("pkg:swift/local-dependency@2.0.0")
        );
        assert_eq!(dep.is_pinned, Some(true));
    }

    #[test]
    fn test_platforms_preserved_in_extra_data() {
        let path = PathBuf::from("testdata/swift/Package.swift.json");
        let data = SwiftManifestJsonParser::extract_first_package(&path);

        let extra = data.extra_data.as_ref().expect("extra_data should exist");
        let platforms = extra
            .get("platforms")
            .and_then(|v| v.as_array())
            .expect("platforms should be array");

        let platform_names: Vec<&str> = platforms
            .iter()
            .filter_map(|p| p.get("platformName").and_then(|v| v.as_str()))
            .collect();
        assert!(platform_names.contains(&"ios"));
        assert!(platform_names.contains(&"macos"));
        assert!(platform_names.contains(&"visionos"));
    }

    #[test]
    fn test_requirement_with_no_matching_type() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let path = temp_dir.path().join("Package.swift.json");
        fs::write(
            &path,
            r#"{
                "name": "TestPkg",
                "dependencies": [{
                    "sourceControl": [{
                        "identity": "some-dep",
                        "location": {"remote": [{"urlString": "https://github.com/org/repo"}]},
                        "requirement": {"unknownType": ["something"]}
                    }]
                }]
            }"#,
        )
        .expect("Failed to write");

        let data = SwiftManifestJsonParser::extract_first_package(&path);
        assert_eq!(data.dependencies.len(), 1);
        let dep = &data.dependencies[0];
        assert_eq!(dep.extracted_requirement, None);
        assert_eq!(dep.is_pinned, Some(false));
    }
}
