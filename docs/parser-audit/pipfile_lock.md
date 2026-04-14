# ADR 0004 Security Audit: pipfile_lock

**File**: `src/parsers/pipfile_lock.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

- JSON parsing via `serde_json::from_str` at line 85 — static deserialization
- TOML parsing via `read_toml_file` at line 217 — static deserialization
- No `Command::new`, `subprocess`, `eval()`, `exec()` anywhere

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

- No `fs::metadata().len()` check before `fs::read_to_string` at line 77 (Pipfile.lock)
- `read_toml_file` (via `python::read_toml_file`) uses `read_file_to_string` without size check
- **GAP**: Entire file read into memory without size limit

### Recursion Depth

- No recursive functions in this parser
- PASS

### Iteration Count

- `extract_lockfile_dependencies` iterates `section_map` entries without cap at line 136
- `extract_pipfile_dependencies` iterates package entries without cap at line 259
- `parse_pipfile_sources` iterates sources array without cap at line 353
- **GAP**: No 100,000 item cap on dependencies or sources

### String Length

- No 10MB per-field truncation for parsed values (name, version, requirement strings)

## Principle 3: Archive Safety

**Status**: N/A

Not an archive parser.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

- No `fs::metadata()` pre-check — `fs::read_to_string` at line 77 returns error
- Error handled gracefully with `warn!` and default return at lines 79-82

### UTF-8 Encoding

- `fs::read_to_string` fails on invalid UTF-8 without lossy fallback
- **GAP**: No lossy UTF-8 conversion with warning

### JSON/YAML Validity

- JSON parse failure returns `default_package_data` at lines 87-90
- TOML parse failure returns `default_package_data` at lines 219-222
- PASS — graceful degradation

### Required Fields

- Missing name/version results in `None` — continues parsing
- `build_lockfile_dependency` returns `None` if requirement extraction fails at line 153

### URL Format

- URLs accepted as-is — compliant with ADR 0004

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution with circular dependency risk.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

- No `.unwrap()` calls in library code
- All `Option`/`Result` values handled with `?`, `match`, or `unwrap_or` patterns

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle       | Severity | Line(s)  | Description                                                     |
| --- | --------------- | -------- | -------- | --------------------------------------------------------------- |
| 1   | P2-FileSize     | HIGH     | 77       | No file size check before `fs::read_to_string` for Pipfile.lock |
| 2   | P2-FileSize     | HIGH     | 217      | No file size check before `read_toml_file` for Pipfile          |
| 3   | P2-Iteration    | LOW      | 136, 259 | No 100K iteration cap on dependencies                           |
| 4   | P2-StringLength | LOW      | N/A      | No 10MB per-field truncation                                    |
| 5   | P4-UTF8         | LOW      | 77       | No lossy UTF-8 fallback                                         |

## Remediation Priority

1. Add `fs::metadata().len()` check (100MB limit) before reading Pipfile.lock and Pipfile
2. Add 100K iteration caps on dependency/package iteration
3. Add lossy UTF-8 fallback with warning log
