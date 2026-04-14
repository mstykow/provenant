# ADR 0004 Security Audit: maven

**File**: `src/parsers/maven.rs`
**Date**: 2026-04-14
**Status**: PARTIAL

## Principle 1: No Code Execution

**Status**: PASS

No `Command::new`, `subprocess`, `eval()`, or any code execution mechanism found. Uses `quick-xml` for XML parsing (static AST-based), and string-based property resolution via custom byte-level parser. All parsing is static.

## Principle 2: DoS Protection

**Status**: PARTIAL

### File Size

No `fs::metadata().len()` check before reading. The pom.xml path opens the file via `File::open` at line 748 without size pre-check. The pom.properties path uses `read_file_to_string` at line 2021 without size pre-check. The MANIFEST.MF path uses `read_file_to_string` at line 2106 without size pre-check.

### Recursion Depth

The `PropertyResolver` enforces `max_depth: 10` (line 94) for property resolution. The `resolve_key` method checks `depth >= self.max_depth` at line 106 and returns `None` with a warning. The `resolve_text` method checks depth at line 158. This is well below the 50-level ADR requirement and is appropriate for property resolution. **PASS** for property resolution specifically.

However, the main XML parsing loop at line 840 (`loop { match reader.read_event_into(&mut buf)`) has no recursion depth tracking. While quick-xml is a streaming parser and doesn't recurse, deeply nested XML elements could cause the `current_element` stack (line 765) to grow without bound.

### Iteration Count

No iteration count cap on:

- XML event loop (line 840): No 100K item limit on parsed elements, dependencies, or properties
- `dependency_data` accumulation (line 768)
- `licenses` accumulation (line 771)
- `properties` HashMap (line 832)
- OSGi dependency parsing: `parse_osgi_package_list` (line 2309) and `parse_osgi_bundle_list` (line 2354) iterate without caps

### String Length

No 10 MB truncation with warning on any field value. XML text content, property values, and other string fields are stored without size limits. The `PropertyResolver` has `max_output_len: 100_000` (line 95) for property resolution output, but this only applies within the resolver, not to the raw XML text content.

## Principle 3: Archive Safety

**Status**: N/A

Maven parser does not handle archives.

## Principle 4: Input Validation

**Status**: PARTIAL

### File Exists

No `fs::metadata()` pre-check. Uses `File::open` at line 748 and returns a default `PackageData` on failure (line 752). The `read_file_to_string` helper used for pom.properties and MANIFEST.MF handles errors similarly. **Partial** — returns error rather than panic, but doesn't use `fs::metadata()` as specified.

### UTF-8 Encoding

XML text decoding uses `e.decode().unwrap_or_default()` (line 935, 1268), which silently replaces invalid bytes. The `PropertyResolver` uses `String::from_utf8(output).unwrap_or_else(|_| text.to_string())` (line 218) as a lossy fallback. No explicit `String::from_utf8()` + log warning + lossy conversion pattern as specified.

### JSON/YAML Validity

Returns default `PackageData` on parse failure for pom.xml (line 1424 error path), pom.properties (line 2024), and MANIFEST.MF (line 2109). **PASS**.

### Required Fields

Missing name/version are left as `None` and the parser continues. For pom.xml, if namespace is missing it falls back to parent_group_id (line 1540). **PASS**.

### URL Format

URLs (homepage_url, scm_url, etc.) are accepted as-is without validation. **PASS** per ADR spec.

## Principle 5: Circular Dependency Detection

**Status**: PASS

The `PropertyResolver` explicitly tracks circular references:

- `resolving_set: HashSet<String>` (line 78) for O(1) cycle detection
- `resolving_stack: Vec<String>` (line 79) for error reporting
- `resolve_key` checks `resolving_set.contains(key)` at line 115 and warns + returns `None` on cycle
- Cache (`HashMap<String, String>`) at line 102 prevents redundant work

Note: This is property reference cycle detection, not dependency graph cycle detection. ADR 0004 Principle 5 applies to dependency resolution, which this parser doesn't perform.

## Additional Checks

### .unwrap() in Library Code

**Status**: PASS

No `.unwrap()` calls in library code. All instances are in `#[cfg(test)]` blocks (lines 2467, 2482, 2490, 2507, 2526, 2551, 2567, 2602, 2638, 2655, 2665, 2674, 2691, 2697, 2712, 2745, 2755, 2766, 2768, 2790, 2792, 2816, 2818).

### Command::new / Subprocess Usage

**Status**: PASS

No `Command::new` or subprocess usage found.

## Findings Summary

| #   | Principle           | Severity | Line(s)         | Description                                                                            |
| --- | ------------------- | -------- | --------------- | -------------------------------------------------------------------------------------- |
| 1   | P2: File Size       | Medium   | 748, 2021, 2106 | No `fs::metadata().len()` check before reading file; 100 MB limit not enforced         |
| 2   | P2: Iteration Count | Medium   | 840, 2309, 2354 | No 100K iteration cap on XML event loop or OSGi dependency list parsing                |
| 3   | P2: String Length   | Low      | 935, 1268       | No 10 MB truncation with warning on XML text field values                              |
| 4   | P4: File Exists     | Low      | 748             | Uses `File::open` instead of `fs::metadata()` pre-check as specified                   |
| 5   | P4: UTF-8 Encoding  | Low      | 935, 1268       | Uses `unwrap_or_default()` instead of `String::from_utf8()` + warning + lossy fallback |

## Remediation Priority

1. Add `fs::metadata().len()` check with 100 MB limit before reading files (lines 748, 2021, 2106)
2. Add iteration count cap (100K) on XML event loop and dependency accumulation
3. Add 10 MB string field truncation with warning on parsed text content
4. Add `fs::metadata()` pre-check before `File::open`
5. Replace `unwrap_or_default()` on text decode with explicit UTF-8 validation + warning + lossy conversion
