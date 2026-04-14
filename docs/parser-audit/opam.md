# ADR 0004 Security Audit: opam

**File**: `src/parsers/opam.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

No code execution mechanisms. Uses custom regex-based text parsing and manual line-by-line extraction. The `Regex::new()` calls at lines 388, 406, 471 are static pattern compilation, not code execution. Fully static analysis.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `std::fs::read_to_string(path)` at line 55 — no size pre-check.

### Recursion Depth

No recursive functions. All parsing is iterative using `while` loops with index manipulation (e.g., `parse_opam_data` at line 211, `parse_multiline_string` at line 295, `parse_string_array` at line 324).

### Iteration Count

- `parse_opam_data` (line 216): `while i < lines.len()` — no cap on line count
- `parse_multiline_string` (line 304): `while *i < lines.len()` — no cap
- `parse_string_array` (line 332): `while *i < lines.len()` — no cap
- `parse_dependency_array` (line 363): `while *i < lines.len()` — no cap
- `parse_checksums` (line 446): `while *i < lines.len()` — no cap
- No 100K iteration cap anywhere

### String Length

No field value truncation at 10MB. String values from parsed fields stored without length checks.

## Principle 3: Archive Safety

**Status**: N/A

No archive extraction performed.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `std::fs::read_to_string(path)` at line 55 returns an error on failure, handled at lines 56-59 with `warn!()` and default return. No explicit pre-check.

### UTF-8 Encoding

Uses `std::fs::read_to_string()` which fails on invalid UTF-8 without lossy fallback.

### JSON/YAML Validity

No JSON/YAML parsing. OPAM format is custom text. Regex parsing failures at lines 388, 406, 471 are handled gracefully via `.ok()?` which returns `None`. **PASS**

### Required Fields

Missing `name`/`version` handled via `Option<String>` in `OpamData`. Correct.

### URL Format

URLs accepted as-is. ADR-compliant.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: FAIL

- Line 410: `caps.get(1).map(|m| m.as_str()).unwrap_or("")` — acceptable (fallback)
- Line 411: `caps.get(2).map(|m| m.as_str()).unwrap_or("")` — acceptable (fallback)
- Lines 291: `VERSION_CONSTRAINT_RE` uses `.unwrap()` in `lazy_static!` block — this is a compile-time constant regex, acceptable.
- No raw `.unwrap()` without fallback in runtime library code. **Actually PASS on closer inspection**

### Command::new / Subprocess Usage

**Status**: PASS

None.

## Findings Summary

| #   | Principle           | Severity | Line(s)                 | Description                                                         |
| --- | ------------------- | -------- | ----------------------- | ------------------------------------------------------------------- |
| 1   | P2: File Size       | HIGH     | 55                      | No `fs::metadata().len()` check before reading; unbounded file read |
| 2   | P2: Iteration Count | MEDIUM   | 216, 304, 332, 363, 446 | No 100K iteration cap on line processing loops                      |
| 3   | P2: String Length   | LOW      | various                 | No 10MB field value truncation                                      |
| 4   | P4: UTF-8 Encoding  | MEDIUM   | 55                      | No lossy UTF-8 fallback on read failure                             |

## Remediation Priority

1. Add `fs::metadata().len()` check before reading (100MB default limit)
2. Add 100K iteration cap in line processing loops
3. Add lossy UTF-8 fallback on read failure
4. Add 10MB field value truncation with warning

## Remediation

- **#1 P2 File Size**: Replaced `std::fs::read_to_string` with `read_file_to_string(path, None)` — enforces 100MB size check before reading and provides lossy UTF-8 fallback.
- **#2 P2 Iteration**: Added `MAX_ITERATION_COUNT` counter caps to all 5 while loops (parse_opam_data, parse_multiline_string, parse_string_array, parse_dependency_array, parse_checksums).
- **#3 P2 String Length**: Applied `truncate_field()` to all extracted string values.
- **#4 P4 UTF-8**: Fixed automatically by `read_file_to_string` — lossy UTF-8 conversion replaces silent failure on non-UTF-8 content.
