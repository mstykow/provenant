# ADR 0004 Security Audit: readme

**File**: `src/parsers/readme.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `exec()`, `eval()`, or subprocess calls. Simple line-based key:value parsing.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading. Uses `read_file_to_string(path)` at line 56 without size pre-check.

### Recursion Depth

No recursive functions. All processing is iterative over lines. — PASS

### Iteration Count

- `content.lines()` iteration (line 67): No 100K cap on number of lines processed. A file with millions of lines would be fully iterated.

### String Length

No field-level truncation at 10MB. Individual line values are used as-is (line 91-107).

## Principle 3: Archive Safety

**Status**: N/A

Text files are not archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `read_file_to_string` at line 56 returns error on missing files, handled via `match`. — Acceptable.

### UTF-8 Encoding

`read_file_to_string` (utils.rs) uses `read_to_string` which fails on non-UTF-8. No lossy conversion fallback. — Minor gap.

### JSON/YAML Validity

N/A — README files are plain text, not JSON/YAML.

### Required Fields

Missing name falls back to parent directory name (lines 115-120). — PASS

### URL Format

URLs accepted as-is. — Per ADR, acceptable.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code.

### Command::new / Subprocess Usage

**Status**: PASS

No subprocess calls found.

## Findings Summary

| #   | Principle        | Severity | Line(s) | Description                       |
| --- | ---------------- | -------- | ------- | --------------------------------- |
| 1   | P2 File Size     | MEDIUM   | 56      | No file size check before reading |
| 2   | P2 Iteration     | LOW      | 67      | No 100K cap on line iteration     |
| 3   | P2 String Length | LOW      | —       | No field-level 10MB truncation    |
| 4   | P4 UTF-8         | LOW      | 56      | No lossy UTF-8 fallback           |

## Remediation Priority

1. Add `fs::metadata().len()` check before reading, reject >100MB
2. Add line iteration cap (100K) on README parsing

## Remediation

- Finding #1 (P2 File Size): Already covered by `read_file_to_string(path, None)` which enforces 100MB size limit
- Finding #2 (P2 Iteration): Added `.take(MAX_ITERATION_COUNT)` cap on line iteration
- Finding #3 (P2 String Length): Applied `truncate_field()` to all extracted string values (name, version, copyright, download_url, homepage_url, extracted_license_statement, fallback name)
- Finding #4 (P4 UTF-8): Already covered by `read_file_to_string` (lossy UTF-8 fallback)
