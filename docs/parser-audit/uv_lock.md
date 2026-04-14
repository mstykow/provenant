# ADR 0004 Security Audit: uv_lock

**File**: `src/parsers/uv_lock.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

- TOML parsing via `read_toml_file` at line 69 — static deserialization
- No `Command::new`, `subprocess`, `eval()`, `exec()` anywhere

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

- No `fs::metadata().len()` check before `read_toml_file` at line 69
- Entire file read into memory without size limit

### Recursion Depth

- `toml_value_to_json` recursively converts TOML to JSON at line 894 — no depth limit
- **GAP**: No recursion depth limit on deeply nested TOML structures

### Iteration Count

- `package_tables` iteration at line 92 without cap
- `collect_reachable_packages` BFS at line 587 has no node count limit
- `collect_root_direct_dependencies` iterates dependency tables without cap at line 285
- `collect_package_dependency_edges` iterates without cap at line 419
- **GAP**: No 100,000 item cap on packages, edges, or BFS nodes

### String Length

- No 10MB per-field truncation for parsed values (name, version, specifier)

## Principle 3: Archive Safety

**Status**: N/A

Not an archive parser.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

- No `fs::metadata()` pre-check
- `read_toml_file` failure handled gracefully at lines 70-74

### UTF-8 Encoding

- `read_file_to_string` fails on invalid UTF-8 without lossy fallback
- **GAP**: No lossy UTF-8 conversion with warning

### JSON/YAML Validity

- TOML parse failure returns `default_package_data` at lines 71-74
- Empty packages array returns default at lines 88-90
- PASS — graceful degradation

### Required Fields

- Missing `name` returns `None` — dependency skipped via `filter_map` at line 156
- Missing `version` returns `None` — dependency skipped at line 186

### URL Format

- URLs accepted as-is — compliant with ADR 0004

## Principle 5: Circular Dependency Detection

**Status**: PASS

- `collect_reachable_packages` at line 587 uses `HashSet<String>` visited set
- BFS `visited.insert(package_name.clone())` at line 613 prevents revisiting
- Cycles broken explicitly

## Additional Checks

### .unwrap() in Library Code

**Status**: FAIL

- Line 86: `.unwrap_or_default()` — safe
- Line 228: `.unwrap_or_default()` — safe (twice, for name and version)
- Line 233: `.unwrap_or_default()` — safe
- Line 293: `.unwrap_or_default()` — safe
- Line 547: `.unwrap_or_default()` — safe
- Line 563: `.unwrap_or_default()` — safe
- Line 611: `.unwrap_or(name)` — safe
- No dangerous bare `.unwrap()` calls in library code

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle       | Severity | Line(s)      | Description                                                      |
| --- | --------------- | -------- | ------------ | ---------------------------------------------------------------- |
| 1   | P2-FileSize     | HIGH     | 69           | No file size check before `read_toml_file`                       |
| 2   | P2-Recursion    | MEDIUM   | 894          | `toml_value_to_json` recurses without depth limit on nested TOML |
| 3   | P2-Iteration    | LOW      | 92, 285, 419 | No 100K iteration cap on packages or dependency edges            |
| 4   | P2-StringLength | LOW      | N/A          | No 10MB per-field truncation                                     |
| 5   | P4-UTF8         | LOW      | N/A          | No lossy UTF-8 fallback                                          |

## Remediation Priority

1. Add `fs::metadata().len()` check (100MB limit) before reading uv.lock
2. Add recursion depth limit to `toml_value_to_json` (max 50)
3. Add 100K iteration caps on package/dependency iteration
4. Add lossy UTF-8 fallback with warning log
