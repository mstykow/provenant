#!/usr/bin/env bash
set -euo pipefail

if ! command -v cargo-machete &> /dev/null; then
    echo "Installing cargo-machete..."
    cargo install cargo-machete
fi

cargo machete ./Cargo.toml
cargo machete ./xtask/Cargo.toml

echo "No unused dependencies in checked Cargo.toml files."
