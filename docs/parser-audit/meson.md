# ADR 0004 Security Audit: meson

**File**: `src/parsers/meson.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

Implements a custom tokenizer (line 455) and recursive-descent parser (lines 551-672) for Meson build syntax. This is static analysis — no `Command::new`, `subprocess`, `eval()`, or code execution. The parser tokenizes and parses without executing any Meson code.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `extract_packages` (line 24) uses `fs::read_to_string(path)` without a size check. Additionally, the parser materializes the entire input as `Vec<char>` in `strip_comments` (line 298) and `tokenize` (line 456), doubling memory usage.

### Recursion Depth

**CRITICAL**: `parse_expr` (line 561) is mutually recursive with `parse_array` and `parse_identifier_or_call`:

- `parse_expr()` (line 561) dispatches to `parse_array()` (line 571) and `parse_identifier_or_call()` (line 572)
- `parse_array()` (line 578) calls `parse_expr()` at line 582
- `parse_identifier_or_call()` (line 593) calls `parse_expr()` at lines 617 and 620

These form mutual recursion: `parse_expr → parse_array/parse_identifier_or_call → parse_expr`. There is **no depth tracking** in the `Parser` struct and **no recursion limit**. A deeply nested input (e.g., `[[[[[...]]]]]`) could cause a stack overflow.

Additionally, `parse_statement` (line 435) calls `parse_expr` at lines 443 and 450.

### Iteration Count

No 100K iteration cap on:

- `strip_comments` (line 297): iterates over all characters without cap
- `split_statements` (line 348): iterates over all characters without cap
- `tokenize` (line 455): iterates over all characters without cap
- `parse_meson_build` (line 48): iterates over all statements without cap
- `parse_array` (line 580): iterates over all array elements without cap
- `parse_identifier_or_call` (line 610): iterates over all call arguments without cap
- `extract_dependencies_from_call` (line 177): processes dependency names without cap

### String Length

No 10 MB truncation with warning on any field value. `tokenize` extracts string tokens (lines 496-520) without size limits. The `strip_comments` output string (line 299) and `split_statements` output strings have no size limits.

## Principle 3: Archive Safety

**Status**: N/A

Meson parser does not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. Uses `fs::read_to_string(path)` (line 24) with error handling that returns `default_package_data()` (lines 26-29). Does not panic, but doesn't use `fs::metadata()` as specified.

### UTF-8 Encoding

`fs::read_to_string` returns an error for non-UTF-8 files. Error is caught and fallback data returned. No explicit `String::from_utf8()` + warning + lossy conversion path.

### JSON/YAML Validity

N/A — parser uses custom Meson syntax parsing, not JSON/YAML. Parse failures return `Err(String)` which is caught at line 32-35, returning `default_package_data()`. **PASS**.

### Required Fields

Missing project name causes `apply_project_call` to return early (line 113). Package-level `name` defaults to `None`. PURL is only generated when name is present (lines 83-86). **PASS**.

### URL Format

N/A — no URL fields directly parsed from user input.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution performed. Dependencies are extracted from build file declarations only.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle           | Severity | Line(s)                     | Description                                                                                                                                  |
| --- | ------------------- | -------- | --------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | P2: Recursion Depth | High     | 561, 578, 593               | Mutual recursion (`parse_expr` ↔ `parse_array`/`parse_identifier_or_call`) with no depth limit; deeply nested input can cause stack overflow |
| 2   | P2: File Size       | Medium   | 24, 298, 456                | No `fs::metadata().len()` check before reading; entire file loaded into memory and materialized as `Vec<char>`                               |
| 3   | P2: Iteration Count | Low      | 297, 348, 455, 48, 580, 610 | No 100K iteration cap on character/statement/argument processing                                                                             |
| 4   | P2: String Length   | Low      | 496-520                     | No 10 MB truncation with warning on string token values                                                                                      |
| 5   | P4: File Exists     | Low      | 24                          | Uses `fs::read_to_string` instead of `fs::metadata()` pre-check                                                                              |
| 6   | P4: UTF-8 Encoding  | Low      | 24                          | No lossy UTF-8 conversion path; invalid UTF-8 causes fallback data return                                                                    |

## Remediation Priority

1. **CRITICAL**: Add recursion depth tracking to `Parser` struct with 50-level limit; check depth on each `parse_expr` entry (lines 561, 578, 593)
2. Add `fs::metadata().len()` check with 100 MB limit before reading file (line 24)
3. Add iteration count cap (100K) on character/statement/argument processing loops
4. Add 10 MB string token truncation with warning
5. Add `fs::metadata()` pre-check before file read
6. Add lossy UTF-8 conversion with warning for encoding errors
