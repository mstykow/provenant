# ADR 0004 Security Audit: rpm_db_native_mod

**File**: `src/parsers/rpm_db_native/mod.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, or subprocess calls. Delegates to BDB/NDB/SQLite parsers which are all native Rust implementations.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before opening database files. `BdbDatabase::open()`, `NdbDatabase::open()`, `SqliteDatabase::open()` all open files without pre-checking size.

### Recursion Depth

No recursive functions. PASS.

### Iteration Count

- Line 52: `reader.read_blobs()?.into_iter().map(...)` iterates all blobs without limit. A database with >100K packages would violate the 100K iteration cap.
- Line 54: `.collect()` collects all results without limit.

### String Length

No 10MB truncation on parsed package data fields.

## Principle 3: Archive Safety

**Status**: N/A

No archive extraction. RPM database files are read structurally.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

Each sub-parser handles file opening errors via `File::open()` returning `Result`. No explicit `fs::metadata()` pre-check.

### UTF-8 Encoding

Delegated to sub-parsers. `entry.rs` uses `String::from_utf8_lossy()` for string reading (line 61-63). PASS for UTF-8 in the entry parsing path.

### JSON/YAML Validity

No JSON/YAML parsing. N/A.

### Required Fields

Delegated to `parse_installed_rpm_package()` in package.rs. Fields default to empty strings. Acceptable per ADR.

### URL Format

N/A.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code (excluding test module at line 63).

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle | Severity | Line(s) | Description                                                    |
| --- | --------- | -------- | ------- | -------------------------------------------------------------- |
| 1   | P2        | HIGH     | 36-37   | No file size check before opening database files (100MB limit) |
| 2   | P2        | MEDIUM   | 52-54   | No iteration count cap on blob/package iteration (100K items)  |

## Remediation Priority

1. Add fs::metadata().len() check before opening database files with 100MB limit
2. Add iteration count cap on blob/package iteration
