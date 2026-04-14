# ADR 0004 Security Audit: dart

**File**: `src/parsers/dart.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

No code execution mechanisms. Uses `yaml_serde::from_str()` for YAML parsing (line 119) and manual field extraction. Fully static analysis.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `read_yaml_file` at line 117 uses `fs::read_to_string(path)` — no size pre-check.

### Recursion Depth

`format_dependency_mapping` at line 511 has a recursive call at line 526 (`format_dependency_mapping(nested)?`). No depth tracking — a deeply nested YAML mapping could cause deep recursion. However, in practice YAML parsers limit nesting.

### Iteration Count

Loops over dependencies (`dep_map` at lines 454-482, lock packages at lines 301-349, SDKs at lines 262-277) have no 100K iteration cap. The `reachable_lock_packages` function (line 772) uses a BFS queue with no iteration cap.

### String Length

No field value truncation at 10MB. String values from YAML fields stored without length checks.

## Principle 3: Archive Safety

**Status**: N/A

No archive extraction performed.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `fs::read_to_string(path)` at line 118 returns an error, handled correctly at lines 73-81, 98-106.

### UTF-8 Encoding

Uses `fs::read_to_string()` which fails on invalid UTF-8 without lossy fallback.

### JSON/YAML Validity

YAML parse failure at line 119 returns `Err`, handled by returning `default_package_data()` at lines 75-81, 99-106. **PASS**

### Required Fields

Missing `name`/`version` handled via `Option<String>`. Correct.

### URL Format

URLs accepted as-is. ADR-compliant.

## Principle 5: Circular Dependency Detection

**Status**: PASS

`reachable_lock_packages` at line 772 uses a `HashSet<String>` (`reachable`) to track visited nodes. At line 790, `reachable.insert(current.clone())` returns `false` for already-visited nodes, and the `if !reachable.insert(...)` check at line 790 causes the loop to `continue`, breaking cycles. **PASS**

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code.

### Command::new / Subprocess Usage

**Status**: PASS

None.

## Findings Summary

| #   | Principle           | Severity | Line(s)                   | Description                                                                  |
| --- | ------------------- | -------- | ------------------------- | ---------------------------------------------------------------------------- |
| 1   | P2: File Size       | HIGH     | 118                       | No `fs::metadata().len()` check before reading; unbounded file read          |
| 2   | P2: Recursion Depth | MEDIUM   | 526                       | `format_dependency_mapping` has unbounded recursion for nested YAML mappings |
| 3   | P2: Iteration Count | MEDIUM   | 301-349, 454-482, 772-808 | No 100K iteration cap on dependency/BFS loops                                |
| 4   | P2: String Length   | LOW      | various                   | No 10MB field value truncation                                               |
| 5   | P4: UTF-8 Encoding  | MEDIUM   | 118                       | No lossy UTF-8 fallback on read failure                                      |

## Remediation Priority

1. Add `fs::metadata().len()` check before reading (100MB default limit)
2. Add depth tracking to `format_dependency_mapping` recursion (50-level max)
3. Add 100K iteration cap in dependency extraction and BFS traversal loops
4. Add lossy UTF-8 fallback on read failure
5. Add 10MB field value truncation with warning

## Remediation

- Finding #1 (P2 File Size): Replaced `fs::read_to_string` with `read_file_to_string(path, None)` — provides 100MB size check, file-exists check, and lossy UTF-8 fallback
- Finding #2 (P2 Recursion Depth): Added `depth: usize` parameter and `MAX_RECURSION_DEPTH = 50` to `format_dependency_mapping`, with `warn!` on overflow
- Finding #3 (P2 Iteration Count): Added `MAX_ITERATION_COUNT` caps to lock packages, SDKs, dependency maps, BFS queue, format_dependency_mapping, and authors loops
- Finding #4 (P2 String Length): Applied `truncate_field()` to all extracted string values across all parsers
- Finding #5 (P4 UTF-8): Fixed automatically by switching to `read_file_to_string`
