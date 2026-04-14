# ADR 0004 Security Audit: rpm_yumdb

**File**: `src/parsers/rpm_yumdb.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, or subprocess calls. All parsing is static text-based path parsing and filesystem reads.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading individual key files. `fs::read_to_string()` at line 93 reads key files without size validation. While individual key files are typically small, a malicious filesystem could contain large files.

### Recursion Depth

No recursive functions. PASS.

### Iteration Count

No 100K iteration cap on loops:

- `extract_packages()` line 83: iterates all entries in the yumdb package directory without limit

### String Length

No 10MB truncation on key values read from filesystem. Each key file's content is stored in `extra_data` without length limits.

## Principle 3: Archive Safety

**Status**: N/A

No archive extraction.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

`fs::read_dir()` at line 72 returns error on missing directory (handled at lines 73-79). `fs::read_to_string()` at line 93 returns error on missing files (handled at lines 103). No explicit `fs::metadata()` pre-check.

### UTF-8 Encoding

`fs::read_to_string()` at line 93 errors on invalid UTF-8. No lossy conversion fallback.

### JSON/YAML Validity

No JSON/YAML parsing. N/A.

### Required Fields

`extract_packages()` line 63: If `parse_yumdb_dir_name()` returns `None`, returns default package data. Name and version are extracted from directory name; missing values handled. Acceptable per ADR.

### URL Format

URLs accepted as-is. PASS per ADR.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code (excluding test module at line 123).

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle | Severity | Line(s) | Description                                                         |
| --- | --------- | -------- | ------- | ------------------------------------------------------------------- |
| 1   | P2        | MEDIUM   | 93      | No file size check before reading key files (100MB limit)           |
| 2   | P2        | MEDIUM   | 83      | No iteration count cap on directory entries (100K items)            |
| 3   | P2        | LOW      | —       | No string length truncation (10MB per field)                        |
| 4   | P4        | LOW      | 72,93   | No explicit fs::metadata() pre-check                                |
| 5   | P4        | MEDIUM   | 93      | No lossy UTF-8 fallback; fs::read_to_string errors on invalid UTF-8 |

## Remediation Priority

1. Add fs::metadata().len() check before reading key files with 100MB limit
2. Add iteration count cap on directory entry loop
3. Add String::from_utf8_lossy() fallback for UTF-8 handling
