# ADR 0004 Security Audit: rpm_db_native_package

**File**: `src/parsers/rpm_db_native/package.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, or subprocess calls. Pure struct construction from parsed entries.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

N/A — operates on in-memory parsed entries.

### Recursion Depth

No recursive functions. PASS.

### Iteration Count

- `parse_installed_rpm_package()` line 34: iterates all entries without limit. However, entries are already bounded by HEADER_MAX_BYTES (256MB) in entry.rs.
- `read_u32_array()` and `read_string_array()` in entry.rs are bounded by `info.count`.

### String Length

No 10MB truncation on string values from `entry.read_string()` or `entry.read_string_array()`. Large string arrays (e.g., file_names, requires) are stored without length limits.

## Principle 3: Archive Safety

**Status**: N/A

No archive extraction.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

N/A — operates on in-memory data.

### UTF-8 Encoding

Delegated to `entry.rs` which uses `String::from_utf8_lossy()`. PASS.

### JSON/YAML Validity

No JSON/YAML parsing. N/A.

### Required Fields

All fields in `InstalledRpmPackage` default to empty strings/vectors. `ensure_kind()` at line 111 validates tag types. Acceptable per ADR.

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

| #   | Principle | Severity | Line(s) | Description                                               |
| --- | --------- | -------- | ------- | --------------------------------------------------------- |
| 1   | P2        | MEDIUM   | 34      | No explicit iteration count cap on entry processing       |
| 2   | P2        | MEDIUM   | —       | No string length truncation on string/string_array fields |

## Remediation Priority

1. Add iteration count cap on entry processing loop
2. Add string length truncation on parsed string values
