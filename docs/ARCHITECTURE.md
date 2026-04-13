# Provenant Architecture

## Overview

Provenant is a Rust reimplementation of [ScanCode Toolkit](https://github.com/aboutcode-org/scancode-toolkit) focused on trustworthy feature parity, explicit behavioral documentation, and targeted improvements where Rust makes the result safer or easier to maintain.

- **Strong compatibility goals**: preserve ScanCode behavior where users depend on it
- **Better performance**: native code, parallel processing, and efficient parsing
- **Enhanced security**: no code execution and explicit DoS protection
- **Intentional improvements**: document deliberate Rust-side enhancements and any remaining parity gaps clearly

See [SUPPORTED_FORMATS.md](SUPPORTED_FORMATS.md) for the full list of supported ecosystems and formats.

## Core Principles

### 1. Correctness Above All

> "always prefer correctness and full feature parity over effort/pragmatism"

- Every feature, edge case, and requirement from Python ScanCode must be preserved
- Zero tolerance for bugs - identify and fix issues from the original
- Comprehensive test coverage across unit, golden, scanner-contract, and integration layers

### 2. Security First

- **No code execution**: AST parsing only, never eval/exec
- **DoS protection**: Explicit limits on file size, recursion, iterations
- **Archive safety**: Zip bomb prevention, compression ratio validation
- **Input validation**: Robust error handling, graceful degradation

See [ADR 0004: Security-First Parsing](adr/0004-security-first-parsing.md) for details.

### 3. Extraction vs Detection Separation

**Critical separation of concerns:**

- **Parsers extract** raw data from manifests and may normalize **trustworthy declared package-license metadata**
- **Detection engines** normalize and analyze **file-content license text** and broader detection inputs

Parsers still MUST NOT:

- Run broad fuzzy license-text matching over file content
- Extract copyright holders from file content (detection engine's job)
- Backfill package declared licenses from sibling files or file detections silently

Parsers MAY populate `declared_license_expression`, `declared_license_expression_spdx`, and deterministic parser-side `license_detections` when the source field is a bounded, trustworthy declared-license surface such as an SPDX-expression-compatible manifest field.

Most package extraction in Provenant is path-owned and flows through `PackageParser` or
recognizer registration. A small set of scanner-owned exceptions can exist when the package surface
is content-aware rather than filename-aware. The current example is compiled-binary package
detection behind `--package-in-compiled`: the scanner already has the file bytes in memory, raw
executables do not have stable manifest-like filenames, and the detector must stay explicitly
bounded and opt-in.

See [ADR 0002: Extraction vs Detection Separation](adr/0002-extraction-vs-detection.md) for details.

## System Architecture Overview

### High-Level Processing Stages

Provenant follows the same broad stage model as ScanCode, but the concrete implementation is narrower in a few places. In particular, Provenant primarily scans native paths and already-extracted inputs, while some archive-aware parsers inspect their own archive formats directly instead of relying on one universal pre-scan extraction stage.

1. **Input preparation**
   - collect input paths
   - apply include/exclude rules and depth limits
   - recognize extracted layouts and parser-specific archive surfaces where applicable
2. **Scanning**
   - package manifest and package-database parsing
   - license detection
   - copyright, email, and URL extraction
3. **Post-processing**
   - package assembly (sibling, nested, file-reference, workspace)
   - summaries, tallies, classification, facets, generated-code handling
4. **Filtering and reshaping**
   - license-policy evaluation
   - include/exclude and findings-only shaping over native scans or `--from-json` inputs
5. **Output**
   - ScanCode-style JSON / JSONL / YAML / HTML
   - SPDX, CycloneDX, Debian copyright, and custom-template output

### Component Inventory

- **Package Parsers**: See [SUPPORTED_FORMATS.md](SUPPORTED_FORMATS.md) for complete list
- **Scanner Pipeline**: File discovery, parallel processing, progress tracking
- **Security Layer**: DoS protection, no code execution, archive safety
- **Package Assembly**: Sibling and nested merge strategies for combining related manifests
- **Text Detection**: License detection (n-gram matching), copyright detection (4-stage pipeline), email/URL extraction
- **Post-Processing**: Summarization, tallies, classification
- **Output Schema**: Dedicated serde-enabled types in `src/output_schema/` that define the ScanCode-compatible JSON schema, separate from internal domain types
- **Output**: JSON, JSON Lines, YAML, HTML, SPDX (TV/RDF), CycloneDX (JSON/XML), Debian copyright, and custom templates
- **Testing Infrastructure**: Doctests, unit tests, golden tests, parser-local scanner/assembly contract tests, and system integration tests
- **Infrastructure**: Caching, enhanced progress tracking, static integration points

### Implementation Status

This document stays architecture-focused. For concrete feature and support status, use:

- **[README.md](../README.md)** for user-facing features and usage
- **[SUPPORTED_FORMATS.md](SUPPORTED_FORMATS.md)** for supported formats and ecosystems
- **[TESTING_STRATEGY.md](TESTING_STRATEGY.md)** for verification and regression approach

### Plugin Architecture

Python ScanCode uses a plugin-based architecture with 5 plugin types:

1. **PreScan Plugins**: Archive extraction, file type detection
2. **Scan Plugins**: Package detection, license detection, copyright detection
3. **PostScan Plugins**: Package assembly, summarization, classification
4. **OutputFilter Plugins**: License policy filtering, custom filters
5. **Output Plugins**: Format-specific output (SPDX, CycloneDX, etc.)

Provenant keeps the same high-level stages, but wires them statically through trait-based parsers and explicit pipeline stages instead of a runtime plugin system.

## Architecture Components

### Trait-Based Parser System

**Core abstraction:** each parser exposes three durable concepts — its package type, a path-matching predicate, and an extraction entry point that returns one or more normalized `PackageData` values.

**Benefits:**

- Type-safe dispatch at compile time
- Zero runtime overhead
- Clear contract for all parsers
- Easy to test in isolation

**Implementation pattern:** parsers are usually zero-sized types with compile-time registration. The exact trait signature and helper APIs belong in code and the parser how-to guide, not in this architecture overview.

See [ADR 0001: Trait-Based Parser Architecture](adr/0001-trait-based-parsers.md) for details.

### Parser Registration System

**How parsers are wired to the scanner:**

Parsers and recognizers are registered centrally in `src/parsers/mod.rs` through the package-handler registration macro.

**What this macro generates:**

1. **`try_parse_file(path: &Path) -> Option<ParsePackagesResult>`**
   - Called by scanner for every file
   - Tries each parser's `is_match()` in order
   - Returns extracted packages plus parser diagnostics

2. **`parse_by_type_name(type_name: &str, path: &Path) -> Option<PackageData>`**
   - Used by test utilities for golden test generation
   - Allows direct parser invocation by name

3. **`list_parser_types() -> Vec<&'static str>`**
   - Returns all registered parser type names
   - Used by integration tests to verify registration

**Critical:** If a parser is implemented but not listed in this macro, it will **never be called** by the scanner, even if fully implemented and tested. Integration coverage verifies that parser registration stays aligned with the scanner entry points.

This registration path is intentionally for path-matched parsers and recognizers. Content-aware
scanner-owned package detectors, such as compiled-binary package extraction, are exceptional
surfaces wired from the scanner rather than through `register_package_handlers!`.

### Unified Data Model

All parsers output the same normalized `PackageData` shape. The durable categories in that model are:

- **identity**: package type, namespace/name/version, qualifiers, PURL, datasource IDs
- **metadata**: description, language, release/homepage information, parties, keywords
- **dependencies**: dependency edges plus scope/optionality/runtime hints
- **license metadata**: extracted statements, declared expressions, and parser-owned declared-license detections
- **provenance and references**: checksums, repository/download/API URLs, source packages, file references, and extra ecosystem-specific metadata

The field-level schema evolves over time and is owned by the Rust model definitions, not this overview.

**Rationale:**

- Normalizes differences across all supported ecosystems
- SBOM-compliant output format
- Single source of truth for structure

### Scanner Pipeline

The scanner also owns a small number of opt-in content-aware package detector paths in addition to
the normal parser/recognizer dispatch. Those paths should reuse the scanner's already-read bytes,
remain explicitly bounded, and carry their own scanner-contract and golden coverage because they do
not travel through the standard parser registry.

```text
┌────────────────────────────────────────────────────────────┐
│                      Provenant                             │
├────────────────────────────────────────────────────────────┤
│                                                            │
│  1. File Discovery           2. Parser Selection           │
│  ┌────────────────┐          ┌───────────────┐             │
│  │ Walk directory │─────────>│ Match file    │             │
│  │ Apply filters  │          │ to parser     │             │
│  └────────────────┘          └───────┬───────┘             │
│                                      │                     │
│  3. Extraction                       v                     │
│  ┌────────────────────────────────────────────┐            │
│  │ PackageParser::extract_packages()          │            │
│  │ ─ Read manifest                            │            │
│  │ ─ Parse structure                          │            │
│  │ ─ Extract metadata                         │            │
│  │ ─ Return PackageData                       │            │
│  └────────────────┬───────────────────────────┘            │
│                   │                                        │
│  4. Output        v                                        │
│  ┌─────────────────────────────────────┐                   │
│  │ Output format dispatch              │                   │
│  │ ─ JSON / YAML / JSONL               │                   │
│  │ ─ SPDX / CycloneDX / HTML / template│                   │
│  └─────────────────────────────────────┘                   │
│                                                            │
│  Detection Engines (Integrated)                            │
│  ┌───────────────────┐  ┌──────────────────┐               │
│  │ License Detection │  │ Copyright        │               │
│  │ ─ SPDX normalize  │  │ Detection        │               │
│  │ ─ Confidence      │  │ ─ Holder extract │               │
│  │ ─ Score threshold │  │ ─ Author extract │               │
│  └───────────────────┘  └──────────────────┘               │
└────────────────────────────────────────────────────────────┘
```

### Parallel Processing

The scanner uses `rayon` to process files in parallel. At a high level, each worker:

1. selects the relevant parser or recognizer for the file
2. extracts package data when applicable
3. runs enabled text-detection stages
4. records scan errors and progress for that file

**Benefits:**

- Utilizes all CPU cores
- Maintains thread safety (Rust ownership guarantees)
- Progress tracking with atomic operations

### Package Assembly System

After scanning, the assembly system merges related manifests into logical packages using `DatasourceId`-based matching.

**Assembly layers:**

- **SiblingMerge**: Combines sibling files in the same directory (e.g., `package.json` + `package-lock.json` → single npm package)
- **NestedMerge**: Combines parent/child manifests across directories (e.g., Maven parent POM + module POMs)
- **TopologyPlan**: Claims directories or multi-directory domains whose package boundaries are defined by project structure instead of plain sibling files (e.g., npm/pnpm workspaces, Cargo workspaces, `go.work`, `pixi.toml`, Hackage project roots)
- **FileRefResolve**: Resolves `file_references` from package database entries (RPM/Alpine/Debian) against scanned files, sets `for_packages` on matched files, tracks missing references, and resolves RPM namespace from os-release
- **Post-assembly passes**: Final targeted repair or enrichment steps that still need whole-scan context (for example file-reference resolution and the remaining workspace-specific finalization hooks)

**How it works:**

1. Each `AssemblerConfig` declares which `DatasourceId` variants belong together and which file patterns to look for.
2. After scanning, assembly groups package-bearing files by directory.
3. A topology-planning phase inspects parser-emitted structural hints and claims directories or multi-directory domains whose package boundaries are project-defined instead of purely sibling-defined.
4. Unclaimed directories continue through the default sibling or nested assembly paths, and combined packages aggregate `datafile_paths` and `datasource_ids` from all contributing files.
5. Claimed topology domains execute with the existing ecosystem-specific assemblers or mergers, but they do so from an explicit plan instead of first creating packages in the generic path and then repairing them later.
6. File reference resolution matches installed-package database entries to files on disk (e.g., Alpine `installed` DB lists files belonging to each package).
7. Post-assembly passes handle the remaining whole-scan cases that still need them. npm/pnpm and Cargo still finalize workspace-specific dependency/resource behavior there, but their roots and members are now planned before the generic directory loop runs.

Assembly is configurable via the `--no-assemble` CLI flag. See `src/assembly/` for implementation details.

### Security Architecture

```text
┌─────────────────────────────────────────────────────────┐
│                  Security Layers                        │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  Layer 1: No Code Execution                             │
│  ┌────────────────────────────────────────────────┐     │
│  │ AST parsing only (setup.py, build.gradle)      │     │
│  │ Never eval/exec/subprocess                     │     │
│  │ Regex/token-based for DSLs                     │     │
│  └────────────────────────────────────────────────┘     │
│                                                         │
│  Layer 2: Resource Limits                               │
│  ┌────────────────────────────────────────────────┐     │
│  │ File size: 100MB max                           │     │
│  │ Recursion depth: 50 levels                     │     │
│  │ Iterations: 100,000 max                        │     │
│  │ String length: 10MB per field                  │     │
│  └────────────────────────────────────────────────┘     │
│                                                         │
│  Layer 3: Archive Safety                                │
│  ┌────────────────────────────────────────────────┐     │
│  │ Uncompressed size: 1GB max                     │     │
│  │ Compression ratio: 100:1 max (zip bomb detect) │     │
│  │ Path traversal: Block ../ patterns             │     │
│  │ Temp cleanup: Automatic via TempDir            │     │
│  └────────────────────────────────────────────────┘     │
│                                                         │
│  Layer 4: Input Validation                              │
│  ┌────────────────────────────────────────────────┐     │
│  │ Result<T, E> error handling                    │     │
│  │ No .unwrap() in library code                   │     │
│  │ Graceful degradation on errors                 │     │
│  │ UTF-8 validation with lossy fallback           │     │
│  └────────────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────────┘
```

The exact numeric thresholds are implementation details. Treat the code and tests as the canonical source for current limits; this document focuses on the architectural safety layers they enforce.

See [ADR 0004: Security-First Parsing](adr/0004-security-first-parsing.md) for comprehensive security analysis.

## Testing Strategy

### Five-Layer Test Model

```text
              /\
             /  \   Layer 4: System integration tests
            /----\  Layer 3: Scanner/assembly contract tests
           /      \
          / Golden \ Layer 2: Golden tests
         /----------\
        /   Unit     \ Layer 1: Unit tests
       /--------------\
      /   Doctests     \ Layer 0: API documentation examples
     /__________________\
```

**Five layers** (see [TESTING_STRATEGY.md](TESTING_STRATEGY.md) for full details):

1. **Layer 0 — Doctests**: API documentation examples that run as tests
2. **Layer 1 — Unit Tests**: Component-level tests for individual functions and edge cases
3. **Layer 2 — Golden Tests**: Fixture-backed regression tests for parser and subsystem contracts
4. **Layer 3 — Scanner/Assembly Contract Tests**: parser-local tests that prove extracted data survives real scanner wiring and assembly
5. **Layer 4 — System Integration Tests**: end-to-end tests validating user-facing behavior across the full scanner pipeline

See [ADR 0003: Golden Test Strategy](adr/0003-golden-test-strategy.md) for golden test details.

## Documentation Strategy

### Three-Layer Documentation

```text
┌─────────────────────────────────────────────────────────┐
│                 Documentation Sources                   │
└─────────────────────────────────────────────────────────┘
           │                    │                  │
           ▼                    ▼                  ▼
    ┌─────────────┐     ┌──────────────┐   ┌────────────┐
    │   Parser    │     │ Doc Comments │   │   Manual   │
    │  Metadata   │     │   (/// //!)  │   │ Markdown   │
    │   (code)    │     │              │   │   Files    │
    └──────┬──────┘     └──────┬───────┘   └──────┬─────┘
           │                   │                   │
           ▼                   ▼                   ▼
    ┌─────────────┐     ┌──────────────┐   ┌────────────┐
    │ Auto-Gen    │     │  cargo doc   │   │   GitHub   │
    │ Formats.md  │     │  (docs.rs)   │   │   README   │
    └─────────────┘     └──────────────┘   └────────────┘
```

**Auto-Generated**: `docs/SUPPORTED_FORMATS.md` (from parser metadata)  
**API Reference**: cargo doc (from `///` and `//!` comments)  
**Architecture**: ADRs, improvements, guides (manual Markdown)

See [ADR 0005: Auto-Generated Documentation](adr/0005-auto-generated-docs.md) for details.

## Beyond-Parity Improvements

We don't just match Python ScanCode - we improve it:

| Parser                  | Improvement                                                                                                                  | Type                                      |
| ----------------------- | ---------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------- |
| **Alpine**              | SHA1 checksums correctly decoded + Provider field extraction                                                                 | 🐛 Bug Fix + ✨ Feature                   |
| **RPM**                 | Full dependency extraction with version constraints                                                                          | ✨ Feature                                |
| **Debian**              | .deb archive introspection                                                                                                   | ✨ Feature                                |
| **Conan**               | conanfile.txt and conan.lock parsers (Python has neither)                                                                    | ✨ Feature                                |
| **Gradle**              | No code execution (token lexer vs Groovy engine)                                                                             | 🛡️ Security                               |
| **Gradle Lockfile**     | gradle.lockfile parser (Python has no equivalent)                                                                            | ✨ Feature                                |
| **Maven**               | SCM developerConnection separation, inception_year, renamed extra_data keys for consistency                                  | 🔍 Enhanced                               |
| **npm Workspace**       | pnpm-workspace.yaml extraction + workspace assembly with per-member packages (Python has stub parser + basic assembly)       | ✨ Feature                                |
| **Cargo Workspace**     | Full `[workspace.package]` metadata inheritance + `workspace = true` dependency resolution (Python has basic assembly)       | ✨ Feature                                |
| **Composer**            | Richer provenance metadata (7 extra fields)                                                                                  | 🔍 Enhanced                               |
| **Ruby**                | Semantic party model (unified name+email)                                                                                    | 🔍 Enhanced                               |
| **Dart**                | Proper scope handling + YAML preservation                                                                                    | 🔍 Enhanced                               |
| **CPAN**                | Full metadata extraction (Python has stubs only)                                                                             | ✨ Feature                                |
| **Copyright Detection** | Year range 2099 (was 2039), regex bug fixes, type-safe POS tags, thread-safe design, Unicode preservation, encoded-data skip | 🐛 Bug Fix + 🔍 Enhanced + ⚡ Performance |
| **Assembly**            | LazyLock static assembler lookup (zero allocation per call)                                                                  | ⚡ Performance                            |

See [docs/improvements/](improvements/) for detailed documentation of each improvement.

## Project Structure

The codebase follows a modular architecture:

- **`src/parsers/`** - Package manifest parsers (one per ecosystem)
- **`src/models/`** - Core data structures (PackageData, Dependency, DatasourceId, etc.)
- **`src/output_schema/`** - ScanCode-compatible output schema types (one file per type, with serde for JSON output)
- **`src/assembly/`** - Package assembly system (merging related manifests)
- **`src/scanner/`** - File system traversal and orchestration
- **`docs/`** - Architecture decisions, improvement docs, and guides
- **`testdata/`** - Test manifests for validation
- **`reference/`** - Python ScanCode Toolkit (reference submodule)

## Performance Characteristics

### Benchmarks

For broad performance-sensitive changes, maintainers use `cargo run --manifest-path xtask/Cargo.toml --bin benchmark-target -- ...` with an explicit target (`--repo-url` or `--target-path`) to measure scanner behavior. Smaller changes usually rely on targeted regression suites plus normal scan-time profiling during development.

### Optimization Strategies

1. **Parallel Processing**: Uses all CPU cores via rayon
2. **Zero-Copy Parsing**: `&str` instead of `String` where possible
3. **Embedded License Artifact**: License loader snapshot embedded via `include_bytes!`
4. **Lazy Evaluation**: Iterators instead of eager Vec building
5. **Efficient Parsers**: quick-xml, toml, serde_json (production-grade)

### Release Optimizations

```toml
[profile.release]
lto = true                # Link-time optimization
codegen-units = 1         # Single codegen unit for max optimization
strip = true              # Strip symbols for smaller binary
opt-level = 3             # Maximum optimization
```

## Extended Architecture

The following sections describe major architectural components in detail.

### Text Detection Engines

**License Detection**:

- License text matching using fingerprinting algorithms
- SPDX license expression generation with boolean simplification of equivalent expressions
- Confidence scoring and multi-license handling
- Integration with existing SPDX license data

**Copyright Detection**:

The copyright detection engine extracts copyright statements, holder names, and author information from source files using a four-stage pipeline:

```text
┌──────────────┐    ┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│  1. Text     │───>│  2. Candidate│───>│  3. Lex +    │───>│  4. Tree     │
│  Preparation │    │  Selection   │    │  Parse       │    │  Walk +      │
│              │    │              │    │              │    │  Refinement  │
└──────────────┘    └──────────────┘    └──────────────┘    └──────────────┘
```

1. **Text Preparation**: Normalizes copyright symbols (`©`, `(c)`, HTML entities), strips comment markers and markup, preserves Unicode (no ASCII transliteration)
2. **Candidate Selection**: Filters lines using hint markers (`opyr`, `auth`, `©`, year patterns), groups multi-line statements, and skips encoded or non-promising content early
3. **Lexing + Parsing**: POS-tags tokens using an ordered pattern set (type-safe `PosTag` enum), then applies grammar rules to build parse trees identifying `COPYRIGHT`, `AUTHOR`, `NAME`, `COMPANY` structures
4. **Tree Walk + Refinement**: Extracts `CopyrightDetection`, `HolderDetection`, `AuthorDetection` from parse trees, then applies cleanup (for example unbalanced parens, duplicate "Copyright" words, and junk patterns)

Key design decisions vs Python reference:

- **Type-safe POS tags**: Enum-based (not string-based) — compiler catches tag typos
- **Thread-safe**: No global mutable state (Python uses a singleton `DETECTOR`)
- **Sequential pattern matching**: `LazyLock<Vec<(Regex, PosTag)>>` with first-match-wins semantics (RegexSet cannot preserve match order)
- **Extended year range**: 1960-2099 (Python stops at 2039)
- **Bug fixes**: Fixed year-year separator bug, short-year typo, French/Spanish case-sensitivity, duplicate patterns

Special cases handled:

- Linux CREDITS files (structured `N:/E:/W:` format)
- SPDX-FileCopyrightText and SPDX-FileContributor
- "All Rights Reserved" in English, German, French, Spanish, Dutch
- Multi-line copyright statements spanning consecutive lines

Behavioral compatibility model:

- **Default expectation**: Follow Python ScanCode behavior closely for copyright, holder, and author extraction.
- **Intentional Rust differences**: Preserve Unicode names, apply correctness bug fixes from the Python reference, and keep detection thread-safe for parallel scans.
- **Known parity gaps**: Some edge-case files still differ from Python output; these are treated as targeted follow-up work with regression tests.
- **Fixture ownership**: Copyright golden fixtures in this repository are Rust-owned expectations; Python fixtures are a reference input, not the source of truth for local expected outputs.

Migration expectation:

- Most projects should observe equivalent results to Python ScanCode.
- Where differences exist, they are either intentional improvements (for example Unicode preservation) or explicitly tracked parity gaps.

Module location: `src/copyright/`

**Email/URL Detection**:

The email/URL detection engine is the simplest text detection feature — regex-based extraction with an ordered filter pipeline to remove junk results.

```text
┌──────────────┐    ┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│  1. Read     │───>│  2. Regex    │───>│  3. Filter   │───>│  4. Yield    │
│  Lines       │    │  Match       │    │  Pipeline    │    │  Results     │
└──────────────┘    └──────────────┘    └──────────────┘    └──────────────┘
```

**Email detection**: RFC-ish regex (`[A-Z0-9._%-]+@[A-Z0-9.-]+\.[A-Z]{2,63}`) → 3-step filter pipeline (junk domain filter, uninteresting email filter, dedup).

**URL detection**: Three regex alternatives (scheme URLs, bare-domain URLs, git-style URLs) → 10-step filter pipeline:

1. CRLF cleanup → trailing junk stripping → empty URL filter → scheme addition → user/password stripping → invalid URL filter → canonicalization (via `url` crate) → junk host filter → junk URL filter → dedup

Both support configurable thresholds (`--max-email N`, `--max-url N`, default 50).

Golden regression coverage for this module uses local, repo-owned fixtures and a dedicated finder golden-test harness.

Key design decisions vs Python reference:

- **`url` crate** for URL parsing/canonicalization (replaces `urlpy`)
- **`std::net`** for IP classification (replaces `ipaddress`)
- **Extended TLD support**: `{2,63}` per RFC 1035 (Python's `{2,4}` rejects `.museum`, `.technology`)
- **Fixed IPv6 private detection**: Python has assignment bug making IPv6 private detection non-functional
- **Proper error handling**: No silent exception swallowing in URL canonicalization

Junk classification data (~150 entries): example domains, private IPs, W3C/XML namespaces, DTD URLs, PKI/certificate URLs, CDN URLs, image file suffixes.

Module location: `src/finder/`

### Post-Processing Pipeline

**Compatibility-Oriented Consolidation (Deferred)**:

- Legacy-compatible grouped package/resource view from ScanCode's `--consolidate`
- Not part of the current Provenant roadmap
- Retained only as a documented future compatibility decision, not as active architecture

**Summarization**:

- License tallies and facets
- Copyright holder aggregation
- File classification (source, docs, data, etc.)
- Summary statistics

### Output Format Support

**Internal types vs. output schema types:**

Provenant separates internal domain types from the ScanCode-compatible JSON output schema:

- **Internal types** (`src/models/`) carry domain invariants (e.g., `LineNumber` wraps `NonZeroUsize`, `Sha1Digest` validates hex length). They retain serde only for cache round-tripping and `--from-json` deserialization.
- **Output schema types** (`src/output_schema/`) are dedicated serde-enabled types that define the public JSON schema: field renames, conditional omission, type widening (`LineNumber` → `u64`, digests → `Option<String>`), and the `FileInfo` info-surface gating logic.
- **Conversion boundary** in `main.rs` converts `models::Output` → `output_schema::Output` before serialization. The `--from-json` path deserializes into output schema types and converts back via `TryFrom`.

See [ADR 0008: Output Schema Type Separation](adr/0008-output-schema-separation.md) for the full decision record.

**Implementation and parity tracking:**

- Multi-format output layer is implemented in `src/output/mod.rs`
- CLI follows ScanCode-style output flags (for example `--json-pp FILE`,
  `--spdx-tv FILE`) and dispatches through `write_output_file`
- Format compatibility is verified through fixture-backed tests and documented
  in `docs/TESTING_STRATEGY.md`

**SBOM Formats**:

- SPDX: Tag-value and RDF/XML
- CycloneDX: JSON, XML
- Compatibility with SBOM tooling ecosystem

**Additional Formats**:

- YAML (human-readable)
- HTML report
- Custom templates (user-defined formats)

#### Infrastructure Enhancements

**Plugin System**:

- No runtime plugin system is planned for Provenant
- Compile-time integration points are preferred over a public plugin ABI
- Revisit only if concrete extension needs justify the complexity

**Caching**:

Provenant uses one shared persistent cache root for the opt-in incremental manifest stored under
`incremental/`.

The cache implementation lives in `src/cache/` (`config`, `io`, `locking`, `incremental`). It
provides cache-root selection, sidecar lock coordination for cache writes/clears, incremental
manifest persistence, and atomic manifest persistence.

The intent is straightforward: repeated scans of the same checkout should reuse unchanged file
results from the last completed scan instead of rescanning the whole tree every time.

User-facing behavior is:

1. `--cache-dir` and `PROVENANT_CACHE` select the shared incremental cache root
2. `--cache-clear` clears that root before scanning
3. `--incremental` reuses unchanged file results from the last completed scan after validating stored metadata + SHA256 against the previous manifest

Custom `--license-rules-path` scans still participate in the incremental manifest workflow. A separate persistent startup snapshot cache for that advanced override is intentionally not planned.

**Progress Tracking**:

Centralized `ScanProgress` struct manages mode-aware progress output via `indicatif::MultiProgress`:

1. **Discovery phase**: Spinner/message while counting files, recording initial file/dir/size counts.
2. **SPDX load phase**: Startup message and timing capture around license DB load.
3. **Scan phase**: Main progress bar (default mode, TTY only) with ETA, elapsed time, and `{per_sec}` throughput; verbose mode keeps file-by-file paths on TTY and degrades to bounded scan lifecycle messages plus per-file warning/error context when stderr is not a TTY.
4. **Assembly and output phases**: Phase messages/spinners with timing capture.
5. **Scan summary**: Files/sec, bytes/sec, error count, initial/final counts (including sizes), package assembly counts, and per-phase timings.

Verbosity behavior is implemented in `src/progress.rs` and wired through `src/main.rs`: quiet suppresses stderr output, default shows progress/summary, and verbose stays detailed without flooding redirected logs by limiting successful per-file path output to TTY runs while still surfacing per-file warnings/errors in non-TTY environments.

Logging integration uses `indicatif-log-bridge` for startup and global warnings, while parser and other file-scoped scan failures are attached to `FileInfo.scan_errors` in `src/scanner/process.rs`. That keeps serialized output, CI logs, and the quiet/default/verbose progress modes aligned: default mode shows concise failing paths, verbose mode shows the underlying per-file error details.

Module location: `src/progress.rs`

### Quality Verification

Quality verification in this area is currently centered on:

- fixture-backed golden and integration suites
- targeted benchmark runs via `cargo run --manifest-path xtask/Cargo.toml --bin benchmark-target -- ...` when broad performance could change
- explicit parity-gap tracking in evergreen docs and completed rollout records where behavior intentionally differs from Python

## License Data Architecture

For detailed documentation of the license detection pipeline, matching algorithms, and engine components, see [LICENSE_DETECTION_ARCHITECTURE.md](LICENSE_DETECTION_ARCHITECTURE.md).

### Self-Contained Binary

The binary ships with a built-in license index embedded at compile time. This eliminates the need for external files during normal usage:

- **Embedded artifact**: `resources/license_detection/license_index.zst`
- **Format**: MessagePack-serialized, zstd-compressed `EmbeddedLoaderSnapshot` data
- **Contents**: Sorted `LoadedRule` and `LoadedLicense` values derived from the ScanCode rules dataset

### Loader/Build Stage Separation

The license detection system uses a two-stage loading process:

```text
┌─────────────────────────────────────────────────────────────────┐
│                    License Index Loading                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Loader Stage (Embedded Artifact)                               │
│  ┌────────────────────────────────────────────────────────┐     │
│  │ • Decompress and deserialize EmbeddedLoaderSnapshot    │     │
│  │ • Validate schema version                              │     │
│  │ • No runtime filesystem access to ScanCode data        │     │
│  └────────────────────────────────────────────────────────┘     │
│                           │                                     │
│                           ▼                                     │
│  Build Stage (Runtime)                                          │
│  ┌────────────────────────────────────────────────────────┐     │
│  │ • Build runtime index from embedded rules/licenses     │     │
│  │ • Apply deprecated filtering policy                    │     │
│  │ • Synthesize license-derived rules                     │     │
│  │ • Build LicenseIndex (token dict, automatons, maps)    │     │
│  │ • Build SpdxMapping                                    │     │
│  └────────────────────────────────────────────────────────┘     │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Artifact-generation responsibilities** (performed when building `license_index.zst`):

- Parse the ScanCode rules and licenses dataset
- Normalize rule/license data before embedding
- Serialize sorted `LoadedRule` / `LoadedLicense` snapshot bytes
- Compress the serialized bytes for embedding

**Loader-stage responsibilities** (runtime, file-local):

- Decompress and deserialize the embedded loader snapshot
- Reconstruct the runtime `LicenseIndex`
- Build the SPDX mapping from the reconstructed index

**Build-stage responsibilities** (cross-file policies):

- Deprecated filtering (`with_deprecated: bool`)
- License-derived rule synthesis
- Tokenization and dictionary building
- Aho-Corasick automaton construction
- SPDX key mapping

### Engine Initialization

```rust
// Default: Use embedded artifact
let engine = LicenseDetectionEngine::from_embedded()?;

// Custom rules: Load from directory
let engine = LicenseDetectionEngine::from_directory(&rules_path)?;
```

The CLI uses `from_embedded()` by default. Use `--license-rules-path` to load from a custom directory instead.

### Regenerating the Embedded Artifact

Maintainers can regenerate the embedded license artifact when the ScanCode rules dataset is updated:

```sh
# Initialize the reference submodule (if not already)
./setup.sh

# Regenerate the artifact
cargo run --manifest-path xtask/Cargo.toml --bin generate-index-artifact

# Commit the updated artifact
git add resources/license_detection/license_index.zst
git commit -m "chore: update embedded license data"
```

### Reference Dataset (Optional)

The `reference/scancode-toolkit/` submodule is **optional for end users**. It's only needed for:

1. **Developers updating embedded data**: Regenerating the compact embedded loader artifact
2. **Custom license rules**: Using `--license-rules-path` to load custom rule sets
3. **Parity testing**: Comparing Rust behavior against Python reference

Normal builds work without the submodule because the embedded artifact is checked into the repository.

## Related Documentation

- [README.md](../README.md) - User-facing overview, installation, and usage
- [AGENTS.md](../AGENTS.md) - Contributor guidelines and code style
- [ADRs](adr/) - Architectural decision records
- [Improvements](improvements/) - Beyond-parity features
- [SUPPORTED_FORMATS.md](SUPPORTED_FORMATS.md) - Complete format list (auto-generated)
