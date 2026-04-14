# ADR 0004 Security Audit: rpm_db_native_entry

**File**: `src/parsers/rpm_db_native/entry.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, or subprocess calls. Pure binary data parsing and struct construction.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

N/A — this module receives `&[u8]` slices, does not read files.

### Recursion Depth

No recursive functions. PASS.

### Iteration Count

- `HeaderBlob::parse()` line 134: iterates `0..index_length` — limited by `HEADER_MAX_BYTES` check at line 124, but `index_length` itself is only bounded indirectly by the 256MB total limit.
- `verify_entries()` line 278: iterates all entry_infos without limit.
- `import_entries()` line 153: iterates all entry_infos without limit.
- `swab_region()` line 328: iterates all entry_infos without limit.
- `read_u32_array()` line 67: limited by `self.info.count` without explicit cap.
- `read_string_array()` line 79: splits on null bytes without count limit.

### String Length

- `HEADER_MAX_BYTES` at line 14: 256 MB limit on total header blob size. This exceeds ADR 0004's 100MB file size limit.
- No 10MB per-field truncation on string values.

## Principle 3: Archive Safety

**Status**: N/A

No archive extraction.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

N/A — operates on in-memory data.

### UTF-8 Encoding

- `read_string()` line 60: uses `String::from_utf8_lossy()` — PASS per ADR.
- `read_string_array()` line 83: uses `String::from_utf8_lossy()` — PASS per ADR.

### JSON/YAML Validity

No JSON/YAML parsing. N/A.

### Required Fields

N/A — returns parsed entries; callers handle missing fields.

### URL Format

N/A.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle | Severity | Line(s)     | Description                                                           |
| --- | --------- | -------- | ----------- | --------------------------------------------------------------------- |
| 1   | P2        | MEDIUM   | 14          | HEADER_MAX_BYTES is 256MB, exceeding ADR 0004's 100MB file size limit |
| 2   | P2        | MEDIUM   | 134,278,328 | No explicit iteration count cap on entry processing                   |
| 3   | P2        | LOW      | 67,79       | No count cap on u32/string array reads                                |

## Remediation Priority

1. Reduce HEADER_MAX_BYTES from 256MB to 100MB or add a separate file-level 100MB check
2. Add iteration count caps on entry/array processing
