# ADR 0004 Security Audit: swift_resolved

**File**: `src/parsers/swift_resolved.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `exec()`, `eval()`, or subprocess calls. Uses `serde_json` for static JSON deserialization (line 108). Uses `url` crate for URL parsing (line 257).

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading. `read_file` (line 285) opens file and reads entire content into a `String` via `file.read_to_string()` without size pre-check.

### Recursion Depth

No recursive functions. All processing is iterative over pin arrays. — PASS

### Iteration Count

- `parse_v2_v3_pins` (line 176): Iterates over `pins` slice — no 100K cap
- `parse_v1_pins` (line 180): Iterates over `pins` slice — no 100K cap

### String Length

No field-level truncation at 10MB.

## Principle 3: Archive Safety

**Status**: N/A

JSON files are not archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `File::open` at line 286 will fail on missing files, error is propagated. — Acceptable.

### UTF-8 Encoding

`file.read_to_string` at line 288 will fail on non-UTF-8 content. No lossy conversion fallback. — Minor gap.

### JSON/YAML Validity

`serde_json::from_str` error at line 109 is handled, returns error that results in `default_package_data()`. — PASS

### Required Fields

Name and version are `Option<String>` from deserialized structs. Missing values result in `None`. — PASS

### URL Format

URLs parsed with `Url::parse` (line 257). Invalid URLs return `None`. — PASS

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution with cycle tracking.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

- Line 118: `.unwrap_or(&[])` — safe, returns empty slice
- Line 261: `.unwrap_or(path)` — safe, returns original path
- No problematic `.unwrap()` calls in library code.

### Command::new / Subprocess Usage

**Status**: PASS

No subprocess calls found.

## Findings Summary

| #   | Principle        | Severity | Line(s)  | Description                                               |
| --- | ---------------- | -------- | -------- | --------------------------------------------------------- |
| 1   | P2 File Size     | MEDIUM   | 285-289  | No file size check before reading entire file into memory |
| 2   | P2 Iteration     | LOW      | 176, 180 | No 100K cap on pins array iteration                       |
| 3   | P2 String Length | LOW      | —        | No field-level 10MB truncation                            |
| 4   | P4 UTF-8         | LOW      | 288      | No lossy UTF-8 fallback                                   |

## Remediation Priority

1. Add `fs::metadata().len()` check before reading, reject >100MB
2. Add iteration cap (100K) on pins processing
