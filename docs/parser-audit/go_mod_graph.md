# ADR 0004 Security Audit: go_mod_graph

**File**: `src/parsers/go_mod_graph.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

Uses line-by-line text parsing with `split_whitespace` and `rsplit_once` for static analysis. No `Command::new`, `subprocess`, `eval()`, or any code execution mechanism.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading. `extract_packages` (line 35) uses `fs::read_to_string(path)` without a size check.

### Recursion Depth

No recursive functions. All parsing is iterative over `content.lines()`. **PASS**.

### Iteration Count

No 100K iteration cap on:

- `parse_go_mod_graph` (line 57): iterates over all lines without cap
- Lines are inserted into `dependency_map` (BTreeMap) without cap on total entries

### String Length

No 10 MB truncation with warning on any field value. Module paths and versions are extracted without size limits (lines 74-75, 137-148).

## Principle 3: Archive Safety

**Status**: N/A

Go mod graph parser does not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. Uses `fs::read_to_string(path)` (line 35) with error handling that returns `default_package_data()` (lines 37-40). Does not panic, but doesn't use `fs::metadata()` as specified.

### UTF-8 Encoding

`fs::read_to_string` returns an error for non-UTF-8 files. Error is caught and fallback data returned. No explicit `String::from_utf8()` + warning + lossy conversion path.

### JSON/YAML Validity

N/A — parser does not parse JSON/YAML. Input is plain text.

### Required Fields

Missing module name handled gracefully. `root_module` defaults to `None` (line 54). When name is empty, it's filtered out at line 127: `(!name.is_empty()).then_some(name)`. **PASS**.

### URL Format

URLs are constructed programmatically (lines 112-116), not parsed from user input. **PASS**.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution performed. Dependencies are extracted from graph lines only.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code. Test code uses `.unwrap()` at lines 183, 189, 207, 213 which is acceptable.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle           | Severity | Line(s)        | Description                                                                    |
| --- | ------------------- | -------- | -------------- | ------------------------------------------------------------------------------ |
| 1   | P2: File Size       | Medium   | 35             | No `fs::metadata().len()` check before reading; entire file loaded into memory |
| 2   | P2: Iteration Count | Low      | 57             | No 100K iteration cap on line processing                                       |
| 3   | P2: String Length   | Low      | 74-75, 137-148 | No 10 MB truncation with warning on string field values                        |
| 4   | P4: File Exists     | Low      | 35             | Uses `fs::read_to_string` instead of `fs::metadata()` pre-check                |
| 5   | P4: UTF-8 Encoding  | Low      | 35             | No lossy UTF-8 conversion path; invalid UTF-8 causes fallback data return      |

## Remediation Priority

1. Add `fs::metadata().len()` check with 100 MB limit before reading file (line 35)
2. Add iteration count cap (100K) on line processing
3. Add 10 MB string field truncation with warning
4. Add `fs::metadata()` pre-check before file read
5. Add lossy UTF-8 conversion with warning for encoding errors
