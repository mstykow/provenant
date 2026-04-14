# ADR 0004 Security Audit: vcpkg

**File**: `src/parsers/vcpkg.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

Uses `serde_json::from_str` (line 32) for static JSON parsing. No `Command::new`, `subprocess`, `eval()`, or any code execution mechanism.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading. `extract_packages` (line 24) uses `fs::read_to_string(path)` without a size check. Additionally, `read_sibling_configuration` (line 254) reads a sibling file `vcpkg-configuration.json` without a size check.

### Recursion Depth

No recursive functions. All parsing is iterative over JSON structures. **PASS**.

### Iteration Count

No 100K iteration cap on:

- `extract_dependencies` (line 134): iterates over all dependency entries without cap
- `extract_dependencies` inner loop (line 149): iterates over feature dependencies without cap
- `extract_maintainers` (line 106): iterates over maintainers array without cap
- `build_extra_data` (line 225): iterates over fixed set of fields (low risk)

### String Length

No 10 MB truncation with warning on any field value. String fields like `name`, `version`, `description`, `homepage`, `license` are extracted without size limits (lines 53-57, 267-273).

## Principle 3: Archive Safety

**Status**: N/A

Vcpkg parser does not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. Uses `fs::read_to_string(path)` (line 24) with error handling that returns `default_package_data()` (lines 26-29). Also, `read_sibling_configuration` (line 254) uses `fs::read_to_string` on a derived sibling path without pre-check. Does not panic, but doesn't use `fs::metadata()` as specified.

### UTF-8 Encoding

`fs::read_to_string` returns an error for non-UTF-8 files. Error is caught and fallback data returned. No explicit `String::from_utf8()` + warning + lossy conversion path.

### JSON/YAML Validity

`serde_json::from_str` (line 32) returns an error on invalid JSON, caught at lines 34-37, returning `default_package_data()`. **PASS**.

### Required Fields

Missing `name` is handled via `get_non_empty_string` (line 53) which returns `Option<String>`. When `None`, `is_private` is set to `true` (line 72). Missing `version` handled similarly (line 54). PURL handles missing values gracefully (lines 76-78). **PASS**.

### URL Format

`homepage` field is accepted as-is (line 56). Per ADR 0004, accept as-is is correct. **PASS**.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution performed. Dependencies are extracted from manifest declarations only.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle           | Severity | Line(s)        | Description                                                                                 |
| --- | ------------------- | -------- | -------------- | ------------------------------------------------------------------------------------------- |
| 1   | P2: File Size       | Medium   | 24, 254        | No `fs::metadata().len()` check before reading; files loaded into memory without size check |
| 2   | P2: Iteration Count | Low      | 134, 149       | No 100K iteration cap on dependency/feature processing                                      |
| 3   | P2: String Length   | Low      | 53-57, 267-273 | No 10 MB truncation with warning on string field values                                     |
| 4   | P4: File Exists     | Low      | 24, 254        | Uses `fs::read_to_string` instead of `fs::metadata()` pre-check                             |
| 5   | P4: UTF-8 Encoding  | Low      | 24             | No lossy UTF-8 conversion path; invalid UTF-8 causes fallback data return                   |

## Remediation Priority

1. Add `fs::metadata().len()` check with 100 MB limit before reading files (lines 24, 254)
2. Add iteration count cap (100K) on dependency/feature processing loops
3. Add 10 MB string field truncation with warning
4. Add `fs::metadata()` pre-check before file read
5. Add lossy UTF-8 conversion with warning for encoding errors
