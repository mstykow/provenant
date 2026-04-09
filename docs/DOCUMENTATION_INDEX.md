# Documentation Index

This index helps you find the right documentation for your needs.

## For Users

- **[README.md](../README.md)** - Installation, usage, and quick start
- **[CLI_GUIDE.md](CLI_GUIDE.md)** - Command-line workflows and important flag combinations
- **[SCANCODE_COMPARISON.md](SCANCODE_COMPARISON.md)** - Provenant's relationship to ScanCode Toolkit and high-level comparison notes
- **[BENCHMARKS.md](BENCHMARKS.md)** - Maintained package-detection compare-run record, timing methodology, and Provenant-vs-ScanCode outcomes
- **[SUPPORTED_FORMATS.md](SUPPORTED_FORMATS.md)** - Generated support matrix for package and package-adjacent detection surfaces
- **[NOTICE](../NOTICE)** - Upstream attribution and licensing details for included ScanCode-derived materials
- **[ACKNOWLEDGEMENTS.md](../ACKNOWLEDGEMENTS.md)** - Project support and acknowledgements, including employer and infrastructure support
- **[SECURITY.md](../SECURITY.md)** - Security reporting guidance

## For Contributors

### Getting Started

- **[ARCHITECTURE.md](ARCHITECTURE.md)** - System design and components
- **[LICENSE_DETECTION_ARCHITECTURE.md](LICENSE_DETECTION_ARCHITECTURE.md)** - Detailed license-detection engine architecture and rule-loading flow
- **[HOW_TO_ADD_A_PARSER.md](HOW_TO_ADD_A_PARSER.md)** - Step-by-step parser implementation guide
- **[TESTING_STRATEGY.md](TESTING_STRATEGY.md)** - Five-layer testing approach

### Design Decisions

- **[adr/README.md](adr/README.md)** - Architectural Decision Records index and guidance

### Beyond-Parity Features

- **[improvements/README.md](improvements/README.md)** - Beyond-parity improvements index and per-area links

## For Maintainers

- **[RELEASING.md](RELEASING.md)** - Release prerequisites, workflow, and verification steps
- **[implementation-plans/README.md](implementation-plans/README.md)** - Active plans, historical rollout records, deferred scope records, and canonical exceptions

### Document Organization

```text
docs/
├── SCANCODE_COMPARISON.md             # Evergreen: Provenant vs. ScanCode positioning
├── BENCHMARKS.md                      # Evergreen: Benchmark methodology and recorded compare runs
├── CLI_GUIDE.md                       # Evergreen: User-facing CLI workflows
├── ARCHITECTURE.md                    # Evergreen: System design
├── LICENSE_DETECTION_ARCHITECTURE.md  # Evergreen: License-detection subsystem
├── RELEASING.md                       # Evergreen: Maintainer release process
├── HOW_TO_ADD_A_PARSER.md             # Evergreen: Parser guide
├── TESTING_STRATEGY.md                # Evergreen: Testing philosophy
├── SUPPORTED_FORMATS.md               # Generated: CI-checked support matrix
├── DOCUMENTATION_INDEX.md             # This file
│
├── adr/                               # Historical decision records + current-contract notes
│
├── improvements/                      # Evergreen: Beyond-parity features
│
└── implementation-plans/              # Mixed: active plans, historical rollout docs, deferred scope
```

## Quick Links by Task

### I want to

**...understand the overall architecture**
→ [ARCHITECTURE.md](ARCHITECTURE.md)

**...understand license detection internals**
→ [LICENSE_DETECTION_ARCHITECTURE.md](LICENSE_DETECTION_ARCHITECTURE.md)

**...add a new package parser**
→ [HOW_TO_ADD_A_PARSER.md](HOW_TO_ADD_A_PARSER.md)

**...understand testing strategy**
→ [TESTING_STRATEGY.md](TESTING_STRATEGY.md)

**...see what formats are supported**
→ [SUPPORTED_FORMATS.md](SUPPORTED_FORMATS.md)

**...figure out which document currently owns a topic**
→ [implementation-plans/README.md](implementation-plans/README.md) for active vs historical plan status, then follow the linked evergreen owner document where one is listed

**...learn CLI usage and flag combinations**
→ [CLI_GUIDE.md](CLI_GUIDE.md)

**...understand Provenant's relationship to ScanCode Toolkit**
→ [SCANCODE_COMPARISON.md](SCANCODE_COMPARISON.md)

**...review upstream attribution or the code/data licensing split**
→ [NOTICE](../NOTICE)

**...review project support and acknowledgements**
→ [ACKNOWLEDGEMENTS.md](../ACKNOWLEDGEMENTS.md)

**...review security reporting guidance**
→ [SECURITY.md](../SECURITY.md)

**...understand a design decision**
→ [adr/README.md](adr/README.md)

**...see where Rust exceeds Python**
→ [improvements/README.md](improvements/README.md)

**...track implementation quality and behavior**
→ [TESTING_STRATEGY.md](TESTING_STRATEGY.md) for testing philosophy, plus [BENCHMARKS.md](BENCHMARKS.md) for the canonical package-detection verification record, compare-run timing references, and maintained Provenant-vs-ScanCode outcomes

**...cut a release**
→ [RELEASING.md](RELEASING.md)

## Document Lifecycle

### Evergreen Documents (Permanent)

- **ARCHITECTURE.md** - Updated as architecture evolves
- **CLI_GUIDE.md** - Updated as the public CLI workflows evolve
- **SCANCODE_COMPARISON.md** - Updated as positioning, trust model, or comparison guidance evolves
- **BENCHMARKS.md** - Updated as maintained benchmark examples and methodology evolve
- **LICENSE_DETECTION_ARCHITECTURE.md** - Updated as the license-detection subsystem evolves
- **RELEASING.md** - Updated as the release workflow changes
- **HOW_TO_ADD_A_PARSER.md** - Updated as parser patterns change
- **TESTING_STRATEGY.md** - Updated as testing approach evolves
- **SUPPORTED_FORMATS.md** - Auto-generated and CI-checked for drift
- **adr/README.md** - ADR index; accepted ADRs are historical decision records and may receive limited maintenance notes to prevent broken or misleading references
- **improvements/README.md** - Landing page for beyond-parity improvement documents
- **implementation-plans/README.md** - Directory map for active plans, historical rollout records, deferred scope records, and canonical exceptions

### Canonical Ownership Rules

- **Current user-facing CLI behavior** lives in `README.md` and `CLI_GUIDE.md`.
- **Current architecture and maintainer contracts** live in evergreen docs such as `ARCHITECTURE.md`, `LICENSE_DETECTION_ARCHITECTURE.md`, `HOW_TO_ADD_A_PARSER.md`, and `TESTING_STRATEGY.md`.
- **Generated support coverage** lives in `SUPPORTED_FORMATS.md`.
- **Historical rationale** lives in `adr/`.
- **Active plans, completed rollout records, and deferred scope decisions** live in `implementation-plans/`; those documents are non-canonical unless they explicitly identify themselves as a maintained reference or checklist.

## Contributing

When adding documentation:

1. **Evergreen docs** go in `docs/` root or subdirectories (`adr/`, `improvements/`)
2. **ADRs** are historical records - create new ADRs for substantive decision changes, but allow narrowly scoped maintenance notes or link fixes that prevent stale guidance
3. **Beyond-parity features** get documented in `improvements/` with examples
4. **Auto-generated docs** (like `SUPPORTED_FORMATS.md`) should not be edited manually

## Maintenance

- **SUPPORTED_FORMATS.md**: Regenerate with `cargo run --manifest-path xtask/Cargo.toml --bin generate-supported-formats` and keep it passing `-- --check` in CI
- **ADRs**: Add new ADRs for significant design decisions
- **Improvements**: Document beyond-parity features as they're implemented
