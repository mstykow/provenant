# Progress Tracking & Reporting Implementation Plan

> **Status**: 🟢 Historical rollout record — implemented and validated with tests and quality gates
> **Current contract owner**: [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) for current runtime/progress behavior and [`../../RELEASING.md`](../../RELEASING.md) / CI for verification expectations
> **Historical priority / effort**: P3, completed
> **Dependencies at implementation time**: None (integrated with the existing scanner pipeline)

## Table of Contents

- [Overview](#overview)
- [Python Reference Analysis](#python-reference-analysis)
- [Current State in Rust](#current-state-in-rust)
- [Rust Architecture Design](#rust-architecture-design)
- [Implementation Phases](#implementation-phases)
- [Beyond-Parity Improvements](#beyond-parity-improvements)
- [Testing Strategy](#testing-strategy)
- [Success Criteria](#success-criteria)

---

## Overview

This document is retained as implementation history for the progress/reporting rollout. Phase plans and success criteria below are historical and should not be treated as the current source of truth for runtime behavior.

Enhanced progress reporting during scans: multi-phase progress bars, ETA, throughput metrics, real-time status updates, verbosity control, and scan summary statistics. The system must integrate cleanly with rayon parallel processing, avoid interfering with structured JSON output, and support quiet/verbose/default modes.

### Scope

**In Scope:**

- **Multi-phase progress bars**: File discovery, scanning, assembly, output writing
- **ETA calculation**: Estimated time to completion (built-in to indicatif)
- **Throughput metrics**: Files/second, bytes/second in scan summary
- **Verbosity control**: `--quiet` (suppress all), `--verbose` (TTY-focused file-by-file details with bounded non-TTY output)
- **Scan summary**: Statistics displayed at end of scan (counts, timings, speed)
- **Error display**: Real-time error reporting alongside progress bars
- **Logging integration**: Log messages printed above progress bars (not corrupting them)
- **Terminal detection**: Automatic TTY detection, graceful degradation when piped

**Out of Scope:**

- Runtime plugin system for custom progress reporters (intentionally deferred; see plugin system plan)
- Streaming JSON output during scan (deferred to output formats plan)
- Per-scanner timing (we have a monolithic pipeline, not a plugin-based one)
- Remote progress reporting (webhooks, etc.)

---

## Python Reference Analysis

### Source Files

| File                                                   | Role                                       |
| ------------------------------------------------------ | ------------------------------------------ |
| `reference/scancode-toolkit/src/scancode/cli.py`       | Main CLI with all progress orchestration   |
| `commoncode.cliutils` (external)                       | `progressmanager`, `path_progress_message` |
| `reference/scancode-toolkit/src/scancode/interrupt.py` | Per-file timeout handling                  |
| `reference/scancode-toolkit/src/scancode/pool.py`      | Multiprocessing pool management            |

### Python Architecture: 7 Scanning Phases

Python ScanCode runs through 7 sequential phases, each with its own progress reporting:

```text
1. Inventory     → "Collect file inventory..."  (no progress bar)
2. Setup         → Per-plugin setup messages    (verbose only)
3. Pre-Scan      → Stage/plugin messages        (verbose only)
4. Scan          → Progress bar OR file listing  (main phase)
5. Post-Scan     → Stage/plugin messages        (verbose only)
6. Output Filter → Stage/plugin messages        (verbose only)
7. Output        → "Save scan results as: ..."  (verbose only)
```

Only phase 4 (Scan) gets a progress bar. All other phases use simple text messages.

### Verbosity Modes

Python supports three mutually exclusive output modes:

| Mode        | Flag               | Behavior                                                     |
| ----------- | ------------------ | ------------------------------------------------------------ |
| **Quiet**   | `-q` / `--quiet`   | No progress, no summary, no messages (only errors in red)    |
| **Default** | (none)             | Progress bar for scan phase + summary at end                 |
| **Verbose** | `-v` / `--verbose` | File-by-file path listing instead of bar + detailed messages |

Key detail: `--quiet` and `--verbose` are **mutually exclusive** (enforced by Click's `conflicting_options`).

### Progress Bar Implementation

Python uses `commoncode.cliutils.progressmanager` — a wrapper around Click's progress bar that:

1. Wraps an iterator of `(location, path, scan_errors, scan_time, scan_result, scan_timings)` tuples
2. Displays progress as items are consumed (one increment per file)
3. Uses `item_show_func=path_progress_message` to show current file in verbose mode
4. Outputs to **stderr** (never stdout, to avoid corrupting JSON output)
5. Supports context manager protocol (`__enter__`, `render_finish`)

### Scan Summary Statistics

At scan completion, Python displays:

```text
Summary:        licenses, copyrights with 4 process(es)
Errors count:   2
Scan Speed:     42.5 files/sec. 1.23 MB/sec.
Initial counts: 1500 resource(s): 1200 file(s) and 300 directorie(s) for 45.2 MB
Final counts:   1450 resource(s): 1150 file(s) and 300 directorie(s) for 44.8 MB
Timings:
  scan_start: <timestamp>
  scan_end:   <timestamp>
  setup: <seconds>
  scan: <seconds>
  post-scan: <seconds>
  output: <seconds>
  total: <seconds>
```

### Error Collection

Python accumulates errors at two levels:

1. **Codebase-level errors**: `codebase.errors` list (global errors)
2. **Resource-level errors**: `resource.scan_errors` list (per-file errors)

At completion, `collect_errors()` gathers both into a combined list. Verbose mode includes stack traces; normal mode shows only the error message.

### Timing Infrastructure

Python tracks timing at multiple granularities:

| Scope    | Storage                 | Contents                                                        |
| -------- | ----------------------- | --------------------------------------------------------------- |
| Phase    | `codebase.timings`      | `{phase_name: seconds}` (setup, scan, post-scan, output, total) |
| Plugin   | `codebase.timings`      | `{stage:plugin_name: seconds}`                                  |
| Per-file | `resource.scan_timings` | `{scanner_name: seconds}` (only if `--timing` flag)             |

### Counters

Python tracks counts at multiple stages:

| Stage    | Counter Keys                                                              |
| -------- | ------------------------------------------------------------------------- |
| Initial  | `initial:files_count`, `initial:dirs_count`, `initial:size_count`         |
| Pre-scan | `pre-scan-scan:files_count`, `pre-scan-scan:size_count`                   |
| Scan     | `scan:files_count`, `scan:dirs_count`, `scan:size_count`, `scan:scanners` |
| Final    | `final:files_count`, `final:dirs_count`, `final:size_count`               |

### Human-Readable Size Formatting

Python formats sizes with a `format_size()` function: `0 Byte → 1 Byte → 123 Bytes → 1 KB → 2.45 MB → 1 GB → 12.30 TB`.

### Known Python Limitations

1. **No progress for most phases**: Only the scan phase gets a progress bar. Setup, pre-scan, post-scan, output filter, and output phases show only text messages (and many have `# TODO: add progress indicator` comments).
2. **No throughput in progress bar**: Files/sec and bytes/sec only appear in the final summary, not during scanning.
3. **Single progress bar**: No multi-bar showing different phases simultaneously.
4. **No ETA in verbose mode**: File-by-file listing has no ETA estimate.
5. **Coarse error display**: Errors shown at end, not inline during scanning.
6. **No color in progress bar**: Only summary and error messages use color (via Click's `secho`).

---

## Current State in Rust

### Implemented

The progress/reporting path is now implemented around a centralized manager:

- **`src/progress.rs`**: `ScanProgress`, `ProgressMode`, `ScanStats`, `format_size`, phase timing, TTY/color handling, summary rendering.
- **`src/main.rs`**: Progress lifecycle wiring for discovery → SPDX load → scan → assembly → output → summary.
- **`src/scanner/process/orchestrator.rs` + `src/scanner/process/pipeline.rs`**: Per-file progress callbacks via `progress.file_completed(...)`, mode-aware error display, runtime error reporting.
- **`src/scanner/count.rs`**: `count_with_size(...)` for initial file/dir/excluded/bytes statistics.
- **Logging bridge**: `indicatif-log-bridge` + runtime `env_logger` initialization.

### Implemented Behavior Matrix

| Capability                 | Status | Notes                                                                                                                                                                          |
| -------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Quiet/default/verbose UX   | ✅     | Quiet suppresses stderr output, default shows progress + summary, verbose prints per-file paths on TTY and keeps bounded non-TTY output with detailed per-file warnings/errors |
| Multi-phase progress       | ✅     | Discovery/SPDX/assembly/output phase indicators + scan progress bar                                                                                                            |
| Throughput + summary stats | ✅     | Files/sec, bytes/sec, initial/final counts with sizes, package assembly counts, phase timings                                                                                  |
| Real-time error display    | ✅     | Inline stderr reporting during scan, plus end-of-scan error section                                                                                                            |
| Logging integration        | ✅     | Startup/global warnings use the logger bridge, while file-scoped parser/runtime failures flow through `scan_errors`                                                            |
| Non-TTY degradation        | ✅     | No progress redraw artifacts when stderr is redirected                                                                                                                         |

### Dependencies

```toml
indicatif
indicatif-log-bridge
log
env_logger
chrono
```

---

## Rust Architecture Design

### Design Decisions

#### D1: Use `indicatif` with `MultiProgress` for Multi-Phase Tracking

**Decision**: Use `MultiProgress` to manage phase-specific progress bars.

**Rationale**: indicatif is already a dependency, has native rayon support via `ParallelProgressIterator`, and `MultiProgress` supports hierarchical bar management. It is the de facto standard for Rust CLI progress (6.7M downloads/month, used in 5,190+ crates).

**Alternative considered**: Custom progress trait with atomic counters. Rejected because indicatif already handles terminal-aware drawing and thread-safe updates; refresh is rate-limited by default (20 Hz for `ProgressBar`, 15 Hz for `MultiProgress`).

#### D2: Three Verbosity Levels via `--quiet` and `--verbose`

**Decision**: Match Python's three modes — quiet, default, verbose — as mutually exclusive flags.

**Rationale**: Feature parity with Python ScanCode. Users expect the same CLI UX.

**Implementation**: Use `clap`'s conflict mechanism:

```rust
#[arg(short, long, conflicts_with = "verbose")]
pub quiet: bool,

#[arg(short, long, conflicts_with = "quiet")]
pub verbose: bool,
```

#### D3: Stderr for All Progress, Stdout Reserved for Structured Output

**Decision**: All progress bars, messages, and summaries go to stderr. Stdout is reserved for future stdout-based output modes.

**Rationale**: Python does this. It prevents progress from corrupting JSON when output goes to stdout. indicatif defaults to stderr already.

#### D4: Use `indicatif-log-bridge` for Log Integration

**Decision**: Wire `log` crate through `indicatif-log-bridge` for startup/global warnings, and route parser/file-scoped scan failures into `scan_errors` so progress modes can render them consistently.

**Rationale**: Global warnings still need clean stderr rendering, but parser read/parse failures should also survive into serialized scan output. Keeping per-file failures in `scan_errors` lets default mode stay concise, verbose mode stay forensic, and CI logs match the JSON error surface.

**New dependency**: `indicatif-log-bridge`

#### D5: Centralized `ScanProgress` Struct — Not a Trait

**Decision**: Use a concrete `ScanProgress` struct rather than a `trait ProgressReporter`.

**Rationale**: We do not plan to build a runtime plugin system in current Provenant scope, and the progress reporting has exactly one consumer (the CLI). A trait would add abstraction without benefit. If that product decision ever changes, we can extract a trait then.

#### D6: Move `env_logger` to Regular Dependencies

**Decision**: Move `env_logger` from dev-dependencies to dependencies. It's lightweight and needed for `RUST_LOG` support in production.

**Rationale**: Enables `RUST_LOG=debug provenant ...` for troubleshooting without recompilation. The bridge requires a concrete logger implementation at runtime.

### Data Structures

#### `ScanProgress` — Central Progress Manager

```rust
use indicatif::{MultiProgress, ProgressBar};

pub struct ScanProgress {
    mode: ProgressMode,
    multi: MultiProgress,
    scan_bar: ProgressBar,
    stats: Mutex<ScanStats>,
    phase_starts: Mutex<HashMap<&'static str, Instant>>,
    phase_spinner: Mutex<Option<ProgressBar>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ProgressMode {
    Quiet,
    Default,
    Verbose,
}

pub struct ScanStats {
    pub processes: usize,
    pub scan_names: String,
    pub phase_timings: Vec<(String, f64)>,
    pub initial_files: usize,
    pub initial_dirs: usize,
    pub initial_size: u64,
    pub excluded_count: usize,
    pub final_files: usize,
    pub final_dirs: usize,
    pub final_size: u64,
    pub error_count: usize,
    pub total_bytes_scanned: u64,
    pub packages_assembled: usize,
    pub manifests_seen: usize,
}
```

#### CLI Extensions

```rust
#[derive(Parser, Debug)]
pub struct Cli {
    // ... existing fields ...

    /// Do not print summary or progress
    #[arg(short, long, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Print progress as file-by-file path instead of a progress bar.
    /// Print verbose scan counters.
    #[arg(short, long, conflicts_with = "quiet")]
    pub verbose: bool,
}
```

### Integration Points

The progress system hooks into the scanner lifecycle at six points:

```text
main.rs::run()
│
├─ 1. DISCOVERY PHASE ──────────────────────────────────────────
│    count_with_size(&dir_path, max_depth, &exclude_patterns)
│    └─ ScanProgress::start_discovery()     ← spinner
│    └─ ScanProgress::finish_discovery()    ← records initial counts
│
├─ 2. LICENSE LOADING ──────────────────────────────────────────
│    load_license_database()
│    └─ start_spdx_load()/finish_spdx_load()
│
├─ 3. SCAN PHASE ───────────────────────────────────────────────
│    process(&dir_path, ..., &progress)
│    └─ ScanProgress::start_scan(total_files)
│    └─ ScanProgress::file_completed(...)   ← per-file (in rayon)
│    └─ ScanProgress::finish_scan()         ← records scan timing
│
├─ 4. ASSEMBLY PHASE ───────────────────────────────────────────
│    assembly::assemble(&mut files)
│    └─ ScanProgress::start_assembly()      ← spinner/message
│    └─ ScanProgress::finish_assembly()     ← records assembly timing
│
├─ 5. OUTPUT PHASE ─────────────────────────────────────────────
│    write_output(&output_file, &output)
│    └─ start_output()/output_written()/finish_output()
│
└─ 6. SUMMARY ──────────────────────────────────────────────────
     ScanProgress::display_summary()        ← final statistics
```

### Behavior by Mode

| Action                      | Quiet  | Default                    | Verbose                               |
| --------------------------- | ------ | -------------------------- | ------------------------------------- |
| Discovery spinner           | Hidden | Shown                      | "Collecting files..." message         |
| "Loading SPDX data..."      | Hidden | Shown                      | Shown                                 |
| "Found N files..."          | Hidden | Shown                      | Shown with size details               |
| Scan progress bar           | Hidden | Shown                      | Hidden (replaced by per-file listing) |
| Per-file path               | Hidden | Hidden                     | Shown on stderr                       |
| Per-file errors             | Hidden | Shown via `println` on bar | Shown with path and detail            |
| Assembly progress           | Hidden | Shown (spinner/message)    | "Assembling packages..." message      |
| "Writing output..."         | Hidden | Shown                      | Shown                                 |
| Scan summary                | Hidden | Shown                      | Shown with extended detail            |
| `log::warn!()` from parsers | Hidden | Shown above bar            | Shown                                 |

### Module Structure

```text
src/
├── progress.rs          ← NEW: ScanProgress struct, ScanStats, formatting
├── main.rs              ← MODIFIED: wire ScanProgress into run()
├── scanner/
│   └── process.rs       ← MODIFIED: accept ScanProgress instead of Arc<ProgressBar>
│   └── count.rs         ← MODIFIED: count_with_size() for initial size metrics
└── tests/
    └── progress_cli_integration.rs  ← NEW: quiet/default/verbose stderr contract tests
```

Implementation touched progress orchestration plus scanner/count integration and dedicated CLI-mode integration tests.

### Summary Display Format

The scan summary follows Python's key structure while remaining aligned with Rust runtime output:

```text
Summary:        <enabled scans> with <process count> process(es)
Errors count:   <error count>
Scan Speed:     <files/sec> files/sec. <bytes/sec>/sec.
Initial counts: <resource count> resource(s): <file count> file(s) and <dir count> directorie(s) for <size>
Final counts:   <resource count> resource(s): <file count> file(s) and <dir count> directorie(s) for <size>
Excluded count: <excluded count>
Packages:       <assembled count> assembled from <manifest count> manifests
Timings:
  scan_start: <timestamp>
  scan_end:   <timestamp>
  discovery:  <seconds>
  spdx_load:  <seconds>
  scan:       <seconds>
  assembly:   <seconds>
  output:     <seconds>
  total:      <seconds>
```

### Human-Readable Size Formatting

A `format_size` utility function, matching Python's output format:

````rust
/// Format a byte count into a human-readable string.
///
/// # Examples
/// ```
/// assert_eq!(format_size(0), "0 Bytes");
/// assert_eq!(format_size(1), "1 Byte");
/// assert_eq!(format_size(1024), "1.00 KB");
/// assert_eq!(format_size(2567000), "2.45 MB");
/// ```
pub fn format_size(bytes: u64) -> String { ... }
````

---

## Implementation Phases

The phased rollout in this plan has been implemented.

### Landed Implementation

1. **Progress manager foundation**
   - `src/progress.rs` added with `ScanProgress`, `ProgressMode`, and `ScanStats`.
   - `main.rs` now uses progress manager lifecycle methods instead of ad-hoc progress/status writes.

2. **Multi-phase instrumentation**
   - Discovery, SPDX-load, scan, assembly, output, and summary phases are wired.
   - Scan phase uses a progress bar in default mode; other phases use mode-aware spinner/message indicators.

3. **Logging integration**
   - `indicatif-log-bridge` added.
   - `env_logger` moved to regular dependencies and initialized with the progress bridge.

4. **Statistics and summary**
   - Timing capture across phases.
   - Initial/final counts with size metrics, throughput (files/sec + bytes/sec), error count, and assembly counts.
   - `format_size()` implemented and used in summary rendering.

5. **Verbose/default/quiet behavior**
   - Quiet suppresses stderr progress/reporting output.
   - Default shows progress + concise inline errors.
   - Verbose shows file-by-file stderr paths and detailed per-file errors.

---

## Beyond-Parity Improvements

### B1: Multi-Phase Progress Bars (Python Has Single Bar)

Python only shows a progress bar for the scan phase. All other phases display text messages or nothing. Rust shows a spinner for discovery, a bar for scanning, and a bar for assembly — providing continuous visual feedback.

### B2: Throughput in Progress Bar Template (Python Shows Only at End)

Rust's progress bar template can include `{per_sec}` to show real-time files/second during scanning, not just in the final summary. indicatif computes this from its internal rate tracking.

### B3: Automatic Terminal Detection (Python Relies on Click)

indicatif draw targets hide progress rendering when the output stream is not an attended terminal. Color behavior should remain aligned with the currently used indicatif behavior in this repository.

### B4: Rate-Limited Updates (Python Updates Every File)

indicatif redraws are rate-limited by default, preventing terminal I/O from becoming a bottleneck on fast scans.

### B5: Log Messages Above Progress Bar

With `indicatif-log-bridge`, parser `warn!()`/`info!()` output can be emitted without corrupting active progress rendering (bridge suspends progress draws while logging). Python has no equivalent built into the progress helper path.

### B6: Real-Time Error Count in Progress Bar

The progress bar template can include a dynamic error counter:

```text
⠿ [00:02:15] [████████████░░░░░░░░░░░░] 450/1200 files (2 errors) (ETA: 3m12s)
```

Python only shows the error count in the final summary.

---

## Testing Strategy

### Automated Validation (Implemented)

- `cargo test --test progress_cli_integration`
  - Quiet mode suppresses stderr output.
  - Default mode emits scan summary to stderr.
  - Verbose mode emits file-by-file paths to stderr.
- `cargo test --test scanner_integration` validates scanner behavior after progress-manager wiring.
- `cargo test --bin provenant main_test::` validates CLI-mode mapping and main-path helpers.
- `cargo clippy --all-targets --all-features -- -D warnings` and `cargo build` pass with progress changes.

### Manual Spot Checks Performed

- Redirected stderr runs were checked for control-sequence artifacts; no `\r`/ANSI redraw leakage observed in non-TTY stderr output.
- Default/quiet/verbose mode behavior was spot-checked through real CLI runs.

---

## Success Criteria

- [x] `--quiet` suppresses stderr progress/reporting output
- [x] `--verbose` shows file-by-file stderr paths on TTY and bounded non-TTY progress with detailed per-file warning/error context
- [x] Default mode renders scan progress with ETA in terminal-attended runs
- [x] Progress rendering does not corrupt structured output files
- [x] Scan summary includes counts, sizes, speed, errors, and timings
- [x] `log::warn!()` integration is wired through progress-compatible logger bridge
- [x] Progress output auto-hides/degrades on non-TTY stderr
- [x] Existing tests pass after integration
- [x] `cargo clippy` is clean (`-D warnings`)

---

## Dependency Summary

| Crate                  | Status          | Purpose                                     |
| ---------------------- | --------------- | ------------------------------------------- |
| `indicatif`            | Already present | Progress bars, multi-bar, rayon integration |
| `clap`                 | Already present | CLI argument parsing                        |
| `log`                  | Already present | Logging facade                              |
| `rayon`                | Already present | Parallel processing                         |
| `chrono`               | Already present | Timestamps for summary                      |
| `env_logger`           | In dependencies | Logger implementation for `RUST_LOG`        |
| `indicatif-log-bridge` | In dependencies | Route log messages above progress bars      |

**Change summary**: Added `indicatif-log-bridge`; moved `env_logger` to regular dependencies.

---

## Related Documents

- **Architecture**: [`docs/ARCHITECTURE.md`](../../ARCHITECTURE.md) — Scanner pipeline section
- **Caching Plan**: [`CACHING_PLAN.md`](CACHING_PLAN.md) — Cache loading will need progress reporting
- **Python Reference**: `reference/scancode-toolkit/src/scancode/cli.py` — see flags, scanner progress, and summary display sections
