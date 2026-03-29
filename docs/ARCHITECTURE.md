# Provenant Architecture

## Overview

Provenant is a complete rewrite of [ScanCode Toolkit](https://github.com/aboutcode-org/scancode-toolkit) in Rust, designed as a **drop-in replacement** with all features of the original, but with:

- **Zero bugs**: Leveraging Rust's type system and ownership model
- **Better performance**: Native code, parallel processing, zero-copy parsing
- **Enhanced security**: No code execution, comprehensive DoS protection
- **Feature parity or better**: 100% compatibility plus intentional improvements

See [SUPPORTED_FORMATS.md](SUPPORTED_FORMATS.md) for the full list of supported ecosystems and formats.

## Core Principles

### 1. Correctness Above All

> "always prefer correctness and full feature parity over effort/pragmatism"

- Every feature, edge case, and requirement from Python ScanCode must be preserved
- Zero tolerance for bugs - identify and fix issues from the original
- Comprehensive test coverage (unit + golden tests against Python reference)

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

See [ADR 0002: Extraction vs Detection Separation](adr/0002-extraction-vs-detection.md) for details.

## System Architecture Overview

### Complete Processing Pipeline

Provenant implements a multi-phase processing pipeline based on Python ScanCode's architecture:

```text
┌─────────────────────────────────────────────────────────────────┐
│                    ScanCode Processing Pipeline                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Phase 1: Pre-Scan                                              │
│  ┌────────────────────────────────────────────────────────┐     │
│  │ • Archive extraction                                    │    │
│  │ • File type detection                                   │    │
│  │ • Pre-processing hooks                                  │    │
│  └────────────────────────────────────────────────────────┘     │
│                           │                                     │
│                           ▼                                     │
│  Phase 2: Scanning                                              │
│  ┌────────────────────────────────────────────────────────┐     │
│  │ • Package manifest parsing (see SUPPORTED_FORMATS.md)  |     │
│  │ • License text detection                               |     │
│  │ • Copyright detection                                  |     │
│  │ • Email/URL extraction                                 |     │
│  └────────────────────────────────────────────────────────┘     │
│                           │                                     │
│                           ▼                                     │
│  Phase 3: Post-Processing                                       │
│  ┌──────────────────────────────────────────────────────────┐   |
│  │ • Package assembly (sibling, nested, file-ref, workspace)│   │
│  │ • Summary, tallies, classification, facets              │   │
│  │ • License/copyright summarization                        │   │
│  │ • Generated-code and key-file analysis                   │   │
│  └──────────────────────────────────────────────────────────┘   │
│                           │                                     │
│                           ▼                                     │
│  Phase 4: Filtering                                             │
│  ┌────────────────────────────────────────────────────────┐     │
│  │ • License policy filtering                             │     │
│  │ • Policy/filtering rules                               │     │
│  └────────────────────────────────────────────────────────┘     │
│                           │                                     │
│                           ▼                                     │
│  Phase 5: Output                                                │
│  ┌────────────────────────────────────────────────────────┐     │
│  │ • JSON output (ScanCode-compatible)                    │     │
│  │ • SPDX, CycloneDX, CSV, YAML, HTML, JSONL              │     │
│  │ • HTML app and custom templates                        │     │
│  └────────────────────────────────────────────────────────┘     │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Component Inventory

- **Package Parsers**: See [SUPPORTED_FORMATS.md](SUPPORTED_FORMATS.md) for complete list
- **Scanner Pipeline**: File discovery, parallel processing, progress tracking
- **Security Layer**: DoS protection, no code execution, archive safety
- **Package Assembly**: Sibling and nested merge strategies for combining related manifests
- **Text Detection**: License detection (n-gram matching), copyright detection (4-stage pipeline), email/URL extraction
- **Post-Processing**: Summarization, tallies, classification
- **Output**: JSON, SPDX (TV/RDF), CycloneDX (JSON/XML), CSV, YAML, JSON Lines, HTML, HTML app, custom templates
- **Testing Infrastructure**: Unit tests, doctests, golden tests, integration tests
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

**Core Abstraction:**

```rust
pub trait PackageParser {
    const PACKAGE_TYPE: &'static str;

    fn is_match(path: &Path) -> bool;
    fn extract_packages(path: &Path) -> Vec<PackageData>;
}
```

**Benefits:**

- Type-safe dispatch at compile time
- Zero runtime overhead
- Clear contract for all parsers
- Easy to test in isolation

**Implementation:**

```rust
pub struct NpmParser;

impl PackageParser for NpmParser {
    const PACKAGE_TYPE: &'static str = "npm";

    fn is_match(path: &Path) -> bool {
        matches!(
            path.file_name().and_then(|n| n.to_str()),
            Some("package.json" | "package-lock.json")
        )
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        // Implementation
    }
}
```

See [ADR 0001: Trait-Based Parser Architecture](adr/0001-trait-based-parsers.md) for details.

### Parser Registration System

**How parsers are wired to the scanner:**

Parsers and recognizers are registered via the `register_package_handlers!` macro in `src/parsers/mod.rs`:

```rust
register_package_handlers! {
    parsers: [
        NpmParser,
        NpmLockParser,
        CargoParser,
        CargoLockParser,
        // ... more parsers ...
    ],
    recognizers: [
        JavaJarRecognizer,
        // ... file-type recognizers ...
    ],
}
```

**What this macro generates:**

1. **`try_parse_file(path: &Path) -> Option<Vec<PackageData>>`**
   - Called by scanner for every file
   - Tries each parser's `is_match()` in order
   - Returns first match's extracted data

2. **`parse_by_type_name(type_name: &str, path: &Path) -> Option<PackageData>`**
   - Used by test utilities for golden test generation
   - Allows direct parser invocation by name

3. **`list_parser_types() -> Vec<&'static str>`**
   - Returns all registered parser type names
   - Used by integration tests to verify registration

**Critical:** If a parser is implemented but not listed in this macro, it will **never be called** by the scanner, even if fully implemented and tested. Integration coverage verifies that parser registration stays aligned with the scanner entry points.

### Unified Data Model

All parsers output a single `PackageData` struct:

```rust
pub struct PackageData {
    // Identity
    pub package_type: Option<String>,
    pub namespace: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub purl: Option<String>,
    pub datasource_id: Option<DatasourceId>,

    // Metadata
    pub description: Option<String>,
    pub primary_language: Option<String>,
    pub release_date: Option<String>,
    pub homepage_url: Option<String>,
    pub parties: Vec<Party>,
    pub keywords: Vec<String>,

    // Dependencies
    pub dependencies: Vec<Dependency>,

    // Licenses (extraction only - detection is separate)
    pub extracted_license_statement: Option<String>,
    pub declared_license_expression: Option<String>,
    pub license_detections: Vec<LicenseDetection>,

    // Checksums & URLs
    pub sha1: Option<String>,
    pub md5: Option<String>,
    pub sha256: Option<String>,
    pub sha512: Option<String>,
    pub download_url: Option<String>,
    pub vcs_url: Option<String>,
    pub repository_homepage_url: Option<String>,
    pub repository_download_url: Option<String>,
    pub api_data_url: Option<String>,

    // Additional data
    pub extra_data: Option<HashMap<String, serde_json::Value>>,
    pub source_packages: Vec<String>,
    pub file_references: Vec<FileReference>,
    pub is_private: bool,
    pub is_virtual: bool,
    // ... and more (see src/models/file_info.rs for complete definition)
}
```

**Rationale:**

- Normalizes differences across all supported ecosystems
- SBOM-compliant output format
- Single source of truth for structure

### Scanner Pipeline

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
│  │ ─ JSON / YAML / CSV / JSONL         │                   │
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

Uses `rayon` for multi-threaded file scanning:

```rust
// Actual implementation in src/scanner/process.rs
files.par_iter()
    .map(|(path, metadata)| {
        // Each file processed in parallel
        let file_entry = process_file(path, metadata, scan_strategy);
        progress.file_completed(path, metadata.len(), &file_entry.scan_errors);
        file_entry
    })
    .collect()
```

Inside `process_file()`, the scanner calls `try_parse_file(path)` (generated by `register_package_handlers!` macro), then runs license detection plus enabled text-detection options on UTF-8 content:

```rust
// src/scanner/process.rs — simplified flow
if let Some(package_data) = try_parse_file(path) {
    file_info_builder.package_data(package_data);
}
extract_license_information(&mut file_info_builder, text_content, scan_strategy)?;
if text_options.detect_copyrights {
    extract_copyright_information(&mut file_info_builder, path, &text_content);
}
extract_email_url_information(&mut file_info_builder, &text_content, text_options);
```

**Benefits:**

- Utilizes all CPU cores
- Maintains thread safety (Rust ownership guarantees)
- Progress tracking with atomic operations

### Package Assembly System

After scanning, the assembly system merges related manifests into logical packages using `DatasourceId`-based matching.

**Four assembly passes:**

- **SiblingMerge**: Combines sibling files in the same directory (e.g., `package.json` + `package-lock.json` → single npm package)
- **NestedMerge**: Combines parent/child manifests across directories (e.g., Maven parent POM + module POMs)
- **FileRefResolve**: Resolves `file_references` from package database entries (RPM/Alpine/Debian) against scanned files, sets `for_packages` on matched files, tracks missing references, and resolves RPM namespace from os-release
- **WorkspaceMerge**: Post-processing pass for monorepo workspaces (e.g., npm/pnpm/Cargo workspaces → separate Package per workspace member with shared resource assignment and `workspace:*` version resolution)

**How it works:**

1. Each `AssemblerConfig` declares which `DatasourceId` variants belong together and which file patterns to look for
2. After scanning, the assembler groups packages by directory
3. Packages whose `datasource_id` values match the same config are merged into a single logical package
4. Combined packages aggregate `datafile_paths` and `datasource_ids` from all contributing files
5. File reference resolution matches installed-package database entries to files on disk (e.g., Alpine `installed` DB lists files belonging to each package)
6. Workspace assembly runs as a final pass: detects workspace roots (npm/pnpm/Cargo), discovers members via glob patterns, creates per-member packages with full metadata inheritance (Cargo `[workspace.package]` and `workspace = true` resolution), hoists dependencies, and resolves `workspace:*` version references

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

See [ADR 0004: Security-First Parsing](adr/0004-security-first-parsing.md) for comprehensive security analysis.

## Testing Strategy

### Four-Layer Test Pyramid

```text
          /\
         /  \    Integration Tests
        /    \   ─ End-to-end scanner pipeline
       /------\  ─ Full scan validation
      /        \
     / Golden   \ Golden Tests
    /  Tests     \ ─ Compare with Python ScanCode output
   /--------------\ ─ Real-world manifest files
  /                \
 /   Unit Tests     \ Unit Tests + Doctests
/  + Doctests        \ ─ Parser functions, edge cases
/_____________________\ ─ API documentation examples
```

**Four layers** (see [TESTING_STRATEGY.md](TESTING_STRATEGY.md) for full details):

1. **Doctests**: API documentation examples that run as tests
2. **Unit Tests**: Component-level tests for individual functions and edge cases
3. **Golden Tests**: Regression tests comparing output against Python ScanCode reference
4. **Integration Tests**: End-to-end tests validating the full scanner pipeline

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
- **`src/assembly/`** - Package assembly system (merging related manifests)
- **`src/scanner/`** - File system traversal and orchestration
- **`docs/`** - Architecture decisions, improvement docs, and guides
- **`testdata/`** - Test manifests for validation
- **`reference/`** - Python ScanCode Toolkit (reference submodule)

## Performance Characteristics

### Benchmarks

_(To be added: criterion benchmarks for parser performance)_

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

- CSV (tabular data export)
- YAML (human-readable)
- HTML report + HTML app
- Custom templates (user-defined formats)

#### Infrastructure Enhancements

**Plugin System**:

- No runtime plugin system is planned for Provenant
- Compile-time integration points are preferred over a public plugin ABI
- Revisit only if concrete extension needs justify the complexity

**Caching**:

Provenant uses one shared persistent cache root. The currently implemented persistent cache is the
optional scan-result cache stored under `scan-results/`, while incremental scanning remains future
work.

The cache implementation lives in `src/cache/` (`config`, `metadata`, `paths`, `io`,
`scan_cache`). It provides cache-root selection, snapshot metadata and invalidation, sharded
scan-result paths, and atomic snapshot persistence.

User-facing behavior is:

1. `--cache <kind>` currently enables `scan-results`
2. `--cache-dir` and `PROVENANT_CACHE` select the shared cache root
3. `--cache-clear` clears that root before scanning
4. persistent caching is opt-in; nothing is written unless `--cache` is specified

Follow-up work is focused on multi-process coordination, incremental scanning, and a more unified default cache-root strategy.

**Progress Tracking**:

Centralized `ScanProgress` struct manages mode-aware progress output via `indicatif::MultiProgress`:

1. **Discovery phase**: Spinner/message while counting files, recording initial file/dir/size counts.
2. **SPDX load phase**: Startup message and timing capture around license DB load.
3. **Scan phase**: Main progress bar (default mode, TTY only) with ETA, elapsed time, and `{per_sec}` throughput; verbose mode emits file-by-file paths.
4. **Assembly and output phases**: Phase messages/spinners with timing capture.
5. **Scan summary**: Files/sec, bytes/sec, error count, initial/final counts (including sizes), package assembly counts, and per-phase timings.

Verbosity behavior is implemented in `src/progress.rs` and wired through `src/main.rs`: quiet suppresses stderr output, default shows progress/summary, verbose shows per-file stderr output with detailed per-file errors.

Logging integration uses `indicatif-log-bridge` for startup and global warnings, while parser and other file-scoped scan failures are attached to `FileInfo.scan_errors` in `src/scanner/process.rs`. That keeps serialized output, CI logs, and the quiet/default/verbose progress modes aligned: default mode shows concise failing paths, verbose mode shows the underlying per-file error details.

Module location: `src/progress.rs`

### Quality Enhancements

Ongoing quality improvements:

- Property-based testing with proptest
- Fuzzing with cargo-fuzz
- Performance benchmarks with criterion
- Memory profiling and optimization
- Continuous golden test expansion

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
