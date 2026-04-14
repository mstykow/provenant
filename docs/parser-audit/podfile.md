# ADR 0004 Security Audit: podfile

**File**: `src/parsers/podfile.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `subprocess`, `eval()`, `exec()`, or code execution primitives found. Parsing uses regex-based Ruby DSL pattern matching (`Regex::new`, `POD_PATTERN.captures`) — no Ruby AST or runtime evaluation.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

- Line 54: `fs::read_to_string(path)` called directly with **no** `fs::metadata().len()` pre-check. A 10GB Podfile would be fully read into memory before any parsing begins.

### Recursion Depth

- No recursive functions in this parser. `extract_dependencies` (line 127) iterates `content.lines()` linearly. **PASS**.

### Iteration Count

- Line 130: `for line in content.lines()` iterates without any 100K cap. A Podfile with millions of lines would be processed indefinitely.
- No cap on number of dependencies collected in `dependencies` vector (line 128).

### String Length

- No field values are truncated at 10MB. `name`, `version_req`, `git_url`, `local_path` extracted from regex captures (lines 133-136) are stored as-is regardless of length.

## Principle 3: Archive Safety

**Status**: N/A

This parser does not handle archive files.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

- Line 54-59: `fs::read_to_string` error is caught and returns `default_package_data()`. No explicit `fs::metadata()` pre-check, but the error is handled gracefully. Functionally acceptable, though ADR prescribes `fs::metadata()` check.

### UTF-8 Encoding

- Line 54: `fs::read_to_string` fails on non-UTF-8 content, returning default PackageData. **No** lossy fallback (`String::from_utf8_lossy`) is attempted. **FAIL** — non-UTF-8 files silently return default data.

### JSON/YAML Validity

- N/A — this parser does not parse JSON or YAML.

### Required Fields

- The Podfile parser does not extract name/version for the package itself (lines 64-107: `name: None, version: None`). Dependencies with empty names are filtered (line 154: `if name.is_empty() { return None; }`). **PASS**.

### URL Format

- URLs (git_url, local_path) extracted from regex captures and stored as-is. Per ADR: "Accept as-is". **PASS**.

## Principle 5: Circular Dependency Detection

**Status**: N/A

This parser does not perform dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: FAIL

- Line 123: `POD_PATTERN` initialization uses `.unwrap()` in `lazy_static!` block:

  ```rust
  static ref POD_PATTERN: Regex = Regex::new(...).unwrap();
  ```

  This will panic at program startup if the regex fails to compile. While this is in a `lazy_static!` (one-time init), it is technically `.unwrap()` in non-test library code. The regex is a compile-time constant, so this is low-risk in practice but violates the no-unwrap rule.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new`, `std::process::Command`, or subprocess usage found.

## Findings Summary

| #   | Principle  | Severity | Line(s) | Description                                                                 |
| --- | ---------- | -------- | ------- | --------------------------------------------------------------------------- |
| 1   | P2: DoS    | Medium   | 54      | No file size pre-check (`fs::metadata().len()`) before `fs::read_to_string` |
| 2   | P2: DoS    | Medium   | 130     | No iteration count cap (100K) on line processing loop                       |
| 3   | P2: DoS    | Low      | 133-136 | No string field truncation at 10MB limit                                    |
| 4   | P4: Input  | Medium   | 54      | No lossy UTF-8 fallback — non-UTF-8 files cause silent default return       |
| 5   | Additional | Low      | 123     | `.unwrap()` in `lazy_static!` regex initialization (non-test library code)  |

## Remediation Priority

1. **[P2: DoS] Add file size pre-check** using `fs::metadata().len()` with 100MB limit before reading
2. **[P4: Input] Add lossy UTF-8 fallback** — use `fs::read()` + `String::from_utf8_lossy()` instead of `fs::read_to_string()`
3. **[P2: DoS] Add iteration count cap** (100K) to line processing loop with early-break and warning
4. **[P2: DoS] Add string field truncation** at 10MB with warning log
5. **[Additional] Replace `.unwrap()`** in `lazy_static!` with `expect()` or `Regex::new(...).expect("POD_PATTERN regex is valid")` for explicit panics with context

## Remediation

- **#1 P2 File Size**: Replaced `fs::read_to_string` with `read_file_to_string(path, None)` — enforces 100MB size check before reading and provides lossy UTF-8 fallback.
- **#2 P2 Iteration**: Added `.take(MAX_ITERATION_COUNT)` to `content.lines()` iteration.
- **#3 P2 String Length**: Applied `truncate_field()` to name, version_req, git_url, local_path, purl, extracted_requirement.
- **#4 P4 UTF-8**: Fixed automatically by `read_file_to_string` — lossy UTF-8 conversion replaces silent failure on non-UTF-8 content.
- **#5 Additional**: Replaced `lazy_static!` + `.unwrap()` with `LazyLock<Regex>` + `.expect("valid regex")` for explicit panic with context.
