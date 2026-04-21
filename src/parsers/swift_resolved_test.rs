// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::models::PackageType;
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DatasourceId;

    #[test]
    fn test_is_match() {
        assert!(SwiftPackageResolvedParser::is_match(&PathBuf::from(
            "Package.resolved"
        )));
        assert!(SwiftPackageResolvedParser::is_match(&PathBuf::from(
            "/path/to/Package.resolved"
        )));
        assert!(SwiftPackageResolvedParser::is_match(&PathBuf::from(
            ".package.resolved"
        )));
        assert!(SwiftPackageResolvedParser::is_match(&PathBuf::from(
            "/path/.package.resolved"
        )));
    }

    #[test]
    fn test_is_not_match() {
        assert!(!SwiftPackageResolvedParser::is_match(&PathBuf::from(
            "Package.swift"
        )));
        assert!(!SwiftPackageResolvedParser::is_match(&PathBuf::from(
            "package.json"
        )));
        assert!(!SwiftPackageResolvedParser::is_match(&PathBuf::from(
            "Cargo.lock"
        )));
        assert!(!SwiftPackageResolvedParser::is_match(&PathBuf::from(
            "Package.resolved.bak"
        )));
    }

    #[test]
    fn test_extract_v2_format() {
        let path = PathBuf::from("testdata/swift/Package-v2.resolved");
        let data = SwiftPackageResolvedParser::extract_first_package(&path);

        assert_eq!(data.package_type, Some(PackageType::Swift));
        assert_eq!(data.datasource_id, Some(DatasourceId::SwiftPackageResolved));
        assert_eq!(data.primary_language, Some("Swift".to_string()));
        assert_eq!(data.dependencies.len(), 3);

        let dep0 = &data.dependencies[0];
        assert_eq!(
            dep0.purl.as_deref(),
            Some("pkg:swift/github.com/mapbox/mapbox-common-ios@24.4.0")
        );
        assert_eq!(dep0.extracted_requirement.as_deref(), Some("24.4.0"));
        assert_eq!(dep0.scope.as_deref(), Some("dependencies"));
        assert_eq!(dep0.is_runtime, None);
        assert_eq!(dep0.is_optional, None);
        assert_eq!(dep0.is_pinned, Some(true));
        assert_eq!(dep0.is_direct, None);

        let dep2 = &data.dependencies[2];
        assert_eq!(
            dep2.purl.as_deref(),
            Some("pkg:swift/github.com/mapbox/turf-swift@2.8.0")
        );
    }

    #[test]
    fn test_extract_v1_format() {
        let path = PathBuf::from("testdata/swift/Package-v1.resolved");
        let data = SwiftPackageResolvedParser::extract_first_package(&path);

        assert_eq!(data.package_type, Some(PackageType::Swift));
        assert_eq!(data.datasource_id, Some(DatasourceId::SwiftPackageResolved));
        assert_eq!(data.dependencies.len(), 2);

        let dep0 = &data.dependencies[0];
        assert_eq!(
            dep0.purl.as_deref(),
            Some("pkg:swift/github.com/kareman/SwiftShell@5.1.0")
        );
        assert_eq!(dep0.extracted_requirement.as_deref(), Some("5.1.0"));
        assert_eq!(dep0.is_pinned, Some(true));
        assert_eq!(dep0.is_runtime, None);
        assert_eq!(dep0.is_direct, None);

        let dep1 = &data.dependencies[1];
        assert_eq!(
            dep1.purl.as_deref(),
            Some("pkg:swift/github.com/apple/swift-atomics@1.1.0")
        );
    }

    #[test]
    fn test_extract_v3_format() {
        let path = PathBuf::from("testdata/swift/Package-v3.resolved");
        let data = SwiftPackageResolvedParser::extract_first_package(&path);

        assert_eq!(data.package_type, Some(PackageType::Swift));
        assert_eq!(data.dependencies.len(), 2);

        let remote_dep = &data.dependencies[0];
        assert_eq!(
            remote_dep.purl.as_deref(),
            Some("pkg:swift/github.com/apple/swift-argument-parser@1.2.3")
        );

        let local_dep = &data.dependencies[1];
        assert_eq!(
            local_dep.extracted_requirement.as_deref(),
            Some("abc123def456")
        );
    }

    #[test]
    fn test_revision_fallback_when_no_version() {
        let path = PathBuf::from("testdata/swift/Package-revision-only.resolved");
        let data = SwiftPackageResolvedParser::extract_first_package(&path);

        assert_eq!(data.dependencies.len(), 1);
        let dep = &data.dependencies[0];
        assert_eq!(
            dep.extracted_requirement.as_deref(),
            Some("deadbeef1234567890abcdef")
        );
        assert_eq!(
            dep.purl.as_deref(),
            Some("pkg:swift/github.com/example/experimental-pkg@deadbeef1234567890abcdef")
        );
    }

    #[test]
    fn test_extract_from_reference_vercelui() {
        let path = PathBuf::from("testdata/swift/Package-vercelui.resolved");
        let data = SwiftPackageResolvedParser::extract_first_package(&path);

        assert_eq!(data.package_type, Some(PackageType::Swift));
        assert_eq!(data.dependencies.len(), 5);

        let purls: Vec<&str> = data
            .dependencies
            .iter()
            .filter_map(|d| d.purl.as_deref())
            .collect();

        assert!(purls.contains(&"pkg:swift/github.com/swift-server/async-http-client@1.19.0"));
        assert!(purls.contains(&"pkg:swift/github.com/apple/swift-atomics@1.1.0"));
        assert!(purls.contains(&"pkg:swift/github.com/apple/swift-nio@2.58.0"));
        assert!(purls.contains(&"pkg:swift/github.com/vapor/vapor@4.79.0"));
        assert!(purls.contains(&"pkg:swift/github.com/swift-cloud/Vercel@1.15.2"));
    }

    #[test]
    fn test_extract_from_reference_fastlane_v1() {
        let path = PathBuf::from("testdata/swift/Package-fastlane-v1.resolved");
        let data = SwiftPackageResolvedParser::extract_first_package(&path);

        assert_eq!(data.dependencies.len(), 2);

        let purls: Vec<&str> = data
            .dependencies
            .iter()
            .filter_map(|d| d.purl.as_deref())
            .collect();

        assert!(purls.contains(&"pkg:swift/github.com/kareman/SwiftShell@5.1.0"));
        assert!(purls.contains(&"pkg:swift/github.com/apple/swift-atomics@1.1.0"));
    }

    #[test]
    fn test_extract_from_reference_mapboxmaps() {
        let path = PathBuf::from("testdata/swift/Package-mapboxmaps.resolved");
        let data = SwiftPackageResolvedParser::extract_first_package(&path);

        assert_eq!(data.dependencies.len(), 3);

        let purls: Vec<&str> = data
            .dependencies
            .iter()
            .filter_map(|d| d.purl.as_deref())
            .collect();

        assert!(purls.contains(&"pkg:swift/github.com/mapbox/mapbox-common-ios@24.4.0"));
        assert!(purls.contains(&"pkg:swift/github.com/mapbox/mapbox-core-maps-ios@11.4.0"));
        assert!(purls.contains(&"pkg:swift/github.com/mapbox/turf-swift@2.8.0"));
    }

    #[test]
    fn test_nonexistent_file_returns_default() {
        let path = PathBuf::from("testdata/swift/nonexistent.resolved");
        let data = SwiftPackageResolvedParser::extract_first_package(&path);

        assert_eq!(data.package_type, Some(PackageType::Swift));
        assert!(data.dependencies.is_empty());
    }

    #[test]
    fn test_all_dependencies_have_required_flags() {
        let path = PathBuf::from("testdata/swift/Package-v2.resolved");
        let data = SwiftPackageResolvedParser::extract_first_package(&path);

        for dep in &data.dependencies {
            assert_eq!(dep.is_runtime, None);
            assert_eq!(dep.is_optional, None);
            assert_eq!(dep.is_pinned, Some(true));
            assert_eq!(dep.is_direct, None);
            assert_eq!(dep.scope.as_deref(), Some("dependencies"));
            assert!(dep.purl.is_some());
            assert!(dep.extracted_requirement.is_some());
        }
    }

    #[test]
    fn test_empty_pins() {
        let path = PathBuf::from("testdata/swift/Package-empty.resolved");
        std::fs::write(&path, r#"{"pins": [], "version": 2}"#).ok();
        let data = SwiftPackageResolvedParser::extract_first_package(&path);
        assert!(data.dependencies.is_empty());
        std::fs::remove_file(&path).ok();
    }
}
