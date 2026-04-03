# Shell Scripts Documentation

This directory now contains only real shell helpers.

Rust-based maintainer commands such as `benchmark-target`, `compare-outputs`,
`update-parser-golden`, `update-license-golden`, `update-copyright-golden`,
`validate-urls`, `generate-supported-formats`, and `generate-index-artifact`
are documented in [`../xtask/README.md`](../xtask/README.md).

## `cargo_sort_manifests.sh`

Sort Cargo manifest sections with `cargo-sort`.

Examples:

```bash
./scripts/cargo_sort_manifests.sh
./scripts/cargo_sort_manifests.sh --check
./scripts/cargo_sort_manifests.sh Cargo.toml xtask/Cargo.toml
```

## `check_unused_deps.sh`

Run `cargo-machete` against the root workspace and `xtask/` manifest.

Example:

```bash
./scripts/check_unused_deps.sh
```

## `check_crate_size.sh`

Package the crate locally and fail if the resulting `.crate` archive exceeds the
crates.io size limit.

Example:

```bash
./scripts/check_crate_size.sh
```

## `check_xtask_lockfile_sync.sh`

Verify that the xtask lockfile view of `provenant-cli` matches the root crate
version in `Cargo.toml`.

Example:

```bash
./scripts/check_xtask_lockfile_sync.sh
```
