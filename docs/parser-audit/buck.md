# ADR 0004 Security Audit: buck

**File**: `src/parsers/buck.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

Uses `starlark_syntax::AstModule::parse` (line 132) for AST-based parsing. Same approach as `bazel.rs` — the `starlark_syntax` crate produces an AST without executing Starlark code. No `Command::new`, `subprocess`, `eval()`, or code execution.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading. Both `parse_buck_build` (line 91) and `parse_metadata_bzl` (line 108) read the entire file into memory via `std::fs::read_to_string(path)` without a size check.

### Recursion Depth

No explicit recursion depth tracking. The Starlark AST parser handles nesting internally. The `extract_call` and `extract_call_expr` functions (lines 484, 492) are not recursive. The `expr_to_json` function from bazel.rs is not duplicated here; this file uses `metadata_value_from_expr` (line 431) which is not recursive. **PASS** for this file's code; relies on starlark_syntax parser's internal limits.

### Iteration Count

No 100K iteration cap on:

- `top_level_statements` (line 135): Returns slice of all statements
- `parse_buck_build` (line 97): Iterates over all statements without cap
- `parse_metadata_bzl` (line 113): Iterates over statements
- `extract_named_kwarg_string_list` (line 513): No cap on list items

### String Length

No 10 MB truncation with warning on any field value. `expr_as_string` (line 523) and `extract_named_kwarg_string` (line 509) return string values without size limits.

## Principle 3: Archive Safety

**Status**: N/A

Buck parser does not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. Uses `std::fs::read_to_string(path)` at lines 91 and 108 with error handling that returns fallback data on failure (lines 53-58, 75-86). Returns error not panic, but doesn't use `fs::metadata()` as specified.

### UTF-8 Encoding

`std::fs::read_to_string` returns an error for non-UTF-8 files. The `starlark_syntax` parser works on `String` (UTF-8 validated). No explicit lossy conversion path — invalid UTF-8 causes the parser to return fallback data. No `String::from_utf8()` + warning + lossy conversion pattern.

### JSON/YAML Validity

Returns fallback/default `PackageData` on parse failure (lines 53-58, 75-86, 119-124). **PASS**.

### Required Fields

Missing `name` in BUCK build rules causes `extract_build_package_from_statement` to return `None` (line 459). Falls back to parent directory name (line 536). Missing name in METADATA.bzl results in partial `PackageData` with available fields. **PASS**.

### URL Format

URLs accepted as-is. **PASS** per ADR spec.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution performed.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code. All instances are in `#[cfg(test)]` blocks.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle           | Severity | Line(s)      | Description                                                                         |
| --- | ------------------- | -------- | ------------ | ----------------------------------------------------------------------------------- |
| 1   | P2: File Size       | Medium   | 91, 108      | No `fs::metadata().len()` check before reading; entire file loaded into memory      |
| 2   | P2: Iteration Count | Low      | 97, 113, 513 | No 100K iteration cap on statement processing or list extraction                    |
| 3   | P2: String Length   | Low      | 509, 523     | No 10 MB truncation with warning on string values                                   |
| 4   | P4: File Exists     | Low      | 91, 108      | Uses `fs::read_to_string` instead of `fs::metadata()` pre-check                     |
| 5   | P4: UTF-8 Encoding  | Low      | 91, 108      | No lossy UTF-8 conversion path; invalid UTF-8 causes parser to return fallback data |

## Remediation Priority

1. Add `fs::metadata().len()` check with 100 MB limit before reading files (lines 91, 108)
2. Add iteration count cap (100K) on statement/dependency processing
3. Add 10 MB string field truncation with warning
4. Add `fs::metadata()` pre-check before file read
5. Add lossy UTF-8 conversion with warning for encoding errors
