# ADR 0004 Security Audit: conan

**File**: `src/parsers/conan.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

- `conanfile.py` parsed via `ruff_python_parser::parse_module` at line 62 — AST only
- `extract_conanfile_data` traverses AST nodes at line 97 — no code execution
- `collect_self_method_calls` recursively traverses AST at line 245 — no execution
- `conanfile.txt` parsed as plain text at line 336 — no code execution
- `conan.lock` parsed as JSON via `serde_json::from_str` at line 370 — no code execution
- No `Command::new`, `subprocess`, `eval()`, `exec()` anywhere

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

- No `fs::metadata().len()` check before `fs::read_to_string` at lines 54, 328, 362
- **GAP**: Entire files read into memory without size limit

### Recursion Depth

- `collect_self_method_calls` at line 245 recurses through AST nodes (if/with/while/for/try/match)
- **GAP**: No recursion depth limit — a deeply nested AST could cause stack overflow
- Unlike `python.rs` which has `MAX_SETUP_PY_AST_DEPTH` = 50 and `MAX_SETUP_PY_AST_NODES` = 10,000, this parser has no such limits

### Iteration Count

- `parse_conanfile_txt` iterates lines without cap at line 438
- `parse_conan_lock` iterates JSON object entries without cap at line 476
- `extract_conanfile_data` iterates class body statements without cap at line 109
- **GAP**: No 100,000 item cap on lines, dependencies, or JSON nodes

### String Length

- No 10MB per-field truncation for parsed values (name, version, license)

## Principle 3: Archive Safety

**Status**: N/A

Not an archive parser.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

- No `fs::metadata()` pre-check
- `fs::read_to_string` failures handled with `warn!` and default return at lines 56-58, 330-332, 364-366

### UTF-8 Encoding

- `fs::read_to_string` fails on invalid UTF-8 without lossy fallback
- **GAP**: No lossy UTF-8 conversion with warning

### JSON/YAML Validity

- `parse_module` failure returns `default_package_data` at lines 63-67
- `serde_json::from_str` failure returns `default_package_data` at lines 371-375
- PASS — graceful degradation

### Required Fields

- Missing name/version handled with `None` — continues parsing
- `parse_conan_reference` returns `None` for invalid references at line 390

### URL Format

- URLs accepted as-is — compliant with ADR 0004

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution with circular dependency risk.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

- No `.unwrap()` calls in library code
- All `Option`/`Result` values handled with `?`, `match`, or safe combinators

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle       | Severity | Line(s)       | Description                                                          |
| --- | --------------- | -------- | ------------- | -------------------------------------------------------------------- |
| 1   | P2-FileSize     | HIGH     | 54, 328, 362  | No file size check before `fs::read_to_string`                       |
| 2   | P2-Recursion    | HIGH     | 245           | `collect_self_method_calls` recurses through AST without depth limit |
| 3   | P2-Iteration    | LOW      | 109, 438, 476 | No 100K iteration cap on statements, lines, or JSON nodes            |
| 4   | P2-StringLength | LOW      | N/A           | No 10MB per-field truncation                                         |
| 5   | P4-UTF8         | LOW      | N/A           | No lossy UTF-8 fallback                                              |

## Remediation Priority

1. Add `fs::metadata().len()` check (100MB limit) before reading conanfile.py/conanfile.txt/conan.lock
2. Add AST node count and recursion depth limits to `collect_self_method_calls` (like python.rs: `MAX_SETUP_PY_AST_DEPTH`=50, `MAX_SETUP_PY_AST_NODES`=10,000)
3. Add 100K iteration caps on line/entry iteration
4. Add lossy UTF-8 fallback with warning log

## Remediation

- Finding #1 (P2 File Size): Replaced all 3 `fs::read_to_string` calls with `read_file_to_string(path, None)`
- Finding #2 (P2 Recursion): Added `MAX_AST_DEPTH = 50` and `MAX_AST_NODES = 10_000` to `collect_self_method_calls` with depth tracking and node count
- Finding #3 (P2 Iteration Count): Added `MAX_ITERATION_COUNT` caps to class body statements, conanfile.txt lines, and conan.lock JSON entries
- Finding #4 (P2 String Length): Applied `truncate_field()` to all extracted string values (name, version, description, author, homepage, url, license, topics, requires, purl, extracted_requirement)
- Finding #5 (P4 UTF-8): Fixed automatically by switching to `read_file_to_string`
