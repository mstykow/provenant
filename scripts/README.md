# Shell Scripts Documentation

This directory now contains only real shell helpers.

Rust-based maintainer commands such as `benchmark-target`, `compare-outputs`,
`update-parser-golden`, `update-license-golden`, `update-copyright-golden`,
`validate-urls`, `generate-supported-formats`, and `generate-index-artifact`
are documented in [`../xtask/README.md`](../xtask/README.md).

The standalone SPDX header checker lives in
[`../tools/license-headers/README.md`](../tools/license-headers/README.md).

## `cargo_sort_manifests.sh`

Sort Cargo manifest sections with `cargo-sort`.

Examples:

```bash
./scripts/cargo_sort_manifests.sh
./scripts/cargo_sort_manifests.sh --check
./scripts/cargo_sort_manifests.sh Cargo.toml tools/license-headers/Cargo.toml xtask/Cargo.toml
```

## `check_unused_deps.sh`

Run `cargo-machete` against the root workspace plus the standalone
`tools/license-headers/` and `xtask/` manifests.

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

## `check_dco_signoff.sh`

Validate that a commit message includes a Developer Certificate of Origin (DCO)
sign-off trailer.

Examples:

```bash
./scripts/check_dco_signoff.sh --commit-msg-file .git/COMMIT_EDITMSG
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
