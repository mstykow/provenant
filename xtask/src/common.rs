// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::ValueEnum;
use regex::Regex;
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ScanProfile {
    Common,
    CommonWithCompiled,
    Licenses,
    Packages,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetSource {
    TargetPath,
    RepoUrl,
}

impl TargetSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::TargetPath => "Target path",
            Self::RepoUrl => "Repo URL",
        }
    }

    pub fn retains_checkout(self) -> bool {
        matches!(self, Self::TargetPath)
    }
}

impl ScanProfile {
    pub fn args(self) -> &'static [&'static str] {
        match self {
            Self::Common => &[
                "-clupe",
                "--system-package",
                "--strip-root",
                "--processes",
                "4",
            ],
            Self::CommonWithCompiled => &[
                "-clupe",
                "--system-package",
                "--package-in-compiled",
                "--strip-root",
            ],
            Self::Licenses => &["-l", "--strip-root"],
            Self::Packages => &["-p", "--strip-root"],
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Common => "common",
            Self::CommonWithCompiled => "common-with-compiled",
            Self::Licenses => "licenses",
            Self::Packages => "packages",
        }
    }
}

pub fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".."))
}

pub fn resolve_scan_args(
    profile: Option<ScanProfile>,
    scan_args: Vec<String>,
    empty_args_message: &str,
) -> Result<Vec<String>> {
    if profile.is_some() && !scan_args.is_empty() {
        anyhow::bail!("use either --profile or explicit scan flags after --, not both");
    }
    if let Some(profile) = profile {
        return Ok(profile
            .args()
            .iter()
            .map(|value| (*value).to_string())
            .collect());
    }
    if scan_args.is_empty() {
        anyhow::bail!(empty_args_message.to_string());
    }
    Ok(scan_args)
}

pub fn realpath(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .with_context(|| format!("failed to resolve path: {}", path.display()))
}

pub fn now_run_id(slug: &str) -> String {
    let now = chrono_like_timestamp();
    format!("{now}-{slug}-{}", std::process::id())
}

fn chrono_like_timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before UNIX_EPOCH")
        .as_secs();
    let datetime = time_from_unix(now);
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        datetime.year,
        datetime.month,
        datetime.day,
        datetime.hour,
        datetime.minute,
        datetime.second
    )
}

struct DateTimeParts {
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
}

fn time_from_unix(timestamp: u64) -> DateTimeParts {
    let days = (timestamp / 86_400) as i64;
    let secs_of_day = (timestamp % 86_400) as u32;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };
    DateTimeParts {
        year: year as i32,
        month: month as u32,
        day: day as u32,
        hour: secs_of_day / 3600,
        minute: (secs_of_day % 3600) / 60,
        second: secs_of_day % 60,
    }
}

pub fn sanitize_label(value: &str, fallback: &str) -> String {
    let regex = Regex::new(r"[^A-Za-z0-9._-]+$").unwrap();
    let invalid = Regex::new(r"[^A-Za-z0-9._-]+").unwrap();
    let trimmed = value.trim();
    let replaced = invalid.replace_all(trimmed, "-");
    let normalized = regex.replace_all(&replaced, "");
    let normalized = normalized.trim_matches(|c| c == '-' || c == '.' || c == '_');
    if normalized.is_empty() {
        fallback.to_string()
    } else {
        normalized.chars().take(80).collect()
    }
}

pub fn shell_join(args: &[String]) -> String {
    args.iter()
        .map(|arg| shell_escape(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_escape(arg: &str) -> String {
    if arg.is_empty() {
        return "''".to_string();
    }
    if arg.bytes().all(|b| matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b':' | b'.' | b'_' | b'-' | b'=')) {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

pub fn derive_repo_name_from_url(repo_url: &str, fallback: &str) -> String {
    let name = repo_url
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .trim_end_matches(".git");
    if name.is_empty() {
        fallback.to_string()
    } else {
        name.to_string()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitWorktreeIdentity {
    pub revision: Option<String>,
    pub dirty: bool,
    pub diff_hash: Option<String>,
}

pub fn resolve_git_worktree_identity(worktree_root: &Path) -> Result<GitWorktreeIdentity> {
    let revision = crate::repo_cache::current_git_revision(worktree_root);
    let status = Command::new("git")
        .current_dir(worktree_root)
        .args(["status", "--short", "--untracked-files=no"])
        .output()
        .with_context(|| format!("failed to inspect git worktree {}", worktree_root.display()))?;
    if !status.status.success() {
        anyhow::bail!(
            "git status failed for {}: {}",
            worktree_root.display(),
            String::from_utf8_lossy(&status.stderr).trim()
        );
    }

    let dirty = !String::from_utf8_lossy(&status.stdout).trim().is_empty();
    let diff_hash = if dirty {
        let diff = Command::new("git")
            .current_dir(worktree_root)
            .args(["diff", "--no-ext-diff", "--binary", "HEAD"])
            .output()
            .with_context(|| format!("failed to diff git worktree {}", worktree_root.display()))?;
        if !diff.status.success() {
            anyhow::bail!(
                "git diff failed for {}: {}",
                worktree_root.display(),
                String::from_utf8_lossy(&diff.stderr).trim()
            );
        }
        let mut hasher = Sha256::new();
        hasher.update(&diff.stdout);
        Some(
            hasher
                .finalize()
                .iter()
                .map(|byte| format!("{:02x}", byte))
                .collect(),
        )
    } else {
        None
    };

    Ok(GitWorktreeIdentity {
        revision,
        dirty,
        diff_hash,
    })
}

pub fn read_binary_version(binary: &Path) -> Result<String> {
    let output = Command::new(binary)
        .arg("-V")
        .output()
        .with_context(|| format!("failed to read binary version from {}", binary.display()))?;
    if !output.status.success() {
        anyhow::bail!(
            "{} -V failed: {}",
            binary.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = stdout
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .last()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("unable to parse version from {} -V", binary.display()))?;
    Ok(version.to_string())
}

pub fn ensure_release_binary(project_root: &Path, binary: &Path, label: &str) -> Result<()> {
    let output = Command::new("cargo")
        .current_dir(project_root)
        .args(["build", "--release"])
        .output()
        .with_context(|| format!("failed to build {label}"))?;
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for line in combined.lines().filter(|line| {
        line.contains("Compiling") || line.contains("Finished") || line.contains("error")
    }) {
        println!("  {line}");
    }
    if !output.status.success() {
        anyhow::bail!("cargo build --release failed for {label}");
    }
    if !binary.is_file() {
        anyhow::bail!("{label} binary not found at {}", binary.display());
    }
    Ok(())
}

pub fn run_and_capture(
    program: &str,
    args: &[String],
    cwd: Option<&Path>,
    log_path: &Path,
) -> Result<String> {
    let mut command = Command::new(program);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command
        .args(args)
        .output()
        .with_context(|| format!("failed to execute {program}"))?;
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    fs::write(log_path, &combined)
        .with_context(|| format!("failed to write command log {}", log_path.display()))?;
    if !output.status.success() {
        anyhow::bail!(
            "command failed: {}",
            shell_join(
                &std::iter::once(program.to_string())
                    .chain(args.iter().cloned())
                    .collect::<Vec<_>>()
            )
        );
    }
    Ok(combined)
}

pub fn write_pretty_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    fs::write(path, serde_json::to_string_pretty(value)? + "\n")
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub fn append_tsv_row(path: &Path, values: &[String]) -> Result<()> {
    use std::io::Write;

    let mut line = values.join("\t");
    line.push('\n');
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {} for append", path.display()))?;
    file.write_all(line.as_bytes())
        .with_context(|| format!("failed to append to {}", path.display()))?;
    Ok(())
}

pub fn write_tsv(path: &Path, header: &[&str], rows: &[Vec<String>]) -> Result<()> {
    let mut content = String::new();
    content.push_str(&header.join("\t"));
    content.push('\n');
    for row in rows {
        content.push_str(&row.join("\t"));
        content.push('\n');
    }
    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub fn render_tsv_table(path: &Path, display_headers: &[&str]) -> Result<Vec<Vec<String>>> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut lines = content.lines();
    let Some(header_line) = lines.next() else {
        return Ok(Vec::new());
    };
    let header_count = header_line.split('\t').count();
    let rows: Vec<Vec<String>> = lines
        .map(|line| {
            line.split('\t')
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
        })
        .filter(|row| row.len() == header_count)
        .collect();

    let mut widths: Vec<usize> = display_headers.iter().map(|label| label.len()).collect();
    for row in &rows {
        for (idx, value) in row.iter().enumerate() {
            widths[idx] = widths[idx].max(value.len());
        }
    }

    let format_row = |values: &[String]| -> String {
        values
            .iter()
            .enumerate()
            .map(|(idx, value)| format!("{value:<width$}", width = widths[idx]))
            .collect::<Vec<_>>()
            .join(" | ")
    };

    println!(
        "{}",
        format_row(
            &display_headers
                .iter()
                .map(|value| (*value).to_string())
                .collect::<Vec<_>>()
        )
    );
    println!(
        "{}",
        widths
            .iter()
            .map(|width| "-".repeat(*width))
            .collect::<Vec<_>>()
            .join("-+-")
    );
    for row in &rows {
        println!("{}", format_row(row));
    }

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    use tempfile::TempDir;

    use super::{
        ScanProfile, read_binary_version, resolve_git_worktree_identity, resolve_scan_args,
    };

    fn git(dir: &Path, args: &[&str]) {
        let output = Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn init_git_repo() -> TempDir {
        let temp = TempDir::new().unwrap();
        git(temp.path(), &["init"]);
        git(temp.path(), &["config", "user.name", "Test User"]);
        git(temp.path(), &["config", "user.email", "test@example.com"]);
        fs::write(temp.path().join("tracked.txt"), "hello\n").unwrap();
        git(temp.path(), &["add", "tracked.txt"]);
        git(temp.path(), &["commit", "-m", "init"]);
        temp
    }

    #[test]
    fn common_profile_expands_to_expected_args() {
        assert_eq!(
            ScanProfile::Common.args(),
            [
                "-clupe",
                "--system-package",
                "--strip-root",
                "--processes",
                "4",
            ]
        );
        assert_eq!(ScanProfile::Common.display_name(), "common");
    }

    #[test]
    fn common_with_compiled_profile_expands_to_expected_args() {
        assert_eq!(
            ScanProfile::CommonWithCompiled.args(),
            [
                "-clupe",
                "--system-package",
                "--package-in-compiled",
                "--strip-root",
            ]
        );
        assert_eq!(
            ScanProfile::CommonWithCompiled.display_name(),
            "common-with-compiled"
        );
    }

    #[test]
    fn resolve_scan_args_uses_common_profile() {
        let resolved = resolve_scan_args(Some(ScanProfile::Common), Vec::new(), "unused")
            .expect("profile should resolve");

        assert_eq!(
            resolved,
            vec![
                "-clupe".to_string(),
                "--system-package".to_string(),
                "--strip-root".to_string(),
                "--processes".to_string(),
                "4".to_string(),
            ]
        );
    }

    #[test]
    fn resolve_scan_args_uses_common_with_compiled_profile() {
        let resolved =
            resolve_scan_args(Some(ScanProfile::CommonWithCompiled), Vec::new(), "unused")
                .expect("profile should resolve");

        assert_eq!(
            resolved,
            vec![
                "-clupe".to_string(),
                "--system-package".to_string(),
                "--package-in-compiled".to_string(),
                "--strip-root".to_string(),
            ]
        );
    }

    #[test]
    fn resolve_git_worktree_identity_reports_clean_repo() {
        let temp = init_git_repo();

        let identity = resolve_git_worktree_identity(temp.path()).unwrap();

        assert!(identity.revision.is_some());
        assert!(!identity.dirty);
        assert_eq!(identity.diff_hash, None);
    }

    #[test]
    fn resolve_git_worktree_identity_reports_dirty_repo() {
        let temp = init_git_repo();
        fs::write(temp.path().join("tracked.txt"), "changed\n").unwrap();

        let identity = resolve_git_worktree_identity(temp.path()).unwrap();

        assert!(identity.revision.is_some());
        assert!(identity.dirty);
        assert!(identity.diff_hash.is_some());
    }

    #[test]
    fn read_binary_version_parses_last_token() {
        let temp = TempDir::new().unwrap();
        let script = temp.path().join("fake-provenant");
        fs::write(
            &script,
            "#!/bin/sh\nif [ \"$1\" = \"-V\" ]; then\n  printf 'provenant 1.2.3\\n'\nelse\n  printf 'provenant 1.2.3\\nLicense detection uses data from ScanCode Toolkit.\\n'\nfi",
        )
        .unwrap();
        let mut perms = fs::metadata(&script).unwrap().permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(0o755);
            fs::set_permissions(&script, perms).unwrap();
        }

        assert_eq!(read_binary_version(&script).unwrap(), "1.2.3");
    }

    #[test]
    fn read_binary_version_uses_short_version_line() {
        let temp = TempDir::new().unwrap();
        let script = temp.path().join("fake-provenant");
        fs::write(
            &script,
            "#!/bin/sh\nif [ \"$1\" = \"-V\" ]; then\n  printf 'provenant-cli 0.0.13\\n'\nelse\n  printf 'provenant-cli 0.0.13\\nLicense detection uses data from ScanCode Toolkit (CC-BY-4.0). See NOTICE or --show_attribution.\\n'\nfi",
        )
        .unwrap();
        let mut perms = fs::metadata(&script).unwrap().permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(0o755);
            fs::set_permissions(&script, perms).unwrap();
        }

        assert_eq!(read_binary_version(&script).unwrap(), "0.0.13");
    }
}
