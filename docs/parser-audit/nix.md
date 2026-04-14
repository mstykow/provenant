# ADR 0004 Security Audit: nix

**File**: `src/parsers/nix.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `exec()`, `eval()`, or subprocess calls. The Nix parser implements a custom lexer/tokenizer and recursive-descent parser (lines 144-773) for static AST analysis. The `try_follow_local_nix_application` function (line 1541) reads local `.nix` files via `fs::read_to_string` and parses them statically — no execution occurs.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading. Files are read directly via `fs::read_to_string` at lines 23, 59, 84, and 1560 without size pre-check. A 5GB file would be fully loaded into memory.

### Recursion Depth

The parser has multiple recursive functions with varying depth limits:

- `extract_default_nix_package` (line 1424): depth limit of 2 (line 1430) — PASS
- `extract_flake_compat_package_from_expr` (line 1585): depth limit of 2 (line 1591) — PASS
- `resolve_flake_compat_source_root` (line 1668): depth limit of 8 (line 1674) — PASS
- `resolve_symbol` (line 1336): depth limit of 8 (line 1341) — PASS
- `resolve_select` (line 1360): depth limit of 8 (line 1366) — PASS
- **`parse_expr`** (line 450): **NO depth limit** — calls `parse_term` which calls `parse_expr` recursively for lambda bodies, `with` expressions, and parenthesized expressions
- **`parse_let_in_expr`** (line 529): **NO depth limit** — recursive via `parse_expr`
- **`parse_attrset`** (line 557): **NO depth limit** — recursive via `parse_expr`
- **`parse_list`** (line 622): **NO depth limit** — recursive via `parse_expr`
- **`expr_to_dependency_symbols_with_scopes`** (line 1117): **NO depth limit** — recursive on `Expr::Symbol` via `resolve_symbol` then back to itself
- **`expr_as_symbol_with_scopes`** (line 1192): **NO depth limit** — recursive on resolved symbols
- **`expr_as_bool_with_scopes`** (line 1215): **NO depth limit** — recursive
- **`expr_as_string_with_scopes`** (line 1230): **NO depth limit** — recursive
- **`list_items_with_scopes`** (line 1167): **NO depth limit** — recursive
- **`interpolate_string`** (line 1390): **NO depth limit** on `${...}` interpolation chains
- **`root_attrset_with_scopes`** (line 1315): **NO depth limit** — recursive on `Expr::Let`
- **`find_attr`** (line 1261): **NO depth limit** — recursive on nested `AttrSet`

### Iteration Count

- `Lexer::tokenize` (line 157): No cap on number of tokens produced from input
- `parse_let_in_expr` (line 529): No cap on number of bindings
- `parse_attrset` (line 557): No cap on number of entries
- `parse_list` (line 622): No cap on number of items
- `parse_inherit_entries` (line 591): No cap on number of entries
- `root_inputs.iter()` (line 836): No 100K cap on dependencies
- `build_list_dependencies` (line 1084): No 100K cap on dependencies

### String Length

No field-level truncation at 10MB. String values extracted from the AST are used as-is without length checks.

## Principle 3: Archive Safety

**Status**: N/A

Nix files are not archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `fs::read_to_string` is called directly (lines 23, 59, 84). Errors are handled gracefully via `match` and logged with `warn!`, returning default `PackageData`. — Acceptable fallback.

### UTF-8 Encoding

`fs::read_to_string` will fail on non-UTF-8 content, which is handled by returning default data. However, there is no lossy conversion fallback (no `String::from_utf8_lossy`). — Minor gap.

### JSON/YAML Validity

`serde_json::from_str` errors at line 31 are handled by returning `default_flake_lock_package_data()`. — PASS

### Required Fields

Missing name/version fields result in `None` values with `fallback_name(path)` as fallback (lines 782-783, 1530-1531). — PASS

### URL Format

URLs are accepted as-is without validation. — Per ADR, acceptable.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution with circular detection. The Nix parser extracts declared dependencies, not resolved dependency graphs.

## Additional Checks

### .unwrap() in Library Code

**Status**: FAIL

- Line 255: `terms.pop().expect("single term")` — `.expect()` in library code
- Line 475: `terms.pop().expect("single term")` — `.expect()` in library code
- Line 750: `extract_local_flake_compat_src_value(content).unwrap_or("./.".to_string())` — `.unwrap_or()` is safe, not a concern
- Line 754: `conda_name_from_locator` uses `.unwrap_or(file_name)` — safe

### Command::new / Subprocess Usage

**Status**: PASS

No subprocess calls found.

## Findings Summary

| #   | Principle        | Severity | Line(s)                                  | Description                                                                                                                |
| --- | ---------------- | -------- | ---------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| 1   | P2 Recursion     | HIGH     | 450, 529, 557, 622                       | No depth tracking in Nix expression parser — deeply nested input causes stack overflow                                     |
| 2   | P2 Recursion     | MEDIUM   | 1117, 1192, 1215, 1230, 1167, 1261, 1315 | Recursive AST evaluation functions have no depth limit (resolve_symbol/resolve_select are capped at 8, but callers aren't) |
| 3   | P2 File Size     | MEDIUM   | 23, 59, 84, 1560                         | No file size check before reading — oversized files loaded entirely into memory                                            |
| 4   | P2 Iteration     | MEDIUM   | 157, 529, 557, 622, 836                  | No iteration caps on tokens, bindings, entries, or dependencies                                                            |
| 5   | P2 String Length | LOW      | —                                        | No field-level 10MB truncation                                                                                             |
| 6   | P4 UTF-8         | LOW      | 23, 59, 84                               | No lossy UTF-8 fallback on non-UTF-8 content                                                                               |
| 7   | Additional       | LOW      | 475                                      | `.expect("single term")` in library code                                                                                   |

## Remediation Priority

1. Add recursion depth tracking (max 50) to `parse_expr`/`parse_term` and all recursive AST evaluation functions
2. Add `fs::metadata().len()` check before reading files, reject >100MB
3. Add iteration caps (100K) on token count, binding count, dependency count
