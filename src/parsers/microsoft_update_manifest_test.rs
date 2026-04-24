// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use super::super::PackageParser;
    use super::super::microsoft_update_manifest::*;
    use crate::models::DatasourceId;
    use crate::models::PackageType;
    use crate::models::Party;
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
        assert_eq!(pkg.holder.as_deref(), Some("Microsoft Corporation"));
        assert_eq!(
            pkg.parties,
            vec![Party {
                r#type: Some("organization".to_string()),
                role: Some("owner".to_string()),
                name: Some("Microsoft Corporation".to_string()),
                email: None,
                url: None,
                organization: None,
                organization_url: None,
                timezone: None,
            }]
        );
        assert_eq!(pkg.package_type, Some(PackageType::WindowsUpdate));
        assert_eq!(
            pkg.datasource_id,
            Some(DatasourceId::MicrosoftUpdateManifestMum)
        );
    }

    #[test]
    fn test_parse_prefers_top_level_assembly_identity() {
        let content = r#"<?xml version="1.0" encoding="utf-8"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v3"
          description="Fix for KB5049993"
          company="Microsoft Corporation"
          copyright="Microsoft Corporation"
          supportInformation="https://support.microsoft.com/help/5049993">
  <assemblyIdentity name="Package_for_RollupFix" version="14393.7699.1.9" />
  <package identifier="KB5049993">
    <parent>
      <assemblyIdentity name="Microsoft-Windows-ServerStandardEdition" version="10.0.14393.0" />
    </parent>
    <update name="5049993-23661_neutral_PACKAGE">
      <package integrate="hidden">
        <assemblyIdentity name="Package_1_for_KB5049993" version="10.0.1.9" />
      </package>
    </update>
  </package>
</assembly>"#;

        let pkg = parse_mum_xml(content);

        assert_eq!(pkg.name.as_deref(), Some("Package_for_RollupFix"));
        assert_eq!(pkg.version.as_deref(), Some("14393.7699.1.9"));
        assert_eq!(pkg.description.as_deref(), Some("Fix for KB5049993"));
        assert_eq!(pkg.holder.as_deref(), Some("Microsoft Corporation"));
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
