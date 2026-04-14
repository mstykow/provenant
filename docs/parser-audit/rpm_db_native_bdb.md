# ADR 0004 Security Audit: rpm_db_native_bdb

**File**: `src/parsers/rpm_db_native/bdb.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, or subprocess calls. Pure binary parsing of BerkeleyDB format using `File`, `Read`, and `Seek` traits.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before opening. `File::open()` at line 30 reads the file without pre-checking size. The `read_blobs()` method iterates from page 0 to `last_page_number` (line 96), which could be very large for a large database file.

### Recursion Depth

No recursive functions. PASS.

### Iteration Count

- `read_blobs()` line 96: iterates `0..=self.metadata.generic.last_page_number` — no cap on number of pages. A database claiming millions of pages could cause excessive I/O.
- `read_overflow_value()` line 57: follows a linked list of overflow pages with no depth limit. A malicious database could create an infinite overflow chain.
- `hash_page_value_indexes()` line 139: processes index entries based on `entry_count` without limit.

### String Length

No 10MB truncation on blob data. Overflow values are accumulated via `value.extend_from_slice()` without size limit.

## Principle 3: Archive Safety

**Status**: N/A

No archive extraction. Raw binary database reading.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

`File::open()` at line 30 returns error on missing files. No explicit `fs::metadata()` pre-check.

### UTF-8 Encoding

N/A — this module reads raw binary data. String conversion happens in `entry.rs` which uses `String::from_utf8_lossy()`.

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

| #   | Principle | Severity | Line(s) | Description                                                     |
| --- | --------- | -------- | ------- | --------------------------------------------------------------- |
| 1   | P2        | HIGH     | 30      | No file size check before opening (100MB limit)                 |
| 2   | P2        | HIGH     | 96      | No cap on page iteration (last_page_number could be very large) |
| 3   | P2        | HIGH     | 57      | No depth limit on overflow page linked-list traversal           |
| 4   | P2        | MEDIUM   | 139     | No cap on hash page entry count processing                      |

## Remediation Priority

1. Add fs::metadata().len() check before opening with 100MB limit
2. Add iteration cap on page loop (0..=last_page_number)
3. Add depth limit on overflow page chain traversal
4. Add size limit on accumulated overflow value data
