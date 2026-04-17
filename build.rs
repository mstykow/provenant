use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "build_support/version_format.rs"]
mod version_format;

fn main() {
    println!("cargo:rerun-if-env-changed=PROVENANT_BUILD_VERSION");
    generate_license_overlay_manifest();

    let package_version =
        env::var("CARGO_PKG_VERSION").expect("Cargo should set CARGO_PKG_VERSION");

    let build_version = env::var("PROVENANT_BUILD_VERSION")
        .ok()
        .and_then(|value| version_format::sanitize_build_version(&value))
        .unwrap_or_else(|| derive_build_version(&package_version));

    println!("cargo:rustc-env=PROVENANT_BUILD_VERSION={build_version}");

    generate_legalese_artifact();
}

fn generate_legalese_artifact() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let data_path = Path::new(&manifest_dir).join("resources/license_detection/legalese_data.txt");
    println!("cargo:rerun-if-changed={}", data_path.display());

    let mut map = BTreeMap::new();
    let content = fs::read_to_string(&data_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", data_path.display()));
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (word, id_str) = line.split_once('\t').unwrap_or_else(|| {
            panic!("invalid legalese data line (no tab): {line:?}");
        });
        let id: u16 = id_str.parse().unwrap_or_else(|e| {
            panic!("invalid token id {id_str:?} for word {word:?}: {e}");
        });
        map.insert(word.to_string(), id);
    }

    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&map).unwrap();
    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir).join("legalese.rkyv");
    fs::write(&out_path, &bytes).unwrap_or_else(|e| {
        panic!("failed to write {}: {e}", out_path.display());
    });
}

fn generate_license_overlay_manifest() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo should set CARGO_MANIFEST_DIR"));
    let overlay_root = manifest_dir.join("resources/license_detection/overlay");
    let out_dir =
        PathBuf::from(env::var("OUT_DIR").expect("Cargo should set OUT_DIR for build scripts"));

    println!("cargo:rerun-if-changed={}", overlay_root.display());

    let generated = format!(
        "pub(crate) const BUNDLED_RULE_OVERLAY_FILES: &[BundledOverlayFile] = &[\n{}\n];\n\n\
         pub(crate) const BUNDLED_LICENSE_OVERLAY_FILES: &[BundledOverlayFile] = &[\n{}\n];\n",
        generate_overlay_entries(&overlay_root.join("rules"), "RULE"),
        generate_overlay_entries(&overlay_root.join("licenses"), "LICENSE"),
    );

    fs::write(out_dir.join("bundled_license_overlays.rs"), generated)
        .expect("Failed to write bundled overlay manifest");
}

fn generate_overlay_entries(dir: &Path, extension: &str) -> String {
    if !dir.exists() {
        return String::new();
    }

    let mut entries = fs::read_dir(dir)
        .unwrap_or_else(|error| {
            panic!(
                "Failed to read overlay directory {}: {}",
                dir.display(),
                error
            )
        })
        .map(|entry| {
            entry.unwrap_or_else(|error| {
                panic!(
                    "Failed to read overlay directory entry in {}: {}",
                    dir.display(),
                    error
                )
            })
        })
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some(extension))
        .collect::<Vec<_>>();
    entries.sort();

    entries
        .into_iter()
        .map(|path| {
            println!("cargo:rerun-if-changed={}", path.display());

            let identifier = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_else(|| panic!("Overlay file has invalid name: {}", path.display()));
            let contents = fs::read_to_string(&path).unwrap_or_else(|error| {
                panic!("Failed to read overlay file {}: {}", path.display(), error)
            });
            let quoted_contents = format!("{:?}", contents);

            format!(
                "    BundledOverlayFile {{ identifier: {:?}, contents: {} }},",
                identifier, quoted_contents
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
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
