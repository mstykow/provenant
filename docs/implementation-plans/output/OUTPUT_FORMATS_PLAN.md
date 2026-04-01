# Output Formats Implementation Plan

> **Status**: Baseline parity complete; maintenance mode
> **Priority**: P1 - High Priority (User-Facing Feature)
> **Estimated Effort**: Iterative; tracked per phase/task in this document
> **Dependencies**: None for infrastructure and core format emitters

## Overview

Implement production output formats beyond ScanCode-compatible JSON: SPDX, CycloneDX, CSV, YAML, HTML, and JSON Lines.

This plan is based on:

- Python reference implementation under
  [`reference/scancode-toolkit/src/formattedcode/`](../../../reference/scancode-toolkit/src/formattedcode/)
- Rust current state in [`src/main.rs`](../../../src/main.rs),
  [`src/cli.rs`](../../../src/cli.rs), and
  [`src/models/`](../../../src/models/)

## Scope

### What This Covers

- **SPDX Tag-Value** (primary compliance target)
- **SPDX RDF/XML**
- **CycloneDX JSON**
- **CycloneDX XML**
- **CSV**
- **YAML**
- **HTML report**
- **JSON Lines**
- **HTML app** (implemented)
- **Custom templates** (implemented)

### What This Doesn't Cover

- License policy evaluation
- Output filtering plugins (`--only-findings`, etc.)
- Full Python plugin architecture parity in this work package

## Python Reference Analysis (Completed)

### Formatter Modules

- `output_json.py`: `JsonCompactOutput`, `JsonPrettyOutput`, `write_results`, `get_results`
- `output_jsonlines.py`: `JsonLinesOutput`
- `output_yaml.py`: `YamlOutput` (reuses `output_json.get_results`)
- `output_csv.py`: `CsvOutput`, `flatten_scan`, `flatten_package`
- `output_html.py`: `HtmlOutput`, `HtmlAppOutput`, `CustomTemplateOutput`
- `output_spdx.py`: `SpdxTvOutput`, `SpdxRdfOutput`, `write_spdx`
- `output_cyclonedx.py`: `CycloneDxJsonOutput`, `CycloneDxXmlOutput`, model mapping helpers

### Python Wiring Pattern

1. CLI discovers plugins (`PluginManager.load_plugins()` in `scancode/cli.py`)
2. Enabled output plugin classes are selected by option flags
3. `run_codebase_plugins(stage='output', ...)` executes `process_codebase`
4. Formatter writes directly to output file handle

### Practical parity notes

- Python CSV is explicitly marked deprecated in upstream.
- Python CycloneDX warns when package data is missing.
- Python SPDX implementation is custom-mapped and uses `spdx-tools` Python library.

## Current Capabilities

### Implemented

- ScanCode-style output flags in CLI (for example `--json FILE`,
  `--json-pp FILE`, `--spdx-tv FILE`)
- Output writer abstraction (`OutputWriter`, `OutputFormat`, dispatch)
- Main output path dispatches through `write_output_file` in
  [`src/output/mod.rs`](../../../src/output/mod.rs)
- Emitters for JSON, YAML, CSV, JSON Lines, SPDX TV/RDF, CycloneDX JSON/XML, HTML report, HTML app, custom templates
- Output schema models are complete and serializable in
  [`src/models/output.rs`](../../../src/models/output.rs) and
  [`src/models/file_info.rs`](../../../src/models/file_info.rs)
- Unit tests for each emitter and local golden fixtures for selected Python parity behaviors

### Post-parity guardrails

- Baseline parity work is complete for most formats tracked in
  [`PARITY_SCORECARD.md`](PARITY_SCORECARD.md).
- SPDX writers now consume current file/package license-info surfaces; any
  remaining SPDX drift is format-specific parity work rather than a blocker on
  missing core license-output data.
- Future work is optional hardening plus regression response unless a format is
  explicitly marked partial in the scorecard.

## Design Decisions

1. **Use output module with static dispatch, not runtime plugin system**
   - Create [`src/output/`](../../../src/output/) with explicit formatter implementations.
2. **Add explicit output format enum in CLI**
   - Keep current behavior as default JSON.
3. **Reuse existing `Output` model as canonical intermediate representation**
   - All format writers consume one normalized `Output` value.
4. **Use dedicated libraries where mature**
   - CycloneDX: `cyclonedx-bom`
   - CSV: `csv`
   - YAML: `yaml_serde`
   - HTML/templates: `tera` (for controlled templating)
5. **SPDX strategy**
   - Implement deterministic SPDX mapping layer in Rust for Tag-Value first.
   - Add RDF/XML writer in a later phase reusing same mapping model.

## Library Strategy (Current)

Current implementation uses serde/csv/tera with a modular output layer in
[`src/output/`](../../../src/output/).

Adopting dedicated SPDX/CycloneDX model crates remains optional future work,
gated by clear parity, validation, or maintainability wins.

## Implementation Status by Format

- Implemented in [`src/output/`](../../../src/output/): JSON/JSON-PP, YAML,
  CSV, JSON Lines, SPDX Tag-Value, SPDX RDF/XML, CycloneDX JSON,
  CycloneDX XML, HTML report, HTML app, and custom template output.
- Output dispatch and writer abstraction are implemented and wired through CLI
  format selection.
- Parity scope and acceptance tracking are maintained in
  [`PARITY_SCORECARD.md`](PARITY_SCORECARD.md).

## Execution Order

Execution order applies only when optional hardening or regression fixes are
scheduled, and should follow user-impact priorities from
[`PARITY_SCORECARD.md`](PARITY_SCORECARD.md).

## Testing Strategy

- **Unit tests** per formatter for mapping behavior
- **Schema/validator tests** for SPDX/CycloneDX
- **Golden tests** on stable fixture projects
- **Cross-format consistency tests** (JSON as baseline)
- **CLI integration tests** for format option routing

## Success Criteria

- Output format selection is explicit and stable in CLI.
- JSON output remains the compatibility baseline.
- SPDX and CycloneDX emitters are implemented and covered by parity-focused
  tests defined in [`PARITY_SCORECARD.md`](PARITY_SCORECARD.md).
- CSV, YAML, and JSON Lines parity is fixture-backed and maintained via golden tests.
- Output writers remain covered by unit and golden/integration tests.

## Related Documents

- [`docs/ARCHITECTURE.md`](../../ARCHITECTURE.md) (output pipeline)
- [`infrastructure/CLI_PLAN.md`](../infrastructure/CLI_PLAN.md) (format flags)
- [`output/PARITY_SCORECARD.md`](PARITY_SCORECARD.md) (format-by-format parity contract)
- Python reference modules in
  [`reference/scancode-toolkit/src/formattedcode/`](../../../reference/scancode-toolkit/src/formattedcode/)

## Notes

- SPDX and CycloneDX remain highest-value user outputs.
- The current Rust codebase is already structured around a canonical `Output` model, which makes multi-format writers straightforward to layer in.
- No blocker from license-detection-engine work is required to start this plan.
- Output implementation is split into per-format internal modules under
  [`src/output/`](../../../src/output/) with `mod.rs` as dispatch façade.
