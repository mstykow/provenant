// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Parser;

const COPYRIGHT_LINE: &str = "SPDX-FileCopyrightText: Provenant contributors";
const LICENSE_LINE: &str = "SPDX-License-Identifier: Apache-2.0";

#[derive(Parser, Debug)]
struct Args {
    /// Fail if any in-scope file lacks the expected header.
    #[arg(long, conflicts_with = "fix")]
    check: bool,

    /// Insert or normalize headers in in-scope files.
    #[arg(long, conflicts_with = "check")]
    fix: bool,

    /// Optional file paths to restrict processing; defaults to all in-scope files.
    paths: Vec<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    anyhow::ensure!(args.check || args.fix, "pass either --check or --fix");

    let repo_root = provenant_xtask::common::project_root();
    let candidates = if args.paths.is_empty() {
        collect_all_candidates(&repo_root)?
    } else {
        collect_requested_candidates(&repo_root, &args.paths)?
    };

    if args.fix {
        let mut updated = Vec::new();
        for path in candidates {
            let original = fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let rewritten = rewrite_with_header(&path, &original)?;
            if rewritten != original {
                fs::write(&path, rewritten)
                    .with_context(|| format!("failed to write {}", path.display()))?;
                updated.push(rel_path(&repo_root, &path)?);
            }
        }

        if updated.is_empty() {
            println!("All in-scope files already have the expected license header.");
        } else {
            println!("Updated license headers:");
            for path in updated {
                println!("  {path}");
            }
        }
        return Ok(());
    }

    let mut missing = Vec::new();
    for path in candidates {
        let original = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let rewritten = rewrite_with_header(&path, &original)?;
        if rewritten != original {
            missing.push(rel_path(&repo_root, &path)?);
        }
    }

    if missing.is_empty() {
        println!("All in-scope files have the expected license header.");
        return Ok(());
    }

    eprintln!("Files missing the expected license header:");
    for path in missing {
        eprintln!("  {path}");
    }
    eprintln!();
    eprintln!(
        "Fix them with: cargo run --quiet --locked --manifest-path xtask/Cargo.toml --bin check-license-headers -- --fix"
    );
    anyhow::bail!("license header check failed");
}

fn collect_all_candidates(repo_root: &Path) -> Result<Vec<PathBuf>> {
    let mut candidates = BTreeSet::new();
    walk_dir(repo_root, repo_root, &mut candidates)?;
    Ok(candidates.into_iter().collect())
}

fn walk_dir(repo_root: &Path, dir: &Path, candidates: &mut BTreeSet<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry.with_context(|| format!("failed to read entry in {}", dir.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to stat {}", path.display()))?;

        if file_type.is_dir() {
            if should_skip_dir(repo_root, &path)? {
                continue;
            }
            walk_dir(repo_root, &path, candidates)?;
            continue;
        }

        if file_type.is_file() && is_in_scope(repo_root, &path)? {
            candidates.insert(path);
        }
    }
    Ok(())
}

fn collect_requested_candidates(repo_root: &Path, raw_paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut candidates = BTreeSet::new();
    for raw_path in raw_paths {
        let path = normalize_requested_path(raw_path)?;
        if !path.is_file() {
            continue;
        }
        if !path.starts_with(repo_root) {
            continue;
        }
        if is_in_scope(repo_root, &path)? {
            candidates.insert(path);
        }
    }
    Ok(candidates.into_iter().collect())
}

fn normalize_requested_path(raw_path: &Path) -> Result<PathBuf> {
    if raw_path.is_absolute() {
        return Ok(raw_path.to_path_buf());
    }
    Ok(std::env::current_dir()
        .context("failed to resolve current working directory")?
        .join(raw_path))
}

fn rel_path(repo_root: &Path, path: &Path) -> Result<String> {
    Ok(path
        .strip_prefix(repo_root)
        .with_context(|| format!("{} is outside {}", path.display(), repo_root.display()))?
        .to_string_lossy()
        .replace('\\', "/"))
}

fn should_skip_dir(repo_root: &Path, path: &Path) -> Result<bool> {
    let relative = rel_path(repo_root, path)?;
    Ok(matches!(
        relative.as_str(),
        ".git"
            | "node_modules"
            | "reference"
            | "resources"
            | "target"
            | "testdata"
            | ".provenant"
            | ".sisyphus"
    ))
}

fn is_in_scope(repo_root: &Path, path: &Path) -> Result<bool> {
    let relative = rel_path(repo_root, path)?;

    if matches!(relative.as_str(), "build.rs" | "release.sh" | "setup.sh") {
        return Ok(true);
    }

    if relative == "docs/SUPPORTED_FORMATS.md" {
        return Ok(false);
    }

    let ext = path.extension().and_then(|value| value.to_str());

    Ok((relative.starts_with("src/") && ext == Some("rs"))
        || (relative.starts_with("tests/") && ext == Some("rs"))
        || (relative.starts_with("xtask/src/") && ext == Some("rs"))
        || (relative.starts_with("build_support/") && ext == Some("rs"))
        || (relative.starts_with("scripts/") && matches!(ext, Some("sh") | Some("py")))
        || (relative.starts_with(".github/workflows/")
            && matches!(ext, Some("yml") | Some("yaml")))
        || (relative.starts_with(".github/actions/") && matches!(ext, Some("yml") | Some("yaml"))))
}

fn comment_prefix(path: &Path) -> Option<&'static str> {
    match path.extension().and_then(|value| value.to_str()) {
        Some("rs") => Some("//"),
        Some("sh") | Some("py") | Some("yml") | Some("yaml") => Some("#"),
        _ if path.file_name().and_then(|value| value.to_str()) == Some("build.rs") => Some("//"),
        _ => None,
    }
}

fn expected_header(prefix: &str) -> [String; 2] {
    [
        format!("{prefix} {COPYRIGHT_LINE}"),
        format!("{prefix} {LICENSE_LINE}"),
    ]
}

fn rewrite_with_header(path: &Path, original: &str) -> Result<String> {
    let prefix = comment_prefix(path)
        .with_context(|| format!("no comment prefix configured for {}", path.display()))?;
    let expected = expected_header(prefix);
    let mut lines: Vec<&str> = original.lines().collect();

    let mut output = Vec::new();
    let mut index = 0;
    if lines.first().is_some_and(|line| line.starts_with("#!")) {
        output.push(lines[0].to_string());
        index = 1;
    }

    while lines.get(index).is_some_and(|line| line.trim().is_empty()) {
        index += 1;
    }

    while lines.get(index).is_some_and(|line| line.contains("SPDX-")) {
        index += 1;
    }

    while lines.get(index).is_some_and(|line| line.trim().is_empty()) {
        index += 1;
    }

    output.extend(expected);
    if index < lines.len() {
        output.push(String::new());
        output.extend(lines.drain(index..).map(ToOwned::to_owned));
    }

    Ok(output.join("\n") + "\n")
}
