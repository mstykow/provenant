# ADR 0006: DatasourceId-Driven Multi-Pass Package Assembly

**Status**: Accepted  
**Authors**: Provenant team
**Supersedes**: None

## Context

Provenant scans many ecosystems where one logical package is described by multiple files rather than a single manifest. Common examples include:

- manifest + lockfile pairs such as `package.json` + `package-lock.json` or `Cargo.toml` + `Cargo.lock`
- nested metadata layouts such as Maven `pom.xml` + `META-INF/MANIFEST.MF`
- installed-package databases whose file ownership must be resolved after scanning
- workspace roots that need to become multiple member packages after the initial parse phase

We needed a package assembly architecture that:

1. Works across ecosystems instead of baking merge logic into each parser
2. Gives parsers a type-safe way to declare what kind of package data they emitted
3. Keeps extraction separate from post-processing, consistent with ADR 0002
4. Produces deterministic top-level `packages[]`, `dependencies[]`, and `for_packages` relationships for ScanCode-compatible output

Path-only heuristics were not enough: the same filename can mean different things across ecosystems, some logical packages span nested paths, and installed-package databases need a later file-reference pass that cannot be decided during initial parsing.

## Decision

We use **`DatasourceId` as the contract between parsers and assembly**, and we run package assembly as a **dedicated multi-pass post-scan phase**.

### Core rules

1. **Every production parser output must carry a `datasource_id`.**
   `PackageData.datasource_id` identifies the exact file format that produced that data.

2. **Every `DatasourceId` must be explicitly classified.**
   Each datasource is either:
   - listed in an `AssemblerConfig` in `src/assembly/assemblers.rs`, or
   - listed in `UNASSEMBLED_DATASOURCE_IDS` when it is intentionally not assembled.

3. **Assembly is a post-scan responsibility, not a parser responsibility.**
   Parsers extract raw `PackageData`; assembly combines related outputs into top-level `Package` objects and hoists dependencies.

4. **Assembly is multi-pass and ordered.**
   The stable architecture is:
   - datasource-config-driven directory assembly (`SiblingMerge` and `OnePerPackageData`)
   - nested merge for ecosystems whose related files live in different subpaths
   - file-reference and metadata-enrichment passes for installed-package/database style inputs
   - workspace assembly passes for monorepo/workspace ecosystems
   - deterministic sorting and deduplication of assembled outputs and file ownership links

5. **Assembler policy is centralized and cross-ecosystem.**
   `AssemblerConfig` defines which datasource IDs belong together, what sibling/nested file patterns anchor them, and which assembly mode they use.

### In practice

- Parsers emit `PackageData` with a concrete `DatasourceId`
- Assembly groups parsed data by location and assembler policy
- Related manifests are merged into a single logical package when identity and config rules match
- Installed-package databases resolve file references after scan-time file discovery
- Workspace passes can replace or refine earlier package candidates into member packages
- Final package/resource links are normalized for deterministic output

## Consequences

### Benefits

1. **Cross-ecosystem consistency**
   - All parsers follow the same contract for assembly participation
   - New ecosystems plug into a shared assembly architecture instead of inventing custom merge behavior

2. **Type safety and completeness checks**
   - `DatasourceId` is a typed enum rather than ad-hoc strings
   - Tests can enforce that every datasource is either assembled or intentionally unassembled

3. **Cleaner separation of concerns**
   - Parsers stay focused on extraction
   - Assembly owns merging, file ownership, workspace expansion, and dependency hoisting

4. **Deterministic output semantics**
   - Final `packages[]`, `dependencies[]`, `datafile_paths`, `datasource_ids`, and `for_packages` links are normalized after assembly
   - This makes output easier to test and reason about

5. **Extensibility for specialized passes**
   - We can add workspace, file-reference, or metadata-enrichment passes without changing every parser

### Trade-offs

1. **More coordination when adding formats**
   - New parsers must choose the correct `DatasourceId`
   - Contributors must also decide whether the datasource belongs in `ASSEMBLERS` or `UNASSEMBLED_DATASOURCE_IDS`

2. **Assembly logic is centralized and non-trivial**
   - The architecture is easier to reason about globally, but harder to understand than purely local parser logic

3. **Ordered passes require discipline**
   - Later passes may refine or replace earlier package candidates, so pass responsibilities must remain explicit and well-tested

## Alternatives Considered

### 1. Path-only or filename-only heuristics

Rejected because filenames alone do not encode enough semantic information for reliable cross-ecosystem assembly, especially for nested metadata, installed-package databases, and workspace/member relationships.

### 2. String-based datasource identifiers

Rejected because raw strings are easier to mistype, harder to validate exhaustively, and weaker as a shared contract than a typed enum.

### 3. Parser-local assembly

Rejected because it would duplicate merge logic across ecosystems, blur the extraction/assembly boundary, and make cross-cutting behaviors like file-reference resolution or workspace post-processing much harder to implement consistently.

### 4. Emit final top-level `Package` objects directly from parsers

Rejected because many package relationships are only knowable after the full scan result exists. Installed-package file ownership, nested merges, and workspace/member resource assignment all require a post-scan view of the tree.

## Related ADRs

- [ADR 0001: Trait-Based Parser Architecture](0001-trait-based-parsers.md) - Parsers provide the extraction side of the contract
- [ADR 0002: Extraction vs Detection Separation](0002-extraction-vs-detection.md) - Assembly remains separate from parser extraction responsibilities
- [ADR 0003: Golden Test Strategy](0003-golden-test-strategy.md) - Assembly and parser behavior are verified with fixture-backed tests

## References

- [`docs/ARCHITECTURE.md`](../ARCHITECTURE.md) - Package assembly system overview and pipeline placement
- [`docs/HOW_TO_ADD_A_PARSER.md`](../HOW_TO_ADD_A_PARSER.md) - `DatasourceId` requirements for parser authors
- [`src/models/datasource_id.rs`](../../src/models/datasource_id.rs) - Global datasource enum contract
- [`src/assembly/mod.rs`](../../src/assembly/mod.rs) - Core assembly pipeline implementation
- [`src/assembly/assemblers.rs`](../../src/assembly/assemblers.rs) - Central assembler policy and datasource coverage checks
