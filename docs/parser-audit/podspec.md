# ADR 0004 Security Audit: podspec

**File**: `src/parsers/podspec.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `subprocess`, `eval()`, `exec()`, or code execution primitives found. All parsing uses regex-based Ruby DSL pattern matching (`Regex::new`, `captures`, `captures_iter`). The `md5` crate usage (line 29) is for hash computation only, not execution.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

- Line 64: `fs::read_to_string(path)` called directly with **no** `fs::metadata().len()` pre-check. A multi-GB .podspec would be fully loaded into memory.

### Recursion Depth

- No recursive functions. `extract_multiline_description` (line 335) iterates `lines.iter().skip(start_index)` linearly. All other functions iterate `content.lines()` linearly. **PASS**.

### Iteration Count

- Line 288: `for line in content.lines()` in `extract_field` — **no** 100K cap.
- Line 301: `for (i, line) in lines.iter().enumerate()` in `extract_description` — **no** 100K cap.
- Line 348: `for line in lines.iter().skip(start_index)` in `extract_multiline_description` — **no** 100K cap.
- Line 374: `for line in content.lines()` in `extract_authors` — **no** 100K cap.
- Line 400: `for line in content.lines()` in `extract_source_url` — **no** 100K cap.
- Line 462: `for line in content.lines()` in `extract_dependencies` — **no** 100K cap.
- No cap on number of dependencies or authors collected.

### String Length

- No field values are truncated at 10MB. `clean_string` (line 525) and `extract_field` return full strings regardless of length.

## Principle 3: Archive Safety

**Status**: N/A

This parser does not handle archive files.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

- Line 64-69: `fs::read_to_string` error is caught and returns `default_package_data()`. No explicit `fs::metadata()` pre-check. Functionally acceptable but doesn't match ADR prescription.

### UTF-8 Encoding

- Line 64: `fs::read_to_string` fails on non-UTF-8 content, returning default PackageData. **No** lossy fallback is attempted. **FAIL** — non-UTF-8 files silently return default data.

### JSON/YAML Validity

- N/A — this parser uses regex-based text parsing, not JSON/YAML deserialization.

### Required Fields

- Missing name/version result in `None` values, the parser continues. Line 480: empty names are filtered (`if name.is_empty() { return None; }`). **PASS**.

### URL Format

- URLs (homepage, source, vcs_url) are extracted from content or constructed programmatically and stored as-is. Per ADR: "Accept as-is". **PASS**.

## Principle 5: Circular Dependency Detection

**Status**: N/A

This parser does not perform dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: FAIL

- Line 137: `.unwrap_or_else(|_| PackageUrl::new("generic", name_str).unwrap())` — nested `.unwrap()` that will panic if both `PackageUrl::new("cocoapods", ...)` and `PackageUrl::new("generic", ...)` fail. The "generic" fallback should always succeed, but the bare `.unwrap()` violates the no-unwrap rule.
- Lines 204-213: Multiple `.unwrap()` calls in `lazy_static!` regex initializations:
  - Line 204: `Regex::new(r"\.name\s*=\s*(.+)").unwrap()`
  - Line 205: `Regex::new(r"\.version\s*=\s*(.+)").unwrap()`
  - Line 206: `Regex::new(r"\.summary\s*=\s*(.+)").unwrap()`
  - Line 207: `Regex::new(r"\.description\s*=\s*(.+)").unwrap()`
  - Line 208: `Regex::new(r"\.homepage\s*=\s*(.+)").unwrap()`
  - Line 209: `Regex::new(r"\.license\s*=\s*(.+)").unwrap()`
  - Line 210: `Regex::new(r"\.source\s*=\s*(.+)").unwrap()`
  - Line 211: `Regex::new(r"\.authors?\s*=\s*(.+)").unwrap()`
  - Line 212: `Regex::new(r#":git\s*=>\s*['\"]([^'\"]+)['\"]"#).unwrap()`
  - Line 213: `Regex::new(r#":http\s*=>\s*['\"]([^'\"]+)['\"]"#).unwrap()`
  - Line 218: `DEPENDENCY_PATTERN` regex `.unwrap()`

  These will panic at first access if regex compilation fails. While the patterns are compile-time constants and unlikely to fail, they are bare `.unwrap()` in library code.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new`, `std::process::Command`, or subprocess usage found.

## Findings Summary

| #   | Principle  | Severity | Line(s)                      | Description                                                                 |
| --- | ---------- | -------- | ---------------------------- | --------------------------------------------------------------------------- |
| 1   | P2: DoS    | Medium   | 64                           | No file size pre-check (`fs::metadata().len()`) before `fs::read_to_string` |
| 2   | P2: DoS    | Medium   | 288, 301, 348, 374, 400, 462 | No iteration count cap (100K) on line processing loops                      |
| 3   | P2: DoS    | Low      | 525                          | No string field truncation at 10MB limit                                    |
| 4   | P4: Input  | Medium   | 64                           | No lossy UTF-8 fallback — non-UTF-8 files cause silent default return       |
| 5   | Additional | Medium   | 137                          | `.unwrap()` in PURL creation fallback — can panic on invalid input          |
| 6   | Additional | Low      | 204-213, 218                 | `.unwrap()` in `lazy_static!` regex initializations (11 instances)          |

## Remediation Priority

1. **[P2: DoS] Add file size pre-check** using `fs::metadata().len()` with 100MB limit before reading
2. **[P4: Input] Add lossy UTF-8 fallback** — use `fs::read()` + `String::from_utf8_lossy()` instead of `fs::read_to_string()`
3. **[Additional] Replace `.unwrap()`** at line 137 with `match`/`map_err` or `expect("generic PURL creation should not fail")`
4. **[P2: DoS] Add iteration count caps** (100K) to all line processing loops with early-break and warning
5. **[Additional] Replace `.unwrap()`** in `lazy_static!` blocks with `expect("regex is valid")` for explicit panics with context
6. **[P2: DoS] Add string field truncation** at 10MB with warning log
