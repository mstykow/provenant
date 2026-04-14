# ADR 0004 Security Audit: helm

**File**: `src/parsers/helm.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

No code execution mechanisms. Uses `yaml_serde::from_str()` for YAML parsing (line 61) and manual field extraction. Fully static analysis.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. Uses `fs::read_to_string(path)` at line 59. A very large Chart.yaml/Chart.lock would be read entirely into memory.

### Recursion Depth

No recursive functions. All parsing is iterative over YAML value structures.

### Iteration Count

Loops over dependencies (`entries.iter()` at lines 126-131, 190-195), maintainers (lines 250-269), and `extract_string_values` (line 285) have no 100K iteration cap. A Chart.yaml with >100K dependencies would process all entries.

### String Length

No field value truncation at 10MB. String values extracted via `extract_string_field` (line 271) are stored without length checks.

## Principle 3: Archive Safety

**Status**: N/A

No archive extraction performed.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. Uses `fs::read_to_string(path)` which returns an error (line 59). Error handling returns default `PackageData` — acceptable but no explicit pre-check.

### UTF-8 Encoding

Uses `fs::read_to_string()` which fails on invalid UTF-8 without lossy fallback. No `String::from_utf8_lossy()` usage.

### JSON/YAML Validity

YAML parse failure at line 61 returns `Err`, which is handled by returning `default_package_data()` at lines 28-29 and 48-50. Correct behavior.

### Required Fields

Missing `name`/`version` handled via `Option<String>` — `None` populated when fields absent. Correct.

### URL Format

URLs accepted as-is from YAML fields. ADR-compliant.

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

| #   | Principle           | Severity | Line(s)                   | Description                                                         |
| --- | ------------------- | -------- | ------------------------- | ------------------------------------------------------------------- |
| 1   | P2: File Size       | HIGH     | 59                        | No `fs::metadata().len()` check before reading; unbounded file read |
| 2   | P2: Iteration Count | MEDIUM   | 126-131, 190-195, 250-269 | No 100K iteration cap on dependency/maintainer loops                |
| 3   | P2: String Length   | LOW      | 271                       | No 10MB field value truncation                                      |
| 4   | P4: UTF-8 Encoding  | MEDIUM   | 59                        | No lossy UTF-8 fallback on read failure                             |

## Remediation Priority

1. Add `fs::metadata().len()` check before reading (100MB default limit) — line 59
2. Add lossy UTF-8 fallback on read failure — line 59
3. Add 100K iteration cap in dependency extraction loops
4. Add 10MB field value truncation with warning

## Remediation

| #   | Finding             | Fix                                                                                      |
| --- | ------------------- | ---------------------------------------------------------------------------------------- |
| 1   | P2: File Size       | Replaced `fs::read_to_string` with `read_file_to_string` which enforces 100MB size limit |
| 2   | P2: Iteration Count | Added `MAX_ITERATION_COUNT` caps on 4 iteration sites (dependencies, maintainers, etc.)  |
| 3   | P2: String Length   | Applied `truncate_field` on all output strings                                           |
| 4   | P4: UTF-8 Encoding  | Fixed by `read_file_to_string` which provides lossy UTF-8 fallback                       |
