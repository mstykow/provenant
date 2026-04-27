// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::common::{derive_repo_name_from_url, sanitize_label};

const SHALLOW_FETCH_DEPTH: &str = "1";

pub fn repo_cache_root(project_root: &Path) -> PathBuf {
    project_root.join(".provenant/repo-cache")
}

pub fn repo_cache_path(project_root: &Path, repo_url: &str) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(repo_url.as_bytes());
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect();
    repo_cache_root(project_root).join(format!(
        "{}-{}.git",
        sanitize_label(&derive_repo_name_from_url(repo_url, "repo"), "repo"),
        &digest[..12]
    ))
}

pub fn ensure_repo_mirror(repo_url: &str, repo_ref: &str, cache_dir: &Path) -> Result<()> {
    if let Some(parent) = cache_dir.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create repo-cache root: {}", parent.display()))?;
    }
    if !cache_dir.exists() {
        run_git(
            Command::new("git").args(["init", "--bare"]).arg(cache_dir),
            "failed to initialize repo cache",
        )?;
    }
    ensure_repo_remote(cache_dir, repo_url)?;
    fetch_repo_ref_shallow(cache_dir, repo_ref)
}

pub fn resolve_repo_ref_to_sha(cache_dir: &Path, repo_ref: &str) -> Result<String> {
    if let Some(sha) = resolve_repo_ref_locally(cache_dir, repo_ref) {
        return Ok(sha);
    }

    bail!(
        "unable to resolve repo ref '{repo_ref}' in cache {}",
        cache_dir.display()
    )
}

fn ensure_repo_remote(cache_dir: &Path, repo_url: &str) -> Result<()> {
    let git_dir_arg = format!("--git-dir={}", cache_dir.display());
    let set_url_output = Command::new("git")
        .arg(&git_dir_arg)
        .args(["remote", "set-url", "origin", repo_url])
        .output()
        .context("failed to configure repo-cache remote")?;
    if set_url_output.status.success() {
        return Ok(());
    }

    let add_origin_output = Command::new("git")
        .arg(&git_dir_arg)
        .args(["remote", "add", "origin", repo_url])
        .output()
        .context("failed to add repo-cache remote")?;
    if add_origin_output.status.success() {
        return Ok(());
    }

    bail!(
        "failed to configure repo-cache remote: {} ; {}",
        String::from_utf8_lossy(&set_url_output.stderr).trim(),
        String::from_utf8_lossy(&add_origin_output.stderr).trim()
    )
}

fn fetch_repo_ref_shallow(cache_dir: &Path, repo_ref: &str) -> Result<()> {
    run_git(
        Command::new("git")
            .arg(format!("--git-dir={}", cache_dir.display()))
            .args([
                "fetch",
                "--prune",
                "--depth",
                SHALLOW_FETCH_DEPTH,
                "origin",
                repo_ref,
            ]),
        "failed to fetch requested ref shallowly",
    )
}

fn resolve_repo_ref_locally(cache_dir: &Path, repo_ref: &str) -> Option<String> {
    let candidates = [
        format!("{repo_ref}^{{commit}}"),
        format!("refs/heads/{repo_ref}^{{commit}}"),
        format!("refs/tags/{repo_ref}^{{commit}}"),
        format!("refs/remotes/origin/{repo_ref}^{{commit}}"),
        "FETCH_HEAD^{commit}".to_string(),
    ];

    candidates.iter().find_map(|candidate| {
        git_output(cache_dir, ["rev-parse", "--verify", candidate.as_str()])
            .ok()
            .map(|sha| sha.trim().to_string())
    })
}

pub fn prepare_repo_worktree(
    cache_dir: &Path,
    resolved_sha: &str,
    worktree_dir: &Path,
) -> Result<()> {
    if let Some(parent) = worktree_dir.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create worktree parent: {}", parent.display()))?;
    }
    cleanup_repo_worktree(cache_dir, worktree_dir)?;
    run_git(
        Command::new("git")
            .arg(format!("--git-dir={}", cache_dir.display()))
            .args(["worktree", "add", "--detach"])
            .arg(worktree_dir)
            .arg(resolved_sha),
        "failed to add worktree",
    )
}

pub fn cleanup_repo_worktree(cache_dir: &Path, worktree_dir: &Path) -> Result<()> {
    let _ = run_git(
        Command::new("git")
            .arg(format!("--git-dir={}", cache_dir.display()))
            .args(["worktree", "remove", "--force"])
            .arg(worktree_dir),
        "failed to remove worktree",
    );
    if worktree_dir.exists() {
        fs::remove_dir_all(worktree_dir).with_context(|| {
            format!("failed to remove worktree dir: {}", worktree_dir.display())
        })?;
    }
    let _ = run_git(
        Command::new("git")
            .arg(format!("--git-dir={}", cache_dir.display()))
            .args(["worktree", "prune"]),
        "failed to prune worktrees",
    );
    Ok(())
}

pub fn current_git_revision(path: &Path) -> Option<String> {
    current_git_repo_root(path)?;
    let output = Command::new("git")
        .current_dir(path)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

pub fn current_git_log_line(path: &Path) -> Option<String> {
    current_git_repo_root(path)?;
    let output = Command::new("git")
        .current_dir(path)
        .args(["log", "-1", "--oneline"])
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

fn current_git_repo_root(path: &Path) -> Option<PathBuf> {
    let canonical_path = path.canonicalize().ok()?;
    let output = Command::new("git")
        .current_dir(&canonical_path)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let repo_root = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    let canonical_repo_root = repo_root.canonicalize().ok()?;
    (canonical_repo_root == canonical_path).then_some(canonical_repo_root)
}

fn git_output<const N: usize>(dir_or_gitdir: &Path, args: [&str; N]) -> Result<String> {
    let output = if args.first() == Some(&"rev-parse") {
        Command::new("git")
            .arg(format!("--git-dir={}", dir_or_gitdir.display()))
            .args(args)
            .output()
    } else {
        Command::new("git")
            .current_dir(dir_or_gitdir)
            .args(args)
            .output()
    }
    .with_context(|| format!("failed to execute git {:?}", args))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        bail!(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn run_git(command: &mut Command, context_message: &str) -> Result<()> {
    let output = command
        .output()
        .with_context(|| context_message.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        bail!(
            "{}: {}",
            context_message,
            String::from_utf8_lossy(&output.stderr).trim()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::TempDir;

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

    fn init_test_repo() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        git(temp_dir.path(), &["init"]);
        git(temp_dir.path(), &["config", "user.name", "Test User"]);
        git(
            temp_dir.path(),
            &["config", "user.email", "test@example.com"],
        );
        fs::write(temp_dir.path().join("README.md"), "hello\n").unwrap();
        git(temp_dir.path(), &["add", "README.md"]);
        git(temp_dir.path(), &["commit", "-m", "init"]);
        temp_dir
    }

    fn git_output(dir: &Path, args: &[&str]) -> String {
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
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn git_dir_output(git_dir: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg(format!("--git-dir={}", git_dir.display()))
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git --git-dir={} {:?} failed: {}",
            git_dir.display(),
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn commit_file(dir: &Path, name: &str, contents: &str, message: &str) -> String {
        fs::write(dir.join(name), contents).unwrap();
        git(dir, &["add", name]);
        git(dir, &["commit", "-m", message]);
        git_output(dir, &["rev-parse", "HEAD"])
    }

    fn current_branch(dir: &Path) -> String {
        git_output(dir, &["rev-parse", "--abbrev-ref", "HEAD"])
    }

    fn file_repo_url(path: &Path) -> String {
        format!("file://{}", path.display())
    }

    #[test]
    fn git_helpers_accept_repo_root() {
        let temp_dir = init_test_repo();

        assert!(current_git_revision(temp_dir.path()).is_some());
        assert!(current_git_log_line(temp_dir.path()).is_some());
    }

    #[test]
    fn git_helpers_reject_nested_directory_inside_repo() {
        let temp_dir = init_test_repo();
        let nested = temp_dir.path().join("nested");
        fs::create_dir_all(&nested).unwrap();

        assert!(current_git_revision(&nested).is_none());
        assert!(current_git_log_line(&nested).is_none());
    }

    #[test]
    fn ensure_repo_mirror_fetches_branch_tip_shallowly() {
        let remote = init_test_repo();
        let branch = current_branch(remote.path());
        let latest_sha = commit_file(remote.path(), "CHANGELOG.md", "next\n", "second");

        let cache_root = TempDir::new().unwrap();
        let cache_dir = cache_root.path().join("branch-cache.git");

        ensure_repo_mirror(&file_repo_url(remote.path()), &branch, &cache_dir).unwrap();

        let resolved_sha = resolve_repo_ref_to_sha(&cache_dir, &branch).unwrap();
        assert_eq!(resolved_sha, latest_sha);
        assert_eq!(
            git_dir_output(&cache_dir, &["rev-list", "--count", "FETCH_HEAD"]),
            "1"
        );
    }

    #[test]
    fn ensure_repo_mirror_fetches_pinned_commit_shallowly() {
        let remote = init_test_repo();
        let first_sha = git_output(remote.path(), &["rev-parse", "HEAD"]);
        commit_file(remote.path(), "CHANGELOG.md", "next\n", "second");

        let cache_root = TempDir::new().unwrap();
        let cache_dir = cache_root.path().join("sha-cache.git");

        ensure_repo_mirror(&file_repo_url(remote.path()), &first_sha, &cache_dir).unwrap();

        let resolved_sha = resolve_repo_ref_to_sha(&cache_dir, &first_sha).unwrap();
        assert_eq!(resolved_sha, first_sha);
        assert_eq!(
            git_dir_output(&cache_dir, &["rev-list", "--count", "FETCH_HEAD"]),
            "1"
        );
    }
}
