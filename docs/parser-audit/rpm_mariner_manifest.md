# ADR 0004 Security Audit: rpm_mariner_manifest

**File**: `src/parsers/rpm_mariner_manifest.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, or subprocess calls. All parsing is static text-based line parsing.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `fs::read_to_string()` at line 52 reads entire files without size validation. Note: this uses `std::fs::read_to_string` directly (not the utility), which also lacks size checks.

### Recursion Depth

No recursive functions. PASS.

### Iteration Count

No 100K iteration cap on loops:

- `parse_rpm_mariner_manifest()` line 67: iterates all lines without limit

### String Length

No 10MB truncation on field values (name, version, arch, filename from line splits).

## Principle 3: Archive Safety

**Status**: N/A

No archive extraction.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

`fs::read_to_string()` at line 52 returns error on missing files (handled at lines 53-57), but no explicit `fs::metadata()` pre-check.

### UTF-8 Encoding

`fs::read_to_string()` errors on invalid UTF-8. No lossy conversion fallback.

### JSON/YAML Validity

No JSON/YAML parsing. N/A.

### Required Fields

`parse_rpm_mariner_manifest()` lines 89-92: `name`, `version`, `arch`, `filename` extracted from tab-split fields. Missing/empty handled by checking `is_empty()` (lines 94, 102, 116, 121). Acceptable per ADR.

### URL Format

URLs accepted as-is. PASS per ADR.

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

| #   | Principle | Severity | Line(s) | Description                                                         |
| --- | --------- | -------- | ------- | ------------------------------------------------------------------- |
| 1   | P2        | HIGH     | 52      | No file size check before reading (100MB limit)                     |
| 2   | P2        | MEDIUM   | 67      | No iteration count cap (100K items)                                 |
| 3   | P2        | MEDIUM   | —       | No string length truncation (10MB per field)                        |
| 4   | P4        | LOW      | 52      | No explicit fs::metadata() pre-check                                |
| 5   | P4        | MEDIUM   | 52      | No lossy UTF-8 fallback; fs::read_to_string errors on invalid UTF-8 |

## Remediation Priority

1. Add fs::metadata().len() check before reading with 100MB limit
2. Add iteration count cap on line loop
3. Add String::from_utf8_lossy() fallback for UTF-8 handling

## Remediation

All 5 findings addressed:

1. **P2-FileSize**: Replaced `fs::read_to_string` with `read_file_to_string` which enforces a size limit.
2. **P2-Iteration**: Added `MAX_ITERATION_COUNT` cap on lines.
3. **P2-StringLength**: Added `truncate_field` on name, version, arch, filename, and namespace.
4. **P4-Pre-check**: Covered by `read_file_to_string`.
5. **P4-UTF8**: Covered by `read_file_to_string`.
