# ADR 0004 Security Audit: deno_lock

**File**: `src/parsers/deno_lock.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, or subprocess calls. Uses `serde_json` for JSON parsing (static). All processing is data extraction from parsed JSON values.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `fs::read_to_string(path)` at line 36 reads entire file without size validation.

### Recursion Depth

No recursive functions. No recursion depth concern.

### Iteration Count

- `workspace_direct` iteration at line 74: no iteration cap.
- `jsr_map.keys()` iteration at line 96: no 100K cap on JSR entries.
- `npm_map.keys()` iteration at line 107: no 100K cap on npm entries.
- `redirects` iteration at line 118: no iteration cap.
- `filter_map(Value::as_str)` at line 308: no iteration cap on npm dependency arrays.
- `filter_map(Value::as_str)` at line 348: no iteration cap on JSR dependency arrays.

### String Length

No 10 MB per-field truncation. Field values from JSON are stored as-is without length checks.

## Principle 3: Archive Safety

**Status**: N/A

This parser does not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `fs::read_to_string(path)` at line 36 will fail if the file doesn't exist, but error is handled gracefully returning `default_package_data()`.

### UTF-8 Encoding

`fs::read_to_string(path)` at line 36 fails on invalid UTF-8 with no lossy fallback. Non-UTF-8 files cause total parse failure without encoding warning.

### JSON/YAML Validity

`serde_json::from_str(&content)` at line 44 handles parse failure gracefully, returning `default_package_data()` with a warning. PASS.

Lockfile version validation at line 58 checks for version "5" and warns on unsupported versions. PASS.

### Required Fields

Missing package data results in `None` values or skipped entries. PASS.

### URL Format

URLs (redirect targets, remote URLs) are parsed via `Url::parse` at line 466, which validates URL format. PASS.

## Principle 5: Circular Dependency Detection

**Status**: N/A

This parser does not resolve transitive dependency graphs.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No bare `.unwrap()` calls in library code.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage.

## Findings Summary

| #   | Principle           | Severity | Line(s)                    | Description                                                        |
| --- | ------------------- | -------- | -------------------------- | ------------------------------------------------------------------ |
| 1   | P2: File Size       | Medium   | 36                         | No `fs::metadata().len()` check before `fs::read_to_string()`      |
| 2   | P2: Iteration Count | Low      | 74, 96, 107, 118, 308, 348 | No 100K iteration cap on JSR/npm/redirect/dependency loops         |
| 3   | P2: String Length   | Low      | Various                    | No 10 MB per-field truncation                                      |
| 4   | P4: File Exists     | Low      | 36                         | No explicit `fs::metadata()` pre-check before reading              |
| 5   | P4: UTF-8 Encoding  | Low      | 36                         | No lossy UTF-8 fallback; non-UTF-8 files cause total parse failure |

## Remediation Priority

1. Add `fs::metadata().len()` check with 100 MB limit before reading file (line 36)
2. Add 100K iteration cap to JSR/npm/redirect/dependency loops
3. Add 10 MB field value truncation with warning
4. Add `fs::metadata()` pre-check for file existence
5. Add lossy UTF-8 fallback for non-UTF-8 files
