// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::is_go_non_production_source;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_is_go_non_production_source_for_test_filename() {
    let temp_dir = tempdir().unwrap();
    let path = temp_dir.path().join("scanner_test.go");
    fs::write(&path, "package scanner\n").unwrap();

    assert!(is_go_non_production_source(&path).unwrap());
}

#[test]
fn test_is_go_non_production_source_for_build_tag() {
    let temp_dir = tempdir().unwrap();
    let path = temp_dir.path().join("scanner.go");
    fs::write(&path, "//go:build test\n\npackage scanner\n").unwrap();

    assert!(is_go_non_production_source(&path).unwrap());
}

#[test]
fn test_is_go_non_production_source_for_regular_go_file() {
    let temp_dir = tempdir().unwrap();
    let path = temp_dir.path().join("scanner.go");
    fs::write(&path, "package scanner\n").unwrap();

    assert!(!is_go_non_production_source(&path).unwrap());
}
