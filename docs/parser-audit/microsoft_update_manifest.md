# ADR 0004 Security Audit: microsoft_update_manifest

**File**: `src/parsers/microsoft_update_manifest.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `exec()`, `eval()`, or subprocess calls. Uses `quick_xml` for streaming XML parsing — static analysis only.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading. `fs::read_to_string` at line 36 without size pre-check.

### Recursion Depth

No recursive functions. `quick_xml::Reader` is used iteratively with a loop (line 64). — PASS

### Iteration Count

- XML parsing loop (line 64-105): No iteration cap on XML events processed. A file with millions of XML events would be fully processed.
- `e.attributes().filter_map(|a| a.ok())` (lines 68, 79): No cap on number of attributes

### String Length

No field-level truncation at 10MB. XML attribute values used as-is.

## Principle 3: Archive Safety

**Status**: N/A

XML files are not archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `fs::read_to_string` at line 36 fails on missing files, handled via `match`. — Acceptable.

### UTF-8 Encoding

`fs::read_to_string` will fail on non-UTF-8. No lossy conversion fallback. — Minor gap. XML attribute values are decoded via `String::from_utf8().ok()` (lines 70-71, 82-86) which silently drops invalid UTF-8 rather than using lossy conversion.

### JSON/YAML Validity

N/A — XML format. XML parse errors (line 94-101) are handled by logging warning and breaking. — PASS

### Required Fields

Missing name/version result in `None` values. — PASS

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

| #   | Principle        | Severity | Line(s)      | Description                                                                                  |
| --- | ---------------- | -------- | ------------ | -------------------------------------------------------------------------------------------- |
| 1   | P2 File Size     | MEDIUM   | 36           | No file size check before reading                                                            |
| 2   | P2 Iteration     | LOW      | 64           | No iteration cap on XML events                                                               |
| 3   | P2 String Length | LOW      | —            | No field-level 10MB truncation                                                               |
| 4   | P4 UTF-8         | LOW      | 70-71, 82-86 | `String::from_utf8().ok()` silently drops invalid UTF-8; should use lossy conversion per ADR |

## Remediation Priority

1. Add `fs::metadata().len()` check before reading, reject >100MB
2. Replace `String::from_utf8().ok()` with `String::from_utf8_lossy()` for XML attribute values

## Remediation

- **#1 P2 File Size**: Replaced `fs::read_to_string` with `read_file_to_string(path, None)` — enforces 100MB size check before reading and provides lossy UTF-8 fallback.
- **#2 P2 Iteration**: Added `MAX_ITERATION_COUNT` counter cap to XML parsing loop.
- **#3 P2 String Length**: Applied `truncate_field()` to all extracted string values (version, description, copyright, homepage_url, purl).
- **#4 P4 UTF-8**: Replaced `String::from_utf8().ok()` with `String::from_utf8_lossy()` + `warn!` log for XML attribute values — invalid UTF-8 is now preserved via lossy conversion instead of silently dropped.
