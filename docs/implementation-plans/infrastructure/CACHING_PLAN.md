# Incremental Scanning Implementation Plan

> **Status**: 🟢 Complete — incremental scanning, XDG cache defaults, cache lock coordination, and license index caching are implemented
> **Current contract owner**: [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) for runtime design, [`../../CLI_GUIDE.md`](../../CLI_GUIDE.md) for user-facing behavior, and [`../../TESTING_STRATEGY.md`](../../TESTING_STRATEGY.md) for verification expectations

## Overview

This plan tracked the rollout of Provenant's opt-in incremental scanning.

The implemented design has three important properties:

- incremental reuse is **opt-in** (`--incremental`)
- reuse is keyed to the scan root and scan options
- correctness is preferred over aggressive reuse

In product terms, this exists to make repeated scans of the same checkout faster after the first
successful run.

The result is a shared cache root with:

- an **incremental manifest** keyed by scan root and scan options
- **multi-process-safe writes** and cache clearing
- a clear product story: repeated native reruns use `--incremental`

This document is retained as the completed rollout record.

## Scope

### What This Covers

- Incremental scanning for unchanged files
- Incremental manifest invalidation based on tool/runtime options
- Shared cache-root selection via XDG defaults, env var, and CLI override
- Cache lifecycle controls: `--cache-dir`, `--cache-clear`
- Scan-time progress/summary reporting for incremental reuse
- Multi-process-safe manifest writes
- ScanCode-aligned in-run spill control via `--max-in-memory`

### What This Does Not Cover

- Distributed/shared-remote caching
- Cache eviction or size-management policies
- A Rust-specific `--no-cache` flag
- Cache eviction or size-management policies for the license index cache
- Distributed/shared-remote caching

Custom `--license-rules-path` scans now participate in the license index cache
(see below) in addition to the normal incremental manifest workflow.

## Reference Constraints from Python ScanCode

- Python does **not** offer incremental rescans of unchanged files.
- Python's `--max-in-memory` controls per-run spill behavior, not reusable
  cross-run scan skipping.
- `--from-json` is not an incremental scan mode.
- `--no-cache` is not part of the current upstream CLI surface.

That means Provenant's `--incremental` remains a beyond-parity feature, while
`--max-in-memory` stays part of the parity-aligned runtime contract.

## Final Design

### Runtime Pieces

| Piece                     | Purpose                                                              | Status         |
| ------------------------- | -------------------------------------------------------------------- | -------------- |
| Embedded license artifact | Default startup source for license matching data                     | ✅ Implemented |
| License index cache       | rkyv-serialized index cache with content-based fingerprinting        | ✅ Implemented |
| Incremental manifest      | Reuse unchanged file results within the same scan root/options space | ✅ Implemented |

### Cache Root and CLI Surface

The cache root is resolved in this order:

1. `--cache-dir`
2. `PROVENANT_CACHE`
3. platform-native default via `directories::ProjectDirs`

Supported CLI behavior:

| Flag                       | Role                                               | Final decision  |
| -------------------------- | -------------------------------------------------- | --------------- |
| `--cache-dir`              | Chooses shared persistent cache root               | Keep            |
| `--cache-clear`            | Clears selected incremental cache root before scan | Keep            |
| `--incremental`            | Enables unchanged-file reuse                       | Keep            |
| `--max-in-memory`          | Controls in-run spill behavior                     | Keep for parity |
| `--no-cache`               | Redundant with opt-in incremental reuse            | Not planned     |
| `--reindex`                | Forces rebuild of license index cache              | Keep            |
| `--no-license-index-cache` | Disables persistent license index cache I/O        | Keep            |

Custom `--license-rules-path` scans now participate in the license index cache
(see License Index Cache section below) in addition to the normal incremental
manifest workflow.

### Persistence Model

- Incremental manifests live under
  `incremental/<input-fingerprint>/manifest.json`.
- Manifests are stored as JSON for readability and operational inspection.
- License index cache lives under `license-index/embedded/<fingerprint>.rkyv`
  or `license-index/custom/<fingerprint>.rkyv` inside the shared cache root.

### Invalidation Model

- Incremental reuse is gated by stored file metadata and verified SHA256 before
  reusing a prior file result.
- Manifest reuse is gated by a fingerprint derived from relevant scan/runtime
  settings.
- Manifest decode or compatibility failures degrade to **full rescan + rewrite**.
- License index cache is invalidated by a SHA-256 content fingerprint stored as
  a 32-byte prefix in the cache file. Mismatch triggers a full rebuild.

### Concurrency and Write Safety

- Incremental manifest writes use a sidecar lock file (`scans.lock`).
- Writes use temp-file persistence plus replace-on-completion semantics.
- `--cache-clear` is coordinated through the same lock boundary.

### What Is Stored

**Tracked in the incremental manifest**:

- file path within the scan root
- file-state fingerprint
- content SHA256
- prior `FileInfo` for reuse after validation

**Stored in the license index cache**:

- 32-byte SHA-256 content fingerprint (prefix)
- rkyv-serialized `CachedLicenseIndex` containing the full runtime index
  (token dictionaries, automatons, rules, licenses, and index structures)
- ~340 MB uncompressed on disk
- SPDX license list version string (so warm path avoids re-parsing the source)

### License Index Cache

The license index cache avoids rebuilding the `LicenseIndex` from scratch on
every run. Building the index takes ~12s (release); loading from cache takes
~0.8s (release).

**Fingerprinting strategy**:

- _Embedded rules_ (default): SHA-256 of the raw embedded artifact bytes. The
  bytes are already in memory via `include_bytes!`, so the hash computation is
  nearly free (~1ms). This allows the warm path to check the cache _before_
  doing any zstd decompression or MessagePack deserialization.
- _Custom rules_ (`--license-rules-path`): SHA-256 of the sorted rules and
  licenses (sorted by `identifier` / `key`). Sorting ensures deterministic
  output regardless of filesystem iteration order.

**Cache location**: fingerprinted license-index entries under the shared cache
root. The current implementation stores
`license-index/embedded/<fingerprint>.rkyv` and
`license-index/custom/<fingerprint>.rkyv`
entries under the shared cache root selected by `--cache-dir` /
`PROVENANT_CACHE` / platform-native defaults.

**Invalidation**: The cache is automatically invalidated when the source rules
change (fingerprint mismatch) or when `--reindex` is passed. A new provenant
binary with different embedded rules will also invalidate the cache since the
artifact bytes differ.

## Rollout Summary

| Phase | Focus                       | Delivered                                                                                |
| ----- | --------------------------- | ---------------------------------------------------------------------------------------- |
| 1     | Cache root, config, and CLI | XDG/env/CLI cache-root selection, `--cache-dir`, `--cache-clear`, `--max-in-memory`      |
| 2     | Locking and atomic writes   | sidecar lock coordination, atomic manifest persistence, lock-aware cache clearing        |
| 3     | Incremental scanning        | `--incremental`, manifest persistence, unchanged-file validation, merge-back into output |
| 4     | Polish and verification     | summary counters, docs updates, focused unit and CLI integration coverage                |

## Testing and Verification Plan

### Unit-Level Coverage

- cache-root resolution and overrides
- atomic write helpers
- lock-file coordination
- incremental manifest persistence
- unchanged-file validation using stored SHA256

### Integration-Level Coverage

- repeated scan reuses unchanged files and preserves output shape
- incompatible or corrupt manifests fall back safely to rescanning
- cache-root lifecycle controls behave correctly from the CLI

### Project-Level Verification

- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo build`
- `npm run check:docs`

## Success Criteria

- [x] Incremental scans only reuse unchanged files after validation
- [x] Manifest invalidation falls back safely to rescanning when needed
- [x] Manifest writes are safe under concurrent use of one cache root
- [x] `--cache-dir`, `--cache-clear`, and `--incremental` are wired into runtime startup
- [x] `--max-in-memory` keeps its parity-aligned scan-time role
- [x] `PROVENANT_CACHE` overrides the cache location
- [x] Platform-native XDG/ProjectDirs defaults are used when no override is supplied
- [x] Atomic persistence prevents partial manifest files from becoming visible as valid entries
- [x] Custom `--license-rules-path` scans continue to use the incremental manifest workflow
- [x] License index cache uses rkyv serialization with SHA-256 content fingerprinting
- [x] Custom `--license-rules-path` scans participate in the license index cache
- [x] `--reindex` forces a license index cache rebuild
- [x] `--no-license-index-cache` disables persistent license index cache reads/writes
- [x] License index cache lives under the shared cache root by default

## Dependencies

| Crate         | Purpose                               | Status      |
| ------------- | ------------------------------------- | ----------- |
| `sha2`        | Content hashing                       | ✅ Existing |
| `directories` | Platform-native cache-root resolution | ✅ Added    |
| `fd-lock`     | Multi-process lock coordination       | ✅ Added    |
| `rkyv`        | Zero-copy license index serialization | ✅ Added    |
| `rancor`      | rkyv error handling                   | ✅ Added    |

## Related Documents

- [ARCHITECTURE.md](../../ARCHITECTURE.md)
- [CLI_GUIDE.md](../../CLI_GUIDE.md)
- [TESTING_STRATEGY.md](../../TESTING_STRATEGY.md)
- [CLI_PLAN.md](CLI_PLAN.md)
- Python reference:
  `reference/scancode-toolkit/src/scancode_config.py`
