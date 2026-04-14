# ADR 0004 Security Audit: sbt

**File**: `src/parsers/sbt.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `subprocess`, `eval()`, or code execution. Uses a custom tokenizer (`tokenize()` at line 249) and pattern-matching parser. All parsing is static string/token analysis. No Scala or JVM code execution.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading. The entire file is read into memory at line 23 (`fs::read_to_string(path)`) without a size check.

### Recursion Depth

The `resolve_alias_value` function (line 428) is recursive and resolves string alias references. It has cycle detection via `visiting: &mut HashSet<String>` (line 399, 438) — inserting returns `false` if already present, which returns `None` and breaks the cycle. However, there is no explicit depth limit. A chain of 50+ alias references would recurse deeply. The `strip_outer_parens` function (line 830) uses a `loop` construct (not recursion). **Partial** — cycle detection present but no depth limit.

### Iteration Count

No 100K iteration cap on:

- `split_top_level_statements` (line 179): Iterates over all input characters
- `tokenize` (line 249): Iterates over all characters in each statement
- `process_statement_tokens` (line 521): Called for each statement without cap
- `parse_library_dependencies` (line 660): No cap on dependencies extracted
- `resolve_string_aliases` (line 387): Iterates over all statements

### String Length

No 10 MB truncation with warning on any field value. Token strings, identifier values, and string literals are stored without size limits.

## Principle 3: Archive Safety

**Status**: N/A

SBT parser does not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. Uses `fs::read_to_string(path)` at line 23 with error handling that returns `default_package_data()` on failure (line 27). Returns error not panic, but doesn't use `fs::metadata()` as specified.

### UTF-8 Encoding

`fs::read_to_string` returns an error for non-UTF-8 files (line 23-28). No explicit lossy conversion path — invalid UTF-8 causes the parser to return default data. No `String::from_utf8()` + warning + lossy conversion pattern.

### JSON/YAML Validity

Returns default `PackageData` on read failure (line 27). Not applicable for SBT format. **PASS**.

### Required Fields

Missing name/version are left as `None` and the parser continues. **PASS**.

### URL Format

URLs accepted as-is. **PASS** per ADR spec.

## Principle 5: Circular Dependency Detection

**Status**: PARTIAL

The `resolve_alias_value` function (line 428) implements circular reference detection for alias resolution:

- Uses `visiting: &mut HashSet<String>` to track currently-resolving names (line 399)
- Inserting an already-visiting name returns `None` (line 438), breaking the cycle
- Memoizes resolved values in `resolved: &mut HashMap<String, String>` (line 398)
- Removes name from `visiting` after resolution completes (line 449)

However, there is no explicit depth limit on the recursion. A linear chain of aliases (not circular, but deeply nested) could cause stack overflow.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code or test code within this file.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle           | Severity | Line(s)            | Description                                                                                         |
| --- | ------------------- | -------- | ------------------ | --------------------------------------------------------------------------------------------------- |
| 1   | P2: File Size       | Medium   | 23                 | No `fs::metadata().len()` check before reading; entire file loaded into memory                      |
| 2   | P2: Recursion Depth | Medium   | 428                | `resolve_alias_value` is recursive with cycle detection but no depth limit (ADR requires 50 levels) |
| 3   | P2: Iteration Count | Medium   | 179, 249, 521, 660 | No 100K iteration cap on statement parsing, tokenization, or dependency extraction                  |
| 4   | P2: String Length   | Low      | 278, 363           | No 10 MB truncation with warning on string token values                                             |
| 5   | P4: File Exists     | Low      | 23                 | Uses `fs::read_to_string` instead of `fs::metadata()` pre-check                                     |
| 6   | P4: UTF-8 Encoding  | Low      | 23                 | No lossy UTF-8 conversion path; invalid UTF-8 causes parser to return default data                  |

## Remediation Priority

1. Add `fs::metadata().len()` check with 100 MB limit before reading file (line 23)
2. Add depth limit (50 levels) to `resolve_alias_value` recursion (line 428)
3. Add iteration count cap (100K) on statement/dependency processing
4. Add 10 MB string field truncation with warning
5. Add `fs::metadata()` pre-check before file read
6. Add lossy UTF-8 conversion with warning for encoding errors

## Remediation

All 6 findings addressed. Added MAX_RECURSION_DEPTH=50 to resolve_alias_value, iteration caps on parsing/tokenization/statement loops, replaced fs::read_to_string with utils version, applied truncate_field to all string values.
