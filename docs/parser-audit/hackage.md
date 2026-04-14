# ADR 0004 Security Audit: hackage

**File**: `src/parsers/hackage.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

Uses line-by-line text parsing for `.cabal` and `cabal.project` files, and `yaml_serde::from_str` (line 80) for `stack.yaml`. No `Command::new`, `subprocess`, `eval()`, or any code execution mechanism. Regex usage (line 658, 964) is for pattern matching only, not code execution.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading. All three `extract_packages` methods use `fs::read_to_string(path)` without a size check:

- HackageCabalParser: line 32
- HackageCabalProjectParser: line 52
- HackageStackYamlParser: line 72

### Recursion Depth

No recursive functions. All parsing is iterative via `while index < lines.len()` loops (lines 193, 333). `collect_indented_field` (line 742) uses a forward scan, not recursion. **PASS**.

### Iteration Count

No 100K iteration cap on:

- `parse_cabal_data` (line 333): iterates over all lines without cap
- `parse_cabal_project` (line 193): iterates over all lines without cap
- `parse_stack_yaml` (line 299-306): iterates over packages and extra-deps arrays without cap
- `split_dependency_entries` (line 769): iterates over all characters in dependency strings without cap
- `parse_hackage_spec_dependency` (line 617): called per dependency entry without cap on total calls

### String Length

No 10 MB truncation with warning on any field value. Fields like `name`, `version`, `description`, `synopsis`, `license` are extracted without size limits (lines 356-362, 826-841).

## Principle 3: Archive Safety

**Status**: N/A

Hackage parsers do not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. All three parsers use `fs::read_to_string(path)` with error handling that returns `default_package_data()`. Does not panic, but doesn't use `fs::metadata()` as specified.

### UTF-8 Encoding

`fs::read_to_string` returns an error for non-UTF-8 files. Error is caught and fallback data returned. No explicit `String::from_utf8()` + warning + lossy conversion path.

### JSON/YAML Validity

`yaml_serde::from_str` (line 80) returns an error on invalid YAML, caught at lines 82-85, returning `default_package_data()`. **PASS**.

### Required Fields

Missing `name` and `version` handled as `Option<String>` (lines 356-357). When `None`, fields are populated as `None` in `PackageData`. PURL handles missing values via `build_hackage_purl` (line 919-924). **PASS**.

### URL Format

URLs from `homepage` and `bug-reports` fields are accepted as-is (lines 363-364). Per ADR 0004, accept as-is is correct. **PASS**.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution performed. Dependencies are extracted from file declarations only.

## Additional Checks

### .unwrap() in Library Code

**Status**: FAIL

- Line 534: `parse_hackage_spec_dependency(package_spec, Some("extra-deps"), None, None).unwrap_or(...)` — this is `unwrap_or`, which is a safe fallback. **Not a violation**.
- Line 811: `line.strip_prefix("-").unwrap_or(line)` — this is `unwrap_or`, which is a safe fallback. **Not a violation**.

No unsafe `.unwrap()` calls in library code. All uses are the safe `unwrap_or` variant. **PASS**.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle           | Severity | Line(s)            | Description                                                                     |
| --- | ------------------- | -------- | ------------------ | ------------------------------------------------------------------------------- |
| 1   | P2: File Size       | Medium   | 32, 52, 72         | No `fs::metadata().len()` check before reading; entire files loaded into memory |
| 2   | P2: Iteration Count | Low      | 193, 333, 299, 769 | No 100K iteration cap on line/dependency processing                             |
| 3   | P2: String Length   | Low      | 356-362, 826-841   | No 10 MB truncation with warning on string field values                         |
| 4   | P4: File Exists     | Low      | 32, 52, 72         | Uses `fs::read_to_string` instead of `fs::metadata()` pre-check                 |
| 5   | P4: UTF-8 Encoding  | Low      | 32, 52, 72         | No lossy UTF-8 conversion path; invalid UTF-8 causes fallback data return       |

## Remediation Priority

1. Add `fs::metadata().len()` check with 100 MB limit before reading files (lines 32, 52, 72)
2. Add iteration count cap (100K) on line/dependency processing loops
3. Add 10 MB string field truncation with warning
4. Add `fs::metadata()` pre-check before file read
5. Add lossy UTF-8 conversion with warning for encoding errors
