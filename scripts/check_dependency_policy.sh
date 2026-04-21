#!/usr/bin/env bash
# SPDX-FileCopyrightText: Provenant contributors
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

if ! command -v cargo-deny &> /dev/null; then
    echo "cargo-deny is required but not installed." >&2
    echo "Install it with: cargo install --locked cargo-deny" >&2
    exit 1
fi

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"

cargo deny \
    --manifest-path "$repo_root/Cargo.toml" \
    check \
    --config "$repo_root/deny.toml" \
    advisories bans licenses sources
