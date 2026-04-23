# License header tooling

`tools/license-headers/` checks or repairs Provenant's SPDX-style header rollout
for first-party code and automation files.

## Why this is a standalone crate

This tool runs on a hot developer path: the local pre-commit hook and a fast CI
check. It is intentionally kept outside `xtask/` so routine header checks do
not inherit the heavier `xtask -> provenant-cli` dependency graph during
release-version bumps or other main-crate rebuilds.

Use a standalone tool crate only when all of these are true:

- the tool is small and self-contained
- it does not need `provenant-cli` internals or the repo-built `provenant`
  binary
- package-boundary isolation materially improves a hot path such as pre-commit
  hooks or fast CI checks

Everything else should stay in `xtask/`, which remains the default home for
maintainer workflows coupled to scanner internals, parser metadata, golden
maintenance, artifact generation, or benchmark/compare orchestration.

## Commands

```bash
cargo run --quiet --locked --manifest-path tools/license-headers/Cargo.toml -- --check
cargo run --quiet --locked --manifest-path tools/license-headers/Cargo.toml -- --fix
```

The checked file scope lives in [`../../.license-headers.toml`](../../.license-headers.toml).
