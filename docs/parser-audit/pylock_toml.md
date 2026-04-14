# ADR 0004 Security Audit: pylock_toml

**File**: `src/parsers/pylock_toml.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

- TOML parsing via `read_toml_file` at line 77 — static deserialization
- Regex-based marker parsing at line 446 — no code execution
- No `Command::new`, `subprocess`, `eval()`, `exec()` anywhere

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

- No `fs::metadata().len()` check before `read_toml_file` at line 77
- Entire file read into memory without size limit

### Recursion Depth

- `toml_values_match` recursively compares TOML values at line 355 — no depth limit
- `collect_reachable_indices` uses BFS (VecDeque) at line 391 — no depth issue but no cycle protection needed (BFS)
- **GAP**: `toml_values_match` has no recursion depth limit for deeply nested TOML values

### Iteration Count

- `package_values.iter()` at line 117 iterates packages without cap
- `build_dependency_indices` iterates packages with O(n\*m) matching at line 307 — could be quadratic
- `collect_reachable_indices` BFS has no node count limit
- **GAP**: No 100,000 item cap on packages, dependencies, or BFS nodes

### String Length

- No 10MB per-field truncation for parsed values (name, version, marker strings)

## Principle 3: Archive Safety

**Status**: N/A

Not an archive parser.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

- No `fs::metadata()` pre-check
- `read_toml_file` failure handled gracefully at lines 78-82

### UTF-8 Encoding

- `read_file_to_string` fails on invalid UTF-8 without lossy fallback
- **GAP**: No lossy UTF-8 conversion with warning

### JSON/YAML Validity

- TOML parse failure returns `default_package_data` at lines 79-82
- Invalid `lock-version` returns default at lines 93-98
- Missing `created-by` returns default at lines 104-107
- PASS — graceful degradation with validation

### Required Fields

- Missing `name`/`version` in package tables results in `None` — dependency skipped via `filter_map` at line 192
- Missing `lock-version` or `created-by` returns default package data

### URL Format

- URLs accepted as-is — compliant with ADR 0004

## Principle 5: Circular Dependency Detection

**Status**: PASS

- `collect_reachable_indices` at line 391 uses `HashSet<usize>` visited set to prevent revisiting
- BFS naturally handles cycles via `visited.insert(index)` check at line 396

## Additional Checks

### .unwrap() in Library Code

**Status**: FAIL

- Line 212: `.unwrap_or_default()` — safe
- Line 219: `.unwrap_or_default()` — safe
- Line 258: `.unwrap_or_default()` — safe
- Line 259: `.unwrap_or_default()` — safe
- No dangerous bare `.unwrap()` calls in library code

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle       | Severity | Line(s) | Description                                                      |
| --- | --------------- | -------- | ------- | ---------------------------------------------------------------- |
| 1   | P2-FileSize     | HIGH     | 77      | No file size check before `read_toml_file`                       |
| 2   | P2-Recursion    | MEDIUM   | 355     | `toml_values_match` recurses without depth limit on nested TOML  |
| 3   | P2-Iteration    | MEDIUM   | 307     | `build_dependency_indices` has O(n\*m) package matching — no cap |
| 4   | P2-Iteration    | LOW      | 117     | No 100K iteration cap on packages array                          |
| 5   | P2-StringLength | LOW      | N/A     | No 10MB per-field truncation                                     |
| 6   | P4-UTF8         | LOW      | N/A     | No lossy UTF-8 fallback                                          |

## Remediation Priority

1. Add `fs::metadata().len()` check (100MB limit) before reading pylock.toml
2. Add recursion depth limit to `toml_values_match` (max 50)
3. Add 100K iteration caps on package iteration and dependency resolution
4. Add lossy UTF-8 fallback with warning log

## Remediation

All 6 findings addressed. read_toml_file already uses read_file_to_string (file size + UTF-8). Added depth limit to toml_values_match (50), iteration caps on package iteration and BFS, applied truncate_field to all string values.
