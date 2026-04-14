# ADR 0004 Security Audit: gradle_lock

**File**: `src/parsers/gradle_lock.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `subprocess`, `eval()`, or code execution. Uses `BufReader` line-by-line text parsing. All parsing is static string splitting.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading. Opens file via `File::open` at line 46 without size pre-check. `BufReader` provides streaming, but no limit on total lines or file size.

### Recursion Depth

No recursive functions. All parsing is iterative (`for line in reader.lines()` at line 108). **PASS**.

### Iteration Count

No 100K iteration cap on the line-reading loop at line 108. A lockfile with >100K dependency lines would be processed without early termination.

### String Length

No 10 MB truncation with warning on any field value. Group, artifact, version strings from parsed lines (lines 158-160) are stored without size limits.

## Principle 3: Archive Safety

**Status**: N/A

Gradle lockfile parser does not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. Uses `File::open` at line 46 with error handling that returns `default_package_data()` on failure (line 50). Returns error not panic, but doesn't use `fs::metadata()` as specified.

### UTF-8 Encoding

`reader.lines()` (line 108) returns `io::Result<String>` which handles UTF-8. Invalid UTF-8 lines produce an `Err` which is logged and skipped (line 111-114). No explicit lossy conversion — invalid lines are silently skipped without a warning about encoding issues.

### JSON/YAML Validity

Returns default `PackageData` on file open failure (line 50). Malformed lines are silently skipped via `parse_dependency_line` returning `None`. **PASS**.

### Required Fields

Missing group/artifact/version (wrong number of colon-separated parts) causes `parse_dependency_line` to return `None` (line 154-156). **PASS**.

### URL Format

N/A — no URL fields parsed.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution performed.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code. All instances are in `#[cfg(test)]` blocks (lines 267, 271, 275, 279, 290, 294, 298, 309, 313, 336-338, 348-349, 378, 389, 402, 416-417).

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle           | Severity | Line(s) | Description                                                               |
| --- | ------------------- | -------- | ------- | ------------------------------------------------------------------------- |
| 1   | P2: File Size       | Medium   | 46      | No `fs::metadata().len()` check before reading; 100 MB limit not enforced |
| 2   | P2: Iteration Count | Low      | 108     | No 100K iteration cap on line-reading loop                                |
| 3   | P2: String Length   | Low      | 158-160 | No 10 MB truncation with warning on parsed field values                   |
| 4   | P4: File Exists     | Low      | 46      | Uses `File::open` instead of `fs::metadata()` pre-check                   |
| 5   | P4: UTF-8 Encoding  | Low      | 108-114 | Invalid UTF-8 lines skipped without explicit lossy conversion warning     |

## Remediation Priority

1. Add `fs::metadata().len()` check with 100 MB limit before reading file (line 46)
2. Add iteration count cap (100K) on line-reading loop
3. Add 10 MB string field truncation with warning
4. Add `fs::metadata()` pre-check before `File::open`
5. Add explicit lossy UTF-8 conversion with warning for encoding errors

## Remediation

| #   | Finding             | Fix                                                                                                      |
| --- | ------------------- | -------------------------------------------------------------------------------------------------------- |
| 1   | P2: File Size       | Replaced `File::open`+`BufReader` with `read_file_to_string(path, None)` which enforces 100MB size limit |
| 2   | P2: Iteration Count | Added `.take(MAX_ITERATION_COUNT)` to line iteration                                                     |
| 3   | P2: String Length   | Applied `truncate_field()` to group, artifact, version, purl, and configuration strings                  |
| 4   | P4: File Exists     | `read_file_to_string` uses `fs::metadata()` pre-check internally                                         |
| 5   | P4: UTF-8 Encoding  | `read_file_to_string` provides lossy UTF-8 fallback internally                                           |
