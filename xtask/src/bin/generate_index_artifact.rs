use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

use provenant::license_detection::build_policy::{
    DEFAULT_INDEX_BUILD_POLICY_PATH, EMBEDDED_LICENSE_INDEX_SOURCE,
    apply_default_index_build_policy,
};
use provenant::license_detection::detect_scancode_spdx_license_list_version;
use provenant::license_detection::embedded::schema::{EmbeddedLoaderSnapshot, SCHEMA_VERSION};
use provenant::license_detection::rules::{
    load_loaded_licenses_from_directory, load_loaded_rules_from_directory,
};

#[derive(Parser, Debug)]
#[command(
    name = "generate-index-artifact",
    about = "Generate the embedded license loader artifact from ScanCode rules and licenses"
)]
struct Args {
    #[arg(long, help = "Output path")]
    output: Option<PathBuf>,

    #[arg(long, help = "Rules directory")]
    rules: Option<PathBuf>,

    #[arg(long, help = "Licenses directory")]
    licenses: Option<PathBuf>,

    #[arg(long, help = "Verify existing artifact matches regenerated output")]
    check: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let output_path = args
        .output
        .unwrap_or_else(|| PathBuf::from("resources/license_detection/license_index.zst"));
    let rules_dir = args.rules.unwrap_or_else(|| {
        PathBuf::from(provenant::license_detection::SCANCODE_LICENSES_RULES_PATH)
    });
    let licenses_dir = args.licenses.unwrap_or_else(|| {
        PathBuf::from(provenant::license_detection::SCANCODE_LICENSES_LICENSES_PATH)
    });

    println!("Loading rules from: {}", rules_dir.display());
    println!("Loading licenses from: {}", licenses_dir.display());

    let mut loaded_rules = load_loaded_rules_from_directory(&rules_dir)
        .with_context(|| format!("Failed to load rules from {}", rules_dir.display()))?;
    let mut loaded_licenses = load_loaded_licenses_from_directory(&licenses_dir)
        .with_context(|| format!("Failed to load licenses from {}", licenses_dir.display()))?;

    let (filtered_rules, filtered_licenses, policy_report) =
        apply_default_index_build_policy(loaded_rules, loaded_licenses)?;
    loaded_rules = filtered_rules;
    loaded_licenses = filtered_licenses;
    let license_index_provenance =
        policy_report.to_license_index_provenance(EMBEDDED_LICENSE_INDEX_SOURCE);

    if !policy_report.is_empty() {
        println!(
            "Applied license index build policy from {} (ignored {} rules, {} licenses, {} dependent rules, added {} rules, replaced {} rules, added {} licenses, replaced {} licenses)",
            DEFAULT_INDEX_BUILD_POLICY_PATH,
            policy_report.ignored_rules.len(),
            policy_report.ignored_licenses.len(),
            policy_report.ignored_rules_due_to_licenses.len(),
            policy_report.added_rules.len(),
            policy_report.replaced_rules.len(),
            policy_report.added_licenses.len(),
            policy_report.replaced_licenses.len()
        );
    }

    println!("Loaded {} rules", loaded_rules.len());
    println!("Loaded {} licenses", loaded_licenses.len());

    loaded_rules.sort_by(|a, b| a.identifier.cmp(&b.identifier));
    loaded_licenses.sort_by(|a, b| a.key.cmp(&b.key));

    let spdx_license_list_version = detect_scancode_spdx_license_list_version(&rules_dir)?
        .context("Failed to detect SPDX license list version from ScanCode source metadata")?;

    let snapshot = EmbeddedLoaderSnapshot {
        schema_version: SCHEMA_VERSION,
        metadata: provenant::license_detection::embedded::schema::EmbeddedArtifactMetadata {
            spdx_license_list_version,
            license_index_provenance,
        },
        rules: loaded_rules,
        licenses: loaded_licenses,
    };

    println!("Serializing...");
    let postcard_bytes =
        postcard::to_allocvec(&snapshot).context("Failed to serialize embedded artifact")?;
    let bytes =
        zstd::encode_all(&postcard_bytes[..], 0).context("Failed to compress embedded artifact")?;

    println!("Total artifact size: {} bytes", bytes.len());

    if args.check {
        let existing = fs::read(&output_path).with_context(|| {
            format!(
                "Failed to read existing artifact from {}",
                output_path.display()
            )
        })?;

        if existing == bytes {
            println!("Artifact is up to date: {}", output_path.display());
        } else {
            eprintln!("Artifact is out of date: {}", output_path.display());
            eprintln!(
                "Run: cargo run --manifest-path xtask/Cargo.toml --bin generate-index-artifact"
            );
            std::process::exit(1);
        }
    } else {
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {}", parent.display()))?;
        }

        fs::write(&output_path, &bytes)
            .with_context(|| format!("Failed to write to {}", output_path.display()))?;

        println!("Wrote artifact to: {}", output_path.display());
    }

    Ok(())
}
