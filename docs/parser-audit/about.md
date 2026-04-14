# ADR 0004 Security Audit: about

**File**: `src/parsers/about.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `exec()`, `eval()`, or subprocess calls. Uses `yaml_serde` for static YAML parsing. Uses `packageurl` crate for purl parsing. Uses `url` crate for URL parsing.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading. `fs::read_to_string` called at line 291 via `read_and_parse_yaml` without size pre-check.

### Recursion Depth

No recursive functions found. All processing is iterative over YAML fields. — PASS

### Iteration Count

- `extract_file_references` (line 345): Small fixed iteration (3 items) — PASS
- `build_extra_data` (line 449): Small fixed iteration (3 keys) — PASS
- No unbounded iteration loops

### String Length

No field-level truncation at 10MB. YAML values are used as-is.

## Principle 3: Archive Safety

**Status**: N/A

YAML files are not archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `fs::read_to_string` at line 291 fails on missing files, error propagated and handled in `extract_packages` (line 72). — Acceptable.

### UTF-8 Encoding

`fs::read_to_string` will fail on non-UTF-8. No lossy conversion fallback. — Minor gap.

### JSON/YAML Validity

YAML parse error at line 294 is handled, returns error string caught in `extract_packages` (line 72). Non-mapping root returns error (line 298). — PASS

### Required Fields

Missing name/version/purl result in `None` values with fallback chains (lines 143-157). — PASS

### URL Format

URLs parsed via `Url::parse` in `infer_about_from_download_url` (line 399). Invalid URLs return `None`. — PASS

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
| 1   | P2 File Size     | MEDIUM   | 291     | No file size check before reading |
| 2   | P2 String Length | LOW      | —       | No field-level 10MB truncation    |
| 3   | P4 UTF-8         | LOW      | 291     | No lossy UTF-8 fallback           |

## Remediation Priority

1. Add `fs::metadata().len()` check before reading, reject >100MB
