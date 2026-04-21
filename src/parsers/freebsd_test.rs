// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use crate::models::PackageType;
use std::path::PathBuf;

use super::PackageParser;
use super::freebsd::{
    FreebsdCompactManifestParser, build_freebsd_license_data, build_freebsd_purl,
    build_license_statement, parse_freebsd_manifest,
};
use crate::models::DatasourceId;

#[test]
fn test_is_match() {
    assert!(FreebsdCompactManifestParser::is_match(&PathBuf::from(
        "/path/to/+COMPACT_MANIFEST"
    )));
    assert!(FreebsdCompactManifestParser::is_match(&PathBuf::from(
        "+COMPACT_MANIFEST"
    )));
    assert!(!FreebsdCompactManifestParser::is_match(&PathBuf::from(
        "+MANIFEST"
    )));
    assert!(!FreebsdCompactManifestParser::is_match(&PathBuf::from(
        "COMPACT_MANIFEST"
    )));
    assert!(!FreebsdCompactManifestParser::is_match(&PathBuf::from(
        "package.json"
    )));
    assert!(!FreebsdCompactManifestParser::is_match(&PathBuf::from(
        "/path/to/+MANIFEST"
    )));
}

#[test]
fn test_basic_extraction() {
    let content = r#"{"name":"dmidecode","origin":"sysutils/dmidecode","version":"2.12","arch":"freebsd:10:x86:64","maintainer":"anders@FreeBSD.org","prefix":"/usr/local","www":"http://www.nongnu.org/dmidecode/","flatsize":188874,"comment":"Tool for dumping DMI (SMBIOS) contents in human-readable format","licenselogic":"single","licenses":["GPLv2"],"desc":"Dmidecode is a tool or dumping a computer's DMI (some say SMBIOS) table\ncontents in a human-readable format.","categories":["sysutils"]}"#;

    let pkg = parse_freebsd_manifest(content);

    assert_eq!(pkg.name, Some("dmidecode".to_string()));
    assert_eq!(pkg.version, Some("2.12".to_string()));
    assert_eq!(
        pkg.description,
        Some(
            "Dmidecode is a tool or dumping a computer\'s DMI (some say SMBIOS) table\ncontents in a human-readable format.".to_string()
        )
    );
    assert_eq!(
        pkg.homepage_url,
        Some("http://www.nongnu.org/dmidecode/".to_string())
    );
    assert_eq!(pkg.keywords, vec!["sysutils".to_string()]);
    assert_eq!(pkg.package_type, Some(PackageType::Freebsd));
    assert_eq!(
        pkg.datasource_id,
        Some(DatasourceId::FreebsdCompactManifest)
    );
}

#[test]
fn test_license_single() {
    let content =
        r#"{"name":"dmidecode","version":"2.12","licenselogic":"single","licenses":["GPLv2"]}"#;

    let pkg = parse_freebsd_manifest(content);

    assert_eq!(pkg.extracted_license_statement, Some("GPLv2".to_string()));
}

#[test]
fn test_license_and() {
    let content = r#"{"name":"py27-idna","version":"2.6","licenselogic":"and","licenses":["PSFL","BSD3CLAUSE"]}"#;

    let pkg = parse_freebsd_manifest(content);

    assert_eq!(
        pkg.extracted_license_statement,
        Some("BSD3CLAUSE AND PSFL".to_string())
    );
}

#[test]
fn test_license_or() {
    let content = r#"{"name":"rubygem-facets","version":"3.1.0","licenselogic":"or","licenses":["RUBY","GPLv2"]}"#;

    let pkg = parse_freebsd_manifest(content);

    assert_eq!(
        pkg.extracted_license_statement,
        Some("GPLv2 OR RUBY".to_string())
    );
}

#[test]
fn test_license_dual() {
    let content = r#"{"name":"rubygem-ttfunk","version":"1.5.1","licenselogic":"or","licenses":["GPLv3","RUBY","GPLv2"]}"#;

    let pkg = parse_freebsd_manifest(content);

    assert_eq!(
        pkg.extracted_license_statement,
        Some("GPLv2 OR GPLv3 OR RUBY".to_string())
    );
}

#[test]
fn test_no_licenses() {
    let content = r#"{"name":"dmidecode","origin":"sysutils/dmidecode","version":"2.12","arch":"freebsd:10:x86:64","maintainer":"anders@FreeBSD.org"}"#;

    let pkg = parse_freebsd_manifest(content);

    assert_eq!(pkg.name, Some("dmidecode".to_string()));
    assert_eq!(pkg.extracted_license_statement, None);
}

#[test]
fn test_url_construction() {
    let content = r#"{"name":"dmidecode","origin":"sysutils/dmidecode","version":"2.12","arch":"freebsd:10:x86:64"}"#;

    let pkg = parse_freebsd_manifest(content);

    assert_eq!(
        pkg.code_view_url,
        Some("https://svnweb.freebsd.org/ports/head/sysutils/dmidecode".to_string())
    );
    assert_eq!(
        pkg.download_url,
        Some("https://pkg.freebsd.org/freebsd:10:x86:64/latest/All/dmidecode-2.12.txz".to_string())
    );
}

#[test]
fn test_url_construction_no_arch() {
    let content = r#"{"name":"dmidecode","origin":"sysutils/dmidecode","version":"2.12"}"#;

    let pkg = parse_freebsd_manifest(content);

    assert_eq!(
        pkg.code_view_url,
        Some("https://svnweb.freebsd.org/ports/head/sysutils/dmidecode".to_string())
    );
    assert_eq!(pkg.download_url, None);
}

#[test]
fn test_url_construction_no_origin() {
    let content = r#"{"name":"dmidecode","version":"2.12","arch":"freebsd:10:x86:64"}"#;

    let pkg = parse_freebsd_manifest(content);

    assert_eq!(pkg.code_view_url, None);
    assert_eq!(
        pkg.download_url,
        Some("https://pkg.freebsd.org/freebsd:10:x86:64/latest/All/dmidecode-2.12.txz".to_string())
    );
}

#[test]
fn test_qualifiers() {
    let content = r#"{"name":"dmidecode","origin":"sysutils/dmidecode","version":"2.12","arch":"freebsd:10:x86:64"}"#;

    let pkg = parse_freebsd_manifest(content);

    let qualifiers = pkg.qualifiers.unwrap();
    assert_eq!(
        qualifiers.get("arch"),
        Some(&"freebsd:10:x86:64".to_string())
    );
    assert_eq!(
        qualifiers.get("origin"),
        Some(&"sysutils/dmidecode".to_string())
    );
}

#[test]
fn test_qualifiers_only_arch() {
    let content = r#"{"name":"dmidecode","version":"2.12","arch":"freebsd:10:x86:64"}"#;

    let pkg = parse_freebsd_manifest(content);

    let qualifiers = pkg.qualifiers.unwrap();
    assert_eq!(
        qualifiers.get("arch"),
        Some(&"freebsd:10:x86:64".to_string())
    );
    assert_eq!(qualifiers.get("origin"), None);
}

#[test]
fn test_qualifiers_none() {
    let content = r#"{"name":"dmidecode","version":"2.12"}"#;

    let pkg = parse_freebsd_manifest(content);

    assert_eq!(pkg.qualifiers, None);
}

#[test]
fn test_maintainer_party() {
    let content = r#"{"name":"dmidecode","version":"2.12","maintainer":"anders@FreeBSD.org"}"#;

    let pkg = parse_freebsd_manifest(content);

    assert_eq!(pkg.parties.len(), 1);
    assert_eq!(pkg.parties[0].role, Some("maintainer".to_string()));
    assert_eq!(pkg.parties[0].email, Some("anders@FreeBSD.org".to_string()));
    assert_eq!(pkg.parties[0].name, None);
    assert_eq!(pkg.parties[0].r#type, Some("person".to_string()));
}

#[test]
fn test_no_maintainer() {
    let content = r#"{"name":"dmidecode","version":"2.12"}"#;

    let pkg = parse_freebsd_manifest(content);

    assert_eq!(pkg.parties.len(), 0);
}

#[test]
fn test_categories_as_keywords() {
    let content = r#"{"name":"dmidecode","version":"2.12","categories":["sysutils","admin"]}"#;

    let pkg = parse_freebsd_manifest(content);

    assert_eq!(
        pkg.keywords,
        vec!["sysutils".to_string(), "admin".to_string()]
    );
}

#[test]
fn test_malformed_json() {
    let content = r#"{"name":"broken"#;

    let pkg = parse_freebsd_manifest(content);

    // Should return default package data
    assert_eq!(pkg.package_type, Some(PackageType::Freebsd));
    assert_eq!(pkg.name, None);
}

#[test]
fn test_build_license_statement_multiple_or() {
    let licenses = Some(vec![
        "GPLv3".to_string(),
        "RUBY".to_string(),
        "GPLv2".to_string(),
    ]);
    let logic = Some("or".to_string());
    let result = build_license_statement(&licenses, &logic);
    assert_eq!(result, Some("GPLv2 OR GPLv3 OR RUBY".to_string()));
}

#[test]
fn test_extract_from_testdata() {
    let test_files = vec![
        "testdata/freebsd/basic/+COMPACT_MANIFEST",
        "testdata/freebsd/basic2/+COMPACT_MANIFEST",
        "testdata/freebsd/multi_license/+COMPACT_MANIFEST",
        "testdata/freebsd/dual_license/+COMPACT_MANIFEST",
        "testdata/freebsd/dual_license2/+COMPACT_MANIFEST",
        "testdata/freebsd/no_licenses/+COMPACT_MANIFEST",
    ];

    for file_path in test_files {
        let path = PathBuf::from(file_path);
        if path.exists() {
            println!("Testing file: {}", file_path);
            let pkg = FreebsdCompactManifestParser::extract_first_package(&path);

            // Basic sanity checks
            assert!(
                pkg.name.is_some(),
                "Name should be present in {}",
                file_path
            );
            assert!(
                pkg.version.is_some(),
                "Version should be present in {}",
                file_path
            );
            assert_eq!(pkg.package_type, Some(PackageType::Freebsd));
            assert_eq!(
                pkg.datasource_id,
                Some(DatasourceId::FreebsdCompactManifest)
            );
        }
    }
}

#[test]
fn test_basic_file() {
    let path = PathBuf::from("testdata/freebsd/basic/+COMPACT_MANIFEST");
    assert!(path.exists(), "missing fixture: {}", path.display());

    let pkg = FreebsdCompactManifestParser::extract_first_package(&path);

    assert_eq!(pkg.name, Some("dmidecode".to_string()));
    assert_eq!(pkg.version, Some("2.12".to_string()));
    assert_eq!(pkg.extracted_license_statement, Some("GPLv2".to_string()));
    assert_eq!(pkg.parties.len(), 1);
    assert_eq!(pkg.parties[0].email, Some("anders@FreeBSD.org".to_string()));

    let qualifiers = pkg.qualifiers.unwrap();
    assert_eq!(
        qualifiers.get("arch"),
        Some(&"freebsd:10:x86:64".to_string())
    );
    assert_eq!(
        qualifiers.get("origin"),
        Some(&"sysutils/dmidecode".to_string())
    );

    assert!(pkg.code_view_url.is_some());
    assert!(pkg.download_url.is_some());
    assert_eq!(pkg.declared_license_expression.as_deref(), Some("gpl-2.0"));
    assert_eq!(
        pkg.purl.as_deref(),
        Some("pkg:freebsd/dmidecode@2.12?arch=freebsd:10:x86:64&origin=sysutils/dmidecode")
    );
}

#[test]
fn test_build_freebsd_purl_with_qualifiers() {
    let purl = build_freebsd_purl(
        "dmidecode",
        Some("2.12"),
        Some("freebsd:10:x86:64"),
        Some("sysutils/dmidecode"),
    );

    assert_eq!(
        purl.as_deref(),
        Some("pkg:freebsd/dmidecode@2.12?arch=freebsd:10:x86:64&origin=sysutils/dmidecode")
    );
}

#[test]
fn test_build_freebsd_license_data_single_license() {
    let licenses = vec!["GPLv2".to_string()];
    let (declared, declared_spdx, detections) =
        build_freebsd_license_data(Some(&licenses), Some("single"), Some("GPLv2"));

    assert_eq!(declared.as_deref(), Some("gpl-2.0"));
    assert_eq!(declared_spdx.as_deref(), Some("GPL-2.0-only"));
    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].license_expression, "gpl-2.0");
}

#[test]
fn test_multi_license_file() {
    let path = PathBuf::from("testdata/freebsd/multi_license/+COMPACT_MANIFEST");
    assert!(path.exists(), "missing fixture: {}", path.display());

    let pkg = FreebsdCompactManifestParser::extract_first_package(&path);

    assert_eq!(pkg.name, Some("py27-idna".to_string()));
    assert_eq!(
        pkg.extracted_license_statement,
        Some("BSD3CLAUSE AND PSFL".to_string())
    );
}

#[test]
fn test_dual_license_file() {
    let path = PathBuf::from("testdata/freebsd/dual_license/+COMPACT_MANIFEST");
    assert!(path.exists(), "missing fixture: {}", path.display());

    let pkg = FreebsdCompactManifestParser::extract_first_package(&path);

    assert_eq!(pkg.name, Some("rubygem-facets".to_string()));
    assert_eq!(
        pkg.extracted_license_statement,
        Some("GPLv2 OR RUBY".to_string())
    );
}

#[test]
fn test_no_licenses_file() {
    let path = PathBuf::from("testdata/freebsd/no_licenses/+COMPACT_MANIFEST");
    assert!(path.exists(), "missing fixture: {}", path.display());

    let pkg = FreebsdCompactManifestParser::extract_first_package(&path);

    assert_eq!(pkg.name, Some("dmidecode".to_string()));
    assert_eq!(pkg.extracted_license_statement, None);
}
