# Releasing Provenant

This guide documents the maintainer release flow for `provenant`.

## Overview

Releases are driven locally with `release.sh`, which wraps `cargo release`, refreshes the embedded license data, and checks for ScanCode output-format drift before publishing.

The published crate name is `provenant-cli`, while the installed binary and product name remain `provenant` / Provenant.

## Prerequisites

Before cutting a release, make sure you have:

- A clean working tree
- The `reference/scancode-toolkit/` submodule initialized via `./setup.sh`
- `cargo-release` installed locally
- `cargo-deny` installed locally if you want to run the full dependency policy preflight
- A valid crates.io login in your Cargo credentials
- GPG signing configured for git tags

Install `cargo-release` if needed:

```sh
cargo install cargo-release
```

Authenticate with crates.io if needed:

```sh
cargo login
```

## Preflight Checks

Before the actual release, verify the repository is in good shape:

```sh
npm ci
npm run check:docs
npm run validate:urls
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo check --all --verbose
cargo test --doc --release --verbose
./scripts/check_dependency_policy.sh
cargo test --lib --release --verbose -- --skip _scan_test::
cargo test --lib --release --verbose _scan_test::
cargo test --test scanner_integration --release --verbose
cargo test --test output_format_golden --release --verbose
cargo run --quiet --locked --manifest-path xtask/Cargo.toml --bin generate-supported-formats -- --check
./scripts/check_release_version_sync.sh
./scripts/check_scancode_output_format_sync.sh
```

The GitHub `Quality Checks` workflow is the primary pre-release quality gate. It verifies the embedded license index, ScanCode output-format version sync, dependency policy via `cargo-deny`, crate size, manifest sorting, unused dependencies, golden-test shards, Windows and Intel macOS build smoke, and the split integration-test matrix defined in `.github/workflows/check.yml`. It is best to start from a branch and commit state where that workflow is already green. The tag-triggered release workflow adds final embedded-license-index and output-format-sync verification before it builds and publishes release artifacts.

## Release Commands

Always start with a dry run:

```sh
./release.sh patch
```

When the dry run looks correct, perform the real release:

```sh
./release.sh patch --execute
```

Supported release types:

- `patch` updates `X.Y.Z` to `X.Y.(Z+1)`
- `minor` updates `X.Y.Z` to `X.(Y+1).0`
- `major` updates `X.Y.Z` to `(X+1).0.0`

## What `release.sh` Does

On every release attempt, the script:

1. Checks that the ScanCode reference submodule is present.
2. Fetches the latest `origin/develop` for `reference/scancode-toolkit`.
3. Updates the submodule checkout if the upstream commit changed.
4. Verifies that Provenant's output-format version is still aligned with the pinned ScanCode submodule and stops early if contract updates are required.
5. Regenerates `resources/license_detection/license_index.zst` from the pinned ScanCode dataset plus the checked-in build policy manifest at `resources/license_detection/index_build_policy.toml` and any local overlay files under `resources/license_detection/overlay/`.
6. In `--execute` mode, commits that license-data refresh as `chore: update license rules/licenses to latest` with `git commit -s` when needed.
7. Confirms the overall release once, then runs the `cargo release` step subcommands in order: `version`, `replace`, `hook`, manual `git commit -s`, then `publish`, `tag`, and `push` with cargo-release's per-step confirmations suppressed.

The repository is configured so the `cargo release` steps used by `release.sh`:

- Rewrites `CITATION.cff` so its `version` field matches the release version
- Regenerates the workspace `Cargo.lock` after bumping the crate version and before creating the release commit
- Creates a GPG-signed tag `vX.Y.Z`
- Publishes the crate to crates.io
- Pushes the commit and tag to GitHub

The release commit created by `release.sh` is intentionally versionless
(`chore: release`) and is written with `git commit -s` so the release flow stays
DCO compliant.

## GitHub Release Automation

Pushing the `vX.Y.Z` tag triggers `.github/workflows/release.yml`.

That workflow:

- Builds release binaries for:
  - `x86_64-unknown-linux-gnu`
  - `aarch64-unknown-linux-gnu`
  - `x86_64-apple-darwin`
  - `aarch64-apple-darwin`
  - `x86_64-pc-windows-msvc`
- Re-runs embedded license index verification as a final release-time safeguard before building artifacts
- Packages each build with platform-first asset names so archives sort by operating system on the release page:
  - `provenant-linux-x86_64.tar.gz`
  - `provenant-linux-aarch64.tar.gz`
  - `provenant-macos-x86_64.tar.gz`
  - `provenant-macos-aarch64.tar.gz`
  - `provenant-windows-x86_64.zip`
- Generates SHA256 checksum files
- Creates a GitHub Release and uploads all generated assets

If the tag contains `-`, GitHub marks the release as a prerelease.

## After Starting the Release

Monitor the [GitHub Actions release workflow](https://github.com/mstykow/provenant/actions) and the resulting [GitHub Releases page](https://github.com/mstykow/provenant/releases).

Verify:

- The crates.io publish step succeeded
- The tag and release commit are present on the remote
- The GitHub Release contains all expected Linux, macOS (Intel and Apple Silicon), and Windows archives and checksum files

## Common Failure Points

- Missing submodule setup: run `./setup.sh`
- Missing crates.io credentials: run `cargo login`
- Missing GPG configuration: `cargo release` cannot create the signed tag
- Dirty working tree: clean up local changes before retrying
