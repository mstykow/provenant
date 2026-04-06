use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::common::{derive_repo_name_from_url, sanitize_label};

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

pub fn ensure_repo_mirror(repo_url: &str, cache_dir: &Path) -> Result<()> {
    if let Some(parent) = cache_dir.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create repo-cache root: {}", parent.display()))?;
    }
    if !cache_dir.exists() {
        run_git(
            Command::new("git")
                .args(["clone", "--mirror", repo_url])
                .arg(cache_dir),
            "failed to clone mirror",
        )?;
        return Ok(());
    }
    run_git(
        Command::new("git")
            .arg(format!("--git-dir={}", cache_dir.display()))
            .args(["remote", "update", "--prune"]),
        "failed to update mirror",
    )
}

pub fn resolve_repo_ref_to_sha(cache_dir: &Path, repo_ref: &str) -> Result<String> {
    let candidates = [
        format!("{repo_ref}^{{commit}}"),
        format!("refs/heads/{repo_ref}^{{commit}}"),
        format!("refs/tags/{repo_ref}^{{commit}}"),
        format!("refs/remotes/origin/{repo_ref}^{{commit}}"),
    ];

    for candidate in &candidates {
        if let Ok(sha) = git_output(cache_dir, ["rev-parse", "--verify", candidate.as_str()]) {
            return Ok(sha.trim().to_string());
        }
    }

    let _ = run_git(
        Command::new("git")
            .arg(format!("--git-dir={}", cache_dir.display()))
            .args(["fetch", "--prune", "origin", repo_ref]),
        "failed to fetch requested ref",
    );

    for candidate in &candidates {
        if let Ok(sha) = git_output(cache_dir, ["rev-parse", "--verify", candidate.as_str()]) {
            return Ok(sha.trim().to_string());
        }
    }

    bail!(
        "unable to resolve repo ref '{repo_ref}' in cache {}",
        cache_dir.display()
    )
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
}
