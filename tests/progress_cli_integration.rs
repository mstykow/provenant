use std::fs;
use std::process::Command;

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
    fs::write(scan_dir.join("a.txt"), "hello world\n").expect("failed to write fixture file");
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
    assert!(stderr.contains("Scanning done."));
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
        stderr.contains("Path:"),
        "default mode should report the failing path"
    );
    assert!(
        !stderr.contains("Failed to read or parse package.json"),
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
