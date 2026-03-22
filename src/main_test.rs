use super::*;
use crate::models::{
    Author, Copyright, DatasourceId, FileInfo, FileReference, FileType, Holder, Match, OutputEmail,
    OutputURL, Package, PackageType,
};
use clap::Parser;
use regex::Regex;
use serde_json::Value;
use serde_json::json;
use std::fs;
use std::sync::Arc;

use crate::assembly;
use crate::progress::{ProgressMode, ScanProgress};
use crate::scanner::TextDetectionOptions;

fn file(path: &str) -> FileInfo {
    FileInfo::new(
        Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string(),
        Path::new(path)
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string(),
        Path::new(path)
            .extension()
            .and_then(|n| n.to_str())
            .map(|ext| format!(".{ext}"))
            .unwrap_or_default(),
        path.to_string(),
        FileType::File,
        None,
        1,
        None,
        None,
        None,
        None,
        None,
        Vec::new(),
        None,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
}

fn dir(path: &str) -> FileInfo {
    FileInfo::new(
        Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string(),
        Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string(),
        String::new(),
        path.to_string(),
        FileType::Directory,
        None,
        0,
        None,
        None,
        None,
        None,
        None,
        Vec::new(),
        None,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
}

fn about_scan_and_assemble(path: &Path) -> assembly::AssemblyResult {
    let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
    let result = crate::scanner::process_with_options(
        path,
        0,
        progress,
        &[],
        None,
        false,
        &TextDetectionOptions::default(),
    )
    .expect("about scan should succeed");

    let mut files = result.files;
    assembly::assemble(&mut files)
}

fn swift_scan_and_assemble(path: &Path) -> Value {
    let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
    let result = crate::scanner::process_with_options(
        path,
        0,
        progress,
        &[],
        None,
        false,
        &TextDetectionOptions::default(),
    )
    .expect("swift scan should succeed");

    let mut files = result.files;
    normalize_paths(
        &mut files,
        path.to_str().expect("swift fixture path should be UTF-8"),
        true,
        false,
    );
    let assembly_result = assembly::assemble(&mut files);

    files.sort_by(|left, right| left.path.cmp(&right.path));
    let files_json: Vec<Value> = files
        .into_iter()
        .filter(|file| !file.path.is_empty())
        .map(|file| {
            json!({
                "path": file.path,
                "type": file.file_type,
                "package_data": file.package_data,
                "for_packages": file.for_packages,
                "scan_errors": file.scan_errors,
            })
        })
        .collect();

    json!({
        "packages": assembly_result.packages,
        "dependencies": assembly_result.dependencies,
        "files": files_json,
    })
}

fn docker_scan_and_assemble(path: &Path) -> (Vec<FileInfo>, assembly::AssemblyResult) {
    let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
    let result = crate::scanner::process_with_options(
        path,
        0,
        progress,
        &[],
        None,
        false,
        &TextDetectionOptions::default(),
    )
    .expect("docker scan should succeed");

    let mut files = result.files;
    let assembly_result = assembly::assemble(&mut files);
    (files, assembly_result)
}

fn python_scan_and_assemble(path: &Path) -> (Vec<FileInfo>, assembly::AssemblyResult) {
    let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
    let result = crate::scanner::process_with_options(
        path,
        0,
        progress,
        &[],
        None,
        false,
        &TextDetectionOptions::default(),
    )
    .expect("python scan should succeed");

    let mut files = result.files;
    let assembly_result = assembly::assemble(&mut files);
    (files, assembly_result)
}

fn debian_scan_and_assemble_with_keyfiles(
    path: &Path,
) -> (Vec<FileInfo>, assembly::AssemblyResult) {
    let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
    let result = crate::scanner::process_with_options(
        path,
        0,
        progress,
        &[],
        None,
        false,
        &TextDetectionOptions::default(),
    )
    .expect("debian scan should succeed");

    let mut files = result.files;
    let assembly_result = assembly::assemble(&mut files);
    classify_key_files(&mut files, &assembly_result.packages);
    (files, assembly_result)
}

fn normalize_test_uuids(json_str: &str) -> String {
    let re = Regex::new(r"uuid=[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
        .expect("uuid regex should compile");
    re.replace_all(json_str, "uuid=fixed-uid-done-for-testing-5642512d1758")
        .to_string()
}

fn compare_scan_json_values(actual: &Value, expected: &Value, path: &str) -> Result<(), String> {
    if path.ends_with("package_data") {
        return Ok(());
    }

    match (actual, expected) {
        (Value::Null, Value::Null) => Ok(()),
        (Value::Bool(a), Value::Bool(e)) if a == e => Ok(()),
        (Value::Number(a), Value::Number(e)) if a == e => Ok(()),
        (Value::String(a), Value::String(e)) if a == e => Ok(()),
        (Value::Array(a), Value::Array(e)) => {
            if a.len() != e.len() {
                return Err(format!(
                    "Array length mismatch at {}: actual={}, expected={}",
                    path,
                    a.len(),
                    e.len()
                ));
            }

            for (index, (actual_item, expected_item)) in a.iter().zip(e.iter()).enumerate() {
                let item_path = if path.is_empty() {
                    format!("[{}]", index)
                } else {
                    format!("{}[{}]", path, index)
                };
                compare_scan_json_values(actual_item, expected_item, &item_path)?;
            }

            Ok(())
        }
        (Value::Object(a), Value::Object(e)) => {
            if path.ends_with("resolved_package") && e.is_empty() {
                return Ok(());
            }

            for key in e.keys() {
                if !a.contains_key(key) {
                    match e.get(key) {
                        Some(Value::Null) => continue,
                        Some(Value::Bool(false)) => continue,
                        Some(Value::Array(values)) if values.is_empty() => continue,
                        Some(Value::Object(values)) if values.is_empty() => continue,
                        _ => {
                            let field_path = if path.is_empty() {
                                key.to_string()
                            } else {
                                format!("{}.{}", path, key)
                            };
                            return Err(format!("Missing key in actual: {}", field_path));
                        }
                    }
                }
            }

            for key in a.keys() {
                if !e.contains_key(key) {
                    if path.ends_with("extra_data") {
                        continue;
                    }

                    match a.get(key) {
                        Some(Value::Null) => continue,
                        Some(Value::Bool(false)) => continue,
                        Some(Value::Array(values)) if values.is_empty() => continue,
                        Some(Value::Object(values)) if values.is_empty() => continue,
                        _ => {
                            let field_path = if path.is_empty() {
                                key.to_string()
                            } else {
                                format!("{}.{}", path, key)
                            };
                            return Err(format!("Extra key in actual: {}", field_path));
                        }
                    }
                }
            }

            for key in a.keys() {
                if let (Some(actual_val), Some(expected_val)) = (a.get(key), e.get(key)) {
                    let field_path = if path.is_empty() {
                        key.to_string()
                    } else {
                        format!("{}.{}", path, key)
                    };
                    compare_scan_json_values(actual_val, expected_val, &field_path)?;
                }
            }

            Ok(())
        }
        _ => Err(format!(
            "Type or value mismatch at {}: actual={}, expected={}",
            path,
            serde_json::to_string(actual).unwrap_or_default(),
            serde_json::to_string(expected).unwrap_or_default()
        )),
    }
}

fn normalize_scan_json(value: &mut Value, parent_key: Option<&str>) {
    match value {
        Value::Array(values) => {
            for item in values.iter_mut() {
                normalize_scan_json(item, parent_key);
            }

            if parent_key.is_some_and(|key| {
                matches!(
                    key,
                    "packages"
                        | "dependencies"
                        | "files"
                        | "package_data"
                        | "datafile_paths"
                        | "datasource_ids"
                        | "for_packages"
                )
            }) {
                values.sort_by_cached_key(|item| serde_json::to_string(item).unwrap_or_default());
            }
        }
        Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                normalize_scan_json(item, Some(key));
            }
        }
        _ => {}
    }
}

fn assert_swift_scan_matches_expected(fixture_dir: &str, expected_file: &str) {
    let actual = swift_scan_and_assemble(Path::new(fixture_dir));
    let actual_str =
        serde_json::to_string_pretty(&actual).expect("actual scan JSON should serialize");
    let expected_str =
        fs::read_to_string(expected_file).expect("expected scan JSON should be readable");

    let actual_normalized = normalize_test_uuids(&actual_str);
    let expected_normalized = normalize_test_uuids(&expected_str);

    let mut actual_value: Value =
        serde_json::from_str(&actual_normalized).expect("normalized actual JSON should parse");
    let mut expected_value: Value =
        serde_json::from_str(&expected_normalized).expect("normalized expected JSON should parse");

    normalize_scan_json(&mut actual_value, None);
    normalize_scan_json(&mut expected_value, None);

    if let Err(error) = compare_scan_json_values(&actual_value, &expected_value, "") {
        panic!(
            "Swift scan golden mismatch for fixture {} vs {}: {}",
            fixture_dir, expected_file, error
        );
    }
}

fn package(uid: &str, path: &str) -> Package {
    Package {
        package_type: Some(PackageType::Gem),
        namespace: None,
        name: Some("inspec-bin".to_string()),
        version: Some("6.8.2".to_string()),
        qualifiers: None,
        subpath: None,
        primary_language: Some("Ruby".to_string()),
        description: None,
        release_date: None,
        parties: vec![],
        keywords: vec![],
        homepage_url: None,
        download_url: None,
        size: None,
        sha1: None,
        md5: None,
        sha256: None,
        sha512: None,
        bug_tracking_url: None,
        code_view_url: None,
        vcs_url: None,
        copyright: None,
        holder: None,
        declared_license_expression: None,
        declared_license_expression_spdx: None,
        license_detections: vec![],
        other_license_expression: None,
        other_license_expression_spdx: None,
        other_license_detections: vec![],
        extracted_license_statement: None,
        notice_text: None,
        source_packages: vec![],
        is_private: false,
        is_virtual: false,
        extra_data: None,
        repository_homepage_url: None,
        repository_download_url: None,
        api_data_url: None,
        datasource_ids: vec![DatasourceId::GemArchiveExtracted],
        purl: Some("pkg:gem/inspec-bin@6.8.2".to_string()),
        package_uid: uid.to_string(),
        datafile_paths: vec![path.to_string()],
    }
}

#[test]
fn include_filter_keeps_matching_files_and_parent_dirs() {
    let mut files = vec![
        dir("project"),
        dir("project/src"),
        file("project/src/main.rs"),
        file("project/README.md"),
    ];
    let include_patterns = vec![Pattern::new("*.rs").expect("valid pattern")];

    apply_include_filter(&mut files, &include_patterns);

    let paths: HashSet<_> = files.into_iter().map(|f| f.path).collect();
    assert!(paths.contains("project/src/main.rs"));
    assert!(paths.contains("project/src"));
    assert!(paths.contains("project"));
    assert!(!paths.contains("project/README.md"));
}

#[test]
fn only_findings_keeps_file_with_findings_and_parent_dirs() {
    let mut files = vec![dir("project"), file("project/a.txt"), file("project/b.txt")];
    files[2].copyrights = vec![Copyright {
        copyright: "Copyright Example".to_string(),
        start_line: 1,
        end_line: 1,
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
    files[0].holders = vec![
        Holder {
            holder: "ACME".to_string(),
            start_line: 1,
            end_line: 1,
        },
        Holder {
            holder: "ACME".to_string(),
            start_line: 1,
            end_line: 1,
        },
    ];
    files[0].authors = vec![
        Author {
            author: "Jane".to_string(),
            start_line: 2,
            end_line: 2,
        },
        Author {
            author: "Jane".to_string(),
            start_line: 2,
            end_line: 2,
        },
    ];
    files[0].emails = vec![
        OutputEmail {
            email: "a@example.com".to_string(),
            start_line: 3,
            end_line: 3,
        },
        OutputEmail {
            email: "a@example.com".to_string(),
            start_line: 3,
            end_line: 3,
        },
    ];
    files[0].urls = vec![
        OutputURL {
            url: "https://example.com".to_string(),
            start_line: 4,
            end_line: 4,
        },
        OutputURL {
            url: "https://example.com".to_string(),
            start_line: 4,
            end_line: 4,
        },
    ];

    filter_redundant_clues(&mut files);

    assert_eq!(files[0].holders.len(), 1);
    assert_eq!(files[0].authors.len(), 1);
    assert_eq!(files[0].emails.len(), 1);
    assert_eq!(files[0].urls.len(), 1);
}

#[test]
fn normalize_paths_strip_root_removes_scan_root_prefix() {
    let mut files = vec![file("project/src/main.rs")];
    normalize_paths(&mut files, "project", true, false);
    assert_eq!(files[0].path, "src/main.rs");
}

#[test]
fn load_scan_from_json_reads_files_and_metadata_sections() {
    let temp_path = std::env::temp_dir().join("provenant-from-json-test.json");
    let content = json!({
        "files": [
            {
                "name": "main.rs",
                "base_name": "main",
                "extension": ".rs",
                "path": "src/main.rs",
                "type": "file",
                "size": 10,
                "programming_language": "Rust"
            }
        ],
        "packages": [],
        "dependencies": [],
        "license_references": [
            {"name":"MIT","short_name":"MIT","spdx_license_key":"MIT","text":"..."}
        ],
        "license_rule_references": []
    });
    fs::write(&temp_path, content.to_string()).expect("write json fixture");

    let parsed = load_scan_from_json(temp_path.to_str().expect("utf-8 path"))
        .expect("from-json loading should succeed");

    assert_eq!(parsed.files.len(), 1);
    assert_eq!(parsed.files[0].path, "src/main.rs");
    assert_eq!(parsed.license_references.len(), 1);

    let _ = fs::remove_file(temp_path);
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
    files[2].programming_language = Some("Rust".to_string());
    files[3].programming_language = Some("Rust".to_string());

    apply_mark_source(&mut files);

    let src = files
        .iter()
        .find(|f| f.path == "project/src")
        .expect("src directory exists");
    assert_eq!(src.is_source, None);
    assert_eq!(src.source_count, None);

    files[4].programming_language = Some("Rust".to_string());
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
    files[3].is_source = Some(false);

    apply_mark_source(&mut files);

    let module_dir = files
        .iter()
        .find(|f| f.path == "module")
        .expect("module dir exists");
    assert_eq!(module_dir.is_source, Some(true));
    assert_eq!(module_dir.source_count, Some(2));

    let test_file = files
        .iter()
        .find(|f| f.path == "module/helper_test.go")
        .expect("test file exists");
    assert_eq!(test_file.is_source, Some(false));
}

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
        start_line: 1,
        end_line: 1,
    }];
    license_file.holders = vec![Holder {
        holder: "Chef Software Inc.".to_string(),
        start_line: 1,
        end_line: 1,
    }];
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
            matched_length: None,
            match_coverage: None,
            rule_relevance: None,
            rule_identifier: None,
            rule_url: None,
            matched_text: None,
        }],
        identifier: None,
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
            matched_length: None,
            match_coverage: None,
            rule_relevance: None,
            rule_identifier: None,
            rule_url: None,
            matched_text: None,
        }],
        identifier: None,
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

    classify_key_files(&mut files, &packages);
    promote_package_metadata_from_key_files(&files, &mut packages);
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
            }],
            identifier: None,
        }],
        ..Default::default()
    }];

    let package =
        Package::from_package_data(&gemspec.package_data[0], "demo/demo.gemspec".to_string());
    gemspec.for_packages.push(package.package_uid.clone());
    let mut files = vec![gemspec];
    let mut packages = vec![package];

    classify_key_files(&mut files, &packages);
    promote_package_metadata_from_key_files(&files, &mut packages);
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
    assert_eq!(summary.license_clarity_score.unwrap().score, 80);
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
fn compute_summary_uses_package_holder_and_primary_language() {
    let uid = "pkg:gem/demo@1.0.0?uuid=test";
    let mut root_package = package(uid, "demo/demo.gemspec");
    root_package.holder = Some("Example Corp.".to_string());
    root_package.primary_language = Some("Ruby".to_string());

    let mut other = package("pkg:pypi/demo?uuid=test2", "demo/setup.py");
    other.package_type = Some(PackageType::Pypi);
    other.purl = Some("pkg:pypi/demo".to_string());
    other.primary_language = Some("Python".to_string());
    other.holder = None;

    let mut extra_ruby = package("pkg:gem/demo-extra@1.0.0?uuid=test3", "demo/extra.gemspec");
    extra_ruby.name = Some("demo-extra".to_string());
    extra_ruby.purl = Some("pkg:gem/demo-extra@1.0.0".to_string());

    let files = vec![];
    let summary =
        compute_summary(&files, &[root_package, other, extra_ruby]).expect("summary exists");

    assert_eq!(summary.declared_holder.as_deref(), Some("Example Corp."));
    assert_eq!(summary.primary_language.as_deref(), Some("Ruby"));
    assert_eq!(summary.other_languages.len(), 1);
    assert_eq!(summary.other_languages[0].value.as_deref(), Some("Python"));
    assert_eq!(summary.other_languages[0].count, 1);
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
    assert_eq!(summary.other_languages.len(), 1);
    assert_eq!(summary.other_languages[0].value.as_deref(), Some("Python"));
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
    assert!(summary.license_clarity_score.is_none());
}

#[test]
fn compute_tallies_counts_file_findings_and_missing_values() {
    let mut mit_file = file("project/src/lib.rs");
    mit_file.programming_language = Some("Rust".to_string());
    mit_file.license_expression = Some("mit".to_string());
    mit_file.license_detections = vec![crate::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("project/src/lib.rs".to_string()),
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
        }],
        identifier: None,
    }];
    mit_file.copyrights = vec![Copyright {
        copyright: "Copyright (c) Example Corp.".to_string(),
        start_line: 1,
        end_line: 1,
    }];
    mit_file.holders = vec![Holder {
        holder: "Example Corp.".to_string(),
        start_line: 1,
        end_line: 1,
    }];
    mit_file.authors = vec![Author {
        author: "Alice".to_string(),
        start_line: 1,
        end_line: 1,
    }];

    let mut dual_license_file = file("project/src/main.c");
    dual_license_file.programming_language = Some("C".to_string());
    dual_license_file.license_expression = Some("apache-2.0 AND mit".to_string());
    dual_license_file.license_detections = vec![
        crate::models::LicenseDetection {
            license_expression: "apache-2.0".to_string(),
            license_expression_spdx: "Apache-2.0".to_string(),
            matches: vec![Match {
                license_expression: "apache-2.0".to_string(),
                license_expression_spdx: "Apache-2.0".to_string(),
                from_file: Some("project/src/main.c".to_string()),
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
            }],
            identifier: None,
        },
        crate::models::LicenseDetection {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            matches: vec![Match {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                from_file: Some("project/src/main.c".to_string()),
                start_line: 2,
                end_line: 2,
                matcher: None,
                score: 100.0,
                matched_length: None,
                match_coverage: None,
                rule_relevance: None,
                rule_identifier: None,
                rule_url: None,
                matched_text: None,
            }],
            identifier: None,
        },
    ];
    dual_license_file.copyrights = vec![Copyright {
        copyright: "Copyright (c) Example Corp.".to_string(),
        start_line: 1,
        end_line: 1,
    }];
    dual_license_file.holders = vec![Holder {
        holder: "Example Corp.".to_string(),
        start_line: 1,
        end_line: 1,
    }];
    dual_license_file.authors = vec![Author {
        author: "Bob".to_string(),
        start_line: 1,
        end_line: 1,
    }];

    let empty_file = file("project/README.md");

    let tallies =
        compute_tallies(&[mit_file, dual_license_file, empty_file]).expect("tallies exist");

    assert_eq!(
        tallies.detected_license_expression,
        vec![
            TallyEntry {
                value: None,
                count: 1,
            },
            TallyEntry {
                value: Some("apache-2.0 AND mit".to_string()),
                count: 1,
            },
            TallyEntry {
                value: Some("mit".to_string()),
                count: 1,
            },
        ]
    );
    assert_eq!(
        tallies.copyrights,
        vec![
            TallyEntry {
                value: Some("Copyright (c) Example Corp.".to_string()),
                count: 2,
            },
            TallyEntry {
                value: None,
                count: 1,
            },
        ]
    );
    assert_eq!(
        tallies.holders,
        vec![
            TallyEntry {
                value: Some("Example Corp.".to_string()),
                count: 2,
            },
            TallyEntry {
                value: None,
                count: 1,
            },
        ]
    );
    assert_eq!(
        tallies.authors,
        vec![
            TallyEntry {
                value: None,
                count: 1,
            },
            TallyEntry {
                value: Some("Alice".to_string()),
                count: 1,
            },
            TallyEntry {
                value: Some("Bob".to_string()),
                count: 1,
            },
        ]
    );
    assert_eq!(
        tallies.programming_language,
        vec![
            TallyEntry {
                value: Some("C".to_string()),
                count: 1,
            },
            TallyEntry {
                value: Some("Rust".to_string()),
                count: 1,
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
        start_line: 1,
        end_line: 1,
    }];
    key_license.holders = vec![Holder {
        holder: "Example Corp.".to_string(),
        start_line: 1,
        end_line: 1,
    }];

    let mut key_readme = file("project/README.md");
    key_readme.is_key_file = true;
    key_readme.programming_language = Some("Markdown".to_string());
    key_readme.authors = vec![Author {
        author: "Alice".to_string(),
        start_line: 1,
        end_line: 1,
    }];

    let mut non_key_source = file("project/src/lib.rs");
    non_key_source.programming_language = Some("Rust".to_string());
    non_key_source.license_expression = Some("mit".to_string());
    non_key_source.is_key_file = false;

    let tallies = compute_key_file_tallies(&[key_license, key_readme, non_key_source])
        .expect("key-file tallies exist");

    assert_eq!(
        tallies.detected_license_expression,
        vec![TallyEntry {
            value: Some("apache-2.0".to_string()),
            count: 1,
        }]
    );
    assert_eq!(
        tallies.copyrights,
        vec![TallyEntry {
            value: Some("Copyright (c) Example Corp.".to_string()),
            count: 1,
        }]
    );
    assert_eq!(
        tallies.holders,
        vec![TallyEntry {
            value: Some("Example Corp.".to_string()),
            count: 1,
        }]
    );
    assert_eq!(
        tallies.authors,
        vec![TallyEntry {
            value: Some("Alice".to_string()),
            count: 1,
        }]
    );
    assert_eq!(
        tallies.programming_language,
        vec![TallyEntry {
            value: Some("Markdown".to_string()),
            count: 1,
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
        start_line: 1,
        end_line: 1,
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
    let main_rs = files
        .iter()
        .find(|file| file.path == "project/src/main.rs")
        .unwrap();
    let readme = files
        .iter()
        .find(|file| file.path == "project/README.md")
        .unwrap();

    assert_eq!(
        main_rs
            .tallies
            .as_ref()
            .unwrap()
            .detected_license_expression,
        vec![TallyEntry {
            value: Some("mit".to_string()),
            count: 1,
        }]
    );
    assert_eq!(
        main_rs.tallies.as_ref().unwrap().authors,
        vec![TallyEntry {
            value: Some("Alice".to_string()),
            count: 1,
        }]
    );
    assert_eq!(
        readme.tallies.as_ref().unwrap().detected_license_expression,
        vec![TallyEntry {
            value: None,
            count: 1,
        }]
    );
    assert_eq!(
        src.tallies.as_ref().unwrap().programming_language,
        vec![TallyEntry {
            value: Some("Rust".to_string()),
            count: 1,
        }]
    );
    assert_eq!(
        root.tallies.as_ref().unwrap().detected_license_expression,
        vec![
            TallyEntry {
                value: None,
                count: 1,
            },
            TallyEntry {
                value: Some("mit".to_string()),
                count: 1,
            },
        ]
    );
    assert_eq!(
        root.tallies.as_ref().unwrap().programming_language,
        vec![
            TallyEntry {
                value: Some("Markdown".to_string()),
                count: 1,
            },
            TallyEntry {
                value: Some("Rust".to_string()),
                count: 1,
            },
        ]
    );
    assert!(empty.tallies.as_ref().unwrap().is_empty());
}

#[test]
fn about_scan_promotes_packages_and_assigns_referenced_files() {
    let result = about_scan_and_assemble(Path::new("testdata/about"));

    assert_eq!(result.packages.len(), 2);
    let apipkg = result
        .packages
        .iter()
        .find(|pkg| pkg.name.as_deref() == Some("apipkg"))
        .expect("apipkg package exists");
    let appdirs = result
        .packages
        .iter()
        .find(|pkg| pkg.name.as_deref() == Some("appdirs"))
        .expect("appdirs package exists");

    assert_eq!(apipkg.package_type, Some(PackageType::Pypi));
    assert_eq!(appdirs.package_type, Some(PackageType::Pypi));
    assert_eq!(apipkg.purl.as_deref(), Some("pkg:pypi/apipkg@1.4"));
    assert_eq!(appdirs.purl.as_deref(), Some("pkg:pypi/appdirs@1.4.3"));
}

#[test]
fn about_scan_tracks_missing_file_references() {
    let result = about_scan_and_assemble(Path::new("testdata/about"));

    let apipkg = result
        .packages
        .iter()
        .find(|pkg| pkg.name.as_deref() == Some("apipkg"))
        .expect("apipkg package exists");
    let appdirs = result
        .packages
        .iter()
        .find(|pkg| pkg.name.as_deref() == Some("appdirs"))
        .expect("appdirs package exists");

    let apipkg_missing = apipkg
        .extra_data
        .as_ref()
        .and_then(|extra| extra.get("missing_file_references"))
        .and_then(|value| value.as_array())
        .expect("apipkg missing refs should exist");
    let apipkg_missing_paths: Vec<_> = apipkg_missing
        .iter()
        .filter_map(|value| value.get("path").and_then(|path| path.as_str()))
        .collect();
    assert_eq!(apipkg_missing_paths, vec!["apipkg.LICENSE"]);

    let missing = appdirs
        .extra_data
        .as_ref()
        .and_then(|extra| extra.get("missing_file_references"))
        .and_then(|value| value.as_array())
        .expect("appdirs missing refs should exist");

    let missing_paths: Vec<_> = missing
        .iter()
        .filter_map(|value| value.get("path").and_then(|path| path.as_str()))
        .collect();
    assert!(missing_paths.contains(&"appdirs-1.4.3-py2.py3-none-any.whl"));
    assert!(missing_paths.contains(&"appdirs.LICENSE"));
}

#[test]
fn swift_scan_uses_show_dependencies_only_fixture() {
    assert_swift_scan_matches_expected(
        "testdata/swift-golden/packages/vercelui_show_dependencies",
        "testdata/swift-golden/swift-vercelui-show-dependencies-expected.json",
    );
}

#[test]
fn swift_scan_uses_resolved_only_fixture() {
    assert_swift_scan_matches_expected(
        "testdata/swift-golden/packages/fastlane_resolved_v1",
        "testdata/swift-golden/swift-fastlane-resolved-v1-package-expected.json",
    );
}

#[test]
fn swift_scan_prefers_show_dependencies_over_manifest_dependencies() {
    assert_swift_scan_matches_expected(
        "testdata/swift-golden/packages/vercelui",
        "testdata/swift-golden/swift-vercelui-expected.json",
    );
}

#[test]
fn swift_scan_falls_back_to_resolved_when_show_dependencies_missing() {
    assert_swift_scan_matches_expected(
        "testdata/swift-golden/packages/mapboxmaps_manifest_and_resolved",
        "testdata/swift-golden/swift-mapboxmaps-manifest-and-resolved-package-expected.json",
    );
}

#[test]
fn containerfile_scan_keeps_package_data_unassembled() {
    let (files, result) = docker_scan_and_assemble(Path::new("testdata/docker-golden/pulp"));

    assert!(result.packages.is_empty());
    assert!(result.dependencies.is_empty());

    let containerfile = files
        .iter()
        .find(|file| file.path.ends_with("Containerfile"))
        .expect("Containerfile should be scanned");

    assert!(containerfile.for_packages.is_empty());
    assert_eq!(containerfile.package_data.len(), 1);

    let package = &containerfile.package_data[0];
    assert_eq!(package.package_type, Some(PackageType::Docker));
    assert_eq!(package.datasource_id, Some(DatasourceId::Dockerfile));
    assert_eq!(package.name.as_deref(), Some("Pulp OCI image"));

    let expected_json = files
        .iter()
        .find(|file| file.path.ends_with("Containerfile.expected.json"))
        .expect("expected fixture JSON should also be scanned");
    assert!(expected_json.package_data.is_empty());
}

#[test]
fn python_metadata_scan_assigns_referenced_site_packages_files() {
    let temp_dir = tempfile::TempDir::new().expect("create temp dir");
    let site_packages = temp_dir.path().join("venv/lib/python3.11/site-packages");
    let dist_info = site_packages.join("click-8.0.4.dist-info");
    let package_dir = site_packages.join("click");

    fs::create_dir_all(&dist_info).expect("create dist-info dir");
    fs::create_dir_all(&package_dir).expect("create package dir");
    fs::write(
        dist_info.join("METADATA"),
        "Metadata-Version: 2.1\nName: click\nVersion: 8.0.4\n",
    )
    .expect("write METADATA");
    fs::write(
        dist_info.join("RECORD"),
        "click/__init__.py,,0\nclick/core.py,,10\nclick-8.0.4.dist-info/LICENSE.rst,,20\n",
    )
    .expect("write RECORD");
    fs::write(dist_info.join("LICENSE.rst"), "license text").expect("write LICENSE.rst");
    fs::write(package_dir.join("__init__.py"), "").expect("write __init__.py");
    fs::write(package_dir.join("core.py"), "def click():\n    pass\n").expect("write core.py");

    let (mut files, result) = debian_scan_and_assemble_with_keyfiles(temp_dir.path());
    classify_key_files(&mut files, &result.packages);

    let package = result
        .packages
        .iter()
        .find(|package| package.name.as_deref() == Some("click"))
        .expect("click package should be assembled");

    let core_file = files
        .iter()
        .find(|file| file.path.ends_with("site-packages/click/core.py"))
        .expect("core.py should be scanned");
    let license_file = files
        .iter()
        .find(|file| {
            file.path
                .ends_with("site-packages/click-8.0.4.dist-info/LICENSE.rst")
        })
        .expect("license file should be scanned");

    assert!(core_file.for_packages.contains(&package.package_uid));
    assert!(license_file.for_packages.contains(&package.package_uid));
}

#[test]
fn python_pkg_info_scan_assigns_installed_files_entries() {
    let temp_dir = tempfile::TempDir::new().expect("create temp dir");
    let site_packages = temp_dir.path().join("venv/lib/python3.11/site-packages");
    let egg_info = site_packages.join("examplepkg.egg-info");
    let package_dir = site_packages.join("examplepkg");

    fs::create_dir_all(&egg_info).expect("create egg-info dir");
    fs::create_dir_all(&package_dir).expect("create package dir");
    fs::write(
        egg_info.join("PKG-INFO"),
        "Metadata-Version: 1.2\nName: examplepkg\nVersion: 1.0.0\n",
    )
    .expect("write PKG-INFO");
    fs::write(
        egg_info.join("installed-files.txt"),
        "../examplepkg/__init__.py\n../examplepkg/core.py\n",
    )
    .expect("write installed-files.txt");
    fs::write(package_dir.join("__init__.py"), "").expect("write __init__.py");
    fs::write(package_dir.join("core.py"), "VALUE = 1\n").expect("write core.py");

    let (files, result) = debian_scan_and_assemble_with_keyfiles(temp_dir.path());

    let package = result
        .packages
        .iter()
        .find(|package| package.name.as_deref() == Some("examplepkg"))
        .expect("examplepkg package should be assembled");

    let core_file = files
        .iter()
        .find(|file| file.path.ends_with("site-packages/examplepkg/core.py"))
        .expect("core.py should be scanned");

    assert!(core_file.for_packages.contains(&package.package_uid));
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

    let (files, result) = debian_scan_and_assemble_with_keyfiles(temp_dir.path());

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

    assert!(
        control.is_manifest,
        "control file should be manifest; file_type={:?}, for_packages={:?}, package_data_len={}",
        control.file_type,
        control.for_packages,
        control.package_data.len()
    );
    assert!(control.is_key_file, "control keyfile flag missing");
    assert!(copyright.is_legal, "copyright should be legal");
    assert!(copyright.is_key_file, "copyright keyfile flag missing");
    assert!(control.for_packages.contains(&package.package_uid));
    assert!(copyright.for_packages.contains(&package.package_uid));
}

#[test]
fn python_pkg_info_scan_assigns_sources_entries() {
    let temp_dir = tempfile::TempDir::new().expect("create temp dir");
    let egg_info = temp_dir.path().join("PyJPString.egg-info");
    let package_dir = temp_dir.path().join("jpstring");

    fs::create_dir_all(&egg_info).expect("create egg-info dir");
    fs::create_dir_all(&package_dir).expect("create package dir");
    fs::write(
        egg_info.join("PKG-INFO"),
        "Metadata-Version: 1.0\nName: PyJPString\nVersion: 0.0.3\n",
    )
    .expect("write PKG-INFO");
    fs::write(
        egg_info.join("SOURCES.txt"),
        "setup.py\nPyJPString.egg-info/PKG-INFO\nPyJPString.egg-info/top_level.txt\njpstring/__init__.py\n",
    )
    .expect("write SOURCES.txt");
    fs::write(
        temp_dir.path().join("setup.py"),
        "from setuptools import setup\n",
    )
    .expect("write setup.py");
    fs::write(egg_info.join("top_level.txt"), "jpstring\n").expect("write top_level.txt");
    fs::write(package_dir.join("__init__.py"), "").expect("write __init__.py");

    let (files, result) = python_scan_and_assemble(temp_dir.path());

    let package = result
        .packages
        .iter()
        .find(|package| package.name.as_deref() == Some("PyJPString"))
        .expect("PyJPString package should be assembled");

    let setup_file = files
        .iter()
        .find(|file| file.path.ends_with("setup.py"))
        .expect("setup.py should be scanned");
    let module_init = files
        .iter()
        .find(|file| file.path.ends_with("jpstring/__init__.py"))
        .expect("module __init__.py should be scanned");
    let top_level = files
        .iter()
        .find(|file| file.path.ends_with("PyJPString.egg-info/top_level.txt"))
        .expect("top_level.txt should be scanned");

    assert!(setup_file.for_packages.contains(&package.package_uid));
    assert!(module_init.for_packages.contains(&package.package_uid));
    assert!(top_level.for_packages.contains(&package.package_uid));
    assert!(package.extra_data.is_none());
}

#[test]
fn debian_status_d_scan_assigns_installed_files_and_keeps_dependencies() {
    let temp_dir = tempfile::TempDir::new().expect("create temp dir");
    let status_dir = temp_dir.path().join("var/lib/dpkg/status.d");
    let info_dir = temp_dir.path().join("var/lib/dpkg/info");
    let bin_dir = temp_dir.path().join("bin");
    let doc_dir = temp_dir.path().join("usr/share/doc/bash");

    fs::create_dir_all(&status_dir).expect("create status.d dir");
    fs::create_dir_all(&info_dir).expect("create info dir");
    fs::create_dir_all(&bin_dir).expect("create bin dir");
    fs::create_dir_all(&doc_dir).expect("create doc dir");

    fs::write(
        status_dir.join("bash"),
        "Package: bash\nStatus: install ok installed\nPriority: required\nSection: shells\nMaintainer: GNU Bash Maintainers <bash@example.com>\nArchitecture: amd64\nVersion: 5.2-1\nDepends: libc6 (>= 2.36)\nDescription: GNU Bourne Again SHell\n shell\n",
    )
    .expect("write status.d package");
    fs::write(
        info_dir.join("bash.list"),
        "/bin/bash\n/usr/share/doc/bash/copyright\n",
    )
    .expect("write bash.list");
    fs::write(
        info_dir.join("bash.md5sums"),
        "77506afebd3b7e19e937a678a185b62e  bin/bash\n9632d707e9eca8b3ba2b1a98c1c3fdce  usr/share/doc/bash/copyright\n",
    )
    .expect("write bash.md5sums");
    fs::write(bin_dir.join("bash"), "#!/bin/sh\n").expect("write /bin/bash");
    fs::write(doc_dir.join("copyright"), "copyright text\n")
        .expect("write /usr/share/doc/bash/copyright");

    let (files, result) = python_scan_and_assemble(temp_dir.path());

    let package = result
        .packages
        .iter()
        .find(|package| package.name.as_deref() == Some("bash"))
        .expect("bash package should be assembled from status.d");

    assert!(result.dependencies.iter().any(|dep| {
        dep.purl.as_deref() == Some("pkg:deb/debian/libc6")
            && dep.scope.as_deref() == Some("depends")
            && dep.for_package_uid.as_deref() == Some(&package.package_uid)
    }));

    let bash_file = files
        .iter()
        .find(|file| file.path.ends_with("/bin/bash"))
        .expect("/bin/bash should be scanned");
    let copyright_file = files
        .iter()
        .find(|file| file.path.ends_with("/usr/share/doc/bash/copyright"))
        .expect("copyright file should be scanned");

    assert!(bash_file.for_packages.contains(&package.package_uid));
    assert!(copyright_file.for_packages.contains(&package.package_uid));
}

#[test]
fn resolve_thread_count_supports_reference_compat_values() {
    assert_eq!(resolve_thread_count(-1), 1);
    assert_eq!(resolve_thread_count(0), default_parallel_threads());
    assert_eq!(resolve_thread_count(4), 4);
}

#[test]
fn validate_scan_option_compatibility_rejects_scan_flags_with_from_json() {
    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--from-json",
        "--copyright",
        "sample-scan.json",
    ])
    .expect("cli parse should succeed");

    let result = validate_scan_option_compatibility(&cli);
    assert!(result.is_err());
}

#[test]
fn validate_scan_option_compatibility_allows_scan_flags_without_from_json() {
    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--copyright",
        "sample-dir",
    ])
    .expect("cli parse should succeed");

    let result = validate_scan_option_compatibility(&cli);
    assert!(result.is_ok());
}

#[test]
fn validate_scan_option_compatibility_allows_multiple_inputs_with_from_json() {
    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--from-json",
        "scan-a.json",
        "scan-b.json",
    ])
    .expect("cli parse should succeed");

    let result = validate_scan_option_compatibility(&cli);
    assert!(result.is_ok());
}

#[test]
fn validate_scan_option_compatibility_rejects_multiple_paths_without_from_json() {
    let cli =
        crate::cli::Cli::try_parse_from(["provenant", "--json-pp", "scan.json", "dir-a", "dir-b"])
            .expect("cli parse should succeed");

    let result = validate_scan_option_compatibility(&cli);
    assert!(result.is_err());
}

#[test]
fn progress_mode_from_cli_maps_quiet_verbose_default() {
    let default_cli =
        crate::cli::Cli::try_parse_from(["provenant", "--json-pp", "scan.json", "sample-dir"])
            .expect("cli parse should succeed");
    assert_eq!(
        progress_mode_from_cli(&default_cli),
        crate::progress::ProgressMode::Default
    );

    let quiet_cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--quiet",
        "sample-dir",
    ])
    .expect("cli parse should succeed");
    assert_eq!(
        progress_mode_from_cli(&quiet_cli),
        crate::progress::ProgressMode::Quiet
    );

    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--verbose",
        "sample-dir",
    ])
    .expect("cli parse should succeed");

    assert_eq!(
        progress_mode_from_cli(&cli),
        crate::progress::ProgressMode::Verbose
    );
}

#[test]
fn prepare_cache_for_scan_defaults_to_scan_root_cache_directory() {
    let temp_dir = tempfile::TempDir::new().expect("create temp dir");
    let scan_root = temp_dir.path().join("scan");
    fs::create_dir_all(&scan_root).expect("create scan root");

    let cli =
        crate::cli::Cli::try_parse_from(["provenant", "--json-pp", "scan.json", "sample-dir"])
            .expect("cli parse should succeed");

    let config = prepare_cache_for_scan(scan_root.to_str().expect("utf-8 path"), &cli)
        .expect("cache preparation should succeed");

    assert_eq!(config.root_dir(), scan_root.join(".provenant-cache"));
    assert!(config.index_dir().exists());
    assert!(config.scan_results_dir().exists());
}

#[test]
fn prepare_cache_for_scan_respects_cache_dir_and_cache_clear() {
    let temp_dir = tempfile::TempDir::new().expect("create temp dir");
    let scan_root = temp_dir.path().join("scan");
    fs::create_dir_all(&scan_root).expect("create scan root");

    let explicit_cache_dir = temp_dir.path().join("explicit-cache");
    fs::create_dir_all(explicit_cache_dir.join("index")).expect("create stale cache dir");
    let stale_file = explicit_cache_dir.join("index").join("stale.txt");
    fs::write(&stale_file, "old").expect("write stale file");

    let cli = crate::cli::Cli::try_parse_from([
        "provenant",
        "--json-pp",
        "scan.json",
        "--cache-dir",
        explicit_cache_dir.to_str().expect("utf-8 path"),
        "--cache-clear",
        "sample-dir",
    ])
    .expect("cli parse should succeed");

    let config = prepare_cache_for_scan(scan_root.to_str().expect("utf-8 path"), &cli)
        .expect("cache preparation should succeed");

    assert_eq!(config.root_dir(), explicit_cache_dir);
    assert!(!stale_file.exists());
    assert!(config.index_dir().exists());
    assert!(config.scan_results_dir().exists());
}
