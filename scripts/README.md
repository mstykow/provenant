# Scripts Documentation

## Benchmark Script

### Purpose

`benchmark.sh` measures Provenant against an explicitly supplied benchmark target
and reports a repeated-run matrix for:

- uncached runs
- incremental runs

This makes it useful for checking repeated-run speedups on unchanged input.

### Usage

```bash
./scripts/benchmark.sh --repo-url https://github.com/org/repo.git -- -clupe
./scripts/benchmark.sh --repo-url https://github.com/org/repo.git --repo-commit <sha> -- -clupe
./scripts/benchmark.sh --target-path /path/to/local/directory -- -clupe
./scripts/benchmark.sh --target-path /path/to/local/directory -- --timeout 300 --license-text
./scripts/benchmark.sh --target-path /path/to/local/directory -- --license --package
```

CLI arguments:

- Exactly one of `--repo-url` or `--target-path` is required.
- `--repo-url URL`: clone and benchmark the given repository URL
- `--target-path PATH`: benchmark an existing local directory in place
- `--repo-commit SHA`: check out a specific revision after cloning `--repo-url`
- Benchmark scan flags are required after `--` and are forwarded to Provenant unchanged.
- A common profile is `-clupe` (`--copyright --license --url --package --email`).

### What It Does

1. Either clones the repository supplied via `--repo-url` or scans a local directory passed via `--target-path`
2. Builds Provenant in release mode
3. Runs cold/warm scenarios with isolated cache roots while forwarding the requested Provenant scan flags unchanged
4. Captures per-scenario output under `/tmp/provenant-benchmark/results/`
5. Prints a summary table with wall time, key phase timings, peak RSS, and
   incremental reuse signals

### Output

For each scenario, the script writes:

- `results/<scenario>/scan-output.json`
- `results/<scenario>/provenant-stdout.txt`

It also writes a tab-separated summary file at:

- `results/summary.tsv`

### Notes

- The script uses an explicit per-scenario `--cache-dir` so incremental manifest
  results do not leak across scenarios.
- `--target-path` mode scans the directory in place; it does not reset, stash, or
  otherwise mutate that path.
- Warm-run comparisons are meaningful only within one invocation because the
  script recreates `/tmp/provenant-benchmark` on every run.
- On macOS, the script falls back to `/usr/bin/time -l`; on systems with GNU
  `time`, it uses verbose memory reporting automatically.

## Golden Fixture Helper Scripts

### Parser Golden Snapshots

`update_parser_golden.sh` updates parser `.expected.json` golden snapshots by invoking the `update-parser-golden` binary from the unpublished `xtask/` helper crate.

**Why this exists**: it regenerates parser golden expectations directly from parser output so fixture updates stay deterministic and aligned with parser behavior.

Show CLI help:

```bash
cargo run --manifest-path xtask/Cargo.toml --bin update-parser-golden -- --help
./scripts/update_parser_golden.sh --help
```

CLI arguments:

- `<ParserType>`: parser struct name (for example `NpmParser`)
- `<input_file>`: fixture input file to parse
- `<output_file>`: `.expected.json` file to write
- `--list`: list all registered parser types

```bash
./scripts/update_parser_golden.sh <ParserType> <input_file> <output_file>
```

### Copyright Golden YAML Fixtures

`update_copyright_golden.sh` syncs and updates copyright golden YAML fixtures (authors / ics / copyrights) by invoking the `update-copyright-golden` binary from the unpublished `xtask/` helper crate.

**Why this exists**: it keeps copyright golden YAMLs in sync with the Rust detector and with Rust-owned fixture policy while preserving reviewable mismatch workflows.

Show CLI help:

```bash
cargo run --manifest-path xtask/Cargo.toml --bin update-copyright-golden -- --help
./scripts/update_copyright_golden.sh --help
```

CLI arguments:

- `<authors|ics|copyrights>`: fixture suite to process
- `--list-mismatches`: print files where Python reference expectations differ from current Rust detector output (parity precheck)
- `--show-diff`: print missing/extra summary for those Python-reference parity mismatches (plus samples with `--filter`)
- `--filter PATTERN`: limit processing to paths containing `PATTERN`
- `--sync-actual`: write expected values from current Rust detector output
- `--write`: apply file updates (without it, command is dry-run)

`ics` here refers to the Android Ice Cream Sandwich (Android 4.0) fixture corpus from ScanCode reference tests.

The updater also removes legacy `expected_failures` keys, preventing Python xfail metadata from being reintroduced into Rust-owned fixtures.

Important distinction: this command is a maintenance/sync tool. Golden tests compare Rust detector output to local Rust-owned fixture YAMLs; `--list-mismatches` compares Rust detector output to Python reference expectations to decide whether a sync is parity-safe.

### Expected Workflow (Copyright Fixtures)

Use this workflow when maintaining `testdata/copyright-golden/*`:

1. **Check Python-reference parity impact first**
   - Run:

     ```bash
     ./scripts/update_copyright_golden.sh copyrights --list-mismatches --show-diff
     ```

   - Purpose: identify fixtures where current Rust output diverges from upstream Python reference expectations.

2. **If parity is acceptable for a fixture, sync from Python reference**
   - Run with `--write` (optionally with `--filter`):

     ```bash
     ./scripts/update_copyright_golden.sh copyrights --filter <pattern> --write
     ```

   - This is a **selective, parity-gated sync** from `reference/scancode-toolkit/tests/cluecode/data/...`.

3. **If divergence is intentional or Rust-specific, update to Rust actuals**
   - Run:

     ```bash
     ./scripts/update_copyright_golden.sh copyrights --sync-actual --filter <pattern> --write
     ```

   - Use this for accepted Rust improvements or known intentional differences.

4. **Validate with tests**
   - Run golden tests after updates to confirm repository expectations are coherent.

Notes:

- Python sync workflow applies to **copyright fixtures only**.
- Parser golden updater (`update-parser-golden`) does **not** sync from Python reference; it always generates expectations from Rust parser output.

```bash
./scripts/update_copyright_golden.sh <authors|ics|copyrights> [--list-mismatches] [--show-diff] [--filter PATTERN] [--write]
```

Useful examples:

```bash
./scripts/update_copyright_golden.sh copyrights --list-mismatches --show-diff
./scripts/update_copyright_golden.sh copyrights --filter essential_smoke --write
```

### License Golden YAML Fixtures

`update_license_golden.sh` syncs license golden YAML fixtures from the Python reference by invoking the `update-license-golden` binary from the unpublished `xtask/` helper crate.

**Why this exists**: it keeps license detection golden YAMLs in sync with the Python reference when the reference submodule is updated.

Show CLI help:

```bash
cargo run --manifest-path xtask/Cargo.toml --bin update-license-golden -- --help
./scripts/update_license_golden.sh --help
```

CLI arguments:

- `--list-diffs`: print files where our expected values differ from Python reference
- `--show-diff`: print detailed diff for differing files
- `--filter PATTERN`: limit processing to paths containing `PATTERN`
- `--suite SUITE`: process only one suite (lic1, lic2, lic3, lic4, external, unknown)
- `--write`: apply file updates (without it, command is dry-run)

```bash
./scripts/update_license_golden.sh [--list-diffs] [--show-diff] [--filter PATTERN] [--suite SUITE] [--write]
```

Useful examples:

```bash
./scripts/update_license_golden.sh --list-diffs
./scripts/update_license_golden.sh --suite lic1 --list-diffs --show-diff
./scripts/update_license_golden.sh --filter bsd --write
```

## URL Validation Tool

### Purpose

`validate-urls` systematically validates all URLs in production documentation and Rust docstrings to catch broken links before they reach users. The binary lives in the unpublished `xtask/` helper crate so it stays out of the published crate package.

### What It Validates

**Included**:

- All markdown files in `docs/`
- Root markdown files: `README.md`, `AGENTS.md`, etc.
- Rust docstrings (`///` and `//!`) in `src/`

**Excluded** (not our responsibility):

- `reference/` - Python ScanCode Toolkit submodule (upstream)
- `testdata/` - Test fixtures and sample data
- `target/` - Build artifacts
- `.sisyphus/` - Session data
- Test files: `*_test.rs`, files in `tests/` directories

### Usage

```bash
# Manual run
cargo run --quiet --manifest-path xtask/Cargo.toml --bin validate-urls -- --root .

# Exit codes:
#   0 = All URLs valid
#   1 = Some URLs failed validation
```

### Output

The script provides:

- Progress updates during validation
- Failed URLs with file locations
- Summary statistics
- Skipped URLs (templates, placeholders)

Example output:

```text
❌ FAIL: https://example.com/broken
   Reason: HTTP 404
   Found in:
     - docs/ARCHITECTURE.md:42

✅ 137 URLs validated successfully
❌ 3 URLs failed validation
```

### CI/CD Integration

**Configured in** `.github/workflows/check.yml`:

```yaml
- name: Validate Documentation URLs
  run: cargo run --quiet --manifest-path xtask/Cargo.toml --bin validate-urls -- --root .
  continue-on-error: true # Informational only - doesn't block PRs
```

Runs on every push to `main` and every pull request.

**Note**: URL validation is informational only and does not block PRs. This prevents contributors from being blocked by:

- URLs that don't exist yet on remote (unpushed changes)
- Sites blocking CI user agents (e.g., crates.io)
- Transient network failures

### When It Reports Failures

If the check reports broken URLs:

1. **Review the output** - Check which URLs are broken
2. **Fix actual broken links** - Update or remove genuinely broken URLs in our docs
3. **Ignore expected failures**:

- URLs to unpushed GitHub paths (will resolve after push)
- crates.io URLs (blocks CI user agents, validated in allowlist)
- Submodule URLs in `reference/` or `resources/` (not our responsibility)

### Maintenance

**No regular maintenance needed** - The tool automatically:

- Skips template URLs (containing `{`, `<`, `...`)
- Handles relative URLs and fragments
- Validates in parallel (10 concurrent requests)
- Times out after 10 seconds per URL

---
