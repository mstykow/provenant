# ADR 0004 Security Audit: gitmodules

**File**: `src/parsers/gitmodules.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `exec()`, `eval()`, or subprocess calls. Simple line-based INI-style parsing.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading. Uses `read_file_to_string(path)` at line 50 without size pre-check.

### Recursion Depth

No recursive functions. All processing is iterative over lines. — PASS

### Iteration Count

- `parse_gitmodules` (line 93): Iterates over `content.lines()` — no 100K cap on lines
- `submodules.into_iter().map()` (line 63): No 100K cap on number of submodules

### String Length

No field-level truncation at 10MB. URL and path values used as-is.

## Principle 3: Archive Safety

**Status**: N/A

INI-style text files are not archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `read_file_to_string` at line 50 returns error on missing files, handled via `match`. — Acceptable.

### UTF-8 Encoding

`read_file_to_string` fails on non-UTF-8. No lossy conversion fallback. — Minor gap.

### JSON/YAML Validity

N/A — .gitmodules is INI-format text, not JSON/YAML.

### Required Fields

Missing path/url in submodule section results in empty strings, checked at line 141-143. — PASS

### URL Format

URLs parsed via `parse_github_url`/`parse_gitlab_url` (lines 166-192) with simple string matching. Non-matching URLs result in `None` purl. — PASS

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

- Line 138: `.unwrap_or_default()` — safe
- Line 139: `.unwrap_or_default()` — safe
- Line 195: `.unwrap_or(path)` — safe
- No problematic `.unwrap()` calls in library code.

### Command::new / Subprocess Usage

**Status**: PASS

No subprocess calls found.

## Findings Summary

| #   | Principle        | Severity | Line(s) | Description                       |
| --- | ---------------- | -------- | ------- | --------------------------------- |
| 1   | P2 File Size     | MEDIUM   | 50      | No file size check before reading |
| 2   | P2 Iteration     | LOW      | 93, 63  | No 100K cap on lines/submodules   |
| 3   | P2 String Length | LOW      | —       | No field-level 10MB truncation    |
| 4   | P4 UTF-8         | LOW      | 50      | No lossy UTF-8 fallback           |

## Remediation Priority

1. Add `fs::metadata().len()` check before reading, reject >100MB
2. Add iteration cap (100K) on line parsing

## Remediation

| #   | Finding           | Fix                                                                             |
| --- | ----------------- | ------------------------------------------------------------------------------- |
| 1   | P2: File Size     | Already covered by `read_file_to_string` which enforces 100MB size limit        |
| 2   | P2: Iteration     | Added `MAX_ITERATION_COUNT` caps on 2 sites (line parsing, submodule iteration) |
| 3   | P2: String Length | Applied `truncate_field` on all output strings                                  |
| 4   | P4: UTF-8         | Already covered by `read_file_to_string` which provides lossy UTF-8 fallback    |
