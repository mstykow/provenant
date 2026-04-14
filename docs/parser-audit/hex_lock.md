# ADR 0004 Security Audit: hex_lock

**File**: `src/parsers/hex_lock.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

Implements a custom recursive-descent parser (lines 324-579) for Elixir term syntax. This is static analysis — no `Command::new`, `subprocess`, `eval()`, or code execution. The parser tokenizes and parses character-by-character without executing any code.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `extract_packages` (line 43) uses `fs::read_to_string(path)` without a size check.

### Recursion Depth

**CRITICAL**: `parse_term` (line 333) is mutually recursive with `parse_map`, `parse_tuple`, `parse_list`, and `parse_string`/`parse_atom`/`parse_integer`/`parse_bool`:

- `parse_term()` (line 333) dispatches to `parse_map()`, `parse_tuple()`, `parse_list()`
- `parse_map()` (line 348) calls `parse_term()` at lines 358 and 365
- `parse_tuple()` (line 375) calls `parse_term()` at line 384
- `parse_list()` (line 393) calls `parse_term()` at lines 408 and 411

These form mutual recursion: `parse_term → parse_map/parse_tuple/parse_list → parse_term`. There is **no depth tracking** in the `Parser` struct and **no recursion limit**. A deeply nested input (e.g., `%{%{%{...} => ...} => ...} => ...}`) could cause a stack overflow.

### Iteration Count

No 100K iteration cap on:

- `parse_map` (line 352): loops over map entries without cap
- `parse_tuple` (line 378): loops over tuple items without cap
- `parse_list` (line 399): loops over list items without cap
- `parse_mix_lock` (line 84): iterates over all map entries without cap
- `term_to_dependency_tuples` (line 222): iterates over all list items without cap

### String Length

No 10 MB truncation with warning on any field value. `parse_string` (line 455) reads string content character-by-character into `out` without size limits. `parse_atom` (line 482) reads atom names without size limits.

## Principle 3: Archive Safety

**Status**: N/A

Hex lock parser does not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. Uses `fs::read_to_string(path)` (line 43) with error handling that returns `default_package_data()` (lines 45-48). Does not panic, but doesn't use `fs::metadata()` as specified.

### UTF-8 Encoding

`fs::read_to_string` returns an error for non-UTF-8 files. The custom parser operates on `Vec<char>` derived from a valid UTF-8 string. No explicit `String::from_utf8()` + warning + lossy conversion path for the file read.

### JSON/YAML Validity

N/A — parser uses custom term parsing, not JSON/YAML. Parse failures return `Err(String)` which is caught at line 51-57, returning `default_package_data()`. **PASS**.

### Required Fields

Missing fields in lock entries cause `build_dependency_from_lock_entry` to return `Ok(None)` (lines 103, 107, 113). These are skipped in `parse_mix_lock` (line 85). Package-level name/version default to `None`. **PASS**.

### URL Format

URLs are constructed programmatically (lines 284-297), not parsed from user input. **PASS**.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution performed. Dependencies are extracted from lockfile declarations only.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle           | Severity | Line(s)            | Description                                                                                                                                |
| --- | ------------------- | -------- | ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------ |
| 1   | P2: Recursion Depth | High     | 333, 348, 375, 393 | Mutual recursion (`parse_term` ↔ `parse_map`/`parse_tuple`/`parse_list`) with no depth limit; deeply nested input can cause stack overflow |
| 2   | P2: File Size       | Medium   | 43                 | No `fs::metadata().len()` check before reading; entire file loaded into memory                                                             |
| 3   | P2: Iteration Count | Low      | 352, 378, 399, 84  | No 100K iteration cap on map/tuple/list entry processing                                                                                   |
| 4   | P2: String Length   | Low      | 455, 482           | No 10 MB truncation with warning on string/atom values                                                                                     |
| 5   | P4: File Exists     | Low      | 43                 | Uses `fs::read_to_string` instead of `fs::metadata()` pre-check                                                                            |
| 6   | P4: UTF-8 Encoding  | Low      | 43                 | No lossy UTF-8 conversion path; invalid UTF-8 causes fallback data return                                                                  |

## Remediation Priority

1. **CRITICAL**: Add recursion depth tracking to `Parser` struct with 50-level limit; check depth on each `parse_term` entry (lines 333, 348, 375, 393)
2. Add `fs::metadata().len()` check with 100 MB limit before reading file (line 43)
3. Add iteration count cap (100K) on map/tuple/list entry processing loops
4. Add 10 MB string/atom field truncation with warning
5. Add `fs::metadata()` pre-check before file read
6. Add lossy UTF-8 conversion with warning for encoding errors
