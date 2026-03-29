# Scan Result Shaping Implementation Plan

> **Status**: 🟢 Done — shaping-specific CLI behavior now lives end-to-end in `src/scan_result_shaping/`, `src/main.rs`, and scanner path selection; remaining parity work is owned by adjacent plans for broader `--info` surface parity and public license-output shape
> **Priority**: P2 - Medium (important user-facing output semantics, but downstream of core scan correctness)
> **Dependencies**: [CLI_PLAN.md](../infrastructure/CLI_PLAN.md), [ASSEMBLY_PLAN.md](../package-detection/ASSEMBLY_PLAN.md), [SUMMARIZATION_PLAN.md](SUMMARIZATION_PLAN.md), [LICENSE_DETECTION_PLAN.md](../text-detection/LICENSE_DETECTION_PLAN.md)

## Overview

This plan covers the **output-shaping** steps that happen after scanning and before final output serialization, but are not part of summary/tally analysis:

- `--filter-clues`
- `--include`
- `--only-findings`
- `--ignore-author`
- `--ignore-copyright-holder`
- `--strip-root`
- `--full-root`
- `--mark-source`

On the current branch these behaviors live in [`src/scan_result_shaping/`](../../../src/scan_result_shaping/) and are orchestrated from [`src/main.rs`](../../../src/main.rs) before assembly and summary/tally output construction.

## Why This Needs Its Own Plan

These steps are not summary generation.

- They reshape the scanned file tree or mutate per-resource presentation fields.
- They run before `create_output(...)` in `src/post_processing/mod.rs`.
- They affect what downstream assembly/summary logic sees, but they are not part of the summarycode-style aggregation layer.

Keeping them in a separate plan prevents `SUMMARIZATION_PLAN.md` from becoming a catch-all post-scan bucket and makes the performance tradeoffs easier to reason about.

## Scope

### What This Covers

- Include-filtering the file tree while preserving required parent directories
- Whole-resource output filtering by author and copyright-holder regexes
- Filtering to only files with findings while preserving required parent directories
- Deduplicating redundant clue entries on individual files
- Path normalization for relative (`--strip-root`) and absolute (`--full-root`) output paths, including embedded output-facing references such as `Match.from_file`
- Source-tree marking (`is_source`, `source_count`) for files and directories
- Keeping preloaded `--from-json` package/dependency sections aligned with the shaped file tree

### What This Doesn't Cover

- Summary, tallies, score, classify, facets, generated detection (covered by [`SUMMARIZATION_PLAN.md`](SUMMARIZATION_PLAN.md))
- `--info` scan-field enablement (tracked by [`CLI_PLAN.md`](../infrastructure/CLI_PLAN.md))
- Package assembly (covered by [`ASSEMBLY_PLAN.md`](../package-detection/ASSEMBLY_PLAN.md))
- Output format rendering (covered by the output plans)

## Current Rust Design

**Location**: [`src/scan_result_shaping/`](../../../src/scan_result_shaping/)

Current shaping steps:

- `apply_ignore_resource_filter()` — drops whole file resources whose detected authors or holders match user-supplied regex filters
- `apply_include_filter()` — keeps matching files and the directory chain needed to retain a valid tree
- `apply_only_findings_filter()` — drops files without findings while retaining necessary ancestor directories
- `filter_redundant_clues()` — deduplicates identical copyrights/holders/authors/emails/URLs by value and line span
- `normalize_paths()` — applies `--strip-root` / `--full-root` to file paths, package file references, and embedded `Match.from_file` paths
- `apply_mark_source()` — marks source-heavy files/directories and computes `source_count`
- `trim_preloaded_assembly_to_files()` — trims preloaded `--from-json` packages/dependencies to the shaped file set before final output

These functions operate directly on the in-memory `Vec<FileInfo>` produced by the scanner. For native scans, ScanCode-style combined `--include` / `--ignore` path selection now happens before file scanning; shaping retains the `--from-json` path-selection case and the output-facing mutation steps that still belong here.

## Performance Principles

This layer should stay cheap and predictable.

- Prefer in-place filtering/mutation of the existing `Vec<FileInfo>`.
- Avoid repeated full-tree cloning or extra serialization steps.
- Preserve directory ancestry with one pass over kept file paths instead of rebuilding the tree repeatedly.
- Keep `mark_source` bottom-up and count-based rather than rescanning file contents.

This already differs favorably from Python's plugin-oriented summarycode approach because there is no per-resource persistence round-trip here.

## Current State in Rust

### Implemented

- ✅ Native-scan `--include` / `--ignore` parity now happens before file scanning, while `--from-json` path selection remains available in shaping with ancestor-directory preservation
- ✅ Root-resource collection and single-file scan support now give path shaping an explicit scan-root model
- ✅ Native multi-input relative scans now follow the upstream common-prefix + synthetic-include model
- ✅ Whole-resource `--ignore-author` / `--ignore-copyright-holder` filtering with ancestor-directory preservation
- ✅ Only-findings filtering with ancestor-directory preservation, including generated-only files
- ✅ Redundant-clue deduplication by exact value/line-span identity
- ✅ Rule-based ignorable clue suppression for `--filter-clues` using matched rule identifiers and coverage thresholds
- ✅ Relative and absolute path normalization for resource paths, package file references, and embedded `Match.from_file` references
- ✅ `--from-json` path selection and root-flag reshaping now run per loaded scan before merge
- ✅ Source-heavy directory marking using descendant source ratios
- ✅ `--mark-source` now requires `--info` and consumes precomputed file `is_source` state instead of inferring from language presence
- ✅ Trimming of preloaded `--from-json` package/dependency sections to the shaped file tree
- ✅ Top-level package/dependency path projection now follows shaping rules without over-applying `--full-root`
- ✅ Explicit main-pipeline shaping order before assembly and summary/tally generation
- ✅ Expanded invariant coverage in `src/scan_result_shaping_test.rs` for whole-resource ignore filters, `full_root`, broader findings retention, clue dedupe, nested `mark_source`, normalized package file-reference paths, normalized `Match.from_file` paths, and preloaded assembly trimming

### Remaining Work Outside This Plan

- Broader `--info` field-surface parity remains tracked in [`CLI_PLAN.md`](../infrastructure/CLI_PLAN.md)
- Completed public-license-shape / serialization parity for `--filter-clues` is recorded in the completed [`LICENSE_DETECTION_PLAN.md`](../text-detection/LICENSE_DETECTION_PLAN.md)
- Ongoing large-scan performance review remains a general maintenance concern, not a shaping-specific parity blocker

## Ordering Contract

For native scans and `--from-json` re-shaping alike, the current shaping contract is:

1. Native-scan path selection (`--include` / `--ignore`) before file scanning
2. `filter_redundant_clues()`
3. `apply_ignore_resource_filter()`
4. `apply_path_selection_filter()` for `--from-json` path selection
5. `apply_only_findings_filter()`
6. `apply_mark_source()`
7. `trim_preloaded_assembly_to_files()` for preloaded `--from-json` package/dependency sections
8. assembly selection
9. final path projection (`normalize_paths()` + top-level package/dependency path projection)
10. `create_output(...)`

This keeps file-tree mutation complete before assembly runs on native scans and
before preloaded package/dependency sections are emitted for `--from-json`.

## Architectural Boundary

This layer should remain a **presentation shaper**, not a data enricher.

- It may remove or relabel existing output-facing fields.
- It should not invent new summary/package conclusions.
- It should not become the place where package ownership or declared-license provenance is inferred.
- It may trim already-loaded package/dependency sections only to keep them consistent with an already-shaped file tree.

## Success Criteria

- [x] All shaping-specific CLI flags are implemented and wired through the intended pipeline stage
- [x] Output-shaping flags remain clearly separated from summary/tally computation
- [x] Filtering preserves required ancestor directories deterministically
- [x] Path normalization behavior is explicit and tested
- [x] `mark_source` remains count-based and does not introduce rescans
- [x] Large scans do not incur Python-style repeated persistence/copy overhead in this layer

## Related Documents

- **Sibling**: [`SUMMARIZATION_PLAN.md`](SUMMARIZATION_PLAN.md)
- **Sibling**: [`CLI_PLAN.md`](../infrastructure/CLI_PLAN.md)
- **Prerequisite**: [`ASSEMBLY_PLAN.md`](../package-detection/ASSEMBLY_PLAN.md)

## Notes

- The current branch already implements the core shaping pipeline; this document exists to make that scope explicit and keep it from being conflated with summarycode parity work.
- The shaping-specific missing CLI flags (`--ignore-author`, `--ignore-copyright-holder`) are now implemented in the Rust CLI/runtime and belong to this plan rather than to summary or package plans.
- Shaping-specific parity is now considered complete; any further differences should land in adjacent plans unless they introduce a new output-shaping flag or ordering rule.
- If future ScanCode-compatible shaping flags land, add them here unless they directly feed summary/tally semantics.
