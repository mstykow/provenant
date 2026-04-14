# ADR 0004 Security Audit: podfile_lock

**File**: `src/parsers/podfile_lock.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `subprocess`, `eval()`, `exec()`, or code execution primitives found. Parsing uses `yaml_serde` for YAML deserialization and structured data traversal — no code execution.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

- Line 63: `fs::read_to_string(path)` called directly with **no** `fs::metadata().len()` pre-check. A multi-GB Podfile.lock would be fully loaded into memory.

### Recursion Depth

- No recursive functions. All parsing iterates over YAML sequences and mappings linearly (lines 101-187, 194-218). **PASS**.

### Iteration Count

- Line 101: `for pod in pods` — iterates over PODS sequence with **no** 100K cap.
- Line 117: `for dep in deps` — iterates over DEPENDENCIES sequence with **no** 100K cap.
- Line 132: `for package in packages` — iterates over SPEC REPOS packages with **no** 100K cap.
- Line 145: `for (name_key, checksum_val) in checksums` — iterates over CHECKSUMS with **no** 100K cap.
- Line 194: `for pod in pods` — second iteration over PODS with **no** 100K cap.
- Line 198: `for (main_pod_key, dep_pods_val) in m` — iterates over mapping entries with **no** 100K cap.
- No cap on number of dependencies collected (line 192).

### String Length

- No field values are truncated at 10MB. Values extracted from YAML (e.g., `dep.as_str()`, `checksum_val.as_str()`) are stored as-is regardless of length.

## Principle 3: Archive Safety

**Status**: N/A

This parser does not handle archive files.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

- Line 63-68: `fs::read_to_string` error is caught and returns `default_package_data()`. No explicit `fs::metadata()` pre-check. Functionally acceptable but doesn't match ADR prescription.

### UTF-8 Encoding

- Line 63: `fs::read_to_string` fails on non-UTF-8 content, returning default PackageData. **No** lossy fallback is attempted. **FAIL** — non-UTF-8 files silently return default data.

### JSON/YAML Validity

- Line 71-76: YAML parse failure (`yaml_serde::from_str`) is caught with `warn!()` and returns `default_package_data()`. **PASS** — graceful handling on invalid YAML.

### Required Fields

- Missing name/version in dependency entries result in empty strings or `None` values. The code continues processing. **PASS**.

### URL Format

- URLs from external sources (git, path) are extracted from YAML mappings and stored as-is. Per ADR: "Accept as-is". **PASS**.

## Principle 5: Circular Dependency Detection

**Status**: N/A

This parser does not perform dependency resolution. It reads resolved dependency data from a lockfile — no traversal or resolution logic.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls found in library code. The only `unwrap_or_default()` usage (line 203) is safe.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new`, `std::process::Command`, or subprocess usage found.

## Findings Summary

| #   | Principle | Severity | Line(s)                      | Description                                                                 |
| --- | --------- | -------- | ---------------------------- | --------------------------------------------------------------------------- |
| 1   | P2: DoS   | Medium   | 63                           | No file size pre-check (`fs::metadata().len()`) before `fs::read_to_string` |
| 2   | P2: DoS   | Medium   | 101, 117, 132, 145, 194, 198 | No iteration count cap (100K) on YAML sequence/mapping iteration loops      |
| 3   | P2: DoS   | Low      | Various                      | No string field truncation at 10MB limit                                    |
| 4   | P4: Input | Medium   | 63                           | No lossy UTF-8 fallback — non-UTF-8 files cause silent default return       |

## Remediation Priority

1. **[P2: DoS] Add file size pre-check** using `fs::metadata().len()` with 100MB limit before reading
2. **[P4: Input] Add lossy UTF-8 fallback** — use `fs::read()` + `String::from_utf8_lossy()` instead of `fs::read_to_string()`
3. **[P2: DoS] Add iteration count caps** (100K) to all YAML sequence/mapping loops with early-break and warning
4. **[P2: DoS] Add string field truncation** at 10MB with warning log
