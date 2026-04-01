#!/usr/bin/env bash

set -euo pipefail

check_only=false
if [[ "${1:-}" == "--check" ]]; then
    check_only=true
    shift
fi

if ! command -v cargo-sort &> /dev/null; then
    echo "cargo-sort is required but not installed." >&2
    echo "Install it with: cargo install cargo-sort" >&2
    exit 1
fi

args=(sort --grouped)
if [[ "${check_only}" == true ]]; then
    args+=(--check)
fi

if [[ "$#" -eq 0 ]]; then
    args+=(--workspace)
else
    for target in "$@"; do
        if [[ "$(basename "${target}")" == "Cargo.toml" ]]; then
            args+=("$(dirname "${target}")")
        else
            args+=("${target}")
        fi
    done
fi

cargo "${args[@]}"
