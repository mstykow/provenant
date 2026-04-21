// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use super::super::PackageParser;
    use super::super::rpm_mariner_manifest::*;
    use crate::models::DatasourceId;
    use crate::models::PackageType;
    use std::path::PathBuf;

    #[test]
    fn test_is_match() {
        assert!(RpmMarinerManifestParser::is_match(&PathBuf::from(
            "/var/lib/rpmmanifest/container-manifest-2"
        )));
        assert!(RpmMarinerManifestParser::is_match(&PathBuf::from(
            "/rootfs/var/lib/rpmmanifest/container-manifest-2"
        )));
        assert!(!RpmMarinerManifestParser::is_match(&PathBuf::from(
            "container-manifest-1"
        )));
        assert!(!RpmMarinerManifestParser::is_match(&PathBuf::from(
            "manifest.txt"
        )));
    }

    #[test]
    fn test_parse_basic_manifest() {
        let content = "bash\t5.0.17\t1\t2\tMicrosoft\t3\t4\tx86_64\tsha256\tbash-5.0.17-1.cm2.x86_64.rpm\n\
                       coreutils\t8.32\t5\t6\tMicrosoft\t7\t8\tx86_64\tsha256\tcoreutils-8.32-1.cm2.x86_64.rpm\n";

        let packages = parse_rpm_mariner_manifest(content);
        assert_eq!(packages.len(), 2);

        // Check first package
        let pkg1 = &packages[0];
        assert_eq!(pkg1.package_type, Some(PackageType::Rpm));
        assert_eq!(pkg1.namespace.as_deref(), Some("mariner"));
        assert_eq!(pkg1.name.as_deref(), Some("bash"));
        assert_eq!(pkg1.version.as_deref(), Some("5.0.17"));
        assert!(pkg1.qualifiers.is_some());
        let quals = pkg1.qualifiers.as_ref().unwrap();
        assert_eq!(quals.get("arch"), Some(&"x86_64".to_string()));
        assert_eq!(pkg1.datasource_id, Some(DatasourceId::RpmMarinerManifest));

        // Check extra_data contains filename
        assert!(pkg1.extra_data.is_some());
        let extra = pkg1.extra_data.as_ref().unwrap();
        assert_eq!(
            extra.get("filename").and_then(|v| v.as_str()),
            Some("bash-5.0.17-1.cm2.x86_64.rpm")
        );

        // Check second package
        let pkg2 = &packages[1];
        assert_eq!(pkg2.name.as_deref(), Some("coreutils"));
        assert_eq!(pkg2.version.as_deref(), Some("8.32"));
    }

    #[test]
    fn test_parse_single_package() {
        let content = "test-pkg\t1.0.0\t1\t2\tMicrosoft\t3\t4\taarch64\tsha256\ttest-1.0.0.rpm\n";

        let packages = parse_rpm_mariner_manifest(content);
        assert_eq!(packages.len(), 1);

        let pkg = &packages[0];
        assert_eq!(pkg.name.as_deref(), Some("test-pkg"));
        assert_eq!(pkg.version.as_deref(), Some("1.0.0"));
        assert!(pkg.qualifiers.is_some());
        let quals = pkg.qualifiers.as_ref().unwrap();
        assert_eq!(quals.get("arch"), Some(&"aarch64".to_string()));
    }

    #[test]
    fn test_parse_empty_fields() {
        let content = "pkg\t1.0\t1\t2\tMicrosoft\t3\t4\t\tsha256\t";

        let packages = parse_rpm_mariner_manifest(content);
        assert_eq!(packages.len(), 1);

        let pkg = &packages[0];
        assert_eq!(pkg.name.as_deref(), Some("pkg"));
        assert_eq!(pkg.version.as_deref(), Some("1.0"));
        // Empty arch should result in None qualifiers
        assert_eq!(pkg.qualifiers, None);
        // Empty filename should result in None extra_data
        assert_eq!(pkg.extra_data, None);
    }

    #[test]
    fn test_parse_empty_content() {
        let content = "\n\n";

        let packages = parse_rpm_mariner_manifest(content);
        // Should return default package
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].package_type, Some(PackageType::Rpm));
        assert_eq!(packages[0].namespace.as_deref(), Some("mariner"));
        assert_eq!(
            packages[0].datasource_id,
            Some(DatasourceId::RpmMarinerManifest)
        );
    }

    #[test]
    fn test_parse_invalid_line() {
        let content = "invalid\tline\n\
                       bash\t5.0.17\t1\t2\tMicrosoft\t3\t4\tx86_64\tsha256\tbash-5.0.17.rpm\n";

        let packages = parse_rpm_mariner_manifest(content);
        // Should skip invalid line, only parse valid one
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name.as_deref(), Some("bash"));
    }
}
