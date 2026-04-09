use std::fs;

use super::test_utils::{dir, file, package, scan_and_assemble_with_keyfiles};
use super::*;
use crate::models::{
    Copyright, DatasourceId, FileReference, Holder, LineNumber, Match, Package, PackageType,
};

#[test]
fn classify_key_files_marks_nested_ruby_license_from_file_references() {
    let uid = "pkg:gem/inspec-bin@6.8.2?uuid=test";
    let mut metadata_file = file("inspec-6.8.2/metadata.gz-extract");
    metadata_file.for_packages.push(uid.to_string());
    metadata_file.package_data = vec![crate::models::PackageData {
        package_type: Some(PackageType::Gem),
        datasource_id: Some(DatasourceId::GemArchiveExtracted),
        file_references: vec![FileReference {
            path: "inspec-6.8.2/inspec-bin/LICENSE".to_string(),
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            extra_data: None,
        }],
        ..Default::default()
    }];

    let mut license_file = file("inspec-6.8.2/inspec-bin/LICENSE");
    license_file.for_packages.push(uid.to_string());
    license_file.license_expression = Some("Apache-2.0".to_string());
    license_file.copyrights = vec![Copyright {
        copyright: "Copyright (c) 2019 Chef Software Inc.".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];
    license_file.holders = vec![Holder {
        holder: "Chef Software Inc.".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];
    license_file.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "apache-2.0".to_string(),
        license_expression_spdx: "Apache-2.0".to_string(),
        matches: vec![Match {
            license_expression: "apache-2.0".to_string(),
            license_expression_spdx: "Apache-2.0".to_string(),
            from_file: Some("inspec-6.8.2/inspec-bin/LICENSE".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(20).unwrap(),
            matcher: None,
            score: 100.0,
            matched_length: Some(161),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: None,
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        identifier: None,
        detection_log: vec![],
    }];

    let mut files = vec![metadata_file, license_file];
    let packages = vec![package(uid, "inspec-6.8.2/metadata.gz-extract")];

    classify_key_files(&mut files, &packages);
    let license = files
        .iter()
        .find(|f| f.path.ends_with("/LICENSE"))
        .expect("license file exists");

    assert!(license.is_legal);
    assert!(license.is_top_level);
    assert!(license.is_key_file);
}

#[test]
fn classify_key_files_does_not_tag_unreferenced_nested_legal_file() {
    let uid = "pkg:gem/demo@1.0.0?uuid=test";
    let mut gemspec = file("demo/demo.gemspec");
    gemspec.for_packages.push(uid.to_string());
    gemspec.package_data = vec![crate::models::PackageData {
        package_type: Some(PackageType::Gem),
        datasource_id: Some(DatasourceId::Gemspec),
        ..Default::default()
    }];

    let mut nested_license = file("demo/subdir/LICENSE");
    nested_license.for_packages.push(uid.to_string());

    let mut files = vec![gemspec, nested_license];
    let packages = vec![package(uid, "demo/demo.gemspec")];

    classify_key_files(&mut files, &packages);
    let nested = files
        .iter()
        .find(|f| f.path.ends_with("subdir/LICENSE"))
        .unwrap();

    assert!(nested.is_legal);
    assert!(!nested.is_top_level);
    assert!(!nested.is_key_file);
}

#[test]
fn classify_key_files_marks_top_level_community_files_without_package_links() {
    let mut files = vec![
        dir("project"),
        file("project/AUTHORS"),
        file("project/README.md"),
    ];

    classify_key_files(&mut files, &[]);

    assert!(files[1].is_community);
    assert!(files[1].is_top_level);
    assert!(!files[1].is_key_file);

    assert!(files[2].is_readme);
    assert!(files[2].is_top_level);
    assert!(files[2].is_key_file);
}

#[test]
fn classify_key_files_matches_scan_code_cli_fixture_patterns() {
    let mut haxelib = file("cli/haxelib.json");
    haxelib.package_data = vec![crate::models::PackageData {
        package_type: Some(PackageType::Haxe),
        ..Default::default()
    }];

    let mut files = vec![
        dir("cli"),
        file("cli/LICENCING.readme"),
        file("cli/README.first"),
        haxelib,
        dir("cli/not-top"),
        file("cli/not-top/composer.json"),
        file("cli/not-top/README.second"),
    ];

    classify_key_files(&mut files, &[]);

    assert!(files[0].is_top_level);
    assert!(!files[0].is_key_file);

    assert!(files[1].is_legal);
    assert!(files[1].is_readme);
    assert!(files[1].is_top_level);
    assert!(files[1].is_key_file);

    assert!(!files[2].is_legal);
    assert!(files[2].is_readme);
    assert!(files[2].is_top_level);
    assert!(files[2].is_key_file);

    assert!(files[3].is_manifest);
    assert!(files[3].is_top_level);
    assert!(files[3].is_key_file);

    assert!(files[4].is_top_level);
    assert!(!files[4].is_key_file);

    assert!(files[5].is_manifest);
    assert!(!files[5].is_top_level);
    assert!(!files[5].is_key_file);

    assert!(files[6].is_readme);
    assert!(!files[6].is_top_level);
    assert!(!files[6].is_key_file);
}

#[test]
fn classify_key_files_marks_package_data_ancestry_like_with_package_data_fixture() {
    let uid = "pkg:maven/org.jboss.logging/jboss-logging@3.4.2.Final?uuid=test";

    let mut manifest_mf = file("jar/META-INF/MANIFEST.MF");
    manifest_mf.for_packages.push(uid.to_string());
    manifest_mf.package_data = vec![crate::models::PackageData::default()];

    let mut license = file("jar/META-INF/LICENSE.txt");
    license.for_packages.push(uid.to_string());

    let mut pom_properties =
        file("jar/META-INF/maven/org.jboss.logging/jboss-logging/pom.properties");
    pom_properties.for_packages.push(uid.to_string());
    pom_properties.package_data = vec![crate::models::PackageData::default()];

    let mut pom_xml = file("jar/META-INF/maven/org.jboss.logging/jboss-logging/pom.xml");
    pom_xml.for_packages.push(uid.to_string());
    pom_xml.package_data = vec![crate::models::PackageData::default()];

    let mut source = file("jar/org/jboss/logging/AbstractLoggerProvider.java");
    source.for_packages.push(uid.to_string());

    let mut files = vec![
        dir("jar"),
        dir("jar/META-INF"),
        license,
        manifest_mf,
        dir("jar/META-INF/maven"),
        dir("jar/META-INF/maven/org.jboss.logging"),
        dir("jar/META-INF/maven/org.jboss.logging/jboss-logging"),
        pom_properties,
        pom_xml,
        dir("jar/org"),
        dir("jar/org/jboss"),
        dir("jar/org/jboss/logging"),
        source,
    ];

    let package = Package {
        package_uid: uid.to_string(),
        datafile_paths: vec![
            "jar/META-INF/maven/org.jboss.logging/jboss-logging/pom.xml".to_string(),
        ],
        ..package(
            uid,
            "jar/META-INF/maven/org.jboss.logging/jboss-logging/pom.xml",
        )
    };

    classify_key_files(&mut files, &[package]);

    assert!(files[2].is_legal);
    assert!(files[2].is_top_level);
    assert!(files[2].is_key_file);
    assert!(files[3].is_manifest);
    assert!(files[3].is_top_level);
    assert!(files[3].is_key_file);
    assert!(files[7].is_manifest);
    assert!(files[7].is_top_level);
    assert!(files[7].is_key_file);
    assert!(files[8].is_manifest);
    assert!(files[8].is_top_level);
    assert!(files[8].is_key_file);
    assert!(!files[12].is_top_level);
    assert!(!files[12].is_key_file);
}

#[test]
fn classify_key_files_uses_lowest_common_parent_for_archive_like_tree() {
    let mut files = vec![
        dir("archive.tar.gz"),
        dir("archive.tar.gz/project"),
        dir("archive.tar.gz/project/sub"),
        dir("archive.tar.gz/project/sub/src"),
        file("archive.tar.gz/project/sub/COPYING"),
        file("archive.tar.gz/project/sub/src/main.c"),
    ];

    classify_key_files(&mut files, &[]);

    let copying = files
        .iter()
        .find(|file| file.path == "archive.tar.gz/project/sub/COPYING")
        .expect("COPYING should exist");
    let source = files
        .iter()
        .find(|file| file.path == "archive.tar.gz/project/sub/src/main.c")
        .expect("source file should exist");

    assert!(copying.is_top_level);
    assert!(copying.is_key_file);
    assert!(!source.is_top_level);
    assert!(!source.is_key_file);
}

#[test]
fn debian_directory_scan_assembles_package_and_marks_keyfiles() {
    let temp_dir = tempfile::TempDir::new().expect("create temp dir");
    let package_root = temp_dir.path().join("mypkg");
    let debian_dir = package_root.join("debian");

    fs::create_dir_all(&debian_dir).expect("create debian dir");
    fs::write(
        debian_dir.join("control"),
        "Source: mypkg\nSection: utils\nPriority: optional\nMaintainer: Example Maintainer <example@example.com>\nStandards-Version: 4.6.2\n\nPackage: mypkg\nArchitecture: all\nDepends: bash\nDescription: sample package\n sample package long description\n",
    )
    .expect("write debian/control");
    fs::write(
        debian_dir.join("copyright"),
        "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\nFiles: *\nCopyright: 2024 Example Org\nLicense: Apache-2.0\n Licensed under the Apache License, Version 2.0.\n",
    )
    .expect("write debian/copyright");

    let (files, result) = scan_and_assemble_with_keyfiles(temp_dir.path());

    let package = result
        .packages
        .iter()
        .find(|package| package.name.as_deref() == Some("mypkg"))
        .expect("debian package should be assembled");

    let control = files
        .iter()
        .find(|file| file.path.ends_with("mypkg/debian/control"))
        .expect("control file should be scanned");
    let copyright = files
        .iter()
        .find(|file| file.path.ends_with("mypkg/debian/copyright"))
        .expect("copyright file should be scanned");

    assert!(control.is_manifest);
    assert!(control.is_key_file);
    assert!(copyright.is_legal);
    assert!(copyright.is_key_file);
    assert!(control.for_packages.contains(&package.package_uid));
    assert!(copyright.for_packages.contains(&package.package_uid));
}
