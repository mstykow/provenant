// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use tempfile::TempDir;

    use crate::models::{DatasourceId, PackageType};
    use crate::parsers::{PackageParser, PubliccodeParser};

    fn create_temp_publiccode(file_name: &str, content: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("tempdir");
        let path = temp_dir.path().join(file_name);
        fs::write(&path, content).expect("write publiccode file");
        (temp_dir, path)
    }

    #[test]
    fn test_is_match() {
        assert!(PubliccodeParser::is_match(
            PathBuf::from("publiccode.yml").as_path()
        ));
        assert!(PubliccodeParser::is_match(
            PathBuf::from("publiccode.yaml").as_path()
        ));
        assert!(!PubliccodeParser::is_match(
            PathBuf::from("package.json").as_path()
        ));
    }

    #[test]
    fn test_extract_basic_publiccode_metadata() {
        let (_temp_dir, path) = create_temp_publiccode(
            "publiccode.yml",
            "publiccodeYmlVersion: '0.4'\nname:\n  en: Demo Public Service\nsoftwareVersion: 2.3.4\nurl: https://github.com/example/public-service\nlandingURL: https://example.com/service\nlongDescription:\n  en: Service description\nlegal:\n  license: AGPL-3.0-or-later\n  mainCopyrightOwner: Example City\nmaintenance:\n  contacts:\n    - name: Maintainer One\n      email: maintainer@example.com\n",
        );

        let package = PubliccodeParser::extract_first_package(&path);
        assert_eq!(package.package_type, Some(PackageType::Publiccode));
        assert_eq!(package.datasource_id, Some(DatasourceId::PubliccodeYaml));
        assert_eq!(package.name.as_deref(), Some("Demo Public Service"));
        assert_eq!(package.version.as_deref(), Some("2.3.4"));
        assert_eq!(package.description.as_deref(), Some("Service description"));
        assert_eq!(
            package.vcs_url.as_deref(),
            Some("https://github.com/example/public-service")
        );
        assert_eq!(
            package.homepage_url.as_deref(),
            Some("https://example.com/service")
        );
        assert_eq!(
            package.declared_license_expression_spdx.as_deref(),
            Some("AGPL-3.0-or-later")
        );
        assert_eq!(package.copyright.as_deref(), Some("Example City"));
        assert_eq!(package.parties.len(), 1);
        assert_eq!(package.parties[0].role.as_deref(), Some("maintainer"));
    }

    #[test]
    fn test_missing_publiccode_version_returns_default_package() {
        let (_temp_dir, path) = create_temp_publiccode("publiccode.yml", "name: Demo\n");
        let package = PubliccodeParser::extract_first_package(&path);
        assert_eq!(package.package_type, Some(PackageType::Publiccode));
        assert_eq!(package.datasource_id, Some(DatasourceId::PubliccodeYaml));
        assert!(package.name.is_none());
    }
}
