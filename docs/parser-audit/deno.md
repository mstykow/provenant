# ADR 0004 Security Audit: deno

**File**: `src/parsers/deno.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, or subprocess calls. Uses `json5::from_str` for JSON/JSONC parsing (static). All processing is data extraction from parsed JSON values.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `fs::read_to_string(path)` at line 37 reads entire file without size validation.

### Recursion Depth

No recursive functions. No recursion depth concern.

### Iteration Count

- `json.get(FIELD_IMPORTS)...into_iter().flatten().filter_map()` at line 124: iterates all import entries without a 100K cap.
- `extra_data` field insertion loop at line 202: iterates a fixed set of 8 fields, so no concern.

### String Length

No 10 MB per-field truncation. Field values from JSON are stored as-is without length checks.

## Principle 3: Archive Safety

**Status**: N/A

This parser does not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `fs::read_to_string(path)` at line 37 will fail if the file doesn't exist, but error is handled gracefully returning `default_package_data()`.

### UTF-8 Encoding

`fs::read_to_string(path)` at line 37 fails on invalid UTF-8 with no lossy fallback. Non-UTF-8 files cause total parse failure without encoding warning.

### JSON/YAML Validity

`json5::from_str(&content)` at line 45 handles parse failure gracefully, returning `default_package_data()` with a warning. PASS.

### Required Fields

Missing `name` and `version` are handled via `extract_non_empty_string` (line 219) which returns `Option<String>`, resulting in `None` values. PASS.

### URL Format

URLs (import specifiers) are parsed via `Url::parse` at line 258, which validates URL format. Other specifiers are accepted as-is. PASS.

## Principle 5: Circular Dependency Detection

**Status**: N/A

This parser does not resolve transitive dependencies.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No bare `.unwrap()` calls in library code. All `.unwrap()` usage is in test files (`deno_test.rs`, `deno_lock_test.rs`) which is acceptable per ADR 0004.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage.

## Findings Summary

| #   | Principle           | Severity | Line(s) | Description                                                        |
| --- | ------------------- | -------- | ------- | ------------------------------------------------------------------ |
| 1   | P2: File Size       | Medium   | 37      | No `fs::metadata().len()` check before `fs::read_to_string()`      |
| 2   | P2: Iteration Count | Low      | 124     | No 100K iteration cap on import entries                            |
| 3   | P2: String Length   | Low      | Various | No 10 MB per-field truncation                                      |
| 4   | P4: File Exists     | Low      | 37      | No explicit `fs::metadata()` pre-check before reading              |
| 5   | P4: UTF-8 Encoding  | Low      | 37      | No lossy UTF-8 fallback; non-UTF-8 files cause total parse failure |

## Remediation Priority

1. Add `fs::metadata().len()` check with 100 MB limit before reading file (line 37)
2. Add 100K iteration cap to import entries loop (line 124)
3. Add 10 MB field value truncation with warning
4. Add `fs::metadata()` pre-check for file existence
5. Add lossy UTF-8 fallback for non-UTF-8 files
