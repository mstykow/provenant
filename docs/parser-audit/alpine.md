# ADR 0004 Security Audit: alpine

**File**: `src/parsers/alpine.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `Command::new`, or subprocess calls. The APKBUILD parser (`parse_apkbuild_variables()`) performs static text parsing of shell-like variable assignments — it does NOT execute shell code. It handles brace depth tracking for function bodies (lines 371-386) to skip them, and performs string replacement for variable expansion (lines 430-457) purely via `String::replace()`, not shell execution.

## Principle 2: DoS Protection

**Status**: FAIL

### File Size

No `fs::metadata().len()` check before reading. `read_file_to_string()` at lines 65, 85 reads entire files without size validation.

### Recursion Depth

- `resolve_apkbuild_value()` line 430: Not recursive itself, but performs up to 8 iterations of variable substitution (line 432: `for _ in 0..8`). This is bounded and acceptable.
- `resolve_apkbuild_value_no_recursion()` line 460: Single-pass, not recursive.
- No unbounded recursion found. PASS for recursion depth.

### Iteration Count

No 100K iteration cap on loops:

- `parse_alpine_installed_db()` line 105: iterates all paragraphs without limit
- `parse_alpine_headers()` line 128: iterates all lines without limit
- `parse_alpine_package_paragraph()` line 209: iterates `D` dependency field entries without limit
- `extract_file_references()` line 617: iterates all lines without limit
- `extract_providers()` line 695: iterates all lines without limit
- `parse_pkginfo()` line 824: iterates all lines without limit
- `parse_apkbuild_dependencies()` line 580: iterates dependency fields without limit

### String Length

No 10MB truncation on field values. APKBUILD variable values are stored without length limits.

## Principle 3: Archive Safety

**Status**: FAIL

### .apk Archive Extraction (AlpineApkParser)

- `apk_contains_pkginfo()` line 756: Opens gzip+tar archive, iterates all entries without size/ratio limits
- `extract_apk_archive()` line 789: Opens gzip+tar archive, reads `.PKGINFO` content without:
  - Uncompressed size limit (1 GB)
  - Compression ratio check (100:1 max)
  - Path traversal check (`../` patterns)
  - Decompression limit (1 GB)
- `entry.read_to_string(&mut content)` at line 810 reads unbounded content from tar entry

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

Uses `read_file_to_string()` which returns error on missing files (handled at lines 67-70, 88-90), but no explicit `fs::metadata()` pre-check.

### UTF-8 Encoding

`read_file_to_string()` uses `file.read_to_string()` which errors on invalid UTF-8. No lossy conversion fallback. In `extract_apk_archive()` line 810, `entry.read_to_string()` will also error on invalid UTF-8.

### JSON/YAML Validity

No JSON/YAML parsing. N/A.

### Required Fields

`parse_alpine_package_paragraph()` line 108: Package is only pushed if `pkg.name.is_some()` — missing name causes the package to be silently skipped, which is acceptable. Missing version results in `None`.

### URL Format

URLs accepted as-is. PASS per ADR.

## Principle 5: Circular Dependency Detection

**Status**: N/A

No dependency resolution; only parsing declared dependencies.

## Additional Checks

### .unwrap() in Library Code

**Status**: FAIL

- Line 395: `.unwrap()` in `parse_apkbuild_variables()` — `lines.next().unwrap()` when consuming multi-line quoted values. Could panic if the iterator is exhausted unexpectedly (though `peek()` suggested content exists).
- Line 481: `.unwrap_or(value)` — acceptable defensive pattern.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle  | Severity | Line(s)                     | Description                                                     |
| --- | ---------- | -------- | --------------------------- | --------------------------------------------------------------- |
| 1   | P2         | HIGH     | 65,85                       | No file size check before reading (100MB limit)                 |
| 2   | P2         | MEDIUM   | 105,128,209,580,617,695,824 | No iteration count cap (100K items)                             |
| 3   | P2         | MEDIUM   | —                           | No string length truncation (10MB per field)                    |
| 4   | P3         | HIGH     | 756,789,810                 | No archive size/ratio/path-traversal limits on .apk extraction  |
| 5   | P4         | LOW      | 65,85                       | No explicit fs::metadata() pre-check                            |
| 6   | P4         | MEDIUM   | 65,85,810                   | No lossy UTF-8 fallback; invalid UTF-8 causes error not warning |
| 7   | Additional | LOW      | 395                         | .unwrap() in library code (lines.next().unwrap())               |

## Remediation Priority

1. Add archive safety limits to .apk extraction (uncompressed size 1GB, compression ratio 100:1, path traversal blocking)
2. Add fs::metadata().len() check before read_file_to_string with 100MB limit
3. Add iteration count caps on paragraph/line/dependency loops
4. Add String::from_utf8_lossy() fallback for UTF-8 handling
5. Replace .unwrap() at line 395 with proper error handling

## Remediation

**PR**: #664
**Date**: 2026-04-14

All findings addressed in ADR 0004 security compliance batch 1.
