// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use super::super::PackageParser;
    use super::super::cpan_dist_ini::*;
    use crate::models::DatasourceId;
    use crate::models::PackageType;
    use std::path::PathBuf;

    #[test]
    fn test_is_match() {
        assert!(CpanDistIniParser::is_match(&PathBuf::from(
            "/path/to/dist.ini"
        )));
        assert!(CpanDistIniParser::is_match(&PathBuf::from(
            "some/module/dist.ini"
        )));
        assert!(!CpanDistIniParser::is_match(&PathBuf::from("dist.txt")));
        assert!(!CpanDistIniParser::is_match(&PathBuf::from("package.ini")));
    }

    #[test]
    fn test_parse_basic_dist_ini() {
        let content = r#"
name = Dancer2-Plugin-Minion
version = 1.0.0
author = Jason A. Crome <[email protected]>
license = Perl_5
copyright_holder = Jason A. Crome
copyright_year = 2024
abstract = Dancer2 plugin for Minion job queue
"#;
        let pkg = parse_dist_ini(content);

        assert_eq!(pkg.package_type, Some(PackageType::Cpan));
        assert_eq!(pkg.namespace.as_deref(), Some("cpan"));
        assert_eq!(pkg.name.as_deref(), Some("Dancer2::Plugin::Minion"));
        assert_eq!(pkg.version.as_deref(), Some("1.0.0"));
        assert_eq!(
            pkg.description.as_deref(),
            Some("Dancer2 plugin for Minion job queue")
        );
        assert_eq!(
            pkg.declared_license_expression.as_deref(),
            Some("gpl-1.0-plus OR artistic-perl-1.0")
        );
        assert_eq!(
            pkg.declared_license_expression_spdx.as_deref(),
            Some("GPL-1.0-or-later OR Artistic-1.0-Perl")
        );
        assert_eq!(pkg.extracted_license_statement.as_deref(), Some("Perl_5"));
        assert_eq!(pkg.license_detections.len(), 1);
        assert_eq!(pkg.primary_language.as_deref(), Some("Perl"));
        assert_eq!(pkg.datasource_id, Some(DatasourceId::CpanDistIni));

        assert_eq!(pkg.parties.len(), 1);
        assert_eq!(pkg.parties[0].role.as_deref(), Some("author"));
        assert_eq!(pkg.parties[0].name.as_deref(), Some("Jason A. Crome"));
        assert_eq!(pkg.parties[0].email.as_deref(), Some("[email protected]"));

        assert!(pkg.extra_data.is_some());
        let extra = pkg.extra_data.as_ref().unwrap();
        assert_eq!(
            extra.get("copyright_holder").and_then(|v| v.as_str()),
            Some("Jason A. Crome")
        );
        assert_eq!(
            extra.get("copyright_year").and_then(|v| v.as_str()),
            Some("2024")
        );
    }

    #[test]
    fn test_parse_with_dependencies() {
        let content = r#"
name = Markdent
version = 0.13
author = Dave Rolsky <[email protected]>
license = Perl_5

[Prereq]
Moose = 0.92
MooseX::Params::Validate = 0.12
namespace::autoclean = 0.09

[Prereq / TestRequires]
Test::More = 0.88
Test::Exception = 0
"#;
        let pkg = parse_dist_ini(content);

        assert_eq!(pkg.name.as_deref(), Some("Markdent"));
        assert_eq!(pkg.version.as_deref(), Some("0.13"));

        assert_eq!(pkg.dependencies.len(), 5);
        let deps = &pkg.dependencies;

        let moose_dep = deps
            .iter()
            .find(|d| d.purl.as_deref() == Some("pkg:cpan/Moose"));
        assert!(moose_dep.is_some());
        let moose = moose_dep.unwrap();
        assert_eq!(moose.extracted_requirement.as_deref(), Some("0.92"));
        assert_eq!(moose.scope.as_deref(), Some("runtime"));
        assert_eq!(moose.is_runtime, Some(true));

        let test_dep = deps
            .iter()
            .find(|d| d.purl.as_deref() == Some("pkg:cpan/Test::More"));
        assert!(test_dep.is_some());
        let test = test_dep.unwrap();
        assert_eq!(test.extracted_requirement.as_deref(), Some("0.88"));
        assert_eq!(test.scope.as_deref(), Some("test"));
        assert_eq!(test.is_runtime, Some(false));
    }

    #[test]
    fn test_parse_configure_requires_scope() {
        let content = r#"
name = Example-Dist
version = 1.0

[Prereq / ConfigureRequires]
ExtUtils::MakeMaker = 7.70

[Prereqs / RuntimeRequires]
Moose = 2.2200
"#;
        let pkg = parse_dist_ini(content);

        assert_eq!(pkg.dependencies.len(), 2);

        let configure_dep = pkg
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:cpan/ExtUtils::MakeMaker"))
            .expect("configure dependency missing");
        assert_eq!(configure_dep.scope.as_deref(), Some("configure"));
        assert_eq!(configure_dep.extracted_requirement.as_deref(), Some("7.70"));
        assert_eq!(configure_dep.is_runtime, Some(false));
        assert_eq!(configure_dep.is_optional, Some(false));

        let runtime_dep = pkg
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:cpan/Moose"))
            .expect("runtime dependency missing");
        assert_eq!(runtime_dep.scope.as_deref(), Some("runtime"));
        assert_eq!(runtime_dep.is_runtime, Some(true));
    }

    #[test]
    fn test_parse_minimal() {
        let content = r#"
name = Simple-Module
version = 1.0
"#;
        let pkg = parse_dist_ini(content);

        assert_eq!(pkg.name.as_deref(), Some("Simple::Module"));
        assert_eq!(pkg.version.as_deref(), Some("1.0"));
        assert_eq!(pkg.declared_license_expression, None);
        assert!(pkg.parties.is_empty());
        assert!(pkg.dependencies.is_empty());
    }

    #[test]
    fn test_parse_author_without_email() {
        let content = r#"
name = Test
version = 1.0
author = John Doe
"#;
        let pkg = parse_dist_ini(content);

        assert_eq!(pkg.parties.len(), 1);
        assert_eq!(pkg.parties[0].name.as_deref(), Some("John Doe"));
        assert_eq!(pkg.parties[0].email, None);
    }

    #[test]
    fn test_parse_empty_content() {
        let content = "";
        let pkg = parse_dist_ini(content);

        assert_eq!(pkg.package_type, Some(PackageType::Cpan));
        assert_eq!(pkg.name, None);
        assert_eq!(pkg.version, None);
    }
}
