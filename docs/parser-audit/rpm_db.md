# ADR 0004 Security Audit: rpm_db

**File**: `src/parsers/rpm_db.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: FAIL

**CRITICAL**: `Command::new("rpm")` is used as a fallback when native parsing fails:

- Line 325: `Command::new("rpm").args(["--dbpath"]).arg(&rpmdb_dir).args(["--query", "--all", "--queryformat", RPM_QUERY_FORMAT]).output()` — executes the `rpm` CLI tool
- Line 353: `Command::new("rpm").arg("--version").output()` — checks if rpm CLI is available

This is a direct violation of ADR 0004 Principle 1: "FORBIDDEN: subprocess calls, Command::new". The `rpm` CLI is executed with user-controlled database paths.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. The native parser reads the entire database file. `read_installed_rpm_packages()` is called without pre-checking file size.

### Recursion Depth

No recursive functions. PASS.

### Iteration Count

No 100K iteration cap on loops:

- `parse_rpm_query_output()` line 359: iterates all package blocks without limit
- `build_package_data()` line 464: iterates all requires without limit
- `build_file_references()` line 272: iterates all base_names without limit
- `build_file_references_from_paths()` line 295: iterates all paths without limit

### String Length

No 10MB truncation on field values from rpm query output or native parsing.

## Principle 3: Archive Safety

**Status**: N/A

No archive extraction. RPM databases are raw database files, not archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

The native parser (`BdbDatabase::open()`, `NdbDatabase::open()`) uses `File::open()` which returns error on missing files. `rpm_command_available()` at line 352 handles missing `rpm` binary. However, no explicit `fs::metadata()` pre-check.

### UTF-8 Encoding

- Line 333: `String::from_utf8_lossy(&output.stderr)` — uses lossy conversion for rpm CLI output. PASS for stderr.
- Line 346: `String::from_utf8(output.stdout)` — uses strict UTF-8 for rpm query output, returns error on invalid UTF-8. Should use lossy conversion.
- Native parser uses `String::from_utf8_lossy()` in `entry.rs:61` for string reading. PASS.

### JSON/YAML Validity

No JSON/YAML parsing. N/A.

### Required Fields

`build_package_data()` line 443: `name` is optional; missing name results in `None`. `version` is constructed via `build_evr_version()` which returns `None` for empty versions. Acceptable per ADR.

### URL Format

URLs accepted as-is. PASS per ADR.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution; only parsing declared dependencies.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code (excluding test module at line 614).

### Command::new / Subprocess Usage

**Status**: FAIL

- Line 325: `Command::new("rpm")` — executes rpm CLI with user-controlled `--dbpath`
- Line 353: `Command::new("rpm")` — checks rpm CLI availability

## Findings Summary

| #   | Principle | Severity | Line(s)         | Description                                                            |
| --- | --------- | -------- | --------------- | ---------------------------------------------------------------------- |
| 1   | P1        | CRITICAL | 325,353         | Command::new("rpm") subprocess execution violates ADR 0004 Principle 1 |
| 2   | P2        | HIGH     | —               | No file size check before reading (100MB limit)                        |
| 3   | P2        | MEDIUM   | 359,464,272,295 | No iteration count cap (100K items)                                    |
| 4   | P2        | MEDIUM   | —               | No string length truncation (10MB per field)                           |
| 5   | P4        | LOW      | —               | No explicit fs::metadata() pre-check                                   |
| 6   | P4        | LOW      | 346             | String::from_utf8() on rpm stdout should use lossy conversion          |

## Remediation Priority

1. **CRITICAL**: Remove `Command::new("rpm")` subprocess calls; rely solely on native RPM database parsing
2. Add fs::metadata().len() check before reading with 100MB limit
3. Add iteration count caps on package/dependency/file loops
4. Add String::from_utf8_lossy() for rpm query output
5. Add string length truncation on field values

## Remediation

1. **P1 CRITICAL**: `Command::new("rpm")` subprocess execution — REMOVED entirely; native parsing only
2. **P2 HIGH**: No file size check — Added `fs::metadata()` pre-check with `MAX_MANIFEST_SIZE=100MB`
3. **P2 MEDIUM**: No iteration caps — Added `MAX_ITERATION_COUNT` caps on packages, requires, file_names, base_names, dir_names
4. **P2 MEDIUM**: No string truncation — Added `truncate_field()` to all extracted string values
5. **P4 LOW**: No `fs::metadata` pre-check — Covered by finding #2
6. **P4 LOW**: `String::from_utf8` on rpm stdout — Moot since `Command::new` removed; native parsing uses lossy UTF-8
