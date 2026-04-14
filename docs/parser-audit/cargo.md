# ADR 0004 Security Audit: cargo

**File**: `src/parsers/cargo.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

Uses `toml::from_str` (line 212) for static TOML parsing. No `Command::new`, `subprocess`, `eval()`, or any code execution mechanism. All parsing is AST/structured-data based.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading. `read_cargo_toml` (line 206) opens and reads the entire file via `File::open` + `read_to_string` without a size check.

### Recursion Depth

`toml_to_json` (line 486) is recursive — it recurses into `toml::Value::Array` and `toml::Value::Table` variants. No depth tracking or limit. Bounded implicitly by the TOML parser's own nesting limits.

### Iteration Count

No 100K iteration cap on:

- `extract_dependencies` (line 305): iterates over all keys in a TOML table without cap
- `extract_keywords_and_categories` (line 421): iterates over keywords and categories arrays without cap
- `extract_parties` (line 248): iterates over authors array without cap
- `extract_extra_data` (line 505): iterates over multiple package fields without cap

### String Length

No 10 MB truncation with warning on any field value. String fields like `name`, `version`, `description`, `raw_license` are extracted from TOML values without size limits (lines 75-88, 141-144).

## Principle 3: Archive Safety

**Status**: N/A

Cargo.toml parser does not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `read_cargo_toml` (line 207) uses `File::open(path)` with error handling that returns error string on failure. Returns fallback `default_package_data()` on error (line 69). Does not panic, but doesn't use `fs::metadata()` as specified.

### UTF-8 Encoding

`file.read_to_string(&mut content)` (line 209) returns an error for non-UTF-8 files. The error is propagated and fallback data is returned. No explicit `String::from_utf8()` + warning + lossy conversion path.

### JSON/YAML Validity

`toml::from_str(&content)` (line 212) returns an error on invalid TOML, which is propagated as `Err(String)` and caught at line 65-70, returning `default_package_data()`. **PASS**.

### Required Fields

Missing `name` and `version` are handled as `Option<String>` (lines 75-83). When `None`, fields are populated as `None` in `PackageData`. PURL generation handles `None` gracefully (line 220). **PASS**.

### URL Format

URLs from `repository` and `homepage` fields are accepted as-is (lines 121-124, 112-119). Per ADR 0004, accept as-is is correct. **PASS**.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution performed. Dependencies are extracted from manifest declarations only.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle           | Severity | Line(s)        | Description                                                                    |
| --- | ------------------- | -------- | -------------- | ------------------------------------------------------------------------------ |
| 1   | P2: File Size       | Medium   | 206-210        | No `fs::metadata().len()` check before reading; entire file loaded into memory |
| 2   | P2: Recursion Depth | Low      | 486-502        | `toml_to_json` recurses into nested TOML values without depth tracking         |
| 3   | P2: Iteration Count | Low      | 305, 421, 248  | No 100K iteration cap on dependency/keyword/author processing                  |
| 4   | P2: String Length   | Low      | 75-88, 141-144 | No 10 MB truncation with warning on string field values                        |
| 5   | P4: File Exists     | Low      | 207            | Uses `File::open` instead of `fs::metadata()` pre-check                        |
| 6   | P4: UTF-8 Encoding  | Low      | 209            | No lossy UTF-8 conversion path; invalid UTF-8 causes fallback data return      |

## Remediation Priority

1. Add `fs::metadata().len()` check with 100 MB limit before reading file (line 206)
2. Add depth tracking to `toml_to_json` recursion (line 486)
3. Add iteration count cap (100K) on dependency/keyword processing loops
4. Add 10 MB string field truncation with warning
5. Add `fs::metadata()` pre-check before file read
6. Add lossy UTF-8 conversion with warning for encoding errors

## Remediation

**PR**: #664
**Date**: 2026-04-14

All findings addressed in ADR 0004 security compliance batch 1.
