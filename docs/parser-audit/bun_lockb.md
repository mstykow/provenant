# ADR 0004 Security Audit: bun_lockb

**File**: `src/parsers/bun_lockb.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, or subprocess calls. Performs custom binary parsing of the bun.lockb format using `LockbCursor` (lines 60-693) — all static analysis of binary data. No code execution.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `std::fs::read(path)` at line 76 reads entire file into memory as bytes without size validation. A very large `.lockb` file would be loaded entirely.

### Recursion Depth

`build_dependencies_for_package` at line 353 is indirectly recursive — it calls `build_resolved_package` at line 427, which calls `build_dependencies_for_package` at line 427 again. There is **no depth tracking** and **no cycle detection** in this recursion. A circular package reference in the lockb data would cause infinite recursion and stack overflow.

### Iteration Count

- `packages` iteration at line 191: iterates all packages without a 100K cap. The `list_len` value comes directly from the binary file at line 122 with no upper bound check.
- `dep_slice.iter().zip(res_slice.iter())` at line 369: no iteration cap on dependency entries.
- `bytes.chunks_exact()` at lines 331, 347: no iteration cap.

### String Length

No 10 MB per-field truncation. Decoded strings from `decode_bun_string` at line 524 are stored as-is without length checks. The `len` value at line 538 comes from the binary data.

## Principle 3: Archive Safety

**Status**: N/A

This parser does not handle archives (bun.lockb is a binary lockfile, not an archive format).

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `std::fs::read(path)` at line 76 will fail if the file doesn't exist, but error is handled gracefully returning `default_package_data()`.

### UTF-8 Encoding

`decode_bun_string` at line 524 uses `std::str::from_utf8` and returns `Err` on invalid UTF-8 (line 531, 543). There is no lossy fallback — invalid UTF-8 causes the entire parse to fail.

### JSON/YAML Validity

Not applicable — this is a binary format parser. The `serde_json` usage at line 751 is only in the `yaml_value_to_json` helper, not for input parsing.

### Required Fields

Missing package data is handled via `Result<..., String>` error propagation. The root package is required (line 289). PASS.

### URL Format

URLs (resolved URLs) are accepted as-is. PASS.

## Principle 5: Circular Dependency Detection

**Status**: FAIL

`build_dependencies_for_package` (line 353) calls `build_resolved_package` (line 405), which calls `build_dependencies_for_package` (line 427) recursively. There is **no visited tracking** and **no cycle detection**. A circular reference between packages in the lockb data would cause infinite recursion.

## Additional Checks

### .unwrap() in Library Code

**Status**: FAIL

Seven bare `.unwrap()` calls in library code (non-test):

- Line 349: `chunk.try_into().unwrap()` — in `parse_resolution_ids`, converting 4-byte chunks to `u32`
- Line 453: `bytes[0..4].try_into().unwrap()` — in `parse_slice_ref`, converting to `u32`
- Line 454: `bytes[4..8].try_into().unwrap()` — in `parse_slice_ref`, converting to `u32`
- Line 471: `bytes[16..20].try_into().unwrap()` — in `parse_resolution`, converting to `u32`
- Line 472: `bytes[20..24].try_into().unwrap()` — in `parse_resolution`, converting to `u32`
- Line 473: `bytes[24..28].try_into().unwrap()` — in `parse_resolution`, converting to `u32`
- Line 536: `bytes[0..4].try_into().unwrap()` — in `decode_bun_string`, converting to `u32`
- Line 537: `bytes[4..8].try_into().unwrap()` — in `decode_bun_string`, converting to `u32`

Note: These `.unwrap()` calls are on fixed-size slice-to-array conversions where the slice length is guaranteed by prior bounds checks. They are safe in practice but technically violate the ADR 0004 rule against `.unwrap()` in library code.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage.

## Findings Summary

| #   | Principle               | Severity | Line(s)                                | Description                                                                                                 |
| --- | ----------------------- | -------- | -------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| 1   | P5: Circular Dependency | High     | 353, 405, 427                          | `build_dependencies_for_package` ↔ `build_resolved_package` recursion has no cycle detection or depth limit |
| 2   | P2: File Size           | Medium   | 76                                     | No `fs::metadata().len()` check before `std::fs::read()`                                                    |
| 3   | P2: Iteration Count     | Low      | 191, 369                               | No 100K iteration cap; `list_len` from binary is unbounded                                                  |
| 4   | P2: String Length       | Low      | 524, 538                               | No 10 MB per-field truncation on decoded strings                                                            |
| 5   | Additional: .unwrap()   | Low      | 349, 453, 454, 471, 472, 473, 536, 537 | 8 `.unwrap()` calls in library code on fixed-size array conversions                                         |
| 6   | P4: File Exists         | Low      | 76                                     | No explicit `fs::metadata()` pre-check before reading                                                       |
| 7   | P4: UTF-8 Encoding      | Low      | 531, 543                               | No lossy UTF-8 fallback; invalid UTF-8 causes total parse failure                                           |

## Remediation Priority

1. Add cycle detection (visited set or depth limit of 50) to `build_dependencies_for_package` ↔ `build_resolved_package` recursion
2. Add `fs::metadata().len()` check with 100 MB limit before reading file (line 76)
3. Add upper bound check on `list_len` (line 122) and 100K iteration cap
4. Replace `.unwrap()` calls with proper error handling (lines 349, 453, 454, 471-473, 536, 537)
5. Add 10 MB string length truncation with warning in `decode_bun_string`
6. Add `fs::metadata()` pre-check for file existence
7. Add lossy UTF-8 fallback for string decoding
