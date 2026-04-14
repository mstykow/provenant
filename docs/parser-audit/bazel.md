# ADR 0004 Security Audit: bazel

**File**: `src/parsers/bazel.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

Uses `starlark_syntax::AstModule::parse` (line 325) for AST-based parsing. This is a compile-time parsing library that produces an AST without executing Starlark code. No `Command::new`, `subprocess`, `eval()`, or code execution. The `starlark_syntax` crate is a parser-only dependency — it does not evaluate Starlark expressions.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading. Both `parse_bazel_build` (line 64) and `parse_bazel_module` (line 250) read the entire file into memory via `std::fs::read_to_string(path)` without a size check.

### Recursion Depth

No explicit recursion depth tracking. The Starlark AST parser handles nesting internally. The `extract_call` and `extract_call_expr` functions (lines 335, 343) are not recursive. The `expr_to_json` function (line 456) recursively converts AST expressions but is bounded by the AST depth from the parser. The `starlark_syntax` parser may have its own depth limits. **Partial** — relies on external parser limits.

### Iteration Count

No 100K iteration cap on:

- `top_level_statements` (line 328): Returns slice of all statements
- `extract_packages` / `parse_bazel_build` (line 63-79): Iterates over all statements without cap
- `parse_bazel_module` (line 259): Iterates over all statements
- `extract_string_list_kwarg` (line 371): No cap on list items

### String Length

No 10 MB truncation with warning on any field value. `extract_string_kwarg` (line 367) and `expr_as_string` (line 431) return string values without size limits.

## Principle 3: Archive Safety

**Status**: N/A

Bazel parser does not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. Uses `std::fs::read_to_string(path)` at lines 64 and 250 with error handling that returns error/fallback on failure. Returns error not panic, but doesn't use `fs::metadata()` as specified.

### UTF-8 Encoding

`std::fs::read_to_string` returns an error for non-UTF-8 files. The `starlark_syntax` parser works on `String` (UTF-8 validated). No explicit lossy conversion path — invalid UTF-8 causes the parser to return error/fallback data. No `String::from_utf8()` + warning + lossy conversion pattern.

### JSON/YAML Validity

Returns fallback/default `PackageData` on parse failure (lines 53-58, 240-245). **PASS**.

### Required Fields

Missing `name` in Bazel BUILD rules causes `extract_package_from_statement` to return `None` (line 90). Missing `name` in MODULE.bazel causes `parse_bazel_module` to return default data (line 307-308). **PASS**.

### URL Format

N/A — no URL fields directly parsed from user input.

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

| #   | Principle           | Severity | Line(s)      | Description                                                                           |
| --- | ------------------- | -------- | ------------ | ------------------------------------------------------------------------------------- |
| 1   | P2: File Size       | Medium   | 64, 250      | No `fs::metadata().len()` check before reading; entire file loaded into memory        |
| 2   | P2: Iteration Count | Low      | 71, 259, 371 | No 100K iteration cap on statement processing or list extraction                      |
| 3   | P2: String Length   | Low      | 367, 431     | No 10 MB truncation with warning on string values                                     |
| 4   | P4: File Exists     | Low      | 64, 250      | Uses `fs::read_to_string` instead of `fs::metadata()` pre-check                       |
| 5   | P4: UTF-8 Encoding  | Low      | 64, 250      | No lossy UTF-8 conversion path; invalid UTF-8 causes parser to return fallback data   |
| 6   | P2: Recursion Depth | Low      | 456          | `expr_to_json` is recursive; relies on starlark_syntax parser's internal depth limits |

## Remediation Priority

1. Add `fs::metadata().len()` check with 100 MB limit before reading files (lines 64, 250)
2. Add iteration count cap (100K) on statement/dependency processing
3. Add 10 MB string field truncation with warning
4. Add `fs::metadata()` pre-check before file read
5. Add lossy UTF-8 conversion with warning for encoding errors
