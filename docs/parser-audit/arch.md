# ADR 0004 Security Audit: arch

**File**: `src/parsers/arch.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, or subprocess calls. All parsing is static text-based key-value parsing.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `read_file_to_string()` at lines 29, 49 reads entire files without size validation.

### Recursion Depth

No recursive functions. PASS.

### Iteration Count

No 100K iteration cap on loops:

- `parse_key_value_lines()` line 82: iterates all lines without limit
- `parse_srcinfo_like()` line 108: iterates all lines without limit
- `build_dependencies()` line 351: iterates all keys and values without limit
- `build_extra_data()` line 408: iterates all fields without limit

### String Length

No 10MB truncation on field values. Values from key-value parsing stored without length limits.

## Principle 3: Archive Safety

**Status**: N/A

No archive extraction.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

Uses `read_file_to_string()` which returns error on missing files (handled at lines 31-34, 51-54), but no explicit `fs::metadata()` pre-check.

### UTF-8 Encoding

`read_file_to_string()` errors on invalid UTF-8. No lossy conversion fallback.

### JSON/YAML Validity

No JSON/YAML parsing. N/A.

### Required Fields

`parse_srcinfo_like()` line 157: Package is only included if `pkg.name.is_some()`. Missing name is handled gracefully. Missing version results in `None`.

### URL Format

URLs accepted as-is. PASS per ADR.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code. `unwrap_or_default()` and `unwrap_or(false)` patterns are used (lines 159, 391, 450), which are safe.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle | Severity | Line(s)        | Description                                                     |
| --- | --------- | -------- | -------------- | --------------------------------------------------------------- |
| 1   | P2        | HIGH     | 29,49          | No file size check before reading (100MB limit)                 |
| 2   | P2        | MEDIUM   | 82,108,351,408 | No iteration count cap (100K items)                             |
| 3   | P2        | MEDIUM   | —              | No string length truncation (10MB per field)                    |
| 4   | P4        | LOW      | 29,49          | No explicit fs::metadata() pre-check                            |
| 5   | P4        | MEDIUM   | 29,49          | No lossy UTF-8 fallback; invalid UTF-8 causes error not warning |

## Remediation Priority

1. Add fs::metadata().len() check before read_file_to_string with 100MB limit
2. Add iteration count caps on line/field loops
3. Add String::from_utf8_lossy() fallback for UTF-8 handling

## Remediation

- Finding #1 (P2 File Size): Already using `read_file_to_string(path, None)` — provides 100MB size check, file-exists check, and lossy UTF-8 fallback
- Finding #2 (P2 Iteration Count): Added `MAX_ITERATION_COUNT` caps to `parse_key_value_lines`, `parse_srcinfo_like`, `build_dependencies`, and `build_extra_data`
- Finding #3 (P2 String Length): Applied `truncate_field()` to all extracted string values (name, version, description, homepage_url, extracted_license_statement, packager name/email, extracted_requirement, dep names, extra_data values)
- Findings #4, #5 (P4): Already covered by `read_file_to_string(path, None)`
