# License Detection Implementation Plan

> **Status**: 🟡 Active — core engine implemented; output, CLI, and downstream parity gaps remain open
> **Priority**: P1 - High Priority Core Feature
> **Estimated Effort**: Multi-phase follow-up; depends on output and CLI wiring
> **Dependencies**: [LICENSE_DETECTION_ARCHITECTURE.md](../../LICENSE_DETECTION_ARCHITECTURE.md), [CLI_PLAN.md](../infrastructure/CLI_PLAN.md), [OUTPUT_FORMATS_PLAN.md](../output/OUTPUT_FORMATS_PLAN.md), [SCAN_RESULT_SHAPING_PLAN.md](../post-processing/SCAN_RESULT_SHAPING_PLAN.md), [PLAN-019-file-region-and-unique-detection.md](../../license-detection/PLAN-019-file-region-and-unique-detection.md)

## Overview

The Rust license-detection engine itself is implemented, but Provenant still has
real ScanCode parity gaps in the **public license-result surface**.

This plan tracks the missing work between the current engine and the user-facing
behavior users actually compare against Python ScanCode:

- file-level `license_detections` vs `license_clues`
- top-level unique `license_detections`
- top-level `license_references` and `license_rule_references`
- license diagnostics and matched-text diagnostics
- CLI flag parity for the remaining license options
- downstream consumers such as SPDX writers and clue filtering

The evergreen architecture document remains the source of truth for the
implemented engine internals. This plan is the temporary tracker for the parity
work that is still missing.

## Why This Plan Exists Again

The previous license-detection implementation plan was retired once the core
engine landed. That was correct for engine implementation, but it left no active
place to track the remaining parity work around output shape, CLI flags, and
downstream consumers.

Those gaps are now large enough and user-visible enough that they need an active
implementation plan again.

## Scope

### What This Covers

- File-level license output parity:
  - `license_detections`
  - `license_clues`
  - `detected_license_expression`
  - `detected_license_expression_spdx`
  - `percentage_of_license_text`
- Detection-level diagnostics and matched-text diagnostics:
  - `detection_log`
  - `matched_text`
  - `matched_text_diagnostics`
- Top-level license output parity:
  - unique `license_detections`
  - `license_references`
  - `license_rule_references`
- Package-level license-detection parity where it affects output and reporting:
  - `license_detections`
  - `other_license_detections`
- CLI parity for ScanCode license-related flags
- Downstream consumers blocked on these gaps, especially SPDX output and
  clue-related post-processing behavior

### What This Does Not Cover

- Copyright/email/URL detection parity (separate plans)
- Summary/tallies logic except where those reducers depend on missing license
  surfaces
- Package parser declared-license normalization that is already implemented
- General output-format plumbing unrelated to license semantics

## Current State in Rust

### Implemented

- ✅ Core multi-strategy license-detection engine
- ✅ Public file/package `license_detections`
- ✅ Public file-level `license_clues`
- ✅ Public package `other_license_detections`
- ✅ `--license` CLI flag
- ✅ `--license-rules-path` CLI flag
- ✅ Upstream-named `--license-text` flag for matched text in output
- ✅ `--license-text-diagnostics` CLI flag
- ✅ `--license-diagnostics` CLI flag
- ✅ `--unknown-licenses` CLI flag
- ✅ `--license-score` CLI flag
- ✅ `--license-url-template` CLI flag
- ✅ Internal clue/reference-aware rule and match kinds
- ✅ Internal detection diagnostics (`detection_log`)
- ✅ Internal unknown-license engine support
- ✅ Public file/package `detection_log`
- ✅ Public match-level `matched_text_diagnostics`
- ✅ Public file-level `percentage_of_license_text`
- ✅ Top-level output model fields for `license_references` and
  `license_rule_references`
- ✅ Live native-scan generation of top-level `license_references` and
  `license_rule_references`
- ✅ Native top-level `license_detections` for identifier-bearing file/resource
  detections
- ✅ `--license-references` CLI flag
- ✅ `--from-json` round-trip preservation of preexisting
  `license_references` / `license_rule_references`
- ✅ Package/file reference-following for manifest-local references, license
  beside manifest, package-context inheritance, and root fallback when no
  package exists
- ✅ Fixture-backed end-to-end coverage for the main reference-following
  scenario families plus `--from-json` recomputation after those cases
- ✅ Followed package detections now drive top-level `license_detections`,
  `license_references`, `license_rule_references`, summary, tallies,
  key-file tallies, and SPDX file/package license-info surfaces consistently

### Known Public Parity Gaps

- ⚠️ Clue-only detections now serialize at file level, but some clue-only output
  and filtering edge cases still diverge from upstream
- ❌ `--filter-clues` is only partially license-aware today: shaping now suppresses
  ignorable clues using public match metadata and rule identifiers, but some
  JSON/public-license-shape edge cases still diverge from upstream
- ⚠️ SPDX output parity now consumes real file/package license-info surfaces, but
  any remaining SPDX drift should be tracked as format-specific follow-up rather
  than as a blocker on missing license-output data

### Known CLI Parity Gaps

- ⚠️ Legacy `--include-text` remains as a compatibility alias; the upstream
  public flag is now `--license-text`
- ⚠️ Upstream `--is-license-text` is no longer a live parity target; current
  parity should instead track the emitted `percentage_of_license_text` field

## Root Causes

### 1. Internal detection information is narrowed too early

The engine still distinguishes clue/reference-style matches and carries
`detection_log` internally, but the scanner conversion step currently emits only
the reduced public `LicenseDetection` shape.

That leaves Provenant without a public place to preserve:

- clue-only detections
- diagnostic classifications
- file-region-dependent aggregation metadata

### 2. Remaining gaps are now mostly clue/filter and enrichment work

`license_references`, `license_rule_references`, and top-level unique
`license_detections` now generate from the final post-follow state, including
the main package/file reference-following scenarios and their downstream
reporting consumers. The remaining work in this plan is now concentrated in:

- clue-only output and `--filter-clues` edge cases
- remaining CLI/help parity drift around legacy aliases and documentation
- any remaining exact parity drift in the richer top-level reference/report
  metadata now emitted from the embedded license and rule index

### 3. File-region consumers are now partly implemented, not missing wholesale

The engine and post-processing layers now have file-region-aware unique
aggregation plus the first real downstream consumers for package/file
reference-following and reporting synchronization. The focused sub-plan for
that work remains
[PLAN-019-file-region-and-unique-detection.md](../../license-detection/PLAN-019-file-region-and-unique-detection.md).

### 4. CLI parity drift accumulated after the engine landed

The repository still has a mix of:

- current Rust-only flag names such as `--include-text`
- upstream flags that are not implemented yet
- outdated references to flags that are no longer current upstream targets

## Relationship to Other Plans

- **[CLI_PLAN.md](../infrastructure/CLI_PLAN.md)** owns the flag inventory and
  runtime CLI gating.
- **[SCAN_RESULT_SHAPING_PLAN.md](../post-processing/SCAN_RESULT_SHAPING_PLAN.md)**
  now owns the completed shaping/runtime implementation of `--filter-clues`;
  this plan owns the remaining public license-shape differences that still
  affect exact filtered-output parity.
- **[OUTPUT_FORMATS_PLAN.md](../output/OUTPUT_FORMATS_PLAN.md)** and
  **[PARITY_SCORECARD.md](../output/PARITY_SCORECARD.md)** own format-specific
  output claims such as SPDX parity.
- **[PLAN-019-file-region-and-unique-detection.md](../../license-detection/PLAN-019-file-region-and-unique-detection.md)**
  owns the focused file-region and unique-detection design work used by this
  broader parity plan.

## Implementation Phases

1. **Phase 0 — Documentation and parity inventory**
   - Re-open an active license-detection plan
   - Correct stale CLI/output parity documents
   - Cross-link focused sub-plans

2. **Phase 1 — Public detection-shape parity**
   - Preserve enough internal detection classification to emit
     `license_clues` and `detection_log`
   - Add `matched_text_diagnostics`
   - Add `percentage_of_license_text`

3. **Phase 2 — Top-level license aggregation parity**
   - ✅ Consume file-region-aware unique aggregation in the main package/file
     reference-following flows
   - ✅ Keep top-level `license_detections`, `license_references`, and
     `license_rule_references` synchronized with the post-follow runtime state
   - ✅ Enrich top-level `license_references` / `license_rule_references` with
     the richer stable metadata already available from the embedded index
   - Remaining work here is limited to any uncovered clue edge cases

4. **Phase 3 — CLI flag parity**
   - ✅ Resolve `--include-text` vs `--license-text`
   - ✅ Add remaining ScanCode-compatible license flags
   - Keep help text and docs aligned with the actual runtime surface

5. **Phase 4 — Downstream consumer parity**
   - Close the remaining `--filter-clues` license-edge cases where appropriate
   - ✅ Feed SPDX writers with real license-info-from-files / extracted-license
     data while preserving upstream `NOASSERTION` conclusions
   - ✅ Keep summary/tallies/key-file tallies aligned with followed
     package-origin license evidence

## Verify-First Gap List

These are the highest-risk user-visible parity gaps and should be verified first
whenever work resumes here:

1. file-level `license_clues`
2. `detection_log`
3. `matched_text_diagnostics`
4. top-level unique `license_detections`
5. live `license_references` / `license_rule_references`
6. `percentage_of_license_text`
7. `--license-text` naming parity
8. SPDX license conclusion/info export

## Success Criteria

- [ ] Provenant emits the ScanCode-style split between `license_detections` and
      `license_clues`
- [ ] License diagnostics are available when the corresponding CLI behavior is
      enabled
- [ ] Top-level unique `license_detections` are generated on native scans with
      the remaining file-region-dependent parity edge cases closed
- [x] `license_references` and `license_rule_references` are generated on native
      scans instead of only being preserved from input JSON
- [x] The CLI plan accurately reflects the implemented and pending license flags
- [x] SPDX writers consume current-scan license data with fixture-backed parity
- [x] Evergreen docs describe the current public output shape accurately

## Related Documents

- [LICENSE_DETECTION_ARCHITECTURE.md](../../LICENSE_DETECTION_ARCHITECTURE.md)
- [CLI_PLAN.md](../infrastructure/CLI_PLAN.md)
- [OUTPUT_FORMATS_PLAN.md](../output/OUTPUT_FORMATS_PLAN.md)
- [PARITY_SCORECARD.md](../output/PARITY_SCORECARD.md)
- [SCAN_RESULT_SHAPING_PLAN.md](../post-processing/SCAN_RESULT_SHAPING_PLAN.md)
- [PLAN-019-file-region-and-unique-detection.md](../../license-detection/PLAN-019-file-region-and-unique-detection.md)
- [GAPS.md](../../license-detection/GAPS.md)

## Notes

- `GAPS.md` remains the place for deferred license-detection gaps that we are
  intentionally not fixing right now. This plan is for active parity work.
- Upstream documentation around some license flags has drifted over time;
  fixture-backed and code-backed behavior should remain the primary parity target.
