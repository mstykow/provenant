# ADR 0004 Security Audit: poetry_lock

**File**: `src/parsers/poetry_lock.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

- TOML parsing via `read_toml_file` at line 62 — static deserialization
- No `Command::new`, `subprocess`, `eval()`, `exec()` anywhere

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

- No `fs::metadata().len()` check before `read_toml_file` at line 62
- `read_toml_file` delegates to `read_file_to_string` without size check
- **GAP**: Entire file read into memory without size limit

### Recursion Depth

- No recursive functions in this parser
- PASS

### Iteration Count

- `parse_poetry_lock` iterates `packages` array without cap at line 86
- `extract_package_dependencies` iterates dependency tables without cap at lines 263, 276
- **GAP**: No 100,000 item cap on packages or dependencies

### String Length

- No 10MB per-field truncation for parsed values (name, version, requirement)

## Principle 3: Archive Safety

**Status**: N/A

Not an archive parser.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

- No `fs::metadata()` pre-check
- `read_toml_file` failure handled gracefully at lines 63-68

### UTF-8 Encoding

- `read_file_to_string` fails on invalid UTF-8 without lossy fallback
- **GAP**: No lossy UTF-8 conversion with warning

### JSON/YAML Validity

- TOML parse failure returns `default_package_data` at lines 64-68
- PASS — graceful degradation

### Required Fields

- `build_dependency_from_package` returns `None` if name or version missing at lines 182-190
- Missing fields result in `None` — continues parsing

### URL Format

- URLs accepted as-is — compliant with ADR 0004

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution with circular dependency risk.

## Additional Checks

### .unwrap() in Library Code

**Status**: FAIL

- Line 79: `.unwrap_or_default()` on `packages` — safe, uses `unwrap_or_default`
- Line 199: `.unwrap_or(false)` on `poetry_optional` — safe, uses `unwrap_or`
- Line 301: `.unwrap_or(false)` on `is_optional` — safe, uses `unwrap_or`
- No dangerous bare `.unwrap()` calls in library code

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle       | Severity | Line(s) | Description                                |
| --- | --------------- | -------- | ------- | ------------------------------------------ |
| 1   | P2-FileSize     | HIGH     | 62      | No file size check before `read_toml_file` |
| 2   | P2-Iteration    | LOW      | 86      | No 100K iteration cap on packages array    |
| 3   | P2-StringLength | LOW      | N/A     | No 10MB per-field truncation               |
| 4   | P4-UTF8         | LOW      | N/A     | No lossy UTF-8 fallback                    |

## Remediation Priority

1. Add `fs::metadata().len()` check (100MB limit) before reading poetry.lock
2. Add 100K iteration caps on package/dependency iteration
3. Add lossy UTF-8 fallback with warning log
