# ADR 0004 Security Audit: pip_inspect_deplock

**File**: `src/parsers/pip_inspect_deplock.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

- `extract_packages` delegates to `PythonParser::extract_first_package` at line 89 — no code execution
- Test-only `parse_pip_inspect_deplock` uses `serde_json::from_str` — static deserialization
- No `Command::new`, `subprocess`, `eval()`, `exec()` anywhere

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

- No `fs::metadata().len()` check before reading
- `PythonParser::extract_first_package` handles reading internally — delegates to python.rs which also lacks pre-check for this format

### Recursion Depth

- No recursive functions
- PASS

### Iteration Count

- Test-only `parse_pip_inspect_deplock` iterates `installed_packages` without cap at line 108
- **GAP**: No 100,000 item cap (though actual parsing is delegated to PythonParser)

### String Length

- No 10MB per-field truncation for parsed metadata values

## Principle 3: Archive Safety

**Status**: N/A

Not an archive parser.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

- No `fs::metadata()` pre-check — file reading handled by PythonParser

### UTF-8 Encoding

- No explicit UTF-8 handling in this module — delegated to PythonParser
- Test-only function has no lossy fallback

### JSON/YAML Validity

- Test-only `parse_pip_inspect_deplock` returns `default_package_data` on JSON parse failure at lines 95-99
- Actual production path delegates to PythonParser which handles errors

### Required Fields

- Missing name/version handled with `None` — continues parsing
- `default_package_data` returned when no main package found at line 121

### URL Format

- URLs accepted as-is — compliant with ADR 0004

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution with circular dependency risk.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

- Line 151: `.unwrap_or_default()` — safe
- Line 158: `.unwrap_or_default()` — safe
- No dangerous `.unwrap()` calls in library code (all `.unwrap_or_*` variants)
- Test code uses `.unwrap()` which is acceptable per ADR 0004

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle       | Severity | Line(s) | Description                                                               |
| --- | --------------- | -------- | ------- | ------------------------------------------------------------------------- |
| 1   | P2-FileSize     | HIGH     | N/A     | No file size check — reading delegated to PythonParser without size limit |
| 2   | P2-Iteration    | LOW      | 108     | No 100K iteration cap on installed packages (test-only code)              |
| 3   | P2-StringLength | LOW      | N/A     | No 10MB per-field truncation                                              |

## Remediation Priority

1. Add `fs::metadata().len()` check (100MB limit) before reading (may need to coordinate with PythonParser delegation)
2. Add 100K iteration caps on installed packages list
3. Add lossy UTF-8 fallback with warning log
