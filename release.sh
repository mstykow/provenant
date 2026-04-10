#!/bin/bash

# Release script that updates license data before releasing
# Usage: ./release.sh <patch|minor|major> [--execute]

set -e

if [ -z "$1" ]; then
    echo "Usage: ./release.sh <patch|minor|major> [--execute]"
    echo "  --execute: Actually perform the release (default is dry-run)"
    exit 1
fi

RELEASE_TYPE=$1
EXECUTE_FLAG=""

if [ "$2" = "--execute" ]; then
    EXECUTE_FLAG="--execute"
    echo "⚠️  This will perform an actual release!"
else
    echo "ℹ️  Dry-run mode (use --execute to perform actual release)"
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
    echo "🔎 Verifying ScanCode output format version sync..."
    ./scripts/check_scancode_output_format_sync.sh
    echo "🔧 Regenerating embedded license index artifact..."
    cargo run --manifest-path xtask/Cargo.toml --bin generate-index-artifact
    
    if [ -n "$EXECUTE_FLAG" ]; then
        git add reference/scancode-toolkit resources/license_detection/license_index.zst
        git commit -m "chore: update license rules/licenses to latest"
        echo "✅ Committed license data update"
    else
        echo "ℹ️  License data would be updated (dry-run mode)"
        git restore reference/scancode-toolkit resources/license_detection/license_index.zst
    fi
else
    echo "✅ License data already up to date"
    echo "🔎 Verifying ScanCode output format version sync..."
    ./scripts/check_scancode_output_format_sync.sh
fi

# Run cargo-release
echo "🚀 Running cargo-release $RELEASE_TYPE..."
if [ -n "$EXECUTE_FLAG" ]; then
    cargo release $RELEASE_TYPE --execute
else
    cargo release $RELEASE_TYPE
fi

RELEASE_EXIT_CODE=$?

if [ $RELEASE_EXIT_CODE -eq 0 ]; then
    echo "✅ Release completed successfully!"
else
    echo "❌ Release failed with exit code $RELEASE_EXIT_CODE"
    exit $RELEASE_EXIT_CODE
fi
