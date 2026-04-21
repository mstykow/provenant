// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::PackageParser;
use super::bitbake::BitbakeRecipeParser;
use crate::models::{DatasourceId, PackageType, Sha256Digest};
use std::path::Path;

#[test]
fn test_is_match() {
    assert!(BitbakeRecipeParser::is_match(Path::new("recipe_1.0.bb")));
    assert!(BitbakeRecipeParser::is_match(Path::new(
        "/some/path/busybox_1.36.1.bb"
    )));
    assert!(BitbakeRecipeParser::is_match(Path::new("simple.bb")));
    assert!(BitbakeRecipeParser::is_match(Path::new("recipe.bbappend")));
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
    assert_eq!(
        pkg.download_url.as_deref(),
        Some("https://example.com/releases/example-${PV}.tar.gz")
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
}

#[test]
fn test_extract_packages_basic_file_references_and_dependencies() {
    let path = Path::new("testdata/bitbake/example_1.2.3.bb");
    let pkg = &BitbakeRecipeParser::extract_packages(path)[0];

    assert_eq!(pkg.file_references.len(), 2);
    assert_eq!(pkg.file_references[0].path, "LICENSE");
    assert!(pkg.file_references[0].md5.is_none());
    assert_eq!(pkg.file_references[1].path, "fix-build.patch");

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
fn test_extract_packages_legacy_append_operators() {
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
fn test_extract_packages_supports_override_style_operators() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("example_1.0.bbappend");
    std::fs::write(
        &path,
        "DEPENDS:append = \" openssl\"\n\
         DEPENDS:prepend = \"libxml2 \"\n\
         DEPENDS:remove = \"zlib\"\n\
         DEPENDS = \"zlib\"\n\
         RDEPENDS:${PN}:append = \" libfoo\"\n",
    )
    .unwrap();

    let packages = BitbakeRecipeParser::extract_packages(&path);
    let pkg = &packages[0];
    assert_eq!(pkg.datasource_id, Some(DatasourceId::BitbakeRecipeAppend));

    let build_deps: Vec<_> = pkg
        .dependencies
        .iter()
        .filter(|d| d.scope.as_deref() == Some("build"))
        .collect();
    let build_purls: Vec<_> = build_deps
        .iter()
        .filter_map(|dep| dep.purl.as_deref())
        .collect();
    assert_eq!(
        build_purls,
        vec!["pkg:bitbake/libxml2", "pkg:bitbake/openssl"]
    );

    let runtime_deps: Vec<_> = pkg
        .dependencies
        .iter()
        .filter(|d| d.scope.as_deref() == Some("runtime"))
        .collect();
    assert_eq!(runtime_deps.len(), 1);
    assert_eq!(runtime_deps[0].purl.as_deref(), Some("pkg:bitbake/libfoo"));
}

#[test]
fn test_extract_packages_supports_legacy_rdepends_syntax() {
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
fn test_extract_packages_bbappend_wildcard_filename_keeps_name() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("busybox_%.bbappend");
    std::fs::write(&path, "SUMMARY = \"BusyBox append\"\n").unwrap();

    let packages = BitbakeRecipeParser::extract_packages(&path);
    let pkg = &packages[0];
    assert_eq!(pkg.datasource_id, Some(DatasourceId::BitbakeRecipeAppend));
    assert_eq!(pkg.name.as_deref(), Some("busybox"));
    assert_eq!(pkg.version, None);
}

#[test]
fn test_extract_packages_bbappend_trims_trailing_wildcard_without_version_segment() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("u-boot%.bbappend");
    std::fs::write(&path, "SUMMARY = \"U-Boot append\"\n").unwrap();

    let packages = BitbakeRecipeParser::extract_packages(&path);
    let pkg = &packages[0];
    assert_eq!(pkg.datasource_id, Some(DatasourceId::BitbakeRecipeAppend));
    assert_eq!(pkg.name.as_deref(), Some("u-boot"));
    assert_eq!(pkg.version, None);
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
fn test_license_with_operators_preserves_raw_statement() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test_1.0.bb");
    std::fs::write(&path, "LICENSE = \"GPL-2.0-only & MIT\"\n").unwrap();

    let packages = BitbakeRecipeParser::extract_packages(&path);
    let pkg = &packages[0];
    assert_eq!(
        pkg.extracted_license_statement.as_deref(),
        Some("GPL-2.0-only & MIT")
    );
    assert_eq!(
        pkg.declared_license_expression_spdx.as_deref(),
        Some("GPL-2.0-only AND MIT")
    );
}

#[test]
fn test_package_specific_license_overrides_plain_license() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("example_1.0.bb");
    std::fs::write(
        &path,
        "PN = \"example\"\n\
         LICENSE = \"MIT\"\n\
         LICENSE:${PN} = \"GPL-2.0-only\"\n",
    )
    .unwrap();

    let packages = BitbakeRecipeParser::extract_packages(&path);
    let pkg = &packages[0];
    assert_eq!(
        pkg.extracted_license_statement.as_deref(),
        Some("GPL-2.0-only")
    );
    assert_eq!(
        pkg.declared_license_expression_spdx.as_deref(),
        Some("GPL-2.0-only")
    );
}

#[test]
fn test_single_remote_src_uri_sets_download_url_and_checksums() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("example_1.0.bb");
    std::fs::write(
        &path,
        "SRC_URI = \"https://example.com/example-1.0.tar.gz;sha256sum=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"\n",
    )
    .unwrap();

    let packages = BitbakeRecipeParser::extract_packages(&path);
    let pkg = &packages[0];
    assert_eq!(
        pkg.download_url.as_deref(),
        Some("https://example.com/example-1.0.tar.gz")
    );
    assert_eq!(
        pkg.sha256,
        Some(
            Sha256Digest::from_hex(
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            )
            .unwrap()
        )
    );
}

#[test]
fn test_named_src_uri_varflag_sets_checksum_for_single_remote_uri() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("example_1.0.bb");
    std::fs::write(
        &path,
        "SRC_URI = \"https://example.com/example-1.0.tar.gz;name=tarball\"\n\
         SRC_URI[tarball.sha256sum] = \"abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd\"\n",
    )
    .unwrap();

    let packages = BitbakeRecipeParser::extract_packages(&path);
    let pkg = &packages[0];
    assert_eq!(
        pkg.download_url.as_deref(),
        Some("https://example.com/example-1.0.tar.gz")
    );
    assert_eq!(
        pkg.sha256,
        Some(
            Sha256Digest::from_hex(
                "abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd"
            )
            .unwrap()
        )
    );
}

#[test]
fn test_multiple_remote_src_uri_entries_do_not_guess_single_download_url() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("example_1.0.bb");
    std::fs::write(
        &path,
        "SRC_URI = \"https://example.com/source.tar.gz https://example.com/fix.patch\"\n\
         SRC_URI[sha256sum] = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"\n",
    )
    .unwrap();

    let packages = BitbakeRecipeParser::extract_packages(&path);
    let pkg = &packages[0];
    assert_eq!(pkg.download_url, None);
    assert_eq!(pkg.sha256, None);
}

#[test]
fn test_shell_function_local_assignment_with_unmatched_quote_does_not_panic() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("example_1.0.bb");
    std::fs::write(
        &path,
        "SUMMARY = \"Example\"\n\
         do_install() {\n\
             files=\"\n\
             echo done\n\
         }\n",
    )
    .unwrap();

    let packages = BitbakeRecipeParser::extract_packages(&path);
    let pkg = &packages[0];
    assert_eq!(pkg.name.as_deref(), Some("example"));
    assert_eq!(pkg.description.as_deref(), Some("Example"));
}

#[test]
fn test_dependency_parsing_skips_inline_python_expression_fragments() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("packagegroup_1.0.bb");
    std::fs::write(
        &path,
        "PN = \"packagegroup\"\n\
         RDEPENDS:${PN} = \"ifuse ${@bb.utils.contains(\"DISTRO_FEATURES\", \"pam\", \"smbnetfs\", \"\", d)} simple-mtpfs\"\n",
    )
    .unwrap();

    let packages = BitbakeRecipeParser::extract_packages(&path);
    let pkg = &packages[0];
    let runtime_purls: Vec<_> = pkg
        .dependencies
        .iter()
        .filter(|dep| dep.scope.as_deref() == Some("runtime"))
        .filter_map(|dep| dep.purl.as_deref())
        .collect();

    assert_eq!(
        runtime_purls,
        vec!["pkg:bitbake/ifuse", "pkg:bitbake/simple-mtpfs"]
    );
}

#[test]
fn test_dependency_parsing_skips_leading_fragment_after_expansion() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pru_1.0.bbappend");
    std::fs::write(&path, "RDEPENDS:${PN}:append = \"${PN}-examples\"\n").unwrap();

    let packages = BitbakeRecipeParser::extract_packages(&path);
    let pkg = &packages[0];
    assert!(pkg.dependencies.is_empty());
}

#[test]
fn test_dependency_requirements_are_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test_1.0.bb");
    std::fs::write(
        &path,
        "DEPENDS = \"zlib (>= 1.2)\"\n\
         RDEPENDS:${PN} = \"libfoo (= 2.0)\"\n",
    )
    .unwrap();

    let packages = BitbakeRecipeParser::extract_packages(&path);
    let pkg = &packages[0];
    let build_dep = pkg
        .dependencies
        .iter()
        .find(|dependency| dependency.scope.as_deref() == Some("build"))
        .unwrap();
    let runtime_dep = pkg
        .dependencies
        .iter()
        .find(|dependency| dependency.scope.as_deref() == Some("runtime"))
        .unwrap();

    assert_eq!(build_dep.extracted_requirement.as_deref(), Some(">= 1.2"));
    assert_eq!(runtime_dep.extracted_requirement.as_deref(), Some("= 2.0"));
}
