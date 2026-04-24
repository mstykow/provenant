// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use crate::models::DatasourceId;
    use crate::models::PackageType;
    use crate::models::Sha1Digest;
    use std::path::PathBuf;

    use crate::parsers::{PackageParser, PodfileLockParser};

    #[test]
    fn test_is_match() {
        assert!(PodfileLockParser::is_match(&PathBuf::from("Podfile.lock")));
        assert!(PodfileLockParser::is_match(&PathBuf::from(
            "/some/path/Podfile.lock"
        )));
        assert!(PodfileLockParser::is_match(&PathBuf::from(
            "artsy_eigen_Podfile.lock"
        )));
        assert!(PodfileLockParser::is_match(&PathBuf::from(
            "braintree_ios_Podfile.lock"
        )));
        assert!(!PodfileLockParser::is_match(&PathBuf::from("Podfile")));
        assert!(!PodfileLockParser::is_match(&PathBuf::from("package.json")));
        assert!(!PodfileLockParser::is_match(&PathBuf::from("Gemfile.lock")));
    }

    #[test]
    fn test_parse_braintree_ios_podfile_lock() {
        let path = PathBuf::from("testdata/cocoapods/podfile_lock/braintree_ios_Podfile.lock");
        let pkg = PodfileLockParser::extract_first_package(&path);

        assert_eq!(pkg.package_type, Some(PackageType::Cocoapods));
        assert_eq!(pkg.primary_language.as_deref(), Some("Objective-C"));
        assert_eq!(pkg.datasource_id, Some(DatasourceId::CocoapodsPodfileLock));
        assert!(pkg.name.is_none());
        assert!(pkg.version.is_none());

        let extra = pkg.extra_data.as_ref().unwrap();
        assert_eq!(extra["cocoapods"], "1.10.1");
        assert_eq!(
            extra["podfile_checksum"],
            "75163f16229528991a9364c7c1a44cd57a30cac6"
        );

        assert_eq!(pkg.dependencies.len(), 11);

        let expecta = &pkg.dependencies[0];
        assert_eq!(expecta.purl.as_deref(), Some("pkg:cocoapods/Expecta@1.0.6"));
        assert_eq!(expecta.extracted_requirement.as_deref(), Some("1.0.6"));
        assert_eq!(expecta.scope.as_deref(), Some("dependencies"));
        assert_eq!(expecta.is_runtime, None);
        assert_eq!(expecta.is_optional, None);
        assert_eq!(expecta.is_pinned, Some(true));
        assert_eq!(expecta.is_direct, Some(true));

        let resolved = expecta.resolved_package.as_ref().unwrap();
        assert_eq!(resolved.name, "Expecta");
        assert_eq!(resolved.version, "1.0.6");
        assert!(resolved.namespace.is_empty());
        assert_eq!(
            resolved.sha1,
            Some(Sha1Digest::from_hex("3b6bd90a64b9a1dcb0b70aa0e10a7f8f631667d5").unwrap())
        );
        assert!(resolved.is_virtual);
        let res_extra = resolved.extra_data.as_ref().unwrap();
        assert_eq!(res_extra["spec_repo"], "trunk");

        let ohhttpstubs = &pkg.dependencies[3];
        assert_eq!(
            ohhttpstubs.purl.as_deref(),
            Some("pkg:cocoapods/OHHTTPStubs@9.0.0")
        );
        assert_eq!(ohhttpstubs.is_direct, Some(true));
        let resolved = ohhttpstubs.resolved_package.as_ref().unwrap();
        assert_eq!(resolved.dependencies.len(), 1);
        let nested = &resolved.dependencies[0];
        assert_eq!(
            nested.purl.as_deref(),
            Some("pkg:cocoapods/OHHTTPStubs/Default@9.0.0")
        );
        assert_eq!(nested.extracted_requirement.as_deref(), Some("= 9.0.0"));

        let ohhttpstubs_core = &pkg.dependencies[4];
        assert_eq!(
            ohhttpstubs_core.purl.as_deref(),
            Some("pkg:cocoapods/OHHTTPStubs/Core@9.0.0")
        );
        assert_eq!(ohhttpstubs_core.is_direct, Some(false));
        let resolved = ohhttpstubs_core.resolved_package.as_ref().unwrap();
        assert_eq!(resolved.namespace, "OHHTTPStubs");
        assert_eq!(resolved.name, "Core");

        let ohhttpstubs_default = &pkg.dependencies[5];
        let resolved = ohhttpstubs_default.resolved_package.as_ref().unwrap();
        assert_eq!(resolved.dependencies.len(), 4);

        let xcbeautify = &pkg.dependencies[10];
        assert_eq!(
            xcbeautify.purl.as_deref(),
            Some("pkg:cocoapods/xcbeautify@0.8.1")
        );
        assert_eq!(xcbeautify.is_direct, Some(true));
        let resolved = xcbeautify.resolved_package.as_ref().unwrap();
        assert_eq!(
            resolved.sha1,
            Some(Sha1Digest::from_hex("a3b03e4a38eb1a5766a83a7a3c53915a233572e3").unwrap())
        );
    }

    #[test]
    fn test_parse_artsy_eigen_podfile_lock() {
        let path = PathBuf::from("testdata/cocoapods/podfile_lock/artsy_eigen_Podfile.lock");
        let pkg = PodfileLockParser::extract_first_package(&path);

        assert_eq!(pkg.package_type, Some(PackageType::Cocoapods));
        assert_eq!(pkg.dependencies.len(), 25);

        let extra = pkg.extra_data.as_ref().unwrap();
        assert_eq!(extra["cocoapods"], "1.14.3");
        assert_eq!(
            extra["podfile_checksum"],
            "5692a82aae086bb5c68f7181faa1760979de637c"
        );

        let aerodramus = &pkg.dependencies[0];
        assert_eq!(
            aerodramus.purl.as_deref(),
            Some("pkg:cocoapods/Aerodramus@2.0.0")
        );
        assert_eq!(aerodramus.is_direct, Some(true));
        let resolved = aerodramus.resolved_package.as_ref().unwrap();
        assert_eq!(
            resolved.sha1,
            Some(Sha1Digest::from_hex("a22de7451c8fc85ae5d974f5d6a656f59046fffc").unwrap())
        );
        let res_extra = resolved.extra_data.as_ref().unwrap();
        assert_eq!(res_extra["spec_repo"], "https://github.com/artsy/Specs.git");
        assert_eq!(resolved.dependencies.len(), 1);
        assert_eq!(
            resolved.dependencies[0].purl.as_deref(),
            Some("pkg:cocoapods/ISO8601DateFormatter@0.7.1")
        );

        let afnetwork_logger = &pkg.dependencies[1];
        assert_eq!(
            afnetwork_logger.purl.as_deref(),
            Some("pkg:cocoapods/AFNetworkActivityLogger@2.0.4")
        );
        let resolved = afnetwork_logger.resolved_package.as_ref().unwrap();
        assert_eq!(resolved.dependencies.len(), 2);
        assert_eq!(
            resolved.dependencies[0].extracted_requirement.as_deref(),
            Some("~> 2.0")
        );

        let appcenter = pkg
            .dependencies
            .iter()
            .find(|d| d.purl.as_deref() == Some("pkg:cocoapods/appcenter-core@5.0.0"))
            .unwrap();
        assert_eq!(appcenter.is_direct, Some(true));
        let resolved = appcenter.resolved_package.as_ref().unwrap();
        let res_extra = resolved.extra_data.as_ref().unwrap();
        assert_eq!(res_extra["external_source"], "../node_modules/appcenter");

        let boost = pkg
            .dependencies
            .iter()
            .find(|d| d.purl.as_deref() == Some("pkg:cocoapods/boost@1.76.0"))
            .unwrap();
        assert_eq!(boost.is_direct, Some(true));
        let resolved = boost.resolved_package.as_ref().unwrap();
        let res_extra = resolved.extra_data.as_ref().unwrap();
        assert_eq!(
            res_extra["external_source"],
            "../node_modules/react-native/third-party-podspecs/boost.podspec"
        );

        let iso8601 = pkg
            .dependencies
            .iter()
            .find(|d| d.purl.as_deref() == Some("pkg:cocoapods/ISO8601DateFormatter@0.7.1"))
            .unwrap();
        let resolved = iso8601.resolved_package.as_ref().unwrap();
        let res_extra = resolved.extra_data.as_ref().unwrap();
        assert_eq!(
            res_extra["external_source"],
            "https://github.com/artsy/iso-8601-date-formatter/tree/1a48b819c85903ded669e74e476aceffebf311fc"
        );

        let pulley = pkg
            .dependencies
            .iter()
            .find(|d| d.purl.as_deref() == Some("pkg:cocoapods/Pulley@2.6.2"))
            .unwrap();
        assert_eq!(pulley.is_direct, Some(true));
        let resolved = pulley.resolved_package.as_ref().unwrap();
        let res_extra = resolved.extra_data.as_ref().unwrap();
        assert_eq!(
            res_extra["external_source"],
            "https://github.com/artsy/Pulley/tree/f677b18b332ea3798dc379879dbc0d038efd3ccc"
        );
    }

    #[test]
    fn test_parse_dep_requirements_simple() {
        let (ns, name, version, req) =
            super::super::podfile_lock::parse_dep_requirements("OHHTTPStubs (9.0.0)");
        assert!(ns.is_none());
        assert_eq!(name, "OHHTTPStubs");
        assert_eq!(version.as_deref(), Some("9.0.0"));
        assert_eq!(req.as_deref(), Some("9.0.0"));
    }

    #[test]
    fn test_parse_dep_requirements_with_namespace() {
        let (ns, name, version, req) =
            super::super::podfile_lock::parse_dep_requirements("OHHTTPStubs/NSURLSession");
        assert_eq!(ns.as_deref(), Some("OHHTTPStubs"));
        assert_eq!(name, "NSURLSession");
        assert!(version.is_none());
        assert!(req.is_none());
    }

    #[test]
    fn test_parse_dep_requirements_with_constraint() {
        let (ns, name, version, req) = super::super::podfile_lock::parse_dep_requirements(
            " AFNetworking/Serialization (= 3.0.4) ",
        );
        assert_eq!(ns.as_deref(), Some("AFNetworking"));
        assert_eq!(name, "Serialization");
        assert_eq!(version.as_deref(), Some("3.0.4"));
        assert_eq!(req.as_deref(), Some("= 3.0.4"));
    }

    #[test]
    fn test_parse_dep_requirements_tilde() {
        let (ns, name, version, req) = super::super::podfile_lock::parse_dep_requirements(
            "AFNetworking/NSURLConnection (~> 2.0)",
        );
        assert_eq!(ns.as_deref(), Some("AFNetworking"));
        assert_eq!(name, "NSURLConnection");
        assert_eq!(version.as_deref(), Some("2.0"));
        assert_eq!(req.as_deref(), Some("~> 2.0"));
    }

    #[test]
    fn test_parse_empty_podfile_lock() {
        use std::fs;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let path = dir.path().join("Podfile.lock");
        fs::write(&path, "{}").unwrap();

        let pkg = PodfileLockParser::extract_first_package(&path);
        assert_eq!(pkg.package_type, Some(PackageType::Cocoapods));
        assert!(pkg.dependencies.is_empty());
    }

    #[test]
    fn test_parse_minimal_podfile_lock() {
        use std::fs;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let path = dir.path().join("Podfile.lock");
        fs::write(
            &path,
            "PODS:\n  - Alamofire (5.4.3)\n\nDEPENDENCIES:\n  - Alamofire\n\nCOCOAPODS: 1.11.0\n",
        )
        .unwrap();

        let pkg = PodfileLockParser::extract_first_package(&path);
        assert_eq!(pkg.dependencies.len(), 1);

        let dep = &pkg.dependencies[0];
        assert_eq!(dep.purl.as_deref(), Some("pkg:cocoapods/Alamofire@5.4.3"));
        assert_eq!(dep.is_direct, Some(true));
        assert_eq!(dep.scope.as_deref(), Some("dependencies"));
        assert_eq!(dep.is_runtime, None);
        assert_eq!(dep.is_optional, None);

        let extra = pkg.extra_data.as_ref().unwrap();
        assert_eq!(extra["cocoapods"], "1.11.0");
    }

    #[test]
    fn test_nonexistent_file() {
        let path = PathBuf::from("nonexistent/Podfile.lock");
        let pkg = PodfileLockParser::extract_first_package(&path);
        assert_eq!(pkg.package_type, Some(PackageType::Cocoapods));
        assert!(pkg.dependencies.is_empty());
    }

    #[test]
    fn test_checksum_lookup_uses_proper_podname() {
        let path = PathBuf::from("testdata/cocoapods/podfile_lock/braintree_ios_Podfile.lock");
        let pkg = PodfileLockParser::extract_first_package(&path);

        let ohhttpstubs = &pkg.dependencies[3];
        let resolved = ohhttpstubs.resolved_package.as_ref().unwrap();
        assert_eq!(
            resolved.sha1,
            Some(Sha1Digest::from_hex("cb29d2a9d09a828ecb93349a2b0c64f99e0db89f").unwrap())
        );

        let ohhttpstubs_core = &pkg.dependencies[4];
        let resolved = ohhttpstubs_core.resolved_package.as_ref().unwrap();
        assert!(resolved.sha1.is_none());
    }

    #[test]
    fn test_lockfile_scope_is_dependencies_for_nested_resolved_dependencies() {
        let path = PathBuf::from("testdata/cocoapods/podfile_lock/braintree_ios_Podfile.lock");
        let pkg = PodfileLockParser::extract_first_package(&path);

        let ohhttpstubs = &pkg.dependencies[3];
        let resolved = ohhttpstubs.resolved_package.as_ref().unwrap();
        let nested = &resolved.dependencies[0];

        assert_eq!(nested.scope.as_deref(), Some("dependencies"));
        assert_eq!(nested.is_runtime, None);
        assert_eq!(nested.is_optional, None);
    }
}
