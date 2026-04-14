# ADR 0004 Security Audit: gradle_module

**File**: `src/parsers/gradle_module.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `subprocess`, `eval()`, or code execution. Uses `serde_json::from_reader` for JSON parsing. All parsing is static JSON deserialization.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading. Opens file via `File::open` at line 76 (`extract_packages`) and line 64 (`is_match`) without size pre-check. `serde_json::from_reader` with `BufReader` provides streaming deserialization, but no limit on total JSON size.

### Recursion Depth

No recursive parsing functions. All data extraction is iterative over JSON arrays/objects. `serde_json` handles deeply nested JSON internally, but there's no explicit depth cap in this parser's code. **Partial** — relies on serde_json's internal limits which may not enforce 50 levels.

### Iteration Count

No 100K iteration cap on:

- Variant processing loop (line 240): `variants.into_iter().flatten().filter_map(Value::as_object)`
- Dependency extraction loop (line 319): iterates over all dependencies in each variant
- File reference extraction loop (line 287): iterates over all files in each variant
- No cap on `seen_dependencies` HashMap or `file_references` Vec

### String Length

No 10 MB truncation with warning on any field value. JSON string values from component fields (lines 121-132) and dependency fields are stored without size limits.

## Principle 3: Archive Safety

**Status**: N/A

Gradle module parser does not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. Uses `File::open` at lines 64 and 76. `is_match` silently returns `false` on open failure (line 64-65). `extract_packages` returns `default_package_data()` on open failure (line 79-80). Returns error not panic, but doesn't use `fs::metadata()` as specified.

### UTF-8 Encoding

JSON parsing via `serde_json::from_reader` handles UTF-8 validation internally. No explicit lossy conversion path — invalid JSON causes parser to return default data (line 87-89). No warning about encoding issues specifically.

### JSON/YAML Validity

Returns default `PackageData` on JSON parse failure (lines 84-89). Checks `is_gradle_module_json` for structural validity (lines 92-94). **PASS**.

### Required Fields

Missing component fields (group, module, version) are handled as `None` (lines 121-132). The `is_gradle_module_json` check requires group, module, and version for `is_match` (lines 108-111) but `extract_packages` handles their absence gracefully. **PASS**.

### URL Format

N/A — no URL fields directly parsed from user input.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution performed.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code. All instances are in test files (`gradle_module_test.rs`).

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle           | Severity | Line(s)       | Description                                                                |
| --- | ------------------- | -------- | ------------- | -------------------------------------------------------------------------- |
| 1   | P2: File Size       | Medium   | 64, 76        | No `fs::metadata().len()` check before reading; 100 MB limit not enforced  |
| 2   | P2: Iteration Count | Medium   | 240, 287, 319 | No 100K iteration cap on variant, file reference, or dependency processing |
| 3   | P2: String Length   | Low      | 121-132       | No 10 MB truncation with warning on JSON string field values               |
| 4   | P4: File Exists     | Low      | 64, 76        | Uses `File::open` instead of `fs::metadata()` pre-check                    |
| 5   | P2: Recursion Depth | Low      | N/A           | No explicit JSON nesting depth cap; relies on serde_json internal limits   |

## Remediation Priority

1. Add `fs::metadata().len()` check with 100 MB limit before reading files (lines 64, 76)
2. Add iteration count cap (100K) on variant/dependency/file processing loops
3. Add 10 MB string field truncation with warning
4. Add `fs::metadata()` pre-check before `File::open`
5. Consider adding explicit JSON nesting depth tracking beyond serde_json's defaults

## Remediation

- #1 P2: File Size — Replaced `File::open`+`serde_json::from_reader` with `read_file_to_string`+`serde_json::from_str`
- #2 P2: Iteration Count — Added `.take(MAX_ITERATION_COUNT)` on variant/file/dependency processing
- #3 P2: String Length — Applied `truncate_field()` to all JSON string field values
- #4 P4: File Exists — Fixed by `read_file_to_string`
- #5 P2: Recursion Depth — Acceptable; serde_json handles internally, parser has no recursive functions
