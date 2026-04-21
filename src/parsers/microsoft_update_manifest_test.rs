// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use super::super::PackageParser;
    use super::super::microsoft_update_manifest::*;
    use crate::models::DatasourceId;
    use crate::models::PackageType;
    use std::path::PathBuf;

    #[test]
    fn test_is_match() {
        assert!(MicrosoftUpdateManifestParser::is_match(&PathBuf::from(
            "update.mum"
        )));
        assert!(MicrosoftUpdateManifestParser::is_match(&PathBuf::from(
            "/path/to/manifest.mum"
        )));
        assert!(!MicrosoftUpdateManifestParser::is_match(&PathBuf::from(
            "package.xml"
        )));
        assert!(!MicrosoftUpdateManifestParser::is_match(&PathBuf::from(
            "manifest.txt"
        )));
    }

    #[test]
    fn test_parse_basic_mum() {
        let content = r#"<?xml version="1.0" encoding="utf-8"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v3"
          description="Windows Update Package"
          company="Microsoft Corporation"
          copyright="Copyright (c) Microsoft Corporation"
          supportInformation="https://support.microsoft.com">
  <assemblyIdentity name="Package-Component" version="10.0.19041.1" />
</assembly>"#;

        let pkg = parse_mum_xml(content);

        assert_eq!(pkg.name.as_deref(), Some("Package-Component"));
        assert_eq!(pkg.version.as_deref(), Some("10.0.19041.1"));
        assert_eq!(pkg.description.as_deref(), Some("Windows Update Package"));
        assert_eq!(
            pkg.copyright.as_deref(),
            Some("Copyright (c) Microsoft Corporation")
        );
        assert_eq!(
            pkg.homepage_url.as_deref(),
            Some("https://support.microsoft.com")
        );
        assert_eq!(pkg.package_type, Some(PackageType::WindowsUpdate));
        assert_eq!(
            pkg.datasource_id,
            Some(DatasourceId::MicrosoftUpdateManifestMum)
        );
    }

    #[test]
    fn test_parse_minimal_mum() {
        let content = r#"<?xml version="1.0"?>
<assembly>
  <assemblyIdentity name="Component" version="1.0" />
</assembly>"#;

        let pkg = parse_mum_xml(content);

        assert_eq!(pkg.name.as_deref(), Some("Component"));
        assert_eq!(pkg.version.as_deref(), Some("1.0"));
    }

    #[test]
    fn test_parse_invalid_xml() {
        let content = "not xml";
        let pkg = parse_mum_xml(content);

        assert_eq!(pkg.package_type, Some(PackageType::WindowsUpdate));
        assert_eq!(
            pkg.datasource_id,
            Some(DatasourceId::MicrosoftUpdateManifestMum)
        );
    }
}
