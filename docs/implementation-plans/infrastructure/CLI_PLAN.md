# CLI Implementation Plan

> **Status**: 🟡 Active — core scan/output parity is substantial, but gaps remain across positional-input parity, `--info`-related behavior, package scanning extensions, output filtering, Debian output, and a few post-scan workflows
> **Priority**: P1 - High (user-facing drop-in replacement parity)
> **Dependencies**: Some flags depend on underlying features (license detection, post-scan processing, caching)

## Overview

CLI parameter parity with Python ScanCode. Rust uses `clap`; Python uses
`click` + plugin-provided options.

This plan tracks progress toward a **drop-in replacement CLI surface**.

It records both implemented compatibility coverage and the remaining explicit
CLI backlog, including upstream flags that would otherwise remain implicit gaps.

**Location**: [`src/cli.rs`](../../../src/cli.rs)

## Planning Rules

### How flags are classified

- List each flag or positional argument exactly once.
- Group flags by what a user expects them to do, not by which subsystem owns the implementation.
- Keep active user-facing scan/output functionality in the parity backlog.
- Mark legacy, review-only, internal, test-harness, or meta-only surfaces as `Won't do` instead of treating them as normal missing work.
- Keep Provenant-only conveniences visible, but label them `Rust-specific` so they do not look like parity requirements.

### Status Legend

| Status          | Meaning                                                                        |
| --------------- | ------------------------------------------------------------------------------ |
| `Done`          | Implemented and intended to remain part of the offered CLI surface             |
| `Partial`       | Implemented enough to expose, but parity details or edge semantics remain open |
| `Planned`       | Worth offering for active user-facing functionality, but not implemented yet   |
| `Won't do`      | Intentionally not offered for current Provenant scope                          |
| `Rust-specific` | Provenant-only convenience, not part of ScanCode parity                        |

## Flag Inventory

### Invocation & Input Handling

| Flag                   | What it does                                            | Status | Notes                                                                                                                                                                                           |
| ---------------------- | ------------------------------------------------------- | ------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `<input>...`           | Supplies the path or paths to scan                      | `Done` | Native scans now support the upstream-style relative multi-input common-prefix flow, and `--from-json` still supports multiple scan files.                                                      |
| `-h, --help`           | Prints CLI help                                         | `Done` | Provided by `clap`.                                                                                                                                                                             |
| `-V, --version`        | Prints CLI version                                      | `Done` | Provided by `clap`.                                                                                                                                                                             |
| `-q, --quiet`          | Reduces runtime output                                  | `Done` | Matches the current quiet-mode surface.                                                                                                                                                         |
| `-v, --verbose`        | Increases runtime path reporting                        | `Done` | Matches the current verbose-path surface.                                                                                                                                                       |
| `-m, --max-depth`      | Limits recursive scan depth                             | `Done` | `0` means no depth limit.                                                                                                                                                                       |
| `-n, --processes`      | Controls worker count                                   | `Done` | Compatible with `0` and `-1` values.                                                                                                                                                            |
| `--timeout`            | Sets per-file processing timeout                        | `Done` | Wired through the scanner runtime.                                                                                                                                                              |
| `--exclude / --ignore` | Excludes files by glob pattern                          | `Done` | `--ignore` is the ScanCode-facing alias.                                                                                                                                                        |
| `--include`            | Re-includes matching paths after filtering              | `Done` | Native scans now apply ScanCode-style combined include/ignore path filtering before file scanning; `--from-json` applies the same path selection as a shaping step over the loaded result tree. |
| `--strip-root`         | Rewrites paths relative to the scan root                | `Done` | Root-resource, single-file, native multi-input, nested reference, and top-level package/dependency path projection are now handled in the final shaping pass.                                   |
| `--full-root`          | Preserves absolute/rooted output paths                  | `Done` | Full-root display paths now follow the ScanCode-style formatting pass, including path cleanup and field-specific projection rules.                                                              |
| `--from-json`          | Loads prior scan JSON instead of rescanning input files | `Done` | Supports multiple input scans, shaping-time include/ignore filtering, root-flag reshaping per loaded scan before merge, and recomputation of followed top-level license outputs after load.     |

### Output Formats & Result Shaping

| Flag                                  | What it does                                           | Status    | Notes                                                                                                                                                                                                                                                             |
| ------------------------------------- | ------------------------------------------------------ | --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--json <FILE>`                       | Writes compact JSON output                             | `Done`    | Core output format.                                                                                                                                                                                                                                               |
| `--json-pp <FILE>`                    | Writes pretty-printed JSON output                      | `Done`    | Core output format.                                                                                                                                                                                                                                               |
| `--json-lines <FILE>`                 | Writes JSON Lines output                               | `Done`    | Core output format.                                                                                                                                                                                                                                               |
| `--yaml <FILE>`                       | Writes YAML output                                     | `Done`    | Core output format.                                                                                                                                                                                                                                               |
| `--csv <FILE>`                        | Writes CSV output                                      | `Done`    | Upstream-deprecated but still supported.                                                                                                                                                                                                                          |
| `--html <FILE>`                       | Writes HTML report output                              | `Done`    | Core output format.                                                                                                                                                                                                                                               |
| `--html-app <FILE>`                   | Writes the deprecated HTML app output                  | `Done`    | Supported and hidden, matching the upstream source-level treatment of this deprecated flag.                                                                                                                                                                       |
| `--spdx-tv <FILE>`                    | Writes SPDX tag/value output                           | `Done`    | Core output format.                                                                                                                                                                                                                                               |
| `--spdx-rdf <FILE>`                   | Writes SPDX RDF/XML output                             | `Done`    | Core output format.                                                                                                                                                                                                                                               |
| `--cyclonedx <FILE>`                  | Writes CycloneDX JSON output                           | `Done`    | Core output format.                                                                                                                                                                                                                                               |
| `--cyclonedx-xml <FILE>`              | Writes CycloneDX XML output                            | `Done`    | Core output format.                                                                                                                                                                                                                                               |
| `--custom-output <FILE>`              | Writes output using a custom template                  | `Done`    | Requires `--custom-template`.                                                                                                                                                                                                                                     |
| `--custom-template <FILE>`            | Supplies the template for `--custom-output`            | `Done`    | Requires `--custom-output`.                                                                                                                                                                                                                                       |
| `--debian <FILE>`                     | Writes Debian copyright output                         | `Planned` | Active user-facing output parity gap.                                                                                                                                                                                                                             |
| `--mark-source`                       | Marks source-heavy files and directories               | `Done`    | Now requires `--info` and consumes precomputed file `is_source` state for directory marking.                                                                                                                                                                      |
| `--only-findings`                     | Filters output down to files with findings             | `Done`    | Implemented in scan-result shaping.                                                                                                                                                                                                                               |
| `--filter-clues`                      | Removes redundant clue output                          | `Partial` | Implemented in scan-result shaping with exact dedupe plus rule-based ignorable clue suppression; remaining parity work is now limited to the license-plan edge cases still tracked in [`LICENSE_DETECTION_PLAN.md`](../text-detection/LICENSE_DETECTION_PLAN.md). |
| `-i, --info`                          | Gates file-info output and related info-only workflows | `Partial` | Compatibility flag now exists and gates `--mark-source`, but broader upstream info-field parity remains open outside the shaping plan.                                                                                                                            |
| `--ignore-author <pattern>`           | Filters author findings by pattern                     | `Done`    | Implemented as a whole-resource shaping filter.                                                                                                                                                                                                                   |
| `--ignore-copyright-holder <pattern>` | Filters copyright-holder findings by pattern           | `Done`    | Implemented as a whole-resource shaping filter.                                                                                                                                                                                                                   |

### Scan & Detection Controls

| Flag                         | What it does                                        | Status          | Notes                                                                                                                                                                      |
| ---------------------------- | --------------------------------------------------- | --------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `-l, --license`              | Enables license scanning                            | `Done`          | The toggle exists; broader license-output parity is tracked in [`LICENSE_DETECTION_PLAN.md`](../text-detection/LICENSE_DETECTION_PLAN.md).                                 |
| `--license-rules-path`       | Loads extra license rules from disk                 | `Done`          | Requires `--license`.                                                                                                                                                      |
| `--include-text`             | Legacy alias for matched license text output        | `Rust-specific` | Retained as a compatibility alias; the upstream-facing flag is now `--license-text`.                                                                                       |
| `--license-score`            | Filters or reports by license score thresholds      | `Planned`       | Tracked in [`LICENSE_DETECTION_PLAN.md`](../text-detection/LICENSE_DETECTION_PLAN.md).                                                                                     |
| `--license-text`             | Emits matched license text                          | `Done`          | Requires `--license`; file/package matches now carry `matched_text` under the upstream flag name.                                                                          |
| `--license-text-diagnostics` | Emits detailed license-text diagnostics             | `Done`          | Requires `--license-text`; match output now includes `matched_text_diagnostics`.                                                                                           |
| `--license-diagnostics`      | Emits detailed license-match diagnostics            | `Done`          | Requires `--license`; file/package detections now include `detection_log` when enabled.                                                                                    |
| `--unknown-licenses`         | Reports unknown-license detections                  | `Done`          | Requires `--license`; wired through to the license engine's unknown-license pass.                                                                                          |
| `--license-url-template`     | Customizes license reference URLs                   | `Planned`       | Tracked in [`LICENSE_DETECTION_PLAN.md`](../text-detection/LICENSE_DETECTION_PLAN.md).                                                                                     |
| `-c, --copyright`            | Enables copyright, holder, and author detection     | `Done`          | Core scan toggle.                                                                                                                                                          |
| `-e, --email`                | Enables email detection                             | `Done`          | Core scan toggle.                                                                                                                                                          |
| `--max-email`                | Caps email findings per file                        | `Done`          | Requires `--email`; `0` means no limit.                                                                                                                                    |
| `-u, --url`                  | Enables URL detection                               | `Done`          | Core scan toggle.                                                                                                                                                          |
| `--max-url`                  | Caps URL findings per file                          | `Done`          | Requires `--url`; `0` means no limit.                                                                                                                                      |
| `-p, --package`              | Enables package manifest and lockfile scanning      | `Done`          | Core scan toggle.                                                                                                                                                          |
| `--generated`                | Detects and reports generated files during scanning | `Partial`       | Implemented, but this compatibility surface is still tracked alongside summary/reporting semantics in [`SUMMARIZATION_PLAN.md`](../post-processing/SUMMARIZATION_PLAN.md). |
| `--facet <facet>=<pattern>`  | Assigns files to facets such as `core` or `tests`   | `Partial`       | Implemented, but keep remaining CLI-compatibility notes centralized here; behavior is owned by [`SUMMARIZATION_PLAN.md`](../post-processing/SUMMARIZATION_PLAN.md).        |

### Post-Scan Analysis & Reporting

| Flag                      | What it does                                                                   | Status     | Notes                                                                                                                                                                                                                      |
| ------------------------- | ------------------------------------------------------------------------------ | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--classify`              | Classifies key files and related project-level signals                         | `Partial`  | Implemented enough to expose; broader parity is still summarized in [`SUMMARIZATION_PLAN.md`](../post-processing/SUMMARIZATION_PLAN.md).                                                                                   |
| `--summary`               | Emits top-level project summary output                                         | `Partial`  | Requires `--classify`; package/file reference-following evidence now feeds summary consistently, but broader parity remains tracked in [`SUMMARIZATION_PLAN.md`](../post-processing/SUMMARIZATION_PLAN.md).                |
| `--license-clarity-score` | Emits project-level license clarity scoring                                    | `Partial`  | Requires `--classify`; followed package/manifest license evidence now feeds clarity scoring, while remaining heuristic edge semantics stay tracked in [`SUMMARIZATION_PLAN.md`](../post-processing/SUMMARIZATION_PLAN.md). |
| `--tallies`               | Emits top-level tallies                                                        | `Partial`  | Implemented over the current tally families, including followed package/manifest license evidence; keep broader CLI parity tracking here.                                                                                  |
| `--tallies-with-details`  | Emits per-resource tallies                                                     | `Partial`  | Implemented for file and directory resources; keep CLI parity tracking here.                                                                                                                                               |
| `--tallies-key-files`     | Emits tallies for key files only                                               | `Partial`  | Requires `--classify` and `--tallies`; key-file tallies now stay aligned with followed package/manifest license evidence.                                                                                                  |
| `--tallies-by-facet`      | Emits tallies split by facet                                                   | `Partial`  | Requires `--facet` and `--tallies`.                                                                                                                                                                                        |
| `--license-references`    | Emits top-level license reference blocks                                       | `Done`     | Requires `--license`; native scans now generate top-level `license_references` and `license_rule_references` from the final post-follow state, and `--from-json` recomputes them when appropriate.                         |
| `--license-policy`        | Evaluates findings against a license policy                                    | `Planned`  | Active functionality gap, but not yet implemented.                                                                                                                                                                         |
| `--is-generated`          | Reports percentage/license-text-style generated indicators in post-scan output | `Planned`  | Tracked in [`SUMMARIZATION_PLAN.md`](../post-processing/SUMMARIZATION_PLAN.md).                                                                                                                                            |
| `--timing`                | Emits per-resource scan timing details                                         | `Planned`  | Diagnostic reporting surface; not yet implemented.                                                                                                                                                                         |
| `--consolidate`           | Emits the legacy consolidated package/component view                           | `Won't do` | Intentionally out of scope; see [`CONSOLIDATION_PLAN.md`](../post-processing/CONSOLIDATION_PLAN.md).                                                                                                                       |
| `--todo`                  | Emits manual-review TODO workflow output                                       | `Won't do` | Intentionally out of scope; see [`SUMMARIZATION_PLAN.md`](../post-processing/SUMMARIZATION_PLAN.md).                                                                                                                       |

### Package, Compatibility & Meta Commands

| Flag                                                 | What it does                                                 | Status     | Notes                                                                                                                |
| ---------------------------------------------------- | ------------------------------------------------------------ | ---------- | -------------------------------------------------------------------------------------------------------------------- |
| `--system-package`                                   | Enables system-package style package detection               | `Planned`  | Package-related parity gap.                                                                                          |
| `--package-in-compiled`                              | Extracts package metadata from compiled artifacts            | `Planned`  | Package-related parity gap.                                                                                          |
| `--package-only`                                     | Restricts output to package-focused results                  | `Planned`  | Package-related parity gap.                                                                                          |
| `--list-packages`                                    | Lists supported package handlers or package-related surfaces | `Won't do` | Inventory/meta surface rather than core scan-output functionality.                                                   |
| `-A, --about`                                        | Prints about/help text beyond normal help output             | `Won't do` | Meta/help surface, not part of core scan-result functionality.                                                       |
| `--examples`                                         | Prints usage examples                                        | `Won't do` | Meta/help surface, not part of core scan-result functionality.                                                       |
| `--plugins`                                          | Lists plugin surfaces                                        | `Won't do` | Provenant intentionally does not plan a runtime plugin system; see [`PLUGIN_SYSTEM_PLAN.md`](PLUGIN_SYSTEM_PLAN.md). |
| `--print-options`                                    | Prints option inventory metadata                             | `Won't do` | Meta/help surface, not part of core scan-result functionality.                                                       |
| `--keep-temp-files`                                  | Preserves temporary debug files                              | `Won't do` | Hidden debugging/housekeeping flag, not part of the intended CLI surface.                                            |
| `--check-version / --no-check-version`               | Controls upstream version-check behavior                     | `Won't do` | Hidden update-check convenience, not part of scan/output parity.                                                     |
| `--test-mode / --test-slow-mode / --test-error-mode` | Exposes upstream internal test-harness modes                 | `Won't do` | Hidden test-only flags, not part of the intended CLI surface.                                                        |

### Cache & Rust-Specific Extras

| Flag                 | What it does                                        | Status          | Notes                                                                                                     |
| -------------------- | --------------------------------------------------- | --------------- | --------------------------------------------------------------------------------------------------------- |
| `--cache <kind>`     | Opts into specific persistent cache kinds           | `Done`          | Repeated/CSV flag; currently only `scan-results` is implemented.                                          |
| `--cache-dir`        | Chooses the shared persistent cache root            | `Done`          | Root selector only; does not enable caching by itself.                                                    |
| `--cache-clear`      | Clears the selected persistent cache root           | `Done`          | Clears cache state before scanning without implicitly enabling caches.                                    |
| `--max-in-memory`    | Caps in-memory scan buffering before spill behavior | `Partial`       | Currently parse-only; upstream default `10000`, `-1` acceptance, and spill semantics are not yet matched. |
| `--no-assemble`      | Skips package assembly after manifest detection     | `Rust-specific` | Provenant-only convenience; Python ScanCode always assembles.                                             |
| `--no-cache`         | Disables Provenant caching                          | `Won't do`      | No longer needed because persistent caches are opt-in by default.                                         |
| `--incremental`      | Enables future incremental scan behavior            | `Rust-specific` | Beyond-parity idea; deferred until the caching model is robust.                                           |
| `--show_attribution` | Prints embedded-data attribution notices            | `Rust-specific` | Provenant-only convenience for bundled license-detection data notices.                                    |

## Key Design Decisions

1. **Compile-time features over runtime plugins** — Rust prioritizes
   compile-time optimization.
2. **Match Python CLI surface for drop-in replacement** — preserve canonical
   ScanCode option names and argument shape (especially output options).
3. **Avoid parallel output-spec APIs** — do not expose a second primary output
   selection mechanism that diverges from ScanCode usage.
4. **`--package` is opt-in** — package manifest detection is disabled by default to match ScanCode.
5. **`--no-assemble` is Rust-specific** — Python always assembles.

## Differences from Python (current intentional)

- No plugin runtime architecture (compile-time wiring instead)
- `--consolidate` is intentionally not planned because it is compatibility-oriented and upstream-deprecated
- `--todo` is intentionally not planned because it is a manual-review workflow rather than a core scan-result surface
- Thread pool via rayon instead of multiprocessing
- JSON output structure matches Python (`OUTPUT_FORMAT_VERSION`)
- `--no-cache` is not a parity requirement (upstream removed it); if retained, it is Rust-specific
- `--show_attribution` is a Rust-specific convenience flag for printing embedded-data notices

## References

- **Python CLI**:
  [`reference/scancode-toolkit/src/scancode/cli.py`](../../../reference/scancode-toolkit/src/scancode/cli.py)
- **Output plugins (reference CLI options)**:
  [`reference/scancode-toolkit/src/formattedcode/`](../../../reference/scancode-toolkit/src/formattedcode/)
- **Official ScanCode help reference**:
  https://scancode-toolkit.readthedocs.io/en/stable/reference/scancode-cli/cli-help-text-options.html
- **Official ScanCode post-scan reference**:
  https://scancode-toolkit.readthedocs.io/en/stable/reference/scancode-cli/cli-post-scan-options.html
- **Official ScanCode output-format reference**:
  https://scancode-toolkit.readthedocs.io/en/stable/reference/scancode-cli/cli-output-format-options.html
- **Clap docs**: https://docs.rs/clap/
