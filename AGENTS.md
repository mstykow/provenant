# Agent Guidelines for Provenant

This guide provides essential information for AI coding agents working on the `Provenant` codebase - a high-performance Rust tool for detecting licenses, copyrights, and package metadata in source code.

## Documentation Map

- **Architecture & Design Decisions**: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) - System design, components, principles
- **Documentation Index**: [`docs/DOCUMENTATION_INDEX.md`](docs/DOCUMENTATION_INDEX.md) - Best entry point for navigating the broader docs set
- **How-To Guides**: [`docs/HOW_TO_ADD_A_PARSER.md`](docs/HOW_TO_ADD_A_PARSER.md) - Step-by-step guide for adding new parsers
- **Architectural Decision Records**: [`docs/adr/`](docs/adr/) - Index of accepted design decisions and contributor guidance
- **Beyond-Parity Features**: [`docs/improvements/`](docs/improvements/) - Index of parser and subsystem improvements beyond Python parity
- **License Detection Architecture**: [`docs/LICENSE_DETECTION_ARCHITECTURE.md`](docs/LICENSE_DETECTION_ARCHITECTURE.md) - Current license detection architecture, embedded index flow, and maintainer workflow
- **Maintainer Workflows**: [`xtask/README.md`](xtask/README.md) - Canonical list of Rust-based maintainer commands from `xtask/Cargo.toml`, including benchmarking, output comparison, golden-fixture maintenance, and artifact generation
- **Supported Formats**: [`docs/SUPPORTED_FORMATS.md`](docs/SUPPORTED_FORMATS.md) - Auto-generated list of all supported package formats
- **API Reference**: Run `cargo doc --open` - Complete API documentation
- **This File**: Repo-specific agent guardrails and durable contributor conventions

## Project Context

**Provenant** is a Rust rewrite of [ScanCode Toolkit](https://github.com/aboutcode-org/scancode-toolkit/) that aims to be a trustworthy drop-in replacement while fixing bugs and using Rust-specific strengths. The original Python codebase is available as a reference submodule at `reference/scancode-toolkit/`.

### Core Philosophy: Correctness and Feature Parity Above All

The primary goal is functional parity users can trust. When implementing features:

- **Maximize correctness and feature parity**: Every feature, edge case, and requirement from the original must be preserved
- **Effort is irrelevant**: Take whatever time and effort needed to get it right. No shortcuts, no compromises
- **Zero tolerance for bugs**: Identify bugs in the original Python code and fix them in the Rust implementation
- **Leverage Rust advantages**: Use Rust's type system, ownership model, and ecosystem to create more robust, performant code
- **Never cut corners**: Proper error handling, comprehensive tests, and thorough edge case coverage are non-negotiable

### Using the Reference Submodule

Use the reference submodule as a behavioral specification: study the original implementation, tests, outputs, and known bugs to understand what must be preserved. Do **not** port it line by line. Use it to learn **what** the Rust implementation must do, not **how** it should be written. For deeper contributor guidance, see [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) and [`docs/HOW_TO_ADD_A_PARSER.md`](docs/HOW_TO_ADD_A_PARSER.md).

When an upstream test fixture is needed for Provenant tests, copy it into Provenant-owned `testdata/` and reference that local copy. Do **not** make tests or golden fixtures depend directly on paths under `reference/scancode-toolkit/`.

### Security and Extraction Boundaries

- Keep parsing static and bounded: do not execute package-manager code, project code, or shell commands to recover metadata.
- Keep package extraction separate from broader detection work: parsers may normalize trustworthy declared package-license metadata, but file-content license/copyright detection belongs to the detection pipeline.
- Not every package surface belongs in `PackageParser`: content-aware or opt-in surfaces such as compiled-binary package extraction can be scanner-owned exceptions. Follow the existing exception pattern instead of forcing everything through path-based parser registration.

## Workflow Entry Points

Treat the executable sources of truth as canonical:

- [`README.md`](README.md) for local setup, bootstrap, and routine developer commands
- [`package.json`](package.json) for documentation formatting/lint scripts
- [`xtask/README.md`](xtask/README.md) for maintainer workflows such as benchmarking, compare runs, golden maintenance, and generated artifacts
- [`docs/TESTING_STRATEGY.md`](docs/TESTING_STRATEGY.md) for test-layer definitions and current command guidance

## Dependency Management

- Use `cargo` to manage Rust dependencies instead of editing `Cargo.toml` by hand. Prefer `cargo add`, `cargo remove`, and targeted `cargo update` commands so `Cargo.toml` and `Cargo.lock` stay in sync.
- Always use the latest available dependency version unless there is a documented repository-specific reason not to.
- Do not add dependencies lightly. Before adding a new dependency, confirm that it clearly earns its weight in maintenance and complexity cost.
- Before adding a new dependency, always check its maintenance status (recent releases, active maintenance, ecosystem health/reputation, and any obvious long-term support concerns).

## Testing and Validation

Local runs must stay tightly scoped. This repository has many slow and specialized tests, so default to the smallest command that proves the change you just made and let CI handle the broader matrix.

- Prefer exact test paths over substring filters.
- Avoid broad local commands such as `cargo test`, `cargo test --all`, `cargo test --lib`, or unfiltered golden suites unless the user explicitly asked for them or there is no narrower way to validate shared infrastructure.
- Only run golden tests locally when the change directly affects golden-covered behavior, and keep them narrowly targeted.
- Do not update golden expected files just to make a failing test pass; fix the implementation unless the new output is intentionally better and documented.

For exact command patterns and test-layer definitions, see [`docs/TESTING_STRATEGY.md`](docs/TESTING_STRATEGY.md).

## Code Quality Guardrails

Let the repository formatters and linters enforce mechanical style. Keep human guidance here focused on semantics and maintainability:

- Avoid `.unwrap()` in library code unless panic is genuinely intended.
- Do not use `#[allow(dead_code)]` just to silence dead code; remove unused code or wire it correctly.
- Do not suppress clippy warnings as a shortcut. Suppressions are only acceptable for genuine false positives and must be permanent, justified, and commented.
- Use comments to explain non-obvious intent or tradeoffs, not to restate the code.

## Adding or Changing Package Parsers

Use [`docs/HOW_TO_ADD_A_PARSER.md`](docs/HOW_TO_ADD_A_PARSER.md) as the canonical guide for parser work. It covers parser invariants, registration, datasource wiring, expected tests, assembly/file-reference integration, and validation against the Python reference or authoritative format specs.

## CI/CD

Canonical hook and CI definitions live in [`lefthook.yml`](lefthook.yml), [`package.json`](package.json), and [`.github/workflows/check.yml`](.github/workflows/check.yml), with helper scripts in [`scripts/`](scripts/). Agents should treat the full CI workflow as CI's job, not the default local workflow. Local iteration should stay focused on the exact tests and checks needed for the files and behavior under change.

**All checks must pass before merging.**

### Commits and Pull Request Titles

- Write git commit messages in Conventional Commits format: `type(scope): short summary` when a scope adds clarity, or `type: short summary` otherwise. Mark breaking changes with `!` when needed.
- Prefer lowercase conventional types such as `feat`, `fix`, `docs`, `refactor`, `test`, `build`, `ci`, `perf`, and `chore`, and keep the summary imperative, concise, and focused on what was accomplished and why.
- Use the same Conventional Commits format for pull request titles so the PR title is squash-merge ready and consistent with release/changelog tooling.

### Opening Pull Requests

- Use [`.github/pull_request_template.md`](.github/pull_request_template.md) for every agent-authored PR. The final PR body should follow its section structure, complete the applicable sections, and omit sections that do not apply.
- With `gh`, use `--template .github/pull_request_template.md` only for interactive/editor-driven PR creation. When supplying `--body` or `--body-file`, do **not** combine them with `--template`; instead, render the template structure manually into the provided body.
- Keep PR scope disciplined. For ecosystem/parser work, prefer one ecosystem family per PR and do not hide unrelated refactors inside the same review unit.

## Performance and Architecture

For scanner/assembly architecture, concurrency assumptions, benchmark workflows, and compare-output workflows, use [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) and [`xtask/README.md`](xtask/README.md) as the canonical sources.

## Common Pitfalls

1. **Taking shortcuts or porting Python line-by-line**: Preserve behavior, not implementation details. Study the tests and edge cases, then implement the Rust version properly.
2. **Datasource ID mistakes**: Setting `datasource_id: None`, choosing the wrong `DatasourceId` variant, or missing an error-path assignment breaks assembly. See [Datasource IDs: The Assembly Bridge](#datasource-ids-the-assembly-bridge).
3. **License data missing**: Run `./setup.sh` to initialize submodule
4. **Cross-platform paths**: Use `Path` and `PathBuf`, not string concatenation
5. **Line endings**: Be careful with `\n` vs `\r\n` in tests
6. **Unwrap in library code**: Use `?` or `match` instead
7. **Breaking parallel processing**: Ensure modifications maintain thread safety
8. **Incomplete testing**: Every feature needs comprehensive test coverage including edge cases
9. **Suppressing clippy warnings**: Never use `#[allow(...)]` or `#[expect(...)]` to ignore clippy errors or warnings as a shortcut or temporary workaround. Clippy suppressions are only acceptable when the lint is genuinely a false positive and the suppression is intended to be permanent. Every suppression must include a comment explaining why it is justified. If clippy flags something, fix the code properly.

## Porting Features from Original ScanCode

When porting behavior from the Python reference, use it as the spec for requirements, edge cases, outputs, and known bugs — never as a line-by-line implementation template.

### Porting Guardrails

1. **Research exhaustively**: read the original implementation, tests, and documentation before designing the Rust version.
2. **Aim for feature parity, not code parity**: preserve behavior and output semantics while using idiomatic Rust.
3. **Design for correctness**: use strong types, explicit error handling, and tests that cover edge cases and bug fixes from the original.
4. **Leverage Rust advantages**: prefer zero-copy parsing, compile-time guarantees, and designs that make invalid states unrepresentable.
5. **Document intentional differences**: if Rust diverges behaviorally, explain why and add tests that demonstrate the improvement.
6. **For parser-specific implementation rules**: follow [`docs/HOW_TO_ADD_A_PARSER.md`](docs/HOW_TO_ADD_A_PARSER.md).

### Quality Checklist

Before considering a feature complete:

- [ ] All original functionality is preserved
- [ ] All edge cases from original tests are covered
- [ ] Known bugs from original are fixed (and tested)
- [ ] Error handling is comprehensive and explicit
- [ ] Code is idiomatic Rust (passes `clippy` without warnings — no suppressed lints unless permanently justified)
- [ ] Performance is equal to or better than original
- [ ] Real-world testdata produces correct output
- [ ] Golden test expected files are unchanged unless output genuinely improved (documented)
- [ ] Documentation explains any intentional behavioral differences

## Datasource IDs: The Assembly Bridge

`datasource_id` is the file-format-level bridge between parsers and assembly. It is **not** the same as `package_type`: one package type can map to many datasource IDs.

Guardrails:

- **Always set `datasource_id`** on every production path, including error and fallback returns.
- **Use the correct enum variant** for the exact file format being parsed.
- **Handle multi-datasource parsers explicitly** when one parser supports multiple file formats.
- **Add new datasource variants and assembly wiring together** so sibling/related files can merge correctly.
- **Use canonical spellings for serialization** (e.g., `NugetNuspec` → `"nuget_nuspec"`, `RpmSpecfile` → `"rpm_specfile"`).
- **Add legacy deserialization aliases** with `#[serde(alias = "...")]` when correcting upstream typos to maintain backward compatibility with `--from-json`.

For the full datasource and assembly workflow, see [`docs/HOW_TO_ADD_A_PARSER.md`](docs/HOW_TO_ADD_A_PARSER.md#step-6-add-assembly-support-if-applicable).

## Additional Notes

- **Rust toolchain**: Version pinned in `rust-toolchain.toml`
- **Output format**: ScanCode Toolkit-compatible JSON with `OUTPUT_FORMAT_VERSION`
- **License detection**: Uses an embedded license index built from the ScanCode rules dataset; see [`docs/LICENSE_DETECTION_ARCHITECTURE.md`](docs/LICENSE_DETECTION_ARCHITECTURE.md) for current detection behavior and maintenance workflow
- **Exclusion patterns**: Supports glob patterns (e.g., `*.git*`, `node_modules/*`)
- **Git submodules**: `reference/scancode-toolkit/` remains the behavioral reference and license-data source for parity work, but routine scans use the embedded index
