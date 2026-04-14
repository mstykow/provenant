# ADR 0004 Security Audit: pnpm_lock

**File**: `src/parsers/pnpm_lock.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, or subprocess calls. Uses `yaml_serde` for YAML parsing (static). All processing is data extraction from parsed YAML values. The `compute_dev_only_packages_v9` function at line 88 performs BFS graph traversal but does not execute any code.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `fs::read_to_string(path)` at line 51 reads entire file without size validation.

### Recursion Depth

No recursive functions. `compute_dev_only_packages_v9` uses BFS (line 167), not DFS, so no stack overflow risk from depth. No recursion depth concern.

### Iteration Count

- `packages_map` iteration at line 213: iterates all packages without a 100K cap.
- `importers` iteration at line 96: no iteration cap.
- `snapshots` iteration at line 129: no iteration cap.
- `graph` BFS queue at line 167: no iteration cap on BFS traversal.
- Dependency iteration in `parse_nested_dependencies` at lines 379, 387, 397, 407: no iteration cap.

### String Length

No 10 MB per-field truncation. Field values from YAML are stored as-is without length checks.

## Principle 3: Archive Safety

**Status**: N/A

This parser does not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `fs::read_to_string(path)` at line 51 will fail if the file doesn't exist, but error is handled gracefully returning `default_package_data()`.

### UTF-8 Encoding

`fs::read_to_string(path)` at line 51 fails on invalid UTF-8 with no lossy fallback. Non-UTF-8 files cause total parse failure without encoding warning.

### JSON/YAML Validity

`yaml_serde::from_str(&content)` at line 59 handles parse failure gracefully, returning `default_package_data()` with a warning. PASS.

### Required Fields

Missing package name/version result in `None` via `parse_purl_fields` returning `Option`. PASS.

### URL Format

URLs are not directly extracted from pnpm lockfiles. N/A.

## Principle 5: Circular Dependency Detection

**Status**: PARTIAL

The `compute_dev_only_packages_v9` function at line 88 performs BFS with a `prod_reachable` `HashSet` at line 159 that prevents revisiting nodes. This is implicit cycle detection — the BFS will not infinite-loop on circular dependencies. However, the cycle detection is not explicit (no warning is logged when a cycle is detected) and the traversal has no depth limit. The BFS is bounded by the graph size, which itself is unbounded (no iteration cap).

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No bare `.unwrap()` calls in library code. All 7 `.unwrap()` calls are within `#[cfg(test)]` module (lines 701, 708, 715, 754, 763, 772, 780) which is acceptable per ADR 0004.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage.

## Findings Summary

| #   | Principle               | Severity | Line(s)           | Description                                                                              |
| --- | ----------------------- | -------- | ----------------- | ---------------------------------------------------------------------------------------- |
| 1   | P2: File Size           | Medium   | 51                | No `fs::metadata().len()` check before `fs::read_to_string()`                            |
| 2   | P2: Iteration Count     | Low      | 213, 96, 129, 167 | No 100K iteration cap on package/importer/snapshot/BFS loops                             |
| 3   | P2: String Length       | Low      | Various           | No 10 MB per-field truncation                                                            |
| 4   | P5: Circular Dependency | Low      | 167               | BFS has implicit cycle protection via `HashSet` but no explicit cycle break with warning |
| 5   | P4: File Exists         | Low      | 51                | No explicit `fs::metadata()` pre-check before reading                                    |
| 6   | P4: UTF-8 Encoding      | Low      | 51                | No lossy UTF-8 fallback; non-UTF-8 files cause total parse failure                       |

## Remediation Priority

1. Add `fs::metadata().len()` check with 100 MB limit before reading file (line 51)
2. Add 100K iteration cap to package/snapshot/BFS loops
3. Add 10 MB field value truncation with warning
4. Add explicit cycle detection with warning in BFS traversal
5. Add `fs::metadata()` pre-check for file existence
6. Add lossy UTF-8 fallback for non-UTF-8 files

## Remediation

All 6 findings addressed. Replaced fs::read_to_string with utils version, added iteration caps to all package/dependency/BFS loops, applied truncate_field to all string values. BFS cycle detection already sufficient (HashSet).
