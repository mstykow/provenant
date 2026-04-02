# Agent Guidelines for Provenant

This guide provides essential information for AI coding agents working on the `Provenant` codebase - a high-performance Rust tool for detecting licenses, copyrights, and package metadata in source code.

## Documentation Map

- **Architecture & Design Decisions**: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) - System design, components, principles
- **How-To Guides**: [`docs/HOW_TO_ADD_A_PARSER.md`](docs/HOW_TO_ADD_A_PARSER.md) - Step-by-step guide for adding new parsers
- **Architectural Decision Records**: [`docs/adr/`](docs/adr/) - Index of accepted design decisions and contributor guidance
- **Beyond-Parity Features**: [`docs/improvements/`](docs/improvements/) - Index of parser and subsystem improvements beyond Python parity
- **License Detection Architecture**: [`docs/LICENSE_DETECTION_ARCHITECTURE.md`](docs/LICENSE_DETECTION_ARCHITECTURE.md) - Current license detection architecture, embedded index flow, and maintainer workflow
- **Supported Formats**: [`docs/SUPPORTED_FORMATS.md`](docs/SUPPORTED_FORMATS.md) - Auto-generated list of all supported package formats
- **API Reference**: Run `cargo doc --open` - Complete API documentation
- **This File**: Quick start, code style, common pitfalls

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

## Quick Start

```bash
# Setup and repository bootstrap
./setup.sh                    # Initialize submodules and local data dependencies when needed
git submodule update --init   # Refresh submodules after clone or submodule changes

# Build & Test
cargo build                   # Development build
cargo build --release         # Optimized build
cargo test -- --list          # Discover exact test paths before running targeted tests
cargo test <full-test-path>   # Prefer exact test paths for local iteration
cargo test --release --features golden-tests --lib <golden-test-path>  # Use only for narrowly targeted golden verification

# Code Quality
cargo fmt                     # Format code
cargo clippy                  # Lint and catch mistakes
cargo clippy --fix            # Auto-fix clippy suggestions
npm run check:docs            # Markdown lint + formatting check
npm run validate:urls         # Validate documentation/docstring URLs

# Run Tool
cargo run -- --json-pp output.json <dir> --ignore "*.git*" --ignore "target/*"
```

## Documentation Tooling

- **Markdown checks**: `npm run check:docs`
- **Markdown autofix**: `npm run fix:docs`
- **URL validation**: `npm run validate:urls`

## Running Single Tests

Local runs must stay tightly scoped. This repository has many slow and specialized tests, so agents should default to the smallest command that proves the change they just made. Prefer exact test paths over substring filters, and prefer a handful of related tests over broad module- or crate-wide sweeps. Let CI handle the broader matrix after code is pushed.

To run a specific test, first discover its full path from `cargo test -- --list`, then run the exact path:

```bash
cargo test <full-test-path>
```

Avoid broad local commands such as `cargo test`, `cargo test --all`, `cargo test --lib`, or unfiltered golden test suites unless the user explicitly asks for them or there is no narrower way to validate a shared infrastructure change.

For test-layer definitions, fixture-maintenance workflows, and broader testing guidance, see [`docs/TESTING_STRATEGY.md`](docs/TESTING_STRATEGY.md).

## Running Golden Tests

Only run golden tests locally when the change directly affects golden-test-covered behavior, and then run the narrowest possible golden test target. Always use `--release` unless explicitly instructed otherwise. Debug golden test runs are far too slow for normal agent work.

Running golden tests is expensive, so keep them narrowly targeted, prefer the dedicated helper scripts in `scripts/` when fixture maintenance is required, and use file-based caching for more complex incremental analysis.

## Project Architecture

**High-Level Structure:**

- `src/parsers/` - Package manifest parsers (trait-based, one per ecosystem)
- `src/models/` - Core data structures (PackageData, Dependency, DatasourceId, etc.)
- `src/assembly/` - Package assembly system (merging related manifests)
- `src/scanner/` - File system traversal and parallel processing
- `src/main.rs` - CLI entry point

**Key Patterns**: Trait-based parsers, Result-based errors, parallel processing with rayon

**For detailed architecture**: See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)

## Code Style Guidelines

### Imports

Organize imports in this order:

1. Standard library (`std::`)
2. External crates (alphabetical)
3. Internal crate modules (`crate::`)
4. Parent/sibling modules (`super::`, `self::`)

```rust
use std::fs::File;
use std::io::Read;
use std::path::Path;

use anyhow::Result;
use log::warn;
use serde::{Deserialize, Serialize};

use crate::models::PackageData;
use super::PackageParser;
```

### Naming Conventions

- **Types**: `PascalCase` (structs, enums, traits)
- **Functions/Variables**: `snake_case`
- **Constants**: `SCREAMING_SNAKE_CASE`
- **Modules**: `snake_case`
- **Test modules**: Use `#[cfg(test)] mod tests { ... }` or separate `_test.rs` files

```rust
const LICENSE_DETECTION_THRESHOLD: f32 = 0.9;

struct PackageParser;

fn extract_package_data(path: &Path) -> PackageData {
    let package_type = "npm";
}
```

### Types and Error Handling

- **Use `Result<T, E>` for fallible operations**: Prefer `anyhow::Error` for general errors
- **Pattern matching over unwrap**: Use `?` operator for error propagation
- **Avoid `.unwrap()` in library code**: Only acceptable in tests or when panic is intentional
- **Use `Option` methods**: `.and_then()`, `.map()`, `.unwrap_or()`, etc.

```rust
// Good
fn read_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| format!("Failed to open: {}", e))?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| format!("Failed to read: {}", e))?;
    Ok(content)
}

// Bad - avoid unwrap in library code
fn read_file_bad(path: &Path) -> String {
    let mut file = File::open(path).unwrap();  // DON'T DO THIS
    // ...
}
```

### Formatting

- **Line length**: No strict limit, but keep reasonable (~100 chars)
- **Indentation**: 4 spaces (enforced by `cargo fmt`)
- **Trailing commas**: Use in multi-line expressions
- **String literals**: Use `"double quotes"` for strings

```rust
PackageData {
    package_type: Some("npm".to_string()),
    name,
    version,
    homepage_url: None,  // Trailing comma
}
```

### Documentation

- **Public APIs**: Document with `///` doc comments
- **Examples**: Include examples for complex functions
- **Inline comments**: Explain "why" not "what"

```rust
/// Extracts package metadata from a manifest file.
///
/// Returns `PackageData` with all available fields populated.
/// Returns a default/empty structure if parsing fails.
pub fn extract_package_data(path: &Path) -> PackageData {
    // Use log::warn for parse errors rather than panicking
    // to allow the scan to continue for other files
    match parse_file(path) {
        Ok(data) => data,
        Err(e) => {
            warn!("Failed to parse {:?}: {}", path, e);
            default_package_data()
        }
    }
}
```

### Dead Code

- **Never use `#[allow(dead_code)]`** except when explicitly requested by the user
- **Dead functions indicate a problem**: Either they became unused by accident (find where they should be used) or they are unnecessary (remove them)
- **All code should serve a purpose**: Unused code is technical debt and should be cleaned up

## Adding a New Package Parser

1. **Create parser file**: `src/parsers/<ecosystem>.rs`
2. **Implement trait**:

   ```rust
   use crate::models::{DatasourceId, PackageData};
   use super::PackageParser;

   pub struct MyParser;

   impl PackageParser for MyParser {
       const PACKAGE_TYPE: &'static str = "my-ecosystem";

       fn is_match(path: &Path) -> bool {
           path.file_name().is_some_and(|name| name == "my-manifest.json")
       }

       fn extract_packages(path: &Path) -> Vec<PackageData> {
           // Implementation - always set datasource_id via DatasourceId enum
       }
   }
   ```

3. **Add test file**: `src/parsers/<ecosystem>_test.rs`
4. **Update mod.rs**: Add module declaration and public re-export
5. **Add test data**: Place sample manifests in `testdata/<ecosystem>/`

## Testing Strategy

Testing philosophy, layer definitions, and when to use each test type are canonical in [`docs/TESTING_STRATEGY.md`](docs/TESTING_STRATEGY.md).

### Golden Test Expected Files: Change with Care

Do not update golden expected files just to make a failing test pass.

- **Default assumption**: fix the implementation, not the expected output.
- **Update expectations only** for intentional, correct output improvements, and document why the new output is better.

## CI/CD

Canonical hook and CI definitions live in [`lefthook.yml`](lefthook.yml), [`package.json`](package.json), and [`.github/workflows/check.yml`](.github/workflows/check.yml), with helper scripts in [`scripts/`](scripts/). Agents should treat the full CI workflow as CI's job, not the default local workflow. Local iteration should stay focused on the exact tests and checks needed for the files and behavior under change.

**All checks must pass before merging.**

### Opening Pull Requests

- Use [`.github/pull_request_template.md`](.github/pull_request_template.md) for every agent-authored PR. The final PR body should follow its section structure, complete the applicable sections, and omit sections that do not apply.
- With `gh`, use `--template .github/pull_request_template.md` only for interactive/editor-driven PR creation. When supplying `--body` or `--body-file`, do **not** combine them with `--template`; instead, render the template structure manually into the provided body.
- Keep PR scope disciplined. For ecosystem/parser work, prefer one ecosystem family per PR and do not hide unrelated refactors inside the same review unit.

## Performance Considerations

- **Parallel processing**: File scanning uses `rayon` - maintain thread safety
- **Read once**: File contents read once into memory for all analysis operations
- **Early filtering**: Exclusion patterns applied early during traversal
- **Atomic progress**: Progress bar updates use atomic operations
- **Release optimizations**: Release builds use additional optimization settings; consult the Cargo configuration and architecture docs for current details
- **Benchmarking**: Run `./scripts/benchmark.sh` to measure performance on a standardized test repository. Use this after changes that could affect general performance. When committing performance-related changes, include the timing data in the commit message.

## Common Pitfalls

1. **Taking shortcuts or porting Python line-by-line**: Preserve behavior, not implementation details. Study the tests and edge cases, then implement the Rust version properly.
2. **Datasource ID mistakes**: Setting `datasource_id: None`, choosing the wrong `DatasourceId` variant, or missing an error-path assignment breaks assembly. See [Datasource IDs: The Assembly Bridge](#datasource-ids-the-assembly-bridge).
3. **License data missing**: Run `./setup.sh` to initialize submodule
4. **Cross-platform paths**: Use `Path` and `PathBuf`, not string concatenation
5. **Line endings**: Be careful with `\n` vs `\r\n` in tests
6. **Unwrap in library code**: Use `?` or `match` instead
7. **Breaking parallel processing**: Ensure modifications maintain thread safety
8. **Incomplete testing**: Every feature needs comprehensive test coverage including edge cases
9. **Modifying golden test expected files**: See [Golden Test Expected Files: Change with Care](#golden-test-expected-files-change-with-care).
10. **Suppressing clippy warnings**: Never use `#[allow(...)]` or `#[expect(...)]` to ignore clippy errors or warnings as a shortcut or temporary workaround. Clippy suppressions are only acceptable when the lint is genuinely a false positive and the suppression is intended to be permanent. Every suppression must include a comment explaining why it is justified. If clippy flags something, fix the code properly.

## Porting Features from Original ScanCode

When porting behavior from the Python reference, use it as the spec for requirements, edge cases, outputs, and known bugs — never as a line-by-line implementation template.

### Porting and Parser Guardrails

1. **Research exhaustively**: read the original implementation, tests, and documentation before designing the Rust version.
2. **Aim for feature parity, not code parity**: preserve behavior and output semantics while using idiomatic Rust.
3. **Design for correctness**: use strong types, explicit error handling, and tests that cover edge cases and bug fixes from the original.
4. **Leverage Rust advantages**: prefer zero-copy parsing, compile-time guarantees, and designs that make invalid states unrepresentable.
5. **Document intentional differences**: if Rust diverges behaviorally, explain why and add tests that demonstrate the improvement.
6. **For parser-specific implementation rules**: follow [`docs/HOW_TO_ADD_A_PARSER.md`](docs/HOW_TO_ADD_A_PARSER.md), especially the security-first parsing constraints, declared-license normalization rules, datasource requirements, and assembly setup guidance.

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

## Dependency Scope Conventions

The `Dependency.scope` field uses **native ecosystem terminology** to preserve semantic fidelity. This enables accurate round-tripping and maintains compatibility with ecosystem-specific tooling.

### npm Ecosystem

- **npm/yarn/pnpm package.json**:
  - `"dependencies"` - Regular runtime dependencies
  - `"devDependencies"` - Development-only dependencies
  - `"peerDependencies"` - Required peer dependencies
  - `"optionalDependencies"` - Optional runtime dependencies
  - `"bundledDependencies"` - Dependencies bundled with the package

- **npm lockfiles** (package-lock.json, npm-shrinkwrap.json):
  - `"dependencies"` - Regular or optional runtime dependencies
  - `"devDependencies"` - Development dependencies

- **yarn lockfiles**:
  - `"dependencies"` - Regular runtime dependencies (v1 and v2+)
  - `"peerDependencies"` - Peer dependencies (v2+ only; v1 doesn't distinguish)

- **pnpm lockfiles** (nested dependencies):
  - `None` - Top-level packages (no scope)
  - `"dev"` - Development dependencies
  - `"peer"` - Peer dependencies
  - `"optional"` - Optional dependencies

### Python Ecosystem

- **pyproject.toml (PEP 621)**:
  - `None` - Regular runtime dependencies (from `dependencies` array)
  - `"<extra_name>"` - Optional dependency groups (from `optional-dependencies.<extra_name>`)

- **pyproject.toml (Poetry)**:
  - `"dependencies"` - Regular runtime dependencies (from `[tool.poetry.dependencies]`)
  - `"dev-dependencies"` - Development dependencies (from `[tool.poetry.dev-dependencies]`)
  - `"<group_name>"` - Dependency groups (from `[tool.poetry.group.<group_name>.dependencies]`)

- **setup.py, setup.cfg**:
  - `"install"` - Regular runtime dependencies
  - `"<extra_name>"` - Optional dependency groups (from `extras_require`)

- **poetry.lock**:
  - `None` - All dependencies (no scope distinction in lockfile)
  - `is_optional` flag indicates dev dependencies

- **Pipfile.lock**:
  - `"install"` - Regular runtime dependencies (from `default` section)
  - `"develop"` - Development dependencies (from `develop` section)

- **requirements.txt** (filename-based):
  - `"install"` - Regular requirements.txt
  - `"develop"` - requirements-dev.txt or requirements/dev.txt
  - `"test"` - requirements-test.txt or requirements/test.txt
  - `"docs"` - requirements-doc.txt or requirements/doc.txt

### Rust Ecosystem

- **Cargo.toml**:
  - `"dependencies"` - Regular runtime dependencies
  - `"dev-dependencies"` - Development-only dependencies
  - `"build-dependencies"` - Build-time dependencies

- **Cargo.lock**:
  - `"dependencies"` - All runtime dependencies (dev/build deps not in lockfile by design)

### Java Ecosystem

- **Maven pom.xml** (`<scope>` element):
  - `None` - Default scope (equivalent to `compile`)
  - `"compile"` - Compile and runtime (default)
  - `"test"` - Test-time only
  - `"provided"` - Provided by runtime environment
  - `"runtime"` - Runtime only (not compile-time)
  - `"system"` - System-provided JARs

### Cross-Ecosystem Normalization

The `scope` field is intentionally **not standardized** across ecosystems. For cross-ecosystem analysis:

- Use `is_runtime` flag: `true` for runtime dependencies, `false` for dev/test/build
- Use `is_optional` flag: `true` for optional dependencies
- Future: Consider adding `normalized_scope` enum for standardized queries

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
