# ADR 0004 Security Audit: gradle

**File**: `src/parsers/gradle.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

Uses a custom token-based lexer (`lex()` function, line 174) and recursive descent parser. No Groovy engine, no `Command::new`, no `eval()`, no subprocess calls. The lexer is a hand-written ~120-line scanner that produces `Tok` tokens. This is exactly the approach ADR 0004 mandates: static analysis / token-based lexing rather than code execution.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading. The file is read entirely into memory at line 81 (`fs::read_to_string(path)`) without a size check. A large file would be fully loaded.

### Recursion Depth

No explicit recursion depth tracking in the parser. The `parse_block` function (line 373) iterates linearly with a `while` loop — not recursive. The `find_matching_paren`/`find_matching_bracket`/`find_matching_brace` functions (lines 796, 816, 1166) are iterative with depth counters. The `lex()` function (line 174) is iterative. No stack overflow risk from recursion. **PASS** for recursion depth.

### Iteration Count

No 100K iteration cap on:

- `lex()` function (line 174): Iterates over all input characters without limit. Token count is unbounded.
- `parse_block()` (line 373): No cap on number of dependencies extracted
- `extract_dependencies()` (line 355): No cap on total dependencies across blocks
- `parse_gradle_version_catalog()` (line 961): No cap on catalog entries

### String Length

No 10 MB truncation with warning on any field value. String tokens from the lexer are stored without size limits. Catalog entry values are not truncated.

## Principle 3: Archive Safety

**Status**: N/A

Gradle parser does not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. Uses `fs::read_to_string(path)` at line 81 with error handling that returns `default_package_data()` on failure (line 85). Returns error not panic, but doesn't use `fs::metadata()` as specified.

### UTF-8 Encoding

`fs::read_to_string` returns an error for non-UTF-8 files (line 81-86). No explicit lossy conversion path — invalid UTF-8 causes the parser to return default data. No `String::from_utf8()` + warning + lossy conversion pattern.

### JSON/YAML Validity

Returns default `PackageData` on read failure (line 85). The version catalog TOML parser (line 961) returns `None` on read failure. **PASS**.

### Required Fields

Missing name/version are handled as empty strings in `RawDep` and filtered out at line 361 (`if rd.name.is_empty() { continue }`). **PASS**.

### URL Format

URLs accepted as-is. **PASS** per ADR spec.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution performed. Dependencies are extracted from build file declarations.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code. All instances are in `#[cfg(test)]` blocks.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle           | Severity | Line(s)            | Description                                                                        |
| --- | ------------------- | -------- | ------------------ | ---------------------------------------------------------------------------------- |
| 1   | P2: File Size       | Medium   | 81                 | No `fs::metadata().len()` check before reading; entire file loaded into memory     |
| 2   | P2: Iteration Count | Medium   | 174, 373, 355, 961 | No 100K iteration cap on lexer, dependency extraction, or catalog parsing          |
| 3   | P2: String Length   | Low      | 210, 230, 282      | No 10 MB truncation with warning on string token values                            |
| 4   | P4: File Exists     | Low      | 81                 | Uses `fs::read_to_string` instead of `fs::metadata()` pre-check                    |
| 5   | P4: UTF-8 Encoding  | Low      | 81                 | No lossy UTF-8 conversion path; invalid UTF-8 causes parser to return default data |

## Remediation Priority

1. Add `fs::metadata().len()` check with 100 MB limit before reading file (line 81)
2. Add iteration count cap (100K) on lexer token production and dependency extraction
3. Add 10 MB string field truncation with warning on string token values
4. Add `fs::metadata()` pre-check before file read
5. Add lossy UTF-8 conversion with warning for non-UTF-8 files

## Remediation

| #   | Finding             | Fix                                                                                                                                                                 |
| --- | ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | P2: File Size       | Replaced `fs::read_to_string` with `read_file_to_string(path, None)` at both call sites (build file + version catalog)                                              |
| 2   | P2: Iteration Count | Added `MAX_ITERATION_COUNT` caps on lexer token production, `parse_block` iterations, dependency extraction, and catalog parsing                                    |
| 3   | P2: String Length   | Applied `truncate_field()` to all extracted string values including name, version, namespace, purl, extracted_requirement, license fields, and catalog entry values |
| 4   | P4: File Exists     | `read_file_to_string` uses `fs::metadata()` pre-check internally                                                                                                    |
| 5   | P4: UTF-8 Encoding  | `read_file_to_string` provides lossy UTF-8 fallback internally                                                                                                      |
