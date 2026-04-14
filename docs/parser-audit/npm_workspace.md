# ADR 0004 Security Audit: npm_workspace

**File**: `src/parsers/npm_workspace.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, or subprocess calls. Uses `yaml_serde` for YAML parsing (static). All processing is data extraction from parsed YAML values.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `fs::read_to_string(path)` at line 43 reads entire file without size validation.

### Recursion Depth

No recursive functions. No recursion depth concern.

### Iteration Count

- `workspace_patterns` iteration at line 79: iterates all workspace patterns without a 100K cap. A YAML file with >100K workspace entries would iterate without limit.

### String Length

No 10 MB per-field truncation. Workspace pattern strings are stored as-is without length checks.

## Principle 3: Archive Safety

**Status**: N/A

This parser does not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `fs::read_to_string(path)` at line 43 will fail if the file doesn't exist, but error is handled gracefully returning `default_package_data()`.

### UTF-8 Encoding

`fs::read_to_string(path)` at line 43 fails on invalid UTF-8 with no lossy fallback. Non-UTF-8 files cause total parse failure without encoding warning.

### JSON/YAML Validity

`yaml_serde::from_str(&content)` at line 51 handles parse failure gracefully, returning `default_package_data()` with a warning. PASS.

### Required Fields

The parser does not extract `name` or `version` fields from workspace YAML — it only extracts workspace patterns. Missing fields result in `None` or empty data. PASS.

### URL Format

No URLs are extracted from workspace files. N/A.

## Principle 5: Circular Dependency Detection

**Status**: N/A

This parser does not resolve transitive dependencies.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code. All `.unwrap()` usage is in `#[cfg(test)]` block (lines 148, 153, 155, 158, 172, 175, 176, etc.) which is acceptable per ADR 0004.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage.

## Findings Summary

| #   | Principle           | Severity | Line(s) | Description                                                        |
| --- | ------------------- | -------- | ------- | ------------------------------------------------------------------ |
| 1   | P2: File Size       | Medium   | 43      | No `fs::metadata().len()` check before `fs::read_to_string()`      |
| 2   | P2: Iteration Count | Low      | 79      | No 100K iteration cap on workspace patterns                        |
| 3   | P2: String Length   | Low      | 79      | No 10 MB per-field truncation                                      |
| 4   | P4: File Exists     | Low      | 43      | No explicit `fs::metadata()` pre-check before reading              |
| 5   | P4: UTF-8 Encoding  | Low      | 43      | No lossy UTF-8 fallback; non-UTF-8 files cause total parse failure |

## Remediation Priority

1. Add `fs::metadata().len()` check with 100 MB limit before reading file (line 43)
2. Add 100K iteration cap to workspace patterns loop (line 79)
3. Add 10 MB field value truncation with warning
4. Add `fs::metadata()` pre-check for file existence
5. Add lossy UTF-8 fallback for non-UTF-8 files
