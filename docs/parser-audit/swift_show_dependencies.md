# ADR 0004 Security Audit: swift_show_dependencies

**File**: `src/parsers/swift_show_dependencies.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `exec()`, `eval()`, or subprocess calls. Uses `serde_json` for static JSON deserialization (line 83).

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `fs::read_to_string` called at line 67 without size pre-check.

### Recursion Depth

**CRITICAL**: `flatten_dependency` (line 118) is recursive with **NO depth tracking or cycle detection**. It recursively traverses `dep.dependencies` children (line 123-124). A deeply nested or cyclic dependency structure (though unlikely from JSON, malformed input could cause deep nesting) will cause stack overflow.

Additionally, `build_dependency` (line 128) recursively calls itself on line 135 to build `nested_dependencies` for each child — also **without depth or cycle tracking**.

### Iteration Count

- `flatten_dependencies` (line 109): No 100K cap on total flattened dependencies
- The recursive traversal can produce unbounded output

### String Length

No field-level truncation at 10MB.

## Principle 3: Archive Safety

**Status**: N/A

JSON files are not archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `fs::read_to_string` at line 67 fails on missing files, handled via `match`. — Acceptable.

### UTF-8 Encoding

`fs::read_to_string` will fail on non-UTF-8. No lossy conversion fallback. — Minor gap.

### JSON/YAML Validity

`serde_json::from_str` error at line 83 is handled, returns `default_package_data()`. — PASS

### Required Fields

Missing name/version result in `None`. — PASS

### URL Format

URLs accepted as-is via `parse_url_namespace_and_name` (line 236). — Per ADR, acceptable.

## Principle 5: Circular Dependency Detection

**Status**: FAIL

`flatten_dependency` (line 118) does not track visited nodes. While JSON deserialization typically prevents true cycles, the recursive function has no protection against deep nesting. No visited-state tracking exists.

`build_dependency` (line 128) also recursively processes nested dependencies (line 132-136) without cycle detection.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

- Line 161: `.unwrap_or_default()` — safe
- No problematic `.unwrap()` calls in library code.

### Command::new / Subprocess Usage

**Status**: PASS

No subprocess calls found.

## Findings Summary

| #   | Principle          | Severity | Line(s)  | Description                                                                |
| --- | ------------------ | -------- | -------- | -------------------------------------------------------------------------- |
| 1   | P2 Recursion       | HIGH     | 118-125  | `flatten_dependency` recursive with no depth tracking or cycle detection   |
| 2   | P2 Recursion       | HIGH     | 128-136  | `build_dependency` recursive on nested_dependencies with no depth tracking |
| 3   | P2 File Size       | MEDIUM   | 67       | No file size check before reading                                          |
| 4   | P2 Iteration       | MEDIUM   | 109      | No 100K cap on total flattened dependencies                                |
| 5   | P2 String Length   | LOW      | —        | No field-level 10MB truncation                                             |
| 6   | P4 UTF-8           | LOW      | 67       | No lossy UTF-8 fallback                                                    |
| 7   | P5 Cycle Detection | HIGH     | 118, 128 | No circular dependency detection in recursive traversal                    |

## Remediation Priority

1. Add depth parameter (max 50) to `flatten_dependency` and `build_dependency`, break on overflow
2. Add visited-set tracking for cycle detection in dependency tree
3. Add `fs::metadata().len()` check before reading, reject >100MB
4. Add iteration cap (100K) on total flattened dependencies

## Remediation

All 7 findings addressed. Added MAX_RECURSION_DEPTH=50 to flatten_dependency and build_dependency, iteration cap via MAX_ITERATION_COUNT, cycle detection via visited HashSet, replaced fs::read_to_string with utils version, applied truncate_field to all string fields.
