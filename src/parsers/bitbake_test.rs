use super::PackageParser;
use super::bitbake::BitbakeRecipeParser;
use crate::models::{DatasourceId, PackageType};
use std::path::Path;

#[test]
fn test_is_match() {
    assert!(BitbakeRecipeParser::is_match(Path::new("recipe_1.0.bb")));
    assert!(BitbakeRecipeParser::is_match(Path::new(
        "/some/path/busybox_1.36.1.bb"
    )));
    assert!(BitbakeRecipeParser::is_match(Path::new("simple.bb")));
    assert!(!BitbakeRecipeParser::is_match(Path::new("recipe.bbappend")));
    assert!(!BitbakeRecipeParser::is_match(Path::new("base.bbclass")));
    assert!(!BitbakeRecipeParser::is_match(Path::new("local.conf")));
    assert!(!BitbakeRecipeParser::is_match(Path::new("package.json")));
}

#[test]
fn test_extract_packages_basic() {
    let path = Path::new("testdata/bitbake/example_1.2.3.bb");
    let packages = BitbakeRecipeParser::extract_packages(path);

    assert_eq!(packages.len(), 1);
    let pkg = &packages[0];
    assert_eq!(pkg.package_type, Some(PackageType::Bitbake));
    assert_eq!(pkg.datasource_id, Some(DatasourceId::BitbakeRecipe));
    assert_eq!(pkg.name.as_deref(), Some("example"));
    assert_eq!(pkg.version.as_deref(), Some("1.2.3"));
    assert_eq!(
        pkg.description.as_deref(),
        Some("Example application for testing")
    );
    assert_eq!(
        pkg.homepage_url.as_deref(),
        Some("https://example.com/project")
    );
    assert_eq!(
        pkg.bug_tracking_url.as_deref(),
        Some("https://example.com/bugs")
    );
    assert_eq!(pkg.purl.as_deref(), Some("pkg:bitbake/example@1.2.3"));

    assert_eq!(pkg.extracted_license_statement.as_deref(), Some("MIT"));

    let extra = pkg.extra_data.as_ref().unwrap();
    assert_eq!(extra.get("section").and_then(|v| v.as_str()), Some("devel"));

    let src_uris = extra.get("src_uri").and_then(|v| v.as_array()).unwrap();
    assert_eq!(src_uris.len(), 1);
    assert_eq!(
        src_uris[0].as_str(),
        Some("https://example.com/releases/example-${PV}.tar.gz")
    );

    let inherits = extra.get("inherit").and_then(|v| v.as_array()).unwrap();
    assert_eq!(inherits.len(), 2);
    assert_eq!(inherits[0].as_str(), Some("autotools"));
    assert_eq!(inherits[1].as_str(), Some("pkgconfig"));

    let build_deps: Vec<_> = pkg
        .dependencies
        .iter()
        .filter(|d| d.scope.as_deref() == Some("build"))
        .collect();
    assert_eq!(build_deps.len(), 2);
    assert_eq!(build_deps[0].purl.as_deref(), Some("pkg:bitbake/zlib"));
    assert_eq!(build_deps[1].purl.as_deref(), Some("pkg:bitbake/openssl"));
    assert_eq!(build_deps[0].is_runtime, Some(false));
    assert_eq!(build_deps[0].is_direct, Some(true));

    let runtime_deps: Vec<_> = pkg
        .dependencies
        .iter()
        .filter(|d| d.scope.as_deref() == Some("runtime"))
        .collect();
    assert_eq!(runtime_deps.len(), 2);
    assert_eq!(runtime_deps[0].purl.as_deref(), Some("pkg:bitbake/libz"));
    assert_eq!(runtime_deps[0].is_runtime, Some(true));
}

#[test]
fn test_extract_packages_no_version_in_filename() {
    let path = Path::new("testdata/bitbake/simple.bb");
    let packages = BitbakeRecipeParser::extract_packages(path);

    assert_eq!(packages.len(), 1);
    let pkg = &packages[0];
    assert_eq!(pkg.name.as_deref(), Some("simple"));
    assert_eq!(pkg.version, None);
    assert_eq!(pkg.purl.as_deref(), Some("pkg:bitbake/simple"));
    assert_eq!(
        pkg.extracted_license_statement.as_deref(),
        Some("GPL-2.0-only")
    );
}

#[test]
fn test_extract_packages_empty_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty_1.0.bb");
    std::fs::write(&path, "# Just a comment\n\n").unwrap();

    let packages = BitbakeRecipeParser::extract_packages(&path);
    assert_eq!(packages.len(), 1);
    let pkg = &packages[0];
    assert_eq!(pkg.name.as_deref(), Some("empty"));
    assert_eq!(pkg.version.as_deref(), Some("1.0"));
    assert!(pkg.dependencies.is_empty());
}

#[test]
fn test_extract_packages_weak_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test_2.0.bb");
    std::fs::write(
        &path,
        "HOMEPAGE ?= \"https://default.example.com\"\n\
         HOMEPAGE = \"https://actual.example.com\"\n\
         SUMMARY ??= \"Weak default summary\"\n",
    )
    .unwrap();

    let packages = BitbakeRecipeParser::extract_packages(&path);
    assert_eq!(packages.len(), 1);
    let pkg = &packages[0];
    assert_eq!(
        pkg.homepage_url.as_deref(),
        Some("https://actual.example.com")
    );
    assert_eq!(pkg.description.as_deref(), Some("Weak default summary"));
}

#[test]
fn test_extract_packages_append_operators() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test_1.0.bb");
    std::fs::write(
        &path,
        "DEPENDS = \"zlib\"\n\
         DEPENDS += \"openssl\"\n",
    )
    .unwrap();

    let packages = BitbakeRecipeParser::extract_packages(&path);
    let pkg = &packages[0];
    let build_deps: Vec<_> = pkg
        .dependencies
        .iter()
        .filter(|d| d.scope.as_deref() == Some("build"))
        .collect();
    assert_eq!(build_deps.len(), 2);
}

#[test]
fn test_extract_packages_legacy_rdepends_syntax() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test_1.0.bb");
    std::fs::write(&path, "RDEPENDS_${PN} = \"libfoo libbar\"\n").unwrap();

    let packages = BitbakeRecipeParser::extract_packages(&path);
    let pkg = &packages[0];
    let runtime_deps: Vec<_> = pkg
        .dependencies
        .iter()
        .filter(|d| d.scope.as_deref() == Some("runtime"))
        .collect();
    assert_eq!(runtime_deps.len(), 2);
    assert_eq!(runtime_deps[0].purl.as_deref(), Some("pkg:bitbake/libfoo"));
}

#[test]
fn test_extract_packages_nonexistent_file() {
    let path = Path::new("testdata/bitbake/nonexistent/recipe_1.0.bb");
    let packages = BitbakeRecipeParser::extract_packages(path);

    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package_type, Some(PackageType::Bitbake));
    assert_eq!(packages[0].datasource_id, Some(DatasourceId::BitbakeRecipe));
}

#[test]
fn test_variable_references_in_deps_are_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test_1.0.bb");
    std::fs::write(&path, "DEPENDS = \"zlib ${EXTRA_DEPENDS}\"\n").unwrap();

    let packages = BitbakeRecipeParser::extract_packages(&path);
    let pkg = &packages[0];
    assert_eq!(pkg.dependencies.len(), 1);
    assert_eq!(
        pkg.dependencies[0].purl.as_deref(),
        Some("pkg:bitbake/zlib")
    );
}

#[test]
fn test_license_with_operators() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test_1.0.bb");
    std::fs::write(&path, "LICENSE = \"GPL-2.0-only & MIT\"\n").unwrap();

    let packages = BitbakeRecipeParser::extract_packages(&path);
    let pkg = &packages[0];
    assert_eq!(
        pkg.extracted_license_statement.as_deref(),
        Some("GPL-2.0-only AND MIT")
    );
}
