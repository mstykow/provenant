// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use clap::Parser;
use glob::Pattern;
use serde::Deserialize;

const COPYRIGHT_LINE: &str = "SPDX-FileCopyrightText: Provenant contributors";
const LICENSE_LINE: &str = "SPDX-License-Identifier: Apache-2.0";
const SCOPE_CONFIG_PATH: &str = ".license-headers.toml";

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

#[derive(Debug, Deserialize, Default)]
struct ScopePatterns {
    #[serde(default)]
    include: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ScopeConfigFile {
    #[serde(default)]
    license_headers: ScopePatterns,
}

#[derive(Debug)]
struct CompiledScopePatterns {
    include: Vec<Pattern>,
    exclude: Vec<Pattern>,
}

#[derive(Debug)]
struct ScopeConfig {
    patterns: CompiledScopePatterns,
}

impl ScopeConfig {
    fn load(repo_root: &Path) -> Result<Self> {
        let config_path = repo_root.join(SCOPE_CONFIG_PATH);
        let contents = fs::read_to_string(&config_path)
            .with_context(|| format!("failed to read {}", config_path.display()))?;
        let parsed: ScopeConfigFile = toml::from_str(&contents)
            .with_context(|| format!("failed to parse {}", config_path.display()))?;

        let include = compile_patterns(&config_path, "include", parsed.license_headers.include)?;
        let exclude = compile_patterns(&config_path, "exclude", parsed.license_headers.exclude)?;

        anyhow::ensure!(
            !include.is_empty(),
            "{} must define at least one include pattern",
            config_path.display()
        );

        Ok(Self {
            patterns: CompiledScopePatterns { include, exclude },
        })
    }

    fn includes(&self, relative_path: &str) -> bool {
        let path = Path::new(relative_path);
        self.patterns
            .include
            .iter()
            .any(|pattern| pattern.matches_path(path))
            && !self
                .patterns
                .exclude
                .iter()
                .any(|pattern| pattern.matches_path(path))
    }
}

fn compile_patterns(
    config_path: &Path,
    kind: &'static str,
    patterns: Vec<String>,
) -> Result<Vec<Pattern>> {
    patterns
        .into_iter()
        .map(|pattern| {
            let normalized = pattern.trim().trim_start_matches('/').to_string();
            anyhow::ensure!(
                !normalized.is_empty(),
                "{} contains an empty {} pattern",
                config_path.display(),
                kind
            );
            Pattern::new(&normalized).with_context(|| {
                format!(
                    "invalid {} pattern {:?} in {}",
                    kind,
                    normalized,
                    config_path.display()
                )
            })
        })
        .collect()
}

fn main() -> Result<()> {
    let args = Args::parse();
    anyhow::ensure!(args.check || args.fix, "pass either --check or --fix");

    let repo_root = find_repo_root()?;
    let scope = ScopeConfig::load(&repo_root)?;
    let candidates = if args.paths.is_empty() {
        collect_all_candidates(&repo_root, &scope)?
    } else {
        collect_requested_candidates(&repo_root, &scope, &args.paths)?
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
    eprintln!("Scope rules live in {SCOPE_CONFIG_PATH}.");
    eprintln!(
        "Fix them with: cargo run --quiet --locked --manifest-path tools/license-headers/Cargo.toml -- --fix"
    );
    anyhow::bail!("license header check failed");
}

fn find_repo_root() -> Result<PathBuf> {
    let mut current = std::env::current_dir()
        .context("failed to resolve current working directory")?
        .canonicalize()
        .context("failed to canonicalize current working directory")?;

    loop {
        if current.join(SCOPE_CONFIG_PATH).is_file() {
            return Ok(current);
        }

        anyhow::ensure!(
            current.pop(),
            "failed to locate {SCOPE_CONFIG_PATH} from current working directory or any parent"
        );
    }
}

fn collect_all_candidates(repo_root: &Path, scope: &ScopeConfig) -> Result<Vec<PathBuf>> {
    let mut candidates = BTreeSet::new();
    for path in git_tracked_files(repo_root)? {
        if !path.is_file() {
            continue;
        }
        if is_in_scope(repo_root, scope, &path)? {
            candidates.insert(path);
        }
    }
    Ok(candidates.into_iter().collect())
}

fn collect_requested_candidates(
    repo_root: &Path,
    scope: &ScopeConfig,
    raw_paths: &[PathBuf],
) -> Result<Vec<PathBuf>> {
    let mut candidates = BTreeSet::new();
    for raw_path in raw_paths {
        let path = normalize_requested_path(raw_path)?;
        if !path.is_file() {
            continue;
        }
        if !path.starts_with(repo_root) {
            continue;
        }
        if is_in_scope(repo_root, scope, &path)? {
            candidates.insert(path);
        }
    }
    Ok(candidates.into_iter().collect())
}

fn git_tracked_files(repo_root: &Path) -> Result<Vec<PathBuf>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("ls-files")
        .arg("-z")
        .output()
        .with_context(|| {
            format!(
                "failed to enumerate tracked files in {}",
                repo_root.display()
            )
        })?;

    anyhow::ensure!(
        output.status.success(),
        "git ls-files failed for {}",
        repo_root.display()
    );

    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|bytes| !bytes.is_empty())
        .map(|bytes| repo_root.join(String::from_utf8_lossy(bytes).into_owned()))
        .collect())
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

fn is_in_scope(repo_root: &Path, scope: &ScopeConfig, path: &Path) -> Result<bool> {
    let relative = rel_path(repo_root, path)?;
    Ok(scope.includes(&relative))
}

fn comment_prefix(path: &Path) -> Option<&'static str> {
    match path.extension().and_then(|value| value.to_str()) {
        Some("rs") => Some("//"),
        Some("sh") | Some("yml") | Some("yaml") => Some("#"),
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
