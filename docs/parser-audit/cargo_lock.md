# ADR 0004 Security Audit: cargo_lock

**File**: `src/parsers/cargo_lock.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

Uses `toml::from_str` (line 161) for static TOML parsing. No `Command::new`, `subprocess`, `eval()`, or any code execution mechanism.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading. `read_cargo_lock` (line 156) opens and reads the entire file via `File::open` + `read_to_string` without a size check.

### Recursion Depth

No recursive functions. All parsing is iterative — `extract_all_dependencies` (line 172) iterates over packages, `build_package_versions` (line 291) and `build_package_provenance` (line 307) use `fold`. **PASS**.

### Iteration Count

No 100K iteration cap on:

- `extract_all_dependencies` (line 182): iterates over all packages and their dependencies without cap
- Inner loop at line 189: iterates over all dependencies in each package without cap
- `build_package_versions` (line 291): processes all packages without cap

### String Length

No 10 MB truncation with warning on any field value. String fields like `name`, `version`, `checksum` are extracted from TOML values without size limits (lines 66-79).

## Principle 3: Archive Safety

**Status**: N/A

Cargo.lock parser does not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `read_cargo_lock` (line 157) uses `File::open(path)` with error handling. Returns fallback `default_package_data()` on error (line 51). Does not panic, but doesn't use `fs::metadata()` as specified.

### UTF-8 Encoding

`file.read_to_string(&mut content)` (line 159) returns an error for non-UTF-8 files. Error is propagated and fallback data returned. No explicit `String::from_utf8()` + warning + lossy conversion path.

### JSON/YAML Validity

`toml::from_str(&content)` (line 161) returns an error on invalid TOML, caught at line 48-53, returning `default_package_data()`. Also handles missing `package` array at line 56-62. **PASS**.

### Required Fields

Missing `name` and `version` are handled as `Option<String>` (lines 66-74). When `None`, fields are populated as `None` in `PackageData`. PURL generation handles missing values gracefully (lines 95-101). **PASS**.

### URL Format

URLs are constructed programmatically (lines 103-107), not parsed from user input. **PASS**.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution performed. Dependencies are extracted from lockfile declarations only.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle           | Severity | Line(s)       | Description                                                                    |
| --- | ------------------- | -------- | ------------- | ------------------------------------------------------------------------------ |
| 1   | P2: File Size       | Medium   | 156-160       | No `fs::metadata().len()` check before reading; entire file loaded into memory |
| 2   | P2: Iteration Count | Low      | 182, 189, 291 | No 100K iteration cap on package/dependency processing                         |
| 3   | P2: String Length   | Low      | 66-79         | No 10 MB truncation with warning on string field values                        |
| 4   | P4: File Exists     | Low      | 157           | Uses `File::open` instead of `fs::metadata()` pre-check                        |
| 5   | P4: UTF-8 Encoding  | Low      | 159           | No lossy UTF-8 conversion path; invalid UTF-8 causes fallback data return      |

## Remediation Priority

1. Add `fs::metadata().len()` check with 100 MB limit before reading file (line 156)
2. Add iteration count cap (100K) on package/dependency processing loops
3. Add 10 MB string field truncation with warning
4. Add `fs::metadata()` pre-check before file read
5. Add lossy UTF-8 conversion with warning for encoding errors

## Remediation

- Finding #1 (P2 File Size): Replaced `File::open`+`read_to_string` with `read_file_to_string(path, None)`
- Finding #2 (P2 Iteration Count): Added `MAX_ITERATION_COUNT` caps to packages and deps iteration
- Finding #3 (P2 String Length): Applied `truncate_field()` to all extracted string values (name, version, checksum, purl, extracted_requirement, source, api_data_url)
- Findings #4, #5 (P4): Fixed automatically by switching to `read_file_to_string`
