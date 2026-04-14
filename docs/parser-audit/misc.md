# ADR 0004 Security Audit: misc

**File**: `src/parsers/misc.rs`
**Date**: 2026-04-14
**Status**: COMPLIANT

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `exec()`, `eval()`, or subprocess calls. All recognizers use simple file extension/path matching and optional magic byte detection via `magic::is_squashfs`, `magic::is_zip`, `magic::is_nsis_installer`.

## Principle 2: DoS Protection

**Status**: PASS

### File Size

No files are read. All recognizers return minimal `PackageData` with only `package_type` and `datasource_id` set (line 45-49). The `extract_packages` function discards the `path` parameter entirely (line 44: `let _ = path;`). — PASS (no file reading = no DoS via file size).

### Recursion Depth

No recursive functions. — PASS

### Iteration Count

No iteration loops. — PASS

### String Length

No string fields are populated. — PASS

## Principle 3: Archive Safety

**Status**: N/A

No archives are extracted. Recognizers only identify file types.

## Principle 4: Input Validation

**Status**: PASS

### File Exists

No file reading occurs. File existence is not checked because it's not needed — the `is_match` function only inspects the path string. — PASS

### UTF-8 Encoding

No file content is read. — PASS

### JSON/YAML Validity

N/A — No parsing occurs.

### Required Fields

Minimal `PackageData` is returned with `package_type` and `datasource_id`. — PASS (by design)

### URL Format

N/A — No URLs are handled.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in this module.

### Command::new / Subprocess Usage

**Status**: PASS

No subprocess calls found.

## Findings Summary

| #   | Principle | Severity | Line(s) | Description |
| --- | --------- | -------- | ------- | ----------- |

No findings. This module is data-only — it identifies file types by extension/path/magic bytes and returns minimal metadata without reading file contents.

## Remediation Priority

None required.
