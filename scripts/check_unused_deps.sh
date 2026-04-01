#!/usr/bin/env bash
set -euo pipefail

if ! command -v cargo-machete &> /dev/null; then
    echo "cargo-machete is required but not installed." >&2
    echo "Install it with: cargo install cargo-machete" >&2
    exit 1
fi

cargo machete ./Cargo.toml
cargo machete ./xtask/Cargo.toml

echo "No unused dependencies in checked Cargo.toml files."
