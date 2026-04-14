# How To Add A Parser

This guide covers the parts of parser work that are specific to Provenant: parser invariants,
registration, datasource wiring, test expectations, and assembly/file-reference integration.

It intentionally does **not** repeat generic setup, Rust style, or broad testing workflow docs.
Use these as the source of truth for project-wide guidance:

- [`README.md`](../README.md) for local setup and hook installation
- [`TESTING_STRATEGY.md`](TESTING_STRATEGY.md) for test-layer definitions and command guidance
- [`ARCHITECTURE.md`](ARCHITECTURE.md) for parser/assembly subsystem rationale
- [`AGENTS.md`](../AGENTS.md) for contributor guardrails and repo conventions

## Parser workflow in this repo

Adding a parser usually means doing all of the following:

1. research the manifest or lockfile behavior you need to preserve
2. implement `src/parsers/<ecosystem>.rs`
3. register the parser in `src/parsers/mod.rs`
4. register parser metadata with `register_parser!`
5. add parser-local tests and, by default, parser goldens
6. classify every new `DatasourceId` for assembly accounting
7. add assembly or file-reference wiring when the ecosystem needs it
8. validate behavior against the Python reference or the authoritative format spec

## 1. Decide the parser surface before coding

Before you write code, answer these questions:

- Which concrete filenames or file patterns does this parser own?
- Is this one datasource or several distinct datasources handled by one parser?
- Does the format carry package identity, dependencies, declared-license metadata, or file
  references?
- Does the ecosystem fit the default sibling/nested assembly path, or does it declare project topology that should be formalized for topology-driven assembly?
- If the Python ScanCode parser exists, what behavior and edge cases must be preserved?

If the ecosystem exists under `reference/scancode-toolkit/src/packagedcode/`, use the Python
implementation and tests as a **behavioral specification**. Use them to learn what the Rust parser
must do, not how to write it.

Collect representative fixtures early. At minimum, gather files that cover:

- a basic success case
- malformed or partially missing input
- dependency scope variations, if the format has them
- declared-license variations, if the format exposes them
- manifest/lockfile or file-reference cases, if downstream assembly depends on them

## 2. Implement the parser

Create `src/parsers/<ecosystem>.rs` and implement `PackageParser`.

Use the current parser contract from `src/parsers/mod.rs`, not an older string-based template:

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

### Assembly and topology hints

- Parsers remain file-local extractors: they should emit package facts for the current file, not
  perform repository-wide assembly.
- If a format declares project structure such as workspace members, root/member roles, or other
  non-local ownership boundaries, preserve that structural intent in parser output so the
  topology-planning layer can consume it consistently.
- Today that structural intent may still be stored in parser `extra_data` for some ecosystems, but
  new parser work should treat topology-aware assembly as a first-class downstream consumer rather
  than assuming every ecosystem fits plain sibling merge.
- If the format has no cross-file topology, prefer the default local assembly path and do not add
  topology-specific wiring without a concrete need.

### Parser invariants that matter here

- Set `datasource_id` on **every production path**, including error and fallback returns.
- Use `crate::parser_warn!` (typically imported as `warn`) for parser failures so diagnostics land
  in structured scan output.
- Do not use plain `log::warn!()` for file-scoped parser failures.
- Do not execute package-manager code or shell commands from parser logic.
- Do not do broad file-content license detection, copyright detection, or backfilling from sibling
  files inside the parser **by default**.
- Preserve raw dependency and license input when the source format is ambiguous.

Rare exceptions should stay rare, bounded, and documented:

- `python.rs` currently performs bounded sibling enrichment for adjacent installed/source metadata
  sidecars such as `requires.txt`, `RECORD`, `installed-files.txt`, `SOURCES.txt`, and sibling
  `WHEEL` files because those files are part of the same Python metadata surface. File ownership
  resolution still belongs in assembly (`src/assembly/file_ref_resolve.rs`), and new parsers should
  not copy this pattern unless an explicit assembly pass is genuinely infeasible.
- `compiled_binary.rs` is an opt-in scanner-owned detector for raw executables with trustworthy
  embedded package metadata. It is intentionally **not** a `PackageParser`: raw binaries do not
  have stable filename ownership, so scanner wiring passes already-read bytes through the dedicated
  `--package-in-compiled` path instead of broad path matching. New compiled-binary detectors should
  stay bounded, consume scanner-provided bytes where possible, and follow this explicit exception
  pattern rather than stretching `PackageParser` beyond its path-matching design.

### Declared-license contract

If the format exposes a **trustworthy declared-license surface** such as an SPDX-compatible manifest
field, populate:

- `extracted_license_statement`
- `declared_license_expression`
- `declared_license_expression_spdx`
- parser-side `license_detections`

Use the shared helper in `src/parsers/license_normalization.rs` instead of writing parser-specific
normalization logic.

If the license surface is weak or ambiguous, keep the parser raw-only:

- preserve `extracted_license_statement`
- leave declared-license fields empty
- do not emit guessed or partial expressions

### Dependency contract

- Populate `dependencies` whenever the format actually carries dependency data.
- Preserve the ecosystem's native scope terminology unless an existing parser pattern says
  otherwise.
- When the source format does **not** prove dependency intent, leave semantic booleans such as
  `is_runtime`, `is_optional`, `is_direct`, and `is_pinned` unset instead of guessing or forcing
  defaults.
- Only emit those booleans when the datasource can actually justify them. Lockfiles and other
  resolved-package inventories often prove version pinning or direct/transitive structure, but they
  frequently do **not** prove runtime-vs-development or optional-vs-required semantics.
- If a downstream compatibility surface ever needs stricter normalization for these fields, do that
  as an explicit output-layer transformation rather than changing parser semantics.
- Treat parser tests and parser goldens as interface-contract checks for dependency fields, not just
  smoke tests.

### Parser metadata registration

Add `crate::register_parser!(...)` near the end of the parser file. This feeds
`docs/SUPPORTED_FORMATS.md` generation through `src/parsers/metadata.rs`.

```rust
crate::register_parser!(
    "npm package.json manifest",
    &["**/package.json"],
    "npm",
    "JavaScript",
    Some("https://docs.npmjs.com/cli/v10/configuring-npm/package-json"),
);
```

If you skip this macro, the parser can still work at scan time, but it will be missing from the
generated supported-formats docs.

### Use existing parsers as templates

Prefer copying patterns from a nearby real parser over inventing a fresh structure. Good starting
points in this repo:

- `src/parsers/cargo.rs` for a manifest parser with declared-license normalization and dependencies
- `src/parsers/about.rs` for file-reference handling
- `src/parsers/npm.rs` or `src/parsers/python.rs` for more complex multi-surface ecosystems

When an ecosystem has both a manifest and a lockfile (or multiple related file formats), put all
`PackageParser` impls in a single `src/parsers/<ecosystem>.rs` file with separate
`register_parser!` invocations for each. This keeps related parsing logic co-located. For example,
`src/parsers/julia.rs` contains both `JuliaProjectTomlParser` and `JuliaManifestTomlParser`.

## 3. Register the parser in `src/parsers/mod.rs`

You need both module wiring and scanner registration.

### Module wiring

Add the parser module and its test modules:

```rust
mod my_ecosystem;
#[cfg(test)]
mod my_ecosystem_test;
#[cfg(test)]
mod my_ecosystem_scan_test;

pub use self::my_ecosystem::MyEcosystemParser;
```

Match the test-module style used by neighboring parsers. Do **not** add per-parser golden modules
directly to `src/parsers/mod.rs`; this repo centralizes parser golden wiring in
`src/parsers/golden_test.rs`.

### Scanner registration

Add the parser to the `parsers:` list inside `register_package_handlers!`.

If the parser is not listed there, it will never be called by scanner dispatch even if the
implementation and tests compile.

You can verify registration with the parser-golden utility:

```bash
cargo run --manifest-path xtask/Cargo.toml --bin update-parser-golden -- --list
```

The parser should appear in that output after registration.

## 4. Add the tests this repo expects

### Unit tests

Add `src/parsers/<ecosystem>_test.rs` and cover the parser contract directly:

- `is_match()`
- basic extraction of package identity
- malformed or partial input
- dependency extraction and scope handling
- declared-license behavior when the format has a trustworthy license field
- any parser-specific edge case the reference implementation already handles

### Parser golden tests

For new production parsers in this repository, parser goldens are the default expectation.

Create `src/parsers/<ecosystem>_golden_test.rs`, follow the feature-gating pattern already used in
neighboring golden tests, and add representative fixtures under `testdata/<ecosystem>-golden/` or
the ecosystem-specific golden layout already used nearby.

After adding the file, register it in `src/parsers/golden_test.rs` with the same pattern used by
existing parsers:

```rust
#[path = "my_ecosystem_golden_test.rs"]
mod my_ecosystem_golden_test;
```

Use the parser-golden maintenance tool to generate expected output:

```bash
cargo run --manifest-path xtask/Cargo.toml --bin update-parser-golden -- --list
```

Then generate the exact expected files you need. See [`../xtask/README.md`](../xtask/README.md)
for the command syntax and [`TESTING_STRATEGY.md`](TESTING_STRATEGY.md) for the current command
patterns and golden-test feature-gating.

The golden tool writes JSON to `.expected` files and then runs prettier on them. If the fixture
filename does not end in `.json`, prettier cannot infer the parser and will fail. Work around
this by running `npx prettier --write --parser json <files>` explicitly.

CI checks that golden `.expected` files exist and match parser output, so they must be committed
alongside the test fixtures.

Scanner-owned detector surfaces that do **not** implement `PackageParser` still need fixture-backed
contract coverage. For those cases, add dedicated detector goldens near the owning detector module
and register them in the centralized golden suite, but do not force them through
`register_package_handlers!` or the parser-golden maintenance tool if the runtime surface is
intentionally scanner-gated.

### Parser-adjacent scan tests

Add `src/parsers/<ecosystem>_scan_test.rs` when parser correctness depends on scanner wiring,
assembly, topology planning, or file/package linkage rather than single-file extraction alone.

Treat a scan test as effectively required when the parser emits meaningful downstream contract data,
including:

- package visibility after assembly
- `for_packages` links
- `datafile_paths`
- dependency hoisting or manifest/lockfile interaction
- `PackageData.file_references`

Scanner-owned compiled-binary detectors belong here too: if extraction only happens through scanner
gating, add at least one focused contract test proving the packages appear only when the detector's
scan option is enabled.

See `src/parsers/cargo_scan_test.rs` for a minimal example.

### Keep local verification scoped

This repo prefers narrow local validation. Do not treat broad commands as the default path from this
guide. Use [`TESTING_STRATEGY.md`](TESTING_STRATEGY.md) for the canonical test taxonomy and command
guidance, then run the smallest unit, golden, scan, or assembly target that proves the parser work.

## 5. Wire `DatasourceId` and assembly accounting

Every new file format needs a `DatasourceId`, and every new datasource must be accounted for in
assembly.

### Add `PackageType` variant

Add a new `PackageType` variant to `src/models/package_type.rs` and its `as_str()` match arm.

Use one variant per ecosystem, not per file format. For example, both `JuliaProjectToml` and
`JuliaManifestToml` datasource IDs share `PackageType::Julia`.

### Add datasource variants

Add the new `DatasourceId` variant or variants to `src/models/datasource_id.rs`.

Use one variant per concrete file format, not one variant per ecosystem. A manifest parser and a
lockfile parser usually need different datasource IDs.

### Classify every datasource

Edit `src/assembly/assemblers.rs` and do one of the following for every new datasource:

- add it to an `AssemblerConfig` when it participates in assembly
- add it to `UNASSEMBLED_DATASOURCE_IDS` when it is intentionally standalone

If you skip this, `test_every_datasource_id_is_accounted_for` will fail even if the parser itself
works.

### Add assembly config when needed

If the ecosystem has related manifest/lockfile or sibling metadata surfaces, add an
`AssemblerConfig` with the exact datasource IDs your parser emits.

Keep `sibling_file_patterns` aligned with the real filenames the scanner will see. The assembler can
only merge package data whose datasource IDs live in the same config.

If the ecosystem needs a brand-new post-assembly behavior rather than just a new datasource entry,
register that pass in `src/assembly/assemblers.rs` via `PostAssemblyPassKind` and
`POST_ASSEMBLY_PASSES`.

### File-reference resolution ownership

If the parser emits `PackageData.file_references`, you must also wire ownership of that resolution.

Register the datasource in `src/assembly/file_ref_resolve.rs` or another explicit post-assembly
pass, then add a parser-adjacent scan test proving the final scanned files link back to the package.

Without this, the parser can extract file references correctly while final scan results still fail
to attach those files to the package.

### Assembly goldens

If the ecosystem assembles multiple files into one logical package, add assembly fixtures under
`testdata/assembly-golden/<ecosystem>-basic/` and a matching test in
`src/assembly/assembly_golden_test.rs`.

Use assembly goldens to prove the final assembled package shape, not just parser extraction.

## 6. Validate behavior before calling the parser done

If a Python ScanCode parser exists, compare behavior against it. Validate at least:

- package identity fields
- dependency presence and scope
- declared-license output and raw statement preservation
- purl shape
- datasource IDs and assembly behavior
- file-reference linkage when applicable

If no Python reference exists, validate against the authoritative format spec and real-world
fixtures from that ecosystem.

For implemented parser families, record representative end-to-end `compare-outputs` references in
[`docs/BENCHMARKS.md`](BENCHMARKS.md).

If the Rust parser intentionally improves on the Python behavior, document the improvement briefly in
`docs/improvements/<ecosystem>-parser.md`. Keep that doc focused on the behavior difference, not as
an implementation diary.

Add a row for the new ecosystem to
[`docs/implementation-plans/package-detection/PARSER_VERIFICATION_SCORECARD.md`](implementation-plans/package-detection/PARSER_VERIFICATION_SCORECARD.md)
so verification progress can be tracked alongside existing parsers.

## Common failure modes in this repo

- The parser compiles but never runs because it was not added to `register_package_handlers!`.
- `datasource_id` is set on the happy path but forgotten on parse-error or fallback returns.
- The parser uses `log::warn!()` instead of `parser_warn!()`, so scan diagnostics are lost.
- The parser guesses declared-license expressions from weak metadata instead of preserving raw input.
- Parser-only tests pass, but the real scanner output is wrong because the parser needed a
  `*_scan_test.rs`.
- The parser emits `file_references`, but no resolver ownership was added in assembly.
- `register_parser!` was skipped, so generated supported-formats docs never pick up the parser.
- `docs/SUPPORTED_FORMATS.md` is stale after adding a parser. The `generate-supported-formats`
  pre-commit hook will reject the commit if regeneration produces changes. If the commit bypasses
  hooks, the file will not be updated. Verify with
  `cargo run --manifest-path xtask/Cargo.toml --bin generate-supported-formats -- --check`.
- Golden `.expected` files were not generated or committed, so the CI golden-test job fails.

## Done definition

Before considering a new parser complete, make sure all of these are true:

- implementation exists in `src/parsers/<ecosystem>.rs`
- `PackageType` variant exists in `src/models/package_type.rs`
- `datasource_id` is correct on every production path
- parser is exported and registered in `src/parsers/mod.rs`
- `register_parser!` metadata is present
- parser unit tests exist
- parser goldens exist unless an explicitly scoped follow-up is already planned
- golden `.expected` files are committed alongside test fixtures
- parser-adjacent scan tests exist when downstream package or file-link behavior matters
- every new datasource is classified in `src/assembly/assemblers.rs`
- file-reference ownership is wired when the parser emits `PackageData.file_references`
- `docs/SUPPORTED_FORMATS.md` is regenerated and staged (pre-commit hook checks this)
- scorecard row added to `docs/implementation-plans/package-detection/PARSER_VERIFICATION_SCORECARD.md`
- behavior has been validated against the Python reference or authoritative spec

For intentionally scanner-gated detector surfaces such as compiled-binary package extraction,
substitute the parser-registry-specific checklist items with documented exception handling,
detector-level goldens, and scanner contract tests that cover the real runtime path.
