# ADR 0004 Security Audit: clojure

**File**: `src/parsers/clojure.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `subprocess`, `eval()`, or code execution. Uses a custom EDN reader (`Reader` struct, line 115) that parses Clojure data structures as static data. Notably, the `parse_dispatch_form` method (line 181) explicitly rejects `#=` reader eval (line 189: returns `Err("unsupported reader eval dispatch")`). Reader conditionals, tagged literals, and function literals are tolerated but parsed as inert data without evaluation. This is a textbook ADR 0004-compliant approach.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading. Both parsers (`ClojureDepsEdnParser` line 23, `ClojureProjectCljParser` line 59) read the entire file into memory via `fs::read_to_string(path)` without a size check.

### Recursion Depth

The `Reader::parse_form` method (line 157) is recursive:

- `parse_collection` calls `parse_form` for each element (line 277)
- `parse_map` calls `parse_form` for keys and values (lines 293, 298)
- `parse_dispatch_form` calls `parse_form` (lines 167, 172, 187, 201, 209-210, 215-216)
- Prefixed forms call `parse_form` (line 172)

**No depth tracking or limit exists.** A deeply nested EDN structure (e.g., 1000 levels of nested vectors) would cause stack overflow. The ADR requires 50 levels with tracking in parser state.

### Iteration Count

No 100K iteration cap on:

- `Reader::parse_all` (line 128): Iterates over all top-level forms
- `parse_collection` (line 265): No cap on collection elements
- `parse_map` (line 281): No cap on map entries
- `extract_deps_map` (line 478): No cap on dependency entries
- `extract_project_dependencies` (line 537): No cap on project dependency entries

### String Length

No 10 MB truncation with warning on any field value. The `parse_string` method (line 223) builds strings without size limits. The `parse_atom` method (line 303) builds symbol strings without size limits.

## Principle 3: Archive Safety

**Status**: N/A

Clojure parser does not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. Both parsers use `fs::read_to_string(path)` (lines 23, 59) with error handling that returns `default_package_data()` on failure (lines 27, 63). Returns error not panic, but doesn't use `fs::metadata()` as specified.

### UTF-8 Encoding

`fs::read_to_string` returns an error for non-UTF-8 files. The `Reader` works on `&str` (UTF-8 validated by Rust). No explicit lossy conversion path — invalid UTF-8 causes the parser to return default data. No `String::from_utf8()` + warning + lossy conversion pattern.

### JSON/YAML/EDN Validity

Both parsers return `default_package_data()` on parse failure (lines 42-43, 77-79). The `parse_forms` function returns `Result<Vec<Form>, String>`, and parse errors are caught and logged. **PASS**.

### Required Fields

Missing name/version in `deps.edn` are left as `None` — deps.edn format doesn't have a top-level name/version. For `project.clj`, missing project identifier or version returns an error (lines 410-415) and falls back to default. **PASS**.

### URL Format

URLs accepted as-is. **PASS** per ADR spec.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution performed. Dependencies are extracted from EDN map declarations.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in any code (library or test).

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle           | Severity | Line(s)                 | Description                                                                                                      |
| --- | ------------------- | -------- | ----------------------- | ---------------------------------------------------------------------------------------------------------------- |
| 1   | P2: Recursion Depth | High     | 157, 265, 281           | Recursive EDN parsing with no depth tracking; deeply nested input causes stack overflow. ADR requires 50 levels. |
| 2   | P2: File Size       | Medium   | 23, 59                  | No `fs::metadata().len()` check before reading; entire file loaded into memory                                   |
| 3   | P2: Iteration Count | Medium   | 128, 265, 281, 478, 537 | No 100K iteration cap on form parsing, collection/map entries, or dependency extraction                          |
| 4   | P2: String Length   | Low      | 223, 303                | No 10 MB truncation with warning on parsed string/atom values                                                    |
| 5   | P4: File Exists     | Low      | 23, 59                  | Uses `fs::read_to_string` instead of `fs::metadata()` pre-check                                                  |
| 6   | P4: UTF-8 Encoding  | Low      | 23, 59                  | No lossy UTF-8 conversion path; invalid UTF-8 causes parser to return default data                               |

## Remediation Priority

1. Add depth tracking to `Reader::parse_form` with 50-level limit (line 157) — **highest priority**, stack overflow risk
2. Add `fs::metadata().len()` check with 100 MB limit before reading files (lines 23, 59)
3. Add iteration count cap (100K) on collection/map parsing and dependency extraction
4. Add 10 MB string field truncation with warning
5. Add `fs::metadata()` pre-check before file read
6. Add lossy UTF-8 conversion with warning for encoding errors
