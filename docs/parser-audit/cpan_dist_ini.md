# ADR 0004 Security Audit: cpan_dist_ini

**File**: `src/parsers/cpan_dist_ini.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No code execution mechanisms. Uses custom INI-style text parsing (`parse_ini_structure` at line 124) with `line.split_once('=')` for key-value extraction. Fully static analysis.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `fs::read_to_string(path)` at line 41 — no size pre-check.

### Recursion Depth

No recursive functions. All parsing is iterative.

### Iteration Count

- `parse_ini_structure` (line 134): `for line in content.lines()` — no cap on line count
- `parse_dependencies` (line 212): iterates over sections and fields with no cap
- No 100K iteration cap anywhere

### String Length

No field value truncation at 10MB. String values from INI fields stored without length checks.

## Principle 3: Archive Safety

**Status**: N/A

No archive extraction performed.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `fs::read_to_string(path)` at line 41 returns an error on failure, handled correctly at lines 42-51.

### UTF-8 Encoding

Uses `fs::read_to_string()` which fails on invalid UTF-8 without lossy fallback.

### JSON/YAML Validity

No JSON/YAML parsing. INI format is custom text. **N/A**

### Required Fields

Missing `name`/`version` handled via `Option<String>`. Correct.

### URL Format

URLs accepted as-is. ADR-compliant.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code.

### Command::new / Subprocess Usage

**Status**: PASS

None.

## Findings Summary

| #   | Principle           | Severity | Line(s)  | Description                                                         |
| --- | ------------------- | -------- | -------- | ------------------------------------------------------------------- |
| 1   | P2: File Size       | HIGH     | 41       | No `fs::metadata().len()` check before reading; unbounded file read |
| 2   | P2: Iteration Count | MEDIUM   | 134, 212 | No 100K iteration cap on line/dependency loops                      |
| 3   | P2: String Length   | LOW      | various  | No 10MB field value truncation                                      |
| 4   | P4: UTF-8 Encoding  | MEDIUM   | 41       | No lossy UTF-8 fallback on read failure                             |

## Remediation Priority

1. Add `fs::metadata().len()` check before reading (100MB default limit)
2. Add 100K iteration cap in line/dependency processing loops
3. Add lossy UTF-8 fallback on read failure
4. Add 10MB field value truncation with warning
