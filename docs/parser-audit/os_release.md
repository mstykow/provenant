# ADR 0004 Security Audit: os_release

**File**: `src/parsers/os_release.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `exec()`, `eval()`, or subprocess calls. Simple line-based key=value parsing.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading. `fs::read_to_string` at line 47 without size pre-check. Note: OS release files are typically very small (<1KB), making this low risk in practice.

### Recursion Depth

No recursive functions. All processing is iterative. — PASS

### Iteration Count

- `parse_key_value_pairs` (line 121): Iterates over `content.lines()` — no 100K cap. OS release files have few lines in practice.

### String Length

No field-level truncation at 10MB.

## Principle 3: Archive Safety

**Status**: N/A

Text files are not archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `fs::read_to_string` at line 47 fails on missing files, handled via `match`. — Acceptable.

### UTF-8 Encoding

`fs::read_to_string` will fail on non-UTF-8. No lossy conversion fallback. — Minor gap.

### JSON/YAML Validity

N/A — plain text format.

### Required Fields

Missing `ID` field defaults to empty string (line 66). Name is always set via `determine_namespace_and_name`. — PASS

### URL Format

URLs accepted as-is. — Per ADR, acceptable.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

- Line 71: `.unwrap_or_default()` — safe
- No problematic `.unwrap()` calls in library code.

### Command::new / Subprocess Usage

**Status**: PASS

No subprocess calls found.

## Findings Summary

| #   | Principle        | Severity | Line(s) | Description                                                                        |
| --- | ---------------- | -------- | ------- | ---------------------------------------------------------------------------------- |
| 1   | P2 File Size     | LOW      | 47      | No file size check before reading (low risk — OS release files are typically tiny) |
| 2   | P2 String Length | LOW      | —       | No field-level 10MB truncation                                                     |
| 3   | P4 UTF-8         | LOW      | 47      | No lossy UTF-8 fallback                                                            |

## Remediation Priority

1. Add `fs::metadata().len()` check before reading for defense-in-depth (low priority due to small expected file size)
