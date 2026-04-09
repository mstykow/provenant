use super::test_utils::{dir, file};
use super::*;
use crate::models::{
    Author, Copyright, Holder, LineNumber, Match, PackageData, PackageType, TallyEntry,
};

#[test]
fn compute_tallies_counts_file_findings_and_missing_values() {
    let mut mit_file = file("project/src/lib.rs");
    mit_file.programming_language = Some("Rust".to_string());
    mit_file.is_source = Some(true);
    mit_file.license_expression = Some("mit".to_string());
    mit_file.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/src/lib.rs".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: None,
            score: 100.0,
            matched_length: None,
            match_coverage: None,
            rule_relevance: None,
            rule_identifier: None,
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        identifier: None,
        detection_log: vec![],
    }];
    mit_file.copyrights = vec![Copyright {
        copyright: "Copyright (c) Example Corp.".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];
    mit_file.holders = vec![Holder {
        holder: "Example Corp.".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];
    mit_file.authors = vec![Author {
        author: "Alice".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];

    let mut dual_license_file = file("project/src/main.c");
    dual_license_file.programming_language = Some("C".to_string());
    dual_license_file.is_source = Some(true);
    dual_license_file.license_expression = Some("apache-2.0 AND mit".to_string());
    dual_license_file.license_detections = vec![
        crate::models::LicenseDetection {
            license_expression: "apache-2.0".to_string(),
            license_expression_spdx: "Apache-2.0".to_string(),
            matches: vec![Match {
                license_expression: "apache-2.0".to_string(),
                license_expression_spdx: "Apache-2.0".to_string(),
                from_file: Some("project/src/main.c".to_string()),
                start_line: LineNumber::ONE,
                end_line: LineNumber::ONE,
                matcher: None,
                score: 100.0,
                matched_length: None,
                match_coverage: None,
                rule_relevance: None,
                rule_identifier: None,
                rule_url: None,
                matched_text: None,
                referenced_filenames: None,
                matched_text_diagnostics: None,
            }],
            identifier: None,
            detection_log: vec![],
        },
        crate::models::LicenseDetection {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            matches: vec![Match {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                from_file: Some("project/src/main.c".to_string()),
                start_line: LineNumber::new(2).unwrap(),
                end_line: LineNumber::new(2).unwrap(),
                matcher: None,
                score: 100.0,
                matched_length: None,
                match_coverage: None,
                rule_relevance: None,
                rule_identifier: None,
                rule_url: None,
                matched_text: None,
                referenced_filenames: None,
                matched_text_diagnostics: None,
            }],
            identifier: None,
            detection_log: vec![],
        },
    ];
    dual_license_file.copyrights = vec![Copyright {
        copyright: "Copyright (c) Example Corp.".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];
    dual_license_file.holders = vec![Holder {
        holder: "Example Corp.".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];
    dual_license_file.authors = vec![Author {
        author: "Bob".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];

    let empty_file = file("project/README.md");
    let tallies =
        compute_tallies(&[mit_file, dual_license_file, empty_file]).expect("tallies exist");

    assert_eq!(tallies.detected_license_expression.len(), 3);
    assert_eq!(
        tallies.detected_license_expression[0].value.as_deref(),
        Some("mit")
    );
    assert_eq!(tallies.detected_license_expression[0].count, 2);
    assert_eq!(tallies.copyrights[0].count, 2);
    assert_eq!(tallies.holders[0].count, 2);
    assert_eq!(tallies.authors.len(), 3);
    assert_eq!(
        tallies.programming_language,
        vec![
            TallyEntry {
                value: Some("C".to_string()),
                count: 1
            },
            TallyEntry {
                value: Some("Rust".to_string()),
                count: 1
            },
        ]
    );
}

#[test]
fn compute_key_file_tallies_only_counts_key_files_and_drops_missing_values() {
    let mut key_license = file("project/LICENSE");
    key_license.is_key_file = true;
    key_license.license_expression = Some("apache-2.0".to_string());
    key_license.copyrights = vec![Copyright {
        copyright: "Copyright (c) Example Corp.".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];
    key_license.holders = vec![Holder {
        holder: "Example Corp.".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];

    let mut key_readme = file("project/README.md");
    key_readme.is_key_file = true;
    key_readme.programming_language = Some("Markdown".to_string());
    key_readme.authors = vec![Author {
        author: "Alice".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];

    let mut non_key_source = file("project/src/lib.rs");
    non_key_source.programming_language = Some("Rust".to_string());
    non_key_source.license_expression = Some("mit".to_string());

    let tallies = compute_key_file_tallies(&[key_license, key_readme, non_key_source])
        .expect("key-file tallies exist");

    assert_eq!(
        tallies.detected_license_expression[0].value.as_deref(),
        Some("apache-2.0")
    );
    assert!(tallies.copyrights.is_empty());
    assert!(tallies.holders.is_empty());
    assert!(tallies.authors.is_empty());
    assert_eq!(
        tallies.programming_language,
        vec![TallyEntry {
            value: Some("Markdown".to_string()),
            count: 1
        }]
    );
}

#[test]
fn compute_tallies_include_package_other_license_detections() {
    let mut manifest = file("project/package.json");
    manifest.package_data = vec![PackageData {
        package_type: Some(PackageType::Npm),
        other_license_detections: vec![crate::models::LicenseDetection {
            license_expression: "gpl-2.0-only".to_string(),
            license_expression_spdx: "GPL-2.0-only".to_string(),
            matches: vec![Match {
                license_expression: "gpl-2.0-only".to_string(),
                license_expression_spdx: "GPL-2.0-only".to_string(),
                from_file: Some("project/package.json".to_string()),
                start_line: LineNumber::ONE,
                end_line: LineNumber::ONE,
                matcher: Some("parser-declared-license".to_string()),
                score: 100.0,
                matched_length: Some(1),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: None,
                rule_url: None,
                matched_text: Some("GPL-2.0-only".to_string()),
                referenced_filenames: None,
                matched_text_diagnostics: None,
            }],
            identifier: Some("gpl-package-id".to_string()),
            detection_log: vec![],
        }],
        ..Default::default()
    }];

    let tallies = compute_tallies(&[manifest]).expect("tallies exist");

    assert_eq!(tallies.detected_license_expression.len(), 1);
    assert_eq!(
        tallies.detected_license_expression[0].value.as_deref(),
        Some("gpl-2.0-only")
    );
    assert_eq!(tallies.detected_license_expression[0].count, 1);
}

#[test]
fn compute_tallies_include_license_clues_in_detected_license_expression() {
    let mut clue_file = file("project/NOTICE");
    clue_file.license_clues = vec![Match {
        license_expression: "unknown-spdx".to_string(),
        license_expression_spdx: "LicenseRef-scancode-unknown-spdx".to_string(),
        from_file: Some("project/NOTICE".to_string()),
        start_line: LineNumber::new(2).unwrap(),
        end_line: LineNumber::new(2).unwrap(),
        matcher: Some("2-aho".to_string()),
        score: 65.0,
        matched_length: Some(2),
        match_coverage: Some(65.0),
        rule_relevance: Some(100),
        rule_identifier: Some("license-clue_1.RULE".to_string()),
        rule_url: None,
        matched_text: None,
        referenced_filenames: None,
        matched_text_diagnostics: None,
    }];

    let tallies = compute_tallies(&[clue_file]).expect("tallies exist");

    assert_eq!(
        tallies.detected_license_expression,
        vec![TallyEntry {
            value: Some("unknown-spdx".to_string()),
            count: 1,
        }]
    );
}

#[test]
fn compute_key_file_tallies_include_license_clues() {
    let mut clue_file = file("project/NOTICE");
    clue_file.is_key_file = true;
    clue_file.license_clues = vec![Match {
        license_expression: "unknown-spdx".to_string(),
        license_expression_spdx: "LicenseRef-scancode-unknown-spdx".to_string(),
        from_file: Some("project/NOTICE".to_string()),
        start_line: LineNumber::new(2).unwrap(),
        end_line: LineNumber::new(2).unwrap(),
        matcher: Some("2-aho".to_string()),
        score: 65.0,
        matched_length: Some(2),
        match_coverage: Some(65.0),
        rule_relevance: Some(100),
        rule_identifier: Some("license-clue_1.RULE".to_string()),
        rule_url: None,
        matched_text: None,
        referenced_filenames: None,
        matched_text_diagnostics: None,
    }];

    let tallies = compute_key_file_tallies(&[clue_file]).expect("key-file tallies exist");

    assert_eq!(
        tallies.detected_license_expression,
        vec![TallyEntry {
            value: Some("unknown-spdx".to_string()),
            count: 1,
        }]
    );
}

#[test]
fn compute_key_file_tallies_include_package_other_license_detections() {
    let mut manifest = file("project/package.json");
    manifest.is_key_file = true;
    manifest.package_data = vec![PackageData {
        package_type: Some(PackageType::Npm),
        other_license_detections: vec![crate::models::LicenseDetection {
            license_expression: "gpl-2.0-only".to_string(),
            license_expression_spdx: "GPL-2.0-only".to_string(),
            matches: vec![Match {
                license_expression: "gpl-2.0-only".to_string(),
                license_expression_spdx: "GPL-2.0-only".to_string(),
                from_file: Some("project/package.json".to_string()),
                start_line: LineNumber::ONE,
                end_line: LineNumber::ONE,
                matcher: Some("parser-declared-license".to_string()),
                score: 100.0,
                matched_length: Some(1),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: None,
                rule_url: None,
                matched_text: Some("GPL-2.0-only".to_string()),
                referenced_filenames: None,
                matched_text_diagnostics: None,
            }],
            identifier: Some("gpl-package-id".to_string()),
            detection_log: vec![],
        }],
        ..Default::default()
    }];

    let tallies = compute_key_file_tallies(&[manifest]).expect("key-file tallies exist");

    assert_eq!(
        tallies.detected_license_expression[0].value.as_deref(),
        Some("gpl-2.0-only")
    );
}

#[test]
fn compute_tallies_include_manifest_package_license_detections() {
    let mut manifest = file("project/Cargo.toml");
    manifest.package_data = vec![PackageData {
        package_type: Some(PackageType::Cargo),
        license_detections: vec![crate::models::LicenseDetection {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            matches: vec![Match {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                from_file: Some("project/Cargo.toml".to_string()),
                start_line: LineNumber::ONE,
                end_line: LineNumber::ONE,
                matcher: Some("parser-declared-license".to_string()),
                score: 100.0,
                matched_length: Some(1),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: None,
                rule_url: None,
                matched_text: Some("MIT".to_string()),
                referenced_filenames: Some(vec!["LICENSE".to_string()]),
                matched_text_diagnostics: None,
            }],
            identifier: Some("mit-package-id".to_string()),
            detection_log: vec!["unknown-reference-to-local-file".to_string()],
        }],
        ..Default::default()
    }];

    let tallies = compute_tallies(&[manifest]).expect("tallies exist");

    assert_eq!(tallies.detected_license_expression.len(), 1);
    assert_eq!(
        tallies.detected_license_expression[0].value.as_deref(),
        Some("mit")
    );
}

#[test]
fn compute_key_file_tallies_include_manifest_package_license_detections() {
    let mut manifest = file("project/Cargo.toml");
    manifest.is_key_file = true;
    manifest.package_data = vec![PackageData {
        package_type: Some(PackageType::Cargo),
        license_detections: vec![crate::models::LicenseDetection {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            matches: vec![Match {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                from_file: Some("project/Cargo.toml".to_string()),
                start_line: LineNumber::ONE,
                end_line: LineNumber::ONE,
                matcher: Some("parser-declared-license".to_string()),
                score: 100.0,
                matched_length: Some(1),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: None,
                rule_url: None,
                matched_text: Some("MIT".to_string()),
                referenced_filenames: Some(vec!["LICENSE".to_string()]),
                matched_text_diagnostics: None,
            }],
            identifier: Some("mit-package-id".to_string()),
            detection_log: vec!["unknown-reference-to-local-file".to_string()],
        }],
        ..Default::default()
    }];

    let tallies = compute_key_file_tallies(&[manifest]).expect("key-file tallies exist");

    assert_eq!(
        tallies.detected_license_expression[0].value.as_deref(),
        Some("mit")
    );
}

#[test]
fn compute_tallies_do_not_double_count_duplicate_file_and_package_detections() {
    let detection = crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/Cargo.toml".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("parser-declared-license".to_string()),
            score: 100.0,
            matched_length: Some(1),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: None,
            rule_url: None,
            matched_text: Some("MIT".to_string()),
            referenced_filenames: Some(vec!["LICENSE".to_string()]),
            matched_text_diagnostics: None,
        }],
        identifier: Some("mit-package-id".to_string()),
        detection_log: vec!["unknown-reference-to-local-file".to_string()],
    };

    let mut manifest = file("project/Cargo.toml");
    manifest.is_key_file = true;
    manifest.license_detections = vec![detection.clone()];
    manifest.license_expression = Some("mit".to_string());
    manifest.package_data = vec![PackageData {
        package_type: Some(PackageType::Cargo),
        license_detections: vec![detection],
        declared_license_expression: Some("mit".to_string()),
        declared_license_expression_spdx: Some("MIT".to_string()),
        ..Default::default()
    }];

    let tallies = compute_tallies(std::slice::from_ref(&manifest)).expect("tallies exist");
    assert_eq!(tallies.detected_license_expression.len(), 1);
    assert_eq!(tallies.detected_license_expression[0].count, 1);

    let key_file_tallies =
        compute_key_file_tallies(std::slice::from_ref(&manifest)).expect("key-file tallies exist");
    assert_eq!(key_file_tallies.detected_license_expression.len(), 1);
    assert_eq!(key_file_tallies.detected_license_expression[0].count, 1);
}

#[test]
fn compute_tallies_deduplicate_duplicate_package_data_entries_per_file() {
    let detection = crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/Cargo.toml".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("parser-declared-license".to_string()),
            score: 100.0,
            matched_length: Some(1),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: None,
            rule_url: None,
            matched_text: Some("MIT".to_string()),
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        identifier: Some("mit-package-id".to_string()),
        detection_log: vec![],
    };

    let mut manifest = file("project/Cargo.toml");
    manifest.is_key_file = true;
    manifest.package_data = vec![
        PackageData {
            package_type: Some(PackageType::Cargo),
            license_detections: vec![detection.clone()],
            declared_license_expression: Some("mit".to_string()),
            declared_license_expression_spdx: Some("MIT".to_string()),
            ..Default::default()
        },
        PackageData {
            package_type: Some(PackageType::Cargo),
            license_detections: vec![detection],
            declared_license_expression: Some("mit".to_string()),
            declared_license_expression_spdx: Some("MIT".to_string()),
            ..Default::default()
        },
    ];

    let tallies = compute_tallies(std::slice::from_ref(&manifest)).expect("tallies exist");
    assert_eq!(tallies.detected_license_expression.len(), 1);
    assert_eq!(
        tallies.detected_license_expression[0].value.as_deref(),
        Some("mit")
    );
    assert_eq!(tallies.detected_license_expression[0].count, 1);

    let key_file_tallies =
        compute_key_file_tallies(std::slice::from_ref(&manifest)).expect("key-file tallies exist");
    assert_eq!(key_file_tallies.detected_license_expression.len(), 1);
    assert_eq!(
        key_file_tallies.detected_license_expression[0]
            .value
            .as_deref(),
        Some("mit")
    );
    assert_eq!(key_file_tallies.detected_license_expression[0].count, 1);
}

#[test]
fn compute_tallies_ignores_legal_file_copyright_holder_and_author_noise() {
    let mut legal = file("project/LICENSE");
    legal.is_legal = true;
    legal.copyrights = vec![Copyright {
        copyright: "copyright and related or neighboring rights".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];
    legal.holders = vec![Holder {
        holder: "Related Rights".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];
    legal.authors = vec![Author {
        author: "be liable for".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];

    let tallies = compute_tallies(&[legal]).expect("tallies exist");

    assert_eq!(
        tallies.copyrights,
        vec![TallyEntry {
            value: None,
            count: 1
        }]
    );
    assert_eq!(
        tallies.holders,
        vec![TallyEntry {
            value: None,
            count: 1
        }]
    );
    assert_eq!(
        tallies.authors,
        vec![TallyEntry {
            value: None,
            count: 1
        }]
    );
}

#[test]
fn compute_key_file_tallies_excludes_legal_file_copyrights_holders_and_languages() {
    let mut legal = file("project/LICENSE");
    legal.is_key_file = true;
    legal.is_legal = true;
    legal.programming_language = Some("Text".to_string());
    legal.copyrights = vec![Copyright {
        copyright: "copyright and related or neighboring rights".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];
    legal.holders = vec![Holder {
        holder: "Related Rights".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];

    assert!(compute_key_file_tallies(&[legal]).is_none());
}

#[test]
fn compute_tallies_normalizes_jboss_style_copyright_and_holder_values() {
    let mut source = file("project/src/lib.java");
    source.copyrights = vec![Copyright {
        copyright: "Copyright 2005, JBoss Inc., and individual contributors as indicated by the @authors tag".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];
    source.holders = vec![Holder {
        holder: "JBoss Inc., and individual contributors as indicated by the @authors tag"
            .to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];

    let tallies = compute_tallies(&[source]).expect("tallies exist");

    assert_eq!(
        tallies.copyrights[0].value.as_deref(),
        Some("Copyright JBoss Inc., and individual contributors")
    );
    assert_eq!(
        tallies.holders[0].value.as_deref(),
        Some("JBoss Inc., and individual contributors")
    );
}

#[test]
fn compute_tallies_strips_leading_years_from_copyright_tallies() {
    let mut source = file("project/src/zlib.h");
    source.copyrights = vec![Copyright {
        copyright: "Copyright (c) 1995-2013 Jean-loup Gailly and Mark Adler".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];

    let tallies = compute_tallies(&[source]).expect("tallies exist");

    assert_eq!(
        tallies.copyrights[0].value.as_deref(),
        Some("Copyright (c) Jean-loup Gailly and Mark Adler")
    );
}

#[test]
fn compute_tallies_filters_lowercase_author_noise() {
    let mut source = file("project/src/lib.java");
    source.authors = vec![Author {
        author: "be liable for".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];

    let tallies = compute_tallies(&[source]).expect("tallies exist");

    assert_eq!(
        tallies.authors,
        vec![TallyEntry {
            value: None,
            count: 1
        }]
    );
}

#[test]
fn compute_detailed_tallies_assigns_file_and_directory_rollups() {
    let mut files = vec![
        dir("project"),
        dir("project/src"),
        dir("project/empty"),
        file("project/src/main.rs"),
        file("project/README.md"),
    ];

    files[3].license_expression = Some("mit".to_string());
    files[3].programming_language = Some("Rust".to_string());
    files[3].authors = vec![Author {
        author: "Alice".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];
    files[4].programming_language = Some("Markdown".to_string());

    compute_detailed_tallies(&mut files);

    let root = files.iter().find(|file| file.path == "project").unwrap();
    let src = files
        .iter()
        .find(|file| file.path == "project/src")
        .unwrap();
    let empty = files
        .iter()
        .find(|file| file.path == "project/empty")
        .unwrap();
    let readme = files
        .iter()
        .find(|file| file.path == "project/README.md")
        .unwrap();
    assert_eq!(
        src.tallies.as_ref().unwrap().programming_language[0]
            .value
            .as_deref(),
        Some("Rust")
    );
    assert_eq!(
        readme.tallies.as_ref().unwrap().programming_language,
        vec![TallyEntry {
            value: Some("Markdown".to_string()),
            count: 1
        }]
    );
    assert!(
        root.tallies
            .as_ref()
            .unwrap()
            .detected_license_expression
            .len()
            >= 2
    );
    assert!(empty.tallies.as_ref().unwrap().is_empty());
}
