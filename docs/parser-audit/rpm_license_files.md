# ADR 0004 Security Audit: rpm_license_files

**File**: `src/parsers/rpm_license_files.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, or subprocess calls. Pure path-based name extraction.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check. However, this parser does NOT read file contents — it only extracts the package name from the file path. No file content is read into memory. PASS for file size (content not read).

### Recursion Depth

No recursive functions. PASS.

### Iteration Count

No loops that iterate over potentially large datasets. PASS.

### String Length

No 10MB truncation on extracted name/path values, but values come from path components which are inherently bounded by filesystem limits. LOW risk.

## Principle 3: Archive Safety

**Status**: N/A

No archive extraction.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No file content reading occurs. The parser only inspects the path string. N/A for file existence check of content.

### UTF-8 Encoding

`path.to_string_lossy()` at line 41 and `path_str.split()` at line 61 use lossy conversion implicitly through `to_string_lossy()`. PASS for UTF-8.

### JSON/YAML Validity

No JSON/YAML parsing. N/A.

### Required Fields

`extract_packages()` line 61: `name` is extracted from path. If path doesn't contain `usr/share/licenses/`, `name` will be `None`. PackageData is still returned with `None` name. Acceptable per ADR.

### URL Format

URLs accepted as-is. PASS per ADR.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle | Severity | Line(s) | Description                                                         |
| --- | --------- | -------- | ------- | ------------------------------------------------------------------- |
| 1   | P2        | LOW      | —       | No string length truncation, but values are filesystem-path-bounded |

## Remediation Priority

1. Consider adding a string length cap on extracted path-based name values (LOW priority — filesystem already limits path component length)
