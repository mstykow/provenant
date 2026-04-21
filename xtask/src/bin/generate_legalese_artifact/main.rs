// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "generate-legalese-artifact",
    about = "Generate the rkyv-serialized legalese dictionary artifact"
)]
struct Args {
    #[arg(long, help = "Output path")]
    output: Option<PathBuf>,

    #[arg(long, help = "Verify existing artifact matches regenerated output")]
    check: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let output_path = args
        .output
        .unwrap_or_else(|| PathBuf::from("resources/license_detection/legalese.rkyv"));

    let data_path = PathBuf::from("resources/license_detection/legalese_data.txt");

    let mut map = BTreeMap::new();
    let content = fs::read_to_string(&data_path)
        .with_context(|| format!("Failed to read {}", data_path.display()))?;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (word, id_str) = line
            .split_once('\t')
            .with_context(|| format!("invalid legalese data line (no tab): {line:?}"))?;
        let id: u16 = id_str
            .parse()
            .with_context(|| format!("invalid token id {id_str:?} for word {word:?}"))?;
        map.insert(word.to_string(), id);
    }

    println!("Serializing {} legalese entries with rkyv...", map.len());

    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&map)
        .map_err(|e| anyhow::anyhow!("Failed to serialize legalese dictionary: {e}"))?;

    println!("Total artifact size: {} bytes", bytes.len());

    if args.check {
        let existing = fs::read(&output_path).with_context(|| {
            format!(
                "Failed to read existing artifact from {}",
                output_path.display()
            )
        })?;

        if existing == bytes[..] {
            println!("Artifact is up to date: {}", output_path.display());
        } else {
            eprintln!("Artifact is out of date: {}", output_path.display());
            eprintln!(
                "Run: cargo run --manifest-path xtask/Cargo.toml --bin generate-legalese-artifact"
            );
            std::process::exit(1);
        }
    } else {
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {}", parent.display()))?;
        }

        fs::write(&output_path, &bytes[..])
            .with_context(|| format!("Failed to write to {}", output_path.display()))?;

        println!("Wrote artifact to: {}", output_path.display());
    }

    Ok(())
}
