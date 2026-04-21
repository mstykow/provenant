// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use crate::models::{DatasourceId, PackageType};
use std::path::PathBuf;

use super::PackageParser;
use super::arch::{ArchPkginfoParser, ArchSrcinfoParser};

#[test]
fn test_arch_srcinfo_is_match() {
    assert!(ArchSrcinfoParser::is_match(&PathBuf::from("/tmp/.SRCINFO")));
    assert!(ArchSrcinfoParser::is_match(&PathBuf::from("/tmp/.AURINFO")));
    assert!(!ArchSrcinfoParser::is_match(&PathBuf::from(
        "/tmp/PKGBUILD"
    )));
}

#[test]
fn test_arch_pkginfo_is_match() {
    assert!(ArchPkginfoParser::is_match(&PathBuf::from("/tmp/.PKGINFO")));
    assert!(!ArchPkginfoParser::is_match(&PathBuf::from(
        "/tmp/PKGINFO.txt"
    )));
}

#[test]
fn test_parse_arch_srcinfo_basic() {
    let path = PathBuf::from("testdata/arch/srcinfo/basic/.SRCINFO");
    let pkg = ArchSrcinfoParser::extract_first_package(&path);

    assert_eq!(pkg.package_type, Some(PackageType::Alpm));
    assert_eq!(pkg.datasource_id, Some(DatasourceId::ArchSrcinfo));
    assert_eq!(pkg.namespace.as_deref(), Some("arch"));
    assert_eq!(pkg.name.as_deref(), Some("rust-basic"));
    assert_eq!(pkg.version.as_deref(), Some("1.0.0-1"));
    assert_eq!(pkg.description.as_deref(), Some("A basic Rust package"));
    assert_eq!(
        pkg.homepage_url.as_deref(),
        Some("https://github.com/example/rust-basic")
    );
    assert_eq!(pkg.extracted_license_statement.as_deref(), Some("MIT"));
    assert_eq!(
        pkg.purl.as_deref(),
        Some("pkg:alpm/arch/rust-basic@1.0.0-1?arch=x86_64")
    );
    assert_eq!(pkg.dependencies.len(), 2);
    assert!(
        pkg.dependencies
            .iter()
            .all(|dep| dep.scope.as_deref() == Some("makedepends"))
    );
    assert!(
        pkg.source_packages
            .contains(&"pkg:alpm/arch/rust-basic@1.0.0-1?arch=x86_64".to_string())
    );
}

#[test]
fn test_parse_arch_srcinfo_split_packages() {
    let path = PathBuf::from("testdata/arch/srcinfo/split/.SRCINFO");
    let pkgs = ArchSrcinfoParser::extract_packages(&path);

    assert_eq!(pkgs.len(), 2);
    assert_eq!(pkgs[0].name.as_deref(), Some("rust-split-bin"));
    assert_eq!(pkgs[1].name.as_deref(), Some("rust-split-lib"));
    assert_eq!(pkgs[0].description.as_deref(), Some("Binary package"));
    assert_eq!(pkgs[1].description.as_deref(), Some("Library package"));
    assert!(
        pkgs[0]
            .dependencies
            .iter()
            .any(|dep| dep.purl.as_deref() == Some("pkg:alpm/arch/glibc"))
    );
    assert!(
        pkgs[1]
            .dependencies
            .iter()
            .any(|dep| dep.purl.as_deref() == Some("pkg:alpm/arch/gcc-libs"))
    );
    assert!(pkgs.iter().all(|pkg| {
        pkg.source_packages
            .contains(&"pkg:alpm/arch/rust-split@1.0.0-1?arch=x86_64".to_string())
    }));
}

#[test]
fn test_parse_arch_srcinfo_arch_specific_and_epoch() {
    let path = PathBuf::from("testdata/arch/srcinfo/arch-specific/.SRCINFO");
    let pkg = ArchSrcinfoParser::extract_first_package(&path);

    assert_eq!(pkg.version.as_deref(), Some("1:1.5.0-2"));
    assert_eq!(pkg.datasource_id, Some(DatasourceId::ArchSrcinfo));
    assert_eq!(
        pkg.purl.as_deref(),
        Some("pkg:alpm/arch/rust-multiarch@1:1.5.0-2")
    );
    assert!(
        pkg.dependencies
            .iter()
            .any(|dep| dep.scope.as_deref() == Some("depends"))
    );
    assert!(
        pkg.dependencies
            .iter()
            .any(|dep| dep.scope.as_deref() == Some("depends_x86_64"))
    );
    assert!(
        pkg.dependencies
            .iter()
            .any(|dep| dep.scope.as_deref() == Some("depends_aarch64"))
    );
    assert!(
        pkg.dependencies
            .iter()
            .any(|dep| dep.scope.as_deref() == Some("checkdepends"))
    );
    let extra = pkg.extra_data.as_ref().expect("extra data should exist");
    assert_eq!(
        extra.get("arch"),
        Some(&serde_json::json!(["x86_64", "aarch64"]))
    );
}

#[test]
fn test_parse_arch_aurinfo_alias() {
    let path = PathBuf::from("testdata/arch/srcinfo/legacy/.AURINFO");
    let pkg = ArchSrcinfoParser::extract_first_package(&path);

    assert_eq!(pkg.datasource_id, Some(DatasourceId::ArchAurinfo));
    assert_eq!(pkg.name.as_deref(), Some("aur-legacy"));
    assert_eq!(pkg.version.as_deref(), Some("0.9.0-4"));
    assert_eq!(
        pkg.purl.as_deref(),
        Some("pkg:alpm/arch/aur-legacy@0.9.0-4?arch=any")
    );
}

#[test]
fn test_parse_arch_pkginfo_basic() {
    let path = PathBuf::from("testdata/arch/pkginfo/basic/.PKGINFO");
    let pkg = ArchPkginfoParser::extract_first_package(&path);

    assert_eq!(pkg.package_type, Some(PackageType::Alpm));
    assert_eq!(pkg.datasource_id, Some(DatasourceId::ArchPkginfo));
    assert_eq!(pkg.namespace.as_deref(), Some("arch"));
    assert_eq!(pkg.name.as_deref(), Some("python-construct"));
    assert_eq!(pkg.version.as_deref(), Some("1:2.10.68-3"));
    assert_eq!(
        pkg.description.as_deref(),
        Some("Parsing library sample package")
    );
    assert_eq!(
        pkg.homepage_url.as_deref(),
        Some("https://github.com/construct/construct")
    );
    assert_eq!(pkg.extracted_license_statement.as_deref(), Some("MIT"));
    assert_eq!(
        pkg.purl.as_deref(),
        Some("pkg:alpm/arch/python-construct@1:2.10.68-3?arch=x86_64")
    );
    assert_eq!(pkg.parties.len(), 1);
    assert_eq!(pkg.parties[0].role.as_deref(), Some("packager"));
    assert_eq!(pkg.parties[0].name.as_deref(), Some("Max Example"));
    assert_eq!(pkg.parties[0].email.as_deref(), Some("max@example.com"));

    assert!(
        pkg.dependencies
            .iter()
            .any(|dep| dep.scope.as_deref() == Some("depend")
                && dep.purl.as_deref() == Some("pkg:alpm/arch/python"))
    );
    assert!(
        pkg.dependencies
            .iter()
            .any(|dep| dep.scope.as_deref() == Some("makedepend"))
    );
    assert!(
        pkg.dependencies
            .iter()
            .any(|dep| dep.scope.as_deref() == Some("checkdepend"))
    );
    assert!(
        pkg.dependencies
            .iter()
            .any(|dep| dep.scope.as_deref() == Some("optdepend") && dep.is_optional == Some(true))
    );

    let extra = pkg.extra_data.as_ref().expect("extra data should exist");
    assert_eq!(pkg.size, Some(123456));
    assert_eq!(
        extra.get("provides"),
        Some(&serde_json::json!(["python-construct-legacy"]))
    );
    assert!(
        pkg.source_packages
            .contains(&"pkg:alpm/arch/python-construct@1:2.10.68-3?arch=x86_64".to_string())
    );
}
