# ADR 0004 Security Audit: yarn_lock

**File**: `src/parsers/yarn_lock.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, or subprocess calls. Uses `yaml_serde` for v2 YAML parsing and custom line-based parsing for v1 format (static). All processing is data extraction.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `fs::read_to_string(path)` at line 59 reads entire file without size validation. Additionally, `load_manifest_dependency_info` at line 505 reads a second file (`package.json`) without size validation.

### Recursion Depth

No recursive functions in this parser. `parse_yarn_v1_block` at line 374 and `parse_yaml_dependencies` at line 760 are iterative. No recursion depth concern.

### Iteration Count

- `content.split("\n\n")` at line 258: iterates all blocks without a 100K cap. A malicious yarn.lock with >100K blocks would iterate without limit.
- `yaml_map` iteration at line 107: iterates all YAML entries without cap.
- `lines` iteration at line 403: iterates lines within a block without cap.
- `mapping` iteration at line 766: iterates dependency entries without cap.
- `manifest_dependencies` keys at lines 587, 565: no iteration cap.

### String Length

No 10 MB per-field truncation. Field values from YAML/JSON are stored as-is without length checks.

## Principle 3: Archive Safety

**Status**: N/A

This parser does not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `fs::read_to_string(path)` at line 59 will fail if the file doesn't exist, but error is handled gracefully. However, `load_manifest_dependency_info` at line 505 also reads `package.json` from the parent directory without pre-check — this is silently handled with `let Ok(content) = ...` pattern.

### UTF-8 Encoding

`fs::read_to_string(path)` at line 59 fails on invalid UTF-8 with no lossy fallback. Non-UTF-8 files cause total parse failure without encoding warning.

### JSON/YAML Validity

- v2: `yaml_serde::from_str(content)` at line 91 handles parse failure gracefully, returning `default_package_data()` with a warning. PASS.
- v1: Custom text parsing handles malformed data gracefully by skipping invalid blocks. PASS.

### Required Fields

Missing `name` and `version` result in `None` values throughout. Package data fields default to `None`. PASS.

### URL Format

URLs (resolved URLs) are accepted as-is. PASS.

## Principle 5: Circular Dependency Detection

**Status**: N/A

This parser does not resolve transitive dependency graphs. It extracts flat dependency lists from lockfiles.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No bare `.unwrap()` calls in library code. All uses are `.unwrap_or()`, `.unwrap_or_default()`, or `.unwrap_or_else()`.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage.

## Findings Summary

| #   | Principle           | Severity | Line(s)            | Description                                                                                            |
| --- | ------------------- | -------- | ------------------ | ------------------------------------------------------------------------------------------------------ |
| 1   | P2: File Size       | Medium   | 59, 505            | No `fs::metadata().len()` check before `fs::read_to_string()` (both lockfile and sibling package.json) |
| 2   | P2: Iteration Count | Low      | 258, 107, 403, 766 | No 100K iteration cap on block/entry/dependency loops                                                  |
| 3   | P2: String Length   | Low      | Various            | No 10 MB per-field truncation                                                                          |
| 4   | P4: File Exists     | Low      | 59, 505            | No explicit `fs::metadata()` pre-check before reading                                                  |
| 5   | P4: UTF-8 Encoding  | Low      | 59                 | No lossy UTF-8 fallback; non-UTF-8 files cause total parse failure                                     |

## Remediation Priority

1. Add `fs::metadata().len()` check with 100 MB limit before reading files (lines 59, 505)
2. Add 100K iteration cap to block/entry/dependency loops
3. Add 10 MB field value truncation with warning
4. Add `fs::metadata()` pre-check for file existence
5. Add lossy UTF-8 fallback for non-UTF-8 files
