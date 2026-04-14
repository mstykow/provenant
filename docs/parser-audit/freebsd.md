# ADR 0004 Security Audit: freebsd

**File**: `src/parsers/freebsd.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, or subprocess calls. Uses `yaml_serde::from_str()` for parsing, which is a safe Rust deserializer.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `read_file_to_string()` at line 61 reads entire files without size validation.

### Recursion Depth

No recursive functions. PASS.

### Iteration Count

No 100K iteration cap on loops. However, parsing is delegated to `yaml_serde::from_str()` which has its own internal limits. The license normalization loops are bounded by the number of licenses in the manifest. LOW risk.

### String Length

No 10MB truncation on field values from deserialized manifest.

## Principle 3: Archive Safety

**Status**: N/A

No archive extraction.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

`read_file_to_string()` returns error on missing files (handled at lines 62-66), but no explicit `fs::metadata()` pre-check.

### UTF-8 Encoding

`read_file_to_string()` errors on invalid UTF-8. No lossy conversion fallback.

### JSON/YAML Validity

`yaml_serde::from_str()` at line 89 returns error on invalid YAML/JSON (handled at lines 90-94). Returns `default_package_data()` on failure. PASS per ADR.

### Required Fields

`parse_freebsd_manifest()` line 97: `name` and `version` are `Option<String>` from deserialized struct. Missing values result in `None`. Acceptable per ADR.

### URL Format

URLs accepted as-is. PASS per ADR.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code (excluding test module at line 304).

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle | Severity | Line(s) | Description                                                     |
| --- | --------- | -------- | ------- | --------------------------------------------------------------- |
| 1   | P2        | HIGH     | 61      | No file size check before reading (100MB limit)                 |
| 2   | P2        | MEDIUM   | —       | No string length truncation (10MB per field)                    |
| 3   | P4        | LOW      | 61      | No explicit fs::metadata() pre-check                            |
| 4   | P4        | MEDIUM   | 61      | No lossy UTF-8 fallback; invalid UTF-8 causes error not warning |

## Remediation Priority

1. Add fs::metadata().len() check before read_file_to_string with 100MB limit
2. Add String::from_utf8_lossy() fallback for UTF-8 handling
3. Add string length truncation on deserialized field values
