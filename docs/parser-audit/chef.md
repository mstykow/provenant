# ADR 0004 Security Audit: chef

**File**: `src/parsers/chef.rs`
**Date**: 2026-04-14
**Status**: NON-COMPLIANT

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `exec()`, `eval()`, or subprocess calls. The Ruby DSL parser (ChefMetadataRbParser) uses regex-based line extraction (line 233-235) rather than executing Ruby code. `IO.read(...)` expressions are explicitly skipped (line 236, 250). Uses `serde_json` for static JSON parsing.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading.

- JSON parser: `File::open` + `read_to_string` at lines 179-183
- Ruby parser: `File::open` at line 221 with `BufReader` — reads line by line, which is more memory-efficient but still no size cap

### Recursion Depth

No recursive functions. All processing is iterative. — PASS

### Iteration Count

- Ruby parser `for line in reader.lines()` (line 238): No 100K cap on lines processed
- JSON parser `deps_obj` iteration (lines 144-150): No 100K cap on dependencies
- JSON parser `depends_obj` iteration (lines 153-159): No 100K cap

### String Length

No field-level truncation at 10MB.

## Principle 3: Archive Safety

**Status**: N/A

JSON/Ruby text files are not archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `File::open` fails on missing files, errors handled via `match` (line 221 for Ruby, line 179 for JSON). — Acceptable.

### UTF-8 Encoding

- JSON parser: `file.read_to_string` at line 182 will fail on non-UTF-8. No lossy conversion.
- Ruby parser: `BufReader::lines()` at line 238 will produce errors on non-UTF-8 lines, which are skipped via `Err(_) => continue` (line 241). This silently drops non-UTF-8 lines without logging a warning. — Should log warning per ADR.

### JSON/YAML Validity

JSON parse error at line 183 is handled, returns error string caught in `extract_packages` (line 89). — PASS

### Required Fields

Missing name/version result in `None` values with filtering for empty strings (lines 94-98, 100-104). — PASS

### URL Format

URLs accepted as-is. — Per ADR, acceptable.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: FAIL

- Line 233: `Regex::new(r#"^\s*(\w+)\s+['"](.+?)['"]"#).unwrap()` — `.unwrap()` on regex compilation in library code. This is a static regex that will always compile, but violates the ADR rule.
- Line 235: `Regex::new(r#"^\s*depends\s+['"](.+?)['"](?:\s*,\s*['"](.+?)['"])?"#).unwrap()` — same issue
- Line 236: `Regex::new(r"IO\.read\(").unwrap()` — same issue
- Line 255: `caps.get(1).map(|m| m.as_str().to_string()).unwrap()` — `.unwrap()` on regex capture group that is guaranteed to exist by the pattern, but still `.unwrap()` in library code
- Line 262: Same `.unwrap()` pattern for regex captures
- Line 263: Same `.unwrap()` pattern for regex captures

### Command::new / Subprocess Usage

**Status**: PASS

No subprocess calls found.

## Findings Summary

| #   | Principle        | Severity | Line(s)       | Description                                                                                            |
| --- | ---------------- | -------- | ------------- | ------------------------------------------------------------------------------------------------------ |
| 1   | Additional       | MEDIUM   | 233, 235, 236 | `.unwrap()` on `Regex::new()` in library code — should use `expect()` with justification or `LazyLock` |
| 2   | Additional       | MEDIUM   | 255, 262, 263 | `.unwrap()` on regex capture groups in library code — should use `?` or `expect()`                     |
| 3   | P2 File Size     | MEDIUM   | 179-183, 221  | No file size check before reading                                                                      |
| 4   | P4 UTF-8         | LOW      | 241           | Non-UTF-8 lines silently skipped without logging warning                                               |
| 5   | P2 String Length | LOW      | —             | No field-level 10MB truncation                                                                         |

## Remediation Priority

1. Replace `Regex::new(...).unwrap()` with `LazyLock` + `expect("documented reason")` or use `once_cell::sync::Lazy`
2. Replace `.unwrap()` on regex captures with `?` operator or proper error handling
3. Add `fs::metadata().len()` check before reading, reject >100MB
4. Log warning when non-UTF-8 lines are skipped in Ruby parser
