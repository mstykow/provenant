# ADR 0004 Security Audit: rpm_db_native_sqlite

**File**: `src/parsers/rpm_db_native/sqlite.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, or subprocess calls. Uses `rusqlite` crate for read-only SQLite database access. The connection is opened with `SQLITE_OPEN_READ_ONLY` flag (line 24), which prevents any write operations or SQL injection-based modifications.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before opening. `File::open()` at line 19 and `Connection::open_with_flags()` at line 24 both operate without pre-checking size.

### Recursion Depth

No recursive functions. PASS.

### Iteration Count

- `read_blobs()` line 36: iterates all rows from the `Packages` table without limit. A database with >100K rows would violate the 100K iteration cap.

### String Length

No 10MB truncation on blob data. SQLite blobs could be arbitrarily large.

## Principle 3: Archive Safety

**Status**: N/A

No archive extraction.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

`File::open()` at line 19 returns error on missing files. `Connection::open_with_flags()` at line 24 also returns error. No explicit `fs::metadata()` pre-check.

### UTF-8 Encoding

N/A — returns raw blobs. String conversion happens in entry.rs.

### JSON/YAML Validity

No JSON/YAML parsing. N/A.

### Required Fields

N/A — returns raw blobs.

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

| #   | Principle | Severity | Line(s) | Description                                                 |
| --- | --------- | -------- | ------- | ----------------------------------------------------------- |
| 1   | P2        | HIGH     | 19,24   | No file size check before opening (100MB limit)             |
| 2   | P2        | MEDIUM   | 36      | No iteration count cap on SQLite row iteration (100K items) |

## Remediation Priority

1. Add fs::metadata().len() check before opening with 100MB limit
2. Add iteration count cap on SQLite row iteration (LIMIT clause or early break)
