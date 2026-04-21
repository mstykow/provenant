#!/bin/bash
# SPDX-FileCopyrightText: Provenant contributors
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

ROOT_MANIFEST="Cargo.toml"
XTASK_LOCKFILE_CANDIDATE="xtask/Cargo.lock"
WORKSPACE_LOCKFILE="Cargo.lock"
CITATION_FILE="CITATION.cff"

python3 - "$ROOT_MANIFEST" "$XTASK_LOCKFILE_CANDIDATE" "$WORKSPACE_LOCKFILE" "$CITATION_FILE" <<'PY'
import pathlib
import re
import sys

root_manifest = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
lockfile_candidate_path = pathlib.Path(sys.argv[2])
workspace_lockfile_path = pathlib.Path(sys.argv[3])
citation_file = pathlib.Path(sys.argv[4]).read_text(encoding="utf-8")

if lockfile_candidate_path.exists():
    lockfile_contents = lockfile_candidate_path.read_text(encoding="utf-8")
    lockfile_label = str(lockfile_candidate_path)
elif workspace_lockfile_path.exists():
    lockfile_contents = workspace_lockfile_path.read_text(encoding="utf-8")
    lockfile_label = str(workspace_lockfile_path)
else:
    raise SystemExit(
        "Could not find xtask/Cargo.lock or workspace Cargo.lock for sync check"
    )

root_version_match = re.search(r'^version = "([^"]+)"$', root_manifest, re.MULTILINE)
if root_version_match is None:
    raise SystemExit("Could not determine root crate version from Cargo.toml")

root_version = root_version_match.group(1)

lockfile_version = None
for block in lockfile_contents.split("[[package]]"):
    if 'name = "provenant-cli"' not in block:
        continue
    version_match = re.search(r'^version = "([^"]+)"$', block, re.MULTILINE)
    if version_match is not None:
        lockfile_version = version_match.group(1)
        break

if lockfile_version is None:
    raise SystemExit(f"Could not determine provenant-cli version from {lockfile_label}")

if root_version != lockfile_version:
    raise SystemExit(
        f"{lockfile_label} is out of sync with Cargo.toml: "
        f"root crate is {root_version}, lockfile has {lockfile_version}.\n"
        "Refresh it with: cargo generate-lockfile --manifest-path xtask/Cargo.toml"
    )

citation_version_match = re.search(r'^version: "([^"]+)"$', citation_file, re.MULTILINE)
if citation_version_match is None:
    raise SystemExit("Could not determine CITATION.cff version")

citation_version = citation_version_match.group(1)

if root_version != citation_version:
    raise SystemExit(
        "CITATION.cff is out of sync with Cargo.toml: "
        f"root crate is {root_version}, CITATION.cff has {citation_version}.\n"
        "Refresh it with: update CITATION.cff or run the cargo-release flow that rewrites it."
    )
PY
