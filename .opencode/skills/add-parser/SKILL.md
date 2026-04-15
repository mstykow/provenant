---
name: add-parser
description: Implement a new package parser in Provenant, following the full workflow from research through registration, testing, assembly wiring, and validation.
---

# Add a Package Parser

This skill implements a new package parser in Provenant following the canonical workflow from `docs/HOW_TO_ADD_A_PARSER.md`.

## Workflow

### Step 1: Research the parser surface

Before writing code, determine:

- Which concrete filenames or file patterns does this parser own?
- Is this one datasource or several distinct datasources handled by one parser?
- Does the format carry package identity, dependencies, declared-license metadata, or file references?
- Does the ecosystem fit default sibling/nested assembly, or does it need topology-driven assembly?
- If a Python ScanCode parser exists under `reference/scancode-toolkit/src/packagedcode/`, use it as a **behavioral specification** — learn what the Rust parser must do, not how to write it.

Collect representative fixtures covering:

- Basic success case
- Malformed or partially missing input
- Dependency scope variations (if the format has them)
- Declared-license variations (if the format exposes them)
- Manifest/lockfile or file-reference cases (if downstream assembly depends on them)
- If you need upstream ScanCode fixtures, copy them into Provenant-owned `testdata/` first; do not make tests depend directly on `reference/scancode-toolkit/` paths.

### Step 2: Implement the parser

Create `src/parsers/<ecosystem>.rs` and implement `PackageParser`.

Use the current parser contract from `src/parsers/mod.rs`. Template:

```rust
use std::path::Path;

use crate::models::{DatasourceId, PackageData, PackageType};
use crate::parser_warn as warn;

use super::PackageParser;

pub struct MyParser;

impl PackageParser for MyParser {
    const PACKAGE_TYPE: PackageType = PackageType::Npm;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "package.json")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        match std::fs::read_to_string(path) {
            Ok(_content) => vec![PackageData {
                package_type: Some(Self::PACKAGE_TYPE),
                datasource_id: Some(DatasourceId::NpmPackageJson),
                ..Default::default()
            }],
            Err(error) => {
                warn!("Failed to read {:?}: {}", path, error);
                vec![PackageData {
                    package_type: Some(Self::PACKAGE_TYPE),
                    datasource_id: Some(DatasourceId::NpmPackageJson),
                    ..Default::default()
                }]
            }
        }
    }
}
```

Add `register_parser!` near the end of the file:

```rust
crate::register_parser!(
    "npm package.json manifest",
    &["**/package.json"],
    "npm",
    "JavaScript",
    Some("https://docs.npmjs.com/cli/v10/configuring-npm/package-json"),
);
```

**Assembly and topology hints**:

- Parsers stay file-local extractors: emit facts for the current file, not repository-wide assembly.
- If a format declares project structure such as workspace members or root/member roles, preserve that structural intent in parser output so topology-aware assembly can consume it.
- New parser work should treat topology-aware assembly as a first-class downstream consumer rather than assuming every ecosystem fits plain sibling merge.
- If the format has no cross-file topology, prefer the default local assembly path and avoid topology-specific wiring without a concrete need.

**Parser invariants** (non-negotiable):

- Set `datasource_id` on **every production path**, including error and fallback returns.
- Use `crate::parser_warn!` (imported as `warn`) for parser failures — never plain `log::warn!()`.
- Do not execute package-manager code or shell commands from parser logic.
- Do not do broad file-content license detection, copyright detection, or backfilling from sibling files inside the parser by default.
- Preserve raw dependency and license input when the source format is ambiguous.

**Rare exceptions stay rare, bounded, and documented**:

- `python.rs` has bounded sibling enrichment for a few adjacent metadata sidecars; new parsers should not copy that pattern unless an explicit assembly pass is genuinely infeasible.
- Some content-aware package surfaces are scanner-owned exceptions rather than `PackageParser`s. The current example is compiled-binary package extraction behind `--package-in-compiled`; do not force those surfaces through path-based parser registration.

**Declared-license contract**:

- If the format exposes a trustworthy declared-license surface, populate `extracted_license_statement`, `declared_license_expression`, `declared_license_expression_spdx`, and parser-side `license_detections`.
- Use the shared helper in `src/parsers/license_normalization.rs` — never write parser-specific normalization logic.
- If the license surface is weak or ambiguous, keep the parser raw-only: preserve `extracted_license_statement`, leave declared-license fields empty, do not emit guessed or partial expressions.

**Dependency contract**:

- Populate `dependencies` whenever the format actually carries dependency data.
- Preserve the ecosystem's native scope terminology unless an existing parser pattern says otherwise.
- Treat parser tests and parser goldens as interface-contract checks for dependency fields, not just smoke tests.

**Multi-format ecosystems**: When an ecosystem has both a manifest and a lockfile, put all `PackageParser` impls in a single `src/parsers/<ecosystem>.rs` file with separate `register_parser!` invocations for each. Example: `src/parsers/julia.rs` contains both `JuliaProjectTomlParser` and `JuliaManifestTomlParser`.

**Security utilities** (`src/parsers/utils.rs`):

Every parser should use these shared helpers rather than reimplementing ADR 0004 bounds checks:

- `read_file_to_string(path, max_size)` — stat-before-read size check (default 100 MB), UTF-8 with lossy fallback and warning
- `truncate_field(value)` — caps individual string fields to 10 MB (`MAX_FIELD_LENGTH`), warns on truncation
- `MAX_ITERATION_COUNT` (100,000) — `.take(MAX_ITERATION_COUNT)` on every `.iter()` over user-supplied collections (dependencies, keywords, authors, archive entries, etc.)
- `MAX_MANIFEST_SIZE`, `MAX_FIELD_LENGTH` — ADR 0004 resource-limit constants

**Use existing parsers as templates**:

- `src/parsers/cargo.rs` — manifest parser with declared-license normalization, `is_pinned` version analysis, and `file_references` extraction (license-file, readme). Shows the `normalize_spdx_expression` license path and workspace-inheritance detection in `extra_data`.
- `src/parsers/about.rs` — file-reference handling
- `src/parsers/npm.rs` — rich manifest parser: multi-scope dependency groups via `extract_dependency_group`, VCS URL normalization (`normalize_repo_url`), SRI integrity hash parsing, party extraction (author/contributor/maintainer), and workspace/overrides metadata in `extra_data`. Good reference for ecosystems with many optional manifest surfaces.
- `src/parsers/python.rs` — complex multi-surface ecosystem: 11 datasources in one parser, AST-based setup.py parsing (no code execution), archive safety (size/compression-ratio limits via `collect_validated_zip_entries`), bounded sibling enrichment from adjacent `.dist-info`/`.egg-info` sidecars. Demonstrates the most extensive parser structure.

### Step 3: Register the parser in `src/parsers/mod.rs`

**Module wiring**:

```rust
mod my_ecosystem;
#[cfg(test)]
mod my_ecosystem_test;
#[cfg(test)]
mod my_ecosystem_scan_test;

pub use self::my_ecosystem::MyEcosystemParser;
```

Do **not** add per-parser golden modules directly to `src/parsers/mod.rs`; golden wiring is centralized in `src/parsers/golden_test.rs`.

**Scanner registration**: Add the parser to the `parsers:` list inside `register_package_handlers!`. If the parser is not listed there, it will never be called by scanner dispatch.

Verify registration:

```bash
cargo run --manifest-path xtask/Cargo.toml --bin update-parser-golden -- --list
```

The parser should appear in the output.

### Step 4: Add tests

**Unit tests** — `src/parsers/<ecosystem>_test.rs`:

- `is_match()`
- Basic extraction of package identity
- Malformed or partial input
- Dependency extraction and scope handling
- Declared-license behavior when the format has a trustworthy license field
- Any parser-specific edge case the reference implementation already handles

**Parser golden tests** — `src/parsers/<ecosystem>_golden_test.rs`:

- Follow the feature-gating pattern used by neighboring golden tests.
- Add representative fixtures under `testdata/<ecosystem>-golden/`.
- Register in `src/parsers/golden_test.rs`:

```rust
#[path = "my_ecosystem_golden_test.rs"]
mod my_ecosystem_golden_test;
```

- Generate expected output:

```bash
cargo run --manifest-path xtask/Cargo.toml --bin update-parser-golden -- <ParserType> <input_file> <output_file>
```

- If fixture filenames don't end in `.json`, run `npx prettier --write --parser json <files>` explicitly.
- Commit golden `.expected` files alongside test fixtures — CI checks that they exist and match.

**Parser-adjacent scan tests** — `src/parsers/<ecosystem>_scan_test.rs`:

- Required when parser correctness depends on scanner wiring, assembly, topology planning, or file/package linkage.
- Effectively required when the parser emits: `for_packages` links, `datafile_paths`, dependency hoisting or manifest/lockfile interaction, `PackageData.file_references`.
- See `src/parsers/cargo_scan_test.rs` for a minimal example.

**Keep local verification scoped**:

- Prefer the smallest owning unit, golden, scan, or assembly target that proves the parser work.
- Avoid broad local commands like `cargo test` or unfiltered golden suites unless there is no narrower way to validate the change.

### Step 5: Wire `DatasourceId` and assembly accounting

**Add `PackageType` variant** in `src/models/package_type.rs` and its `as_str()` match arm. Use one variant per ecosystem (not per file format).

**Add `DatasourceId` variant(s)** in `src/models/datasource_id.rs`. Use one variant per concrete file format.

**Classify every datasource** in `src/assembly/assemblers.rs`:

- Add to an `AssemblerConfig` when it participates in assembly.
- Add to `UNASSEMBLED_DATASOURCE_IDS` when it is intentionally standalone.
- If you skip this, `test_every_datasource_id_is_accounted_for` will fail.

**Add assembly config when needed**: If the ecosystem has related manifest/lockfile or sibling metadata surfaces, add an `AssemblerConfig` with the exact datasource IDs your parser emits. Keep `sibling_file_patterns` aligned with real filenames.

**File-reference resolution**: If the parser emits `PackageData.file_references`, register the datasource in `src/assembly/file_ref_resolve.rs` and add a scan test proving files link back to the package.

**Assembly goldens**: If the ecosystem assembles multiple files into one logical package, add assembly fixtures under `testdata/assembly-golden/<ecosystem>-basic/` and a matching test in `src/assembly/assembly_golden_test.rs`.

### Step 6: Validate behavior

Compare against the Python ScanCode reference (if it exists) or the authoritative format spec. Validate at least:

- Package identity fields
- Dependency presence and scope
- Declared-license output and raw statement preservation
- PURL shape
- Datasource IDs and assembly behavior
- File-reference linkage when applicable

For implemented parser families, the main end-to-end parity workflow is `compare-outputs`, not ad hoc manual scanner runs:

```bash
cargo run --manifest-path xtask/Cargo.toml --bin compare-outputs -- --repo-url https://github.com/org/repo.git --repo-ref <ref> --profile common
```

Record representative `compare-outputs` references in `docs/BENCHMARKS.md`.

If the Rust parser intentionally improves on Python behavior, document the improvement in `docs/improvements/<ecosystem>-parser.md`.

Add a scorecard row to `docs/implementation-plans/package-detection/PARSER_VERIFICATION_SCORECARD.md`.

Regenerate supported formats and verify:

```bash
cargo run --manifest-path xtask/Cargo.toml --bin generate-supported-formats -- --check
```

## Done checklist

Before considering a parser complete, verify ALL of the following:

- [ ] Implementation exists in `src/parsers/<ecosystem>.rs`
- [ ] `PackageType` variant exists in `src/models/package_type.rs`
- [ ] `datasource_id` is correct on every production path
- [ ] Parser is exported and registered in `src/parsers/mod.rs`
- [ ] `register_parser!` metadata is present
- [ ] Parser unit tests exist
- [ ] Parser goldens exist (default expectation)
- [ ] Golden `.expected` files are committed alongside test fixtures
- [ ] Parser-adjacent scan tests exist when downstream package or file-link behavior matters
- [ ] Every new datasource is classified in `src/assembly/assemblers.rs`
- [ ] File-reference ownership is wired when the parser emits `PackageData.file_references`
- [ ] `docs/SUPPORTED_FORMATS.md` is regenerated and staged
- [ ] Representative `compare-outputs` references are recorded in `docs/BENCHMARKS.md`
- [ ] Scorecard row added to `docs/implementation-plans/package-detection/PARSER_VERIFICATION_SCORECARD.md`
- [ ] Behavior has been validated against the Python reference or authoritative spec

## Common failure modes

- Parser compiles but never runs because it was not added to `register_package_handlers!`.
- `datasource_id` set on happy path but forgotten on parse-error or fallback returns.
- Parser uses `log::warn!()` instead of `parser_warn!()`.
- Parser guesses declared-license expressions from weak metadata instead of preserving raw input.
- Parser-only tests pass but real scanner output is wrong because `*_scan_test.rs` was skipped.
- Parser emits `file_references` but no resolver ownership was added in assembly.
- `register_parser!` was skipped, so supported-formats docs never pick up the parser.
- `docs/SUPPORTED_FORMATS.md` was not regenerated, so the pre-commit hook or CI docs checks fail.
- Golden `.expected` files were not generated or committed.

## Reference documents

- `docs/HOW_TO_ADD_A_PARSER.md` — full canonical guide
- `docs/ARCHITECTURE.md` — parser/assembly subsystem rationale
- `docs/adr/0004-security-first-parsing.md` — security-first parsing decision and threat model (no code execution, DoS limits, archive safety, input validation)
- `docs/TESTING_STRATEGY.md` — test-layer definitions and command guidance
- `xtask/README.md` — xtask command CLI reference
- `AGENTS.md` — contributor guardrails and repo conventions
