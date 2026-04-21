#!/bin/bash
# SPDX-FileCopyrightText: Provenant contributors
# SPDX-License-Identifier: Apache-2.0

# Release script that updates license data before releasing
# Usage: ./release.sh <patch|minor|major> [--execute]

set -euo pipefail

usage() {
    echo "Usage: ./release.sh <patch|minor|major> [--execute]"
    echo "  --execute: Actually perform the release (default is dry-run)"
    exit 1
}

confirm_execute() {
    local response

    printf "Proceed with %s release? [y/N] " "$RELEASE_TYPE"
    read -r response

    case "$response" in
        y|Y|yes|YES)
            ;;
        *)
            echo "Release cancelled."
            exit 1
            ;;
    esac
}

run_release_step() {
    local step="$1"
    shift

    if [ -n "$EXECUTE_FLAG" ]; then
        cargo release "$step" "$@" --execute --no-confirm
    else
        cargo release "$step" "$@"
    fi
}

case "${1:-}" in
    patch|minor|major)
        RELEASE_TYPE="$1"
        ;;
    *)
        usage
        ;;
esac

EXECUTE_FLAG=""

if [ "${2:-}" = "--execute" ]; then
    EXECUTE_FLAG="--execute"
    echo "⚠️  This will perform an actual release!"
    confirm_execute
elif [ -n "${2:-}" ]; then
    usage
else
    echo "ℹ️  Dry-run mode (use --execute to perform actual release)"
fi

if [ -n "$(git status --porcelain)" ]; then
    echo "⚠️  Working tree is not clean. Commit or stash changes before releasing."
    exit 1
fi

echo "📦 Preparing for $RELEASE_TYPE release..."

# Update license data to latest before releasing
echo "📥 Updating license rules/licenses to latest version..."
if [ ! -e "reference/scancode-toolkit/.git" ]; then
    echo "⚠️  Submodule not initialized. Run ./setup.sh first."
    exit 1
fi

cd reference/scancode-toolkit
CURRENT_COMMIT=$(git rev-parse HEAD)
git fetch origin develop --depth=1
git -c advice.detachedHead=false checkout origin/develop
NEW_COMMIT=$(git rev-parse HEAD)
cd ../..

if [ "$CURRENT_COMMIT" != "$NEW_COMMIT" ]; then
    echo "✅ License data updated: $CURRENT_COMMIT → $NEW_COMMIT"
else
    echo "✅ License data already up to date"
fi

echo "🔎 Verifying ScanCode output format version sync..."
./scripts/check_scancode_output_format_sync.sh
echo "🔧 Regenerating embedded license index artifact..."
cargo run --manifest-path xtask/Cargo.toml --bin generate-index-artifact

if [ -n "$EXECUTE_FLAG" ] && [ "$CURRENT_COMMIT" != "$NEW_COMMIT" ]; then
    git add reference/scancode-toolkit resources/license_detection/license_index.zst
    git commit -s -m "chore: update license rules/licenses to latest"
    echo "✅ Committed license data update"
elif [ -z "$EXECUTE_FLAG" ]; then
    echo "ℹ️  Embedded license index artifact regenerated for validation (dry-run mode)"
    git restore resources/license_detection/license_index.zst

    if [ "$CURRENT_COMMIT" != "$NEW_COMMIT" ]; then
        git restore reference/scancode-toolkit
    fi
fi

echo "🚀 Running cargo-release versioning steps..."
run_release_step version "$RELEASE_TYPE"
run_release_step replace
run_release_step hook

if [ -n "$EXECUTE_FLAG" ]; then
    echo "📝 Creating DCO-signed release commit..."
    git add -u

    if git diff --cached --quiet; then
        echo "⚠️  No release changes were staged for commit."
        exit 1
    fi

    git commit -s -m "chore: release"
fi

echo "🚀 Running cargo-release publish, tag, and push steps..."
run_release_step publish
run_release_step tag
run_release_step push

echo "✅ Release completed successfully!"
