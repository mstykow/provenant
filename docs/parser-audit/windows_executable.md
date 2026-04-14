# ADR 0004 Security Audit: windows_executable

**File**: `src/parsers/windows_executable.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `exec()`, `eval()`, or subprocess calls. Uses `object` crate for static PE binary parsing. All analysis is done on raw bytes via memory-mapped structures.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

The parser operates on `bytes: &[u8]` received from the scanner. File size checking is the caller's responsibility. However, the module has `MAX_SIBLING_LICENSE_BYTES` (256KB, line 28) for sibling license files — a partial size limit. The main binary bytes have no size limit in this module.

### Recursion Depth

No recursive functions. All processing is iterative (XML parsing loop, version info iteration). — PASS

### Iteration Count

- `iter_version_blocks` (line 348): Iterator over version blocks — no 100K cap
- `extract_utf16_version_string_fallback` (line 324): Processes entire byte array — no size cap
- `WINDOWS_VERSION_FALLBACK_KEYS` iteration (line 331): Fixed small iteration (11 keys) — PASS
- `build_windows_executable_package` (line 438): No cap on string tables or dependencies

### String Length

No field-level truncation at 10MB. PE version string values used as-is.

## Principle 3: Archive Safety

**Status**: N/A

PE binaries are not archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

For sibling license reading (`read_sibling_license_text`, line 759), `fs::metadata` IS checked at line 771. File size IS checked at line 773 against `MAX_SIBLING_LICENSE_BYTES`. — PASS for sibling files.

For main binary parsing, the function signature accepts `bytes: &[u8]` — file existence and reading is the caller's responsibility.

### UTF-8 Encoding

- `decode_utf16_bytes` (line 423): Uses `String::from_utf16(&units).ok()` — silently drops invalid UTF-16 rather than using lossy conversion. Should use `String::from_utf16_lossy()` or at minimum log a warning.
- `extract_utf16_version_string_fallback` (line 324): Uses `String::from_utf16_lossy` — PASS
- `String::from_utf8` is not used (PE uses UTF-16LE encoding)

### JSON/YAML Validity

N/A — binary format.

### Required Fields

Missing name results in `fallback_windows_executable_name` (line 535). If still `None`, returns `None` from `build_windows_executable_package` (line 471). — PASS

### URL Format

URLs accepted as-is. — Per ADR, acceptable.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: FAIL

- Line 297: `block_bytes.get(children_start..).unwrap_or(&[])` — safe, fallback to empty slice
- Line 360: `bytes.get(next_offset..).unwrap_or(&[])` — safe, fallback to empty slice
- Line 797: `file_stem.and_then(|stem| stem.to_str()).unwrap_or(trimmed)` — safe fallback
- No truly problematic `.unwrap()` calls in library code.

### Command::new / Subprocess Usage

**Status**: PASS

No subprocess calls found.

## Findings Summary

| #   | Principle        | Severity | Line(s) | Description                                                                                           |
| --- | ---------------- | -------- | ------- | ----------------------------------------------------------------------------------------------------- |
| 1   | P2 File Size     | MEDIUM   | —       | No file size check in this module for main binary bytes (caller's responsibility)                     |
| 2   | P2 Iteration     | LOW      | 348     | No 100K cap on version block iteration                                                                |
| 3   | P2 String Length | LOW      | —       | No field-level 10MB truncation                                                                        |
| 4   | P4 UTF-8         | MEDIUM   | 423     | `String::from_utf16().ok()` silently drops invalid UTF-16; should use lossy conversion or log warning |

## Remediation Priority

1. Replace `String::from_utf16().ok()` with `String::from_utf16_lossy()` at line 423
2. Add iteration cap (100K) on version block processing
3. Consider adding explicit file size check documentation for callers
