# ADR 0004 Security Audit: conan_data

**File**: `src/parsers/conan_data.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

- YAML parsing via `yaml_serde::from_str` at line 102 — static deserialization
- No `Command::new`, `subprocess`, `eval()`, `exec()` anywhere

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

- No `fs::metadata().len()` check before `fs::read_to_string` at line 89
- Entire file read into memory without size limit

### Recursion Depth

- No recursive functions in this parser
- PASS

### Iteration Count

- `parse_conandata_yml` iterates `sources` HashMap entries without cap at line 116
- **GAP**: No 100,000 item cap on source entries

### String Length

- No 10MB per-field truncation for parsed URL strings, SHA256 hashes, or patch descriptions

## Principle 3: Archive Safety

**Status**: N/A

Not an archive parser.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

- No `fs::metadata()` pre-check
- `fs::read_to_string` failure handled with `warn!` and default return at lines 90-94

### UTF-8 Encoding

- `fs::read_to_string` fails on invalid UTF-8 without lossy fallback
- **GAP**: No lossy UTF-8 conversion with warning

### JSON/YAML Validity

- YAML parse failure returns `vec![default_package_data()]` at lines 103-107
- Missing `sources` field returns default at lines 110-112
- PASS — graceful degradation

### Required Fields

- No name field extracted from conandata.yml (format doesn't include it)
- Version extracted from HashMap key at line 116 — always present if in sources
- Empty sources results in default package data at lines 171-173

### URL Format

- URLs accepted as-is — compliant with ADR 0004

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution with circular dependency risk.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

- No `.unwrap()` calls in library code
- All `Option`/`Result` values handled with `match` or `?`

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle       | Severity | Line(s) | Description                                    |
| --- | --------------- | -------- | ------- | ---------------------------------------------- |
| 1   | P2-FileSize     | HIGH     | 89      | No file size check before `fs::read_to_string` |
| 2   | P2-Iteration    | LOW      | 116     | No 100K iteration cap on source entries        |
| 3   | P2-StringLength | LOW      | N/A     | No 10MB per-field truncation                   |
| 4   | P4-UTF8         | LOW      | N/A     | No lossy UTF-8 fallback                        |

## Remediation Priority

1. Add `fs::metadata().len()` check (100MB limit) before reading conandata.yml
2. Add 100K iteration caps on source entries
3. Add lossy UTF-8 fallback with warning log
