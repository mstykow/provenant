# ADR 0004 Security Audit: publiccode

**File**: `src/parsers/publiccode.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `exec()`, `eval()`, or subprocess calls. Uses `yaml_serde` for static YAML parsing.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading. `fs::read_to_string` at line 23 without size pre-check.

### Recursion Depth

No recursive functions. All processing is iterative over YAML fields. — PASS

### Iteration Count

- `extract_contact_parties` (line 129): Iterates over contacts sequence — no 100K cap
- `extract_localized_string` (line 115): Iterates over mapping values — small iteration expected

### String Length

No field-level truncation at 10MB.

## Principle 3: Archive Safety

**Status**: N/A

YAML files are not archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `fs::read_to_string` at line 23 fails on missing files, handled via `match`. — Acceptable.

### UTF-8 Encoding

`fs::read_to_string` will fail on non-UTF-8. No lossy conversion fallback. — Minor gap.

### JSON/YAML Validity

YAML parse error at line 34 is handled, returns `default_package_data()`. — PASS

### Required Fields

Missing `publiccodeYmlVersion` returns `default_package_data()` (line 58-64). Missing name/version result in `None`. — PASS

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
| 1   | P2 File Size     | MEDIUM   | 23      | No file size check before reading |
| 2   | P2 String Length | LOW      | —       | No field-level 10MB truncation    |
| 3   | P4 UTF-8         | LOW      | 23      | No lossy UTF-8 fallback           |

## Remediation Priority

1. Add `fs::metadata().len()` check before reading, reject >100MB
