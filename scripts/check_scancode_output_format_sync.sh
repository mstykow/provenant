#!/bin/bash
# SPDX-FileCopyrightText: Provenant contributors
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

SCANCODE_CONFIG="reference/scancode-toolkit/src/scancode_config.py"
LOCAL_OUTPUT_MODEL="src/models/output.rs"

python3 - "$SCANCODE_CONFIG" "$LOCAL_OUTPUT_MODEL" <<'PY'
import pathlib
import re
import sys

scancode_config = pathlib.Path(sys.argv[1])
local_output_model = pathlib.Path(sys.argv[2])

if not scancode_config.is_file():
    raise SystemExit(
        f"Could not find ScanCode config at {scancode_config}. "
        "Initialize the reference/scancode-toolkit submodule first."
    )

scancode_text = scancode_config.read_text(encoding="utf-8")
local_text = local_output_model.read_text(encoding="utf-8")

upstream_match = re.search(
    r"^__output_format_version__\s*=\s*'([^']+)'$",
    scancode_text,
    re.MULTILINE,
)
if upstream_match is None:
    raise SystemExit(
        f"Could not determine ScanCode output format version from {scancode_config}"
    )

local_match = re.search(
    r'^pub const OUTPUT_FORMAT_VERSION: &str = "([^"]+)";$',
    local_text,
    re.MULTILINE,
)
if local_match is None:
    raise SystemExit(
        f"Could not determine Provenant output format version from {local_output_model}"
    )

upstream_version = upstream_match.group(1)
local_version = local_match.group(1)

if upstream_version != local_version:
    raise SystemExit(
        "Provenant output format version is out of sync with the pinned ScanCode submodule:\n"
        f"  ScanCode:   {upstream_version}\n"
        f"  Provenant:  {local_version}\n\n"
        "Update the Provenant output contract before merging this submodule change.\n"
        "At minimum review:\n"
        "  - src/models/output.rs\n"
        "  - src/output_schema/\n"
        "  - tests/output_format_golden.rs\n"
        "  - testdata/output-formats/\n\n"
        "Then rerun:\n"
        "  cargo test --test output_format_golden --release --verbose"
    )
PY
