# ADR 0004 Security Audit: nuget

**File**: `src/parsers/nuget.rs`
**Date**: 2026-04-14
**Status**: DONE

## Principle 1: No Code Execution

**Status**: PASS

No `eval()`, `exec()`, `subprocess`, `Command::new`, or any code execution mechanism. Uses `quick_xml` for XML parsing, `serde_json` for JSON parsing, and `zip::ZipArchive` for archive extraction. All static analysis.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

- **NupkgParser**: Has `fs::metadata().len()` check at line 2484 (`file_metadata.len()`) and enforces `MAX_ARCHIVE_SIZE` (100MB, line 547) at line 2487. **PASS**
- **All other parsers** (PackagesConfigParser line 199, NuspecParser line 257, PackagesLockParser line 564, DotNetDepsJsonParser line 669, ProjectJsonParser line 969, ProjectLockJsonParser line 1001, PackageReferenceProjectParser line 1041): Use `File::open(path)` without `fs::metadata().len()` pre-check. **FAIL**

### Recursion Depth

- `resolve_directory_packages_props` (line 1668) and `resolve_directory_build_props` (line 1732) are recursive functions that follow `Import` project references. They use a `visited: &mut HashSet<PathBuf>` (lines 1671, 1734) to detect cycles — the `canonical` path is checked at lines 1672-1674 and 1736-1738. However, there is no explicit **depth counter** — a deeply nested chain of imports (within the 100-entry cycle set) would recurse up to 100+ levels before a cycle is detected. The recursion depth is implicitly bounded by the `visited` set, but not explicitly tracked at 50 levels. **PARTIAL**

### Iteration Count

No 100K iteration cap on:

- XML event loops (lines 216-233, 290-389, 1081-1244, 1798-1943, 1962-2036, 2618-2714)
- JSON dependency iteration (lines 582-646, 708-782, 1385-1417, 1503-1521)
- Archive entry iteration (lines 2498-2551, 2562-2591)
- `replace_matching_dependency_group` (line 2194) iterates without cap

### String Length

No 10MB field value truncation. XML text events (e.g., line 340-366), JSON string values (e.g., line 589-590), and archive content (line 2528-2531) are stored without length limits. However, `MAX_FILE_SIZE` (50MB, line 548) limits individual archive entries.

## Principle 3: Archive Safety

**Status**: PASS

The `NupkgParser` (lines 2457-2477) extracts `.nupkg` (ZIP) archives with comprehensive safety checks:

- **Size Limits**: `MAX_ARCHIVE_SIZE` (100MB, line 547) checked at line 2487. `MAX_FILE_SIZE` (50MB, line 548) checked at lines 2510 and 2574. Note: 100MB is below the ADR's 1GB uncompressed limit — more conservative.
- **Compression Ratio**: `MAX_COMPRESSION_RATIO` (100:1, line 549) checked at lines 2518-2526.
- **Path Traversal**: The archive extraction only reads `.nuspec` and license files by name — it does NOT extract to disk or construct file paths from archive entries. No path traversal risk since no filesystem writes occur.
- **Decompression Limits**: The per-entry `MAX_FILE_SIZE` (50MB) and `read_to_string`/`read_to_end` calls limit decompression.

Note: The archive size limits are more conservative than ADR requirements (100MB vs 1GB for archives, 50MB vs 100MB for files), which is acceptable.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

- **NupkgParser**: `fs::metadata(path)` at line 2484 before opening. **PASS**
- **All other parsers**: Use `File::open(path)` which returns an error if file doesn't exist, handled with `warn!()` and default return. No explicit `fs::metadata()` pre-check. **PARTIAL** — functionally correct but no explicit pre-check.

### UTF-8 Encoding

- XML text: `e.decode()` at lines 340, 1171, 1887, 1994, 2668 — `quick_xml` handles encoding internally.
- JSON: `serde_json` handles encoding internally.
- Archive license file: `String::from_utf8_lossy(&content)` at line 2587. **PASS** — uses lossy conversion.
- Non-archive parsers: No explicit lossy UTF-8 fallback for file reads.

### JSON/YAML Validity

- JSON parse failures at lines 572-577, 677-682, 977-982, 1011-1018 return `default_package_data()`. **PASS**
- XML parse failures at lines 224-229, 382-385, 1236-1239 return `default_package_data()`. **PASS**
- `parse_nuspec_content` at line 2593 returns `Err` on XML failure, propagated correctly. **PASS**

### Required Fields

Missing `name`/`version` handled via `Option<String>`. Correct.

### URL Format

URLs accepted as-is. ADR-compliant.

## Principle 5: Circular Dependency Detection

**Status**: PARTIAL

- `resolve_directory_packages_props` (line 1668) and `resolve_directory_build_props` (line 1732) use `visited: &mut HashSet<PathBuf>` for cycle detection. This prevents infinite loops from circular imports. **PASS for cycle detection**
- However, no explicit depth counter (50-level limit). The `visited` set provides implicit depth bounding but not at the ADR-specified 50-level granularity.

## Additional Checks

### .unwrap() in Library Code

**Status**: FAIL

- Line 1672: `path.canonicalize().unwrap_or_else(|_| path.to_path_buf())` — uses `unwrap_or_else`, acceptable.
- Line 1736: Same pattern, acceptable.
- Line 785: `split_library_key(&root_key).unwrap_or(("", ""))` — acceptable fallback.
- Line 705: `.cloned().unwrap_or_default()` — acceptable.
- Line 293: `unwrap_or(&[])` — acceptable.

No raw `.unwrap()` without fallback in library code. All uses are `unwrap_or`, `unwrap_or_default`, or `unwrap_or_else`. **PASS on closer inspection**

### Command::new / Subprocess Usage

**Status**: PASS

None.

## Findings Summary

| #   | Principle           | Severity | Line(s)                             | Description                                                                                             |
| --- | ------------------- | -------- | ----------------------------------- | ------------------------------------------------------------------------------------------------------- |
| 1   | P2: File Size       | HIGH     | 199, 257, 564, 669, 969, 1001, 1041 | No `fs::metadata().len()` pre-check for non-archive parsers                                             |
| 2   | P2: Recursion Depth | MEDIUM   | 1668, 1732                          | Recursive import resolution lacks explicit 50-level depth counter; only cycle detection via visited set |
| 3   | P2: Iteration Count | MEDIUM   | 216, 290, 582, 1081, 1798, 2498     | No 100K iteration cap on XML/JSON/archive processing loops                                              |
| 4   | P2: String Length   | LOW      | 340-366, 589                        | No 10MB field value truncation                                                                          |
| 5   | P4: UTF-8 Encoding  | LOW      | 199, 257, etc.                      | No lossy UTF-8 fallback for non-archive file reads                                                      |

## Remediation Priority

1. Add `fs::metadata().len()` pre-check (100MB limit) for all non-archive parsers
2. Add explicit 50-level depth counter to `resolve_directory_packages_props` and `resolve_directory_build_props`
3. Add 100K iteration cap on primary parsing loops
4. Add 10MB field value truncation with warning
5. Add lossy UTF-8 fallback for file reads

## Remediation

1. **P2 HIGH**: No file size checks for 7 non-archive parsers — Added `fs::metadata()` checks with `MAX_MANIFEST_SIZE` or replaced with `read_file_to_string`
2. **P2 MEDIUM**: No recursion depth limit on import resolution — Added `MAX_RECURSION_DEPTH=50` to `resolve_directory_packages_props` and `resolve_directory_build_props`
3. **P2 MEDIUM**: No iteration caps on XML/JSON loops — Added `MAX_ITERATION_COUNT` caps on all primary parsing loops
4. **P2 LOW**: No string truncation — Added `truncate_field()` to all extracted string values
5. **P4 LOW**: No lossy UTF-8 for non-archive reads — Replaced `serde_json::from_reader` with `read_file_to_string`+`from_str` for JSON parsers; added `check_file_size` for XML parsers
