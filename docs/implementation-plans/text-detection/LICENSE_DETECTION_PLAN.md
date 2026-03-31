# License Detection Implementation Plan

> **Status**: 🟢 Complete — public license-result, CLI, and downstream parity work tracked here is implemented
> **Priority**: P1 - High Priority Core Feature
> **Estimated Effort**: Completed
> **Dependencies**: [LICENSE_DETECTION_ARCHITECTURE.md](../../LICENSE_DETECTION_ARCHITECTURE.md), [CLI_PLAN.md](../infrastructure/CLI_PLAN.md), [OUTPUT_FORMATS_PLAN.md](../output/OUTPUT_FORMATS_PLAN.md), [SCAN_RESULT_SHAPING_PLAN.md](../post-processing/SCAN_RESULT_SHAPING_PLAN.md)

## Overview

The Rust license-detection engine and the public license-result parity work that
this plan tracked are now implemented.

This document remains as the completed implementation record for the user-facing
behavior Provenant needed to match from Python ScanCode:

- file-level `license_detections` vs `license_clues`
- top-level unique `license_detections`
- top-level `license_references` and `license_rule_references`
- license diagnostics and matched-text diagnostics
- CLI flag parity for the remaining license options
- downstream consumers such as SPDX writers and clue filtering

The evergreen architecture document remains the source of truth for the
implemented engine internals.

## Final Follow-Up Status

The focused follow-up items that were temporarily tracked outside this document
have now been implemented and folded back into the evergreen architecture and
completed-plan docs.

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

No open public-surface parity blockers remain in this plan.

Any future SPDX or other output-format drift should be tracked as a
format-specific follow-up in [`OUTPUT_FORMATS_PLAN.md`](../output/OUTPUT_FORMATS_PLAN.md)
or [`PARITY_SCORECARD.md`](../output/PARITY_SCORECARD.md), not as missing
license-output data.

### Compatibility Notes

- `--is-license-text` is a removed upstream legacy flag and therefore a
  `Won't do` compatibility surface; current parity tracks the emitted
  `percentage_of_license_text` field instead

## Implementation Summary

The work this plan originally tracked is complete.

The final implementation now includes:

- the public split between `license_detections` and `license_clues`,
- detection diagnostics and matched-text diagnostics,
- top-level unique `license_detections`, `license_references`, and
  `license_rule_references`,
- file-region-aware aggregation for the current Provenant parity scope,
- ScanCode-facing license CLI parity for the supported surface, and
- downstream SPDX/reporting consumers wired to live scan results.

Any future regressions should be tracked in the relevant evergreen or
format-specific document rather than by reopening this plan as an active work
tracker.

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
- Review-oriented `--todo` parity remains intentionally out of scope and is
  owned by broader CLI/post-processing scope decisions, not by this license
  plan.

## Historical Implementation Phases

The phased rollout this document originally tracked has been completed and is
retained here only as historical context in the surrounding sections and linked
documents.

## Success Criteria

- [x] Provenant emits the ScanCode-style split between `license_detections` and
      `license_clues`
- [x] License diagnostics are available when the corresponding CLI behavior is
      enabled
- [x] Top-level unique `license_detections` are generated on native scans with
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

## Notes

- Upstream documentation around some license flags has drifted over time;
  fixture-backed and code-backed behavior should remain the primary parity target.
