# ADR 0004 Security Audit: haxe

**File**: `src/parsers/haxe.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `exec()`, `eval()`, or subprocess calls. Uses `serde_json` for static JSON deserialization.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading. `read_haxelib_json` (line 185) uses `File::open` + `read_to_string` without size pre-check.

### Recursion Depth

No recursive functions. All processing is iterative. — PASS

### Iteration Count

- `deps_list` iteration (line 84): Iterates over dependencies HashMap — no 100K cap
- `json_content.contributors` iteration (line 103): Iterates over contributors — no 100K cap
- Both are expected to be small in practice

### String Length

No field-level truncation at 10MB.

## Principle 3: Archive Safety

**Status**: N/A

JSON files are not archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `File::open` at line 186 fails on missing files, error propagated and handled in `extract_packages` (line 51). — Acceptable.

### UTF-8 Encoding

`file.read_to_string` at line 189 will fail on non-UTF-8. No lossy conversion fallback. — Minor gap.

### JSON/YAML Validity

`serde_json::from_str` error at line 192 is handled, returns error string caught in `extract_packages` (line 51). — PASS

### Required Fields

Missing name/version from serde deserialization are `Option<String>` with `#[serde(default)]` (lines 166-169). — PASS

### URL Format

URLs accepted as-is. — Per ADR, acceptable.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code.

### Command::new / Subprocess Usage

**Status**: PASS

No subprocess calls found.

## Findings Summary

| #   | Principle        | Severity | Line(s) | Description                       |
| --- | ---------------- | -------- | ------- | --------------------------------- |
| 1   | P2 File Size     | MEDIUM   | 186-190 | No file size check before reading |
| 2   | P2 String Length | LOW      | —       | No field-level 10MB truncation    |
| 3   | P4 UTF-8         | LOW      | 189     | No lossy UTF-8 fallback           |

## Remediation Priority

1. Add `fs::metadata().len()` check before reading, reject >100MB
