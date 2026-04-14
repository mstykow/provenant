# ADR 0004 Security Audit: license_normalization

**File**: `src/parsers/license_normalization.rs`
**Date**: 2026-04-14
**Status**: NON-COMPLIANT

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `exec()`, `eval()`, or subprocess calls. Uses `LicenseDetectionEngine` for static license expression parsing and normalization.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

N/A — This module does not read files directly. It operates on already-parsed strings.

### Recursion Depth

**CRITICAL**: `normalize_expression_ast` (line 426) is recursive with **NO depth tracking**. It recurses on `LicenseExpression::And`, `Or`, and `With` variants (lines 442-486), calling itself for both `left` and `right` sub-expressions. A deeply nested license expression like `((((A AND B) AND C) AND D)...))` with depth >50 would cause stack overflow.

`collect_boolean_chain` (line 581) is also recursive (lines 589-590) without depth tracking, though this is on the already-processed AST and follows the same tree structure.

### Iteration Count

- `combine_license_expressions` calls into external functions — no caps visible here
- `collect_reference_strings` (line 404): Iterates over JSON arrays — no 100K cap
- `collect_declared_license_reference_filenames` (line 387): Iterates over extra_data — no 100K cap

### String Length

No field-level truncation at 10MB. License expressions are used as-is.

## Principle 3: Archive Safety

**Status**: N/A

Not applicable — this module processes license strings, not archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

N/A — This module does not read files directly.

### UTF-8 Encoding

N/A — Operates on Rust `String` values which are already valid UTF-8.

### JSON/YAML Validity

N/A — Does not parse JSON/YAML directly. Uses `serde_json::Value` for extra_data access.

### Required Fields

Missing/empty license statements return `None` from `normalize_spdx_expression` (line 147-149) and `empty_declared_license_data` (line 76). — PASS

### URL Format

N/A — No URL handling.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution. License expression trees are data structures, not dependency graphs.

## Additional Checks

### .unwrap() in Library Code

**Status**: FAIL

- Line 508: `.unwrap_or_else(|| normalized_key.to_string())` — safe fallback
- Line 521: `.unwrap_or(canonical_spdx_key)` — safe fallback
- Line 535: `.unwrap_or_else(|| format!("LicenseRef-scancode-{}", ...))` — safe fallback
- No truly problematic `.unwrap()` calls in library code.

### Command::new / Subprocess Usage

**Status**: PASS

No subprocess calls found.

## Findings Summary

| #   | Principle    | Severity | Line(s) | Description                                                                                                          |
| --- | ------------ | -------- | ------- | -------------------------------------------------------------------------------------------------------------------- |
| 1   | P2 Recursion | HIGH     | 426-486 | `normalize_expression_ast` recursive without depth tracking — deeply nested license expressions cause stack overflow |
| 2   | P2 Recursion | MEDIUM   | 581-590 | `collect_boolean_chain` recursive without depth tracking                                                             |
| 3   | P2 Iteration | LOW      | 404     | No 100K cap on reference string iteration                                                                            |

## Remediation Priority

1. Add depth parameter (max 50) to `normalize_expression_ast`, return `None` on overflow
2. Add depth parameter to `collect_boolean_chain`, or convert to iterative approach
3. Add iteration cap (100K) on reference string collection
