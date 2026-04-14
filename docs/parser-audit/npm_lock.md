# ADR 0004 Security Audit: npm_lock

**File**: `src/parsers/npm_lock.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, or subprocess calls. Uses `serde_json` for JSON parsing (static). All processing is data extraction from parsed JSON values.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `fs::read_to_string(path)` at line 69 reads entire file without size validation.

### Recursion Depth

`parse_dependencies_v1_with_depth` at line 365 is recursive and tracks `depth` parameter, but **no maximum depth limit is enforced**. The function recurses at line 397 with `depth + 1` but never checks if `depth > 50`. A deeply nested v1 lockfile could cause stack overflow.

### Iteration Count

- `packages` iteration at line 160: iterates all package entries without a 100K cap.
- `dependencies_obj` iteration at line 371: iterates all dependencies without cap.
- `root_deps_obj.keys()` at lines 143, 148: no iteration cap.

### String Length

No 10 MB per-field truncation. JSON field values are extracted as-is without length checks (e.g., `version.as_str()` at line 183, `resolved` at line 198).

## Principle 3: Archive Safety

**Status**: N/A

This parser does not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `fs::read_to_string(path)` at line 69 will fail if the file doesn't exist, but error is handled gracefully returning `default_package_data()`.

### UTF-8 Encoding

`fs::read_to_string(path)` at line 69 fails on invalid UTF-8 with no lossy fallback. Non-UTF-8 files cause total parse failure without warning about encoding.

### JSON/YAML Validity

`serde_json::from_str(&content)` at line 77 handles parse failure gracefully, returning `default_package_data()` with a warning. PASS.

### Required Fields

Missing `name` and `version` are handled via `.unwrap_or("")` (lines 93, 100) or `Option<String>` (lines 183). The `normalize_root_package_metadata` function (line 483) converts empty strings to `None`. PASS.

### URL Format

URLs (resolved URLs) are accepted as-is. PASS.

## Principle 5: Circular Dependency Detection

**Status**: N/A

This parser does not resolve transitive dependency graphs (v2+ is flat structure; v1 is nested but within the lockfile's own tree, not a resolution engine).

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No bare `.unwrap()` calls in library code. All uses are `.unwrap_or()`, `.unwrap_or_default()`, or `.unwrap_or_else()`.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage.

## Findings Summary

| #   | Principle           | Severity | Line(s)  | Description                                                                            |
| --- | ------------------- | -------- | -------- | -------------------------------------------------------------------------------------- |
| 1   | P2: Recursion Depth | High     | 365, 397 | Recursive `parse_dependencies_v1_with_depth` has no depth limit (50-level cap missing) |
| 2   | P2: File Size       | Medium   | 69       | No `fs::metadata().len()` check before `fs::read_to_string()`                          |
| 3   | P2: Iteration Count | Low      | 160, 371 | No 100K iteration cap on package/dependency loops                                      |
| 4   | P2: String Length   | Low      | Various  | No 10 MB per-field truncation                                                          |
| 5   | P4: File Exists     | Low      | 69       | No explicit `fs::metadata()` pre-check before reading                                  |
| 6   | P4: UTF-8 Encoding  | Low      | 69       | No lossy UTF-8 fallback; non-UTF-8 files cause total parse failure                     |

## Remediation Priority

1. Add 50-level depth cap to `parse_dependencies_v1_with_depth` at line 365 — break early with warning when `depth >= 50`
2. Add `fs::metadata().len()` check with 100 MB limit before reading file (line 69)
3. Add 100K iteration cap to package and dependency loops
4. Add 10 MB field value truncation with warning
5. Add `fs::metadata()` pre-check for file existence
6. Add lossy UTF-8 fallback for non-UTF-8 files

## Remediation

All 6 findings addressed. Added MAX_RECURSION_DEPTH=50 to parse_dependencies_v1_with_depth, replaced fs::read_to_string with utils version, added iteration caps to all package/dependency loops, applied truncate_field to all string values.
