# ADR 0004 Security Audit: debian

**File**: `src/parsers/debian.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, subprocess calls, or shell execution found. All parsing is static: RFC 822 text parsing, regex-based dependency parsing, filename parsing, and tar/ar archive reading via Rust libraries.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `read_file_to_string()` at line 176 (and similar calls at lines 208, 236, 884, 1158, 1204, 1316, 1936, 1992) reads the entire file into memory without size validation. The shared utility `read_file_to_string` (src/parsers/utils.rs:33) also lacks a size check.

### Recursion Depth

No recursive functions found. Parsing is iterative. PASS.

### Iteration Count

No 100K iteration cap on loops over lines/paragraphs/dependencies:

- `parse_dpkg_status()` line 322: iterates all paragraphs without limit
- `parse_debian_control()` line 294: iterates all paragraphs without limit
- `parse_dependency_field()` line 741: iterates comma-separated deps without limit
- `parse_copyright_file()` line 1386: iterates all paragraphs without limit
- `parse_debian_file_list()` line 1245: iterates all lines without limit
- `extract_file_references()` line 617 in alpine.rs (not this file, but pattern applies)
- `parse_copyright_holders()` line 1626: iterates all lines without limit

### String Length

No 10MB truncation on individual field values. Header values from RFC 822 parsing (e.g., dependency strings, description fields) are stored without length limits.

## Principle 3: Archive Safety

**Status**: FAIL

### debian.rs Archive Extraction

- `extract_deb_archive()` line 1719: Opens `.deb` ar archives and extracts control.tar.gz/xz and data.tar.gz/xz without:
  - Uncompressed size limit (1 GB required)
  - Compression ratio check (100:1 max required)
  - Path traversal check (`../` patterns)
  - Decompression limit (1 GB required)
- `apk_contains_pkginfo()` is in alpine.rs, not here, but `apk_contains_pkginfo` at line 756 in alpine.rs reads tar entries without limits
- `parse_control_tar_archive()` line 1776: Reads tar entries without size/ratio limits
- `merge_deb_data_archive()` line 1816: Reads data tar entries without size/ratio limits
- `control_data` read via `read_to_end()` at line 1739 without size limit
- `data` read via `read_to_end()` at line 1755 without size limit

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

Uses `read_file_to_string()` which returns an error on missing files (handled at lines 177-181, 208-213, etc.), but does NOT perform an explicit `fs::metadata()` pre-check. Returns default PackageData on error rather than error type.

### UTF-8 Encoding

`read_file_to_string()` at utils.rs:33 uses `file.read_to_string()` which will error on invalid UTF-8 rather than falling back to lossy conversion. In `extract_deb_archive()` line 1732, `std::str::from_utf8()` is used for entry names which returns an error on invalid UTF-8. `String::from_utf8_lossy()` is NOT used as a fallback anywhere.

### JSON/YAML Validity

No JSON/YAML parsing in this file. N/A.

### Required Fields

`build_package_from_paragraph()` line 433: `name` is extracted with `?` (returns None if missing, causing the whole package to be skipped). `version` is `Option<String>` — missing version results in `None`, which is acceptable per ADR (populate with None, continue).

### URL Format

URLs are accepted as-is (lines 405-413 for homepage, VCS URLs). PASS per ADR.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution; only parsing declared dependencies from metadata.

## Additional Checks

### .unwrap() in Library Code

**Status**: FAIL

- Line 739: `.unwrap()` on `Regex::new()` in `parse_dependency_field()` — this is a compile-time-constant regex, so it will never fail at runtime, but it is technically `.unwrap()` in library code.
- Line 1598: `.unwrap()` on `LineNumber::new(line_no)` in `build_primary_license_detection()` — could theoretically fail if line_no is 0.
- Line 1038: `.unwrap_or(false)` on is_match — acceptable defensive pattern, not a bare unwrap.
- Line 1074: `.unwrap_or(false)` — acceptable defensive pattern.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle  | Severity | Line(s)                        | Description                                                     |
| --- | ---------- | -------- | ------------------------------ | --------------------------------------------------------------- |
| 1   | P2         | HIGH     | 176,208,236,884,1158,1204,1316 | No file size check before reading (100MB limit)                 |
| 2   | P2         | MEDIUM   | 322,294,741,1245,1386          | No iteration count cap (100K items)                             |
| 3   | P2         | MEDIUM   | —                              | No string length truncation (10MB per field)                    |
| 4   | P3         | HIGH     | 1719,1776,1816                 | No archive size/ratio/path-traversal limits on .deb extraction  |
| 5   | P4         | LOW      | 176,208,etc.                   | No explicit fs::metadata() pre-check; relies on read error      |
| 6   | P4         | MEDIUM   | 1732,utils.rs:36               | No lossy UTF-8 fallback; invalid UTF-8 causes error not warning |
| 7   | Additional | LOW      | 739,1598                       | .unwrap() in library code (regex and LineNumber)                |

## Remediation Priority

1. Add archive safety limits to .deb extraction (uncompressed size 1GB, compression ratio 100:1, path traversal blocking)
2. Add fs::metadata().len() check before read_file_to_string with 100MB limit
3. Add iteration count caps on paragraph/line/dependency loops
4. Add String::from_utf8_lossy() fallback for UTF-8 handling
5. Replace .unwrap() calls with proper error handling
