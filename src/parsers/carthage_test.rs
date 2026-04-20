use super::PackageParser;
use super::carthage::{CarthageCartfileParser, CarthageCartfileResolvedParser};
use crate::models::{DatasourceId, PackageType};
use std::path::Path;

#[test]
fn test_cartfile_is_match() {
    assert!(CarthageCartfileParser::is_match(Path::new("Cartfile")));
    assert!(CarthageCartfileParser::is_match(Path::new(
        "/some/path/Cartfile"
    )));
    assert!(CarthageCartfileParser::is_match(Path::new(
        "Cartfile.private"
    )));
    assert!(!CarthageCartfileParser::is_match(Path::new(
        "Cartfile.resolved"
    )));
    assert!(!CarthageCartfileParser::is_match(Path::new("cartfile")));
    assert!(!CarthageCartfileParser::is_match(Path::new("package.json")));
}

#[test]
fn test_cartfile_resolved_is_match() {
    assert!(CarthageCartfileResolvedParser::is_match(Path::new(
        "Cartfile.resolved"
    )));
    assert!(CarthageCartfileResolvedParser::is_match(Path::new(
        "/some/path/Cartfile.resolved"
    )));
    assert!(!CarthageCartfileResolvedParser::is_match(Path::new(
        "Cartfile"
    )));
    assert!(!CarthageCartfileResolvedParser::is_match(Path::new(
        "Cartfile.private"
    )));
}

#[test]
fn test_cartfile_extract_packages() {
    let path = Path::new("testdata/carthage/Cartfile");
    let packages = CarthageCartfileParser::extract_packages(path);

    assert_eq!(packages.len(), 1);
    let pkg = &packages[0];
    assert_eq!(pkg.package_type, Some(PackageType::Carthage));
    assert_eq!(pkg.datasource_id, Some(DatasourceId::CarthageCartfile));
    assert_eq!(pkg.primary_language.as_deref(), Some("Objective-C"));

    assert_eq!(pkg.dependencies.len(), 7);

    // github with >= requirement
    let dep0 = &pkg.dependencies[0];
    assert_eq!(
        dep0.purl.as_deref(),
        Some("pkg:github/reactivecocoa/reactivecocoa")
    );
    assert_eq!(dep0.extracted_requirement.as_deref(), Some(">= 2.3.1"));
    assert_eq!(dep0.is_direct, Some(true));
    assert_eq!(dep0.is_pinned, None);

    // github with ~> requirement
    let dep1 = &pkg.dependencies[1];
    assert_eq!(dep1.purl.as_deref(), Some("pkg:github/mantle/mantle"));
    assert_eq!(dep1.extracted_requirement.as_deref(), Some("~> 1.0"));

    // github with == requirement
    let dep2 = &pkg.dependencies[2];
    assert_eq!(
        dep2.purl.as_deref(),
        Some("pkg:github/jspahrsummers/libextobjc")
    );
    assert_eq!(dep2.extracted_requirement.as_deref(), Some("== 0.4.1"));

    // github with no requirement
    let dep3 = &pkg.dependencies[3];
    assert_eq!(
        dep3.purl.as_deref(),
        Some("pkg:github/jspahrsummers/xcconfigs")
    );
    assert_eq!(dep3.extracted_requirement, None);

    // git dependencies have no purl
    let dep4 = &pkg.dependencies[4];
    assert_eq!(dep4.purl, None);
    let name4 = dep4
        .extra_data
        .as_ref()
        .and_then(|m| m.get("name"))
        .and_then(|v| v.as_str());
    assert_eq!(name4, Some("git-error-translations2"));

    // git with branch spec
    let dep5 = &pkg.dependencies[5];
    assert_eq!(dep5.purl, None);
    assert_eq!(dep5.extracted_requirement.as_deref(), Some("development"));
    let name5 = dep5
        .extra_data
        .as_ref()
        .and_then(|m| m.get("name"))
        .and_then(|v| v.as_str());
    assert_eq!(name5, Some("FReactiveSwift"));

    // binary dependency
    let dep6 = &pkg.dependencies[6];
    assert_eq!(dep6.purl, None);
    assert_eq!(dep6.extracted_requirement.as_deref(), Some("~> 2.3"));
    let name6 = dep6
        .extra_data
        .as_ref()
        .and_then(|m| m.get("name"))
        .and_then(|v| v.as_str());
    assert_eq!(name6, Some("MyFramework"));
}

#[test]
fn test_cartfile_resolved_extract_packages() {
    let path = Path::new("testdata/carthage/Cartfile.resolved");
    let packages = CarthageCartfileResolvedParser::extract_packages(path);

    assert_eq!(packages.len(), 1);
    let pkg = &packages[0];
    assert_eq!(pkg.package_type, Some(PackageType::Carthage));
    assert_eq!(
        pkg.datasource_id,
        Some(DatasourceId::CarthageCartfileResolved)
    );

    assert_eq!(pkg.dependencies.len(), 4);

    let dep0 = &pkg.dependencies[0];
    assert_eq!(
        dep0.purl.as_deref(),
        Some("pkg:github/reactivecocoa/reactivecocoa@v2.3.1")
    );
    assert_eq!(dep0.extracted_requirement.as_deref(), Some("v2.3.1"));
    assert_eq!(dep0.is_pinned, Some(true));

    let dep1 = &pkg.dependencies[1];
    assert_eq!(dep1.purl.as_deref(), Some("pkg:github/mantle/mantle@1.5.8"));
    assert_eq!(dep1.extracted_requirement.as_deref(), Some("1.5.8"));
    assert_eq!(dep1.is_pinned, Some(true));
}

#[test]
fn test_cartfile_private_extract_packages() {
    let path = Path::new("testdata/carthage/Cartfile.private");
    let packages = CarthageCartfileParser::extract_packages(path);

    assert_eq!(packages.len(), 1);
    let pkg = &packages[0];
    assert_eq!(pkg.dependencies.len(), 2);

    let dep0 = &pkg.dependencies[0];
    assert_eq!(dep0.purl.as_deref(), Some("pkg:github/quick/quick"));
    assert_eq!(dep0.extracted_requirement.as_deref(), Some("~> 1.0"));

    let dep1 = &pkg.dependencies[1];
    assert_eq!(dep1.purl.as_deref(), Some("pkg:github/quick/nimble"));
    assert_eq!(dep1.extracted_requirement.as_deref(), Some("~> 7.0"));
}

#[test]
fn test_cartfile_empty_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Cartfile");
    std::fs::write(&path, "# Just a comment\n\n").unwrap();

    let packages = CarthageCartfileParser::extract_packages(&path);
    assert_eq!(packages.len(), 1);
    assert!(packages[0].dependencies.is_empty());
}

#[test]
fn test_cartfile_malformed_lines() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Cartfile");
    std::fs::write(
        &path,
        "invalid line\ngithub \"Valid/Repo\" ~> 1.0\nalso invalid\n",
    )
    .unwrap();

    let packages = CarthageCartfileParser::extract_packages(&path);
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].dependencies.len(), 1);
    assert_eq!(
        packages[0].dependencies[0].purl.as_deref(),
        Some("pkg:github/valid/repo")
    );
}

#[test]
fn test_cartfile_nonexistent_file() {
    let path = Path::new("testdata/carthage/nonexistent/Cartfile");
    let packages = CarthageCartfileParser::extract_packages(path);

    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package_type, Some(PackageType::Carthage));
    assert_eq!(
        packages[0].datasource_id,
        Some(DatasourceId::CarthageCartfile)
    );
}
