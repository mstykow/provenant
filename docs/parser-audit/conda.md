# ADR 0004 Security Audit: conda

**File**: `src/parsers/conda.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No code execution mechanisms. Uses `yaml_serde::from_str()` for YAML parsing, `regex::Regex` for pattern matching, and `pep508_rs::Requirement` for dependency parsing. The Jinja2 handling at lines 398-458 is strictly text substitution — it finds `{% set %}` patterns and replaces `{{ variable }}` references with string values. No Jinja2 engine or template evaluation is performed. Fully static analysis.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `fs::read_to_string(path)` at lines 149 and 323 — no size pre-check.

### Recursion Depth

No recursive functions. All parsing is iterative.

### Iteration Count

Loops over requirements (`for req in reqs` at line 228), dependencies (`for dep_value in dependencies` at line 562), Jinja2 variable processing (`for line in content.lines()` at line 401), and YAML dependency iteration have no 100K iteration cap.

### String Length

No field value truncation at 10MB. String values from YAML fields stored without length checks.

## Principle 3: Archive Safety

**Status**: N/A

No archive extraction performed.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `fs::read_to_string(path)` at lines 149, 323 returns an error on failure, handled correctly with `warn!()` and default return.

### UTF-8 Encoding

Uses `fs::read_to_string()` which fails on invalid UTF-8 without lossy fallback.

### JSON/YAML Validity

YAML parse failure at lines 162-167, 331-336 returns `default_package_data()`. **PASS**

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

No `.unwrap()` calls in library code. Uses `unwrap_or()` and `unwrap_or_default()` where needed.

### Command::new / Subprocess Usage

**Status**: PASS

None.

## Findings Summary

| #   | Principle           | Severity | Line(s)       | Description                                                         |
| --- | ------------------- | -------- | ------------- | ------------------------------------------------------------------- |
| 1   | P2: File Size       | HIGH     | 149, 323      | No `fs::metadata().len()` check before reading; unbounded file read |
| 2   | P2: Iteration Count | MEDIUM   | 228, 401, 562 | No 100K iteration cap on requirement/line/dependency loops          |
| 3   | P2: String Length   | LOW      | various       | No 10MB field value truncation                                      |
| 4   | P4: UTF-8 Encoding  | MEDIUM   | 149, 323      | No lossy UTF-8 fallback on read failure                             |

## Remediation Priority

1. Add `fs::metadata().len()` check before reading (100MB default limit)
2. Add 100K iteration cap in requirement/line/dependency processing loops
3. Add lossy UTF-8 fallback on read failure
4. Add 10MB field value truncation with warning
