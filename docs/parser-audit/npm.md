# ADR 0004 Security Audit: npm

**File**: `src/parsers/npm.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, subprocess calls, or dynamic code execution found. Uses `serde_json` for parsing (AST-level), regex-free field name extraction via character iteration (`extract_field_name` at line 254). All parsing is static.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `fs::read_to_string(path)` at line 234 reads entire file without size validation. A 100 MB+ file would be loaded into memory without warning.

### Recursion Depth

No recursive functions in this parser. `extract_field_name` (line 254) uses a simple loop. No recursion depth concern.

### Iteration Count

- `content.lines().enumerate()` at line 242: iterates all lines without a 100K cap. A malicious file with >100K lines would iterate without limit.
- `deps.iter()` at line 715: iterates dependency entries without iteration cap.
- `bundled_array.iter()` at line 825: no iteration cap.
- `licenses.iter()` at line 328: no iteration cap.
- `keywords` iteration at line 927: no iteration cap.

### String Length

No 10 MB per-field truncation. Field values from JSON (e.g., `json.get(field).and_then(|v| v.as_str())`) are stored as-is without length checks.

## Principle 3: Archive Safety

**Status**: N/A

This parser does not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `fs::read_to_string(path)` at line 234 will fail with an error if the file doesn't exist, but the ADR requires an explicit `fs::metadata()` check before reading. The error is handled gracefully via `map_err` and returns `default_package_data()`.

### UTF-8 Encoding

`fs::read_to_string(path)` at line 234 will fail on invalid UTF-8, returning an error. There is no `String::from_utf8()` with lossy fallback — the parser simply fails and returns default data. No warning about encoding issues is logged for partial recovery.

### JSON/YAML Validity

`serde_json::from_str(&content)` at line 238 handles parse failure gracefully, returning `default_package_data()` with a warning. PASS.

### Required Fields

Missing `name` and `version` are handled via `extract_non_empty_string` (line 628) which returns `Option<String>`, resulting in `None` values. Fields are populated with `None` and parsing continues. PASS.

### URL Format

URLs (homepage, repository, download URLs) are accepted as-is without aggressive parsing. PASS.

## Principle 5: Circular Dependency Detection

**Status**: N/A

This parser does not resolve transitive dependencies.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No bare `.unwrap()` calls in library code. Only `.unwrap_or()`, `.unwrap_or_default()`, and `.unwrap_or_else()` are used (lines 207, 293, 368, 376, 939, 1026).

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage.

## Findings Summary

| #   | Principle           | Severity | Line(s)                 | Description                                                        |
| --- | ------------------- | -------- | ----------------------- | ------------------------------------------------------------------ |
| 1   | P2: File Size       | Medium   | 234                     | No `fs::metadata().len()` check before `fs::read_to_string()`      |
| 2   | P2: Iteration Count | Low      | 242, 715, 825, 328, 927 | No 100K iteration cap on line/entry/dependency loops               |
| 3   | P2: String Length   | Low      | Various                 | No 10 MB per-field truncation                                      |
| 4   | P4: File Exists     | Low      | 234                     | No explicit `fs::metadata()` pre-check before reading              |
| 5   | P4: UTF-8 Encoding  | Low      | 234                     | No lossy UTF-8 fallback; non-UTF-8 files cause total parse failure |

## Remediation Priority

1. Add `fs::metadata().len()` check with 100 MB limit before reading file (line 234)
2. Add 100K iteration cap to `content.lines()` loop (line 242) and dependency iteration loops
3. Add 10 MB field value truncation with warning
4. Add `fs::metadata()` pre-check for file existence before reading
5. Add lossy UTF-8 fallback for non-UTF-8 files

## Remediation

**PR**: #664
**Date**: 2026-04-14

All findings addressed in ADR 0004 security compliance batch 1.
