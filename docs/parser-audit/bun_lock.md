# ADR 0004 Security Audit: bun_lock

**File**: `src/parsers/bun_lock.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, or subprocess calls. Uses `json5::from_str` for JSON5 parsing (static). All processing is data extraction from parsed JSON values.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `fs::read_to_string(path)` at line 43 reads entire file without size validation.

### Recursion Depth

No recursive functions. No recursion depth concern.

### Iteration Count

- `packages` iteration at line 115: iterates all package entries without a 100K cap.
- `workspaces.values()` iterations at lines 156, 168: no iteration cap.
- `deps.iter()` at line 462: no iteration cap on nested dependencies.
- `map.keys()` at line 221: no iteration cap.

### String Length

No 10 MB per-field truncation. Field values from JSON are stored as-is without length checks.

## Principle 3: Archive Safety

**Status**: N/A

This parser does not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `fs::read_to_string(path)` at line 43 will fail if the file doesn't exist, but error is handled gracefully returning `default_package_data()`.

### UTF-8 Encoding

`fs::read_to_string(path)` at line 43 fails on invalid UTF-8 with no lossy fallback. Non-UTF-8 files cause total parse failure without encoding warning.

### JSON/YAML Validity

`json5::from_str(&content)` at line 51 handles parse failure gracefully, returning `default_package_data()` with a warning. PASS.

### Required Fields

Missing package name/version result in `None` or skipped entries (e.g., `split_locator` at line 303 returns `Option`). PASS.

### URL Format

URLs (resolved download URLs) are accepted as-is. PASS.

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

| #   | Principle           | Severity | Line(s)            | Description                                                        |
| --- | ------------------- | -------- | ------------------ | ------------------------------------------------------------------ |
| 1   | P2: File Size       | Medium   | 43                 | No `fs::metadata().len()` check before `fs::read_to_string()`      |
| 2   | P2: Iteration Count | Low      | 115, 156, 168, 462 | No 100K iteration cap on package/workspace/dependency loops        |
| 3   | P2: String Length   | Low      | Various            | No 10 MB per-field truncation                                      |
| 4   | P4: File Exists     | Low      | 43                 | No explicit `fs::metadata()` pre-check before reading              |
| 5   | P4: UTF-8 Encoding  | Low      | 43                 | No lossy UTF-8 fallback; non-UTF-8 files cause total parse failure |

## Remediation Priority

1. Add `fs::metadata().len()` check with 100 MB limit before reading file (line 43)
2. Add 100K iteration cap to package/workspace/dependency loops
3. Add 10 MB field value truncation with warning
4. Add `fs::metadata()` pre-check for file existence
5. Add lossy UTF-8 fallback for non-UTF-8 files
