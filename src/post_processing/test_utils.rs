#[cfg(feature = "golden-tests")]
use std::fs;
use std::path::Path;
#[cfg(feature = "golden-tests")]
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(feature = "golden-tests")]
use std::sync::OnceLock;

#[cfg(feature = "golden-tests")]
use chrono::Utc;
#[cfg(feature = "golden-tests")]
use flate2::read::GzDecoder;
use glob::Pattern;
#[cfg(feature = "golden-tests")]
use serde_json::{Value, json};
#[cfg(feature = "golden-tests")]
use tar::Archive;
#[cfg(feature = "golden-tests")]
use tempfile::{TempDir, tempdir};

use super::*;
use crate::assembly;
use crate::cache::{DEFAULT_CACHE_DIR_NAME, build_collection_exclude_patterns};
#[cfg(feature = "golden-tests")]
use crate::license_detection::LicenseDetectionEngine;
use crate::models::{FileInfo, FileType, Package, PackageType, PackageUid};
use crate::progress::{ProgressMode, ScanProgress};
#[cfg(feature = "golden-tests")]
use crate::scan_result_shaping::normalize_paths;
use crate::scanner::{LicenseScanOptions, TextDetectionOptions, collect_paths, process_collected};

pub(crate) fn file(path: &str) -> FileInfo {
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
        Vec::new(),
    )
}

pub(crate) fn dir(path: &str) -> FileInfo {
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
        Vec::new(),
    )
}

pub(crate) fn package(uid: &str, path: &str) -> Package {
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
        package_uid: PackageUid::from_raw(uid.to_string()),
        datafile_paths: vec![path.to_string()],
    }
}

#[cfg(feature = "golden-tests")]
pub(crate) fn test_license_engine() -> Arc<LicenseDetectionEngine> {
    static ENGINE: OnceLock<Arc<LicenseDetectionEngine>> = OnceLock::new();
    Arc::clone(ENGINE.get_or_init(|| {
        Arc::new(
            LicenseDetectionEngine::from_embedded()
                .expect("embedded license engine should initialize"),
        )
    }))
}

#[cfg(feature = "golden-tests")]
pub(crate) struct FixtureScanRoot {
    pub(crate) scan_root: PathBuf,
    pub(crate) normalize_root: PathBuf,
    _temp_dir: Option<TempDir>,
}

#[cfg(feature = "golden-tests")]
pub(crate) fn compare_scan_json_values(
    actual: &Value,
    expected: &Value,
    path: &str,
) -> Result<(), String> {
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

#[cfg(feature = "golden-tests")]
pub(crate) fn normalize_scan_json(value: &mut Value, parent_key: Option<&str>) {
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

pub(crate) fn fixture_exclude_patterns(root: &Path) -> Vec<Pattern> {
    build_collection_exclude_patterns(root, &root.join(DEFAULT_CACHE_DIR_NAME))
}

#[cfg(feature = "golden-tests")]
pub(crate) fn extract_archive_fixture(archive_path: &Path) -> FixtureScanRoot {
    let temp_dir = tempdir().expect("tempdir should be created");
    let extracted_root = temp_dir.path().join(
        archive_path
            .file_name()
            .expect("archive fixture should have a file name"),
    );

    fs::create_dir_all(&extracted_root).expect("archive fixture extraction root should be created");

    let archive_file =
        fs::File::open(archive_path).expect("archive fixture should be readable for extraction");
    let decoder = GzDecoder::new(archive_file);
    let mut archive = Archive::new(decoder);
    archive
        .unpack(&extracted_root)
        .expect("archive fixture should extract successfully");

    FixtureScanRoot {
        scan_root: extracted_root,
        normalize_root: temp_dir.path().to_path_buf(),
        _temp_dir: Some(temp_dir),
    }
}

#[cfg(feature = "golden-tests")]
pub(crate) fn resolve_fixture_scan_root(fixture_root: &Path) -> FixtureScanRoot {
    if !fixture_root.exists()
        && let Some(file_name) = fixture_root.file_name().and_then(|name| name.to_str())
    {
        let archive_path = fixture_root.with_file_name(format!("{file_name}.tar.gz"));
        if archive_path.is_file() {
            return extract_archive_fixture(&archive_path);
        }
    }

    let codebase_root = fixture_root.join("codebase");
    if codebase_root.is_dir() {
        return FixtureScanRoot {
            scan_root: codebase_root,
            normalize_root: fixture_root.to_path_buf(),
            _temp_dir: None,
        };
    }

    let project_entries: Vec<PathBuf> = std::fs::read_dir(fixture_root)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                || path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| !name.contains(".expected"))
        })
        .collect();

    if project_entries.len() == 1 {
        let project_entry = &project_entries[0];
        if project_entry.is_dir() {
            return FixtureScanRoot {
                scan_root: project_entry.clone(),
                normalize_root: fixture_root.to_path_buf(),
                _temp_dir: None,
            };
        }

        if project_entry
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".tar.gz"))
        {
            return extract_archive_fixture(project_entry);
        }
    }

    FixtureScanRoot {
        scan_root: fixture_root.to_path_buf(),
        normalize_root: fixture_root.to_path_buf(),
        _temp_dir: None,
    }
}

#[cfg(feature = "golden-tests")]
pub(crate) fn strip_root_prefix_for_test(path: &Path, root: &Path) -> Option<PathBuf> {
    if let Ok(stripped) = path.strip_prefix(root)
        && !stripped.as_os_str().is_empty()
    {
        return Some(stripped.to_path_buf());
    }

    let canonical_path = path.canonicalize().ok()?;
    let canonical_root = root.canonicalize().ok()?;
    let stripped = canonical_path.strip_prefix(canonical_root).ok()?;
    if stripped.as_os_str().is_empty() {
        None
    } else {
        Some(stripped.to_path_buf())
    }
}

#[cfg(feature = "golden-tests")]
pub(crate) fn normalize_paths_for_test(files: &mut [FileInfo], scan_root: &str) {
    normalize_paths(files, scan_root, true, false);
}

#[cfg(feature = "golden-tests")]
pub(crate) fn normalize_package_datafile_paths(packages: &mut [Package], scan_root: &Path) {
    for package in packages {
        for path in &mut package.datafile_paths {
            if let Some(stripped) = strip_root_prefix_for_test(Path::new(path), scan_root) {
                *path = stripped.to_string_lossy().to_string();
            }
        }
    }
}

#[cfg(feature = "golden-tests")]
pub(crate) struct FixtureOutputOptions<'a> {
    pub(crate) facet_defs: &'a [String],
    pub(crate) include_classify: bool,
    pub(crate) include_summary: bool,
    pub(crate) include_license_clarity_score: bool,
    pub(crate) include_tallies: bool,
    pub(crate) include_tallies_of_key_files: bool,
    pub(crate) include_tallies_with_details: bool,
    pub(crate) include_tallies_by_facet: bool,
    pub(crate) include_generated: bool,
    pub(crate) include_top_level_license_data: bool,
}

#[cfg(feature = "golden-tests")]
pub(crate) fn compute_fixture_output(
    fixture_dir: &str,
    options: FixtureOutputOptions<'_>,
) -> Value {
    let fixture_root = Path::new(fixture_dir);
    let resolved_scan_root = resolve_fixture_scan_root(fixture_root);
    let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
    let exclude_patterns = fixture_exclude_patterns(&resolved_scan_root.scan_root);
    let collected = collect_paths(&resolved_scan_root.scan_root, 0, &exclude_patterns);
    let facet_rules = build_facet_rules(options.facet_defs).expect("facet rules should compile");
    let scan_result = process_collected(
        &collected,
        progress,
        Some(test_license_engine()),
        LicenseScanOptions::default(),
        &TextDetectionOptions {
            collect_info: true,
            detect_packages: true,
            detect_application_packages: true,
            detect_system_packages: true,
            detect_packages_in_compiled: false,
            detect_generated: options.include_generated,
            ..TextDetectionOptions::default()
        },
    );

    let mut files = scan_result.files;
    normalize_paths_for_test(
        &mut files,
        resolved_scan_root
            .normalize_root
            .to_str()
            .expect("fixture path should be UTF-8"),
    );
    if let Some(root_name) = resolved_scan_root
        .scan_root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        && !files.iter().any(|file| file.path == root_name)
    {
        files.push(dir(root_name));
    }
    let mut assembly_result = assembly::assemble(&mut files);
    for package in &mut assembly_result.packages {
        package.backfill_license_provenance();
    }
    apply_package_reference_following(&mut files, &mut assembly_result.packages);

    let (license_detections, license_references, license_rule_references) = if options
        .include_top_level_license_data
    {
        let engine = test_license_engine();
        let license_detections = collect_top_level_license_detections(&files);
        let (license_references, license_rule_references) = collect_top_level_license_references(
            &files,
            &assembly_result.packages,
            engine.index(),
            DEFAULT_LICENSEDB_URL_TEMPLATE,
        );
        (
            license_detections,
            license_references,
            license_rule_references,
        )
    } else {
        (Vec::new(), Vec::new(), Vec::new())
    };

    let models_output = create_output(
        Utc::now(),
        Utc::now(),
        crate::scanner::ProcessResult {
            excluded_count: scan_result.excluded_count,
            files,
        },
        CreateOutputContext {
            total_dirs: collected.directories.len(),
            assembly_result,
            license_detections,
            license_references,
            license_rule_references,
            spdx_license_list_version: "3.27".to_string(),
            license_index_provenance: None,
            extra_errors: vec![],
            extra_warnings: vec![],
            header_options: serde_json::Map::new(),
            options: CreateOutputOptions {
                facet_rules: &facet_rules,
                include_classify: options.include_classify,
                include_summary: options.include_summary,
                include_license_clarity_score: options.include_license_clarity_score,
                include_tallies: options.include_tallies,
                include_tallies_of_key_files: options.include_tallies_of_key_files,
                include_tallies_with_details: options.include_tallies_with_details,
                include_tallies_by_facet: options.include_tallies_by_facet,
                include_generated: options.include_generated,
                verbose: false,
            },
        },
    );
    serde_json::to_value(crate::output_schema::Output::from(&models_output))
        .expect("fixture output should serialize")
}

#[cfg(feature = "golden-tests")]
pub(crate) fn compute_fixture_summary(
    fixture_dir: &str,
    include_summary: bool,
    include_score: bool,
) -> Value {
    let fixture_root = Path::new(fixture_dir);
    let resolved_scan_root = resolve_fixture_scan_root(fixture_root);
    let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
    let exclude_patterns = fixture_exclude_patterns(&resolved_scan_root.scan_root);
    let collected = collect_paths(&resolved_scan_root.scan_root, 0, &exclude_patterns);
    let scan_result = process_collected(
        &collected,
        progress,
        Some(test_license_engine()),
        LicenseScanOptions::default(),
        &TextDetectionOptions {
            collect_info: true,
            detect_packages: true,
            detect_application_packages: true,
            detect_system_packages: true,
            detect_packages_in_compiled: false,
            ..TextDetectionOptions::default()
        },
    );

    let mut files = scan_result.files;
    normalize_paths_for_test(
        &mut files,
        resolved_scan_root
            .normalize_root
            .to_str()
            .expect("fixture path should be UTF-8"),
    );
    let mut assembly_result = assembly::assemble(&mut files);
    for package in &mut assembly_result.packages {
        package.backfill_license_provenance();
    }
    apply_package_reference_following(&mut files, &mut assembly_result.packages);
    let mut packages = assembly_result.packages;
    normalize_package_datafile_paths(&mut packages, &resolved_scan_root.normalize_root);

    let package_file_index = PackageFileIndex::build(&files, &packages);
    apply_file_classification(&mut files, &package_file_index);
    let indexes = OutputIndexes::build(
        &files,
        Some(&package_file_index),
        false,
        OutputIndexMode::Full,
    );
    promote_package_metadata_from_key_files(&files, &mut packages, &indexes);

    let models_summary =
        compute_summary_with_options(&files, &packages, &indexes, include_summary, include_score)
            .expect("fixture summary should exist");
    serde_json::to_value(crate::output_schema::OutputSummary::from(&models_summary))
        .expect("fixture summary should serialize")
}

#[cfg(feature = "golden-tests")]
pub(crate) fn assert_summary_fixture_matches_expected(
    fixture_dir: &str,
    expected_file: &str,
    include_summary: bool,
    include_score: bool,
) {
    let actual_summary = compute_fixture_summary(fixture_dir, include_summary, include_score);
    let expected: Value = serde_json::from_str(
        &fs::read_to_string(expected_file).expect("expected summary fixture should be readable"),
    )
    .expect("expected summary fixture should parse");
    let expected_summary = expected
        .get("summary")
        .expect("expected fixture should contain summary")
        .clone();

    if let Err(error) = compare_scan_json_values(&actual_summary, &expected_summary, "summary") {
        panic!(
            "Summary fixture mismatch for {} vs {}: {}\nactual summary: {}\nexpected summary: {}",
            fixture_dir,
            expected_file,
            error,
            serde_json::to_string_pretty(&actual_summary).unwrap_or_default(),
            serde_json::to_string_pretty(&expected_summary).unwrap_or_default()
        );
    }
}

#[cfg(feature = "golden-tests")]
pub(crate) fn project_classify_fields(value: &Value) -> Value {
    let bool_or_false =
        |file: &Value, key: &str| file.get(key).cloned().unwrap_or(Value::Bool(false));

    let files = value
        .get("files")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    json!({
        "files": files
            .into_iter()
            .map(|file| {
                json!({
                    "path": file.get("path").cloned().unwrap_or(Value::Null),
                    "type": file.get("type").cloned().unwrap_or(Value::Null),
                    "name": file.get("name").cloned().unwrap_or(Value::Null),
                    "base_name": file.get("base_name").cloned().unwrap_or(Value::Null),
                    "extension": file.get("extension").cloned().unwrap_or(Value::Null),
                    "is_legal": bool_or_false(&file, "is_legal"),
                    "is_manifest": bool_or_false(&file, "is_manifest"),
                    "is_readme": bool_or_false(&file, "is_readme"),
                    "is_top_level": bool_or_false(&file, "is_top_level"),
                    "is_key_file": bool_or_false(&file, "is_key_file"),
                    "is_community": bool_or_false(&file, "is_community"),
                })
            })
            .collect::<Vec<_>>()
    })
}

#[cfg(feature = "golden-tests")]
pub(crate) fn project_file_info_fields(value: &Value) -> Value {
    let bool_or_false =
        |file: &Value, key: &str| file.get(key).cloned().unwrap_or(Value::Bool(false));

    let files = value
        .get("files")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    json!({
        "files": files
            .into_iter()
            .map(|file| {
                json!({
                    "path": file.get("path").cloned().unwrap_or(Value::Null),
                    "type": file.get("type").cloned().unwrap_or(Value::Null),
                    "name": file.get("name").cloned().unwrap_or(Value::Null),
                    "base_name": file.get("base_name").cloned().unwrap_or(Value::Null),
                    "extension": file.get("extension").cloned().unwrap_or(Value::Null),
                    "mime_type": file.get("mime_type").cloned().unwrap_or(Value::Null),
                    "file_type": file.get("file_type").cloned().unwrap_or(Value::Null),
                    "programming_language": file.get("programming_language").cloned().unwrap_or(Value::Null),
                    "is_binary": bool_or_false(&file, "is_binary"),
                    "is_text": bool_or_false(&file, "is_text"),
                    "is_archive": bool_or_false(&file, "is_archive"),
                    "is_media": bool_or_false(&file, "is_media"),
                    "is_source": bool_or_false(&file, "is_source"),
                    "is_script": bool_or_false(&file, "is_script"),
                })
            })
            .collect::<Vec<_>>()
    })
}

#[cfg(feature = "golden-tests")]
pub(crate) fn project_tally_fields(value: &Value) -> Value {
    let files = value
        .get("files")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    json!({
        "tallies": value.get("tallies").cloned().unwrap_or(Value::Null),
        "tallies_of_key_files": value.get("tallies_of_key_files").cloned().unwrap_or(Value::Null),
        "tallies_by_facet": value.get("tallies_by_facet").cloned().unwrap_or(Value::Null),
        "files": files
            .into_iter()
            .map(|file| {
                json!({
                    "path": file.get("path").cloned().unwrap_or(Value::Null),
                    "type": file.get("type").cloned().unwrap_or(Value::Null),
                    "is_legal": file.get("is_legal").cloned().unwrap_or(Value::Bool(false)),
                    "is_manifest": file.get("is_manifest").cloned().unwrap_or(Value::Bool(false)),
                    "is_readme": file.get("is_readme").cloned().unwrap_or(Value::Bool(false)),
                    "is_top_level": file.get("is_top_level").cloned().unwrap_or(Value::Bool(false)),
                    "is_key_file": file.get("is_key_file").cloned().unwrap_or(Value::Bool(false)),
                    "is_community": file.get("is_community").cloned().unwrap_or(Value::Bool(false)),
                    "facets": file.get("facets").cloned().unwrap_or_else(|| json!([])),
                    "tallies": file.get("tallies").cloned().unwrap_or(Value::Null),
                })
            })
            .collect::<Vec<_>>()
    })
}

#[cfg(feature = "golden-tests")]
pub(crate) fn project_facet_fields(value: &Value) -> Value {
    let files = value
        .get("files")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    json!({
        "files": files
            .into_iter()
            .map(|file| {
                json!({
                    "path": file.get("path").cloned().unwrap_or(Value::Null),
                    "type": file.get("type").cloned().unwrap_or(Value::Null),
                    "facets": file.get("facets").cloned().unwrap_or_else(|| json!([])),
                    "scan_errors": file.get("scan_errors").cloned().unwrap_or_else(|| json!([])),
                })
            })
            .collect::<Vec<_>>()
    })
}

#[cfg(feature = "golden-tests")]
pub(crate) fn project_package_fields(value: &Value) -> Value {
    let packages = value
        .get("packages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let dependencies = value
        .get("dependencies")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    json!({
        "packages": packages
            .into_iter()
            .map(|package| {
                json!({
                    "type": package.get("type").cloned().unwrap_or(Value::Null),
                    "namespace": package.get("namespace").cloned().unwrap_or(Value::Null),
                    "name": package.get("name").cloned().unwrap_or(Value::Null),
                    "version": package.get("version").cloned().unwrap_or(Value::Null),
                    "purl": package.get("purl").cloned().unwrap_or(Value::Null),
                    "declared_license_expression": package.get("declared_license_expression").cloned().unwrap_or(Value::Null),
                    "declared_license_expression_spdx": package.get("declared_license_expression_spdx").cloned().unwrap_or(Value::Null),
                    "other_license_expression": package.get("other_license_expression").cloned().unwrap_or(Value::Null),
                    "other_license_expression_spdx": package.get("other_license_expression_spdx").cloned().unwrap_or(Value::Null),
                    "datafile_paths": package.get("datafile_paths").cloned().unwrap_or_else(|| json!([])),
                    "datasource_ids": package.get("datasource_ids").cloned().unwrap_or_else(|| json!([])),
                })
            })
            .collect::<Vec<_>>(),
        "dependencies": dependencies
            .into_iter()
            .map(|dependency| {
                json!({
                    "purl": dependency.get("purl").cloned().unwrap_or(Value::Null),
                    "extracted_requirement": dependency.get("extracted_requirement").cloned().unwrap_or(Value::Null),
                    "scope": dependency.get("scope").cloned().unwrap_or(Value::Null),
                    "is_runtime": dependency.get("is_runtime").cloned().unwrap_or(Value::Null),
                    "is_optional": dependency.get("is_optional").cloned().unwrap_or(Value::Null),
                    "is_pinned": dependency.get("is_pinned").cloned().unwrap_or(Value::Null),
                    "is_direct": dependency.get("is_direct").cloned().unwrap_or(Value::Null),
                    "datafile_path": dependency.get("datafile_path").cloned().unwrap_or(Value::Null),
                    "datasource_id": dependency.get("datasource_id").cloned().unwrap_or(Value::Null),
                })
            })
            .collect::<Vec<_>>()
    })
}

#[cfg(feature = "golden-tests")]
fn project_detection_fields(detection: &Value) -> Value {
    json!({
        "license_expression": detection.get("license_expression").cloned().unwrap_or(Value::Null),
        "license_expression_spdx": detection.get("license_expression_spdx").cloned().unwrap_or(Value::Null),
        "detection_log": detection.get("detection_log").cloned().unwrap_or_else(|| json!([])),
        "identifier": detection.get("identifier").cloned().unwrap_or(Value::Null),
        "matches": detection
            .get("matches")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|match_value| {
                json!({
                    "from_file": match_value.get("from_file").cloned().unwrap_or(Value::Null),
                    "matched_text": match_value.get("matched_text").cloned().unwrap_or(Value::Null),
                    "license_expression": match_value.get("license_expression").cloned().unwrap_or(Value::Null),
                    "license_expression_spdx": match_value.get("license_expression_spdx").cloned().unwrap_or(Value::Null),
                    "rule_identifier": match_value.get("rule_identifier").cloned().unwrap_or(Value::Null),
                    "referenced_filenames": match_value.get("referenced_filenames").cloned().unwrap_or_else(|| json!([])),
                })
            })
            .collect::<Vec<_>>()
    })
}

#[cfg(feature = "golden-tests")]
fn project_tally_entries(entries: Option<&Value>) -> Value {
    Value::Array(
        entries
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|entry| entry.get("value").is_some_and(|value| !value.is_null()))
            .collect(),
    )
}

#[cfg(feature = "golden-tests")]
fn project_tallies(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };

    json!({
        "detected_license_expression": project_tally_entries(value.get("detected_license_expression")),
        "copyrights": project_tally_entries(value.get("copyrights")),
        "holders": project_tally_entries(value.get("holders")),
        "authors": project_tally_entries(value.get("authors")),
        "programming_language": project_tally_entries(value.get("programming_language")),
    })
}

#[cfg(feature = "golden-tests")]
pub(crate) fn project_reference_follow_fields(value: &Value) -> Value {
    let files = value
        .get("files")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let packages = value
        .get("packages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let top_level_detections = value
        .get("license_detections")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let license_references = value
        .get("license_references")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let license_rule_references = value
        .get("license_rule_references")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    json!({
        "summary": value.get("summary").map(|summary| {
            json!({
                "declared_license_expression": summary.get("declared_license_expression").cloned().unwrap_or(Value::Null),
                "license_clarity_score": summary.get("license_clarity_score").cloned().unwrap_or(Value::Null),
                "declared_holder": summary
                    .get("declared_holder")
                    .cloned()
                    .filter(|holder| holder != "")
                    .unwrap_or(Value::Null),
                "primary_language": summary.get("primary_language").cloned().unwrap_or(Value::Null),
                "other_license_expressions": project_tally_entries(summary.get("other_license_expressions")),
                "other_holders": project_tally_entries(summary.get("other_holders")),
                "other_languages": project_tally_entries(summary.get("other_languages")),
            })
        }).unwrap_or(Value::Null),
        "tallies": project_tallies(value.get("tallies")),
        "tallies_of_key_files": project_tallies(value.get("tallies_of_key_files")),
        "license_detections": top_level_detections.into_iter().map(|detection| {
            json!({
                "identifier": detection.get("identifier").cloned().unwrap_or(Value::Null),
                "license_expression": detection.get("license_expression").cloned().unwrap_or(Value::Null),
                "license_expression_spdx": detection.get("license_expression_spdx").cloned().unwrap_or(Value::Null),
                "detection_count": detection.get("detection_count").cloned().unwrap_or(Value::Null),
                "detection_log": detection.get("detection_log").cloned().unwrap_or_else(|| json!([])),
                "reference_matches": detection
                    .get("reference_matches")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|match_value| {
                        json!({
                            "from_file": match_value.get("from_file").cloned().unwrap_or(Value::Null),
                            "license_expression": match_value.get("license_expression").cloned().unwrap_or(Value::Null),
                            "license_expression_spdx": match_value.get("license_expression_spdx").cloned().unwrap_or(Value::Null),
                            "rule_identifier": match_value.get("rule_identifier").cloned().unwrap_or(Value::Null),
                        })
                    })
                    .collect::<Vec<_>>()
            })
        }).collect::<Vec<_>>(),
        "license_references": license_references.into_iter().map(|reference| {
            json!({
                "key": reference.get("key").cloned().unwrap_or(Value::Null),
                "language": reference.get("language").cloned().unwrap_or(Value::Null),
                "short_name": reference.get("short_name").cloned().unwrap_or(Value::Null),
                "name": reference.get("name").cloned().unwrap_or(Value::Null),
                "owner": reference.get("owner").cloned().unwrap_or(Value::Null),
                "homepage_url": reference.get("homepage_url").cloned().unwrap_or(Value::Null),
                "spdx_license_key": reference.get("spdx_license_key").cloned().unwrap_or(Value::Null),
                "osi_license_key": reference.get("osi_license_key").cloned().unwrap_or(Value::Null),
                "text_urls": reference.get("text_urls").cloned().unwrap_or_else(|| json!([])),
                "osi_url": reference.get("osi_url").cloned().unwrap_or(Value::Null),
                "faq_url": reference.get("faq_url").cloned().unwrap_or(Value::Null),
                "other_urls": reference.get("other_urls").cloned().unwrap_or_else(|| json!([])),
                "category": reference.get("category").cloned().unwrap_or(Value::Null),
                "is_exception": reference.get("is_exception").cloned().unwrap_or(Value::Bool(false)),
                "is_unknown": reference.get("is_unknown").cloned().unwrap_or(Value::Bool(false)),
                "is_generic": reference.get("is_generic").cloned().unwrap_or(Value::Bool(false)),
                "minimum_coverage": reference.get("minimum_coverage").cloned().unwrap_or(Value::Null),
                "standard_notice": reference.get("standard_notice").cloned().unwrap_or(Value::Null),
            })
        }).collect::<Vec<_>>(),
        "license_rule_references": license_rule_references.into_iter().map(|rule| {
            json!({
                "identifier": rule.get("identifier").cloned().unwrap_or(Value::Null),
                "license_expression": rule.get("license_expression").cloned().unwrap_or(Value::Null),
                "language": rule.get("language").cloned().unwrap_or(Value::Null),
                "rule_url": rule.get("rule_url").cloned().unwrap_or(Value::Null),
                "is_synthetic": rule.get("is_synthetic").cloned().unwrap_or(Value::Bool(false)),
                "length": rule.get("length").cloned().unwrap_or(Value::from(0)),
                "referenced_filenames": rule.get("referenced_filenames").cloned().unwrap_or_else(|| json!([])),
            })
        }).collect::<Vec<_>>(),
        "packages": packages.into_iter().map(|package| {
            json!({
                "type": package.get("type").cloned().unwrap_or(Value::Null),
                "name": package.get("name").cloned().unwrap_or(Value::Null),
                "version": package.get("version").cloned().unwrap_or(Value::Null),
                "declared_license_expression": package.get("declared_license_expression").cloned().unwrap_or(Value::Null),
                "declared_license_expression_spdx": package.get("declared_license_expression_spdx").cloned().unwrap_or(Value::Null),
                "other_license_expression": package.get("other_license_expression").cloned().unwrap_or(Value::Null),
                "other_license_expression_spdx": package.get("other_license_expression_spdx").cloned().unwrap_or(Value::Null),
                "datafile_paths": package.get("datafile_paths").cloned().unwrap_or_else(|| json!([])),
                "license_detections": package
                    .get("license_detections")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|detection| project_detection_fields(&detection))
                    .collect::<Vec<_>>(),
                "other_license_detections": package
                    .get("other_license_detections")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|detection| project_detection_fields(&detection))
                    .collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
        "files": files.into_iter().filter(|file| {
            file.get("type").and_then(Value::as_str) == Some("file")
                && (
                    file.get("is_key_file").and_then(Value::as_bool).unwrap_or(false)
                        || file.get("is_manifest").and_then(Value::as_bool).unwrap_or(false)
                        || file
                            .get("license_detections")
                            .and_then(Value::as_array)
                            .is_some_and(|detections| !detections.is_empty())
                        || file
                            .get("package_data")
                            .and_then(Value::as_array)
                            .is_some_and(|package_data| !package_data.is_empty())
                )
        }).map(|file| {
            json!({
                "path": file.get("path").cloned().unwrap_or(Value::Null),
                "type": file.get("type").cloned().unwrap_or(Value::Null),
                "is_top_level": file.get("is_top_level").cloned().unwrap_or(Value::Bool(false)),
                "is_key_file": file.get("is_key_file").cloned().unwrap_or(Value::Bool(false)),
                "is_manifest": file.get("is_manifest").cloned().unwrap_or(Value::Bool(false)),
                "detected_license_expression": file.get("detected_license_expression").cloned().unwrap_or(Value::Null),
                "detected_license_expression_spdx": file.get("detected_license_expression_spdx").cloned().unwrap_or(Value::Null),
                "license_detections": file
                    .get("license_detections")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|detection| project_detection_fields(&detection))
                    .collect::<Vec<_>>(),
                "package_data": file
                    .get("package_data")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|package_data| {
                        json!({
                            "type": package_data.get("type").cloned().unwrap_or(Value::Null),
                            "name": package_data.get("name").cloned().unwrap_or(Value::Null),
                            "version": package_data.get("version").cloned().unwrap_or(Value::Null),
                            "declared_license_expression": package_data.get("declared_license_expression").cloned().unwrap_or(Value::Null),
                            "declared_license_expression_spdx": package_data.get("declared_license_expression_spdx").cloned().unwrap_or(Value::Null),
                            "other_license_expression": package_data.get("other_license_expression").cloned().unwrap_or(Value::Null),
                            "other_license_expression_spdx": package_data.get("other_license_expression_spdx").cloned().unwrap_or(Value::Null),
                            "license_detections": package_data
                                .get("license_detections")
                                .and_then(Value::as_array)
                                .cloned()
                                .unwrap_or_default()
                                .into_iter()
                                .map(|detection| project_detection_fields(&detection))
                                .collect::<Vec<_>>(),
                            "other_license_detections": package_data
                                .get("other_license_detections")
                                .and_then(Value::as_array)
                                .cloned()
                                .unwrap_or_default()
                                .into_iter()
                                .map(|detection| project_detection_fields(&detection))
                                .collect::<Vec<_>>(),
                        })
                    })
                    .collect::<Vec<_>>()
            })
        }).collect::<Vec<_>>()
    })
}

#[cfg(feature = "golden-tests")]
pub(crate) fn assert_reference_follow_fixture_matches_expected(
    fixture_dir: &str,
    expected_file: &str,
) {
    let actual = project_reference_follow_fields(&compute_fixture_output(
        fixture_dir,
        FixtureOutputOptions {
            facet_defs: &[],
            include_classify: true,
            include_summary: true,
            include_license_clarity_score: false,
            include_tallies: true,
            include_tallies_of_key_files: true,
            include_tallies_with_details: false,
            include_tallies_by_facet: false,
            include_generated: false,
            include_top_level_license_data: true,
        },
    ));
    let expected: Value = serde_json::from_str(
        &fs::read_to_string(expected_file)
            .expect("expected reference follow fixture should be readable"),
    )
    .expect("expected reference follow fixture should parse");

    let mut actual_normalized = actual;
    let mut expected_normalized = expected;
    normalize_scan_json(&mut actual_normalized, None);
    normalize_scan_json(&mut expected_normalized, None);

    if let Err(error) = compare_scan_json_values(&actual_normalized, &expected_normalized, "") {
        panic!(
            "Reference-follow fixture mismatch for {} vs {}: {}\nactual={}\nexpected={}",
            fixture_dir,
            expected_file,
            error,
            serde_json::to_string_pretty(&actual_normalized).unwrap_or_default(),
            serde_json::to_string_pretty(&expected_normalized).unwrap_or_default()
        );
    }
}

#[cfg(feature = "golden-tests")]
pub(crate) fn assert_package_fixture_matches_expected(fixture_dir: &str, expected_file: &str) {
    let actual = project_package_fields(&compute_fixture_output(
        fixture_dir,
        FixtureOutputOptions {
            facet_defs: &[],
            include_classify: false,
            include_summary: false,
            include_license_clarity_score: false,
            include_tallies: false,
            include_tallies_of_key_files: false,
            include_tallies_with_details: false,
            include_tallies_by_facet: false,
            include_generated: false,
            include_top_level_license_data: false,
        },
    ));
    let expected: Value = serde_json::from_str(
        &fs::read_to_string(expected_file).expect("expected package fixture should be readable"),
    )
    .expect("expected package fixture should parse");
    let expected = project_package_fields(&expected);

    let mut actual_normalized = actual;
    let mut expected_normalized = expected;
    normalize_scan_json(&mut actual_normalized, None);
    normalize_scan_json(&mut expected_normalized, None);

    if let Err(error) = compare_scan_json_values(&actual_normalized, &expected_normalized, "") {
        panic!(
            "Package fixture mismatch for {} vs {}: {}\nactual={}\nexpected={}",
            fixture_dir,
            expected_file,
            error,
            serde_json::to_string_pretty(&actual_normalized).unwrap_or_default(),
            serde_json::to_string_pretty(&expected_normalized).unwrap_or_default()
        );
    }
}

#[cfg(all(test, feature = "golden-tests"))]
mod tests {
    use super::*;

    #[test]
    fn test_projection_helpers_keep_other_license_expression_spdx() {
        let package_projection = project_package_fields(&json!({
            "packages": [
                {
                    "type": "deb",
                    "name": "demo",
                    "declared_license_expression": "mit",
                    "declared_license_expression_spdx": "MIT",
                    "other_license_expression": "apache-2.0",
                    "other_license_expression_spdx": "Apache-2.0",
                    "datafile_paths": [],
                    "datasource_ids": []
                }
            ],
            "dependencies": []
        }));
        assert_eq!(
            package_projection["packages"][0]["other_license_expression_spdx"],
            json!("Apache-2.0")
        );

        let reference_projection = project_reference_follow_fields(&json!({
            "packages": [
                {
                    "type": "deb",
                    "name": "demo",
                    "declared_license_expression": "mit",
                    "declared_license_expression_spdx": "MIT",
                    "other_license_expression": "apache-2.0",
                    "other_license_expression_spdx": "Apache-2.0",
                    "datafile_paths": [],
                    "license_detections": [],
                    "other_license_detections": []
                }
            ],
            "files": [
                {
                    "path": "demo/copyright",
                    "type": "file",
                    "is_top_level": true,
                    "is_key_file": true,
                    "is_manifest": true,
                    "detected_license_expression": null,
                    "detected_license_expression_spdx": null,
                    "license_detections": [],
                    "package_data": [
                        {
                            "type": "deb",
                            "name": "demo",
                            "version": "1.0.0",
                            "declared_license_expression": "mit",
                            "declared_license_expression_spdx": "MIT",
                            "other_license_expression": "apache-2.0",
                            "other_license_expression_spdx": "Apache-2.0",
                            "license_detections": [],
                            "other_license_detections": []
                        }
                    ]
                }
            ],
            "license_detections": [],
            "license_references": [],
            "license_rule_references": [],
            "summary": null,
            "tallies": null,
            "tallies_of_key_files": null
        }));
        assert_eq!(
            reference_projection["packages"][0]["other_license_expression_spdx"],
            json!("Apache-2.0")
        );
        assert_eq!(
            reference_projection["files"][0]["package_data"][0]["other_license_expression_spdx"],
            json!("Apache-2.0")
        );
    }
}

#[cfg(feature = "golden-tests")]
pub(crate) fn assert_facet_fixture_matches_expected(
    fixture_dir: &str,
    expected_file: &str,
    facet_defs: &[String],
) {
    let actual = project_facet_fields(&compute_fixture_output(
        fixture_dir,
        FixtureOutputOptions {
            facet_defs,
            include_classify: false,
            include_summary: false,
            include_license_clarity_score: false,
            include_tallies: false,
            include_tallies_of_key_files: false,
            include_tallies_with_details: false,
            include_tallies_by_facet: false,
            include_generated: false,
            include_top_level_license_data: false,
        },
    ));
    let expected: Value = serde_json::from_str(
        &fs::read_to_string(expected_file).expect("expected facet fixture should be readable"),
    )
    .expect("expected facet fixture should parse");
    let expected = project_facet_fields(&expected);

    let mut actual_normalized = actual;
    let mut expected_normalized = expected;
    normalize_scan_json(&mut actual_normalized, None);
    normalize_scan_json(&mut expected_normalized, None);

    if let Err(error) = compare_scan_json_values(&actual_normalized, &expected_normalized, "") {
        panic!(
            "Facet fixture mismatch for {} vs {}: {}\nactual={}\nexpected={}",
            fixture_dir,
            expected_file,
            error,
            serde_json::to_string_pretty(&actual_normalized).unwrap_or_default(),
            serde_json::to_string_pretty(&expected_normalized).unwrap_or_default()
        );
    }
}

#[cfg(feature = "golden-tests")]
pub(crate) fn assert_tally_fixture_matches_expected(
    fixture_dir: &str,
    expected_file: &str,
    options: FixtureOutputOptions<'_>,
) {
    let actual = project_tally_fields(&compute_fixture_output(fixture_dir, options));
    let expected: Value = serde_json::from_str(
        &fs::read_to_string(expected_file).expect("expected tally fixture should be readable"),
    )
    .expect("expected tally fixture should parse");
    let expected = project_tally_fields(&expected);

    let mut actual_normalized = actual;
    let mut expected_normalized = expected;
    normalize_scan_json(&mut actual_normalized, None);
    normalize_scan_json(&mut expected_normalized, None);

    if let Err(error) = compare_scan_json_values(&actual_normalized, &expected_normalized, "") {
        panic!(
            "Tally fixture mismatch for {} vs {}: {}\nactual={}\nexpected={}",
            fixture_dir,
            expected_file,
            error,
            serde_json::to_string_pretty(&actual_normalized).unwrap_or_default(),
            serde_json::to_string_pretty(&expected_normalized).unwrap_or_default()
        );
    }
}

#[cfg(feature = "golden-tests")]
pub(crate) fn assert_classify_fixture_matches_expected(
    fixture_dir: &str,
    expected_file: &str,
    normalize_against_parent: bool,
) {
    let fixture_root = Path::new(fixture_dir);
    let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
    let collected = collect_paths(fixture_root, 0, &fixture_exclude_patterns(fixture_root));
    let scan_result = process_collected(
        &collected,
        progress,
        Some(test_license_engine()),
        LicenseScanOptions::default(),
        &TextDetectionOptions {
            collect_info: true,
            detect_packages: true,
            detect_application_packages: true,
            detect_system_packages: true,
            detect_packages_in_compiled: false,
            ..TextDetectionOptions::default()
        },
    );

    let mut files = scan_result.files;
    let normalize_root = if normalize_against_parent {
        fixture_root.parent().expect("fixture should have parent")
    } else {
        fixture_root.parent().unwrap_or(fixture_root)
    };
    normalize_paths_for_test(
        &mut files,
        normalize_root
            .to_str()
            .expect("fixture path should be UTF-8"),
    );

    if normalize_against_parent {
        let dir_name = fixture_root
            .file_name()
            .and_then(|name| name.to_str())
            .expect("fixture dir should have utf-8 file name");
        if !files.iter().any(|file| file.path == dir_name) {
            files.push(dir(dir_name));
        }
    } else if let Some(dir_name) = fixture_root.file_name().and_then(|name| name.to_str())
        && !files.iter().any(|file| file.path == dir_name)
    {
        files.push(dir(dir_name));
    }

    let assembly_result = assembly::assemble(&mut files);
    classify_key_files(&mut files, &assembly_result.packages);

    let actual = project_classify_fields(&json!({ "files": files }));
    let expected: Value = serde_json::from_str(
        &fs::read_to_string(expected_file).expect("expected classify fixture should be readable"),
    )
    .expect("expected classify fixture should parse");
    let expected = project_classify_fields(&expected);

    let mut actual_normalized = actual;
    let mut expected_normalized = expected;
    normalize_scan_json(&mut actual_normalized, None);
    normalize_scan_json(&mut expected_normalized, None);

    if let Err(error) = compare_scan_json_values(&actual_normalized, &expected_normalized, "") {
        panic!(
            "Classify fixture mismatch for {} vs {}: {}\nactual={}\nexpected={}",
            fixture_dir,
            expected_file,
            error,
            serde_json::to_string_pretty(&actual_normalized).unwrap_or_default(),
            serde_json::to_string_pretty(&expected_normalized).unwrap_or_default()
        );
    }
}

#[cfg(feature = "golden-tests")]
pub(crate) fn assert_file_info_fixture_matches_expected(
    fixture_dir: &str,
    expected_file: &str,
    normalize_against_parent: bool,
) {
    let fixture_root = Path::new(fixture_dir);
    let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
    let collected = collect_paths(fixture_root, 0, &fixture_exclude_patterns(fixture_root));
    let scan_result = process_collected(
        &collected,
        progress,
        Some(test_license_engine()),
        LicenseScanOptions::default(),
        &TextDetectionOptions {
            collect_info: true,
            detect_packages: true,
            detect_application_packages: true,
            detect_system_packages: true,
            detect_packages_in_compiled: false,
            ..TextDetectionOptions::default()
        },
    );

    let mut files = scan_result.files;
    let normalize_root = if normalize_against_parent {
        fixture_root.parent().expect("fixture should have parent")
    } else {
        fixture_root.parent().unwrap_or(fixture_root)
    };
    normalize_paths_for_test(
        &mut files,
        normalize_root
            .to_str()
            .expect("fixture path should be UTF-8"),
    );

    if normalize_against_parent {
        let dir_name = fixture_root
            .file_name()
            .and_then(|name| name.to_str())
            .expect("fixture dir should have utf-8 file name");
        if !files.iter().any(|file| file.path == dir_name) {
            files.push(dir(dir_name));
        }
    } else if let Some(dir_name) = fixture_root.file_name().and_then(|name| name.to_str())
        && !files.iter().any(|file| file.path == dir_name)
    {
        files.push(dir(dir_name));
    }

    let assembly_result = assembly::assemble(&mut files);
    classify_key_files(&mut files, &assembly_result.packages);

    let actual = project_file_info_fields(&json!({ "files": files }));
    let expected: Value = serde_json::from_str(
        &fs::read_to_string(expected_file).expect("expected file-info fixture should be readable"),
    )
    .expect("expected file-info fixture should parse");
    let expected = project_file_info_fields(&expected);

    let mut actual_normalized = actual;
    let mut expected_normalized = expected;
    normalize_scan_json(&mut actual_normalized, None);
    normalize_scan_json(&mut expected_normalized, None);

    if let Err(error) = compare_scan_json_values(&actual_normalized, &expected_normalized, "") {
        panic!(
            "File-info fixture mismatch for {} vs {}: {}\nactual={}\nexpected={}",
            fixture_dir,
            expected_file,
            error,
            serde_json::to_string_pretty(&actual_normalized).unwrap_or_default(),
            serde_json::to_string_pretty(&expected_normalized).unwrap_or_default()
        );
    }
}

pub(crate) fn scan_and_assemble_with_keyfiles(
    path: &Path,
) -> (Vec<FileInfo>, assembly::AssemblyResult) {
    let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
    let collected = collect_paths(path, 0, &fixture_exclude_patterns(path));
    let result = process_collected(
        &collected,
        progress,
        None,
        LicenseScanOptions::default(),
        &TextDetectionOptions {
            collect_info: true,
            detect_packages: true,
            detect_application_packages: true,
            detect_system_packages: true,
            detect_packages_in_compiled: false,
            ..TextDetectionOptions::default()
        },
    );

    let mut files = result.files;
    let assembly_result = assembly::assemble(&mut files);
    classify_key_files(&mut files, &assembly_result.packages);
    (files, assembly_result)
}
