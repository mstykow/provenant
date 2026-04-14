# ADR 0004 Security Audit: go

**File**: `src/parsers/go.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

Uses line-by-line text parsing and `serde_json::from_str` for static analysis. No `Command::new`, `subprocess`, `eval()`, or any code execution mechanism. All four parsers (GoMod, GoSum, GoWork, Godeps) use static parsing only.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading. All four `extract_packages` methods use `fs::read_to_string(path)` without a size check:

- GoModParser: line 46
- GoSumParser: line 539
- GoWorkParser: line 656
- GodepsParser: line 1037

Additionally, `resolve_workspace_use_dependencies` (line 848) reads additional `go.mod` files via `fs::read_to_string` without size checks.

### Recursion Depth

No recursive functions in any parser. All parsing is iterative over `content.lines()`. The `parse_go_tokens` function (line 975) uses a simple loop. **PASS**.

### Iteration Count

No 100K iteration cap on:

- `parse_go_mod` (line 82): iterates over all lines without cap
- `parse_go_sum` (line 559): iterates over all lines without cap
- `parse_go_work` (line 680): iterates over all lines without cap
- `parse_godeps_json` (line 1086): iterates over all Deps array entries without cap
- `resolve_workspace_use_dependencies` (line 846): iterates over all use_paths without cap

### String Length

No 10 MB truncation with warning on any field value. Module paths, versions, and other string fields are extracted without size limits throughout all four parsers.

## Principle 3: Archive Safety

**Status**: N/A

Go parsers do not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. All parsers use `fs::read_to_string(path)` with error handling that returns fallback data. Does not panic, but doesn't use `fs::metadata()` as specified.

### UTF-8 Encoding

`fs::read_to_string` returns an error for non-UTF-8 files. Error is caught and fallback data returned. No explicit `String::from_utf8()` + warning + lossy conversion path.

### JSON/YAML Validity

`serde_json::from_str` (line 1054) for Godeps.json returns an error on invalid JSON, caught at line 1056-1059, returning `default_godeps_package_data()`. **PASS**.

### Required Fields

Missing module name/version handled gracefully throughout. `parse_go_mod` returns `None` for missing namespace/name (lines 72-73). PURL construction handles `None` values. **PASS**.

### URL Format

URLs are constructed programmatically (lines 211-215, 1115-1119), not parsed from user input. Per ADR 0004, accept as-is is correct. **PASS**.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution performed. Dependencies are extracted from file declarations only.

## Additional Checks

### .unwrap() in Library Code

**Status**: FAIL

- Line 573: `raw_version.strip_suffix("/go.mod").unwrap_or(raw_version)` — this is `unwrap_or`, which is a safe fallback, not a panic-risk `unwrap()`. **Actually PASS** — `unwrap_or` is the safe variant.

No unsafe `.unwrap()` calls in library code.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle           | Severity | Line(s)                 | Description                                                                     |
| --- | ------------------- | -------- | ----------------------- | ------------------------------------------------------------------------------- |
| 1   | P2: File Size       | Medium   | 46, 539, 656, 848, 1037 | No `fs::metadata().len()` check before reading; entire files loaded into memory |
| 2   | P2: Iteration Count | Low      | 82, 559, 680, 846, 1086 | No 100K iteration cap on line/dependency processing                             |
| 3   | P2: String Length   | Low      | Multiple                | No 10 MB truncation with warning on string field values                         |
| 4   | P4: File Exists     | Low      | 46, 539, 656, 1037      | Uses `fs::read_to_string` instead of `fs::metadata()` pre-check                 |
| 5   | P4: UTF-8 Encoding  | Low      | 46, 539, 656, 1037      | No lossy UTF-8 conversion path; invalid UTF-8 causes fallback data return       |

## Remediation Priority

1. Add `fs::metadata().len()` check with 100 MB limit before reading files (lines 46, 539, 656, 848, 1037)
2. Add iteration count cap (100K) on line/dependency processing loops
3. Add 10 MB string field truncation with warning
4. Add `fs::metadata()` pre-check before file read
5. Add lossy UTF-8 conversion with warning for encoding errors

## Remediation

| #   | Finding             | Fix                                                                                                                                                           |
| --- | ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | P2: File Size       | Replaced all 5 `fs::read_to_string` calls with `read_file_to_string(path, None)` — GoMod, GoSum, GoWork, resolve_workspace_use_dependencies, Godeps           |
| 2   | P2: Iteration Count | Added `.take(MAX_ITERATION_COUNT)` to 6 line/dependency iteration loops across all parsers                                                                    |
| 3   | P2: String Length   | Applied `truncate_field()` to all extracted string values across all 4 parsers (namespace, name, version, purl, extracted_requirement, homepage_url, vcs_url) |
| 4   | P4: File Exists     | `read_file_to_string` uses `fs::metadata()` pre-check internally                                                                                              |
| 5   | P4: UTF-8 Encoding  | `read_file_to_string` provides lossy UTF-8 fallback internally                                                                                                |
