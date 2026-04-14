# ADR 0004 Security Audit: swift_manifest_json

**File**: `src/parsers/swift_manifest_json.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `exec()`, `eval()`, or subprocess calls. Uses `serde_json` for static JSON parsing.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading. `fs::read_to_string` called at line 53 via `read_swift_manifest_json`. Entire file loaded into memory without size pre-check.

### Recursion Depth

No recursive functions found. All processing is iterative over JSON arrays. — PASS

### Iteration Count

- `get_dependencies` (line 136): Iterates over `deps_array` — no 100K cap
- No other unbounded iteration loops

### String Length

No field-level truncation at 10MB.

## Principle 3: Archive Safety

**Status**: N/A

JSON files are not archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `fs::read_to_string` at line 53 fails on missing files, handled by returning error which is caught in `extract_packages` (line 31). — Acceptable fallback.

### UTF-8 Encoding

`fs::read_to_string` will fail on non-UTF-8 content. No lossy conversion fallback. — Minor gap.

### JSON/YAML Validity

`serde_json::from_str` errors at line 55 are handled by returning error string, caught in `extract_packages` (line 31). Returns default `PackageData`. — PASS

### Required Fields

Missing name results in `None` (line 59-62). Version is always `None` for manifest JSON. — PASS

### URL Format

URLs parsed via `get_namespace_and_name` (line 261) with simple string operations. Accepted as-is. — Per ADR, acceptable.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution with cycle tracking.

## Additional Checks

### .unwrap() in Library Code

**Status**: FAIL

- Line 159: `.unwrap_or_default()` on identity extraction — safe (returns empty string)
- Line 208: `.unwrap_or_default()` on identity extraction — safe
- Line 281: `.unwrap_or(path)` on `.strip_suffix(".git")` — safe
- No problematic `.unwrap()` calls in library code.

### Command::new / Subprocess Usage

**Status**: PASS

No subprocess calls found.

## Findings Summary

| #   | Principle        | Severity | Line(s) | Description                       |
| --- | ---------------- | -------- | ------- | --------------------------------- |
| 1   | P2 File Size     | MEDIUM   | 53      | No file size check before reading |
| 2   | P2 Iteration     | LOW      | 136     | No 100K cap on dependencies array |
| 3   | P2 String Length | LOW      | —       | No field-level 10MB truncation    |
| 4   | P4 UTF-8         | LOW      | 53      | No lossy UTF-8 fallback           |

## Remediation Priority

1. Add `fs::metadata().len()` check before reading files, reject >100MB
2. Add iteration cap (100K) on dependencies array
