# ADR 0004 Security Audit: requirements_txt

**File**: `src/parsers/requirements_txt.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

- Pure string/regex parsing via `parse_pep508_requirement` at line 524
- No `Command::new`, `subprocess`, `eval()`, `exec()` anywhere
- No dynamic code execution of any kind

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

- No `fs::metadata().len()` check before `fs::read_to_string` at line 193
- Entire file read into memory without size limit

### Recursion Depth

- `parse_requirements_with_includes` recurses for `-r` and `-c` includes at lines 228, 245
- Circular include detection via `state.visited` HashSet at lines 186-189 prevents infinite recursion
- **GAP**: No explicit recursion depth limit (e.g., 50 levels). Malicious file chain could create deep recursion before hitting the visited-set check, though cycles are broken. Non-circular deep chains (A includes B includes C ...) have no depth cap.

### Iteration Count

- `collect_logical_lines` iterates all lines without cap at line 283
- `for line in collect_logical_lines(&content)` at line 201 has no 100K iteration limit
- **GAP**: No 100,000 item cap on lines or dependencies

### String Length

- No 10MB per-field truncation for parsed requirement strings
- `trimmed` at line 203 and dependency lines are used as-is

## Principle 3: Archive Safety

**Status**: N/A

Not an archive parser.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

- `path.canonicalize()` at line 178 returns early on error — no explicit `fs::metadata()` pre-check
- `included_path.exists()` checked at lines 227, 244 before processing includes

### UTF-8 Encoding

- `fs::read_to_string` at line 193 — will fail on invalid UTF-8 without lossy fallback
- **GAP**: No lossy UTF-8 conversion with warning

### JSON/YAML Validity

- N/A — no JSON/YAML parsing in this module

### Required Fields

- Dependencies without names handled gracefully — `parse_pep508_requirement` returns `None` for invalid specs
- `build_dependency` returns `None` for empty/invalid input at line 367

### URL Format

- URLs accepted as-is — compliant with ADR 0004

## Principle 5: Circular Dependency Detection

**Status**: PASS

- `ParseState.visited` HashSet at line 111 tracks visited file paths
- Circular include detected and logged at lines 186-189
- Path canonicalized at line 178 for consistent comparison

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

- Line 224: `unwrap_or_else(|| Path::new("."))` — safe, `unwrap_or_else`
- Line 349: `unwrap_or_default()` — safe
- Line 611: `unwrap_or_else(|| (name_part.to_string(), Vec::new(), None))` — safe
- No dangerous `.unwrap()` calls in library code

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle       | Severity | Line(s)  | Description                                                                    |
| --- | --------------- | -------- | -------- | ------------------------------------------------------------------------------ |
| 1   | P2-FileSize     | HIGH     | 193      | No file size check before `fs::read_to_string` — unbounded memory read         |
| 2   | P2-Recursion    | MEDIUM   | 228, 245 | No explicit recursion depth limit for include chains (only circular detection) |
| 3   | P2-Iteration    | LOW      | 201, 283 | No 100K iteration cap on lines or dependencies                                 |
| 4   | P2-StringLength | LOW      | N/A      | No 10MB per-field truncation for requirement strings                           |
| 5   | P4-UTF8         | LOW      | 193      | No lossy UTF-8 fallback on `fs::read_to_string` failure                        |

## Remediation Priority

1. Add `fs::metadata().len()` check (100MB limit) before reading requirements.txt files
2. Add explicit recursion depth counter (max 50) to `parse_requirements_with_includes`
3. Add 100K iteration cap on lines/dependencies with early break and warning
4. Add lossy UTF-8 fallback with warning log
