// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use super::super::PackageParser;
    use super::super::cpan_makefile_pl::*;
    use crate::models::DatasourceId;
    use crate::models::PackageType;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_is_match() {
        assert!(CpanMakefilePlParser::is_match(&PathBuf::from(
            "/path/to/Makefile.PL"
        )));
        assert!(CpanMakefilePlParser::is_match(&PathBuf::from(
            "some/module/Makefile.PL"
        )));
        assert!(!CpanMakefilePlParser::is_match(&PathBuf::from("Makefile")));
        assert!(!CpanMakefilePlParser::is_match(&PathBuf::from(
            "makefile.pl"
        )));
        assert!(!CpanMakefilePlParser::is_match(&PathBuf::from(
            "Makefile.pm"
        )));
    }

    #[test]
    fn test_parse_basic_makefile_pl() {
        let content = r#"
use ExtUtils::MakeMaker;
WriteMakefile(
    NAME         => 'Acme::Example',
    VERSION      => '1.23',
    AUTHOR       => 'Jane Smith <jane@example.com>',
    ABSTRACT     => 'An example CPAN module',
    LICENSE      => 'perl_5',
    PREREQ_PM    => {
        'Carp'          => 0,
        'File::Spec'    => '3.40',
    },
    BUILD_REQUIRES => {
        'Test::More'    => '0.88',
    },
    MIN_PERL_VERSION => '5.008001',
);
"#;
        let pkg = parse_makefile_pl(content);

        assert_eq!(pkg.package_type, Some(PackageType::Cpan));
        assert_eq!(pkg.namespace.as_deref(), Some("cpan"));
        assert_eq!(pkg.name.as_deref(), Some("Acme::Example"));
        assert_eq!(pkg.version.as_deref(), Some("1.23"));
        assert_eq!(pkg.description.as_deref(), Some("An example CPAN module"));
        assert_eq!(pkg.extracted_license_statement.as_deref(), Some("perl_5"));
        assert_eq!(
            pkg.declared_license_expression.as_deref(),
            Some("gpl-1.0-plus OR artistic-perl-1.0")
        );
        assert_eq!(
            pkg.declared_license_expression_spdx.as_deref(),
            Some("GPL-1.0-or-later OR Artistic-1.0-Perl")
        );
        assert_eq!(pkg.license_detections.len(), 1);
        assert_eq!(pkg.primary_language.as_deref(), Some("Perl"));
        assert_eq!(pkg.datasource_id, Some(DatasourceId::CpanMakefile));
        assert_eq!(pkg.purl.as_deref(), Some("pkg:cpan/Acme-Example@1.23"));

        // Check author
        assert_eq!(pkg.parties.len(), 1);
        assert_eq!(pkg.parties[0].role.as_deref(), Some("author"));
        assert_eq!(pkg.parties[0].name.as_deref(), Some("Jane Smith"));
        assert_eq!(pkg.parties[0].email.as_deref(), Some("jane@example.com"));
        assert_eq!(pkg.parties[0].r#type.as_deref(), Some("person"));

        // Check extra_data
        assert!(pkg.extra_data.is_some());
        let extra = pkg.extra_data.as_ref().unwrap();
        assert_eq!(
            extra.get("MIN_PERL_VERSION").and_then(|v| v.as_str()),
            Some("5.008001")
        );

        // Check dependencies
        assert_eq!(pkg.dependencies.len(), 3);

        // Check PREREQ_PM (runtime dependencies)
        let carp_dep = pkg
            .dependencies
            .iter()
            .find(|d| d.purl.as_deref() == Some("pkg:cpan/Carp"));
        assert!(carp_dep.is_some());
        let carp = carp_dep.unwrap();
        assert_eq!(carp.extracted_requirement, None); // version 0 is treated as no requirement
        assert_eq!(carp.scope.as_deref(), Some("runtime"));
        assert_eq!(carp.is_runtime, Some(true));
        assert_eq!(carp.is_optional, Some(false));
        assert_eq!(carp.is_direct, Some(true));

        let file_spec_dep = pkg
            .dependencies
            .iter()
            .find(|d| d.purl.as_deref() == Some("pkg:cpan/File::Spec"));
        assert!(file_spec_dep.is_some());
        let file_spec = file_spec_dep.unwrap();
        assert_eq!(file_spec.extracted_requirement.as_deref(), Some("3.40"));
        assert_eq!(file_spec.scope.as_deref(), Some("runtime"));
        assert_eq!(file_spec.is_runtime, Some(true));

        // Check BUILD_REQUIRES
        let test_more_dep = pkg
            .dependencies
            .iter()
            .find(|d| d.purl.as_deref() == Some("pkg:cpan/Test::More"));
        assert!(test_more_dep.is_some());
        let test_more = test_more_dep.unwrap();
        assert_eq!(test_more.extracted_requirement.as_deref(), Some("0.88"));
        assert_eq!(test_more.scope.as_deref(), Some("build"));
        assert_eq!(test_more.is_runtime, Some(false));
    }

    #[test]
    fn test_parse_multi_author() {
        let content = r#"
use ExtUtils::MakeMaker;
WriteMakefile(
    NAME      => 'Multi::Author',
    VERSION   => '0.01',
    AUTHOR    => [
        'Author One <one@example.com>',
        'Author Two <two@example.com>',
    ],
    LICENSE   => 'artistic_2',
);
"#;
        let pkg = parse_makefile_pl(content);

        assert_eq!(pkg.name.as_deref(), Some("Multi::Author"));
        assert_eq!(pkg.version.as_deref(), Some("0.01"));
        assert_eq!(
            pkg.extracted_license_statement.as_deref(),
            Some("artistic_2")
        );
        assert_eq!(
            pkg.declared_license_expression.as_deref(),
            Some("artistic-2.0")
        );
        assert_eq!(
            pkg.declared_license_expression_spdx.as_deref(),
            Some("Artistic-2.0")
        );
        assert_eq!(pkg.license_detections.len(), 1);

        // Check multiple authors
        assert_eq!(pkg.parties.len(), 2);
        assert_eq!(pkg.parties[0].name.as_deref(), Some("Author One"));
        assert_eq!(pkg.parties[0].email.as_deref(), Some("one@example.com"));
        assert_eq!(pkg.parties[1].name.as_deref(), Some("Author Two"));
        assert_eq!(pkg.parties[1].email.as_deref(), Some("two@example.com"));
    }

    #[test]
    fn test_parse_minimal() {
        let content = r#"
use ExtUtils::MakeMaker;
WriteMakefile(NAME => 'Minimal');
"#;
        let pkg = parse_makefile_pl(content);

        assert_eq!(pkg.name.as_deref(), Some("Minimal"));
        assert_eq!(pkg.version, None);
        assert_eq!(pkg.extracted_license_statement, None);
        assert!(pkg.parties.is_empty());
        assert!(pkg.dependencies.is_empty());
        assert_eq!(pkg.purl.as_deref(), Some("pkg:cpan/Minimal"));
    }

    #[test]
    fn test_parse_version_from() {
        let temp_dir = TempDir::new().expect("temp dir");
        let lib_dir = temp_dir.path().join("lib/My");
        fs::create_dir_all(&lib_dir).expect("create module dir");
        fs::write(
            temp_dir.path().join("Makefile.PL"),
            r#"
use ExtUtils::MakeMaker;
WriteMakefile(
    NAME         => 'My::Module',
    VERSION_FROM => 'lib/My/Module.pm',
);
"#,
        )
        .expect("write Makefile.PL");
        fs::write(
            lib_dir.join("Module.pm"),
            "package My::Module;\nour $VERSION = '1.23';\n1;\n",
        )
        .expect("write module file");

        let pkg = CpanMakefilePlParser::extract_first_package(&temp_dir.path().join("Makefile.PL"));

        assert_eq!(pkg.name.as_deref(), Some("My::Module"));
        assert_eq!(pkg.version.as_deref(), Some("1.23"));
        let extra = pkg.extra_data.as_ref().expect("extra_data should exist");
        assert_eq!(
            extra.get("VERSION_FROM").and_then(|v| v.as_str()),
            Some("lib/My/Module.pm")
        );
    }

    #[test]
    fn test_parse_abstract_from() {
        let temp_dir = TempDir::new().expect("temp dir");
        let lib_dir = temp_dir.path().join("lib/My");
        fs::create_dir_all(&lib_dir).expect("create module dir");
        fs::write(
            temp_dir.path().join("Makefile.PL"),
            r#"
use ExtUtils::MakeMaker;
WriteMakefile(
    NAME          => 'My::Module',
    VERSION       => '1.0',
    ABSTRACT_FROM => 'lib/My/Module.pm',
);
"#,
        )
        .expect("write Makefile.PL");
        fs::write(
            lib_dir.join("Module.pm"),
            "package My::Module;\n\n=head1 NAME\n\nMy::Module - Example abstract from POD\n\n=cut\n\n1;\n",
        )
        .expect("write module file");

        let pkg = CpanMakefilePlParser::extract_first_package(&temp_dir.path().join("Makefile.PL"));

        assert_eq!(
            pkg.description.as_deref(),
            Some("Example abstract from POD")
        );
        let extra = pkg.extra_data.as_ref().expect("extra_data should exist");
        assert_eq!(
            extra.get("ABSTRACT_FROM").and_then(|v| v.as_str()),
            Some("lib/My/Module.pm")
        );
    }

    #[test]
    fn test_parse_version_and_abstract_from_same_module() {
        let temp_dir = TempDir::new().expect("temp dir");
        let lib_dir = temp_dir.path().join("lib/Acme");
        fs::create_dir_all(&lib_dir).expect("create module dir");
        fs::write(
            temp_dir.path().join("Makefile.PL"),
            r#"
use ExtUtils::MakeMaker;
WriteMakefile(
    NAME          => 'Acme::Combined',
    VERSION_FROM  => 'lib/Acme/Combined.pm',
    ABSTRACT_FROM => 'lib/Acme/Combined.pm',
);
"#,
        )
        .expect("write Makefile.PL");
        fs::write(
            lib_dir.join("Combined.pm"),
            "package Acme::Combined;\nour $VERSION = \"0.42\";\n\n=head1 NAME\n\nAcme::Combined - Combined metadata from module\n\n=cut\n\n1;\n",
        )
        .expect("write module file");

        let pkg = CpanMakefilePlParser::extract_first_package(&temp_dir.path().join("Makefile.PL"));

        assert_eq!(pkg.version.as_deref(), Some("0.42"));
        assert_eq!(
            pkg.description.as_deref(),
            Some("Combined metadata from module")
        );
    }

    #[test]
    fn test_parse_inline_version_takes_precedence_over_version_from() {
        let temp_dir = TempDir::new().expect("temp dir");
        let lib_dir = temp_dir.path().join("lib/My");
        fs::create_dir_all(&lib_dir).expect("create module dir");
        fs::write(
            temp_dir.path().join("Makefile.PL"),
            r#"
use ExtUtils::MakeMaker;
WriteMakefile(
    NAME         => 'My::Module',
    VERSION      => '9.99',
    VERSION_FROM => 'lib/My/Module.pm',
);
"#,
        )
        .expect("write Makefile.PL");
        fs::write(
            lib_dir.join("Module.pm"),
            "package My::Module;\nour $VERSION = '1.23';\n1;\n",
        )
        .expect("write module file");

        let pkg = CpanMakefilePlParser::extract_first_package(&temp_dir.path().join("Makefile.PL"));
        assert_eq!(pkg.version.as_deref(), Some("9.99"));
    }

    #[test]
    fn test_parse_inline_abstract_takes_precedence_over_abstract_from() {
        let temp_dir = TempDir::new().expect("temp dir");
        let lib_dir = temp_dir.path().join("lib/My");
        fs::create_dir_all(&lib_dir).expect("create module dir");
        fs::write(
            temp_dir.path().join("Makefile.PL"),
            r#"
use ExtUtils::MakeMaker;
WriteMakefile(
    NAME          => 'My::Module',
    VERSION       => '1.0',
    ABSTRACT      => 'Inline abstract wins',
    ABSTRACT_FROM => 'lib/My/Module.pm',
);
"#,
        )
        .expect("write Makefile.PL");
        fs::write(
            lib_dir.join("Module.pm"),
            "package My::Module;\n\n=head1 NAME\n\nMy::Module - Example abstract from POD\n\n=cut\n\n1;\n",
        )
        .expect("write module file");

        let pkg = CpanMakefilePlParser::extract_first_package(&temp_dir.path().join("Makefile.PL"));
        assert_eq!(pkg.description.as_deref(), Some("Inline abstract wins"));
    }

    #[test]
    fn test_parse_version_from_rejects_absolute_paths() {
        let temp_dir = TempDir::new().expect("temp dir");
        let absolute_module = temp_dir.path().join("outside.pm");
        fs::write(
            temp_dir.path().join("Makefile.PL"),
            format!(
                "use ExtUtils::MakeMaker;\nWriteMakefile(NAME => 'My::Module', VERSION_FROM => '{}');\n",
                absolute_module.display()
            ),
        )
        .expect("write Makefile.PL");
        fs::write(
            &absolute_module,
            "package My::Module;\nour $VERSION = '1.23';\n1;\n",
        )
        .expect("write outside module file");

        let pkg = CpanMakefilePlParser::extract_first_package(&temp_dir.path().join("Makefile.PL"));
        assert_eq!(pkg.version, None);
    }

    #[test]
    fn test_parse_version_from_rejects_parent_traversal() {
        let parent_dir = TempDir::new().expect("parent dir");
        let project_dir = parent_dir.path().join("project");
        fs::create_dir_all(&project_dir).expect("create project dir");
        fs::write(
            project_dir.join("Makefile.PL"),
            "use ExtUtils::MakeMaker;\nWriteMakefile(NAME => 'My::Module', VERSION_FROM => '../escape.pm');\n",
        )
        .expect("write Makefile.PL");
        fs::write(
            parent_dir.path().join("escape.pm"),
            "package My::Module;\nour $VERSION = '1.23';\n1;\n",
        )
        .expect("write outside file");

        let pkg = CpanMakefilePlParser::extract_first_package(&project_dir.join("Makefile.PL"));
        assert_eq!(pkg.version, None);
    }

    #[test]
    fn test_parse_version_from_ignores_commented_version_assignment() {
        let temp_dir = TempDir::new().expect("temp dir");
        let lib_dir = temp_dir.path().join("lib/My");
        fs::create_dir_all(&lib_dir).expect("create module dir");
        fs::write(
            temp_dir.path().join("Makefile.PL"),
            r#"
use ExtUtils::MakeMaker;
WriteMakefile(
    NAME         => 'My::Module',
    VERSION_FROM => 'lib/My/Module.pm',
);
"#,
        )
        .expect("write Makefile.PL");
        fs::write(
            lib_dir.join("Module.pm"),
            "package My::Module;\n# our $VERSION = '9.99';\n# example: $VERSION = '8.88';\n1;\n",
        )
        .expect("write module file");

        let pkg = CpanMakefilePlParser::extract_first_package(&temp_dir.path().join("Makefile.PL"));
        assert_eq!(pkg.version, None);
    }

    #[test]
    fn test_parse_test_requires() {
        let content = r#"
use ExtUtils::MakeMaker;
WriteMakefile(
    NAME          => 'My::Module',
    VERSION       => '1.0',
    TEST_REQUIRES => {
        'Test::More'      => '0.98',
        'Test::Exception' => '0.43',
    },
);
"#;
        let pkg = parse_makefile_pl(content);

        assert_eq!(pkg.dependencies.len(), 2);

        let test_more = pkg
            .dependencies
            .iter()
            .find(|d| d.purl.as_deref() == Some("pkg:cpan/Test::More"));
        assert!(test_more.is_some());
        let test_more = test_more.unwrap();
        assert_eq!(test_more.scope.as_deref(), Some("test"));
        assert_eq!(test_more.is_runtime, Some(false));
        assert_eq!(test_more.extracted_requirement.as_deref(), Some("0.98"));
    }

    #[test]
    fn test_parse_configure_requires() {
        let content = r#"
use ExtUtils::MakeMaker;
WriteMakefile(
    NAME               => 'My::Module',
    VERSION            => '1.0',
    CONFIGURE_REQUIRES => {
        'ExtUtils::MakeMaker' => '6.64',
    },
);
"#;
        let pkg = parse_makefile_pl(content);

        assert_eq!(pkg.dependencies.len(), 1);

        let dep = &pkg.dependencies[0];
        assert_eq!(dep.purl.as_deref(), Some("pkg:cpan/ExtUtils::MakeMaker"));
        assert_eq!(dep.scope.as_deref(), Some("configure"));
        assert_eq!(dep.is_runtime, Some(false));
        assert_eq!(dep.extracted_requirement.as_deref(), Some("6.64"));
    }

    #[test]
    fn test_parse_writemakefile1() {
        let content = r#"
use ExtUtils::MakeMaker;
WriteMakefile1(
    NAME    => 'My::Module',
    VERSION => '1.0',
);
"#;
        let pkg = parse_makefile_pl(content);

        assert_eq!(pkg.name.as_deref(), Some("My::Module"));
        assert_eq!(pkg.version.as_deref(), Some("1.0"));
    }

    #[test]
    fn test_parse_empty_content() {
        let content = "";
        let pkg = parse_makefile_pl(content);

        assert_eq!(pkg.package_type, Some(PackageType::Cpan));
        assert_eq!(pkg.name, None);
        assert_eq!(pkg.version, None);
    }

    #[test]
    fn test_parse_no_writemakefile() {
        let content = r#"
use strict;
use warnings;
# Just a comment
"#;
        let pkg = parse_makefile_pl(content);

        assert_eq!(pkg.package_type, Some(PackageType::Cpan));
        assert_eq!(pkg.name, None);
    }

    #[test]
    fn test_purl_conversion() {
        // Test that Foo::Bar becomes Foo-Bar in PURL
        let content = r#"
use ExtUtils::MakeMaker;
WriteMakefile(
    NAME    => 'Foo::Bar::Baz',
    VERSION => '2.34',
);
"#;
        let pkg = parse_makefile_pl(content);

        assert_eq!(pkg.name.as_deref(), Some("Foo::Bar::Baz"));
        assert_eq!(pkg.purl.as_deref(), Some("pkg:cpan/Foo-Bar-Baz@2.34"));
    }

    #[test]
    fn test_extract_from_testdata() {
        let path = PathBuf::from("testdata/cpan/makefile-pl/basic/Makefile.PL");
        assert!(path.exists(), "missing fixture: {}", path.display());

        let packages = CpanMakefilePlParser::extract_packages(&path);
        assert_eq!(packages.len(), 1);

        let pkg = &packages[0];
        assert_eq!(pkg.name.as_deref(), Some("Acme::Example"));
        assert_eq!(pkg.version.as_deref(), Some("1.23"));
    }

    #[test]
    fn test_author_without_email() {
        let content = r#"
use ExtUtils::MakeMaker;
WriteMakefile(
    NAME    => 'My::Module',
    VERSION => '1.0',
    AUTHOR  => 'John Doe',
);
"#;
        let pkg = parse_makefile_pl(content);

        assert_eq!(pkg.parties.len(), 1);
        assert_eq!(pkg.parties[0].name.as_deref(), Some("John Doe"));
        assert_eq!(pkg.parties[0].email, None);
    }

    #[test]
    fn test_parse_makefile_skips_unresolved_template_placeholders() {
        let content = r#"
use strict;
use warnings;
use ExtUtils::MakeMaker;

WriteMakefile(
    NAME         => '[d2% appname %2d]',
    AUTHOR       => q{YOUR NAME <youremail@example.com>},
    VERSION_FROM => '[d2% appfile %2d]',
    ABSTRACT     => 'YOUR APPLICATION ABSTRACT',
    PREREQ_PM    => {
        'Test::More' => 0,
        'YAML'       => 0,
        'Dancer2'    => [d2% dancer_version %2d],
    },
);
"#;

        let pkg = parse_makefile_pl(content);

        assert_eq!(pkg.name, None);
        assert_eq!(pkg.description, None);
        assert!(pkg.parties.is_empty());
        assert_eq!(pkg.purl, None);
        assert!(pkg.extra_data.is_none());
        assert_eq!(pkg.dependencies.len(), 2);
        assert_eq!(
            pkg.dependencies[0].purl.as_deref(),
            Some("pkg:cpan/Test::More")
        );
        assert_eq!(pkg.dependencies[1].purl.as_deref(), Some("pkg:cpan/YAML"));
    }
}
