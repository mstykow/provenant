# ADR 0004 Security Audit: cran

**File**: `src/parsers/cran.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No code execution mechanisms. Uses custom DCF (Debian Control File) text parsing (`parse_dcf` at line 181), `regex::Regex` for version constraint parsing (line 290-291), and `packageurl::PackageUrl` for PURL generation. Fully static analysis.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `File::open(path)` at line 166 then `file.read_to_string()` at line 169 — no size pre-check.

### Recursion Depth

No recursive functions. All parsing is iterative.

### Iteration Count

- `parse_dcf` (line 186): `for line in content.lines()` — no cap on line count
- `parse_dependencies` (line 225): `for dep in deps_str.split(',')` — no cap on dependency count
- `split_author_entries` (line 337): iterates over characters — no cap
- No 100K iteration cap anywhere

### String Length

No field value truncation at 10MB. String values from DCF fields stored without length checks.

## Principle 3: Archive Safety

**Status**: N/A

No archive extraction performed.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. `File::open(path)` at line 166 returns an error on failure, handled correctly with `warn!()` and default return at lines 52-56.

### UTF-8 Encoding

Uses `file.read_to_string()` at line 169 which fails on invalid UTF-8 without lossy fallback.

### JSON/YAML Validity

No JSON/YAML parsing. DCF format is custom text. **N/A**

### Required Fields

Missing `name`/`version` handled via `Option<String>`. Correct.

### URL Format

URLs accepted as-is. ADR-compliant.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution.

## Additional Checks

### .unwrap() in Library Code

**Status**: FAIL

- Line 291: `Regex::new(r"...").unwrap()` in `lazy_static!` — compile-time constant, acceptable.
- Line 300: `captures.get(1).unwrap().as_str()` — this is after `VERSION_CONSTRAINT_RE.captures(dep)` succeeded, so group 1 is guaranteed to exist. However, this is a raw `.unwrap()` without a safer alternative. Low risk but violates the rule.
- Line 301: `captures.get(2).unwrap().as_str()` — same pattern as above.
- Line 302: `captures.get(3).unwrap().as_str()` — same pattern as above.

### Command::new / Subprocess Usage

**Status**: PASS

None.

## Findings Summary

| #   | Principle             | Severity | Line(s)       | Description                                                                                       |
| --- | --------------------- | -------- | ------------- | ------------------------------------------------------------------------------------------------- |
| 1   | P2: File Size         | HIGH     | 166-170       | No `fs::metadata().len()` check before reading; unbounded file read                               |
| 2   | P2: Iteration Count   | MEDIUM   | 186, 225, 337 | No 100K iteration cap on line/dependency/character processing loops                               |
| 3   | P2: String Length     | LOW      | various       | No 10MB field value truncation                                                                    |
| 4   | P4: UTF-8 Encoding    | MEDIUM   | 169           | No lossy UTF-8 fallback on read failure                                                           |
| 5   | Additional: .unwrap() | LOW      | 300-302       | `.unwrap()` on regex capture groups after successful match — low risk but violates no-unwrap rule |

## Remediation Priority

1. Add `fs::metadata().len()` check before reading (100MB default limit)
2. Add 100K iteration cap in line/dependency processing loops
3. Add lossy UTF-8 fallback on read failure
4. Replace `.unwrap()` on regex captures with safer alternatives (e.g., `expect()` with message or `if let`)
5. Add 10MB field value truncation with warning
