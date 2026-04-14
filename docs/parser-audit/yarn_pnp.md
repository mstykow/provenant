# ADR 0004 Security Audit: yarn_pnp

**File**: `src/parsers/yarn_pnp.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, or subprocess calls. Uses `serde_json` for JSON parsing (static). The `extract_raw_runtime_state_json` function at line 149 performs manual bracket-matching to extract JSON from a JavaScript file, but does not execute any code — it's purely text scanning.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `fs::read_to_string(path)` at line 21 reads entire file without size validation. A `.pnp.cjs` file could be arbitrarily large.

### Recursion Depth

No recursive functions. No recursion depth concern.

### Iteration Count

- `content[json_start..].char_indices()` at line 160: iterates all characters in the JSON portion without cap. A very large JSON blob would iterate without limit.
- `registry_entries` iteration at line 66: iterates all registry entries without a 100K cap.
- `array.iter()` at line 120: no iteration cap on dependency pairs.
- `value.as_object().into_iter().flatten()` at line 131: no iteration cap.

### String Length

No 10 MB per-field truncation. Field values from JSON are stored as-is without length checks.

## Principle 3: Archive Safety

**Status**: N/A

This parser does not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `fs::read_to_string(path)` at line 21 will fail if the file doesn't exist, but error is handled gracefully returning `default_package_data()`.

### UTF-8 Encoding

`fs::read_to_string(path)` at line 21 fails on invalid UTF-8 with no lossy fallback. However, the `.pnp.cjs` file is JavaScript, so UTF-8 is expected. Non-UTF-8 files cause total parse failure without encoding warning.

### JSON/YAML Validity

`serde_json::from_str(json_text)` at line 51 handles parse failure gracefully via `?` operator propagating to the `Err` branch, which logs a warning and returns `default_package_data()`. PASS.

If `extract_raw_runtime_state_json` at line 149 fails to find the marker, it returns `None`, which is converted to an `Err` and handled. PASS.

### Required Fields

Missing package names/versions result in `None` or skipped entries (e.g., `split_locator` at line 141 returns `Option`). PASS.

### URL Format

URLs are not extracted from this parser. N/A.

## Principle 5: Circular Dependency Detection

**Status**: N/A

This parser does not resolve transitive dependency graphs.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No bare `.unwrap()` calls in library code.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage.

## Findings Summary

| #   | Principle           | Severity | Line(s)      | Description                                                        |
| --- | ------------------- | -------- | ------------ | ------------------------------------------------------------------ |
| 1   | P2: File Size       | Medium   | 21           | No `fs::metadata().len()` check before `fs::read_to_string()`      |
| 2   | P2: Iteration Count | Low      | 160, 66, 120 | No 100K iteration cap on character/entry/dependency loops          |
| 3   | P2: String Length   | Low      | Various      | No 10 MB per-field truncation                                      |
| 4   | P4: File Exists     | Low      | 21           | No explicit `fs::metadata()` pre-check before reading              |
| 5   | P4: UTF-8 Encoding  | Low      | 21           | No lossy UTF-8 fallback; non-UTF-8 files cause total parse failure |

## Remediation Priority

1. Add `fs::metadata().len()` check with 100 MB limit before reading file (line 21)
2. Add 100K iteration cap to character scanning loop (line 160) and registry entry loop (line 66)
3. Add 10 MB field value truncation with warning
4. Add `fs::metadata()` pre-check for file existence
5. Add lossy UTF-8 fallback for non-UTF-8 files
