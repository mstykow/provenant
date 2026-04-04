# Cargo Parser Improvements

## Summary

Rust now goes beyond the released Python ScanCode Cargo handling in several concrete ways:

1. assigns ordinary crate files to their Cargo package instead of limiting ownership to `Cargo.toml` and `Cargo.lock`
2. assigns workspace member ownership for non-manifest files such as `LICENSE` and `README.md`
3. closes parser parity gaps around lowercase Cargo filenames and missing manifest fields like `readme` and `publish`
4. links manifest-declared files such as `readme` and `license-file` through explicit Cargo `file_references`
5. preserves per-dependency Cargo.lock provenance such as `source` and checksums

## Reference limitation

The Python reference covers the main Cargo surfaces, but package-wide file ownership, manifest-declared file references, and finer lockfile provenance remain incomplete.

## Rust Improvements

### Cargo package file ownership

- Plain Cargo crates now assign files under the crate root to the crate package, not only the manifest and lockfile.
- The assignment logic still skips `target/` and avoids stealing files from nested Cargo package roots.

### Workspace member ownership

- Cargo workspace member files like `crates/cli/LICENSE` and `crates/core/README.md` now stay attached to their member packages, not just to `Cargo.toml`.
- Cargo workspace roots and member directories are now claimed by topology planning before the generic directory loop runs, so member packages are built from an explicit workspace plan instead of being created by the local path and repaired afterward.

### Parser parity fixes

- `CargoParser::is_match()` now accepts lowercase `cargo.toml`.
- `CargoLockParser::is_match()` now accepts lowercase `cargo.lock`.
- Cargo.toml parsing now preserves:
  - `readme` as `extra_data.readme_file`
  - `publish` as `extra_data.publish`
- Cargo workspace readme inheritance markers are preserved in `extra_data` for downstream workspace-aware handling.
- Error-path fallback package data now keeps the correct Cargo `package_type` and `datasource_id` for both manifest and lockfile parsing.

### Manifest file references

- Cargo.toml now emits `file_references` for string-valued `license-file` and `readme` entries.
- These references are deduplicated and serialized as regular package file references, which makes downstream file-to-package linkage more useful without changing the underlying metadata fields.

### Lockfile provenance

- Cargo.lock dependency entries now preserve per-package `source` and `checksum` values in dependency `extra_data`.
- Annotated lockfile dependency strings such as `serde 1.0.228 (registry+...)` now normalize correctly to the pinned package version while still retaining provenance.
- Hoisted Cargo.lock dependencies keep this provenance when manifest and lockfile data are merged.

## Why this matters

- **Better crate ownership**: Cargo packages can claim the files users expect, not just manifest files
- **Richer manifest provenance**: declared readme and license files become explicit package references
- **Better lockfile fidelity**: dependency source and checksum data remain available for downstream analysis
