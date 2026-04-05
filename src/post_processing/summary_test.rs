use super::test_utils::{dir, file, package};
use super::*;
use crate::models::{
    Copyright, DatasourceId, FileReference, Holder, Match, Package, PackageType, TallyEntry,
};

#[test]
fn key_file_license_clues_feed_summary_without_mutating_package_license_provenance() {
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
    license_file.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "apache-2.0".to_string(),
        license_expression_spdx: "Apache-2.0".to_string(),
        matches: vec![Match {
            license_expression: "apache-2.0".to_string(),
            license_expression_spdx: "Apache-2.0".to_string(),
            from_file: Some("inspec-6.8.2/inspec-bin/LICENSE".to_string()),
            start_line: 1,
            end_line: 20,
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
    license_file.copyrights = vec![Copyright {
        copyright: "Copyright (c) 2019 Chef Software Inc.".to_string(),
        start_line: 1,
        end_line: 1,
    }];
    license_file.holders = vec![Holder {
        holder: "Chef Software Inc.".to_string(),
        start_line: 1,
        end_line: 1,
    }];

    let mut files = vec![metadata_file, license_file];
    let mut packages = vec![package(uid, "inspec-6.8.2/metadata.gz-extract")];

    let package_file_index = PackageFileIndex::build(&files, &packages);
    apply_file_classification(&mut files, &package_file_index);
    let indexes = OutputIndexes::build(
        &files,
        Some(&package_file_index),
        false,
        OutputIndexMode::Full,
    );
    promote_package_metadata_from_key_files(&files, &mut packages, &indexes);
    let summary = compute_summary(&files, &packages).expect("summary exists");

    assert_eq!(packages[0].holder.as_deref(), Some("Chef Software Inc."));
    assert!(packages[0].declared_license_expression.is_none());
    assert!(packages[0].declared_license_expression_spdx.is_none());
    assert!(packages[0].license_detections.is_empty());
    assert_eq!(
        summary.declared_license_expression.as_deref(),
        Some("apache-2.0")
    );
    let score = summary.license_clarity_score.expect("score exists");
    assert_eq!(score.score, 100);
    assert!(score.declared_license);
    assert!(score.identification_precision);
    assert!(score.has_license_text);
    assert!(score.declared_copyrights);
    assert!(!score.ambiguous_compound_licensing);
}

#[test]
fn manifest_declared_license_survives_into_package_and_summary() {
    let mut gemspec = file("demo/demo.gemspec");
    gemspec.package_data = vec![crate::models::PackageData {
        package_type: Some(PackageType::Gem),
        datasource_id: Some(DatasourceId::Gemspec),
        declared_license_expression: Some("mit".to_string()),
        declared_license_expression_spdx: Some("MIT".to_string()),
        license_detections: vec![crate::models::LicenseDetection {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            matches: vec![Match {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                from_file: Some("demo/demo.gemspec".to_string()),
                start_line: 1,
                end_line: 1,
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
        }],
        ..Default::default()
    }];

    let package =
        Package::from_package_data(&gemspec.package_data[0], "demo/demo.gemspec".to_string());
    gemspec.for_packages.push(package.package_uid.clone());
    gemspec.license_expression = Some("mit".to_string());
    gemspec.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("demo/demo.gemspec".to_string()),
            start_line: 1,
            end_line: 1,
            matcher: Some("1-spdx-id".to_string()),
            score: 100.0,
            matched_length: Some(1),
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

    let mut files = vec![gemspec];
    let mut packages = vec![package];

    let package_file_index = PackageFileIndex::build(&files, &packages);
    apply_file_classification(&mut files, &package_file_index);
    let indexes = OutputIndexes::build(
        &files,
        Some(&package_file_index),
        false,
        OutputIndexMode::Full,
    );
    promote_package_metadata_from_key_files(&files, &mut packages, &indexes);
    let summary = compute_summary(&files, &packages).expect("summary exists");

    assert!(files[0].is_manifest);
    assert!(files[0].is_key_file);
    assert_eq!(
        packages[0].declared_license_expression_spdx.as_deref(),
        Some("MIT")
    );
    assert_eq!(packages[0].license_detections.len(), 1);
    assert_eq!(
        packages[0].license_detections[0].license_expression_spdx,
        "MIT"
    );
    assert_eq!(summary.declared_license_expression.as_deref(), Some("mit"));
    assert_eq!(summary.license_clarity_score.unwrap().score, 90);
}

#[test]
fn summary_other_license_expressions_include_license_clues() {
    let mut clue_file = file("project/NOTICE");
    clue_file.license_clues = vec![Match {
        license_expression: "unknown-spdx".to_string(),
        license_expression_spdx: "LicenseRef-scancode-unknown-spdx".to_string(),
        from_file: Some("project/NOTICE".to_string()),
        start_line: 2,
        end_line: 2,
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

    let summary = compute_summary(&[clue_file], &[]).expect("summary exists");

    assert!(summary.declared_license_expression.is_none());
    assert_eq!(
        summary.other_license_expressions,
        vec![TallyEntry {
            value: Some("unknown-spdx".to_string()),
            count: 1,
        }]
    );
}

#[test]
fn compute_summary_includes_package_other_license_detections_as_other_expressions() {
    let mut manifest = file("project/package.json");
    manifest.package_data = vec![crate::models::PackageData {
        package_type: Some(PackageType::Npm),
        other_license_detections: vec![crate::models::LicenseDetection {
            license_expression: "gpl-2.0-only".to_string(),
            license_expression_spdx: "GPL-2.0-only".to_string(),
            matches: vec![Match {
                license_expression: "gpl-2.0-only".to_string(),
                license_expression_spdx: "GPL-2.0-only".to_string(),
                from_file: Some("project/package.json".to_string()),
                start_line: 1,
                end_line: 1,
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

    let summary = compute_summary(&[manifest], &[]).expect("summary exists");

    assert!(summary.declared_license_expression.is_none());
    assert_eq!(summary.other_license_expressions.len(), 1);
    assert_eq!(
        summary.other_license_expressions[0].value.as_deref(),
        Some("gpl-2.0-only")
    );
}

#[test]
fn compute_summary_uses_manifest_package_license_detections_when_file_detections_are_empty() {
    let mut manifest = file("project/Cargo.toml");
    manifest.is_manifest = true;
    manifest.is_key_file = true;
    manifest.is_top_level = true;
    manifest.package_data = vec![crate::models::PackageData {
        package_type: Some(PackageType::Cargo),
        datasource_id: Some(DatasourceId::CargoToml),
        declared_license_expression: Some("mit".to_string()),
        declared_license_expression_spdx: Some("MIT".to_string()),
        license_detections: vec![crate::models::LicenseDetection {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            matches: vec![Match {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                from_file: Some("project/Cargo.toml".to_string()),
                start_line: 1,
                end_line: 1,
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
            identifier: None,
            detection_log: vec!["unknown-reference-to-local-file".to_string()],
        }],
        ..Default::default()
    }];

    let summary = compute_summary(&[manifest], &[]).expect("summary exists");

    assert_eq!(summary.declared_license_expression.as_deref(), Some("mit"));
    let score = summary.license_clarity_score.expect("score exists");
    assert!(score.declared_license);
    assert!(score.identification_precision);
}

#[test]
fn compute_summary_prefers_file_license_detections_over_duplicate_package_data_detections() {
    let detection = crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/Cargo.toml".to_string()),
            start_line: 1,
            end_line: 1,
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
    manifest.is_manifest = true;
    manifest.is_key_file = true;
    manifest.is_top_level = true;
    manifest.license_detections = vec![detection.clone()];
    manifest.license_expression = Some("mit".to_string());
    manifest.package_data = vec![crate::models::PackageData {
        package_type: Some(PackageType::Cargo),
        datasource_id: Some(DatasourceId::CargoToml),
        declared_license_expression: Some("mit".to_string()),
        declared_license_expression_spdx: Some("MIT".to_string()),
        license_detections: vec![detection],
        ..Default::default()
    }];

    let summary = compute_summary(&[manifest], &[]).expect("summary exists");

    assert_eq!(summary.declared_license_expression.as_deref(), Some("mit"));
    assert!(summary.other_license_expressions.is_empty());
    let score = summary.license_clarity_score.expect("score exists");
    assert_eq!(score.score, 90);
    assert!(score.declared_license);
    assert!(score.identification_precision);
}

#[test]
fn compute_summary_deduplicates_duplicate_other_license_package_data_entries_per_file() {
    let other_detection = crate::models::LicenseDetection {
        license_expression: "gpl-2.0-only".to_string(),
        license_expression_spdx: "GPL-2.0-only".to_string(),
        matches: vec![Match {
            license_expression: "gpl-2.0-only".to_string(),
            license_expression_spdx: "GPL-2.0-only".to_string(),
            from_file: Some("project/package.json".to_string()),
            start_line: 1,
            end_line: 1,
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
    };

    let mut manifest = file("project/package.json");
    manifest.package_data = vec![
        crate::models::PackageData {
            package_type: Some(PackageType::Npm),
            other_license_detections: vec![other_detection.clone()],
            ..Default::default()
        },
        crate::models::PackageData {
            package_type: Some(PackageType::Npm),
            other_license_detections: vec![other_detection],
            ..Default::default()
        },
    ];

    let summary = compute_summary(&[manifest], &[]).expect("summary exists");

    assert_eq!(summary.declared_license_expression, None);
    assert_eq!(summary.other_license_expressions.len(), 1);
    assert_eq!(
        summary.other_license_expressions[0].value.as_deref(),
        Some("gpl-2.0-only")
    );
    assert_eq!(summary.other_license_expressions[0].count, 1);
}

#[test]
fn compute_summary_deduplicates_duplicate_primary_package_data_entries_per_file() {
    let detection = crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/Cargo.toml".to_string()),
            start_line: 1,
            end_line: 1,
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
    manifest.is_manifest = true;
    manifest.is_key_file = true;
    manifest.is_top_level = true;
    manifest.package_data = vec![
        crate::models::PackageData {
            package_type: Some(PackageType::Cargo),
            datasource_id: Some(DatasourceId::CargoToml),
            declared_license_expression: Some("mit".to_string()),
            declared_license_expression_spdx: Some("MIT".to_string()),
            license_detections: vec![detection.clone()],
            ..Default::default()
        },
        crate::models::PackageData {
            package_type: Some(PackageType::Cargo),
            datasource_id: Some(DatasourceId::CargoToml),
            declared_license_expression: Some("mit".to_string()),
            declared_license_expression_spdx: Some("MIT".to_string()),
            license_detections: vec![detection],
            ..Default::default()
        },
    ];

    let summary = compute_summary(&[manifest], &[]).expect("summary exists");

    assert_eq!(summary.declared_license_expression.as_deref(), Some("mit"));
    assert!(summary.other_license_expressions.is_empty());
    let score = summary.license_clarity_score.expect("score exists");
    assert_eq!(score.score, 90);
    assert!(score.declared_license);
    assert!(score.identification_precision);
}

#[test]
fn compute_summary_uses_root_prefixed_top_level_key_files() {
    let mut files = vec![dir("project"), file("project/LICENSE")];
    files[1].license_expression = Some("mit".to_string());
    files[1].license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/LICENSE".to_string()),
            start_line: 1,
            end_line: 1,
            matcher: Some("1-hash".to_string()),
            score: 100.0,
            matched_length: Some(10),
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

    classify_key_files(&mut files, &[]);
    let summary = compute_summary(&files, &[]).expect("summary exists");

    assert!(files[1].is_top_level);
    assert!(files[1].is_key_file);
    assert_eq!(summary.declared_license_expression.as_deref(), Some("mit"));
    assert_eq!(
        summary
            .license_clarity_score
            .as_ref()
            .map(|score| score.score),
        Some(90)
    );
}

#[test]
fn compute_summary_uses_package_holder_and_primary_language() {
    let uid = "pkg:gem/demo@1.0.0?uuid=test";
    let mut root_package = package(uid, "demo/demo.gemspec");
    root_package.holder = Some("Example Corp.".to_string());
    root_package.primary_language = Some("Ruby".to_string());

    let mut other = package("pkg:pypi/demo?uuid=test2", "demo/setup.py");
    other.package_type = Some(PackageType::Pypi);
    other.purl = Some("pkg:pypi/demo".to_string());
    other.holder = None;

    let mut extra_ruby = package("pkg:gem/demo-extra@1.0.0?uuid=test3", "demo/extra.gemspec");
    extra_ruby.name = Some("demo-extra".to_string());
    extra_ruby.purl = Some("pkg:gem/demo-extra@1.0.0".to_string());

    let mut python = file("demo/helper.py");
    python.programming_language = Some("Python".to_string());
    python.is_source = Some(true);

    let summary =
        compute_summary(&[python], &[root_package, other, extra_ruby]).expect("summary exists");
    assert_eq!(summary.declared_holder.as_deref(), Some("Example Corp."));
    assert_eq!(summary.primary_language.as_deref(), Some("Ruby"));
    assert_eq!(summary.other_languages[0].value.as_deref(), Some("Python"));
}

#[test]
fn compute_summary_prefers_package_origin_info_and_preserves_other_tallies() {
    let mut package = package("pkg:pypi/codebase?uuid=test", "codebase/setup.py");
    package.declared_license_expression = Some("apache-2.0".to_string());
    package.primary_language = Some("Python".to_string());
    package.holder = Some("Example Corp.".to_string());

    let mut readme = file("codebase/README.txt");
    readme.is_key_file = true;
    readme.is_readme = true;
    readme.is_top_level = true;
    readme.license_expression = Some("apache-2.0 AND (apache-2.0 OR mit)".to_string());

    let mut apache = file("codebase/apache-2.0.LICENSE");
    apache.is_key_file = true;
    apache.is_legal = true;
    apache.is_top_level = true;
    apache.license_expression = Some("apache-2.0".to_string());
    apache.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "apache-2.0".to_string(),
        license_expression_spdx: "Apache-2.0".to_string(),
        matches: vec![Match {
            license_expression: "apache-2.0".to_string(),
            license_expression_spdx: "Apache-2.0".to_string(),
            from_file: Some("codebase/apache-2.0.LICENSE".to_string()),
            start_line: 1,
            end_line: 1,
            matcher: Some("1-hash".to_string()),
            score: 100.0,
            matched_length: Some(10),
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

    let mut mit = file("codebase/mit.LICENSE");
    mit.is_key_file = true;
    mit.is_legal = true;
    mit.is_top_level = true;
    mit.license_expression = Some("mit".to_string());
    mit.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("codebase/mit.LICENSE".to_string()),
            start_line: 1,
            end_line: 1,
            matcher: Some("1-hash".to_string()),
            score: 100.0,
            matched_length: Some(10),
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

    let summary = compute_summary(&[readme, apache, mit], &[package]).expect("summary exists");
    assert_eq!(
        summary.declared_license_expression.as_deref(),
        Some("apache-2.0")
    );
    assert_eq!(summary.declared_holder.as_deref(), Some("Example Corp."));
    assert_eq!(summary.primary_language.as_deref(), Some("Python"));
    assert_eq!(summary.other_license_expressions.len(), 1);
}

#[test]
fn compute_summary_resolves_joined_primary_license_without_ambiguity() {
    let mut readme = file("codebase/README.txt");
    readme.is_key_file = true;
    readme.is_readme = true;
    readme.is_top_level = true;
    readme.license_expression = Some("apache-2.0 AND (apache-2.0 OR mit)".to_string());
    readme.copyrights = vec![Copyright {
        copyright: "Copyright Example Corp.".to_string(),
        start_line: 1,
        end_line: 1,
    }];

    let mut apache = file("codebase/apache-2.0.LICENSE");
    apache.is_key_file = true;
    apache.is_legal = true;
    apache.is_top_level = true;
    apache.license_expression = Some("apache-2.0".to_string());
    apache.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "apache-2.0".to_string(),
        license_expression_spdx: "Apache-2.0".to_string(),
        matches: vec![Match {
            license_expression: "apache-2.0".to_string(),
            license_expression_spdx: "Apache-2.0".to_string(),
            from_file: Some("codebase/apache-2.0.LICENSE".to_string()),
            start_line: 1,
            end_line: 1,
            matcher: Some("1-hash".to_string()),
            score: 100.0,
            matched_length: Some(10),
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

    let mut mit = file("codebase/mit.LICENSE");
    mit.is_key_file = true;
    mit.is_legal = true;
    mit.is_top_level = true;
    mit.license_expression = Some("mit".to_string());
    mit.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("codebase/mit.LICENSE".to_string()),
            start_line: 1,
            end_line: 1,
            matcher: Some("1-hash".to_string()),
            score: 100.0,
            matched_length: Some(10),
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

    let summary = compute_summary(&[readme, apache, mit], &[]).expect("summary exists");
    let score = summary.license_clarity_score.expect("clarity exists");
    assert_eq!(
        summary.declared_license_expression.as_deref(),
        Some("apache-2.0 AND mit")
    );
    assert_eq!(score.score, 90);
    assert!(score.ambiguous_compound_licensing);
    assert!(!score.conflicting_license_categories);
}

#[test]
fn compute_summary_penalizes_conflicting_non_key_licenses_without_false_ambiguity() {
    let mut readme = file("codebase/README.txt");
    readme.is_key_file = true;
    readme.is_readme = true;
    readme.is_top_level = true;
    readme.copyrights = vec![Copyright {
        copyright: "Copyright Example Corp.".to_string(),
        start_line: 1,
        end_line: 1,
    }];

    let mut mit = file("codebase/mit.LICENSE");
    mit.is_key_file = true;
    mit.is_legal = true;
    mit.is_top_level = true;
    mit.license_expression = Some("mit".to_string());
    mit.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("codebase/mit.LICENSE".to_string()),
            start_line: 1,
            end_line: 1,
            matcher: Some("1-hash".to_string()),
            score: 100.0,
            matched_length: Some(10),
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

    let mut non_key_gpl = file("codebase/tests/test_a.py");
    non_key_gpl.license_expression = Some("gpl-2.0-only".to_string());
    non_key_gpl.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "gpl-2.0-only".to_string(),
        license_expression_spdx: "GPL-2.0-only".to_string(),
        matches: vec![Match {
            license_expression: "gpl-2.0-only".to_string(),
            license_expression_spdx: "GPL-2.0-only".to_string(),
            from_file: Some("codebase/tests/test_a.py".to_string()),
            start_line: 1,
            end_line: 1,
            matcher: Some("2-aho".to_string()),
            score: 100.0,
            matched_length: Some(10),
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

    let summary = compute_summary(&[readme, mit, non_key_gpl], &[]).expect("summary exists");
    let score = summary.license_clarity_score.expect("clarity exists");
    assert_eq!(summary.declared_license_expression.as_deref(), Some("mit"));
    assert_eq!(score.score, 80);
    assert!(score.conflicting_license_categories);
}

#[test]
fn compute_summary_uses_package_datafile_holders_before_global_holder_fallback() {
    let mut package = package("pkg:pypi/atheris?uuid=test", "codebase/setup.py");
    package.declared_license_expression = Some("apache-2.0".to_string());
    package.primary_language = Some("Python".to_string());
    package.holder = None;
    package.datafile_paths = vec!["codebase/setup.py".to_string()];

    let mut setup_py = file("codebase/setup.py");
    setup_py.is_manifest = true;
    setup_py.is_key_file = true;
    setup_py.is_top_level = true;
    setup_py.for_packages = vec![package.package_uid.clone()];
    setup_py.holders = vec![
        Holder {
            holder: "Google".to_string(),
            start_line: 1,
            end_line: 1,
        },
        Holder {
            holder: "Fraunhofer FKIE".to_string(),
            start_line: 2,
            end_line: 2,
        },
    ];

    let mut readme = file("codebase/README.txt");
    readme.is_readme = true;
    readme.is_key_file = true;
    readme.is_top_level = true;
    readme.holders = vec![Holder {
        holder: "Example Corporation".to_string(),
        start_line: 1,
        end_line: 1,
    }];

    let summary = compute_summary(&[setup_py, readme], &[package]).expect("summary exists");
    assert_eq!(
        summary.declared_holder.as_deref(),
        Some("Google, Fraunhofer FKIE")
    );
    assert_eq!(
        summary.other_holders[0].value.as_deref(),
        Some("Example Corporation")
    );
}

#[test]
fn compute_summary_prefers_package_copyright_holders_over_package_resource_holders() {
    let mut package = package("pkg:nuget/demo?uuid=test", "codebase/Demo.nuspec");
    package.package_type = Some(PackageType::Nuget);
    package.copyright = Some("Copyright Example Corp.".to_string());
    package.declared_license_expression = Some("mit".to_string());

    let mut nuspec = file("codebase/Demo.nuspec");
    nuspec.is_manifest = true;
    nuspec.is_key_file = true;
    nuspec.is_top_level = true;
    nuspec.for_packages = vec![package.package_uid.clone()];
    nuspec.holders = vec![Holder {
        holder: "Different Holder".to_string(),
        start_line: 1,
        end_line: 1,
    }];

    let summary = compute_summary(&[nuspec], &[package]).expect("summary exists");

    assert_eq!(summary.declared_holder.as_deref(), Some("Example Corp."));
    assert_eq!(
        summary.other_holders,
        vec![TallyEntry {
            value: Some("Different Holder".to_string()),
            count: 1,
        }]
    );
}

#[test]
fn compute_summary_keeps_null_other_license_expressions_when_declared_expression_exists() {
    let mut readme = file("project/README.md");
    readme.is_key_file = true;
    readme.is_readme = true;
    readme.is_top_level = true;

    let mut mit = file("project/LICENSE");
    mit.is_key_file = true;
    mit.is_legal = true;
    mit.is_top_level = true;
    mit.license_expression = Some("mit".to_string());
    mit.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/LICENSE".to_string()),
            start_line: 1,
            end_line: 1,
            matcher: Some("1-hash".to_string()),
            score: 100.0,
            matched_length: Some(10),
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

    let summary = compute_summary(&[readme, mit], &[]).expect("summary exists");
    assert_eq!(
        summary.other_license_expressions,
        vec![TallyEntry {
            value: None,
            count: 1
        }]
    );
}

#[test]
fn compute_summary_keeps_null_other_holders_and_removes_declared_holder_only() {
    let mut readme = file("project/README.md");
    readme.is_key_file = true;
    readme.is_readme = true;
    readme.is_top_level = true;
    readme.holders = vec![Holder {
        holder: "Example Corp.".to_string(),
        start_line: 1,
        end_line: 1,
    }];

    let mut authors = file("project/AUTHORS");
    authors.is_community = true;
    authors.holders = vec![Holder {
        holder: "Demo Corp.".to_string(),
        start_line: 1,
        end_line: 1,
    }];

    let mut license = file("project/LICENSE");
    license.is_key_file = true;
    license.is_legal = true;
    license.is_top_level = true;

    let summary = compute_summary(&[readme, authors, license], &[]).expect("summary exists");
    assert_eq!(summary.declared_holder.as_deref(), Some("Example Corp."));
    assert_eq!(summary.other_holders.len(), 2);
}

#[test]
fn compute_summary_keeps_holder_tallies_when_no_declared_holder_exists() {
    let mut source_one = file("project/src/main.c");
    source_one.holders = vec![Holder {
        holder: "Members of the Gmerlin project".to_string(),
        start_line: 1,
        end_line: 1,
    }];
    let mut source_two = file("project/src/helper.c");
    source_two.holders = vec![Holder {
        holder: "Members of the Gmerlin project".to_string(),
        start_line: 1,
        end_line: 1,
    }];
    let summary = compute_summary(&[source_one, source_two], &[]).expect("summary exists");
    assert_eq!(summary.declared_holder.as_deref(), Some(""));
    assert_eq!(summary.other_holders[0].count, 2);
}

#[test]
fn compute_summary_removes_punctuation_only_holder_variants_from_other_holders() {
    let mut readme = file("project/README.md");
    readme.is_key_file = true;
    readme.is_readme = true;
    readme.is_top_level = true;
    readme.holders = vec![Holder {
        holder: "Example Corp.".to_string(),
        start_line: 1,
        end_line: 1,
    }];

    let mut notice = file("project/NOTICE");
    notice.holders = vec![Holder {
        holder: "Example Corp".to_string(),
        start_line: 1,
        end_line: 1,
    }];

    let mut license = file("project/LICENSE");
    license.is_key_file = true;
    license.is_legal = true;
    license.is_top_level = true;

    let summary = compute_summary(&[readme, notice, license], &[]).expect("summary exists");
    assert_eq!(summary.declared_holder.as_deref(), Some("Example Corp."));
    assert_eq!(
        summary.other_holders,
        vec![TallyEntry {
            value: None,
            count: 1
        }]
    );
}

#[test]
fn compute_summary_uses_source_file_languages_when_packages_lack_them() {
    let mut ruby = file("project/lib/demo.rb");
    ruby.programming_language = Some("Ruby".to_string());
    ruby.is_source = Some(true);
    let mut ruby_two = file("project/lib/more.rb");
    ruby_two.programming_language = Some("Ruby".to_string());
    ruby_two.is_source = Some(true);
    let mut python = file("project/tools/helper.py");
    python.programming_language = Some("Python".to_string());
    python.is_source = Some(true);
    let summary = compute_summary(&[ruby, ruby_two, python], &[]).expect("summary exists");
    assert_eq!(summary.primary_language.as_deref(), Some("Ruby"));
    assert_eq!(summary.other_languages[0].value.as_deref(), Some("Python"));
}

#[test]
fn compute_summary_uses_tallied_primary_language_when_top_level_packages_disagree() {
    let mut cargo = package("pkg:cargo/codebase?uuid=test1", "codebase/cargo.toml");
    cargo.primary_language = Some("Rust".to_string());
    cargo.declared_license_expression = Some("mit".to_string());
    let mut pypi = package("pkg:pypi/codebase?uuid=test2", "codebase/PKG-INFO");
    pypi.primary_language = Some("Python".to_string());
    pypi.declared_license_expression = Some("apache-2.0".to_string());
    let mut py1 = file("codebase/a.py");
    py1.is_source = Some(true);
    py1.programming_language = Some("Python".to_string());
    let mut py2 = file("codebase/b.py");
    py2.is_source = Some(true);
    py2.programming_language = Some("Python".to_string());
    let mut rs = file("codebase/lib.rs");
    rs.is_source = Some(true);
    rs.programming_language = Some("Rust".to_string());
    let summary = compute_summary(&[py1, py2, rs], &[cargo, pypi]).expect("summary exists");
    assert_eq!(
        summary.declared_license_expression.as_deref(),
        Some("apache-2.0 AND mit")
    );
    assert_eq!(summary.primary_language.as_deref(), Some("Python"));
}

#[test]
fn compute_summary_combines_package_licenses_when_present_datafile_is_not_key_classified() {
    let mut pypi = package("pkg:pypi/codebase?uuid=test1", "codebase/setup.py");
    pypi.package_type = Some(PackageType::Pypi);
    pypi.declared_license_expression = Some("apache-2.0".to_string());
    pypi.primary_language = Some("Python".to_string());

    let mut cargo = package("pkg:cargo/codebase?uuid=test2", "codebase/cargo.toml");
    cargo.package_type = Some(PackageType::Cargo);
    cargo.declared_license_expression = Some("mit".to_string());
    cargo.primary_language = Some("Rust".to_string());

    let mut setup = file("codebase/setup.py");
    setup.is_manifest = true;
    setup.is_key_file = false;
    setup.is_top_level = false;
    setup.for_packages = vec![pypi.package_uid.clone()];

    let mut cargo_toml = file("codebase/cargo.toml");
    cargo_toml.is_manifest = true;
    cargo_toml.is_key_file = true;
    cargo_toml.is_top_level = true;
    cargo_toml.for_packages = vec![cargo.package_uid.clone()];
    cargo_toml.license_expression = Some("mit".to_string());
    cargo_toml.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("codebase/cargo.toml".to_string()),
            start_line: 1,
            end_line: 1,
            matcher: Some("1-spdx-id".to_string()),
            score: 100.0,
            matched_length: Some(1),
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

    let summary = compute_summary(&[setup, cargo_toml], &[pypi, cargo]).expect("summary exists");

    assert_eq!(
        summary.declared_license_expression.as_deref(),
        Some("apache-2.0 AND mit")
    );
}

#[test]
fn compute_summary_serializes_empty_declared_holder_when_none_found() {
    let mut package = package("pkg:pypi/pip?uuid=test", "pip-22.0.4/PKG-INFO");
    package.primary_language = Some("Python".to_string());
    package.declared_license_expression = Some("mit".to_string());
    let mut pkg_info = file("pip-22.0.4/PKG-INFO");
    pkg_info.is_manifest = true;
    pkg_info.is_key_file = true;
    pkg_info.is_top_level = true;
    pkg_info.for_packages = vec![package.package_uid.clone()];
    pkg_info.license_expression = Some("mit".to_string());
    pkg_info.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("pip-22.0.4/PKG-INFO".to_string()),
            start_line: 1,
            end_line: 1,
            matcher: Some("1-spdx-id".to_string()),
            score: 100.0,
            matched_length: Some(1),
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
    let summary = compute_summary(&[pkg_info], &[package]).expect("summary exists");
    assert_eq!(summary.declared_holder.as_deref(), Some(""));
    assert!(summary.other_holders.is_empty());
}

#[test]
fn compute_summary_joins_multiple_holders_from_single_top_level_license_file() {
    let mut license = file("codebase/jetty.LICENSE");
    license.is_key_file = true;
    license.is_legal = true;
    license.is_top_level = true;
    license.license_expression = Some("jetty".to_string());
    license.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "jetty".to_string(),
        license_expression_spdx: "LicenseRef-scancode-jetty".to_string(),
        matches: vec![Match {
            license_expression: "jetty".to_string(),
            license_expression_spdx: "LicenseRef-scancode-jetty".to_string(),
            from_file: Some("codebase/jetty.LICENSE".to_string()),
            start_line: 1,
            end_line: 132,
            matcher: Some("1-hash".to_string()),
            score: 100.0,
            matched_length: Some(996),
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
    license.copyrights = vec![Copyright {
        copyright: "Copyright Mort Bay and Sun Microsystems.".to_string(),
        start_line: 1,
        end_line: 1,
    }];
    license.holders = vec![
        Holder {
            holder: "Mort Bay Consulting Pty. Ltd. (Australia) and others".to_string(),
            start_line: 1,
            end_line: 1,
        },
        Holder {
            holder: "Sun Microsystems".to_string(),
            start_line: 2,
            end_line: 2,
        },
    ];
    let summary = compute_summary(&[license], &[]).expect("summary exists");
    assert_eq!(
        summary.declared_holder.as_deref(),
        Some("Mort Bay Consulting Pty. Ltd. (Australia) and others, Sun Microsystems")
    );
}

#[test]
fn compute_score_mode_ignores_package_declared_license_without_key_file_license_evidence() {
    let mut package = package("pkg:npm/demo?uuid=test", "project/package.json");
    package.declared_license_expression = Some("mit".to_string());
    let mut package_json = file("project/package.json");
    package_json.is_manifest = true;
    package_json.is_key_file = true;
    package_json.is_top_level = true;
    package_json.for_packages = vec![package.package_uid.clone()];
    package_json.copyrights = vec![Copyright {
        copyright: "Copyright Example Corp.".to_string(),
        start_line: 1,
        end_line: 1,
    }];
    let files = vec![package_json];
    let indexes = OutputIndexes::build(&files, None, false, OutputIndexMode::Full);
    let summary = compute_summary_with_options(&files, &[package], &indexes, false, true)
        .expect("score-only summary exists");
    let score = summary.license_clarity_score.expect("clarity exists");
    assert!(summary.declared_license_expression.is_none());
    assert_eq!(score.score, 0);
}

#[test]
fn compute_score_mode_without_license_text_returns_zero_with_copyright_only() {
    let mut package = package("pkg:npm/demo?uuid=test", "no_license_text/package.json");
    package.declared_license_expression = Some("mit".to_string());
    let mut package_json = file("no_license_text/package.json");
    package_json.is_manifest = true;
    package_json.is_key_file = true;
    package_json.is_top_level = true;
    package_json.for_packages = vec![package.package_uid.clone()];
    package_json.copyrights = vec![Copyright {
        copyright: "Copyright Example Corp.".to_string(),
        start_line: 1,
        end_line: 1,
    }];
    let files = vec![package_json];
    let indexes = OutputIndexes::build(&files, None, false, OutputIndexMode::Full);
    let summary = compute_summary_with_options(&files, &[package], &indexes, false, true)
        .expect("score-only summary exists");
    assert_eq!(summary.license_clarity_score.unwrap().score, 0);
}

#[test]
fn compute_score_mode_without_license_or_copyright_returns_zero() {
    let package = package(
        "pkg:npm/demo?uuid=test",
        "no_license_or_copyright/package.json",
    );
    let mut package_json = file("no_license_or_copyright/package.json");
    package_json.is_manifest = true;
    package_json.is_key_file = true;
    package_json.is_top_level = true;
    package_json.for_packages = vec![package.package_uid.clone()];
    let files = vec![package_json];
    let indexes = OutputIndexes::build(&files, None, false, OutputIndexMode::Full);
    let summary = compute_summary_with_options(&files, &[package], &indexes, false, true)
        .expect("score-only summary exists");
    assert_eq!(summary.license_clarity_score.unwrap().score, 0);
}

#[test]
fn compute_score_mode_uses_single_joined_expression_without_ambiguity() {
    let mut cargo = file("no_license_ambiguity/Cargo.toml");
    cargo.is_manifest = true;
    cargo.is_key_file = true;
    cargo.is_top_level = true;
    cargo.license_expression = Some("mit OR apache-2.0".to_string());
    cargo.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit OR apache-2.0".to_string(),
        license_expression_spdx: "MIT OR Apache-2.0".to_string(),
        matches: vec![Match {
            license_expression: "mit OR apache-2.0".to_string(),
            license_expression_spdx: "MIT OR Apache-2.0".to_string(),
            from_file: Some("no_license_ambiguity/Cargo.toml".to_string()),
            start_line: 1,
            end_line: 1,
            matcher: Some("1-hash".to_string()),
            score: 100.0,
            matched_length: Some(5),
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
    cargo.copyrights = vec![Copyright {
        copyright: "Copyright The Rand Project Developers.".to_string(),
        start_line: 1,
        end_line: 1,
    }];
    let mut apache = file("no_license_ambiguity/LICENSE-APACHE");
    apache.is_legal = true;
    apache.is_key_file = true;
    apache.is_top_level = true;
    apache.license_expression = Some("apache-2.0".to_string());
    apache.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "apache-2.0".to_string(),
        license_expression_spdx: "Apache-2.0".to_string(),
        matches: vec![Match {
            license_expression: "apache-2.0".to_string(),
            license_expression_spdx: "Apache-2.0".to_string(),
            from_file: Some("no_license_ambiguity/LICENSE-APACHE".to_string()),
            start_line: 1,
            end_line: 176,
            matcher: Some("1-hash".to_string()),
            score: 100.0,
            matched_length: Some(1410),
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
    let mut mit = file("no_license_ambiguity/LICENSE-MIT");
    mit.is_legal = true;
    mit.is_key_file = true;
    mit.is_top_level = true;
    mit.license_expression = Some("mit".to_string());
    mit.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("no_license_ambiguity/LICENSE-MIT".to_string()),
            start_line: 1,
            end_line: 18,
            matcher: Some("1-hash".to_string()),
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
    let files = vec![cargo, apache, mit];
    let indexes = OutputIndexes::build(&files, None, false, OutputIndexMode::Full);
    let summary = compute_summary_with_options(&files, &[], &indexes, false, true)
        .expect("score-only summary exists");
    let score = summary.license_clarity_score.expect("clarity exists");
    assert_eq!(
        summary.declared_license_expression.as_deref(),
        Some("mit OR apache-2.0")
    );
    assert_eq!(score.score, 100);
    assert!(!score.ambiguous_compound_licensing);
}

#[test]
fn get_primary_license_keeps_with_expression_ambiguous_against_base_license() {
    let expressions = vec![
        "gpl-2.0 WITH classpath-exception-2.0".to_string(),
        "gpl-2.0".to_string(),
    ];

    assert_eq!(get_primary_license(&expressions), None);
}

#[test]
fn compute_score_mode_does_not_treat_with_expression_as_covering_base_license() {
    let mut manifest = file("with_exception_ambiguity/Cargo.toml");
    manifest.is_manifest = true;
    manifest.is_key_file = true;
    manifest.is_top_level = true;
    manifest.license_expression = Some("gpl-2.0 WITH classpath-exception-2.0".to_string());
    manifest.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "gpl-2.0 WITH classpath-exception-2.0".to_string(),
        license_expression_spdx: "GPL-2.0-only WITH Classpath-exception-2.0".to_string(),
        matches: vec![Match {
            license_expression: "gpl-2.0 WITH classpath-exception-2.0".to_string(),
            license_expression_spdx: "GPL-2.0-only WITH Classpath-exception-2.0".to_string(),
            from_file: Some("with_exception_ambiguity/Cargo.toml".to_string()),
            start_line: 1,
            end_line: 1,
            matcher: Some("1-hash".to_string()),
            score: 100.0,
            matched_length: Some(5),
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
    manifest.copyrights = vec![Copyright {
        copyright: "Copyright Example Corp.".to_string(),
        start_line: 1,
        end_line: 1,
    }];

    let mut gpl = file("with_exception_ambiguity/LICENSE-GPL");
    gpl.is_legal = true;
    gpl.is_key_file = true;
    gpl.is_top_level = true;
    gpl.license_expression = Some("gpl-2.0".to_string());
    gpl.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "gpl-2.0".to_string(),
        license_expression_spdx: "GPL-2.0-only".to_string(),
        matches: vec![Match {
            license_expression: "gpl-2.0".to_string(),
            license_expression_spdx: "GPL-2.0-only".to_string(),
            from_file: Some("with_exception_ambiguity/LICENSE-GPL".to_string()),
            start_line: 1,
            end_line: 176,
            matcher: Some("1-hash".to_string()),
            score: 100.0,
            matched_length: Some(1410),
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

    let files = vec![manifest, gpl];
    let indexes = OutputIndexes::build(&files, None, false, OutputIndexMode::Full);
    let summary = compute_summary_with_options(&files, &[], &indexes, false, true)
        .expect("score-only summary exists");

    assert_eq!(
        summary.declared_license_expression.as_deref(),
        Some("gpl-2.0 AND gpl-2.0 WITH classpath-exception-2.0")
    );
    let score = summary.license_clarity_score.expect("clarity exists");
    assert_eq!(score.score, 90);
    assert!(score.ambiguous_compound_licensing);
}

#[test]
fn compute_score_mode_scores_nested_manifest_key_file_without_copyright() {
    let mut pom = file("jar/META-INF/maven/org.jboss.logging/jboss-logging/pom.xml");
    pom.is_manifest = true;
    pom.is_key_file = true;
    pom.is_top_level = true;
    pom.license_expression = Some("apache-2.0".to_string());
    pom.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "apache-2.0".to_string(),
        license_expression_spdx: "Apache-2.0".to_string(),
        matches: vec![Match {
            license_expression: "apache-2.0".to_string(),
            license_expression_spdx: "Apache-2.0".to_string(),
            from_file: Some(
                "jar/META-INF/maven/org.jboss.logging/jboss-logging/pom.xml".to_string(),
            ),
            start_line: 1,
            end_line: 2,
            matcher: Some("1-hash".to_string()),
            score: 100.0,
            matched_length: Some(16),
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
    let mut license = file("jar/META-INF/LICENSE.txt");
    license.is_legal = true;
    license.is_key_file = true;
    license.is_top_level = true;
    license.license_expression = Some("apache-2.0".to_string());
    license.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "apache-2.0".to_string(),
        license_expression_spdx: "Apache-2.0".to_string(),
        matches: vec![Match {
            license_expression: "apache-2.0".to_string(),
            license_expression_spdx: "Apache-2.0".to_string(),
            from_file: Some("jar/META-INF/LICENSE.txt".to_string()),
            start_line: 1,
            end_line: 176,
            matcher: Some("1-hash".to_string()),
            score: 100.0,
            matched_length: Some(1410),
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
    let files = vec![pom, license];
    let indexes = OutputIndexes::build(&files, None, false, OutputIndexMode::Full);
    let summary = compute_summary_with_options(&files, &[], &indexes, false, true)
        .expect("score-only summary exists");
    assert_eq!(
        summary.declared_license_expression.as_deref(),
        Some("apache-2.0")
    );
    assert_eq!(summary.license_clarity_score.unwrap().score, 90);
}

#[test]
fn compute_summary_without_license_evidence_has_no_clarity_score() {
    let uid = "pkg:gem/demo@1.0.0?uuid=test";
    let mut root_package = package(uid, "demo/demo.gemspec");
    root_package.holder = Some("Example Corp.".to_string());
    root_package.primary_language = Some("Ruby".to_string());
    let summary = compute_summary(&[], &[root_package]).expect("summary exists");
    assert_eq!(summary.declared_holder.as_deref(), Some("Example Corp."));
    assert_eq!(summary.primary_language.as_deref(), Some("Ruby"));
    assert_eq!(summary.license_clarity_score.unwrap().score, 0);
}
