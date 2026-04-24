// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn apply_package_reference_following_resolves_manifest_origin_local_file() {
    let package_uid = "pkg:cargo/demo?uuid=test".to_string();
    let mut package = super::test_utils::package(&package_uid, "project/Cargo.toml");
    package.datafile_paths = vec!["project/Cargo.toml".to_string()];
    package.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "unknown-license-reference".to_string(),
        license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
        matches: vec![Match {
            license_expression: "unknown-license-reference".to_string(),
            license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
            from_file: Some("project/Cargo.toml".to_string()),
            start_line: LineNumber::new(5).unwrap(),
            end_line: LineNumber::new(5).unwrap(),
            matcher: Some("parser-declared-license".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(1),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: None,
            rule_url: None,
            matched_text: Some("MIT".to_string()),
            referenced_filenames: Some(vec!["LICENSE".to_string()]),
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("unknown-ref".to_string()),
    }];

    let mut manifest = file("project/Cargo.toml");
    manifest.for_packages = vec![PackageUid::from_raw(package_uid.clone())];
    manifest.package_data = vec![PackageData {
        package_type: Some(PackageType::Cargo),
        license_detections: package.license_detections.clone(),
        ..Default::default()
    }];

    let mut license = file("project/LICENSE");
    license.license_expression = Some("mit".to_string());
    license.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/LICENSE".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(20).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(100),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("mit.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("mit-license".to_string()),
    }];

    let mut files = vec![dir("project"), manifest, license];
    let mut packages = vec![package];
    apply_package_reference_following(&mut files, &mut packages);

    assert_eq!(
        packages[0].declared_license_expression.as_deref(),
        Some("mit")
    );
    assert_eq!(packages[0].license_detections[0].matches.len(), 2);
    assert_eq!(
        packages[0].license_detections[0].matches[1]
            .from_file
            .as_deref(),
        Some("project/LICENSE")
    );
    assert_eq!(
        files[1].package_data[0]
            .declared_license_expression
            .as_deref(),
        Some("mit")
    );
}

#[test]
fn apply_package_reference_following_resolves_absolute_rootfs_license_reference() {
    let mut common_license = file("usr/share/common-licenses/GPL-2");
    common_license.license_expression = Some("gpl-2.0".to_string());
    common_license.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "gpl-2.0".to_string(),
        license_expression_spdx: "GPL-2.0-only".to_string(),
        matches: vec![Match {
            license_expression: "gpl-2.0".to_string(),
            license_expression_spdx: "GPL-2.0-only".to_string(),
            from_file: Some("usr/share/common-licenses/GPL-2".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(339).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(2931),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("gpl-2.0.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("gpl-root".to_string()),
    }];

    let mut service = file("usr/sbin/service");
    service.license_expression = Some("gpl-2.0-plus".to_string());
    service.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "gpl-2.0-plus".to_string(),
        license_expression_spdx: "GPL-2.0-or-later".to_string(),
        matches: vec![Match {
            license_expression: "gpl-2.0-plus".to_string(),
            license_expression_spdx: "GPL-2.0-or-later".to_string(),
            from_file: Some("usr/sbin/service".to_string()),
            start_line: LineNumber::new(16).unwrap(),
            end_line: LineNumber::new(31).unwrap(),
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(139),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("gpl-2.0-plus_233.RULE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: Some(vec!["/usr/share/common-licenses/GPL-2".to_string()]),
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("service-gpl".to_string()),
    }];

    let mut files = vec![
        dir("usr"),
        dir("usr/sbin"),
        dir("usr/share"),
        dir("usr/share/common-licenses"),
        common_license,
        service,
    ];
    let mut packages = Vec::new();
    let snapshot = super::build_reference_follow_snapshot(&files, &packages);
    let resolved = super::resolve_referenced_resource(
        "/usr/share/common-licenses/GPL-2",
        &files[5].license_detections[0],
        "usr/sbin/service",
        &[],
        &snapshot,
    )
    .expect("absolute rootfs reference should resolve");
    assert_eq!(resolved.path, "usr/share/common-licenses/GPL-2");
    assert!(super::use_referenced_license_expression(
        Some("gpl-2.0"),
        &files[5].license_detections[0],
    ));

    apply_package_reference_following(&mut files, &mut packages);

    let service = files
        .iter()
        .find(|file| file.path == "usr/sbin/service")
        .expect("service file should exist");
    assert_eq!(
        service.license_expression.as_deref(),
        Some("gpl-2.0 AND gpl-2.0-plus")
    );
    assert_eq!(
        service.license_detections[0].license_expression_spdx,
        "GPL-2.0-or-later AND GPL-2.0-only"
    );
    assert_eq!(service.license_detections[0].matches.len(), 2);
    assert_eq!(
        service.license_detections[0].matches[1]
            .from_file
            .as_deref(),
        Some("usr/share/common-licenses/GPL-2")
    );
    assert_eq!(
        service.license_detections[0].matches[1].license_expression_spdx,
        "GPL-2.0-only"
    );
}

#[test]
fn apply_package_reference_following_falls_back_to_root_when_package_missing() {
    let mut root_copying = file("project/COPYING");
    root_copying.license_expression = Some("gpl-3.0".to_string());
    root_copying.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "gpl-3.0".to_string(),
        license_expression_spdx: "GPL-3.0-only".to_string(),
        matches: vec![Match {
            license_expression: "gpl-3.0".to_string(),
            license_expression_spdx: "GPL-3.0-only".to_string(),
            from_file: Some("project/COPYING".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(10).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(50),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("gpl-3.0.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("gpl-root".to_string()),
    }];

    let mut po = file("project/po/en_US.po");
    po.license_expression = Some("unknown-license-reference".to_string());
    po.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "unknown-license-reference".to_string(),
        license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
        matches: vec![Match {
            license_expression: "unknown-license-reference".to_string(),
            license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
            from_file: Some("project/po/en_US.po".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(5),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("unknown-license-reference_see-license_1.RULE".to_string()),
            rule_url: None,
            matched_text: Some("same license as package".to_string()),
            referenced_filenames: Some(vec!["COPYING".to_string()]),
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("unknown-ref".to_string()),
    }];

    let mut files = vec![dir("project"), root_copying, po];
    let mut packages = Vec::new();
    apply_package_reference_following(&mut files, &mut packages);

    let po = files
        .iter()
        .find(|file| file.path == "project/po/en_US.po")
        .expect("po file should exist");
    assert_eq!(po.license_expression.as_deref(), Some("gpl-3.0"));
    assert_eq!(
        po.license_detections[0].detection_log,
        vec!["unknown-reference-to-local-file"]
    );
}

#[test]
fn apply_package_reference_following_prefers_intermediate_ancestor_for_source_tree_root_notice() {
    let mut repo_root_license = file("project/LICENSE");
    repo_root_license.license_expression = Some("apache-2.0".to_string());
    repo_root_license.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "apache-2.0".to_string(),
        license_expression_spdx: "Apache-2.0".to_string(),
        matches: vec![Match {
            license_expression: "apache-2.0".to_string(),
            license_expression_spdx: "Apache-2.0".to_string(),
            from_file: Some("project/LICENSE".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(10).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(50),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("apache-2.0.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("apache-root".to_string()),
    }];

    let mut nested_license = file("project/java/LICENSE");
    nested_license.license_expression = Some("mit".to_string());
    nested_license.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/java/LICENSE".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(17).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(120),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("mit.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("mit-java".to_string()),
    }];

    let mut source = file("project/java/src/com/example/Callback.java");
    source.license_expression = Some("mit".to_string());
    source.license_detections = vec![
        crate::models::LicenseDetection {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            matches: vec![Match {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                from_file: Some("project/java/src/com/example/Callback.java".to_string()),
                start_line: LineNumber::new(4).unwrap(),
                end_line: LineNumber::new(5).unwrap(),
                matcher: Some("2-aho".to_string()),
                score: MatchScore::MAX,
                matched_length: Some(22),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: Some("mit_101.RULE".to_string()),
                rule_url: None,
                matched_text: Some(
                    "This source code is licensed under the MIT license found in the LICENSE file in the root directory of this source tree.".to_string(),
                ),
                referenced_filenames: Some(vec!["LICENSE".to_string()]),
                matched_text_diagnostics: None,
            }],
            detection_log: vec![],
            identifier: Some("source-mit".to_string()),
        },
        crate::models::LicenseDetection {
            license_expression: "apache-2.0".to_string(),
            license_expression_spdx: "Apache-2.0".to_string(),
            matches: vec![Match {
                license_expression: "apache-2.0".to_string(),
                license_expression_spdx: "Apache-2.0".to_string(),
                from_file: Some("project/java/src/com/example/Callback.java".to_string()),
                start_line: LineNumber::new(12).unwrap(),
                end_line: LineNumber::new(22).unwrap(),
                matcher: Some("2-aho".to_string()),
                score: MatchScore::MAX,
                matched_length: Some(85),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: Some("apache-2.0_7.RULE".to_string()),
                rule_url: None,
                matched_text: None,
                referenced_filenames: None,
                matched_text_diagnostics: None,
            }],
            detection_log: vec![],
            identifier: Some("source-apache".to_string()),
        },
    ];

    let mut files = vec![
        dir("project"),
        dir("project/java"),
        dir("project/java/src"),
        dir("project/java/src/com"),
        dir("project/java/src/com/example"),
        repo_root_license,
        nested_license,
        source,
    ];
    let mut packages = Vec::new();

    let snapshot = super::build_reference_follow_snapshot(&files, &packages);
    let resolved = super::resolve_referenced_resource(
        "LICENSE",
        &files[7].license_detections[0],
        "project/java/src/com/example/Callback.java",
        &[],
        &snapshot,
    )
    .expect("nested source-tree LICENSE should resolve");
    assert_eq!(resolved.path, "project/java/LICENSE");

    apply_package_reference_following(&mut files, &mut packages);

    let source = files
        .iter()
        .find(|file| file.path == "project/java/src/com/example/Callback.java")
        .expect("source file should exist");
    assert_eq!(
        source.license_expression.as_deref(),
        Some("apache-2.0 AND mit")
    );
    assert_eq!(source.license_detections.len(), 2);
    let followed = source
        .license_detections
        .iter()
        .find(|detection| detection.license_expression_spdx == "MIT")
        .expect("followed MIT detection should exist");
    assert_eq!(followed.detection_log, ["unknown-reference-to-local-file"]);
    assert!(
        source
            .license_detections
            .iter()
            .any(|detection| detection.license_expression_spdx == "Apache-2.0")
    );
    assert!(followed.matches.iter().any(|detection_match| {
        detection_match.from_file.as_deref() == Some("project/java/LICENSE")
    }));
}

#[test]
fn reference_root_language_accepts_project_scope_but_not_bare_root_directory() {
    let project_root_notice = Match {
        license_expression: "unknown-license-reference".to_string(),
        license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
        from_file: Some("project/src/file.c".to_string()),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
        matcher: Some("2-aho".to_string()),
        score: MatchScore::MAX,
        matched_length: Some(10),
        match_coverage: Some(100.0),
        rule_relevance: Some(100),
        rule_identifier: Some("unknown-license-reference_see-license_1.RULE".to_string()),
        rule_url: None,
        matched_text: Some(
            "This source code is licensed under the BSD-style license found in the LICENSE file in the root directory of this project.".to_string(),
        ),
        referenced_filenames: Some(vec!["LICENSE".to_string()]),
        matched_text_diagnostics: None,
    };
    assert!(
        super::reference_following::detection_match_explicitly_mentions_reference_root(
            &project_root_notice
        )
    );

    let bare_root_notice = Match {
        matched_text: Some(
            "This source code is licensed under the BSD-style license found in the LICENSE file in the root directory.".to_string(),
        ),
        ..project_root_notice
    };
    assert!(
        !super::reference_following::detection_match_explicitly_mentions_reference_root(
            &bare_root_notice
        )
    );
}

#[test]
fn apply_package_reference_following_falls_back_past_nested_root_to_repo_root() {
    let mut root_license = file("LICENSE");
    root_license.license_expression = Some("mit".to_string());
    root_license.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("LICENSE".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(20).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(100),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("mit.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("mit-root".to_string()),
    }];

    let mut nested_license = file("docs/LICENSE");
    nested_license.license_expression = Some("apache-2.0".to_string());
    nested_license.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "apache-2.0".to_string(),
        license_expression_spdx: "Apache-2.0".to_string(),
        matches: vec![Match {
            license_expression: "apache-2.0".to_string(),
            license_expression_spdx: "Apache-2.0".to_string(),
            from_file: Some("docs/LICENSE".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(20).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(100),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("apache-2.0.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("apache-docs".to_string()),
    }];

    let mut manpage = file("docs/man-xlate/nmap-id.1");
    manpage.license_expression = Some("unknown-license-reference".to_string());
    manpage.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "unknown-license-reference".to_string(),
        license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
        matches: vec![Match {
            license_expression: "unknown-license-reference".to_string(),
            license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
            from_file: Some("docs/man-xlate/nmap-id.1".to_string()),
            start_line: LineNumber::new(100).unwrap(),
            end_line: LineNumber::new(100).unwrap(),
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(2),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("unknown-license-reference_see-license_1.RULE".to_string()),
            rule_url: None,
            matched_text: Some("See LICENSE".to_string()),
            referenced_filenames: Some(vec!["LICENSE".to_string()]),
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("manpage-ref".to_string()),
    }];

    let mut files = vec![
        dir("docs"),
        dir("docs/man-xlate"),
        root_license,
        nested_license,
        manpage,
    ];
    let mut packages = Vec::new();
    apply_package_reference_following(&mut files, &mut packages);

    let manpage = files
        .iter()
        .find(|file| file.path == "docs/man-xlate/nmap-id.1")
        .expect("manpage file should exist");
    assert_eq!(manpage.license_expression.as_deref(), Some("mit"));
    assert_eq!(
        manpage.license_detections[0].detection_log,
        vec!["unknown-reference-to-local-file"]
    );
    assert_eq!(
        manpage.license_detections[0].matches[1]
            .from_file
            .as_deref(),
        Some("LICENSE")
    );
}

#[test]
fn apply_package_reference_following_inherits_license_from_package_context() {
    let package_uid = "pkg:pypi/demo?uuid=test".to_string();
    let mut package = super::test_utils::package(&package_uid, "project/PKG-INFO");
    package.datafile_paths = vec!["project/PKG-INFO".to_string()];
    package.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "bsd-new".to_string(),
        license_expression_spdx: "BSD-3-Clause".to_string(),
        matches: vec![Match {
            license_expression: "bsd-new".to_string(),
            license_expression_spdx: "BSD-3-Clause".to_string(),
            from_file: Some("project/PKG-INFO".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("1-hash".to_string()),
            score: MatchScore::from_percentage(99.0),
            matched_length: Some(5),
            match_coverage: Some(100.0),
            rule_relevance: Some(99),
            rule_identifier: Some("pypi_bsd_license.RULE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("package-license".to_string()),
    }];

    let mut source = file("project/locale/django.po");
    source.for_packages = vec![PackageUid::from_raw(package_uid.clone())];
    source.license_expression = Some("free-unknown".to_string());
    source.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "free-unknown".to_string(),
        license_expression_spdx: "LicenseRef-scancode-free-unknown".to_string(),
        matches: vec![Match {
            license_expression: "free-unknown".to_string(),
            license_expression_spdx: "LicenseRef-scancode-free-unknown".to_string(),
            from_file: Some("project/locale/django.po".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(11),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("free-unknown-package_1.RULE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: Some(vec!["INHERIT_LICENSE_FROM_PACKAGE".to_string()]),
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("unknown-package-ref".to_string()),
    }];

    let mut files = vec![dir("project"), source];
    let mut packages = vec![package];
    apply_package_reference_following(&mut files, &mut packages);

    let source = files
        .iter()
        .find(|file| file.path == "project/locale/django.po")
        .expect("source file should exist");
    assert_eq!(source.license_expression.as_deref(), Some("bsd-new"));
    assert_eq!(
        source.license_detections[0].detection_log,
        vec!["unknown-reference-in-file-to-package"]
    );
    assert_eq!(source.license_detections[0].matches.len(), 2);
    assert_eq!(
        source.license_detections[0].matches[1].from_file.as_deref(),
        Some("project/PKG-INFO")
    );
}

#[test]
fn apply_package_reference_following_falls_back_to_root_for_missing_package_reference() {
    let mut root_copying = file("project/COPYING");
    root_copying.license_expression = Some("gpl-3.0".to_string());
    root_copying.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "gpl-3.0".to_string(),
        license_expression_spdx: "GPL-3.0-only".to_string(),
        matches: vec![Match {
            license_expression: "gpl-3.0".to_string(),
            license_expression_spdx: "GPL-3.0-only".to_string(),
            from_file: Some("project/COPYING".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(10).unwrap(),
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(50),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("gpl-3.0.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("gpl-root".to_string()),
    }];

    let mut po = file("project/po/en_US.po");
    po.license_expression = Some("free-unknown".to_string());
    po.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "free-unknown".to_string(),
        license_expression_spdx: "LicenseRef-scancode-free-unknown".to_string(),
        matches: vec![Match {
            license_expression: "free-unknown".to_string(),
            license_expression_spdx: "LicenseRef-scancode-free-unknown".to_string(),
            from_file: Some("project/po/en_US.po".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(5),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("free-unknown-package_2.RULE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: Some(vec!["INHERIT_LICENSE_FROM_PACKAGE".to_string()]),
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("unknown-package-ref".to_string()),
    }];

    let mut files = vec![dir("project"), root_copying, po];
    let mut packages = Vec::new();
    apply_package_reference_following(&mut files, &mut packages);

    let po = files
        .iter()
        .find(|file| file.path == "project/po/en_US.po")
        .expect("po file should exist");
    assert_eq!(po.license_expression.as_deref(), Some("gpl-3.0"));
    assert_eq!(
        po.license_detections[0].detection_log,
        vec!["unknown-reference-in-file-to-nonexistent-package"]
    );
    assert_eq!(
        po.license_detections[0].matches[1].from_file.as_deref(),
        Some("project/COPYING")
    );
}

#[test]
fn apply_package_reference_following_leaves_ambiguous_multi_package_file_unresolved() {
    let first_uid = "pkg:pypi/demo-a?uuid=test".to_string();
    let second_uid = "pkg:pypi/demo-b?uuid=test".to_string();

    let mut first_package = super::test_utils::package(&first_uid, "project/a/PKG-INFO");
    first_package.datafile_paths = vec!["project/a/PKG-INFO".to_string()];
    first_package.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/a/PKG-INFO".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(5),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("mit.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("mit-license".to_string()),
    }];

    let mut second_package = super::test_utils::package(&second_uid, "project/b/PKG-INFO");
    second_package.datafile_paths = vec!["project/b/PKG-INFO".to_string()];
    second_package.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "apache-2.0".to_string(),
        license_expression_spdx: "Apache-2.0".to_string(),
        matches: vec![Match {
            license_expression: "apache-2.0".to_string(),
            license_expression_spdx: "Apache-2.0".to_string(),
            from_file: Some("project/b/PKG-INFO".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("1-hash".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(5),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("apache-2.0.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("apache-license".to_string()),
    }];

    let mut shared_file = file("project/shared/locale.po");
    shared_file.for_packages = vec![
        PackageUid::from_raw(first_uid),
        PackageUid::from_raw(second_uid),
    ];
    shared_file.license_expression = Some("free-unknown".to_string());
    shared_file.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "free-unknown".to_string(),
        license_expression_spdx: "LicenseRef-scancode-free-unknown".to_string(),
        matches: vec![Match {
            license_expression: "free-unknown".to_string(),
            license_expression_spdx: "LicenseRef-scancode-free-unknown".to_string(),
            from_file: Some("project/shared/locale.po".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(11),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("free-unknown-package_1.RULE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: Some(vec!["INHERIT_LICENSE_FROM_PACKAGE".to_string()]),
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: Some("unknown-package-ref".to_string()),
    }];

    let mut files = vec![dir("project"), shared_file];
    let mut packages = vec![first_package, second_package];
    apply_package_reference_following(&mut files, &mut packages);

    let shared_file = files
        .iter()
        .find(|file| file.path == "project/shared/locale.po")
        .expect("shared file should exist");
    assert_eq!(
        shared_file.license_expression.as_deref(),
        Some("free-unknown")
    );
    assert_eq!(shared_file.license_detections[0].matches.len(), 1);
    assert!(shared_file.license_detections[0].detection_log.is_empty());
}
