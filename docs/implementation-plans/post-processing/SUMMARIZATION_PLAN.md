# Summary, Tallies & Analysis Implementation Plan

> **Status**: 🟢 Historical rollout record — the summary/tally/classify/generated implementation tracked here is complete
> **Current contract owner**: [`../../CLI_GUIDE.md`](../../CLI_GUIDE.md) for public workflows, [`../../TESTING_STRATEGY.md`](../../TESTING_STRATEGY.md) for verification expectations, and [`SCAN_RESULT_SHAPING_PLAN.md`](SCAN_RESULT_SHAPING_PLAN.md) / [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) for adjacent current boundaries
> **Historical priority / effort**: P2, originally estimated at 3-4 weeks
> **Dependencies at implementation time**: [LICENSE_DETECTION_ARCHITECTURE.md](../../LICENSE_DETECTION_ARCHITECTURE.md), [COPYRIGHT_DETECTION_PLAN.md](../text-detection/COPYRIGHT_DETECTION_PLAN.md), [ASSEMBLY_PLAN.md](../package-detection/ASSEMBLY_PLAN.md)

## Overview

This document is kept as a completed rollout record. Sections such as recommendation, execution order, phase plans, and success criteria below should be read as historical implementation context rather than the live maintainer contract.

This plan covers the remaining ScanCode-compatible **summary and tally surface**: codebase summary output, codebase/file/directory tallies, license clarity scoring, key-file classification, facets, and generated-code detection.

These features are the main post-processing value layer that turns raw file/package findings into project-level answers users actually consume: what the project is primarily licensed under, which files are key licensing files, how clear the licensing story is, and which licenses/copyrights dominate the scan.

Upstream implements these behaviors across multiple plugins (`--summary`, `--tallies`, `--license-clarity-score`, `--classify`, `--facet`, `--generated`). Provenant tracks them in one plan because they share the same data flow and should be implemented against one coherent summary model.

This plan does **not** own the separate output-shaping layer (`--include`, `--only-findings`, `--strip-root`, `--full-root`, `--mark-source`, `--filter-clues`). That work now has its own sibling plan in [`SCAN_RESULT_SHAPING_PLAN.md`](SCAN_RESULT_SHAPING_PLAN.md).

## Historical Recommendation

**Finish the summary/tally pipeline with performance-first constraints.**

Why:

- It is the broader, non-deprecated parity surface in ScanCode.
- Provenant already ships meaningful foundations for it.
- It unlocks multiple pending CLI options and user-facing outputs.
- Consolidation has now been intentionally deferred as a compatibility-only feature, so summarization is the clear remaining summary-oriented post-processing priority.
- Classify/facet/generated remain in scope here because they are not independent reporting features in Provenant; they are input stages for summary, tallies, and clarity.

The practical order should be:

1. shared provenance cleanup in `src/main.rs` so key-file enrichment stops implicitly redefining package declared-license fields
2. summarization parity work (summary, tallies, clarity, classify/facet/generated support)

## Why This Feature Is Justified

For a drop-in ScanCode replacement, summarization is **not optional**.

- Official ScanCode docs still present `--summary`, `--tallies`, `--license-clarity-score`, `--classify`, `--facet`, and `--generated` as active user-facing features.
- These are the main project-level reporting features used for triage, dashboards, compliance review, and quick human interpretation of a scan.
- Provenant already has a partial summary foundation, so finishing this area yields high parity value for relatively low architectural risk.

## Upstream Parity Targets

The current parity target is the actual reference implementation and test surface, not every stale name in upstream docs.

### Must-match upstream behavior

- `--classify`
- `--facet`
- `--generated`
- `--license-clarity-score`
- `--summary`
- `--tallies`
- `--tallies-with-details`
- `--tallies-key-files`
- `--tallies-by-facet`

### Naming caveat

Upstream documentation still mentions names such as `--summary-key-files`, `--summary-by-facet`, and `--summary-with-details` in some places, but the live implementation and active plugin/test surface use the `--tallies-*` family. Provenant should target the real implemented CLI surface and, if helpful, document the upstream doc drift rather than reproducing it as a separate feature set.

## Architectural Boundary

Summarization is a **consumer**, not a normalizer.

- **Parsers** should already provide manifest-derived declared package-license data when the source field is trustworthy enough to normalize.
- **Summarization** should read package-declared metadata plus discovered file/resource evidence and turn them into:
  - tallies
  - clarity scoring
  - scan-level summary expressions
  - classification/facets
- **Summarization should not become the primary place that decides a package's declared license**.

### Performance boundary

Summarization must also remain a **reducer**, not a second scanner.

- Do not re-persist or re-materialize the whole resource tree the way Python `summarycode` does with repeated `resource.save(...)` / `codebase.save_resource(...)` passes.
- Do not re-expand already-counted tallies back into discrete `[value] * count` lists only to count them again.
- Do not repeatedly convert nested package/resource mappings back into heavyweight objects during post-processing.
- Prefer in-place mutation of the already-built Rust `Vec<FileInfo>` / `Vec<Package>` plus one-time indexes for package↔file joins.
- Treat extra file reads as an explicit cost center. Generated-code detection now uses scanner-time hints on normal scans, and any fallback rereads should remain exceptional rather than becoming the default path again.

## Scope

### What This Covers

- **Scan Summary**: Top-level project summary output (`declared_license_expression`, holder/language summaries, other values)
- **License Tallies**: Count and categorize licenses across the codebase
- **Copyright Tallies**: Aggregate copyright holders and statements
- **License Clarity Score**: Calculate project-level license clarity metrics
- **Key-File Classification**: Mark likely licensing/manifest/readme files for summary logic and user inspection
- **Facet Assignment**: Tag files with facets (`core`, `dev`, `tests`, `docs`, `data`, `examples`)
- **Generated Code Detection**: Identify auto-generated files
- **Detailed Tallies**: File- and directory-level tallies where upstream exposes them via `--tallies-with-details`

### What This Doesn't Cover

- License policy evaluation (separate feature)
- Package consolidation (covered by `CONSOLIDATION_PLAN.md` in this directory)
- Scan-result/output shaping such as `--include`, `--only-findings`, root normalization, `--mark-source`, and clue deduplication (covered by [`SCAN_RESULT_SHAPING_PLAN.md`](SCAN_RESULT_SHAPING_PLAN.md))
- Output formatting (covered by OUTPUT_FORMATS_PLAN.md)
- Review-oriented `--todo` workflow parity from Python `summarycode/todo.py` — intentionally out of current Provenant scope because it is a manual-review surface, not a core scan-summary requirement

## Python Reference Implementation

**Location**: `reference/scancode-toolkit/src/summarycode/`

**Key Components**:

- `tallies.py` - License, copyright, package, author, holder, and language tallies
- `score.py` - License clarity scoring
- `classify_plugin.py` - `--classify` plugin and key-file flags
- `classify.py` - File classification rules
- `facet.py` - `--facet` behavior and facet partitioning
- `generated.py` - Generated code detection
- `summarizer.py` - `--summary` generation and reuse of tallies/clarity logic
- `copyright_tallies.py` - Copyright statistics

### Upstream Value Surface

- `--summary` gives users a top-level project view instead of forcing file-by-file interpretation.
- `--tallies` and `--tallies-with-details` support reporting, dashboards, and inventory analysis.
- `--tallies-key-files` narrows reporting to the files most likely to represent project-level licensing intent.
- `--tallies-by-facet` separates shipping code from tests/docs/examples.
- `--license-clarity-score` gives a triage-friendly confidence signal for how clearly licensing is stated.

## Python Performance Pitfalls to Avoid

The Python reference is a useful behavior spec, but its `summarycode` architecture is also a warning sign:

- it performs multiple whole-codebase passes for classify, summary, score, tallies, key-file tallies, by-facet tallies, and review workflows
- it persists per-resource mutations during aggregation (`resource.save(...)`, `codebase.save_resource(...)`)
- it re-expands tallies into repeated value lists (`[value] * count`) and then recounts them
- it repeatedly converts nested mappings back into objects (`Package.from_dict(...)`, `LicenseMatchFromResult.from_dicts(...)`, `resource.to_dict(...)`)
- `--tallies-with-details` amplifies the cost by materializing tallies on every file and directory

Provenant should match the useful behavior surface without inheriting those structural costs.

## Current State in Rust

### Implemented

- ✅ Basic scan statistics (file count, scan time)
- ✅ Output format structure
- ✅ Top-level `summary` output block
- ✅ Top-level `tallies` output block
- ✅ Key-file tagging foundations (`is_legal`, `is_manifest`, `is_readme`, `is_top_level`, `is_key_file`)
- ✅ Package metadata promotion from key files
- ✅ Shared provenance cleanup so key-file license clues no longer mutate package declared-license fields or package detections
- ✅ Initial `license_clarity_score` model/output
- ✅ Core codebase tallies for:
  - `detected_license_expression`
  - `copyrights`
  - `holders`
  - `authors`
  - `programming_language`
- ✅ `tallies_of_key_files` for key-file-only aggregation over the same top-level tally families
- ✅ Per-resource `files[*].tallies` rollups for files and directories over those same tally families
- ✅ File-level `facets` assignment with the ScanCode facet set (`core`, `dev`, `tests`, `docs`, `data`, `examples`)
- ✅ Top-level `tallies_by_facet` buckets over the existing five tally families
- ✅ Local copied fixture coverage for:
  - `facet/cli.expected.json`
  - the `tallies/` fixture corpus under `testdata/summarycode-golden/`
- ✅ Initial non-license-dependent summary fields:
  - `declared_holder`
  - `primary_language`
  - `other_languages`
- ✅ Package-preferred summary origin data for:
  - `declared_license_expression`
  - `declared_holder`
  - `primary_language`
- ✅ Initial summary parity rollups for:
  - `other_license_expressions`
  - `other_holders`
- ✅ Initial license-clarity penalties for:
  - ambiguous compound licensing (`-10`)
  - conflicting lower-level license categories (`-20`)
- ✅ Broader classify substrate for top-level/community file handling on normal root-prefixed scans
- ✅ Generated-file detection (`is_generated`) from ScanCode-style conspicuous header clues
- ✅ Initial CLI gating for:
  - `--summary`
  - `--license-clarity-score`
  - `--tallies`
  - `--tallies-key-files`
  - `--tallies-with-details`
  - `--generated`
- ✅ Active summary/score parity improvements for:
  - boolean simplification of equivalent declared and detected license expressions before summary/tally scoring (for example collapsing `A AND (A OR B)` to `A`)
  - joined-expression primary-license resolution without false ambiguity
  - `WITH` exception expressions no longer suppress ambiguity when a separate plain base-license key-file expression is also present
  - score-only mode using key-file resource evidence instead of package-only origin data
  - package-datafile holder fallback ahead of global key-file holder fallback
  - `other_holders` retaining null buckets while pruning only declared holders
  - top-level/community classification on normal root-prefixed scans feeding summary/score correctly
  - empty declared-holder output when no holder can be established
  - primary-language fallback from tallied sources when top-level packages disagree
  - multi-holder aggregation for a single top-level key file
  - score parity for `no_license_text` and `no_license_or_copyright`
  - score parity for single joined-expression declarations without false ambiguity
  - score parity for nested manifest-style key files without declared copyrights
  - package-copyright-derived declared-holder precedence unless the package datafile already provides a richer holder set
  - package declared-license summary origin from any present package datafile, even when a sibling package datafile is the only top-level key-classified manifest
- ✅ Broader classify parity for active fixtures:
  - resource-level `is_top_level` on root directories and their direct children
  - `is_legal` / `is_readme` checks using both file name and base name
  - path-suffix manifest detection across the wider ScanCode manifest set
  - package-data files still treated as manifests when present
  - package-data ancestry now promotes manifest/legal siblings and ancestor directories into the top-level package view where the active `with_package_data` fixture expects it

### Current performance profile

Implemented Rust behavior is already materially leaner than Python:

- ✅ post-processing operates on in-memory `Vec<FileInfo>` / `Vec<Package>` instead of persisting every resource after each step
- ✅ top-level tallies aggregate counts directly in `HashMap<Option<String>, usize>` rather than re-expanding counted values
- ✅ detailed tallies are bottom-up over already-present file/directory nodes
- ✅ package metadata promotion no longer mutates declared-license provenance from key-file evidence
- ✅ `assign_facets()` now precomputes rule match scope and uses a fixed facet mask instead of per-file string sort/dedup churn
- ✅ generated-file flags are now owned by scan-time detection on normal scans, and production post-processing no longer rereads file contents to recover missing `is_generated` values

### Missing

- ✅ No remaining functional or tracked performance gaps remain in this plan

### Already handled elsewhere

- ✅ Parser-side normalization of trustworthy declared package-license metadata
- ✅ Initial summary consumption of package/key-file declared license data
- ✅ Initial package metadata promotion from key files

### Concrete follow-up before deeper summary parity work

- Shared provenance cleanup is complete.
- `promote_package_metadata_from_key_files(...)` now limits key-file promotion to copyright and holder enrichment.
- Remaining summary work can build on explicit summary/tally outputs instead of implicit package declared-license mutation.

## Implementation Phases

1. **Phase 0**: Shared provenance cleanup so package declared-license fields are no longer implicitly redefined from key-file evidence. ✅
2. **Phase 1**: File classification and key-file tagging foundations ✅
3. **Phase 2**: Package/file metadata promotion foundations ✅
4. **Phase 3**: Initial summary model/output structure ✅
5. **Phase 4**: Initial non-license-dependent summary fields ✅
6. **Phase 5**: Core codebase tallies (`--tallies`) over existing declared/discovered evidence. ✅
7. **Phase 6**: Detailed tally variants (`--tallies-with-details`, `--tallies-key-files`, `--tallies-by-facet`). ✅
8. **Phase 7**: Full license clarity parity. ✅ Complete for the active emitted ScanCode `score/` fixture surface (`basic`, `no_license_text`, `no_license_or_copyright`, `no_license_ambiguity`, `inconsistent_licenses_copyleft`, and `jar`), including joined-expression resolution, score-only key-file evidence, manifest allowlist behavior, ambiguity/conflict penalties, and the current holder-driven score cases.
9. **Phase 8**: Generated-code detection parity plus remaining classify/facet parity gaps. ✅ Complete for the active emitted ScanCode generated/classify fixture surface, including generated hint samples and CLI output (`generated/simple`, `generated/jspc`, `generated/cli.expected.json`) plus active classify fixtures (`cli.expected.json`, `with_package_data.expected.json`).
10. **Phase 9**: Comprehensive `--summary` parity over the completed tally/clarity/classification inputs. ✅ Complete: package-preferred origin fields, `other_license_expressions`/`other_holders`, package-datafile holder fallback, package-copyright holder precedence, empty declared-holder parity, tallied-language fallback when packages disagree, package-datafile eligibility for declared-license summary origin, the main ambiguity/holder fixtures, and localized regression coverage including `package_copyright_precedence`.
11. **Phase 10**: CLI parity wiring for the remaining summary/tally/classify/facet/generated options and regression coverage. ✅ Implemented: `--classify`, `--summary`, `--license-clarity-score`, `--tallies`, `--tallies-key-files`, `--tallies-with-details`, `--tallies-by-facet`, `--facet`, and `--generated` reducer/runtime coverage with localized fixture coverage.
12. **Phase 11**: Performance hardening. ✅ Complete: facet rules now precompute path-vs-broad matching scope, `assign_facets()` uses a fixed facet mask/materialization pass instead of cloning/sorting/deduping matched facet strings per file, and production output now relies on scanner-populated generated flags instead of post-processing file rereads.

## Success Criteria

- [x] Generates accurate codebase tallies for licenses, copyrights, holders, authors, and languages where upstream does
- [x] Produces `--summary` output compatible with the ScanCode reference for covered scenarios
- [x] Calculates license clarity score matching Python semantics
- [x] Classifies key files and broader file categories compatibly with ScanCode
- [x] Supports facet-driven and key-file-driven tally variants
- [x] Detects generated code with documented heuristic behavior
- [x] Exposes the corresponding CLI options with parity-compatible semantics
- [x] Preserves the current in-place/count-based architecture and avoids Python-style repeated-walk/recount/copy regressions
- [x] Golden and integration tests pass

## Related Documents

- **Evergreen**: [`docs/LICENSE_DETECTION_ARCHITECTURE.md`](../../LICENSE_DETECTION_ARCHITECTURE.md) — implemented license-detection engine and match pipeline
- **Implementation**: [`COPYRIGHT_DETECTION_PLAN.md`](../text-detection/COPYRIGHT_DETECTION_PLAN.md) — prerequisite
- **Implementation**: [`ASSEMBLY_PLAN.md`](../package-detection/ASSEMBLY_PLAN.md) — prerequisite
- **Evergreen**: [`docs/ARCHITECTURE.md`](../../ARCHITECTURE.md) — broader processing pipeline

## Notes

- Some summarization foundations can land before full detector parity (for example key-file tagging, package metadata promotion, initial summary fields, and primary-language/holder derivation).
- Full parity for tallies and Python-style scoring still depends on richer discovered-license/copyright coverage and clearer package-vs-file provenance.
- The recent parser-side declared-license normalization work reduces one gap for summarization consumers, but it does not remove the need for summary tallies, facets, generated-code detection, or scan-level aggregation.
- This plan intentionally groups some upstream pre-scan/scan/post-scan features together because they converge on one post-processing summary surface in Provenant.
- Implement incrementally, but preserve the user-visible dependency chain: classification/facets/generated feed tallies and clarity; tallies and clarity feed full summary parity.
- License clarity score is a key metric for compliance teams.
- Python's `--todo` workflow is intentionally not being tracked here; if Provenant ever needs a review/TODO surface, it should start as a separate scoped plan rather than being folded into summary/tallies work.
