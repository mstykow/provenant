# ADR 0004 Security Audit: bower

**File**: `src/parsers/bower.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, or subprocess calls. Uses `serde_json` for JSON parsing (static). All processing is data extraction from parsed JSON values.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `fs::read_to_string(path)` at line 158 (inside `read_and_parse_json`) reads entire file without size validation.

### Recursion Depth

No recursive functions. No recursion depth concern.

### Iteration Count

- `licenses.iter()` at line 176 (in `extract_license_statement`): no iteration cap on license array.
- `authors` iteration at line 246: no 100K cap.
- `deps.iter()` at line 372: no 100K cap on dependencies.
- `keywords` iteration at line 232: no iteration cap.

### String Length

No 10 MB per-field truncation. Field values from JSON are stored as-is without length checks.

## Principle 3: Archive Safety

**Status**: N/A

This parser does not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `fs::read_to_string(path)` at line 158 will fail if the file doesn't exist, but error is handled gracefully via `read_and_parse_json` returning `Err`, which is handled in `extract_packages` at line 58.

### UTF-8 Encoding

`fs::read_to_string(path)` at line 158 fails on invalid UTF-8 with no lossy fallback. Non-UTF-8 files cause total parse failure without encoding warning.

### JSON/YAML Validity

`serde_json::from_str(&content)` at line 159 handles parse failure gracefully, returning an error that is handled in `extract_packages` at line 58, returning `default_package_data()` with a warning. PASS.

### Required Fields

Missing `name` and `version` are handled via `.and_then(|v| v.as_str()).map(String::from)` (lines 65-68, 79-82) resulting in `None` values. When `name` is `None`, the package is marked as private (line 71). PASS.

### URL Format

URLs (homepage, VCS) are accepted as-is. PASS.

## Principle 5: Circular Dependency Detection

**Status**: N/A

This parser does not resolve transitive dependencies.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No bare `.unwrap()` calls in library code. Only `.unwrap_or()` and `.unwrap_or_default()` are used.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage.

## Findings Summary

| #   | Principle           | Severity | Line(s)            | Description                                                        |
| --- | ------------------- | -------- | ------------------ | ------------------------------------------------------------------ |
| 1   | P2: File Size       | Medium   | 158                | No `fs::metadata().len()` check before `fs::read_to_string()`      |
| 2   | P2: Iteration Count | Low      | 176, 246, 372, 232 | No 100K iteration cap on license/author/dependency/keyword loops   |
| 3   | P2: String Length   | Low      | Various            | No 10 MB per-field truncation                                      |
| 4   | P4: File Exists     | Low      | 158                | No explicit `fs::metadata()` pre-check before reading              |
| 5   | P4: UTF-8 Encoding  | Low      | 158                | No lossy UTF-8 fallback; non-UTF-8 files cause total parse failure |

## Remediation Priority

1. Add `fs::metadata().len()` check with 100 MB limit before reading file (line 158)
2. Add 100K iteration cap to license/author/dependency/keyword loops
3. Add 10 MB field value truncation with warning
4. Add `fs::metadata()` pre-check for file existence
5. Add lossy UTF-8 fallback for non-UTF-8 files
