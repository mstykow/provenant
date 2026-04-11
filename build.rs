use std::env;
use std::process::Command;

#[path = "src/version_format.rs"]
mod version_format;

fn main() {
    println!("cargo:rerun-if-env-changed=PROVENANT_BUILD_VERSION");

    let package_version =
        env::var("CARGO_PKG_VERSION").expect("Cargo should set CARGO_PKG_VERSION");
    emit_git_rerun_hints(&package_version);

    let build_version = env::var("PROVENANT_BUILD_VERSION")
        .ok()
        .and_then(|value| version_format::sanitize_build_version(&value))
        .unwrap_or_else(|| derive_build_version(&package_version));

    println!("cargo:rustc-env=PROVENANT_BUILD_VERSION={build_version}");
}

fn derive_build_version(package_version: &str) -> String {
    let describe = git_output(&[
        "describe",
        "--tags",
        "--dirty",
        "--always",
        "--match",
        &format!("v{package_version}"),
    ]);
    let short_sha = git_output(&["rev-parse", "--short", "HEAD"]);

    version_format::derive_build_version(package_version, describe.as_deref(), short_sha.as_deref())
}

fn emit_git_rerun_hints(package_version: &str) {
    for git_path in [
        "HEAD".to_string(),
        "index".to_string(),
        "packed-refs".to_string(),
        format!("refs/tags/v{package_version}"),
    ] {
        if let Some(path) = git_output(&["rev-parse", "--git-path", &git_path]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }

    if let Some(reference) = git_output(&["symbolic-ref", "-q", "HEAD"])
        && let Some(path) = git_output(&["rev-parse", "--git-path", &reference])
    {
        println!("cargo:rerun-if-changed={path}");
    }
}

fn git_output(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return None;
    }

    Some(trimmed.to_string())
}
