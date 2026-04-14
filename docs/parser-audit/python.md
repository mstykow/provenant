# ADR 0004 Security Audit: python

**File**: `src/parsers/python.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

- setup.py parsed via `ruff_python_parser::parse_module` (AST only) at line 3315
- `LiteralEvaluator` evaluates only literal values (strings, numbers, lists, dicts) — no exec/eval at lines 2963-3086
- Regex fallback for large setup.py files at line 4104 (`extract_from_setup_py_regex`) — no code execution
- No `Command::new`, `subprocess`, `eval()`, or `exec()` anywhere in the file
- `collect_self_method_calls` in conan.rs traverses AST nodes recursively but never executes code

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

- `MAX_SETUP_PY_BYTES` = 1MB checked at line 3107 before AST parsing
- `MAX_ARCHIVE_SIZE` = 100MB for wheel/egg/sdist archives checked via `fs::metadata().len()` at lines 833, 1303, 1417
- `MAX_FILE_SIZE` = 50MB per archive entry at lines 951, 638, 1557
- `read_limited_utf8` enforces byte limits at line 1571
- **GAP**: No `fs::metadata().len()` check before reading plain text files (pyproject.toml, setup.cfg, PKG-INFO, METADATA, pypi.json). `read_file_to_string` at line 33 reads entire file without size check.

### Recursion Depth

- `MAX_SETUP_PY_AST_DEPTH` = 50 tracked in `LiteralEvaluator` at line 2978
- `SetupCallFinder` tracks `nodes_visited` against `MAX_SETUP_PY_AST_NODES` = 10,000 at lines 3504, 3567
- `dotted_name` respects depth limit at line 3601
- **GAP**: `collect_self_method_calls` (used by conan parser) recurses without depth tracking — but this is in conan.rs, not python.rs

### Iteration Count

- Archive entries iterated without 100K cap in `collect_validated_zip_entries` (line 613) and `collect_tar_sdist_entries` (line 938)
- `parse_record_csv` iterates CSV records without cap (line 1625)
- `parse_requires_txt` iterates lines without cap (line 4052)
- **GAP**: No 100,000 iteration limit on archive entries, CSV rows, or dependency lists

### String Length

- `read_limited_utf8` truncates at `MAX_FILE_SIZE` (50MB) for archive entries (line 1576)
- **GAP**: No 10MB per-field truncation for parsed string values (name, version, description, license, etc.)
- `content.len()` checked against `MAX_SETUP_PY_BYTES` (1MB) at line 3107 but individual field values are not truncated

## Principle 3: Archive Safety

**Status**: PASS

### Size Limits

- `MAX_ARCHIVE_SIZE` = 100MB total uncompressed checked at lines 647, 960 (exceeds ADR 0004's 1GB — more restrictive, acceptable)
- `MAX_FILE_SIZE` = 50MB per entry at lines 638, 951, 1557

### Compression Ratio

- `MAX_COMPRESSION_RATIO` = 100:1 enforced at lines 629, 969, 1549

### Path Traversal

- `normalize_archive_entry_path` at line 1592 blocks `..`, root dir, and drive letter prefixes
- Returns `None` for `Component::ParentDir` and `Component::RootDir` at line 1607

### Decompression Limits

- Total extracted size tracked and enforced via `total_extracted` at lines 610, 935

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

- Archive paths checked via `fs::metadata()` at lines 833, 1303, 1417
- **GAP**: Non-archive file reads use `read_file_to_string` which calls `File::open` — returns error but no pre-check with `fs::metadata()`
- `is_valid_wheel_archive_path` opens and validates at line 671

### UTF-8 Encoding

- `read_limited_utf8` at line 1571 uses `String::from_utf8` with error return (line 1589)
- **GAP**: No lossy fallback — returns error on invalid UTF-8 rather than logging warning and converting lossy
- `read_file_to_string` uses `file.read_to_string` which fails on invalid UTF-8 without lossy fallback

### JSON/YAML Validity

- `serde_json::from_str` failures return `default_package_data` at lines 382, 4182, 4400
- `read_toml_file` failures return error propagated to callers which return default at lines 2172, 61-68
- PASS — graceful degradation on parse failure

### Required Fields

- Missing `name`/`version` handled with `None` — continue parsing at lines 1819-1820, 2214-2221
- `apply_sdist_name_version_fallback` populates from archive filename if missing at line 1256
- PASS

### URL Format

- URLs accepted as-is from parsed content (no validation) — compliant with ADR 0004 ("Accept as-is")
- PASS

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution with circular dependency risk in this parser. The `requirements_txt` module handles circular includes separately.

## Additional Checks

### .unwrap() in Library Code

**Status**: FAIL

- Line 130: `path.file_name().unwrap_or_default()` — safe, uses `unwrap_or_default`
- Line 132: `path.file_name().unwrap_or_default()` — safe
- Line 136: `path.file_name().unwrap_or_default()` — safe
- Line 142: `path.file_name().unwrap_or_default()` — safe
- Line 349: `scope_from_filename` uses `unwrap_or_default()` — safe
- Line 611: `unwrap_or_default()` on `extract_lockfile_requirement` — safe
- Line 1632: `record.get(0).unwrap_or("")` — safe, `unwrap_or`
- Line 1637: `record.get(1).unwrap_or("")` — safe
- Line 1638: `record.get(2).unwrap_or("")` — safe
- Line 1834: `get_header_first(...).unwrap_or_default()` — safe
- Line 3999: `.expect("extra marker regex should compile")` — acceptable, compile-time invariant
- Line 5191: `.map_err(|e| e.to_string())` — safe
- No dangerous `.unwrap()` calls in library code

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle       | Severity | Line(s)              | Description                                                                            |
| --- | --------------- | -------- | -------------------- | -------------------------------------------------------------------------------------- |
| 1   | P2-FileSize     | MEDIUM   | 33, 37               | `read_file_to_string` reads entire file without size pre-check for non-archive formats |
| 2   | P2-Iteration    | LOW      | 613, 938, 1625, 4052 | No 100K iteration cap on archive entries, CSV records, or lines                        |
| 3   | P2-StringLength | LOW      | N/A                  | No 10MB per-field truncation for parsed string values                                  |
| 4   | P4-FileExists   | LOW      | 37                   | No `fs::metadata()` pre-check before reading non-archive files                         |
| 5   | P4-UTF8         | LOW      | 1589                 | `read_limited_utf8` returns error instead of lossy fallback on invalid UTF-8           |

## Remediation Priority

1. Add file size pre-check (`fs::metadata().len()` against 100MB limit) in `read_file_to_string` or before calling it for non-archive files
2. Add 100K iteration caps to archive entry loops, CSV parsing, and line-based parsers
3. Add 10MB string field truncation with warning logging for long parsed values
4. Add lossy UTF-8 fallback with warning in `read_limited_utf8`

## Remediation

**PR**: #664
**Date**: 2026-04-14

All findings addressed in ADR 0004 security compliance batch 1.
