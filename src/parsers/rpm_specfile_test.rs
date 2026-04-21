// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use crate::models::PackageType;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

use super::PackageParser;
use super::rpm_specfile::RpmSpecfileParser;
use crate::models::DatasourceId;

fn assert_fixture_exists(path: &std::path::Path) {
    assert!(path.exists(), "missing fixture: {}", path.display());
}

#[test]
fn test_is_match_positive() {
    assert!(RpmSpecfileParser::is_match(&PathBuf::from("cpio.spec")));
    assert!(RpmSpecfileParser::is_match(&PathBuf::from(
        "openssl_package.spec"
    )));
    assert!(RpmSpecfileParser::is_match(&PathBuf::from(
        "/usr/src/rpm/SPECS/mypackage.spec"
    )));
}

#[test]
fn test_is_match_negative() {
    assert!(!RpmSpecfileParser::is_match(&PathBuf::from("package.rpm")));
    assert!(!RpmSpecfileParser::is_match(&PathBuf::from(
        "package.tar.gz"
    )));
    assert!(!RpmSpecfileParser::is_match(&PathBuf::from("README.md")));
    assert!(!RpmSpecfileParser::is_match(&PathBuf::from("spec.txt")));
}

#[test]
fn test_parse_minimal_spec() {
    let test_file = PathBuf::from("testdata/rpm/specfile/minimal.spec");
    assert_fixture_exists(&test_file);

    let pkg = RpmSpecfileParser::extract_first_package(&test_file);

    assert_eq!(pkg.package_type, Some(PackageType::Rpm));
    assert_eq!(pkg.datasource_id, Some(DatasourceId::RpmSpecfile));
    assert_eq!(pkg.name, Some("minimal-pkg".to_string()));
    assert_eq!(pkg.version, Some("1.0".to_string()));
    assert_eq!(pkg.extracted_license_statement, Some("MIT".to_string()));
    assert!(pkg.description.is_some());
    assert!(pkg.description.as_ref().unwrap().contains("Minimal"));

    // Check PURL
    assert!(pkg.purl.is_some());
    assert_eq!(pkg.purl, Some("pkg:rpm/minimal-pkg@1.0".to_string()));
}

#[test]
fn test_parse_cpio_spec() {
    let test_file = PathBuf::from("testdata/rpm/specfile/cpio.spec");
    assert_fixture_exists(&test_file);

    let pkg = RpmSpecfileParser::extract_first_package(&test_file);

    assert_eq!(pkg.package_type, Some(PackageType::Rpm));
    assert_eq!(pkg.name, Some("cpio".to_string()));
    assert_eq!(pkg.version, Some("2.9".to_string()));
    assert_eq!(pkg.extracted_license_statement, Some("GPLv3+".to_string()));
    assert_eq!(
        pkg.homepage_url,
        Some("http://www.gnu.org/software/cpio/".to_string())
    );

    // Check release in extra_data
    assert!(pkg.extra_data.is_some());
    let extra = pkg.extra_data.as_ref().unwrap();
    assert!(extra.contains_key("release"));

    // Check group in extra_data
    assert!(extra.contains_key("group"));
    let group = extra.get("group").unwrap();
    assert_eq!(group.as_str(), Some("Applications/Archiving"));

    // Check description
    assert!(pkg.description.is_some());
    let desc = pkg.description.as_ref().unwrap();
    assert!(desc.contains("GNU cpio copies files"));

    // Check download URL (should have macro expanded)
    assert!(pkg.download_url.is_some());
    let download = pkg.download_url.as_ref().unwrap();
    assert!(download.contains("cpio-2.9.tar.gz"));

    // Check dependencies
    assert!(!pkg.dependencies.is_empty());

    // Find BuildRequires
    let build_deps: Vec<_> = pkg
        .dependencies
        .iter()
        .filter(|d| d.scope == Some("build".to_string()))
        .collect();
    assert!(build_deps.len() >= 3); // texinfo, autoconf, gettext

    // Check that BuildRequires have is_runtime=false
    for dep in &build_deps {
        assert_eq!(dep.is_runtime, Some(false));
    }

    // Find Requires
    let runtime_deps: Vec<_> = pkg
        .dependencies
        .iter()
        .filter(|d| d.is_runtime == Some(true))
        .collect();
    assert!(runtime_deps.len() >= 2); // Requires(post) and Requires(preun)

    // Check scoped Requires
    let post_deps: Vec<_> = pkg
        .dependencies
        .iter()
        .filter(|d| d.scope == Some("post".to_string()))
        .collect();
    assert!(!post_deps.is_empty());

    let preun_deps: Vec<_> = pkg
        .dependencies
        .iter()
        .filter(|d| d.scope == Some("preun".to_string()))
        .collect();
    assert!(!preun_deps.is_empty());
}

#[test]
fn test_parse_openssl_spec() {
    let test_file = PathBuf::from("testdata/rpm/specfile/openssl.spec");
    assert_fixture_exists(&test_file);

    let pkg = RpmSpecfileParser::extract_first_package(&test_file);

    assert_eq!(pkg.package_type, Some(PackageType::Rpm));
    assert_eq!(pkg.name, Some("openssl".to_string()));
    assert_eq!(pkg.version, Some("1.0.2e".to_string()));
    assert_eq!(pkg.extracted_license_statement, Some("OpenSSL".to_string()));
    assert_eq!(
        pkg.homepage_url,
        Some("http://www.openssl.org/".to_string())
    );

    // Check Packager
    assert!(!pkg.parties.is_empty());
    let packager = pkg
        .parties
        .iter()
        .find(|p| p.role == Some("packager".to_string()));
    assert!(packager.is_some());
    let packager = packager.unwrap();
    assert_eq!(packager.name, Some("Damien Miller".to_string()));
    assert_eq!(packager.email, Some("djm@mindrot.org".to_string()));

    // Check Provides in extra_data
    assert!(pkg.extra_data.is_some());
    let extra = pkg.extra_data.as_ref().unwrap();
    assert!(extra.contains_key("provides"));

    // Check download URL with macro expansion
    assert!(pkg.download_url.is_some());
    let download = pkg.download_url.as_ref().unwrap();
    assert!(download.contains("openssl-1.0.2e.tar.gz"));

    // Check description (should be full description, not summary)
    assert!(pkg.description.is_some());
    let desc = pkg.description.as_ref().unwrap();
    assert!(desc.contains("OpenSSL Project"));
}

#[test]
fn test_macro_expansion() {
    let test_file = PathBuf::from("testdata/rpm/specfile/cpio.spec");
    assert_fixture_exists(&test_file);

    let pkg = RpmSpecfileParser::extract_first_package(&test_file);

    // The Release field should have %{?dist} stripped
    if let Some(extra) = pkg.extra_data
        && let Some(release) = extra.get("release")
    {
        let release_str = release.as_str().unwrap();
        // Should not contain %{?dist}
        assert!(!release_str.contains("%{?dist}"));
    }
}

#[test]
fn test_comma_separated_buildrequires() {
    let test_file = PathBuf::from("testdata/rpm/specfile/cpio.spec");
    assert_fixture_exists(&test_file);

    let pkg = RpmSpecfileParser::extract_first_package(&test_file);

    // BuildRequires: texinfo, autoconf, gettext
    let build_deps: Vec<_> = pkg
        .dependencies
        .iter()
        .filter(|d| d.scope == Some("build".to_string()))
        .collect();

    let has_texinfo = build_deps
        .iter()
        .any(|d| d.extracted_requirement.as_deref() == Some("texinfo"));
    let has_autoconf = build_deps
        .iter()
        .any(|d| d.extracted_requirement.as_deref() == Some("autoconf"));
    let has_gettext = build_deps
        .iter()
        .any(|d| d.extracted_requirement.as_deref() == Some("gettext"));

    assert!(has_texinfo);
    assert!(has_autoconf);
    assert!(has_gettext);
}

#[test]
fn test_purl_generation() {
    let test_file = PathBuf::from("testdata/rpm/specfile/minimal.spec");
    assert_fixture_exists(&test_file);

    let pkg = RpmSpecfileParser::extract_first_package(&test_file);

    assert!(pkg.purl.is_some());
    let purl = pkg.purl.as_ref().unwrap();

    // Should start with pkg:rpm/
    assert!(purl.starts_with("pkg:rpm/"));

    // Should contain package name
    assert!(purl.contains("minimal-pkg"));

    // Should contain version
    assert!(purl.contains("@1.0"));
}

#[test]
fn test_description_overrides_summary() {
    let test_file = PathBuf::from("testdata/rpm/specfile/cpio.spec");
    assert_fixture_exists(&test_file);

    let pkg = RpmSpecfileParser::extract_first_package(&test_file);

    // Should use %description content, not Summary tag
    assert!(pkg.description.is_some());
    let desc = pkg.description.as_ref().unwrap();

    // Description is multi-line from %description section
    assert!(desc.len() > 50); // More than just the summary
    assert!(desc.contains("GNU cpio copies files"));
}

#[test]
fn test_conditional_preamble_keeps_parsing_later_tags_and_deps() {
    let temp_dir = TempDir::new().unwrap();
    let spec_path = temp_dir.path().join("conditional.spec");

    let spec = r#"
Name: conditional-pkg
Version: 1.0
Release: 1%{?dist}

%if 0%{?fedora}
Summary: Fedora summary
BuildRequires: gcc
%endif

License: MIT
Requires: bash

%description
Example package
"#;

    fs::write(&spec_path, spec).unwrap();
    let pkg = RpmSpecfileParser::extract_first_package(&spec_path);

    assert_eq!(pkg.name.as_deref(), Some("conditional-pkg"));
    assert_eq!(pkg.version.as_deref(), Some("1.0"));
    assert_eq!(pkg.extracted_license_statement.as_deref(), Some("MIT"));
    assert!(
        pkg.dependencies
            .iter()
            .any(|dep| dep.extracted_requirement.as_deref() == Some("bash"))
    );
}

#[test]
fn test_dependency_macros_are_expanded_in_requires_and_provides() {
    let temp_dir = TempDir::new().unwrap();
    let spec_path = temp_dir.path().join("macros.spec");

    let spec = r#"
Name: macro-pkg
Version: 2.4
Release: 3%{?dist}
Summary: Macro package
License: MIT
Requires: %{name} = %{version}-%{release}
BuildRequires: pkgconfig(%{name})
Provides: %{name}-libs = %{version}

%description
Macro package
"#;

    fs::write(&spec_path, spec).unwrap();
    let pkg = RpmSpecfileParser::extract_first_package(&spec_path);

    let req = pkg
        .dependencies
        .iter()
        .find(|dep| dep.scope.as_deref() == Some("runtime"))
        .unwrap();
    assert_eq!(
        req.extracted_requirement.as_deref(),
        Some("macro-pkg = 2.4-3")
    );
    assert_eq!(req.purl.as_deref(), Some("pkg:rpm/macro-pkg"));

    let build = pkg
        .dependencies
        .iter()
        .find(|dep| dep.scope.as_deref() == Some("build"))
        .unwrap();
    assert_eq!(
        build.extracted_requirement.as_deref(),
        Some("pkgconfig(macro-pkg)")
    );

    let provides = pkg
        .extra_data
        .as_ref()
        .and_then(|extra| extra.get("provides"))
        .and_then(|value| value.as_array())
        .unwrap();
    assert_eq!(provides[0].as_str(), Some("macro-pkg-libs = 2.4"));
}
