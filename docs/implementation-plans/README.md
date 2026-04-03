# Implementation Plans

This directory contains the project's **active implementation plans**, **completed rollout records**, and **deferred-scope decision records** for porting Python ScanCode features to Rust. Not every file here is a current source of truth. Use this index to tell which documents are still active, which are historical, and which evergreen document now owns the live maintainer contract.

## Directory Structure

Plans are organized by major feature area:

```text
implementation-plans/
├── package-detection/     # Package manifest parsing and assembly
├── text-detection/        # License, copyright, email/URL detection
├── post-processing/       # Summarization, tallies, classification
├── output/                # Output format support (SPDX, CycloneDX, etc.)
└── infrastructure/        # Plugin system, caching, progress tracking
```

## Active Plans

## Historical Rollout Records and Reference Documents

These topics are implemented. Some remain useful as completed historical records, while others point to the evergreen maintainer document that now owns the live contract.

### Package Detection (`package-detection/`)

- **[PARSER_PLAN.md](package-detection/PARSER_PLAN.md)** - Individual file format parser implementations
  - Status: 🟢 Complete — planned production parser/recognizer coverage is implemented; deferred and future-scope items are documented in [PARSER_PLAN.md](package-detection/PARSER_PLAN.md)

- **[ASSEMBLY_PLAN.md](package-detection/ASSEMBLY_PLAN.md)** - Package assembly roadmap
  - Status: 🟢 Complete — All phases done (sibling merge, nested merge, workspace assembly, file reference resolution)

- **[PARSER_ENHANCEMENT_PLAN.md](package-detection/PARSER_ENHANCEMENT_PLAN.md)** - Cross-cutting parser enhancement and shared declared-license normalization record
  - Status: 🟢 Complete — the shared parser-side declared-license normalization rollout is implemented, and the document is now kept as completed historical/reference documentation

### Text Detection (`text-detection/`)

- **[LICENSE_DETECTION_PLAN.md](text-detection/LICENSE_DETECTION_PLAN.md)** - Completed public license-result, CLI, and downstream parity rollout record
  - Status: 🟢 Complete — the public license-result, diagnostics, references, clue handling, and CLI parity work tracked in [LICENSE_DETECTION_PLAN.md](text-detection/LICENSE_DETECTION_PLAN.md) is implemented; the document remains as the completed implementation record

- **[LICENSE_DETECTION_ARCHITECTURE.md](../LICENSE_DETECTION_ARCHITECTURE.md)** - Evergreen architecture reference for the implemented license-detection engine
  - Status: 🟢 Reference — the evergreen engine architecture lives in [LICENSE_DETECTION_ARCHITECTURE.md](../LICENSE_DETECTION_ARCHITECTURE.md), and the completed public-surface rollout is recorded in [LICENSE_DETECTION_PLAN.md](text-detection/LICENSE_DETECTION_PLAN.md)

- **[COPYRIGHT_DETECTION_PLAN.md](text-detection/COPYRIGHT_DETECTION_PLAN.md)** - Copyright statement extraction
  - Status: 🟢 Complete — scanner/runtime ingestion now covers decoded non-UTF text, PDF text, and binary printable strings; Rust also adds supported-image EXIF/XMP metadata as a beyond-parity clue source, and intentional divergences are tracked in the plan

- **[EMAIL_URL_DETECTION_PLAN.md](text-detection/EMAIL_URL_DETECTION_PLAN.md)** - Email and URL extraction
  - Status: 🟢 Complete — scanner/runtime ingestion now covers decoded non-UTF text, PDF text, and binary printable strings; Rust also adds supported-image EXIF/XMP metadata as a beyond-parity clue source, and intentional divergences are tracked in the plan

### Post-Processing (`post-processing/`)

- **[SUMMARIZATION_PLAN.md](post-processing/SUMMARIZATION_PLAN.md)** - Completed summary/tally/classify/generated rollout record
  - Status: 🟢 Historical — implemented, but not the canonical source for current testing or architecture guidance

- **[SCAN_RESULT_SHAPING_PLAN.md](post-processing/SCAN_RESULT_SHAPING_PLAN.md)** - Include/filter/root/source output shaping
  - Status: 🟢 Complete — shaping-specific CLI behavior now lives in `src/scan_result_shaping/`, scanner path selection, and the main orchestration pipeline; remaining non-shaping parity follow-up is tracked in adjacent plans

### Infrastructure (`infrastructure/`)

- **[CACHING_PLAN.md](infrastructure/CACHING_PLAN.md)** - Incremental scanning
  - Status: 🟢 Complete — incremental scanning, XDG cache defaults, and cache lock coordination are implemented; the plan remains as the rollout record

- **[CLI_PLAN.md](infrastructure/CLI_PLAN.md)** - Completed command-line interface parity rollout record
  - Status: 🟢 Complete — the current ScanCode-facing CLI surface and explicit `Won't do` scope decisions are implemented and recorded in [CLI_PLAN.md](infrastructure/CLI_PLAN.md); any post-rollout parity follow-up remains tracked there as maintenance

- **[PROGRESS_TRACKING_PLAN.md](infrastructure/PROGRESS_TRACKING_PLAN.md)** - Enhanced progress reporting
  - Status: 🟢 Implemented — progress manager, mode handling, summary/reporting, and integration tests are tracked in the plan document

### Output Formats (`output/`)

- **[OUTPUT_FORMATS_PLAN.md](output/OUTPUT_FORMATS_PLAN.md)** - SPDX, CycloneDX, YAML, HTML, JSON Lines, Debian, and template output
  - Status: 🟢 Historical — broad output coverage is implemented, but use [PARITY_SCORECARD.md](output/PARITY_SCORECARD.md) for the current equivalent/partial parity breakdown

- **[PARITY_SCORECARD.md](output/PARITY_SCORECARD.md)** - Format-by-format parity contract and fixture coverage
  - Status: 🟢 Canonical reference — maintained as the current output parity contract and verification checklist

## Deferred / Not Planned

These documents are retained as explicit product-scope decisions. They describe upstream functionality and possible implementation paths, but they are intentionally not on the current Provenant roadmap.

### Post-Processing (`post-processing/`)

- **[CONSOLIDATION_PLAN.md](post-processing/CONSOLIDATION_PLAN.md)** - Legacy-compatible resource/package grouping view
  - Status: ⚪ Deferred — intentionally not planned because it is compatibility-oriented, upstream-deprecated, and not required for Provenant's latest-functionality goal

### Infrastructure (`infrastructure/`)

- **[PLUGIN_SYSTEM_PLAN.md](infrastructure/PLUGIN_SYSTEM_PLAN.md)** - Runtime/extensible plugin architecture
  - Status: ⚪ Deferred — intentionally not planned because Provenant is favoring compile-time integration over runtime plugin loading

## Placeholder Plans (Still High-Level)

These remain intentionally high-level until implementation work begins.

## Document Lifecycle

1. **Placeholder** - Brief description of component, scope, and dependencies
2. **Planning** - Detailed analysis, design decisions, implementation phases
3. **Active** - Work in progress, updated with status
4. **Complete** - Feature implemented; document retained either as historical rollout documentation or, if explicitly marked, as a maintained checklist/reference
5. **Deferred / Not Planned** - Explicitly out of current product scope; retained as a decision record and future reference

### Documentation Style for Plan Status

- Prefer stable wording (for example: "tracked in the plan document") over point-in-time snapshots.
- Avoid embedding volatile counts, one-off verification snapshots, or temporary pass/fail badges.
- Keep detailed status updates in the linked plan documents and CI/PR logs.
- When referencing internal files or documents, prefer explicit relative Markdown links over plain path text.

## Relationship to Evergreen Docs

These implementation plans complement the **evergreen** documentation in [`docs/`](../), but they are usually **not** the canonical source of truth once a feature has shipped:

| Evergreen (Permanent)               | Implementation Plans (Temporary)                  |
| ----------------------------------- | ------------------------------------------------- |
| `ARCHITECTURE.md`                   | Component-specific implementation plans           |
| `LICENSE_DETECTION_ARCHITECTURE.md` | Implemented license-detection subsystem reference |
| `HOW_TO_ADD_A_PARSER.md`            | `PARSER_PLAN.md`                                  |
| `TESTING_STRATEGY.md`               | Test plans within implementation docs             |
| `adr/`                              | Design decisions made during implementation       |
| `improvements/`                     | Beyond-parity features documented here            |

Once a feature is complete, relevant architectural decisions move to ADRs, and the implementation plan should either be archived, clearly marked historical, or redirected to the evergreen document that now owns the topic. Unless a file is explicitly labeled as a maintained checklist/reference (for example `PARITY_SCORECARD.md`), treat completed plans here as non-canonical historical records.
