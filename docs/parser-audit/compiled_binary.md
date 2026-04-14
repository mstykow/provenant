# ADR 0004 Security Audit: compiled_binary

**File**: `src/parsers/compiled_binary.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `exec()`, `eval()`, or subprocess calls. Uses `object` crate for static binary format parsing. Uses `serde_json` for JSON deserialization of audit data. Uses `flate2` for zlib decompression — all static analysis.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

The parser operates on `bytes: &[u8]` received from the scanner. File size checking is the caller's responsibility. However, there IS a size limit for decompressed Rust audit data:

- `MAX_RUST_AUDIT_JSON_SIZE` = 8MB (line 50), enforced at line 120-121 via `decoder.take()` + length check — PASS for Rust audit data
- Go binary parsing: No size limit on `bytes` input or `modinfo` string processing
- `read_sibling_license_text` is in `windows_executable.rs`, not this module

### Recursion Depth

No recursive functions. All processing is iterative. — PASS

### Iteration Count

- `audit_data.packages.iter()` (line 110): No 100K cap on Rust audit packages
- `package.dependencies.iter()` (line 133): No 100K cap on per-package dependencies
- `parse_go_modinfo_packages` (line 282): Iterates over `modinfo.lines()` — no 100K cap
- `find_aligned_magic` (line 224): Scans entire binary for magic bytes — no size cap on scan window

### String Length

No field-level truncation at 10MB.

### Decompression Limits

- `decode_rust_audit_data` (line 117): Uses `decoder.take(MAX_RUST_AUDIT_JSON_SIZE + 1)` at line 120, then checks decoded length at line 121 — PASS (8MB limit). This satisfies ADR Principle 3 decompression limits.

## Principle 3: Archive Safety

**Status**: N/A

Binary files are not archives in the traditional sense. The decompression of Rust audit data has a size limit (8MB).

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

The function signature accepts `bytes: &[u8]` — file existence and reading is the caller's responsibility.

### UTF-8 Encoding

- `decode_varint_string` (line 247): Uses `std::str::from_utf8(bytes.get(start..end)?).ok()?` — silently drops invalid UTF-8 rather than using lossy conversion
- `decode_go_build_info_inline` (line 233): Uses `String::from_utf8(modinfo[...].to_vec()).ok()?` — same issue
- `decode_utf16_bytes` is in `windows_executable.rs`, not this module

### JSON/YAML Validity

`serde_json::from_slice` at line 125 returns `Option` via `.ok()`. Parse failure results in `None`, which is handled by returning no packages. — PASS

### Required Fields

Missing name/version in Rust audit packages are required by the struct definition (`String`, not `Option<String>`). — PASS (serde deserialization would fail on missing fields)

### URL Format

URLs accepted as-is. — Per ADR, acceptable.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution with cycle tracking. Rust audit data has dependency indices but these are just used for lookup, not cycle detection.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code. Test code at lines 406, 408, 859, etc. uses `.expect()` which is acceptable per ADR.

### Command::new / Subprocess Usage

**Status**: PASS

No subprocess calls found.

## Findings Summary

| #   | Principle    | Severity | Line(s)       | Description                                                                  |
| --- | ------------ | -------- | ------------- | ---------------------------------------------------------------------------- |
| 1   | P2 Iteration | MEDIUM   | 110, 133, 282 | No 100K cap on packages, dependencies, or modinfo lines                      |
| 2   | P4 UTF-8     | LOW      | 247, 239      | `from_utf8().ok()` silently drops invalid UTF-8; should use lossy conversion |
| 3   | P2 File Size | LOW      | —             | No file size check in this module (caller's responsibility)                  |

## Remediation Priority

1. Add iteration caps (100K) on package/dependency/modinfo line processing
2. Replace `from_utf8().ok()` with `from_utf8_lossy()` or log warnings for invalid UTF-8
