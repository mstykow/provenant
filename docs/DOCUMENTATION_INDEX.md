# Documentation Index

This index helps you find the right documentation for your needs.

## For Users

- **[README.md](../README.md)** - Installation, usage, and quick start
- **[CLI_GUIDE.md](CLI_GUIDE.md)** - Command-line workflows and important flag combinations
- **[SUPPORTED_FORMATS.md](SUPPORTED_FORMATS.md)** - List of all supported package formats

## For Contributors

### Getting Started

- **[ARCHITECTURE.md](ARCHITECTURE.md)** - System design and components
- **[LICENSE_DETECTION_ARCHITECTURE.md](LICENSE_DETECTION_ARCHITECTURE.md)** - Detailed license-detection engine architecture and rule-loading flow
- **[HOW_TO_ADD_A_PARSER.md](HOW_TO_ADD_A_PARSER.md)** - Step-by-step parser implementation guide
- **[TESTING_STRATEGY.md](TESTING_STRATEGY.md)** - Four-layer testing approach

### Design Decisions

- **[adr/README.md](adr/README.md)** - Architectural Decision Records index and guidance

### Beyond-Parity Features

- **[improvements/README.md](improvements/README.md)** - Beyond-parity improvements index and per-area links

## For Maintainers

- **[RELEASING.md](RELEASING.md)** - Release prerequisites, workflow, and verification steps

### Document Organization

```text
docs/
├── CLI_GUIDE.md                      # Evergreen: User-facing CLI workflows
├── ARCHITECTURE.md                    # Evergreen: System design
├── LICENSE_DETECTION_ARCHITECTURE.md  # Evergreen: License-detection subsystem
├── RELEASING.md                       # Evergreen: Maintainer release process
├── HOW_TO_ADD_A_PARSER.md            # Evergreen: Parser guide
├── TESTING_STRATEGY.md                # Evergreen: Testing philosophy
├── SUPPORTED_FORMATS.md               # Evergreen: Auto-generated format list
├── DOCUMENTATION_INDEX.md             # This file
│
├── adr/                               # Evergreen: Design decisions
│
└── improvements/                      # Evergreen: Beyond-parity features
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

**...learn CLI usage and flag combinations**
→ [CLI_GUIDE.md](CLI_GUIDE.md)

**...understand a design decision**
→ [adr/README.md](adr/README.md)

**...see where Rust exceeds Python**
→ [improvements/README.md](improvements/README.md)

**...track implementation quality and behavior**
→ [TESTING_STRATEGY.md](TESTING_STRATEGY.md)

**...cut a release**
→ [RELEASING.md](RELEASING.md)

## Document Lifecycle

### Evergreen Documents (Permanent)

- **ARCHITECTURE.md** - Updated as architecture evolves
- **CLI_GUIDE.md** - Updated as the public CLI workflows evolve
- **LICENSE_DETECTION_ARCHITECTURE.md** - Updated as the license-detection subsystem evolves
- **RELEASING.md** - Updated as the release workflow changes
- **HOW_TO_ADD_A_PARSER.md** - Updated as parser patterns change
- **TESTING_STRATEGY.md** - Updated as testing approach evolves
- **SUPPORTED_FORMATS.md** - Auto-generated, always current
- **adr/README.md** - ADR index; individual ADRs are immutable once written
- **improvements/README.md** - Landing page for beyond-parity improvement documents

## Contributing

When adding documentation:

1. **Evergreen docs** go in `docs/` root or subdirectories (`adr/`, `improvements/`)
2. **ADRs** are immutable - create new ADRs instead of editing old ones
3. **Beyond-parity features** get documented in `improvements/` with examples
4. **Auto-generated docs** (like `SUPPORTED_FORMATS.md`) should not be edited manually

## Maintenance

- **SUPPORTED_FORMATS.md**: Regenerate with `cargo run --manifest-path xtask/Cargo.toml --bin generate-supported-formats`
- **ADRs**: Add new ADRs for significant design decisions
- **Improvements**: Document beyond-parity features as they're implemented
