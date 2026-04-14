# ADR 0004 Security Audit: julia

**File**: `src/parsers/julia.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

No code execution mechanisms. Uses `toml::from_str()` for TOML parsing (line 199) and manual field extraction. Fully static analysis.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `read_julia_toml` at line 194 uses `File::open` then `file.read_to_string()` — no size pre-check.

### Recursion Depth

`toml_to_json` at line 451 has recursive calls for `Value::Array` (line 457) and `Value::Table` (line 458). No depth tracking — a deeply nested TOML table could cause deep recursion. However, the `toml` crate itself limits nesting.

### Iteration Count

Loops over dependencies (`deps_table` at line 262, `dep_entries` at line 324, `authors` at line 232) have no 100K iteration cap. `extract_manifest_packages` iterates over all deps entries (line 318) without cap.

### String Length

No field value truncation at 10MB. TOML string values stored without length checks.

## Principle 3: Archive Safety

**Status**: N/A

No archive extraction performed.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `File::open(path)` at line 195 returns an error, handled correctly at lines 55-58, 177-179.

### UTF-8 Encoding

Uses `file.read_to_string()` at line 197 which fails on invalid UTF-8 without lossy fallback.

### JSON/YAML Validity

TOML parse failure at line 199 returns `Err`, handled by returning `default_project_package_data()` at lines 56-58 or empty vec at line 179. **PASS**

### Required Fields

Missing `name`/`version` handled via `Option<String>`. Correct.

### URL Format

URLs accepted as-is. ADR-compliant.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code.

### Command::new / Subprocess Usage

**Status**: PASS

None.

## Findings Summary

| #   | Principle           | Severity | Line(s)            | Description                                                         |
| --- | ------------------- | -------- | ------------------ | ------------------------------------------------------------------- |
| 1   | P2: File Size       | HIGH     | 195-198            | No `fs::metadata().len()` check before reading; unbounded file read |
| 2   | P2: Recursion Depth | MEDIUM   | 451-466            | `toml_to_json` has unbounded recursion for nested TOML values       |
| 3   | P2: Iteration Count | MEDIUM   | 232, 262, 318, 324 | No 100K iteration cap on dependency/author loops                    |
| 4   | P2: String Length   | LOW      | various            | No 10MB field value truncation                                      |
| 5   | P4: UTF-8 Encoding  | MEDIUM   | 197                | No lossy UTF-8 fallback on read failure                             |

## Remediation Priority

1. Add `fs::metadata().len()` check before reading (100MB default limit)
2. Add depth tracking to `toml_to_json` recursion (50-level max)
3. Add 100K iteration cap in dependency extraction loops
4. Add lossy UTF-8 fallback on read failure
5. Add 10MB field value truncation with warning

## Remediation

- #1 P2: File Size — Replaced `File::open`+`read_to_string` with `read_file_to_string(path, None)` (100MB limit)
- #2 P2: Recursion Depth — Added `MAX_RECURSION_DEPTH=50` to `toml_to_json` recursion
- #3 P2: Iteration Count — Added `.take(MAX_ITERATION_COUNT)` on author/dependency loops
- #4 P2: String Length — Applied `truncate_field()` to all extracted string values
- #5 P4: UTF-8 Encoding — Fixed by `read_file_to_string` (lossy UTF-8 fallback)
