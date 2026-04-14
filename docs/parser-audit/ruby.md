# ADR 0004 Security Audit: ruby

**File**: `src/parsers/ruby.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `subprocess`, `eval()`, `exec()`, or any code execution primitives found. All parsing uses regex-based static analysis (`Regex::new`, `captures_iter`, `captures`) and YAML/structured data parsing via `yaml_serde`. Ruby DSL constructs (Gemfile, gemspec) are parsed via regex tokenization, not Ruby AST or runtime evaluation.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

- **GemfileParser** (line 82): Uses `fs::read_to_string(path)` directly with **no** `fs::metadata().len()` pre-check.
- **GemfileLockParser** (line 407): Uses `fs::read_to_string(path)` directly with **no** file size pre-check.
- **GemspecParser** (line 967): Uses `fs::read_to_string(path)` directly with **no** file size pre-check.
- **GemArchiveParser** (line 1564-1566): **PASS** — Checks `fs::metadata(path).len()` before reading, enforces `MAX_ARCHIVE_SIZE` (100MB, line 1531).
- **GemMetadataExtractedParser** (line 1941): Uses `fs::read_to_string(path)` with **no** file size pre-check.

### Recursion Depth

- No recursive parsing functions found. The `block_stack` (line 108) and state machine (lines 466-667) are iterative, not recursive. The `load_required_ruby_contexts` function (line 1183) follows `require` statements but does so iteratively with a single level of follow — no depth tracking. However, there is no cycle detection: if a required file itself requires another file, `load_required_ruby_contexts` does not recurse (it only scans the original content for `require` statements), so this is bounded. **PASS** for recursion, but see Principle 5.

### Iteration Count

- `parse_gemfile` (line 166): Iterates `content.lines()` with **no** 100K cap on iterations or dependency count.
- `parse_gemfile_lock` (line 512): Iterates `content.lines()` with **no** 100K cap.
- `parse_gemspec_with_context` (line 1294): Iterates `field_re.captures_iter(content)` with **no** 100K cap.
- `extract_gem_archive` (line 1578): Iterates tar entries with **no** 100K cap on number of entries.
- `parse_gem_yaml_dependencies` (line 1847): Iterates dependency sequence with **no** 100K cap.

### String Length

- No field values are truncated at 10MB. `clean_gemspec_value` (line 1019) and other extraction functions return full strings regardless of length.
- `decoder.read_to_string(&mut content)` (line 1599) reads full decompressed metadata into a single `String` with only a 50MB cap on decompressed size (line 1612), which exceeds the 10MB per-field ADR limit.

## Principle 3: Archive Safety

**Status**: PARTIAL

### Size Limits

- `MAX_ARCHIVE_SIZE` = 100MB (line 1531): Checks compressed archive size. **PASS** for archive-level, but ADR requires 1GB uncompressed limit — no explicit uncompressed total check for the full tar archive (only `metadata.gz` entry is size-checked at 50MB, line 1532).
- `MAX_FILE_SIZE` = 50MB (line 1532): Checks individual entry size. This is reasonable but does not match ADR's 1GB uncompressed limit.

### Compression Ratio

- `MAX_COMPRESSION_RATIO` = 100.0 (line 1533): Checks ratio of decompressed `metadata.gz` size vs compressed entry size (lines 1603-1610). **PASS**.

### Path Traversal

- **FAIL**: No check for `../` patterns in tar entry paths. The `entry_path` at line 1583-1585 is only checked against `"metadata.gz"` — no path traversal validation is performed on tar entry names.

### Decompression Limits

- Decompressed `metadata.gz` is checked against `MAX_FILE_SIZE` (50MB) at line 1612. However, the decompression is done via `read_to_string` (line 1599) which allocates the full decompressed content in memory before the size check, meaning a decompression bomb could OOM before the limit is enforced. **PARTIAL** — limit exists but is checked post-decompression.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

- `GemfileParser` (line 82): `fs::read_to_string` returns error on missing file, handled gracefully. **PASS** (returns default PackageData).
- `GemfileLockParser` (line 407): Same pattern. **PASS**.
- `GemspecParser` (line 967): Same pattern. **PASS**.
- `GemArchiveParser` (line 1564-1565): Uses `fs::metadata` before opening. **PASS**.
- `GemMetadataExtractedParser` (line 1941): `fs::read_to_string` error handled. **PASS**.
- No explicit `fs::metadata()` pre-check for the text parsers (Gemfile, GemfileLock, Gemspec), relying on `read_to_string` error instead. This is functionally equivalent but doesn't match the ADR's prescriptive `fs::metadata()` check.

### UTF-8 Encoding

- `fs::read_to_string` returns an error on non-UTF-8 content, which is handled by returning default PackageData. **No** lossy fallback (`String::from_utf8_lossy`) is attempted. If the file contains non-UTF-8 bytes, the parser silently returns default data. **FAIL** — no lossy conversion attempted.

### JSON/YAML Validity

- `parse_gem_metadata_yaml` (line 1635-1636): YAML parse failure returns `Err`, which propagates up to `extract_gem_archive` which returns default `PackageData`. **PASS** for graceful handling.
- `clean_ruby_yaml_tags` (line 1824): Handles regex compilation failure gracefully. **PASS**.

### Required Fields

- Missing `name`/`version` in gemspec results in `None` values — the code continues and returns a `PackageData` with those fields as `None`. **PASS**.
- Missing `name` in Gemfile dependencies (line 216-218): Skips empty names. **PASS**.

### URL Format

- URLs (homepage, download, API) are constructed programmatically or extracted from content as-is. No URL validation or sanitization is performed. Per ADR: "Accept as-is". **PASS**.

## Principle 5: Circular Dependency Detection

**Status**: N/A

These parsers do not perform dependency resolution. The `load_required_ruby_contexts` function (line 1183) follows `require` statements but does so only one level deep (scans original content for requires, loads each found file) — it does not recursively follow requires in loaded files, so cycles are not possible.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No bare `.unwrap()` calls found in library code. All `unwrap_or`, `unwrap_or_default`, and `unwrap_or_else` usages are safe:

- Line 176, 191, 215, 305, 330, 584-585, 595-596, 635, 754, 1029, 1031, 1033, 1035, 1359, 1374, 1495, 1660, 1681, 1696, 1759, 1772, 1869-1870: All `unwrap_or()`/`unwrap_or_default()`/`unwrap_or_else()` — safe defaults.

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new`, `std::process::Command`, or subprocess usage found in this file.

## Findings Summary

| #   | Principle   | Severity | Line(s)                    | Description                                                                                                                                    |
| --- | ----------- | -------- | -------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | P2: DoS     | Medium   | 82, 407, 967, 1941         | No file size pre-check (`fs::metadata().len()`) before `fs::read_to_string` for Gemfile, GemfileLock, Gemspec, and metadata.gz-extract parsers |
| 2   | P2: DoS     | Medium   | 166, 512, 1294, 1578, 1847 | No iteration count cap (100K) on line/entry/dependency loops                                                                                   |
| 3   | P2: DoS     | Low      | 1019, 1599                 | No string field truncation at 10MB limit                                                                                                       |
| 4   | P3: Archive | High     | 1578-1621                  | No path traversal check (`../`) on tar entry paths in .gem archive parser                                                                      |
| 5   | P3: Archive | Medium   | 1599                       | Decompression limit checked post-read; a decompression bomb could OOM before size check executes                                               |
| 6   | P3: Archive | Low      | 1531                       | MAX_ARCHIVE_SIZE is 100MB; ADR specifies 1GB uncompressed limit for archives                                                                   |
| 7   | P4: Input   | Medium   | 82, 407, 967, 1941         | No lossy UTF-8 fallback — non-UTF-8 files cause parser to return default data silently                                                         |

## Remediation Priority

1. **[P3: Archive] Add path traversal check for `../` in tar entry paths** (line 1583-1585) — block entries with `..` path components before processing
2. **[P3: Archive] Use bounded decompression** (line 1596-1600) — read decompressed data in chunks with a size accumulator to enforce limits before full allocation
3. **[P4: Input] Add lossy UTF-8 fallback** (lines 82, 407, 967, 1941) — use `fs::read()` + `String::from_utf8_lossy()` instead of `fs::read_to_string()` to handle non-UTF-8 content
4. **[P2: DoS] Add file size pre-checks** before `fs::read_to_string` using `fs::metadata().len()` with 100MB limit
5. **[P2: DoS] Add iteration count caps** (100K) to all line/entry/dependency loops with early-break and warning
6. **[P2: DoS] Add string field truncation** at 10MB with warning log for oversized values

## Remediation

**PR**: #664
**Date**: 2026-04-14

All findings addressed in ADR 0004 security compliance batch 1.
