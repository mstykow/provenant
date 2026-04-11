use std::fs;
use std::process::Command;

use regex::Regex;
use serde_json::Value;
use tempfile::TempDir;

fn provenant_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_provenant"));
    command.current_dir(env!("CARGO_MANIFEST_DIR"));
    command
}

fn create_scan_fixture() -> (TempDir, String) {
    let temp = TempDir::new().expect("failed to create temp dir");
    let scan_dir = temp.path().join("scan");
    fs::create_dir_all(&scan_dir).expect("failed to create scan dir");
    fs::write(scan_dir.join("a.txt"), "hello cache@example.com\n")
        .expect("failed to write fixture file");
    (temp, scan_dir.to_string_lossy().to_string())
}

fn create_malformed_package_fixture() -> (TempDir, String) {
    let temp = TempDir::new().expect("failed to create temp dir");
    let scan_dir = temp.path().join("scan");
    fs::create_dir_all(&scan_dir).expect("failed to create scan dir");
    fs::write(scan_dir.join("package.json"), "{ this is not valid json }")
        .expect("failed to write malformed fixture");
    (temp, scan_dir.to_string_lossy().to_string())
}

fn create_ignore_fixture() -> (TempDir, String) {
    let temp = TempDir::new().expect("failed to create temp dir");
    let scan_dir = temp.path().join("scan");
    let build_dir = scan_dir.join("build");

    fs::create_dir_all(&build_dir).expect("failed to create build dir");
    fs::write(scan_dir.join("keep.txt"), "keep me\n").expect("failed to write keep.txt");
    fs::write(scan_dir.join("report.csv"), "col\n1\n").expect("failed to write report.csv");
    fs::write(build_dir.join("generated.txt"), "generated\n")
        .expect("failed to write generated.txt");

    (temp, scan_dir.to_string_lossy().to_string())
}

fn normalize_multi_parser_header(output: &mut Value) {
    let header = output["headers"]
        .as_array_mut()
        .and_then(|headers| headers.first_mut())
        .expect("headers[0] should exist");

    header["tool_version"] = Value::String("<tool_version>".to_string());
    header["start_timestamp"] = Value::String("<start_timestamp>".to_string());
    header["end_timestamp"] = Value::String("<end_timestamp>".to_string());
    header["duration"] = Value::String("<duration>".to_string());
    header["options"]["--json-pp"] = Value::String("<output_file>".to_string());
    header["extra_data"]["system_environment"]["operating_system"] =
        Value::String("<operating_system>".to_string());
    header["extra_data"]["system_environment"]["cpu_architecture"] =
        Value::String("<cpu_architecture>".to_string());
    header["extra_data"]["system_environment"]["platform"] =
        Value::String("<platform>".to_string());
    header["extra_data"]["system_environment"]["platform_version"] =
        Value::String("<platform_version>".to_string());
    header["extra_data"]["system_environment"]["rust_version"] =
        Value::String("<rust_version>".to_string());
}

#[test]
fn quiet_mode_suppresses_stderr_output() {
    let (temp, scan_dir) = create_scan_fixture();
    let output_file = temp.path().join("out.json");

    let output = provenant_command()
        .args([
            "--json-pp",
            output_file.to_str().expect("utf8 output path"),
            "--quiet",
            &scan_dir,
        ])
        .output()
        .expect("failed to run provenant");

    assert!(output.status.success());
    assert!(
        output.stderr.is_empty(),
        "quiet mode should not emit stderr"
    );
}

#[test]
fn default_mode_emits_summary_to_stderr() {
    let (temp, scan_dir) = create_scan_fixture();
    let output_file = temp.path().join("out.json");

    let output = provenant_command()
        .args([
            "--json-pp",
            output_file.to_str().expect("utf8 output path"),
            "--package",
            &scan_dir,
        ])
        .output()
        .expect("failed to run provenant");

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Scanning 1 file..."));
    assert!(stderr.contains("Scan complete."));
    assert!(stderr.contains("Summary:"));
    assert!(!stderr.contains("Scanning done."));

    let scan_timestamp_re = Regex::new(r"scan_(start|end):\s+\d{4}-\d{2}-\d{2}T\d{6}\.\d{6}")
        .expect("timestamp regex should compile");
    let matches = scan_timestamp_re.find_iter(&stderr).count();
    assert_eq!(matches, 2, "summary should emit ScanCode-style timestamps");
}

#[test]
fn default_mode_emits_hierarchical_timing_summary() {
    let (temp, scan_dir) = create_scan_fixture();
    let output_file = temp.path().join("out.json");

    let output = provenant_command()
        .args([
            "--json-pp",
            output_file.to_str().expect("utf8 output path"),
            "--only-findings",
            "--package",
            &scan_dir,
        ])
        .output()
        .expect("failed to run provenant");

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Timings:"));
    assert!(stderr.contains("  setup:"));
    assert!(stderr.contains("  inventory:"));
    assert!(stderr.contains("  scan:"));
    assert!(stderr.contains("  post-scan:"));
    assert!(stderr.contains("  finalize:"));
    assert!(stderr.contains("  output:"));
    assert!(stderr.contains("  total:"));
    assert!(stderr.contains("    scan:packages:"));
    assert!(stderr.contains("    output-filter:only-findings:"));
}

#[test]
fn verbose_mode_emits_file_by_file_paths() {
    let (temp, scan_dir) = create_scan_fixture();
    let output_file = temp.path().join("out.json");

    let output = provenant_command()
        .args([
            "--json-pp",
            output_file.to_str().expect("utf8 output path"),
            "--verbose",
            "--package",
            &scan_dir,
        ])
        .output()
        .expect("failed to run provenant");

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("a.txt"));
}

#[test]
fn default_mode_keeps_parser_failures_concise_on_stderr() {
    let (temp, scan_dir) = create_malformed_package_fixture();
    let output_file = temp.path().join("out.json");

    let output = provenant_command()
        .args([
            "--json-pp",
            output_file.to_str().expect("utf8 output path"),
            "--package",
            &scan_dir,
        ])
        .output()
        .expect("failed to run provenant");

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Failed to read or parse package.json:"),
        "default mode should report a concise failure reason"
    );
    assert!(
        stderr.contains("package.json"),
        "default mode should report the failing path"
    );
    assert!(
        !stderr.contains("key must be a string at line 1 column 3"),
        "default mode should avoid duplicating parser failure details"
    );
}

#[test]
fn verbose_mode_includes_structured_parser_failure_details() {
    let (temp, scan_dir) = create_malformed_package_fixture();
    let output_file = temp.path().join("out.json");

    let output = provenant_command()
        .args([
            "--json-pp",
            output_file.to_str().expect("utf8 output path"),
            "--verbose",
            "--package",
            &scan_dir,
        ])
        .output()
        .expect("failed to run provenant");

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("package.json"));
    assert!(
        stderr.contains("Failed to read or parse package.json"),
        "verbose mode should include structured parser failure details"
    );
}

#[test]
fn incremental_mode_reuses_unchanged_files_and_keeps_them_in_output() {
    let (temp, scan_dir) = create_scan_fixture();
    let cache_dir = temp.path().join("shared-cache");
    let first_output = temp.path().join("first.json");
    let second_output = temp.path().join("second.json");

    let first = provenant_command()
        .args([
            "--json-pp",
            first_output.to_str().expect("utf8 output path"),
            "--cache-dir",
            cache_dir.to_str().expect("utf8 cache path"),
            "--incremental",
            "--email",
            &scan_dir,
        ])
        .output()
        .expect("failed to run first incremental scan");
    assert!(first.status.success());

    let second = provenant_command()
        .args([
            "--json-pp",
            second_output.to_str().expect("utf8 output path"),
            "--cache-dir",
            cache_dir.to_str().expect("utf8 cache path"),
            "--incremental",
            "--email",
            &scan_dir,
        ])
        .output()
        .expect("failed to run second incremental scan");
    assert!(second.status.success());

    let stderr = String::from_utf8_lossy(&second.stderr);
    assert!(stderr.contains("Incremental:"), "stderr was: {stderr}");
    assert!(
        stderr.contains("1 unchanged file(s) reused"),
        "stderr was: {stderr}"
    );

    let output_json: Value = serde_json::from_slice(
        &fs::read(&second_output).expect("failed to read second incremental output"),
    )
    .expect("failed to parse second incremental output");
    let files = output_json["files"]
        .as_array()
        .expect("files should be an array");
    assert!(files.iter().any(|file| {
        file["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("a.txt"))
    }));
}

#[test]
fn ignore_build_glob_excludes_build_subtree_from_cli_output() {
    let (temp, scan_dir) = create_ignore_fixture();
    let output_file = temp.path().join("out.json");

    let output = provenant_command()
        .args([
            "--json-pp",
            output_file.to_str().expect("utf8 output path"),
            "--ignore",
            "build/*",
            &scan_dir,
        ])
        .output()
        .expect("failed to run provenant");

    assert!(output.status.success());
    let output_json: Value =
        serde_json::from_slice(&fs::read(&output_file).expect("failed to read output json"))
            .expect("output json should parse");
    let files = output_json["files"]
        .as_array()
        .expect("files should be an array");
    let paths: Vec<&str> = files
        .iter()
        .filter_map(|file| file["path"].as_str())
        .collect();

    assert!(
        paths
            .iter()
            .any(|path| path.ends_with("/keep.txt") || *path == "keep.txt"),
        "paths: {paths:#?}"
    );
    assert!(
        paths
            .iter()
            .any(|path| path.ends_with("/build") || *path == "build"),
        "paths: {paths:#?}"
    );
    assert!(
        !paths
            .iter()
            .any(|path| path.ends_with("/build/generated.txt") || *path == "build/generated.txt"),
        "build descendants should be excluded: {paths:#?}"
    );
}

#[test]
fn ignore_root_csv_glob_excludes_root_csv_from_cli_output() {
    let (temp, scan_dir) = create_ignore_fixture();
    let output_file = temp.path().join("out.json");

    let output = provenant_command()
        .args([
            "--json-pp",
            output_file.to_str().expect("utf8 output path"),
            "--ignore",
            "*.csv",
            &scan_dir,
        ])
        .output()
        .expect("failed to run provenant");

    assert!(output.status.success());
    let output_json: Value =
        serde_json::from_slice(&fs::read(&output_file).expect("failed to read output json"))
            .expect("output json should parse");
    let files = output_json["files"]
        .as_array()
        .expect("files should be an array");
    let paths: Vec<&str> = files
        .iter()
        .filter_map(|file| file["path"].as_str())
        .collect();

    assert!(
        paths
            .iter()
            .any(|path| path.ends_with("/keep.txt") || *path == "keep.txt"),
        "paths: {paths:#?}"
    );
    assert!(
        !paths
            .iter()
            .any(|path| path.ends_with("/report.csv") || *path == "report.csv"),
        "root csv should be excluded: {paths:#?}"
    );
}

#[test]
fn multi_parser_expected_header_fixture_matches_cli_output() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let output_file = temp.path().join("multi-parser.json");

    let output = provenant_command()
        .args([
            "--json-pp",
            output_file.to_str().expect("utf8 output path"),
            "--package",
            "--info",
            "testdata/integration/multi-parser",
        ])
        .output()
        .expect("failed to run provenant");

    assert!(output.status.success());

    let mut actual: Value =
        serde_json::from_slice(&fs::read(&output_file).expect("failed to read output json"))
            .expect("output json should parse");
    let mut expected: Value = serde_json::from_str(
        &fs::read_to_string("testdata/integration/multi-parser.expected.json")
            .expect("failed to read expected fixture"),
    )
    .expect("expected fixture should parse");

    normalize_multi_parser_header(&mut actual);
    normalize_multi_parser_header(&mut expected);

    assert_eq!(actual["headers"], expected["headers"]);
}
