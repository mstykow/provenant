# xtask Maintainer Commands

`xtask/` is the home for Provenant's Rust-based maintainer workflows. Run these
commands directly with:

```bash
cargo run --manifest-path xtask/Cargo.toml --bin <command> -- ...
```

## Command Index

| Command                      | Purpose                                                                                        |
| ---------------------------- | ---------------------------------------------------------------------------------------------- |
| `benchmark-target`           | Measure Provenant against an explicit local or remote benchmark target.                        |
| `compare-outputs`            | Run Provenant and ScanCode on the same target and write raw plus reduced comparison artifacts. |
| `update-parser-golden`       | Regenerate parser `.expected.json` fixtures from current Rust parser output.                   |
| `update-copyright-golden`    | Maintain copyright golden YAML fixtures with parity-gated or Rust-owned update modes.          |
| `update-license-golden`      | Maintain license golden YAML fixtures with parity-gated or Rust-owned update modes.            |
| `validate-urls`              | Validate URLs in production docs and Rust docstrings.                                          |
| `check-license-headers`      | Check or repair SPDX-style headers on the repo's allowlisted first-party files.                |
| `generate-supported-formats` | Regenerate `docs/SUPPORTED_FORMATS.md` from parser metadata.                                   |
| `generate-benchmark-chart`   | Regenerate the benchmark duration-vs-files SVG from timing rows in `docs/BENCHMARKS.md`.       |
| `generate-index-artifact`    | Regenerate the embedded license index artifact from ScanCode rules and licenses.               |

## `benchmark-target`

### Purpose

`benchmark-target` measures Provenant against an explicitly supplied benchmark
target and reports a repeated-run matrix for:

- uncached runs
- incremental runs

This makes it useful for checking repeated-run speedups on unchanged input.

### Usage

```bash
cargo run --manifest-path xtask/Cargo.toml --bin benchmark-target -- --help
cargo run --manifest-path xtask/Cargo.toml --bin benchmark-target -- --repo-url https://github.com/org/repo.git --repo-ref main --profile common
cargo run --manifest-path xtask/Cargo.toml --bin benchmark-target -- --target-path /path/to/local/directory --profile common-with-compiled
cargo run --manifest-path xtask/Cargo.toml --bin benchmark-target -- --repo-url https://github.com/org/repo.git --repo-ref v1.2.3 --profile licenses
cargo run --manifest-path xtask/Cargo.toml --bin benchmark-target -- --target-path /path/to/local/directory --profile packages
cargo run --manifest-path xtask/Cargo.toml --bin benchmark-target -- --repo-url https://github.com/org/repo.git --repo-ref <sha> --profile common
cargo run --manifest-path xtask/Cargo.toml --bin benchmark-target -- --target-path /path/to/local/directory --profile common
cargo run --manifest-path xtask/Cargo.toml --bin benchmark-target -- --repo-url https://github.com/org/repo.git --repo-ref <sha> -- -clupe
cargo run --manifest-path xtask/Cargo.toml --bin benchmark-target -- --target-path /path/to/local/directory -- --timeout 300 --license-text
cargo run --manifest-path xtask/Cargo.toml --bin benchmark-target -- --target-path /path/to/local/directory -- --license --package
```

CLI arguments:

- Exactly one of `--repo-url` or one-or-more `--target-path` values is required.
- `--repo-url URL`: benchmark the given repository URL via the shared repo cache.
- `--repo-ref REF`: required with `--repo-url`; commit SHA, tag, or branch to resolve and benchmark.
- `--target-path PATH`: benchmark an existing local directory in place.
- `--profile common`: convenience shorthand for `-clupe --system-package --strip-root --processes 4`.
- `--profile common-with-compiled`: convenience shorthand for `-clupe --system-package --package-in-compiled --strip-root`.
- `--profile licenses`: convenience shorthand for `-l --strip-root`.
- `--profile packages`: convenience shorthand for `-p --strip-root`.
- Pass either a supported `--profile` or explicit benchmark scan flags after `--`.
- A common explicit profile is `-clupe` (`--copyright --license --url --package --email`).

### What It Does

1. Either scans a local directory passed via `--target-path` or resolves `--repo-url` + `--repo-ref` through a shared repo cache.
2. Builds Provenant in release mode.
3. Updates or creates a shared cached mirror under `.provenant/repo-cache/`, resolves the requested ref to a full commit SHA, and materializes a detached checkout for the run.
4. Runs cold/warm scenarios with isolated cache roots while forwarding the requested Provenant scan flags unchanged.
5. Writes a run manifest plus benchmark results under `.provenant/benchmarks/`.
6. Prints a summary table with wall time, key phase timings, peak RSS, and incremental reuse signals.

### Output

For each scenario, the command writes:

- `results/<scenario>/scan-output.json`
- `results/<scenario>/provenant-stdout.txt`
- `run-manifest.json`

It also writes a tab-separated summary file at:

- `results/summary.tsv`

### Notes

- The command uses an explicit per-scenario `--cache-dir` so incremental manifest results do not leak across scenarios.
- The command also adds `--no-license-index-cache` to every Provenant invocation so repeated benchmark runs do not inherit warmed license-index state from earlier repositories.
- `--target-path` mode scans the directory in place; it does not reset, stash, or otherwise mutate that path.
- `--repo-url` mode requires `--repo-ref`; the command resolves that ref to a full commit SHA and records the exact SHA in `run-manifest.json`.
- Warm-run comparisons are meaningful only within one invocation because the command recreates `.provenant/benchmarks` on every run.
- Benchmark artifacts are kept in the repo-local `.provenant/` developer artifact directory rather than `/tmp`, so they stay near future comparison runs and are easier to inspect before cleanup.
- Repo URL runs reuse cached git objects from `.provenant/repo-cache/` instead of recloning the upstream repository on every invocation.
- `run-manifest.json` records the Provenant binary version plus the current Provenant repository revision, dirty state, and diff hash for the run, so benchmark snapshots stay attributable as the scanner evolves.
- On macOS, the command falls back to `/usr/bin/time -l`; on systems with GNU `time`, it uses verbose memory reporting automatically.

## `compare-outputs`

### Purpose

`compare-outputs` runs the same shared scan profile through Provenant and
ScanCode. It saves both raw JSON outputs and produces reduced comparison
artifacts for later manual or agent review.

### Requirements

- Docker is required on ScanCode cache misses.
- The command builds a local ScanCode Docker image from the bundled
  `reference/scancode-toolkit` submodule automatically when the matching image
  is missing and a ScanCode run is required.

### Usage

```bash
cargo run --manifest-path xtask/Cargo.toml --bin compare-outputs -- --help
cargo run --manifest-path xtask/Cargo.toml --bin compare-outputs -- --repo-url https://github.com/org/repo.git --repo-ref main --profile common
cargo run --manifest-path xtask/Cargo.toml --bin compare-outputs -- --target-path /path/to/local/directory --profile common-with-compiled
cargo run --manifest-path xtask/Cargo.toml --bin compare-outputs -- --repo-url https://github.com/org/repo.git --repo-ref v1.2.3 --profile licenses
cargo run --manifest-path xtask/Cargo.toml --bin compare-outputs -- --target-path /path/to/local/directory --profile packages
cargo run --manifest-path xtask/Cargo.toml --bin compare-outputs -- --repo-url https://github.com/org/repo.git --repo-ref <sha> --profile common
cargo run --manifest-path xtask/Cargo.toml --bin compare-outputs -- --target-path /path/to/local/directory --profile common
cargo run --manifest-path xtask/Cargo.toml --bin compare-outputs -- --target-path /path/to/local/directory -- --license --package --strip-root
```

CLI arguments:

- Exactly one of `--repo-url` or one-or-more `--target-path` values is required.
- `--repo-url URL`: compare the given repository URL via the shared repo cache.
- `--repo-ref REF`: required with `--repo-url`; commit SHA, tag, or branch to resolve and compare.
- `--target-path PATH`: compare an existing local directory in place, or repeat the flag to stage multiple local files into one compare run.
- `--scancode-cache-identity ID`: optional with `--target-path`; opt in to shared ScanCode cache reuse for a caller-asserted local snapshot identity.
- `--profile common`: convenience shorthand for `-clupe --system-package --strip-root --processes 4`.
- `--profile common-with-compiled`: convenience shorthand for `-clupe --system-package --package-in-compiled --strip-root`.
- `--profile licenses`: convenience shorthand for `-l --strip-root`.
- `--profile packages`: convenience shorthand for `-p --strip-root`.
- Pass either a supported `--profile` or explicit shared scan flags after `--`.

### What It Does

1. Creates a per-run artifact directory under `.provenant/compare-runs/`.
2. Either scans the local directory in place or resolves `--repo-url` + `--repo-ref` through a shared repo cache.
3. Builds Provenant in release mode.
4. Updates or creates a shared cached mirror under `.provenant/repo-cache/`, resolves the requested ref to a full commit SHA, and materializes a detached checkout for the run.
5. Resolves the ScanCode runtime identity and, on cache misses, ensures a local Docker-backed ScanCode runtime exists by building the image from `reference/scancode-toolkit` if needed.
6. Reuses cached ScanCode raw artifacts when available, otherwise runs ScanCode alongside Provenant with the same shared scan profile and ephemeral license-cache directories.
7. Saves raw outputs and logs under `raw/`.
8. Produces reduced comparison artifacts under `comparison/` and prints the absolute artifact paths at the end.

### Output

Each run writes artifacts under:

- `.provenant/compare-runs/<run-id>/`

Core files:

- `run-manifest.json`
- `raw/scancode.json`
- `raw/provenant.json`
- `comparison/summary.json`
- `comparison/summary.tsv`
- `comparison/samples/*.json`

Optional diagnostic logs when available:

- `raw/scancode-stdout.txt`
- `raw/provenant-stdout.txt`

### Notes

- The command keeps the full raw scanner outputs; it does **not** stream giant machine-readable payloads to stdout.
- Stdout is reserved for progress, a reduced summary table, and the saved artifact paths.
- ScanCode currently runs via Docker on all platforms for this workflow because that is the reproducible runtime path verified in this repository.
- When `compare-outputs` actually executes either scanner, it disables persistent license-cache reuse for fairness: Provenant runs with `--no-license-index-cache`, and ScanCode uses container-local ephemeral cache directories.
- `compare-outputs` passes the same shared scan args to both scanners. The `common` profile includes installed package database coverage and fixes the shared worker count at `--processes 4`, which is usually a no-op on ordinary source repositories but matters for extracted rootfs/container trees and other artifact targets. Use `common-with-compiled` when you also want Go/Rust compiled-binary package extraction in the shared scan profile.
- For `--profile common`, the ScanCode Docker invocation also adds `--memory 12g --memory-swap 12g`, and that runtime cap is part of ScanCode cache validation.
- `--repo-url` mode requires `--repo-ref`; the command records both the requested ref and the resolved full commit SHA in `run-manifest.json`.
- `run-manifest.json` also records the Provenant binary version plus the current Provenant repository revision, dirty state, and diff hash, alongside the ScanCode runtime identity.
- Repo URL runs reuse cached git objects from `.provenant/repo-cache/`, and the temporary detached checkout is removed after the run so compare artifacts do not retain duplicate full repository trees.
- Repo URL runs also reuse cached raw ScanCode artifacts from `.provenant/scancode-cache/` when the resolved target commit, ScanCode runtime identity, and effective ScanCode scan args are unchanged.
- Local `--target-path` runs rerun ScanCode by default. Pass `--scancode-cache-identity <id>` to opt into shared ScanCode raw-artifact reuse for a local snapshot you have identified explicitly.
- For local target-path cache hits, the **path itself is not the cache identity**. The cache key is derived from your explicit `--scancode-cache-identity` plus the effective ScanCode runtime/args, so reusing the same identity across different local paths will intentionally hit the same cache entry when the staged snapshot is meant to be the same.
- For local target-path cache hits, keep all of these stable between runs: the explicit `--scancode-cache-identity`, the effective ScanCode scan args/profile, the ScanCode runtime identity (`image`, runtime revision/dirty state/diff hash, Docker platform), and the ScanCode memory limits that `compare-outputs` applies.
- Treat `--scancode-cache-identity` as a caller-owned snapshot label, not a convenience name. If the local artifact contents changed in any meaningful way, bump the identity yourself; xtask validates the manifest/runtime/args, but it does **not** hash local target contents for you.
- Repeated `--target-path` values currently support **files only** and are mainly intended for multi-input `--from-json` replay compares. The harness stages those files under one temporary input directory and passes them explicitly to both scanners in the same order.
- For repeated local `--target-path` file inputs, cache hits also depend on keeping the same file list order, because xtask stages them as ordered numbered filenames under one temporary input tree before invoking ScanCode.
- When shared scan args reference a local auxiliary file for both scanners (for example `--license-policy <path>`, `--license-rules-path <path>`, or `--custom-template <path>`), `compare-outputs` stages that file separately and rewrites the ScanCode Docker path automatically so both scanners see the same auxiliary input.
- Cache hits now require a cached `scancode.json` plus cache `manifest.json`; `scancode-stdout.txt` is reused when available but is no longer required for cache completeness.
- A quick target-path rerun checklist for expected ScanCode cache hits is: same `--scancode-cache-identity`, same `--profile` or explicit scan args, same auxiliary inputs, same local file order when repeating `--target-path`, and no ScanCode runtime change since the cache was written.
- `scancode-stdout.txt` and `provenant-stdout.txt` are best-effort diagnostic logs. The compare pipeline only requires the JSON outputs, so a log-write failure no longer makes the command fail.
- The command adds Git control-path ignore rules (`.git`, nested `.git`, and their contents) on the ScanCode side so repository metadata does not dominate the comparison artifacts without hiding package-adjacent files such as `.gitmodules`. Provenant already excludes `.git` directories during path collection by default, so xtask does not need to restate those ignores for Provenant. The harness no longer injects a blanket `target/*` ignore because some upstream repositories legitimately use `target/` as source content.

## `update-parser-golden`

`update-parser-golden` regenerates parser `.expected.json` fixtures directly from current Rust parser output.

Show CLI help:

```bash
cargo run --manifest-path xtask/Cargo.toml --bin update-parser-golden -- --help
```

CLI arguments:

- `<ParserType>`: parser struct name (for example `NpmParser`)
- `<input_file>`: fixture input file to parse
- `<output_file>`: `.expected.json` file to write
- `--list`: list all registered parser types

Example:

```bash
cargo run --manifest-path xtask/Cargo.toml --bin update-parser-golden -- <ParserType> <input_file> <output_file>
```

## `update-copyright-golden`

`update-copyright-golden` syncs and updates copyright golden YAML fixtures (authors / ics / copyrights).

Show CLI help:

```bash
cargo run --manifest-path xtask/Cargo.toml --bin update-copyright-golden -- --help
```

CLI arguments:

- `<authors|ics|copyrights>`: fixture suite to process
- `--list-mismatches`: print files where Python reference expectations differ from current Rust detector output (parity precheck)
- `--show-diff`: print missing/extra summary for those Python-reference parity mismatches (plus samples with `--filter`)
- `--filter PATTERN`: limit processing to paths containing `PATTERN`
- `--sync-actual`: write expected values from current Rust detector output
- `--write`: apply file updates (without it, command is dry-run)

`ics` here refers to the Android Ice Cream Sandwich (Android 4.0) fixture corpus from ScanCode reference tests.

Important distinction: this command is a maintenance/sync tool. Golden tests compare Rust detector output to local Rust-owned fixture YAMLs; `--list-mismatches` compares Rust detector output to Python reference expectations to decide whether a sync is parity-safe.

Expected workflow:

1. Check Python-reference parity impact first:

   ```bash
   cargo run --manifest-path xtask/Cargo.toml --bin update-copyright-golden -- copyrights --list-mismatches --show-diff
   ```

2. If parity is acceptable for a fixture, sync from Python reference:

   ```bash
   cargo run --manifest-path xtask/Cargo.toml --bin update-copyright-golden -- copyrights --filter <pattern> --write
   ```

3. If divergence is intentional or Rust-specific, update to Rust actuals:

   ```bash
   cargo run --manifest-path xtask/Cargo.toml --bin update-copyright-golden -- copyrights --sync-actual --filter <pattern> --write
   ```

## `update-license-golden`

`update-license-golden` syncs and updates license golden YAML fixtures.

Show CLI help:

```bash
cargo run --manifest-path xtask/Cargo.toml --bin update-license-golden -- --help
```

CLI arguments:

- `--list-mismatches` (`--list-diffs` alias): print files where Python reference expectations differ from current Rust detector output (parity precheck)
- `--show-diff`: print detailed diff for those mismatches
- `--filter PATTERN`: limit processing to paths containing `PATTERN`
- `--suite SUITE`: process only one suite (lic1, lic2, lic3, lic4, external, unknown)
- `--sync-actual`: write expected values from current Rust detector output
- `--write`: apply file updates (without it, command is dry-run)

Expected workflow:

1. Check Python-reference parity impact first:

   ```bash
   cargo run --manifest-path xtask/Cargo.toml --bin update-license-golden -- --list-mismatches --show-diff
   ```

2. If parity is acceptable for a fixture, sync from Python reference:

   ```bash
   cargo run --manifest-path xtask/Cargo.toml --bin update-license-golden -- --suite lic1 --filter <pattern> --write
   ```

3. If divergence is intentional or Rust-specific, update to Rust actuals:

   ```bash
   cargo run --manifest-path xtask/Cargo.toml --bin update-license-golden -- --sync-actual --suite unknown --filter <pattern> --write
   ```

## `validate-urls`

`validate-urls` systematically validates all URLs in production documentation and Rust docstrings.

Manual run:

```bash
cargo run --quiet --manifest-path xtask/Cargo.toml --bin validate-urls -- --root .
```

Exit codes:

- `0`: all URLs valid
- `1`: some URLs failed validation

This command is informational in CI and does not block PRs.

## `check-license-headers`

`check-license-headers` checks or repairs the repo's SPDX-style header rollout
for first-party code and automation files.

The current rollout intentionally covers repo-owned, comment-friendly files
such as Rust sources, selected shell scripts, and GitHub workflow/action YAML.
It intentionally excludes `reference/**`, `testdata/**`,
`resources/license_detection/**`, and generated docs such as
`docs/SUPPORTED_FORMATS.md`.

Scope rules live in `.license-headers.toml` with explicit `include` and
`exclude` lists over repo-root-relative glob patterns.

Lefthook checks staged in-scope files without rewriting them. If a header is
missing, repair it explicitly with the `--fix` form below.

The header intentionally uses a holder-only copyright line:

```text
SPDX-FileCopyrightText: Provenant contributors
SPDX-License-Identifier: Apache-2.0
```

This avoids pointless repo-wide churn from updating years every calendar year.

Examples:

```bash
cargo run --manifest-path xtask/Cargo.toml --bin check-license-headers -- --check
cargo run --manifest-path xtask/Cargo.toml --bin check-license-headers -- --fix
cargo run --manifest-path xtask/Cargo.toml --bin check-license-headers -- --fix src/lib.rs .github/workflows/check.yml
```

## `generate-supported-formats`

`generate-supported-formats` regenerates `docs/SUPPORTED_FORMATS.md` from parser metadata.

Examples:

```bash
cargo run --manifest-path xtask/Cargo.toml --bin generate-supported-formats
cargo run --manifest-path xtask/Cargo.toml --bin generate-supported-formats -- --check
```

## `generate-benchmark-chart`

`generate-benchmark-chart` regenerates `docs/benchmarks/scan-duration-vs-files.svg` and refreshes the headline benchmark summary stats in `docs/BENCHMARKS.md` from the benchmark timing rows in that document.

The command computes and prints:

- number of recorded runs
- number of runs where Provenant is faster
- median speedup
- geometric-mean speedup
- median speedup on sub-100-file targets
- median speedup on 10k+-file targets

`--check` verifies both the checked-in SVG and the BENCHMARKS headline summary line.

Examples:

```bash
cargo run --manifest-path xtask/Cargo.toml --bin generate-benchmark-chart
cargo run --manifest-path xtask/Cargo.toml --bin generate-benchmark-chart -- --check
```

## `generate-index-artifact`

`generate-index-artifact` regenerates the embedded license index artifact from ScanCode rules and licenses.
The generated artifact reflects the checked-in build policy at
`resources/license_detection/index_build_policy.toml`, so policy changes should
be committed alongside the regenerated `license_index.zst` artifact.
Downstream add/replace overlays live as regular `.RULE` / `.LICENSE` files under
`resources/license_detection/overlay/`, and the generated artifact embeds
provenance that surfaces in structured scan output headers.
If upstream absorbs one of these local curations, artifact generation now fails
fast so maintainers remove the redundant ignore/overlay instead of silently
shipping dead policy.

Examples:

```bash
cargo run --manifest-path xtask/Cargo.toml --bin generate-index-artifact
cargo run --manifest-path xtask/Cargo.toml --bin generate-index-artifact -- --check
```
