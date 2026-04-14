# ADR 0004 Security Audit: conda_meta_json

**File**: `src/parsers/conda_meta_json.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No code execution mechanisms. Uses `serde_json::from_str()` for JSON parsing (line 96) and `fs::read_to_string()` for file reading. Fully static analysis.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `fs::read_to_string(path)` at line 79 — no size pre-check.

### Recursion Depth

No recursive functions. All parsing is iterative.

### Iteration Count

- `build_conda_file_references` (lines 213-224): `for file in files` iterates over the `files` array from JSON with no cap. A JSON with >100K file entries would process all of them.
- No 100K iteration cap anywhere.

### String Length

No field value truncation at 10MB. JSON string values (name, version, license, url, etc.) stored without length checks.

## Principle 3: Archive Safety

**Status**: N/A

No archive extraction performed.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `fs::read_to_string(path)` at line 79 returns an error on failure, handled correctly with `warn!()` and default return at lines 80-84.

### UTF-8 Encoding

Uses `fs::read_to_string()` which fails on invalid UTF-8 without lossy fallback.

### JSON/YAML Validity

JSON parse failure at line 96-101 returns `default_package_data()`. **PASS**

### Required Fields

Missing `name`/`version` handled via `Option<String>` in the `CondaMetaJson` struct. Correct.

### URL Format

URLs accepted as-is. ADR-compliant.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code.

### Command::new / Subprocess Usage

**Status**: PASS

None.

## Findings Summary

| #   | Principle           | Severity | Line(s) | Description                                                         |
| --- | ------------------- | -------- | ------- | ------------------------------------------------------------------- |
| 1   | P2: File Size       | HIGH     | 79      | No `fs::metadata().len()` check before reading; unbounded file read |
| 2   | P2: Iteration Count | MEDIUM   | 213-224 | No 100K iteration cap on file references loop                       |
| 3   | P2: String Length   | LOW      | various | No 10MB field value truncation                                      |
| 4   | P4: UTF-8 Encoding  | MEDIUM   | 79      | No lossy UTF-8 fallback on read failure                             |

## Remediation Priority

1. Add `fs::metadata().len()` check before reading (100MB default limit)
2. Add 100K iteration cap in file references loop
3. Add lossy UTF-8 fallback on read failure
4. Add 10MB field value truncation with warning
