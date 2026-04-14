# ADR 0004 Security Audit: rpm_parser

**File**: `src/parsers/rpm_parser.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, or subprocess calls. Uses the `rpm` crate for binary RPM package parsing via `Package::parse()` (line 184), which is a static Rust library — no shell execution.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `File::open()` at line 175 opens the file and `Package::parse()` reads it without pre-checking size. The `is_match()` function at line 166 also opens the file without size checks.

### Recursion Depth

No recursive functions. PASS.

### Iteration Count

No 100K iteration cap on loops:

- `extract_rpm_dependencies()` line 405: iterates all requires without limit
- `extract_rpm_relationships()` line 453: iterates all relationships without limit

### String Length

No 10MB truncation on field values from RPM metadata.

## Principle 3: Archive Safety

**Status**: N/A

RPM package files are parsed structurally (not extracted as archives in the traditional sense). The `rpm` crate handles decompression internally.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

`File::open()` at line 175 returns an error on missing files (handled at lines 176-179). No explicit `fs::metadata()` pre-check.

### UTF-8 Encoding

The `rpm` crate's `get_name()`, `get_version()`, etc. return `String` types. The internal `rpm_header_string()` function at line 93 uses `get_entry_data_as_string()` which likely handles UTF-8 internally. No explicit lossy conversion fallback in this file.

### JSON/YAML Validity

No JSON/YAML parsing. N/A.

### Required Fields

`parse_rpm_package()` line 221: `name` from `metadata.get_name()` is `Option<String>`. Missing name results in `None`. `version` from `build_evr_version()` returns `None` if version is missing. Acceptable per ADR.

### URL Format

URLs accepted as-is. PASS per ADR.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: FAIL

- Line 574: `.unwrap()` in test code only — acceptable.
- No `.unwrap()` in non-test library code. PASS for library code.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle | Severity | Line(s) | Description                                               |
| --- | --------- | -------- | ------- | --------------------------------------------------------- |
| 1   | P2        | HIGH     | 175,166 | No file size check before reading RPM files (100MB limit) |
| 2   | P2        | MEDIUM   | 405,453 | No iteration count cap (100K items)                       |
| 3   | P2        | MEDIUM   | —       | No string length truncation (10MB per field)              |
| 4   | P4        | LOW      | 175     | No explicit fs::metadata() pre-check                      |
| 5   | P4        | LOW      | —       | No explicit lossy UTF-8 fallback for RPM metadata strings |

## Remediation Priority

1. Add fs::metadata().len() check before opening RPM files with 100MB limit
2. Add iteration count caps on dependency/relationship loops
3. Add string length truncation on field values
