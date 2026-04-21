// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use super::super::{CpanManifestParser, CpanMetaJsonParser, CpanMetaYmlParser, PackageParser};
    use crate::models::{DatasourceId, PackageData, PackageType};
    use std::path::PathBuf;

    fn assert_cpan_fallback_identity(package: &PackageData, datasource_id: DatasourceId) {
        assert_eq!(package.package_type, Some(PackageType::Cpan));
        assert_eq!(package.primary_language, Some("Perl".to_string()));
        assert_eq!(package.datasource_id, Some(datasource_id));
        assert_eq!(package.name, None);
        assert_eq!(package.version, None);
    }

    #[test]
    fn test_is_match_meta_json() {
        assert!(CpanMetaJsonParser::is_match(&PathBuf::from("META.json")));
        assert!(CpanMetaJsonParser::is_match(&PathBuf::from(
            "/path/to/META.json"
        )));
        assert!(!CpanMetaJsonParser::is_match(&PathBuf::from(
            "package.json"
        )));
        assert!(!CpanMetaJsonParser::is_match(&PathBuf::from("META.yml")));
    }

    #[test]
    fn test_is_match_meta_yml() {
        assert!(CpanMetaYmlParser::is_match(&PathBuf::from("META.yml")));
        assert!(CpanMetaYmlParser::is_match(&PathBuf::from(
            "/path/to/META.yml"
        )));
        assert!(!CpanMetaYmlParser::is_match(&PathBuf::from("package.json")));
        assert!(!CpanMetaYmlParser::is_match(&PathBuf::from("META.json")));
    }

    #[test]
    fn test_is_match_manifest() {
        assert!(CpanManifestParser::is_match(&PathBuf::from("MANIFEST")));
        assert!(CpanManifestParser::is_match(&PathBuf::from(
            "/path/to/MANIFEST"
        )));
        assert!(!CpanManifestParser::is_match(&PathBuf::from(
            "manifest.txt"
        )));
        assert!(!CpanManifestParser::is_match(&PathBuf::from("META.json")));
    }

    #[test]
    fn test_meta_json_basic() {
        let path = PathBuf::from("testdata/cpan/meta_json/META.json");
        let package = CpanMetaJsonParser::extract_first_package(&path);

        assert_eq!(package.package_type, Some(PackageType::Cpan));
        assert_eq!(package.name, Some("Example-Web-Toolkit".to_string()));
        assert_eq!(package.version, Some("1.042".to_string()));
        assert_eq!(
            package.description,
            Some("A modern Perl toolkit for web development".to_string())
        );
        assert_eq!(package.primary_language, Some("Perl".to_string()));
        assert_eq!(package.datasource_id, Some(DatasourceId::CpanMetaJson));
    }

    #[test]
    fn test_meta_json_license() {
        let path = PathBuf::from("testdata/cpan/meta_json/META.json");
        let package = CpanMetaJsonParser::extract_first_package(&path);

        assert_eq!(
            package.extracted_license_statement,
            Some("perl_5".to_string())
        );
        assert_eq!(
            package.declared_license_expression.as_deref(),
            Some("gpl-1.0-plus OR artistic-perl-1.0")
        );
        assert_eq!(
            package.declared_license_expression_spdx.as_deref(),
            Some("GPL-1.0-or-later OR Artistic-1.0-Perl")
        );
        assert_eq!(package.license_detections.len(), 1);
    }

    #[test]
    fn test_meta_json_parties() {
        let path = PathBuf::from("testdata/cpan/meta_json/META.json");
        let package = CpanMetaJsonParser::extract_first_package(&path);

        assert_eq!(package.parties.len(), 2);

        let first_author = &package.parties[0];
        assert_eq!(first_author.role, Some("author".to_string()));
        assert_eq!(first_author.name, Some("John Doe".to_string()));
        assert_eq!(first_author.email, Some("john@example.com".to_string()));
        assert_eq!(first_author.r#type, Some("person".to_string()));

        let second_author = &package.parties[1];
        assert_eq!(second_author.role, Some("author".to_string()));
        assert_eq!(second_author.name, Some("Jane Smith".to_string()));
        assert_eq!(second_author.email, Some("jane@example.com".to_string()));
    }

    #[test]
    fn test_meta_json_resources() {
        let path = PathBuf::from("testdata/cpan/meta_json/META.json");
        let package = CpanMetaJsonParser::extract_first_package(&path);

        assert_eq!(
            package.homepage_url,
            Some("https://example.com/web-toolkit".to_string())
        );
        assert_eq!(
            package.vcs_url,
            Some("https://github.com/example/web-toolkit.git".to_string())
        );
        assert_eq!(
            package.code_view_url,
            Some("https://github.com/example/web-toolkit".to_string())
        );
        assert_eq!(
            package.bug_tracking_url,
            Some("https://github.com/example/web-toolkit/issues".to_string())
        );
    }

    #[test]
    fn test_meta_json_dependencies() {
        let path = PathBuf::from("testdata/cpan/meta_json/META.json");
        let package = CpanMetaJsonParser::extract_first_package(&path);

        // Should have dependencies from all 4 scopes (runtime, build, test, configure)
        // Excluding "perl" itself: 3 runtime + 2 build + 2 test + 1 configure = 8
        assert_eq!(package.dependencies.len(), 8);

        // Check runtime dependencies
        let runtime_deps: Vec<_> = package
            .dependencies
            .iter()
            .filter(|d| d.scope == Some("runtime".to_string()))
            .collect();
        assert_eq!(runtime_deps.len(), 3);

        let moose_dep = runtime_deps
            .iter()
            .find(|d| d.purl == Some("pkg:cpan/Moose".to_string()))
            .expect("Should have Moose dependency");
        assert_eq!(moose_dep.extracted_requirement, Some("2.2011".to_string()));
        assert_eq!(moose_dep.is_runtime, Some(true));
        assert_eq!(moose_dep.is_optional, Some(false));
        assert_eq!(moose_dep.is_direct, Some(true));

        // Check build dependencies
        let build_deps: Vec<_> = package
            .dependencies
            .iter()
            .filter(|d| d.scope == Some("build".to_string()))
            .collect();
        assert_eq!(build_deps.len(), 2);

        let build_dep = build_deps
            .iter()
            .find(|d| d.purl == Some("pkg:cpan/Module::Build".to_string()))
            .expect("Should have Module::Build dependency");
        assert_eq!(build_dep.is_runtime, Some(false));

        // Check test dependencies
        let test_deps: Vec<_> = package
            .dependencies
            .iter()
            .filter(|d| d.scope == Some("test".to_string()))
            .collect();
        assert_eq!(test_deps.len(), 2);

        // Check configure dependencies
        let configure_deps: Vec<_> = package
            .dependencies
            .iter()
            .filter(|d| d.scope == Some("configure".to_string()))
            .collect();
        assert_eq!(configure_deps.len(), 1);
    }

    #[test]
    fn test_meta_yml_basic() {
        let path = PathBuf::from("testdata/cpan/meta_yml/META.yml");
        let package = CpanMetaYmlParser::extract_first_package(&path);

        assert_eq!(package.package_type, Some(PackageType::Cpan));
        assert_eq!(package.name, Some("Example-DBLayer".to_string()));
        assert_eq!(package.version, Some("0.005".to_string()));
        assert_eq!(
            package.description,
            Some("Simple database abstraction layer".to_string())
        );
        assert_eq!(package.primary_language, Some("Perl".to_string()));
        assert_eq!(package.datasource_id, Some(DatasourceId::CpanMetaYml));
    }

    #[test]
    fn test_meta_yml_license() {
        let path = PathBuf::from("testdata/cpan/meta_yml/META.yml");
        let package = CpanMetaYmlParser::extract_first_package(&path);

        assert_eq!(
            package.extracted_license_statement,
            Some("artistic_2".to_string())
        );
        assert_eq!(
            package.declared_license_expression.as_deref(),
            Some("artistic-2.0")
        );
        assert_eq!(
            package.declared_license_expression_spdx.as_deref(),
            Some("Artistic-2.0")
        );
        assert_eq!(package.license_detections.len(), 1);
    }

    #[test]
    fn test_meta_yml_parties() {
        let path = PathBuf::from("testdata/cpan/meta_yml/META.yml");
        let package = CpanMetaYmlParser::extract_first_package(&path);

        assert_eq!(package.parties.len(), 1);

        let author = &package.parties[0];
        assert_eq!(author.role, Some("author".to_string()));
        assert_eq!(author.name, Some("Alice Developer".to_string()));
        assert_eq!(author.email, Some("alice@cpan.org".to_string()));
        assert_eq!(author.r#type, Some("person".to_string()));
    }

    #[test]
    fn test_meta_yml_resources() {
        let path = PathBuf::from("testdata/cpan/meta_yml/META.yml");
        let package = CpanMetaYmlParser::extract_first_package(&path);

        assert_eq!(
            package.homepage_url,
            Some("https://metacpan.org/dist/Example-DBLayer".to_string())
        );
        assert_eq!(
            package.vcs_url,
            Some("https://github.com/example/dblayer".to_string())
        );
        assert_eq!(
            package.bug_tracking_url,
            Some("https://rt.cpan.org/Public/Dist/Display.html?Name=Example-DBLayer".to_string())
        );
    }

    #[test]
    fn test_meta_yml_dependencies() {
        let path = PathBuf::from("testdata/cpan/meta_yml/META.yml");
        let package = CpanMetaYmlParser::extract_first_package(&path);

        // Should have dependencies from 4 scopes (requires, build_requires, test_requires, configure_requires)
        // Excluding "perl": 2 runtime + 2 build + 2 test + 1 configure = 7
        assert_eq!(package.dependencies.len(), 7);

        // Check runtime dependencies (from requires)
        let runtime_deps: Vec<_> = package
            .dependencies
            .iter()
            .filter(|d| d.scope == Some("runtime".to_string()))
            .collect();
        assert_eq!(runtime_deps.len(), 2);

        let dbi_dep = runtime_deps
            .iter()
            .find(|d| d.purl == Some("pkg:cpan/DBI".to_string()))
            .expect("Should have DBI dependency");
        assert_eq!(dbi_dep.extracted_requirement, Some("1.643".to_string()));
        assert_eq!(dbi_dep.is_runtime, Some(true));
        assert_eq!(dbi_dep.is_optional, Some(false));

        // Check build dependencies
        let build_deps: Vec<_> = package
            .dependencies
            .iter()
            .filter(|d| d.scope == Some("build".to_string()))
            .collect();
        assert_eq!(build_deps.len(), 2);

        // Check test dependencies
        let test_deps: Vec<_> = package
            .dependencies
            .iter()
            .filter(|d| d.scope == Some("test".to_string()))
            .collect();
        assert_eq!(test_deps.len(), 2);

        // Check configure dependencies
        let configure_deps: Vec<_> = package
            .dependencies
            .iter()
            .filter(|d| d.scope == Some("configure".to_string()))
            .collect();
        assert_eq!(configure_deps.len(), 1);
    }

    #[test]
    fn test_manifest_file_references() {
        let path = PathBuf::from("testdata/cpan/manifest/MANIFEST");
        let package = CpanManifestParser::extract_first_package(&path);

        assert_eq!(package.package_type, Some(PackageType::Cpan));
        assert_eq!(package.primary_language, Some("Perl".to_string()));
        assert_eq!(package.datasource_id, Some(DatasourceId::CpanManifest));

        // Check file references
        assert_eq!(package.file_references.len(), 16);

        let first_file = &package.file_references[0];
        assert_eq!(first_file.path, ".gitignore");

        let makefile = package
            .file_references
            .iter()
            .find(|f| f.path == "Makefile.PL")
            .expect("Should have Makefile.PL");
        assert_eq!(makefile.path, "Makefile.PL");

        let meta_yml = package
            .file_references
            .iter()
            .find(|f| f.path == "META.yml")
            .expect("Should have META.yml");
        assert_eq!(meta_yml.path, "META.yml");
    }

    #[test]
    fn test_malformed_json() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "{{ invalid json").unwrap();
        temp_file.flush().unwrap();

        let package = CpanMetaJsonParser::extract_first_package(temp_file.path());

        assert_cpan_fallback_identity(&package, DatasourceId::CpanMetaJson);
    }

    #[test]
    fn test_unreadable_meta_json_preserves_fallback_identity() {
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let meta_json_dir = temp_dir.path().join("META.json");
        std::fs::create_dir(&meta_json_dir).unwrap();

        let package = CpanMetaJsonParser::extract_first_package(&meta_json_dir);

        assert_cpan_fallback_identity(&package, DatasourceId::CpanMetaJson);
    }

    #[test]
    fn test_non_object_json_root_preserves_fallback_identity() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, r#"["not", "an", "object"]"#).unwrap();
        temp_file.flush().unwrap();

        let package = CpanMetaJsonParser::extract_first_package(temp_file.path());

        assert_cpan_fallback_identity(&package, DatasourceId::CpanMetaJson);
    }

    #[test]
    fn test_malformed_yaml() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "invalid: yaml: structure: [[[").unwrap();
        temp_file.flush().unwrap();

        let package = CpanMetaYmlParser::extract_first_package(temp_file.path());

        assert_cpan_fallback_identity(&package, DatasourceId::CpanMetaYml);
    }

    #[test]
    fn test_unreadable_meta_yml_preserves_fallback_identity() {
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let meta_yml_dir = temp_dir.path().join("META.yml");
        std::fs::create_dir(&meta_yml_dir).unwrap();

        let package = CpanMetaYmlParser::extract_first_package(&meta_yml_dir);

        assert_cpan_fallback_identity(&package, DatasourceId::CpanMetaYml);
    }

    #[test]
    fn test_non_mapping_yaml_root_preserves_fallback_identity() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "- not").unwrap();
        writeln!(temp_file, "- a").unwrap();
        writeln!(temp_file, "- mapping").unwrap();
        temp_file.flush().unwrap();

        let package = CpanMetaYmlParser::extract_first_package(temp_file.path());

        assert_cpan_fallback_identity(&package, DatasourceId::CpanMetaYml);
    }

    #[test]
    fn test_unreadable_manifest_preserves_fallback_identity() {
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let manifest_dir = temp_dir.path().join("MANIFEST");
        std::fs::create_dir(&manifest_dir).unwrap();

        let package = CpanManifestParser::extract_first_package(&manifest_dir);

        assert_cpan_fallback_identity(&package, DatasourceId::CpanManifest);
        assert!(package.file_references.is_empty());
    }

    #[test]
    fn test_empty_manifest() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file).unwrap();
        writeln!(temp_file, "# Just a comment").unwrap();
        writeln!(temp_file).unwrap();
        temp_file.flush().unwrap();

        let package = CpanManifestParser::extract_first_package(temp_file.path());

        // Should have no file references
        assert_eq!(package.file_references.len(), 0);
    }

    #[test]
    fn test_manifest_with_comments() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "lib/Module.pm").unwrap();
        writeln!(temp_file, "# This is a comment").unwrap();
        writeln!(
            temp_file,
            "META.json                                Module JSON meta-data"
        )
        .unwrap();
        writeln!(temp_file).unwrap();
        writeln!(temp_file, "README.md").unwrap();
        temp_file.flush().unwrap();

        let package = CpanManifestParser::extract_first_package(temp_file.path());

        // Should have 3 file references (ignoring comment and empty line)
        assert_eq!(package.file_references.len(), 3);
        assert_eq!(package.file_references[0].path, "lib/Module.pm");
        assert_eq!(package.file_references[1].path, "META.json");
        assert_eq!(package.file_references[2].path, "README.md");
    }

    #[test]
    fn test_version_as_number_in_json() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, r#"{{"name": "Test-Module", "version": 1.5}}"#).unwrap();
        temp_file.flush().unwrap();

        let package = CpanMetaJsonParser::extract_first_package(temp_file.path());

        assert_eq!(package.name, Some("Test-Module".to_string()));
        assert_eq!(package.version, Some("1.5".to_string()));
    }

    #[test]
    fn test_license_array_in_json() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(
            temp_file,
            r#"{{"name": "Test-Module", "license": ["apache_2_0", "mit"]}}"#
        )
        .unwrap();
        temp_file.flush().unwrap();

        let package = CpanMetaJsonParser::extract_first_package(temp_file.path());

        assert_eq!(
            package.extracted_license_statement,
            Some("apache_2_0 AND mit".to_string())
        );
        assert_eq!(
            package.declared_license_expression.as_deref(),
            Some("apache-2.0 AND mit")
        );
        assert_eq!(
            package.declared_license_expression_spdx.as_deref(),
            Some("Apache-2.0 AND MIT")
        );
        assert_eq!(package.license_detections.len(), 1);
    }

    #[test]
    fn test_author_without_email() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(
            temp_file,
            r#"{{"name": "Test-Module", "author": ["John Doe"]}}"#
        )
        .unwrap();
        temp_file.flush().unwrap();

        let package = CpanMetaJsonParser::extract_first_package(temp_file.path());

        assert_eq!(package.parties.len(), 1);
        assert_eq!(package.parties[0].name, Some("John Doe".to_string()));
        assert_eq!(package.parties[0].email, None);
    }
}
