# ADR 0004 Security Audit: cpan_makefile_pl

**File**: `src/parsers/cpan_makefile_pl.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No code execution mechanisms. Uses regex-based extraction from Perl source code (`RE_WRITEMAKEFILE`, `RE_SIMPLE_KV`, etc. at lines 33-53) — no Perl interpreter or eval is used. The regex patterns extract key-value pairs from `WriteMakefile()` calls without executing any Perl code. Fully static analysis. This is explicitly noted in the file's documentation (line 13): "Uses regex-based extraction (no Perl code execution for security)".

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

- **Main file**: No `fs::metadata().len()` check before reading `Makefile.PL`. `fs::read_to_string(path)` at line 68 — no size pre-check. **FAIL**
- **Referenced metadata files**: `read_safe_metadata_file` at line 241 DOES check `fs::metadata(&canonical_candidate).len()` at line 254, and enforces `MAX_METADATA_FILE_SIZE` (1MB, line 56) at line 255. **PASS** for referenced files only.

### Recursion Depth

No recursive functions. All parsing is iterative.

### Iteration Count

- `extract_writemakefile_block` (line 300): iterates over `chars` with no cap — a very long line could iterate millions of characters
- `parse_hash_fields` (line 335): `RE_SIMPLE_KV.captures_iter(content)` — no cap on number of captures
- `extract_deps_from_hash` (line 490): `RE_DEP_PAIR.captures_iter(hash_content)` — no cap on dependency count
- No 100K iteration cap anywhere

### String Length

No field value truncation at 10MB. String values from regex captures stored without length checks.

## Principle 3: Archive Safety

**Status**: N/A

No archive extraction performed.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

- **Main file**: No `fs::metadata()` pre-check. `fs::read_to_string(path)` at line 68 returns an error, handled at lines 69-78. **PARTIAL**
- **Referenced files**: `read_safe_metadata_file` at line 241 performs comprehensive path traversal protection:
  - Rejects absolute paths (line 243)
  - Canonicalizes base directory (line 247)
  - Canonicalizes candidate path (line 249)
  - Verifies candidate starts with base directory (line 250)
  - Checks `is_file()` and `len() <= MAX_METADATA_FILE_SIZE` (lines 254-255)
    **PASS** for referenced files — excellent path traversal protection.

### UTF-8 Encoding

Uses `fs::read_to_string()` which fails on invalid UTF-8 without lossy fallback. No lossy conversion for either main file or referenced metadata files.

### JSON/YAML Validity

No JSON/YAML parsing. Makefile.PL format is custom text parsed via regex. **N/A**

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

- Line 34: `Regex::new(r"...").unwrap()` in `LazyLock` — compile-time constant, acceptable.
- Lines 36-37: `Regex::new(r"...").unwrap()` in `LazyLock` — compile-time constant, acceptable.
- Lines 40-47: `Regex::new(r"...").unwrap()` in `LazyLock` — compile-time constant, acceptable.
- Line 339: `cap.get(1).expect("group 1 always exists")` — uses `expect()` with justification. Technically `.expect()` is equivalent to `.unwrap()` with a message. Low risk since the regex guarantees the group exists.
- Line 341: `cap.get(2).or_else(|| cap.get(3)).or_else(|| cap.get(4)).or_else(|| cap.get(5))` — safe fallback chain.
- Line 367: `cap.get(1).expect("group 1 always exists")` — same pattern.
- Line 368: `cap.get(2).expect("group 2 always exists")` — same pattern.
- Line 383: `cap.get(1).expect("group 1 always exists")` — same pattern.
- Line 494: `cap.get(1).expect("group 1 always exists")` — same pattern.

The `expect()` calls are on regex capture groups after successful match where the group is guaranteed to exist by the regex pattern. Low risk but technically uses panic-on-failure semantics.

### Command::new / Subprocess Usage

**Status**: PASS

None.

## Findings Summary

| #   | Principle             | Severity | Line(s)                 | Description                                                                        |
| --- | --------------------- | -------- | ----------------------- | ---------------------------------------------------------------------------------- |
| 1   | P2: File Size         | HIGH     | 68                      | No `fs::metadata().len()` check before reading Makefile.PL; unbounded file read    |
| 2   | P2: Iteration Count   | MEDIUM   | 300, 335, 490           | No 100K iteration cap on character/capture/dependency loops                        |
| 3   | P2: String Length     | LOW      | various                 | No 10MB field value truncation                                                     |
| 4   | P4: UTF-8 Encoding    | MEDIUM   | 68                      | No lossy UTF-8 fallback on read failure                                            |
| 5   | Additional: .expect() | LOW      | 339, 367, 368, 383, 494 | `.expect()` on regex capture groups — low risk but uses panic-on-failure semantics |

## Remediation Priority

1. Add `fs::metadata().len()` check before reading Makefile.PL (100MB default limit, similar to `MAX_METADATA_FILE_SIZE` pattern already used for referenced files)
2. Add 100K iteration cap in character/capture processing loops
3. Add lossy UTF-8 fallback on read failure
4. Add 10MB field value truncation with warning
5. Consider replacing `.expect()` with safer alternatives or documenting why panic is acceptable
