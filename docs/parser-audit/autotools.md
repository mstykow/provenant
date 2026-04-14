# ADR 0004 Security Audit: autotools

**File**: `src/parsers/autotools.rs`
**Date**: 2026-04-14
**Status**: COMPLIANT

## Principle 1: No Code Execution

**Status**: PASS

Does not parse file contents at all. Extracts package name from the parent directory path only (line 41-50). No `Command::new`, `subprocess`, `eval()`, or any code execution mechanism.

## Principle 2: DoS Protection

**Status**: PASS

### File Size

N/A — the parser does not read file contents. No file I/O beyond path manipulation.

### Recursion Depth

No recursive functions. **PASS**.

### Iteration Count

No iteration over file contents or collections. **PASS**.

### String Length

The only string extracted is the parent directory name (line 41-50), which is bounded by filesystem path limits. **PASS**.

## Principle 3: Archive Safety

**Status**: N/A

Autotools parser does not handle archives.

## Principle 4: Input Validation

**Status**: PASS

### File Exists

N/A — the parser does not read file contents. The `extract_packages` function only uses `path.parent()` and `path.file_name()` for path manipulation. No file I/O is performed, so no file existence check is needed.

### UTF-8 Encoding

N/A — no file content is read. Path components are accessed via `to_str()` which returns `None` for non-UTF-8 paths, handled at line 44.

### JSON/YAML Validity

N/A — no JSON/YAML parsing.

### Required Fields

Missing parent directory name is handled with a fallback to `"input"` (line 47-49). **PASS**.

### URL Format

N/A — no URL fields.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution performed.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle | Severity | Line(s) | Description |
| --- | --------- | -------- | ------- | ----------- |

No findings. The parser does not read file contents, does not recurse, does not iterate over collections, and has no code execution vectors.

## Remediation Priority

No remediation needed. This parser is fully compliant with ADR 0004 by virtue of not reading or parsing file contents.
