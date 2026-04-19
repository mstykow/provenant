use super::*;
use crate::models::{
    Author, Copyright, DatasourceId, Dependency, FileReference, LineNumber, MatchScore,
    OutputEmail, OutputURL, Package, PackageData, PackageUid, TopLevelDependency,
};
use crate::scan_result_shaping::test_fixtures::{dir, file};
use regex::Regex;
use std::collections::HashSet;

#[test]
fn include_filter_keeps_matching_files_and_parent_dirs() {
    let mut files = vec![
        dir("project"),
        dir("project/src"),
        file("project/src/main.rs"),
        file("project/README.md"),
    ];

    apply_path_selection_filter(&mut files, |file| file.path.ends_with(".rs"));

    let paths: HashSet<_> = files.into_iter().map(|f| f.path).collect();
    assert!(paths.contains("project/src/main.rs"));
    assert!(paths.contains("project/src"));
    assert!(paths.contains("project"));
    assert!(!paths.contains("project/README.md"));
}

#[test]
fn path_selection_filter_keeps_matching_relative_paths_and_parent_dirs() {
    let mut files = vec![
        dir("project"),
        dir("project/src"),
        dir("project/src/test"),
        file("project/src/test/sample.doc"),
        file("project/src/test/sample.txt"),
    ];

    apply_path_selection_filter(&mut files, |file| file.path.ends_with(".doc"));

    let paths: HashSet<_> = files.into_iter().map(|f| f.path).collect();
    assert!(paths.contains("project"));
    assert!(paths.contains("project/src"));
    assert!(paths.contains("project/src/test"));
    assert!(paths.contains("project/src/test/sample.doc"));
    assert!(!paths.contains("project/src/test/sample.txt"));
}

#[test]
fn path_selection_filter_keeps_matching_directories_without_file_descendants() {
    let mut files = vec![
        dir("virtual_root"),
        dir("virtual_root/.github"),
        dir("virtual_root/.github/actions"),
        file("virtual_root/README.md"),
    ];

    apply_path_selection_filter(&mut files, |file| {
        file.path == "virtual_root"
            || file.path == "virtual_root/.github"
            || file.path == "virtual_root/.github/actions"
            || file.path.ends_with("README.md")
    });

    let paths: HashSet<_> = files.into_iter().map(|f| f.path).collect();
    assert!(paths.contains("virtual_root"));
    assert!(paths.contains("virtual_root/.github"));
    assert!(paths.contains("virtual_root/.github/actions"));
    assert!(paths.contains("virtual_root/README.md"));
}

#[test]
fn only_findings_keeps_file_with_findings_and_parent_dirs() {
    let mut files = vec![dir("project"), file("project/a.txt"), file("project/b.txt")];
    files[2].copyrights = vec![Copyright {
        copyright: "Copyright Example".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];

    apply_only_findings_filter(&mut files);

    let paths: HashSet<_> = files.into_iter().map(|f| f.path).collect();
    assert!(paths.contains("project"));
    assert!(paths.contains("project/b.txt"));
    assert!(!paths.contains("project/a.txt"));
}

#[test]
fn filter_redundant_clues_dedupes_exact_duplicates() {
    let mut files = vec![file("project/a.txt")];
    files[0].authors = vec![
        Author {
            author: "Jane".to_string(),
            start_line: LineNumber::new(2).unwrap(),
            end_line: LineNumber::new(2).unwrap(),
        },
        Author {
            author: "Jane".to_string(),
            start_line: LineNumber::new(2).unwrap(),
            end_line: LineNumber::new(2).unwrap(),
        },
    ];
    files[0].emails = vec![
        OutputEmail {
            email: "a@example.com".to_string(),
            start_line: LineNumber::new(3).unwrap(),
            end_line: LineNumber::new(3).unwrap(),
        },
        OutputEmail {
            email: "a@example.com".to_string(),
            start_line: LineNumber::new(3).unwrap(),
            end_line: LineNumber::new(3).unwrap(),
        },
    ];
    files[0].urls = vec![
        OutputURL {
            url: "https://example.com".to_string(),
            start_line: LineNumber::new(4).unwrap(),
            end_line: LineNumber::new(4).unwrap(),
        },
        OutputURL {
            url: "https://example.com".to_string(),
            start_line: LineNumber::new(4).unwrap(),
            end_line: LineNumber::new(4).unwrap(),
        },
    ];

    filter_redundant_clues(&mut files);

    assert_eq!(files[0].authors.len(), 1);
    assert_eq!(files[0].emails.len(), 1);
    assert_eq!(files[0].urls.len(), 1);
}

#[test]
fn filter_redundant_clues_keeps_distinct_line_ranges_and_dedupes_copyrights_and_holders() {
    let mut files = vec![file("project/a.txt")];
    files[0].copyrights = vec![
        Copyright {
            copyright: "Copyright Example".to_string(),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
        },
        Copyright {
            copyright: "Copyright Example".to_string(),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
        },
    ];
    files[0].holders = vec![
        crate::models::Holder {
            holder: "Example Corp".to_string(),
            start_line: LineNumber::new(2).unwrap(),
            end_line: LineNumber::new(2).unwrap(),
        },
        crate::models::Holder {
            holder: "Example Corp".to_string(),
            start_line: LineNumber::new(3).unwrap(),
            end_line: LineNumber::new(3).unwrap(),
        },
    ];

    filter_redundant_clues(&mut files);

    assert_eq!(files[0].copyrights.len(), 1);
    assert_eq!(files[0].holders.len(), 2);
}

#[test]
fn filter_redundant_clues_with_rules_suppresses_ignorable_rule_and_cross_clues() {
    let mut files = vec![file("project/a.txt")];
    files[0].license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![crate::models::Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: None,
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(5).unwrap(),
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(42),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("mit_1.RULE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        identifier: None,
        detection_log: vec![],
    }];
    files[0].copyrights = vec![Copyright {
        copyright: "Copyright Example Corp".to_string(),
        start_line: LineNumber::new(2).unwrap(),
        end_line: LineNumber::new(2).unwrap(),
    }];
    files[0].holders = vec![crate::models::Holder {
        holder: "Example Corp".to_string(),
        start_line: LineNumber::new(2).unwrap(),
        end_line: LineNumber::new(2).unwrap(),
    }];
    files[0].authors = vec![Author {
        author: "Jane Example".to_string(),
        start_line: LineNumber::new(2).unwrap(),
        end_line: LineNumber::new(2).unwrap(),
    }];
    files[0].emails = vec![OutputEmail {
        email: "legal@example.com".to_string(),
        start_line: LineNumber::new(2).unwrap(),
        end_line: LineNumber::new(2).unwrap(),
    }];
    files[0].urls = vec![OutputURL {
        url: "https://example.com/".to_string(),
        start_line: LineNumber::new(2).unwrap(),
        end_line: LineNumber::new(2).unwrap(),
    }];

    let clue_rule_lookup = HashMap::from([(
        "mit_1.RULE".to_string(),
        ClueRuleData {
            ignorable_copyrights: vec!["Copyright Example Corp".to_string()],
            ignorable_holders: vec!["Example Corp".to_string()],
            ignorable_authors: vec!["Jane Example".to_string()],
            ignorable_urls: vec!["https://example.com".to_string()],
            ignorable_emails: vec!["legal@example.com".to_string()],
        },
    )]);

    filter_redundant_clues_with_rules(&mut files, Some(&clue_rule_lookup));

    assert!(files[0].copyrights.is_empty());
    assert!(files[0].holders.is_empty());
    assert!(files[0].authors.is_empty());
    assert!(files[0].emails.is_empty());
    assert!(files[0].urls.is_empty());
}

#[test]
fn filter_redundant_clues_with_rules_keeps_non_exact_ignorable_values() {
    let mut files = vec![file("project/a.txt")];
    files[0].license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![crate::models::Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: None,
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(5).unwrap(),
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(42),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("mit_1.RULE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        identifier: None,
        detection_log: vec![],
    }];
    files[0].holders = vec![crate::models::Holder {
        holder: "Example Corp".to_string(),
        start_line: LineNumber::new(2).unwrap(),
        end_line: LineNumber::new(2).unwrap(),
    }];

    let clue_rule_lookup = HashMap::from([(
        "mit_1.RULE".to_string(),
        ClueRuleData {
            ignorable_holders: vec!["Example Corp Europe".to_string()],
            ..Default::default()
        },
    )]);

    filter_redundant_clues_with_rules(&mut files, Some(&clue_rule_lookup));

    assert_eq!(files[0].holders.len(), 1);
    assert_eq!(files[0].holders[0].holder, "Example Corp");
}

#[test]
fn filter_redundant_clues_suppresses_cross_clues_without_license_rules() {
    let mut files = vec![file("project/a.txt")];
    files[0].copyrights = vec![Copyright {
        copyright: "Copyright Example <legal@example.com> https://example.com".to_string(),
        start_line: LineNumber::new(2).unwrap(),
        end_line: LineNumber::new(2).unwrap(),
    }];
    files[0].holders = vec![crate::models::Holder {
        holder: "Jane Example".to_string(),
        start_line: LineNumber::new(2).unwrap(),
        end_line: LineNumber::new(2).unwrap(),
    }];
    files[0].authors = vec![Author {
        author: "Jane Example".to_string(),
        start_line: LineNumber::new(2).unwrap(),
        end_line: LineNumber::new(2).unwrap(),
    }];
    files[0].emails = vec![OutputEmail {
        email: "legal@example.com".to_string(),
        start_line: LineNumber::new(2).unwrap(),
        end_line: LineNumber::new(2).unwrap(),
    }];
    files[0].urls = vec![OutputURL {
        url: "https://example.com/".to_string(),
        start_line: LineNumber::new(2).unwrap(),
        end_line: LineNumber::new(2).unwrap(),
    }];

    filter_redundant_clues(&mut files);

    assert!(files[0].authors.is_empty());
    assert!(files[0].emails.is_empty());
    assert!(files[0].urls.is_empty());
    assert_eq!(files[0].holders.len(), 1);
    assert_eq!(files[0].copyrights.len(), 1);
}

#[test]
fn filter_redundant_clues_with_rules_uses_package_origin_detections() {
    let mut files = vec![file("project/package.json")];
    files[0].package_data = vec![PackageData {
        license_detections: vec![crate::models::LicenseDetection {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            matches: vec![crate::models::Match {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                from_file: Some("project/package.json".to_string()),
                start_line: LineNumber::ONE,
                end_line: LineNumber::new(5).unwrap(),
                matcher: Some("parser-declared-license".to_string()),
                score: MatchScore::MAX,
                matched_length: Some(42),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: Some("mit_1.RULE".to_string()),
                rule_url: None,
                matched_text: None,
                referenced_filenames: None,
                matched_text_diagnostics: None,
            }],
            identifier: Some("mit-from-package".to_string()),
            detection_log: vec![],
        }],
        ..Default::default()
    }];
    files[0].emails = vec![OutputEmail {
        email: "legal@example.com".to_string(),
        start_line: LineNumber::new(2).unwrap(),
        end_line: LineNumber::new(2).unwrap(),
    }];
    files[0].urls = vec![OutputURL {
        url: "https://example.com/".to_string(),
        start_line: LineNumber::new(2).unwrap(),
        end_line: LineNumber::new(2).unwrap(),
    }];

    let clue_rule_lookup = HashMap::from([(
        "mit_1.RULE".to_string(),
        ClueRuleData {
            ignorable_urls: vec!["https://example.com".to_string()],
            ignorable_emails: vec!["legal@example.com".to_string()],
            ..Default::default()
        },
    )]);

    filter_redundant_clues_with_rules(&mut files, Some(&clue_rule_lookup));

    assert!(files[0].emails.is_empty());
    assert!(files[0].urls.is_empty());
}

#[test]
fn filter_redundant_clues_with_rules_ignores_low_coverage_matches() {
    let mut files = vec![file("project/a.txt")];
    files[0].license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![crate::models::Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: None,
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(5).unwrap(),
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(42),
            match_coverage: Some(89.0),
            rule_relevance: Some(100),
            rule_identifier: Some("mit_1.RULE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        identifier: None,
        detection_log: vec![],
    }];
    files[0].emails = vec![OutputEmail {
        email: "legal@example.com".to_string(),
        start_line: LineNumber::new(2).unwrap(),
        end_line: LineNumber::new(2).unwrap(),
    }];

    let clue_rule_lookup = HashMap::from([(
        "mit_1.RULE".to_string(),
        ClueRuleData {
            ignorable_emails: vec!["legal@example.com".to_string()],
            ..Default::default()
        },
    )]);

    filter_redundant_clues_with_rules(&mut files, Some(&clue_rule_lookup));

    assert_eq!(files[0].emails.len(), 1);
}

#[test]
fn ignore_resource_filter_removes_matching_files_and_preserves_needed_dirs() {
    let mut files = vec![
        dir("project"),
        dir("project/sub"),
        file("project/keep.txt"),
        file("project/drop-author.txt"),
        file("project/sub/drop-holder.txt"),
        file("project/sub/keep.rs"),
    ];
    files[3].authors = vec![Author {
        author: "Jane Doe".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];
    files[3].license_expression = Some("mit".to_string());
    files[4].holders = vec![crate::models::Holder {
        holder: "Example Corp".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];
    files[4].scan_errors = vec!["should still be dropped".to_string()];

    let ignored_holders = vec![Regex::new("Example Corp").expect("valid holder regex")];
    let ignored_authors = vec![Regex::new("Jane.*").expect("valid author regex")];

    apply_ignore_resource_filter(&mut files, &ignored_holders, &ignored_authors);

    let paths: HashSet<_> = files.into_iter().map(|f| f.path).collect();
    assert!(paths.contains("project"));
    assert!(paths.contains("project/sub"));
    assert!(paths.contains("project/keep.txt"));
    assert!(paths.contains("project/sub/keep.rs"));
    assert!(!paths.contains("project/drop-author.txt"));
    assert!(!paths.contains("project/sub/drop-holder.txt"));
}

#[test]
fn normalize_paths_strip_root_removes_scan_root_prefix() {
    let mut files = vec![file("project/src/main.rs")];
    normalize_paths(&mut files, "project", true, false);
    assert_eq!(files[0].path, "src/main.rs");
}

#[test]
fn normalize_paths_full_root_keeps_absolute_paths() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let file_path = temp.path().join("src").join("main.rs");
    std::fs::create_dir_all(file_path.parent().unwrap()).expect("parent dir should exist");
    std::fs::write(&file_path, "fn main() {}\n").expect("file should be written");

    let mut files = vec![file(file_path.to_str().unwrap())];
    normalize_paths(&mut files, temp.path().to_str().unwrap(), false, true);

    assert_eq!(
        files[0].path,
        file_path
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/")
            .trim_matches('/')
            .to_string()
    );
}

#[test]
fn normalize_paths_updates_package_file_references_too() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let root = temp.path().join("project");
    let referenced = root.join("src").join("main.rs");
    std::fs::create_dir_all(referenced.parent().unwrap()).expect("parent dir should exist");
    std::fs::write(&referenced, "fn main() {}\n").expect("file should be written");

    let mut manifest = file(root.join("package.json").to_str().unwrap());
    manifest.package_data = vec![crate::models::PackageData {
        file_references: vec![FileReference {
            path: referenced.to_string_lossy().to_string(),
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            extra_data: None,
        }],
        ..Default::default()
    }];

    let mut files = vec![manifest];
    normalize_paths(&mut files, root.to_str().unwrap(), true, false);

    assert_eq!(files[0].path, "package.json");
    assert_eq!(
        files[0].package_data[0].file_references[0].path,
        "src/main.rs"
    );
}

#[test]
fn normalize_paths_updates_license_match_from_file_paths_too() {
    let mut files = vec![file("project/NOTICE")];
    files[0].license_clues = vec![crate::models::Match {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        from_file: Some("project/NOTICE".to_string()),
        start_line: LineNumber::ONE,
        end_line: LineNumber::new(2).unwrap(),
        matcher: Some("2-aho".to_string()),
        score: MatchScore::MAX,
        matched_length: Some(12),
        match_coverage: Some(100.0),
        rule_relevance: Some(100),
        rule_identifier: Some("mit_1.RULE".to_string()),
        rule_url: None,
        matched_text: None,
        referenced_filenames: None,
        matched_text_diagnostics: None,
    }];
    files[0].license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![crate::models::Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/LICENSE".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::new(5).unwrap(),
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(42),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("mit_1.RULE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        identifier: None,
        detection_log: vec![],
    }];

    normalize_paths(&mut files, "project", true, false);

    assert_eq!(files[0].path, "NOTICE");
    assert_eq!(
        files[0].license_clues[0].from_file.as_deref(),
        Some("NOTICE")
    );
    assert_eq!(
        files[0].license_detections[0].matches[0]
            .from_file
            .as_deref(),
        Some("LICENSE")
    );
}

#[test]
fn normalize_paths_updates_package_level_license_match_from_file_paths_too() {
    let mut manifest = file("project/package.json");
    manifest.package_data = vec![PackageData {
        license_detections: vec![crate::models::LicenseDetection {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            matches: vec![crate::models::Match {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                from_file: Some("project/LICENSE".to_string()),
                start_line: LineNumber::ONE,
                end_line: LineNumber::new(5).unwrap(),
                matcher: Some("2-aho".to_string()),
                score: MatchScore::MAX,
                matched_length: Some(42),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: Some("mit_1.RULE".to_string()),
                rule_url: None,
                matched_text: None,
                referenced_filenames: None,
                matched_text_diagnostics: None,
            }],
            identifier: None,
            detection_log: vec![],
        }],
        other_license_detections: vec![crate::models::LicenseDetection {
            license_expression: "apache-2.0".to_string(),
            license_expression_spdx: "Apache-2.0".to_string(),
            matches: vec![crate::models::Match {
                license_expression: "apache-2.0".to_string(),
                license_expression_spdx: "Apache-2.0".to_string(),
                from_file: Some("project/NOTICE".to_string()),
                start_line: LineNumber::ONE,
                end_line: LineNumber::new(3).unwrap(),
                matcher: Some("2-aho".to_string()),
                score: MatchScore::MAX,
                matched_length: Some(30),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: Some("apache_2_0_1.RULE".to_string()),
                rule_url: None,
                matched_text: None,
                referenced_filenames: None,
                matched_text_diagnostics: None,
            }],
            identifier: None,
            detection_log: vec![],
        }],
        ..Default::default()
    }];

    let mut files = vec![manifest];
    normalize_paths(&mut files, "project", true, false);

    assert_eq!(
        files[0].package_data[0].license_detections[0].matches[0]
            .from_file
            .as_deref(),
        Some("LICENSE")
    );
    assert_eq!(
        files[0].package_data[0].other_license_detections[0].matches[0]
            .from_file
            .as_deref(),
        Some("NOTICE")
    );
}

#[test]
fn only_findings_keeps_all_supported_finding_types() {
    let mut files = vec![
        dir("project"),
        file("project/license.txt"),
        file("project/pkg.json"),
        file("project/error.txt"),
        file("project/empty.txt"),
    ];
    files[1].license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![],
        identifier: None,
        detection_log: vec![],
    }];
    files[2].package_data = vec![crate::models::PackageData::default()];
    files[3].scan_errors = vec!["boom".to_string()];

    apply_only_findings_filter(&mut files);

    let paths: HashSet<_> = files.into_iter().map(|f| f.path).collect();
    assert!(paths.contains("project/license.txt"));
    assert!(paths.contains("project/pkg.json"));
    assert!(paths.contains("project/error.txt"));
    assert!(!paths.contains("project/empty.txt"));
}

#[test]
fn only_findings_keeps_clue_only_files() {
    let mut files = vec![
        dir("project"),
        file("project/NOTICE"),
        file("project/empty.txt"),
    ];
    files[1].license_clues = vec![crate::models::Match {
        license_expression: "unknown-license-reference".to_string(),
        license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
        from_file: Some("project/NOTICE".to_string()),
        start_line: LineNumber::ONE,
        end_line: LineNumber::new(2).unwrap(),
        matcher: Some("2-aho".to_string()),
        score: MatchScore::MAX,
        matched_length: Some(19),
        match_coverage: Some(100.0),
        rule_relevance: Some(100),
        rule_identifier: Some("license-clue_1.RULE".to_string()),
        rule_url: None,
        matched_text: None,
        referenced_filenames: None,
        matched_text_diagnostics: None,
    }];

    apply_only_findings_filter(&mut files);

    let paths: HashSet<_> = files.into_iter().map(|f| f.path).collect();
    assert!(paths.contains("project/NOTICE"));
    assert!(!paths.contains("project/empty.txt"));
}

#[test]
fn only_findings_keeps_generated_only_files() {
    let mut files = vec![
        dir("project"),
        file("project/generated.js"),
        file("project/empty.txt"),
    ];
    files[1].is_generated = Some(true);

    apply_only_findings_filter(&mut files);

    let paths: HashSet<_> = files.into_iter().map(|f| f.path).collect();
    assert!(paths.contains("project"));
    assert!(paths.contains("project/generated.js"));
    assert!(!paths.contains("project/empty.txt"));
}

#[test]
fn mark_source_sets_directory_flags_at_threshold() {
    let mut files = vec![
        dir("project"),
        dir("project/src"),
        file("project/src/a.rs"),
        file("project/src/b.rs"),
        file("project/src/c.txt"),
    ];
    files[2].is_source = Some(true);
    files[3].is_source = Some(true);
    files[4].is_source = Some(false);

    apply_mark_source(&mut files);

    let src = files
        .iter()
        .find(|f| f.path == "project/src")
        .expect("src directory exists");
    assert_eq!(src.is_source, Some(false));
    assert_eq!(src.source_count, Some(0));

    files[4].is_source = Some(true);
    apply_mark_source(&mut files);

    let src_after = files
        .iter()
        .find(|f| f.path == "project/src")
        .expect("src directory exists");
    assert_eq!(src_after.is_source, Some(true));
    assert_eq!(src_after.source_count, Some(3));
}

#[test]
fn mark_source_ignores_go_test_only_files_for_directory_threshold() {
    let mut files = vec![
        dir("module"),
        file("module/main.go"),
        file("module/helper.go"),
        file("module/helper_test.go"),
    ];
    files[1].programming_language = Some("Go".to_string());
    files[2].programming_language = Some("Go".to_string());
    files[3].programming_language = Some("Go".to_string());
    files[1].is_source = Some(true);
    files[2].is_source = Some(true);
    files[3].is_source = Some(false);

    apply_mark_source(&mut files);

    let module_dir = files
        .iter()
        .find(|f| f.path == "module")
        .expect("module dir exists");
    assert_eq!(module_dir.is_source, Some(true));
    assert_eq!(module_dir.source_count, Some(2));
}

#[test]
fn mark_source_propagates_counts_through_nested_directories() {
    let mut files = vec![
        dir("project"),
        dir("project/src"),
        dir("project/src/nested"),
        file("project/src/nested/a.rs"),
        file("project/src/nested/b.rs"),
    ];
    files[3].is_source = Some(true);
    files[4].is_source = Some(true);

    apply_mark_source(&mut files);

    let root = files.iter().find(|f| f.path == "project").unwrap();
    let src = files.iter().find(|f| f.path == "project/src").unwrap();
    let nested = files
        .iter()
        .find(|f| f.path == "project/src/nested")
        .unwrap();
    assert_eq!(root.is_source, Some(true));
    assert_eq!(root.source_count, Some(2));
    assert_eq!(src.is_source, Some(true));
    assert_eq!(src.source_count, Some(2));
    assert_eq!(nested.is_source, Some(true));
    assert_eq!(nested.source_count, Some(2));
}

#[test]
fn trim_preloaded_assembly_to_files_drops_unreferenced_packages_and_dependencies() {
    let files = vec![dir("project"), file("project/keep-package.json")];

    let mut packages = vec![
        Package::from_package_data(
            &PackageData {
                datasource_id: Some(DatasourceId::NpmPackageJson),
                ..Default::default()
            },
            "project/keep-package.json".to_string(),
        ),
        Package::from_package_data(
            &PackageData {
                datasource_id: Some(DatasourceId::NpmPackageJson),
                ..Default::default()
            },
            "project/drop-package.json".to_string(),
        ),
    ];
    packages[0].package_uid = PackageUid::from_raw("pkg:npm/keep@1.0.0?uuid=keep".to_string());
    packages[1].package_uid = PackageUid::from_raw("pkg:npm/drop@1.0.0?uuid=drop".to_string());

    let mut dependencies = vec![
        TopLevelDependency::from_dependency(
            &Dependency {
                purl: Some("pkg:npm/dep@1.0.0".to_string()),
                extracted_requirement: None,
                scope: Some("dependencies".to_string()),
                is_runtime: Some(true),
                is_optional: Some(false),
                is_pinned: Some(true),
                is_direct: Some(true),
                resolved_package: None,
                extra_data: None,
            },
            "project/keep-package.json".to_string(),
            DatasourceId::NpmPackageJson,
            Some(PackageUid::from_raw(
                "pkg:npm/keep@1.0.0?uuid=keep".to_string(),
            )),
        ),
        TopLevelDependency::from_dependency(
            &Dependency {
                purl: Some("pkg:npm/dep-drop@1.0.0".to_string()),
                extracted_requirement: None,
                scope: Some("dependencies".to_string()),
                is_runtime: Some(true),
                is_optional: Some(false),
                is_pinned: Some(true),
                is_direct: Some(true),
                resolved_package: None,
                extra_data: None,
            },
            "project/drop-package.json".to_string(),
            DatasourceId::NpmPackageJson,
            Some(PackageUid::from_raw(
                "pkg:npm/drop@1.0.0?uuid=drop".to_string(),
            )),
        ),
    ];

    trim_preloaded_assembly_to_files(&files, &mut packages, &mut dependencies);

    assert_eq!(packages.len(), 1);
    assert_eq!(
        packages[0].datafile_paths,
        vec!["project/keep-package.json"]
    );
    assert_eq!(dependencies.len(), 1);
    assert_eq!(dependencies[0].datafile_path, "project/keep-package.json");
}

#[test]
fn normalize_top_level_output_paths_only_applies_strip_root() {
    let mut packages = vec![Package::from_package_data(
        &PackageData {
            datasource_id: Some(DatasourceId::NpmPackageJson),
            ..Default::default()
        },
        "/tmp/project/package.json".to_string(),
    )];
    let mut dependencies = vec![TopLevelDependency::from_dependency(
        &Dependency {
            purl: Some("pkg:npm/dep@1.0.0".to_string()),
            extracted_requirement: None,
            scope: Some("dependencies".to_string()),
            is_runtime: Some(true),
            is_optional: Some(false),
            is_pinned: Some(true),
            is_direct: Some(true),
            resolved_package: None,
            extra_data: None,
        },
        "/tmp/project/package.json".to_string(),
        DatasourceId::NpmPackageJson,
        Some(PackageUid::from_raw(
            "pkg:npm/demo@1.0.0?uuid=demo".to_string(),
        )),
    )];

    normalize_top_level_output_paths(&mut packages, &mut dependencies, "/tmp/project", true);

    assert_eq!(packages[0].datafile_paths, vec!["package.json"]);
    assert_eq!(dependencies[0].datafile_path, "package.json");
}
