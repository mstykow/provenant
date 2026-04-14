# ADR 0004 Security Audit: mod

**File**: `src/parsers/mod.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `exec()`, `eval()`, or subprocess calls. The module defines parser infrastructure, trait, and dispatch macros.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No direct file reading. `capture_parser_diagnostics` (line 346) wraps parser `extract` closures — file reading happens in the individual parsers, not here. — N/A

### Recursion Depth

No recursive functions. — PASS

### Iteration Count

- `packages.into_iter().map(...)` (line 364-369): No cap on number of packages returned by a parser
- `register_package_handlers!` macro generates iteration over all registered parsers (line 599-618) — this is a fixed, compile-time list

### String Length

No string processing beyond diagnostic messages. — PASS

## Principle 3: Archive Safety

**Status**: N/A

No file operations.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No file existence checks in this module. Delegated to individual parsers. — N/A

### UTF-8 Encoding

N/A — No file reading.

### JSON/YAML Validity

N/A — No parsing.

### Required Fields

`finalize_package_declared_license_references` (line 284) is called on every extracted package to ensure license references are properly attached. This is post-processing, not validation. — PASS

### URL Format

N/A — No URL handling.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: FAIL

- Line 360: `stack.borrow_mut().pop().unwrap_or_default()` — safe, uses `unwrap_or_default()`
- Line 483: `.unwrap_or_default()` — safe, used for `extract_first_package` fallback
- No truly problematic `.unwrap()` calls.

### Command::new / Subprocess Usage

**Status**: PASS

No subprocess calls found.

## Findings Summary

| #   | Principle    | Severity | Line(s) | Description                                                                         |
| --- | ------------ | -------- | ------- | ----------------------------------------------------------------------------------- |
| 1   | P2 Iteration | LOW      | 364     | No cap on number of packages returned by a parser (delegated to individual parsers) |

No significant findings. This module is infrastructure code that delegates to individual parsers.

## Remediation Priority

None required. Individual parsers are responsible for their own DoS protections.
