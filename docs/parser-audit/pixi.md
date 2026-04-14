# ADR 0004 Security Audit: pixi

**File**: `src/parsers/pixi.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `exec()`, `eval()`, or subprocess calls. Uses `serde` for JSON/TOML deserialization and `yaml_serde` for YAML — all static parsing.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading. Files are read via `read_toml_file(path)` (line 50) and `read_file_to_string(path)` (line 72) — both read entire content without size pre-check.

### Recursion Depth

No recursive functions found in pixi.rs. All processing is iterative. — PASS

### Iteration Count

- `extract_manifest_dependencies` (line 250): Iterates over feature table entries — no 100K cap
- `extract_conda_dependencies` (line 292): Iterates over dependency table — no 100K cap
- `extract_pypi_dependencies` (line 346): Iterates over dependency table — no 100K cap
- `extract_v6_lock_dependencies` (line 462): Iterates over packages array — no 100K cap
- `collect_v6_package_refs` (line 475): Nested iteration over environments → platforms → entries — no 100K cap
- `extract_v4_lock_dependencies` (line 615): Iterates over packages array — no 100K cap
- `extract_authors` (line 182): Iterates over authors array — no 100K cap

### String Length

No field-level truncation at 10MB.

## Principle 3: Archive Safety

**Status**: N/A

Pixi files are not archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. Uses `read_toml_file` and `read_file_to_string` which return errors on missing files, handled gracefully via `match`. — Acceptable fallback.

### UTF-8 Encoding

`read_file_to_string` (from utils.rs line 33) uses `File::open` + `read_to_string`, which will fail on non-UTF-8. No lossy conversion fallback. — Minor gap.

### JSON/YAML Validity

- TOML parse errors (line 141): Handled, falls back to YAML parsing attempt
- YAML parse errors (line 145): Handled, returns error string
- Both return `default_package_data` on failure — PASS

### Required Fields

Missing name/version fields result in `None` values — PASS (line 102-108)

### URL Format

URLs accepted as-is. — Per ADR, acceptable.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution with cycle tracking.

## Additional Checks

### .unwrap() in Library Code

**Status**: FAIL

- Line 754: `conda_name_from_locator` uses `.unwrap_or(file_name)` — this is safe, not a concern
- No problematic `.unwrap()` calls found in library code.

### Command::new / Subprocess Usage

**Status**: PASS

No subprocess calls found.

## Findings Summary

| #   | Principle        | Severity | Line(s)                      | Description                                                                     |
| --- | ---------------- | -------- | ---------------------------- | ------------------------------------------------------------------------------- |
| 1   | P2 File Size     | MEDIUM   | 50, 72                       | No file size check before reading — oversized files loaded entirely into memory |
| 2   | P2 Iteration     | LOW      | 250, 292, 346, 462, 475, 615 | No 100K iteration cap on dependency/table entries                               |
| 3   | P2 String Length | LOW      | —                            | No field-level 10MB truncation                                                  |
| 4   | P4 UTF-8         | LOW      | 50, 72                       | No lossy UTF-8 fallback on non-UTF-8 content                                    |

## Remediation Priority

1. Add `fs::metadata().len()` check before reading files, reject >100MB
2. Add iteration caps (100K) on dependency extraction loops

## Remediation

- **#1 P2 File Size**: Already covered by `read_file_to_string` and `read_toml_file` — both enforce size limits. Verified, no changes needed.
- **#2 P2 Iteration**: Added `.take(MAX_ITERATION_COUNT)` to all 7 iteration sites (extract_manifest_dependencies, extract_conda_dependencies, extract_pypi_dependencies, extract_v6_lock_dependencies, collect_v6_package_refs, extract_v4_lock_dependencies, extract_authors).
- **#3 P2 String Length**: Applied `truncate_field()` to all extracted string values in PackageData, Dependency, Party, and FileReference fields.
- **#4 P4 UTF-8**: Already covered by `read_file_to_string` — lossy UTF-8 conversion is built in. No changes needed.
