# CLI Implementation Plan

> **Status**: 🟢 Maintained parity ledger — the ScanCode-facing CLI rollout is complete, and the residual notes below track narrow follow-up gaps rather than reopen the plan
> **Current contract owner**: [`../../CLI_GUIDE.md`](../../CLI_GUIDE.md) for evergreen user workflows and [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) for architectural placement of CLI-driven subsystems
> **Priority**: P1 - High (user-facing drop-in replacement parity)
> **Dependencies**: Some flags depend on underlying features (license detection, post-scan processing, caching)

## Overview

CLI parameter parity with Python ScanCode. Rust uses `clap`; Python uses
`click` + plugin-provided options.

This plan records the completed rollout toward a **drop-in replacement CLI surface**.

It records the implemented compatibility coverage and the final classification
of upstream flags, including explicit `Won't do` decisions for deprecated,
legacy, and intentionally out-of-scope surfaces.

Treat this file as a maintained compatibility ledger rather than the primary user-facing CLI guide.

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
| `-n, --processes`      | Controls worker count                                   | `Done` | Positive values set the worker count; `0` disables parallel file scanning; `-1` also disables timeout-backed interruption checks.                                                               |
| `--timeout`            | Sets per-file processing timeout                        | `Done` | Wired through the scanner runtime.                                                                                                                                                              |
| `--exclude / --ignore` | Excludes files by glob pattern                          | `Done` | `--ignore` is the ScanCode-facing alias.                                                                                                                                                        |
| `--include`            | Re-includes matching paths after filtering              | `Done` | Native scans now apply ScanCode-style combined include/ignore path filtering before file scanning; `--from-json` applies the same path selection as a shaping step over the loaded result tree. |
| `--strip-root`         | Rewrites paths relative to the scan root                | `Done` | Root-resource, single-file, native multi-input, nested reference, and top-level package/dependency path projection are now handled in the final shaping pass.                                   |
| `--full-root`          | Preserves absolute/rooted output paths                  | `Done` | Full-root display paths now follow the ScanCode-style formatting pass, including path cleanup and field-specific projection rules.                                                              |
| `--from-json`          | Loads prior scan JSON instead of rescanning input files | `Done` | Supports multiple input scans, shaping-time include/ignore filtering, root-flag reshaping per loaded scan before merge, and recomputation of followed top-level license outputs after load.     |

### Output Formats & Result Shaping

| Flag                                  | What it does                                           | Status     | Notes                                                                                                                                                              |
| ------------------------------------- | ------------------------------------------------------ | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `--json <FILE>`                       | Writes compact JSON output                             | `Done`     | Core output format.                                                                                                                                                |
| `--json-pp <FILE>`                    | Writes pretty-printed JSON output                      | `Done`     | Core output format.                                                                                                                                                |
| `--json-lines <FILE>`                 | Writes JSON Lines output                               | `Done`     | Core output format.                                                                                                                                                |
| `--yaml <FILE>`                       | Writes YAML output                                     | `Done`     | Core output format.                                                                                                                                                |
| `--csv <FILE>`                        | Writes CSV output                                      | `Won't do` | Removed from Provenant because it is an upstream-deprecated legacy surface rather than part of the intended current CLI offering.                                  |
| `--html <FILE>`                       | Writes HTML report output                              | `Done`     | Core output format.                                                                                                                                                |
| `--html-app <FILE>`                   | Writes the deprecated HTML app output                  | `Won't do` | Removed from Provenant because the upstream surface is deprecated and superseded by Workbench rather than part of the intended current CLI offering.               |
| `--spdx-tv <FILE>`                    | Writes SPDX tag/value output                           | `Done`     | Core output format.                                                                                                                                                |
| `--spdx-rdf <FILE>`                   | Writes SPDX RDF/XML output                             | `Done`     | Core output format.                                                                                                                                                |
| `--cyclonedx <FILE>`                  | Writes CycloneDX JSON output                           | `Done`     | Core output format.                                                                                                                                                |
| `--cyclonedx-xml <FILE>`              | Writes CycloneDX XML output                            | `Done`     | Core output format.                                                                                                                                                |
| `--custom-output <FILE>`              | Writes output using a custom template                  | `Done`     | Requires `--custom-template`.                                                                                                                                      |
| `--custom-template <FILE>`            | Supplies the template for `--custom-output`            | `Done`     | Requires `--custom-output`.                                                                                                                                        |
| `--debian <FILE>`                     | Writes Debian copyright output                         | `Done`     | Requires `--copyright`, `--license`, and `--license-text`; emits a DEP-5-style machine-readable Debian copyright document.                                         |
| `--mark-source`                       | Marks source-heavy files and directories               | `Done`     | Now requires `--info` and consumes precomputed file `is_source` state for directory marking.                                                                       |
| `--only-findings`                     | Filters output down to files with findings             | `Done`     | Implemented in scan-result shaping.                                                                                                                                |
| `--filter-clues`                      | Removes redundant clue output                          | `Done`     | Implemented in scan-result shaping with exact dedupe, rule-based ignorable clue suppression, and clue-aware downstream license tallies/summary handling.           |
| `-i, --info`                          | Gates file-info output and related info-only workflows | `Done`     | Native scans now gate the ScanCode-style info surface, including checksums, `sha1_git`, MIME/file-type classification hints, and `is_source` / file-kind booleans. |
| `--ignore-author <pattern>`           | Filters author findings by pattern                     | `Done`     | Implemented as a whole-resource shaping filter.                                                                                                                    |
| `--ignore-copyright-holder <pattern>` | Filters copyright-holder findings by pattern           | `Done`     | Implemented as a whole-resource shaping filter.                                                                                                                    |

### Residual `--info` / file-info parity gaps

Confirmed remaining gaps:

- `mime_type` now uses a broader pure-Rust `file-format` reader set plus
  targeted overrides and `mime_guess` fallback, but it still does not match the
  breadth or exact strings of libmagic-backed content typing.
- `file_type` now benefits from the same expanded pure-Rust detector set and
  more specific text/configuration labels, but it remains a local approximation
  rather than the raw libmagic descriptions ScanCode exposes.
- `programming_language`, `is_source`, `is_script`, `is_binary`, and
  `is_text` now cover more real manifest/source filenames and extensions (for
  example Gradle, Nix, Bazel/Starlark, PowerShell, and JavaScript shebang
  scripts), but they still reflect Provenant-owned heuristics rather than
  typecode/Pygments/libmagic behavior, especially for ambiguous extensionless
  text files.
- Archive/media classification and text-extraction suppression now cover more
  package/archive/media containers directly from detected format information,
  but they still lack typecode's broader extractability-driven distinctions and
  libmagic's long-tail format coverage.

### Scan & Detection Controls

| Flag                         | What it does                                        | Status     | Notes                                                                                                                                                                                               |
| ---------------------------- | --------------------------------------------------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `-l, --license`              | Enables license scanning                            | `Done`     | The toggle exists; broader license-output parity is tracked in [`LICENSE_DETECTION_PLAN.md`](../text-detection/LICENSE_DETECTION_PLAN.md).                                                          |
| `--license-rules-path`       | Loads extra license rules from disk                 | `Done`     | Requires `--license`. Kept as an advanced maintainer/expert override for custom rule-set validation and parity work; not the default end-user path.                                                 |
| `--license-score`            | Filters or reports by license score thresholds      | `Done`     | Requires `--license`; native scans now filter returned license detections and clue-bearing matches by a minimum score from `0` to `100`, and the threshold participates in scan-cache fingerprints. |
| `--license-text`             | Emits matched license text                          | `Done`     | Requires `--license`; file/package matches now carry `matched_text` under the upstream flag name.                                                                                                   |
| `--is-license-text`          | Legacy helper for license-text percentage reporting | `Won't do` | Removed upstream. Current ScanCode behavior is built into `--license-text` / `--info` and exposed through the emitted `percentage_of_license_text` field instead of a separate flag.                |
| `--license-text-diagnostics` | Emits detailed license-text diagnostics             | `Done`     | Requires `--license-text`; match output now includes `matched_text_diagnostics`.                                                                                                                    |
| `--license-diagnostics`      | Emits detailed license-match diagnostics            | `Done`     | Requires `--license`; file/package detections now include `detection_log` when enabled.                                                                                                             |
| `--unknown-licenses`         | Reports unknown-license detections                  | `Done`     | Requires `--license`; wired through to the license engine's unknown-license pass.                                                                                                                   |
| `--license-url-template`     | Customizes license reference URLs                   | `Done`     | Requires `--license`; native top-level `license_references[].licensedb_url` now uses the configured template when references are generated or recomputed.                                           |
| `-c, --copyright`            | Enables copyright, holder, and author detection     | `Done`     | Core scan toggle.                                                                                                                                                                                   |
| `-e, --email`                | Enables email detection                             | `Done`     | Core scan toggle.                                                                                                                                                                                   |
| `--max-email`                | Caps email findings per file                        | `Done`     | Requires `--email`; `0` means no limit.                                                                                                                                                             |
| `-u, --url`                  | Enables URL detection                               | `Done`     | Core scan toggle.                                                                                                                                                                                   |
| `--max-url`                  | Caps URL findings per file                          | `Done`     | Requires `--url`; `0` means no limit.                                                                                                                                                               |
| `-p, --package`              | Enables package manifest and lockfile scanning      | `Done`     | Core scan toggle.                                                                                                                                                                                   |
| `--generated`                | Detects and reports generated files during scanning | `Done`     | Implemented and verified through the completed summarization/generation rollout.                                                                                                                    |
| `--facet <facet>=<pattern>`  | Assigns files to facets such as `core` or `tests`   | `Done`     | Implemented and verified through the completed summarization/facet rollout.                                                                                                                         |

### Post-Scan Analysis & Reporting

| Flag                      | What it does                                                                   | Status     | Notes                                                                                                                                                                                                        |
| ------------------------- | ------------------------------------------------------------------------------ | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `--classify`              | Classifies key files and related project-level signals                         | `Done`     | Implemented and covered by the completed summarization/classification rollout.                                                                                                                               |
| `--summary`               | Emits top-level project summary output                                         | `Done`     | Requires `--classify`; implemented and covered by the completed summarization rollout.                                                                                                                       |
| `--license-clarity-score` | Emits project-level license clarity scoring                                    | `Done`     | Requires `--classify`; implemented and covered by the completed summarization rollout.                                                                                                                       |
| `--tallies`               | Emits top-level tallies                                                        | `Done`     | Implemented and covered by the completed summarization/tallies rollout.                                                                                                                                      |
| `--tallies-with-details`  | Emits per-resource tallies                                                     | `Done`     | Implemented for file and directory resources.                                                                                                                                                                |
| `--tallies-key-files`     | Emits tallies for key files only                                               | `Done`     | Requires `--classify` and `--tallies`; implemented and kept aligned with package/manifest evidence.                                                                                                          |
| `--tallies-by-facet`      | Emits tallies split by facet                                                   | `Done`     | Requires `--facet` and `--tallies`; implemented and covered by the completed facet/tallies rollout.                                                                                                          |
| `--license-references`    | Emits top-level license reference blocks                                       | `Done`     | Requires `--license`; native scans now generate richer runtime-backed `license_references` / `license_rule_references` from the final post-follow state, and `--from-json` recomputes them when appropriate. |
| `--license-policy`        | Evaluates findings against a license policy                                    | `Done`     | Loads an upstream-style YAML license policy file and populates per-file `license_policy` matches from detected license keys.                                                                                 |
| `--is-generated`          | Reports percentage/license-text-style generated indicators in post-scan output | `Won't do` | Not part of the current upstream ScanCode CLI surface; current parity uses the live `--generated` flag and emitted `is_generated` field instead.                                                             |
| `--timing`                | Emits per-resource scan timing details                                         | `Won't do` | Not part of the current upstream ScanCode CLI surface. Current parity tracks the live `--timeout` control plus scan header timestamps instead of a separate timing flag.                                     |
| `--consolidate`           | Emits the legacy consolidated package/component view                           | `Won't do` | Intentionally out of scope; see [`CONSOLIDATION_PLAN.md`](../post-processing/CONSOLIDATION_PLAN.md).                                                                                                         |
| `--todo`                  | Emits manual-review TODO workflow output                                       | `Won't do` | Intentionally out of scope; see [`SUMMARIZATION_PLAN.md`](../post-processing/SUMMARIZATION_PLAN.md).                                                                                                         |

### Package, Compatibility & Meta Commands

| Flag                                                 | What it does                                                     | Status     | Notes                                                                                                                                                                                                                                               |
| ---------------------------------------------------- | ---------------------------------------------------------------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--system-package`                                   | Enables system-package style package detection                   | `Done`     | Restricts package scanning to installed system-package sources such as dpkg, RPM, yumdb, Alpine installed DBs, and similar datasource-backed package databases.                                                                                     |
| `--package-in-compiled`                              | Extracts package metadata from compiled artifacts                | `Done`     | Adds native compiled-binary package extraction for current Go binaries (Go build-info blobs) and Rust binaries with cargo-auditable `.dep-v0` sections, while regular archive package handlers remain part of normal `--package` scanning.          |
| `--package-only`                                     | Scans only for package data and skips top-level package creation | `Done`     | Conflicts with the upstream-incompatible mixes `--license`, `--summary`, `--package`, and `--system-package`; scans for system and application package data, skips copyright detection, and does **not** implicitly enable `--package-in-compiled`. |
| `--list-packages`                                    | Lists supported package handlers or package-related surfaces     | `Won't do` | Inventory/meta surface rather than core scan-output functionality.                                                                                                                                                                                  |
| `-A, --about`                                        | Prints about/help text beyond normal help output                 | `Won't do` | Meta/help surface, not part of core scan-result functionality.                                                                                                                                                                                      |
| `--examples`                                         | Prints usage examples                                            | `Won't do` | Meta/help surface, not part of core scan-result functionality.                                                                                                                                                                                      |
| `--plugins`                                          | Lists plugin surfaces                                            | `Won't do` | Provenant intentionally does not plan a runtime plugin system; see [`PLUGIN_SYSTEM_PLAN.md`](PLUGIN_SYSTEM_PLAN.md).                                                                                                                                |
| `--print-options`                                    | Prints option inventory metadata                                 | `Won't do` | Meta/help surface, not part of core scan-result functionality.                                                                                                                                                                                      |
| `--keep-temp-files`                                  | Preserves temporary debug files                                  | `Won't do` | Hidden debugging/housekeeping flag, not part of the intended CLI surface.                                                                                                                                                                           |
| `--check-version / --no-check-version`               | Controls upstream version-check behavior                         | `Won't do` | Hidden update-check convenience, not part of scan/output parity.                                                                                                                                                                                    |
| `--test-mode / --test-slow-mode / --test-error-mode` | Exposes upstream internal test-harness modes                     | `Won't do` | Hidden test-only flags, not part of the intended CLI surface.                                                                                                                                                                                       |

### Cache & Rust-Specific Extras

| Flag                 | What it does                                        | Status          | Notes                                                                                                                                                                                                                                                                                 |
| -------------------- | --------------------------------------------------- | --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--cache-dir`        | Chooses the shared incremental cache root           | `Done`          | Root selector only; does not enable incremental reuse by itself.                                                                                                                                                                                                                      |
| `--cache-clear`      | Clears the selected incremental cache root          | `Done`          | Clears incremental cache state before scanning without implicitly enabling reuse.                                                                                                                                                                                                     |
| `--max-in-memory`    | Caps in-memory scan buffering before spill behavior | `Done`          | Matches the current ScanCode-facing CLI contract: default `10000`, `0` for unlimited memory, `-1` for disk-only scan-detail spill, and bounded in-run spill of processed file details before post-scan output assembly. The limit is count-based (files/directories), not byte-based. |
| `--no-assemble`      | Skips package assembly after manifest detection     | `Rust-specific` | Provenant-only convenience; Python ScanCode always assembles.                                                                                                                                                                                                                         |
| `--no-cache`         | Disables Provenant caching                          | `Won't do`      | No longer needed because incremental reuse is opt-in by default.                                                                                                                                                                                                                      |
| `--incremental`      | Enables unchanged-file reuse on repeated scans      | `Rust-specific` | Beyond-parity feature; kept as the sole repeated-run reuse mechanism.                                                                                                                                                                                                                 |
| `--show-attribution` | Prints embedded-data attribution notices            | `Rust-specific` | Provenant-only convenience for bundled license-detection data notices.                                                                                                                                                                                                                |

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
- `--csv` and `--html-app` are intentionally not offered because they are upstream-deprecated legacy output surfaces
- Thread pool via rayon instead of multiprocessing
- JSON output structure matches Python (`OUTPUT_FORMAT_VERSION`)
- `--no-cache` is not a parity requirement (upstream removed it); if retained, it is Rust-specific
- `--show-attribution` is a Rust-specific convenience flag for printing embedded-data notices

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
