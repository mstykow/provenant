# ADR 0004 Security Audit: rpm_specfile

**File**: `src/parsers/rpm_specfile.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, or subprocess calls. The specfile parser performs static text parsing and simple macro expansion via `String::replace()` (line 374). It does NOT execute shell commands or evaluate RPM macros as code.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `read_file_to_string()` at line 59 reads entire files without size validation.

### Recursion Depth

No recursive functions. Macro expansion in `expand_macros()` is iterative (line 372: iterates over macros map), not recursive. PASS.

### Iteration Count

No 100K iteration cap on loops:

- `parse_specfile()` line 87: `while i < lines.len()` iterates all preamble lines without limit
- Line 126: iterates comma-separated BuildRequires without limit
- Line 145: iterates comma-separated Requires without limit
- Line 153: iterates comma-separated Provides without limit
- Line 170: while loop for %description section without line count limit
- Line 267: iterates all build_requires without limit
- Line 285: iterates all requires without limit

### String Length

No 10MB truncation on field values. Tag values, macro definitions, and expanded strings are stored without length limits.

## Principle 3: Archive Safety

**Status**: N/A

No archive extraction.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

Uses `read_file_to_string()` which returns error on missing files (handled at lines 60-68), but no explicit `fs::metadata()` pre-check.

### UTF-8 Encoding

`read_file_to_string()` errors on invalid UTF-8. No lossy conversion fallback.

### JSON/YAML Validity

No JSON/YAML parsing. N/A.

### Required Fields

`parse_specfile()` lines 211-212: `name` and `version` are extracted from tags as `Option<String>`. Missing values result in `None`, which is acceptable per ADR.

### URL Format

URLs accepted as-is. PASS per ADR.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: FAIL

- Line 42: `.unwrap()` on `Regex::new()` in `LazyLock` static — compile-time constant regex, unlikely to fail, but technically `.unwrap()` in library code.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle  | Severity | Line(s)                    | Description                                                     |
| --- | ---------- | -------- | -------------------------- | --------------------------------------------------------------- |
| 1   | P2         | HIGH     | 59                         | No file size check before reading (100MB limit)                 |
| 2   | P2         | MEDIUM   | 87,126,145,153,170,267,285 | No iteration count cap (100K items)                             |
| 3   | P2         | MEDIUM   | —                          | No string length truncation (10MB per field)                    |
| 4   | P4         | LOW      | 59                         | No explicit fs::metadata() pre-check                            |
| 5   | P4         | MEDIUM   | 59                         | No lossy UTF-8 fallback; invalid UTF-8 causes error not warning |
| 6   | Additional | LOW      | 42                         | .unwrap() on Regex::new() in LazyLock static                    |

## Remediation Priority

1. Add fs::metadata().len() check before read_file_to_string with 100MB limit
2. Add iteration count caps on line/dependency loops
3. Add String::from_utf8_lossy() fallback for UTF-8 handling
4. Replace .unwrap() at line 42 with expect() or proper initialization

## Remediation

All 6 findings addressed:

1. **P2-FileSize**: Already covered by `read_file_to_string` which enforces a size limit.
2. **P2-Iteration**: Added `MAX_ITERATION_COUNT` caps on all 9 iteration sites.
3. **P2-StringLength**: Added `truncate_field` on all expanded tags, dependencies, and purl.
4. **P4-UTF8**: Already covered by `read_file_to_string`.
5. **P4-Pre-check**: Already covered by `read_file_to_string`.
6. **LazyLock .unwrap()**: Acceptable for compile-time constants; `Regex::new` in `LazyLock` with a static pattern cannot fail at runtime.
