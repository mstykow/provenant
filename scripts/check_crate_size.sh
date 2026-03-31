#!/usr/bin/env bash

set -euo pipefail

readonly MAX_CRATE_BYTES=10000000
readonly PACKAGE_DIR="target/package"

python3 - "$PACKAGE_DIR" <<'PY'
from pathlib import Path
import sys

for path in Path(sys.argv[1]).glob("provenant-cli-*.crate"):
    path.unlink()
PY

cargo package --locked --allow-dirty --no-verify > /dev/null

crate_path=$(python3 - <<'PY'
from pathlib import Path

package_dir = Path("target/package")
candidates = sorted(
    package_dir.glob("provenant-cli-*.crate"),
    key=lambda path: path.stat().st_mtime,
    reverse=True,
)

if not candidates:
    raise SystemExit("No packaged crate archive found under target/package")

print(candidates[0])
PY
)

crate_size_bytes=$(python3 - "$crate_path" <<'PY'
from pathlib import Path
import sys

print(Path(sys.argv[1]).stat().st_size)
PY
)

crate_size_mb=$(python3 - "$crate_size_bytes" <<'PY'
import sys

print(f"{int(sys.argv[1]) / 1_000_000:.2f}")
PY
)

max_crate_mb=$(python3 - "$MAX_CRATE_BYTES" <<'PY'
import sys

print(f"{int(sys.argv[1]) / 1_000_000:.2f}")
PY
)

if (( crate_size_bytes > MAX_CRATE_BYTES )); then
    echo "Packaged crate exceeds the crates.io size limit:"
    echo "  archive: ${crate_path}"
    echo "  size: ${crate_size_bytes} bytes (${crate_size_mb} MB)"
    echo "  limit: ${MAX_CRATE_BYTES} bytes (${max_crate_mb} MB)"
    exit 1
fi

echo "Packaged crate size is within the crates.io limit:"
echo "  archive: ${crate_path}"
echo "  size: ${crate_size_bytes} bytes (${crate_size_mb} MB)"
echo "  limit: ${MAX_CRATE_BYTES} bytes (${max_crate_mb} MB)"
