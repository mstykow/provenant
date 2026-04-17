# ADR 0009: Parser Submodule Structure for Large Ecosystems

**Status**: Accepted  
**Authors**: Provenant team  
**Supersedes**: None  
**Current Contract Owner**: [`../HOW_TO_ADD_A_PARSER.md`](../HOW_TO_ADD_A_PARSER.md)

## Context

Provenant's parser convention (recorded in HOW_TO_ADD_A_PARSER.md) directs contributors to put all `PackageParser` impls for one ecosystem into a single `src/parsers/<ecosystem>.rs`. This works well for small-to-moderate ecosystems where the file stays under ~1,500 lines.

Several ecosystems have grown far beyond that. As of this writing:

| File        | Lines |
| ----------- | ----- |
| `python.rs` | 5,407 |
| `debian.rs` | 3,878 |
| `nuget.rs`  | 3,031 |
| `maven.rs`  | 2,951 |
| `ruby.rs`   | 2,203 |
| `nix.rs`    | 2,060 |
| `gradle.rs` | 1,812 |

Large monolithic files hurt navigation, review, and incremental compilation. They also discourage the kind of focused refactoring that keeps parser code healthy.

The repo already has a **flat sibling** pattern for ecosystems with independently registered `PackageParser` impls: `npm.rs` + `npm_lock.rs` + `npm_workspace.rs`, `gradle.rs` + `gradle_lock.rs` + `gradle_module.rs`, `rpm_parser.rs` + `rpm_specfile.rs` + `rpm_db.rs`, etc. These are separate top-level modules, each with their own `PackageParser` impl and `register_parser!` call.

That pattern works when each surface type has its own impl, but it does not address the common case where **one** `PackageParser` impl dispatches to many large private extraction functions. For example, `PythonParser::extract_packages` routes to `extract_from_pyproject_toml`, `extract_from_setupCfg`, `extract_from_wheel_archive`, and a dozen more — all private helpers in the same 5,400-line file.

We need a structural prescription for breaking up these large dispatcher-backed parsers.

## Decision

### 1. Two-scale module structure

Ecosystems use either a **single file** or a **nested submodule directory**, depending on size:

| Scale     | Structure                                        | When to use                                                |
| --------- | ------------------------------------------------ | ---------------------------------------------------------- |
| **Small** | `src/parsers/<ecosystem>.rs`                     | File is ≤ 1,500 lines and unlikely to grow significantly   |
| **Large** | `src/parsers/<ecosystem>/mod.rs` + sibling files | File exceeds 1,500 lines or has clearly separable concerns |

The 1,500-line threshold is a soft guide, not a hard rule. The intent is to split when navigation and review friction become noticeable, not to force reorganization of files that are cohesive and readable.

### 2. Nested submodule layout

When an ecosystem converts to a directory, the structure is:

```text
src/parsers/<ecosystem>/
├── mod.rs              # PackageParser impl(s), dispatcher, is_match, register_parser!
├── <surface>.rs        # One file per major extraction surface or concern
├── <surface>_test.rs   # Unit tests for that surface
├── scan_test.rs        # Full-pipeline scan tests (ecosystem-wide)
└── ...
```

For a concrete example, `python/` might look like:

```text
src/parsers/python/
├── mod.rs              # PythonParser impl, dispatcher, register_parser!
├── wheel.rs            # wheel/egg/sdist archive extraction
├── wheel_test.rs       # unit tests for wheel/egg/sdist
├── setup_py.rs         # setup.py AST + regex parsing
├── setup_py_test.rs
├── pyproject.rs        # pyproject.toml + Poetry
├── pyproject_test.rs
├── setup_cfg.rs        # setup.cfg INI parsing
├── setup_cfg_test.rs
├── rfc822.rs           # PKG-INFO/METADATA parsing
├── rfc822_test.rs
├── pypi_json.rs        # pypi.json
├── pip_inspect.rs      # pip-inspect.deplock
├── utils.rs            # shared helpers (if needed)
└── scan_test.rs        # full-pipeline scan tests
```

Specific rules:

- **`mod.rs` owns the public contract**: `PackageParser` impl(s), `is_match`, the `extract_packages` dispatcher, `register_parser!`, and any types needed by sibling files (helper structs, enums, constants). It re-exports everything the rest of the crate needs.
- **Surface files are private by default**: `mod <surface>;` without `pub`. They contain the `extract_from_*` functions and their supporting helpers for one format or concern (e.g., `wheel.rs`, `setup_py.rs`, `pyproject.rs`).
- **Surface files may use `pub(super)` for shared helpers**: If two surfaces share a utility function (e.g., `normalize_python_package_name`), place it in the surface that owns it and mark it `pub(super)`. If sharing becomes widespread, extract a shared `utils.rs` (or `common.rs`) inside the directory.
- **Test files are co-located inside the directory**: Each surface file has a matching `<surface>_test.rs` in the same directory. This gives tests natural visibility into `pub(super)` and `pub(self)` items without forcing `pub(crate)` leaks, and keeps the test code as navigable as the source it tests.
- **Ecosystem-wide test files stay as single files**: `scan_test.rs` (full-pipeline integration) and any golden-test files test across surfaces and belong as single files in the directory, not split per surface.

### 3. Flat siblings remain for independently-registered parsers

The existing flat-sibling pattern (e.g., `npm.rs` + `npm_lock.rs` + `npm_workspace.rs`) is **not** affected. Use flat siblings when each file has its own `PackageParser` impl and `register_parser!` call. Use nested submodules when one `PackageParser` impl dispatches to many large private helpers.

These two patterns can coexist within the same ecosystem. For example, `python/` could contain the main `PythonParser` dispatcher and its extraction surfaces, while `pip_inspect_deplock.rs` stays flat with its own independent `PipInspectDeplockParser`.

### 4. Conversion guide

When splitting an existing `<ecosystem>.rs` into a directory:

1. Create `src/parsers/<ecosystem>/` directory.
2. Move the `PackageParser` impl, `register_parser!`, dispatcher logic, shared types, and constants into `mod.rs`.
3. Extract each `extract_from_*` function group and its private helpers into a named surface file.
4. Add `mod <surface>;` declarations in `mod.rs`.
5. Move the corresponding test sections from the flat `<ecosystem>_test.rs` into co-located `<surface>_test.rs` files inside the directory, and add `#[cfg(test)] mod <surface>_test;` declarations in `mod.rs`. Move `<ecosystem>_scan_test.rs` into the directory as `scan_test.rs`.
6. Remove the old flat test module declarations from `src/parsers/mod.rs` and add the directory's test-module declarations in the ecosystem's `mod.rs` instead.
7. Update `src/parsers/mod.rs` if the public re-export path changes (it usually won't, since `mod python;` resolves to either `python.rs` or `python/mod.rs`).
8. Run `cargo check` and the ecosystem's existing tests to verify no breakage.

## Consequences

### Benefits

- **Navigability**: Contributors can find a format's logic in a focused file instead of scrolling through thousands of lines.
- **Reviewability**: PRs that touch one surface produce smaller diffs confined to that surface file.
- **Incremental compilation**: Rust's compilation unit is the module; smaller modules recompile faster after changes.
- **Natural grouping**: The directory namespace (`python::wheel`, `python::setup_py`) mirrors the mental model better than flat siblings.
- **Backward compatible**: `mod python;` in `mod.rs` resolves to either `python.rs` or `python/mod.rs` without wiring changes.
- **Coexists with flat siblings**: The two patterns address different structural needs and don't conflict.
- **Test visibility**: Co-located test files can reach `pub(super)` and private items without `pub(crate)` leaks, which flat test files cannot do against a nested directory structure.
- **Test navigability**: Test files shrink alongside source files — a 3,000-line monolithic test file benefits from the same split as a 5,000-line source file.
- **PR locality**: Changing a surface means its tests are in the same directory, not three directories up.

### Trade-offs

- **More files to manage**: A split ecosystem has more files than a monolith. This is a net win when files are cohesive and small, but adds overhead if the split is premature or arbitrary.
- **Visibility discipline**: Shared helpers need `pub(super)` annotations. Contributors must understand visibility boundaries within the directory.
- **Conversion cost**: Splitting an existing file is a one-time refactoring that produces a large diff. It should be done on its own PR without functional changes.
- **Transition inconsistency**: During migration, some ecosystems will have nested co-located tests and others will have flat tests. This is temporary and resolves as ecosystems are migrated.

## Alternatives Considered

### Flat sibling modules for everything

Approach: split `python.rs` into `python.rs` (dispatcher) + `python_wheel.rs` + `python_setup_py.rs` etc. at the top level.

Rejected because:

- Prefix-based naming (`python_wheel.rs`) is ad-hoc and doesn't scale across 40+ ecosystems — the flat namespace becomes cluttered.
- Private helpers must become `pub(crate)` to be shared across siblings, leaking implementation details.
- No namespace grouping: `python::wheel` is more discoverable than scanning a flat list for the `python_` prefix.

### One file per `PackageParser` impl only

Approach: only split when each file gets its own `PackageParser` impl; leave dispatchers monolithic.

Rejected because:

- The largest files (`python.rs` at 5,400 lines, `debian.rs` at 3,900 lines) have a single `PackageParser` impl with many private helpers. This approach would not help them.
- It preserves the problem this ADR aims to solve.

### Strict line-count threshold with forced splits

Approach: mandate splitting at exactly N lines.

Rejected because:

- Line counts fluctuate as features are added. A hard threshold creates churn at the boundary.
- Cohesion matters more than line count. A well-structured 1,800-line file is better than an artificially split 800 + 1,000 pair with a tangled cross-module interface.

### Flat test files outside the directory

Approach: keep `python_test.rs` and `python_scan_test.rs` at `src/parsers/` level, not inside `python/`.

Rejected because:

- Flat test files cannot access private items in surface modules — they would need `pub(crate)` re-exports in `mod.rs`, leaking implementation details that the directory structure is designed to encapsulate.
- A 3,000+ line monolithic test file suffers the same navigability problem as the source file it tests, so splitting the source but not the tests leaves half the problem unsolved.
- PRs changing a surface would require editing tests in a different directory, reducing the locality benefit of the split.

## Related ADRs

- [ADR 0001: Trait-Based Parser Architecture](0001-trait-based-parsers.md) — Defines the `PackageParser` trait contract that this structure organizes
- [ADR 0006: DatasourceId-Driven Package Assembly](0006-datasourceid-driven-package-assembly.md) — Explains why multi-surface ecosystems need multiple datasource IDs, which drives the number of extraction functions

## References

- [`docs/HOW_TO_ADD_A_PARSER.md`](../HOW_TO_ADD_A_PARSER.md) — Contributor guide for parser structure (current contract owner, to be updated to reflect this ADR)
- Existing flat-sibling examples: `npm.rs` + `npm_lock.rs` + `npm_workspace.rs`, `gradle.rs` + `gradle_lock.rs` + `gradle_module.rs`, `rpm_parser.rs` + `rpm_specfile.rs` + `rpm_db.rs`
