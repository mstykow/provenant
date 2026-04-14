# ADR 0004 Security Audit: podspec_json

**File**: `src/parsers/podspec_json.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `subprocess`, `eval()`, `exec()`, or code execution primitives found. Parsing uses `serde_json` for JSON deserialization and structured data traversal. The `md5` crate usage (line 485) is for hash computation only, not execution.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

- Line 262: `read_json_file` opens the file with `File::open(path)` and reads with `file.read_to_string(&mut contents)` — **no** `fs::metadata().len()` pre-check. A multi-GB .podspec.json would be fully loaded into memory.
- The `serde_json::from_str` at line 266 then parses the entire string, potentially consuming significant memory for deeply nested JSON structures.

### Recursion Depth

- No recursive functions in this parser. All extraction iterates over JSON object key-value pairs linearly. **PASS**.

### Iteration Count

- Line 383: `for (name, value) in authors_obj` — iterates over authors object with **no** 100K cap.
- Line 438: `for (name, requirement) in deps_obj` — iterates over dependencies object with **no** 100K cap.
- No cap on number of dependencies or parties collected.

### String Length

- No field values are truncated at 10MB. Values extracted from JSON (`.as_str()`, `.trim()`) are stored as-is regardless of length.
- The entire JSON content is cloned into `extra_data["podspec.json"]` at line 147 without size check.

## Principle 3: Archive Safety

**Status**: N/A

This parser does not handle archive files.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

- Line 262: `File::open(path)` returns error on missing file, propagated via `Result<Value, String>`. At line 58-63, the error is caught and `default_package_data()` is returned. No explicit `fs::metadata()` pre-check. Functionally acceptable but doesn't match ADR prescription.

### UTF-8 Encoding

- Line 264: `file.read_to_string(&mut contents)` fails on non-UTF-8 content. The error is propagated up to line 58-63 which returns `default_package_data()`. **No** lossy fallback is attempted. **FAIL** — non-UTF-8 files silently return default data.

### JSON/YAML Validity

- Line 266: `serde_json::from_str(&contents)` parse failure returns `Err(String)`, which propagates to line 58-63 and returns `default_package_data()`. **PASS** — graceful handling on invalid JSON.

### Required Fields

- Missing name/version are handled via `.filter(|s| !s.is_empty())` (lines 70, 76) which results in `None`. The parser continues with `None` values. **PASS**.
- Empty dependency names are skipped (line 440: `if name_str.is_empty() { continue; }`). **PASS**.

### URL Format

- URLs (homepage, git, http) are extracted from JSON fields and stored as-is. Per ADR: "Accept as-is". **PASS**.

## Principle 5: Circular Dependency Detection

**Status**: N/A

This parser does not perform dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: FAIL

- Line 198: `.unwrap_or_else(|_| PackageUrl::new("generic", name_str).unwrap())` — nested `.unwrap()` that will panic if both `PackageUrl::new("cocoapods", ...)` and `PackageUrl::new("generic", ...)` fail. The "generic" fallback should always succeed for non-empty strings, but the bare `.unwrap()` violates the no-unwrap rule.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new`, `std::process::Command`, or subprocess usage found.

## Findings Summary

| #   | Principle  | Severity | Line(s)  | Description                                                                  |
| --- | ---------- | -------- | -------- | ---------------------------------------------------------------------------- |
| 1   | P2: DoS    | Medium   | 262-264  | No file size pre-check (`fs::metadata().len()`) before reading file          |
| 2   | P2: DoS    | Medium   | 383, 438 | No iteration count cap (100K) on authors/dependencies object iteration loops |
| 3   | P2: DoS    | Low      | 147      | Entire JSON content cloned into `extra_data` without size check              |
| 4   | P2: DoS    | Low      | Various  | No string field truncation at 10MB limit                                     |
| 5   | P4: Input  | Medium   | 264      | No lossy UTF-8 fallback — non-UTF-8 files cause silent default return        |
| 6   | Additional | Medium   | 198      | `.unwrap()` in PURL creation fallback — can panic on invalid input           |

## Remediation Priority

1. **[P2: DoS] Add file size pre-check** using `fs::metadata().len()` with 100MB limit before reading
2. **[P4: Input] Add lossy UTF-8 fallback** — use `fs::read()` + `String::from_utf8_lossy()` instead of `read_to_string()`
3. **[Additional] Replace `.unwrap()`** at line 198 with `match`/`map_err` or `expect("generic PURL creation should not fail")`
4. **[P2: DoS] Add iteration count caps** (100K) to authors/dependencies iteration loops with early-break and warning
5. **[P2: DoS] Add size check** before cloning full JSON into `extra_data["podspec.json"]` at line 147
6. **[P2: DoS] Add string field truncation** at 10MB with warning log

## Remediation

1. **P2 Medium**: No file size pre-check — Replaced `File::open`+`read_to_string` with `read_file_to_string`
2. **P2 Medium**: No iteration caps — Added `MAX_ITERATION_COUNT` caps on authors/deps
3. **P2 Low**: No extra_data size check — Added 10MB size check before cloning JSON to `extra_data`
4. **P2 Low**: No string truncation — Added `truncate_field()` to all extracted string values
5. **P4 Medium**: No lossy UTF-8 — Fixed by `read_file_to_string`
6. **Additional Medium**: `.unwrap()` in PURL fallback — Replaced with safe `.or_else().ok()` and `.map()` pattern
