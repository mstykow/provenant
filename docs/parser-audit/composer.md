# ADR 0004 Security Audit: composer

**File**: `src/parsers/composer.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No code execution mechanisms. Uses `serde_json` for JSON parsing (line 239) and manual field extraction. Fully static analysis.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `read_json_file` at line 234 uses `File::open` then `file.read_to_string()` — no size pre-check.

### Recursion Depth

No recursive functions. All parsing is iterative.

### Iteration Count

Loops over dependencies (`deps.iter()` at line 253, `for package in packages` at line 320, `for author in authors` at line 621) have no 100K iteration cap. A composer.lock with >100K packages would process all entries.

### String Length

No field value truncation at 10MB. JSON string values are stored without length checks.

## Principle 3: Archive Safety

**Status**: N/A

No archive extraction performed.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `File::open(path)` at line 235 returns an error on failure, handled correctly. No explicit pre-check.

### UTF-8 Encoding

Uses `file.read_to_string()` at line 237 which fails on invalid UTF-8 without lossy fallback. No `String::from_utf8_lossy()` usage.

### JSON/YAML Validity

JSON parse failure at line 239 returns `Err`, handled by returning `default_package_data()` at lines 73-75 and 203-205. **PASS**

### Required Fields

Missing `name`/`version` handled via `Option<String>`. `full_name` being `None` results in `name: None` (line 135). Correct.

### URL Format

URLs accepted as-is. ADR-compliant.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code. Uses `unwrap_or_default()` (lines 293, 611) which are acceptable.

### Command::new / Subprocess Usage

**Status**: PASS

None.

## Findings Summary

| #   | Principle           | Severity | Line(s)       | Description                                                         |
| --- | ------------------- | -------- | ------------- | ------------------------------------------------------------------- |
| 1   | P2: File Size       | HIGH     | 234-238       | No `fs::metadata().len()` check before reading; unbounded file read |
| 2   | P2: Iteration Count | MEDIUM   | 253, 320, 621 | No 100K iteration cap on dependency/author loops                    |
| 3   | P2: String Length   | LOW      | various       | No 10MB field value truncation                                      |
| 4   | P4: UTF-8 Encoding  | MEDIUM   | 237           | No lossy UTF-8 fallback on read failure                             |

## Remediation Priority

1. Add `fs::metadata().len()` check before reading (100MB default limit)
2. Add lossy UTF-8 fallback on read failure
3. Add 100K iteration cap in dependency/author extraction loops
4. Add 10MB field value truncation with warning
