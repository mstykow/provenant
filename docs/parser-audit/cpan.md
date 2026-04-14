# ADR 0004 Security Audit: cpan

**File**: `src/parsers/cpan.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No code execution mechanisms. Uses `serde_json::from_str()` for JSON parsing (line 248), `yaml_serde::from_str()` for YAML parsing (line 257), and `fs::read_to_string()` for MANIFEST reading (line 199). Fully static analysis.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `fs::read_to_string(path)` at lines 199, 246, 255 — no size pre-check.

### Recursion Depth

No recursive functions. All parsing is iterative.

### Iteration Count

- MANIFEST parser (line 207): `content.lines()` with no cap on line count
- Dependency extraction (lines 543-583, 585-635): iterates over JSON/YAML prereq objects with no cap
- Party extraction (lines 399-422, 424-447): iterates over author arrays with no cap
- No 100K iteration cap anywhere

### String Length

No field value truncation at 10MB. String values from JSON/YAML fields stored without length checks.

## Principle 3: Archive Safety

**Status**: N/A

No archive extraction performed.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `fs::read_to_string(path)` returns an error on failure, handled correctly with `warn!()` and default return at lines 70-73, 137-139, 199-205.

### UTF-8 Encoding

Uses `fs::read_to_string()` which fails on invalid UTF-8 without lossy fallback.

### JSON/YAML Validity

- JSON parse failure at line 248 returns `Err`, handled at lines 70-73. **PASS**
- YAML parse failure at line 257 returns `Err`, handled at lines 137-139. **PASS**
- Non-object root at lines 249-251, 258-260 returns `Err`. **PASS**

### Required Fields

Missing `name`/`version` handled via `Option<String>`. Correct.

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

| #   | Principle           | Severity | Line(s)               | Description                                                         |
| --- | ------------------- | -------- | --------------------- | ------------------------------------------------------------------- |
| 1   | P2: File Size       | HIGH     | 199, 246, 255         | No `fs::metadata().len()` check before reading; unbounded file read |
| 2   | P2: Iteration Count | MEDIUM   | 207, 543-583, 585-635 | No 100K iteration cap on line/dependency/author loops               |
| 3   | P2: String Length   | LOW      | various               | No 10MB field value truncation                                      |
| 4   | P4: UTF-8 Encoding  | MEDIUM   | 199, 246, 255         | No lossy UTF-8 fallback on read failure                             |

## Remediation Priority

1. Add `fs::metadata().len()` check before reading (100MB default limit)
2. Add 100K iteration cap in line/dependency/author processing loops
3. Add lossy UTF-8 fallback on read failure
4. Add 10MB field value truncation with warning
