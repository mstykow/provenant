# ADR 0004 Security Audit: utils

**File**: `src/parsers/utils.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

- Pure string manipulation and base64 decoding — no code execution
- No `Command::new`, `subprocess`, `eval()`, `exec()` anywhere

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

- `read_file_to_string` at line 33 reads entire file with `file.read_to_string` — no size limit
- **GAP**: This is the primary shared file-reading utility and has no `fs::metadata().len()` pre-check
- This affects ALL parsers that use this function (python.rs, pipfile_lock.rs, poetry_lock.rs, pylock_toml.rs, uv_lock.rs, conan_data.rs)

### Recursion Depth

- No recursive functions
- PASS

### Iteration Count

- `split_name_email` iterates characters at line 129 — O(n) bounded
- `parse_sri` iterates decoded bytes at line 85 — O(n) bounded
- `npm_purl` string splitting at line 46 — O(1) bounded
- PASS — no unbounded iteration

### String Length

- No 10MB truncation for file contents read via `read_file_to_string`
- No truncation for individual field values

## Principle 3: Archive Safety

**Status**: N/A

Not an archive parser.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

- No `fs::metadata()` pre-check in `read_file_to_string`
- `File::open` failure propagated as `anyhow::Error`
- **GAP**: No explicit existence check before opening

### UTF-8 Encoding

- `file.read_to_string` at line 36 fails on invalid UTF-8
- **GAP**: No lossy UTF-8 conversion with warning — this is the central utility used by many parsers

### JSON/YAML Validity

- N/A — no JSON/YAML parsing in this module

### Required Fields

- N/A — this module provides utility functions, not direct parsing

### URL Format

- N/A — URLs not handled in this module

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

- No `.unwrap()` calls in library code
- Test code uses `.unwrap()` which is acceptable per ADR 0004

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle     | Severity | Line(s) | Description                                                                          |
| --- | ------------- | -------- | ------- | ------------------------------------------------------------------------------------ |
| 1   | P2-FileSize   | CRITICAL | 33-37   | `read_file_to_string` reads entire file without size check — used by 6+ parsers      |
| 2   | P4-UTF8       | HIGH     | 36      | `read_to_string` fails on invalid UTF-8 without lossy fallback — affects all callers |
| 3   | P4-FileExists | LOW      | 34      | No `fs::metadata()` pre-check before `File::open`                                    |

## Remediation Priority

1. **CRITICAL**: Add `fs::metadata().len()` check (100MB default) in `read_file_to_string` before reading — this is the single highest-impact fix as it protects all dependent parsers
2. Add lossy UTF-8 fallback path: on `read_to_string` failure, try `read_to_end` + `String::from_utf8_lossy` with a warning log
3. Optionally add `fs::metadata()` existence check before `File::open`

## Remediation

**PR**: #664
**Date**: 2026-04-14

All findings addressed in ADR 0004 security compliance batch 1.
