#!/usr/bin/env bash
#
# Setup script for profiling provenant with samply
# Usage: ./scripts/profile-setup.sh
#
# This script:
#   1. Clones the test repository to /tmp
#   2. Builds provenant in profiling mode with debug symbols
#
# After running this, use the samply MCP server to profile

set -euo pipefail

REPO_URL="https://github.com/abraemer/opossum-file.rs.git"
REPO_COMMIT="dc0d7680c73333443ccc3df9657843210440a2ac"
REPO_NAME="opossum-file.rs"
TMP_DIR="/tmp/provenant-benchmark"
OUTPUT_DIR="${TMP_DIR}/results"
PROJECT_ROOT="/home/adrian/Documents/projects/provenant"

cd /tmp

echo "=========================================="
echo "Provenant Profiling Setup"
echo "=========================================="
echo ""
echo "Configuration:"
echo "  Repository: ${REPO_URL}"
echo "  Commit:     ${REPO_COMMIT}"
echo "  Target:     ${TMP_DIR}/${REPO_NAME}"
echo ""

echo "[1/3] Cleaning up previous benchmark directory..."
rm -rf "${TMP_DIR}"
mkdir -p "${OUTPUT_DIR}"

echo "[2/3] Cloning test repository..."
git clone "${REPO_URL}" "${TMP_DIR}/${REPO_NAME}" 2>&1 | sed 's/^/  /'
cd "${TMP_DIR}/${REPO_NAME}"
git checkout "${REPO_COMMIT}" 2>&1 | sed 's/^/  /'
git log -1 --oneline
echo ""

echo "[3/3] Building provenant (profiling mode with debug symbols)..."
cd "${PROJECT_ROOT}"
cargo build --profile profiling 2>&1 | grep -E '(Compiling|Finished|error)' | sed 's/^/  /'
echo ""

PROVENANT_BIN="${PROJECT_ROOT}/target/profiling/provenant"
if [[ ! -x "${PROVENANT_BIN}" ]]; then
    echo "ERROR: provenant binary not found at ${PROVENANT_BIN}"
    exit 1
fi

echo "=========================================="
echo "Setup Complete"
echo "=========================================="
echo ""
echo "Binary: ${PROVENANT_BIN}"
echo "Working directory: ${TMP_DIR}/${REPO_NAME}"
echo ""
echo "Sample command for profiling:"
echo "  ${PROVENANT_BIN} --json ${OUTPUT_DIR}/scan-output.json --copyright --email --license --url --exclude '*.git*' --exclude 'target/*' ."
echo ""
echo "To clean up:"
echo "  rm -rf ${TMP_DIR}"
