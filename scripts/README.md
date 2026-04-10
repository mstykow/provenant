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

## `check_dependency_policy.sh`

Run `cargo-deny` against the shipped workspace dependency graph using the
repo-root `deny.toml` policy.

Example:

```bash
./scripts/check_dependency_policy.sh
```

## `check_crate_size.sh`

Package the crate locally and fail if the resulting `.crate` archive exceeds the
crates.io size limit.

Example:

```bash
./scripts/check_crate_size.sh
```

## `check_release_version_sync.sh`

Verify that the crate version in `Cargo.toml`, the packaged `provenant-cli`
entry in the lockfile, and `CITATION.cff` all stay aligned for releases.

Example:

```bash
./scripts/check_release_version_sync.sh
```

## `check_scancode_output_format_sync.sh`

Verify that Provenant's `OUTPUT_FORMAT_VERSION` stays aligned with the pinned
`reference/scancode-toolkit/` submodule.

Example:

```bash
./scripts/check_scancode_output_format_sync.sh
```
