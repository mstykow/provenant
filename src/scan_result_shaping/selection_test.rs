// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::fs;
use std::path::PathBuf;

#[test]
fn is_included_path_requires_include_match_before_excludes() {
    assert!(is_included_path(
        "user/src/test/sample.doc",
        &["*.doc".to_string()],
        &[]
    ));
    assert!(!is_included_path(
        "user/src/test/sample.txt",
        &["*.doc".to_string()],
        &[]
    ));
}

#[test]
fn is_included_path_applies_exclude_after_include() {
    assert!(!is_included_path(
        "src/dist/build/mylib.so",
        &["/src/*".to_string()],
        &["/src/*.so".to_string()]
    ));
    assert!(is_included_path(
        "some/src/this/that",
        &["src".to_string()],
        &["src/*.so".to_string()]
    ));
}

#[test]
fn apply_user_path_filters_to_collected_filters_files_without_pruning_directories() {
    let scan_root = PathBuf::from("/scan");
    let placeholder_metadata = fs::metadata(std::env::temp_dir()).expect("temp dir metadata");
    let mut collected = crate::scanner::CollectedPaths {
        files: vec![
            (
                scan_root.join("src/test/sample.doc"),
                placeholder_metadata.clone(),
            ),
            (
                scan_root.join("src/test/sample.txt"),
                placeholder_metadata.clone(),
            ),
        ],
        directories: vec![
            (scan_root.clone(), placeholder_metadata.clone()),
            (scan_root.join("src"), placeholder_metadata.clone()),
            (scan_root.join("src/test"), placeholder_metadata.clone()),
            (scan_root.join("other"), placeholder_metadata.clone()),
        ],
        excluded_count: 0,
        total_file_bytes: 0,
        collection_errors: Vec::new(),
    };

    let removed = apply_user_path_filters_to_collected(
        &mut collected,
        &scan_root,
        &["*.doc".to_string()],
        &[],
    );

    assert_eq!(removed, 2);
    assert_eq!(collected.files.len(), 1);
    let kept_dirs: Vec<_> = collected
        .directories
        .iter()
        .map(|(path, _)| normalize_scan_relative_path(path, &scan_root))
        .collect();
    assert_eq!(
        kept_dirs,
        vec!["".to_string(), "src".to_string(), "src/test".to_string()]
    );
    assert_eq!(
        normalize_scan_relative_path(&collected.files[0].0, &scan_root),
        "src/test/sample.doc"
    );
}

#[test]
fn normalize_scan_relative_path_uses_filename_for_single_file_root() {
    let scan_root = PathBuf::from("/scan/d2s.ipp");

    assert_eq!(
        normalize_scan_relative_path(&scan_root, &scan_root),
        "d2s.ipp"
    );
}

#[test]
fn apply_user_path_filters_to_collected_keeps_single_file_root_input() {
    let scan_root = PathBuf::from("/scan/d2s.ipp");
    let placeholder_metadata = fs::metadata(std::env::temp_dir()).expect("temp dir metadata");
    let mut collected = crate::scanner::CollectedPaths {
        files: vec![(scan_root.clone(), placeholder_metadata)],
        directories: Vec::new(),
        excluded_count: 0,
        total_file_bytes: 0,
        collection_errors: Vec::new(),
    };

    let removed = apply_user_path_filters_to_collected(&mut collected, &scan_root, &[], &[]);

    assert_eq!(removed, 0);
    assert_eq!(collected.files.len(), 1);
    assert_eq!(
        normalize_scan_relative_path(&collected.files[0].0, &scan_root),
        "d2s.ipp"
    );
}

#[test]
fn is_included_path_treats_directory_include_patterns_recursively() {
    assert!(is_included_path(
        "src/foo/bar/baz.txt",
        &["src/foo".to_string()],
        &[]
    ));
    assert!(!is_included_path(
        "src/other/bar.txt",
        &["src/foo".to_string()],
        &[]
    ));
}

#[test]
fn resolve_native_scan_inputs_builds_common_prefix_and_synthetic_includes() {
    let (scan_root, includes) =
        resolve_native_scan_inputs(&["src/foo".to_string(), "src/bar/baz".to_string()])
            .expect("multiple relative inputs should resolve");

    assert_eq!(scan_root, "src");
    assert_eq!(includes, vec!["src/foo", "src/bar/baz"]);
}

#[test]
fn resolve_native_scan_inputs_uses_component_aware_prefix_for_siblings() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let parent = temp_dir.path().join("src");
    fs::create_dir_all(parent.join("bar")).expect("create bar dir");
    fs::create_dir_all(parent.join("baz")).expect("create baz dir");

    let old_cwd = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(temp_dir.path()).expect("set cwd");

    let result = resolve_native_scan_inputs(&["src/bar".to_string(), "src/baz".to_string()]);

    std::env::set_current_dir(old_cwd).expect("restore cwd");

    let (scan_root, includes) = result.expect("sibling inputs should resolve");
    assert_eq!(scan_root, "src");
    assert_eq!(includes, vec!["src/bar", "src/baz"]);
}

#[test]
fn resolve_paths_file_entries_normalizes_existing_entries_and_tracks_missing() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let scan_root = temp_dir.path().join("repo");
    fs::create_dir_all(scan_root.join("src/nested")).expect("create nested source dir");
    fs::create_dir_all(scan_root.join("docs")).expect("create docs dir");
    fs::write(scan_root.join("src/nested/main.rs"), "fn main() {}\n").expect("write source");

    let resolved = resolve_paths_file_entries(
        &scan_root,
        &[
            "./src/nested/../nested/main.rs".to_string(),
            "docs\r".to_string(),
            "src/nested/main.rs".to_string(),
            "missing/file.rs".to_string(),
            "  ".to_string(),
        ],
    )
    .expect("paths file entries should resolve");

    assert_eq!(resolved.includes, vec!["src/nested/main.rs", "docs"]);
    assert_eq!(resolved.missing_entries, vec!["missing/file.rs"]);
}

#[test]
fn resolve_paths_file_entries_rejects_entries_that_escape_root() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let scan_root = temp_dir.path().join("repo");
    fs::create_dir_all(&scan_root).expect("create scan root");

    let error = resolve_paths_file_entries(&scan_root, &["../secret.txt".to_string()])
        .expect_err("escaping entry should be rejected");

    assert!(error.to_string().contains("escapes the declared scan root"));
}

#[test]
fn resolve_paths_file_entries_uses_explicit_root_not_current_working_directory() {
    let scan_root_parent = tempfile::tempdir().expect("scan root parent");
    let other_cwd = tempfile::tempdir().expect("alternate cwd");
    let scan_root = scan_root_parent.path().join("repo");
    fs::create_dir_all(scan_root.join("src")).expect("create src dir");
    fs::write(scan_root.join("src/lib.rs"), "pub fn demo() {}\n").expect("write lib");

    let old_cwd = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(other_cwd.path()).expect("set cwd");

    let result = resolve_paths_file_entries(&scan_root, &["src/lib.rs".to_string()]);

    std::env::set_current_dir(old_cwd).expect("restore cwd");

    let resolved = result.expect("absolute scan root should make cwd irrelevant");
    assert_eq!(resolved.includes, vec!["src/lib.rs"]);
    assert!(resolved.missing_entries.is_empty());
}
