# ADR 0004 Security Audit: rpm_db_native_ndb

**File**: `src/parsers/rpm_db_native/ndb.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, or subprocess calls. Pure binary parsing of RPM NDB format using `BufReader<File>`, `Read`, and `Seek`.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before opening. `File::open()` at line 88 reads the file without pre-checking size. However, there are some bounds:

- Line 98: `slot_page_count > 2048` is rejected — this limits the number of slot entries.

### Recursion Depth

No recursive functions. PASS.

### Iteration Count

- `open()` line 107: iterates `0..slot_count` (bounded by `slot_page_count * 4096/16 - 2`). With `slot_page_count <= 2048`, max slots = 2048 \* 256 - 2 = 524286. This exceeds the 100K iteration cap.
- `read_blobs()` line 118: iterates all slots without limit beyond the slot_page_count cap.

### String Length

No 10MB truncation on blob data. `blob_header.blob_length` at line 144 determines the read size — a malicious NDB could specify a very large `blob_length`, causing a large memory allocation.

## Principle 3: Archive Safety

**Status**: N/A

No archive extraction.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

`File::open()` at line 88 returns error on missing files. No explicit `fs::metadata()` pre-check.

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

| #   | Principle | Severity | Line(s) | Description                                                                   |
| --- | --------- | -------- | ------- | ----------------------------------------------------------------------------- |
| 1   | P2        | HIGH     | 88      | No file size check before opening (100MB limit)                               |
| 2   | P2        | MEDIUM   | 107     | Slot count can reach 524K, exceeding 100K iteration cap                       |
| 3   | P2        | MEDIUM   | 144     | No size cap on blob_length; malicious NDB could cause large memory allocation |

## Remediation Priority

1. Add fs::metadata().len() check before opening with 100MB limit
2. Reduce slot_page_count limit or add explicit 100K iteration cap on slot processing
3. Add size cap on individual blob allocation
