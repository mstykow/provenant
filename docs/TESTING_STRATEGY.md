# Testing Strategy

## Philosophy

Provenant uses a **behavior-focused, multi-layered testing approach** that prioritizes intelligent coverage over arbitrary test quotas.

### Core Principles

1. **Test Behavior, Not Implementation**
   - Focus on what the code does, not how it does it
   - Tests should survive refactoring
   - Edge cases matter more than line coverage

2. **High-Value Tests Over High Counts**
   - One well-designed test beats ten redundant tests
   - Every test should verify meaningful behavior
   - No tests for the sake of reaching coverage targets

3. **Fast Feedback Loops**
   - Unit tests run in milliseconds (parallel execution)
   - Instant failure isolation
   - Developers get immediate actionable feedback

4. **Complementary Layers**
   - Doctests verify API documentation examples work
   - Unit tests verify component correctness
   - Scanner/assembly contract tests verify parser data survives real scan wiring
   - Golden tests catch regressions
   - System integration tests validate end-to-end behavior

---

## Test Architecture

### Layer 0: Doctests

**Purpose**: Verify API documentation examples work correctly and serve as living documentation

**Characteristics**:

- Code examples in `///` doc comments that run as tests
- Ensures documentation stays synchronized with code
- Provides working examples for users
- Runs with `cargo test --doc`

**When to Write**:

- For all public API functions with non-trivial usage
- When examples would help users understand the API
- For complex function signatures requiring setup examples

**Example**:

````rust
/// Extracts package metadata from a manifest file.
///
/// # Examples
///
/// ```rust
/// use provenant::scanner::process;
/// use provenant::progress::{ProgressMode, ScanProgress};
/// use std::path::PathBuf;
/// use std::sync::Arc;
///
/// let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
/// let result = process(&PathBuf::from("."), 50, progress, &patterns, &strategy)?;
/// println!("Found {} files", result.files.len());
/// ```
pub fn process(...) -> Result<ScanResult> {
    // Implementation
}
````

**Why This Matters**: Documentation examples that don't compile or fail are caught immediately. Users can trust that documented examples actually work.

**Location**: Inline in source code as `///` or `//!` doc comments

**Current Status**: Doctests cover the main API entry points.

---

### Layer 1: Unit Tests

**Purpose**: Verify individual components work correctly in isolation

**Characteristics**:

- Test single functions or small groups of related functions
- Mock external dependencies where appropriate
- Fast execution (parallel, minimal I/O)
- Pinpoint exact failure location

**When to Write**:

- Every parser function (field extraction, validation, transformation)
- Every edge case (empty input, malformed data, extreme values)
- Every business rule (dependency resolution, version constraints, PURL generation)

**Example**:

```rust
#[test]
fn test_parse_dependency_with_alternatives() {
    let deps = parse_dependency_field("libc6 | libc6-udeb", ...);
    assert_eq!(deps.len(), 2);
    assert!(deps[0].is_optional.unwrap());
    assert!(deps[1].is_optional.unwrap());
}
```

**Why This Matters**: When this test fails, you immediately know which exact function and input combination failed.

**Location**: Inline `#[cfg(test)] mod tests { ... }` blocks in implementation files or separate `*_test.rs` files in `src/parsers/`

---

### Layer 2: Golden Tests

**Purpose**: Catch regressions by comparing output against reference implementation

**Characteristics**:

- Compare full PackageData output to expected JSON
- Use real-world manifest files as test data
- Validate against Python ScanCode Toolkit reference
- Document intentional differences (see ADR 0003)

**When to Write**:

- After parser is stable and fully implemented
- When reference output available from Python ScanCode
- For parsers with complex output structures

**Example**:

```rust
#[test]
fn test_golden_debian_control() {
    let result = DebianControlParser::extract_package_data("testdata/debian/control");
    let expected = read_expected_json("testdata/debian/control.expected.json");
    assert_package_data_eq(result, expected);
}
```

**Why This Matters**: Prevents accidentally breaking feature parity with Python reference.

**Location**: Separate `*_golden_test.rs` files in `src/parsers/` with test data in `testdata/<ecosystem>-golden/`

**Test Utilities**: Uses `test_utils::compare_package_data_parser_only()` which:

- Skips dynamic/time-sensitive fields (identifiers, line numbers, matched_text)
- Handles optional license detection fields gracefully
- Provides clear diff messages on mismatch

Parser goldens are intentionally narrower than scan/assembly or output tests. They validate
`PackageData` extraction, but by themselves they will not catch downstream contract drift such as
package visibility after assembly, `for_packages` assignment, `datafile_paths`, or dependency
hoisting.

---

### Layer 3: Scanner/Assembly Contract Tests

**Purpose**: Validate scanner-wired package behavior that sits above parser-only extraction and below
full-system integration (file discovery → parsing → assembly/output contracts)

**Characteristics**:

- Run the full `collect_paths()`/`process_collected()` pipeline for targeted fixtures
- Verify package visibility after assembly, `for_packages`, dependency hoisting, and
  `datafile_paths`
- Catch parser regressions that only appear once scanner wiring and assembly are involved
- Stay close to the owning parser behavior while exercising higher-level contracts

**Location**: `src/parsers/*_scan_test.rs`

**When to Write**:

- When parser behavior depends on scanner wiring or assembly/file-reference handling
- When installed metadata must link files back to the assembled package
- When downstream package/dependency contracts must stay stable
- For broad retroactive coverage work across many existing parsers

**Example Scenarios Covered**:

- installed metadata linking files back to the assembled package
- archive/extracted layouts where normalized paths matter
- intentionally unassembled formats whose scanner behavior must stay stable
- package-input fields whose downstream consumers depend on the assembled/output contract (for
  example `purl`, `namespace`/`name`, declared-license fields, dependency hoisting, and
  `datafile_paths`)

**Why This Matters**: Parser golden tests prove extraction; scanner/assembly contract tests prove
that the extracted data survives the real scan pipeline and assembly behavior.

Scanner-owned detector surfaces that do not implement `PackageParser` still belong to this layer.
For example, compiled-binary package extraction is intentionally gated by scanner options rather
than path-based parser registration, so its default contract tests should prove both the opt-in gate
and the resulting package data shape.

---

### Layer 4: System Integration Tests

**Purpose**: Validate end-to-end scanner behavior and user-facing contracts across the full system

**Location**: top-level `tests/*.rs` suites such as `tests/scanner_integration.rs`,
`tests/progress_cli_integration.rs`, `tests/scanner_copyright_credits.rs`, and
`tests/output_format_golden.rs`

**Characteristics**:

- Test the full `process()` pipeline across multiple subsystems
- Verify multi-parser coordination
- Validate CLI/runtime behavior and graceful degradation
- Test output-format and fixture-backed contracts that matter to end users

**When to Write**:

- After major scanner changes
- When adding new scanner features (filters, output formats)
- To verify cross-parser interactions
- To test error handling across the pipeline

**Example Scenarios Covered**:

- Multi-parser discovery (npm + pypi + cargo in same directory)
- Output format structure validation (all required fields present)
- Error handling (malformed manifests don't crash scanner)
- Exclusion patterns work correctly
- Max depth limits are respected
- Empty directories handled gracefully
- Scan-result cache entry persistence (first scan writes cache, repeat scan reuses stable findings)
- Cache-control CLI wiring behavior (`--cache`, `--cache-dir`, `--cache-clear`) via startup/runtime tests

**Why This Matters**: Layer 3 proves scanner-wired package contracts; Layer 4 proves the system still works together from the user's perspective.

These are **not** a replacement for the top-level `tests/*.rs` suites. Parser-local scan tests stay
close to the owning parser behavior they protect, while system integration tests stay cross-parser
and user-facing.

For parsers that emit meaningful downstream package/dependency data, Layer 3 should be treated as
the default expectation rather than an optional extra.

When a detector surface is intentionally scanner-gated, pair Layer 3 tests with detector-level
goldens near the owning module so extraction drift and scanner-wiring drift are both covered.

**Layer 4 Example**:

```rust
#[test]
fn test_scanner_discovers_all_registered_parsers() {
    let result = process("testdata/integration/multi-parser", ...);

    assert!(result.files.iter().any(|f| f.package_data[0].package_type == Some("npm")));
    assert!(result.files.iter().any(|f| f.package_data[0].package_type == Some("pypi")));
    assert!(result.files.iter().any(|f| f.package_data[0].package_type == Some("cargo")));
}
```

---

## Rust vs Python Comparison

### Python ScanCode Toolkit Approach

**Structure**:

- Primarily golden tests (parse file → compare to `.expected.json`)
- Tests entire pipeline at once
- Often relies on a compact set of fixture-backed tests per ecosystem

**Trade-offs**:

- ✅ Catches regressions in full output
- ❌ Hard to debug when tests fail (which field? which line?)
- ❌ Large JSON diffs are difficult to interpret
- ❌ Slower execution (file I/O, JSON serialization)

### Provenant Approach

**Structure**:

- Doctests for API documentation verification
- Comprehensive unit tests for component behavior
- Scanner/assembly contract tests for parser data after real scan wiring
- Golden tests for regression detection
- System integration tests for end-to-end validation

**Trade-offs**:

- ✅ Immediate failure isolation (know exactly what broke)
- ✅ Fast parallel execution (minimal I/O in unit tests)
- ✅ Easy to maintain (update specific assertions, not large JSON files)
- ✅ Better coverage of edge cases
- ✅ Tests survive refactoring (test behavior, not implementation)
- ❌ More tests to write initially (but pays off long-term)

**Performance Advantage**: Rust tests typically run 3-5x faster than equivalent Python tests due to parallel execution and no interpreter overhead.

---

## Testing Guidelines

### What Makes a Good Test

**DO**:

- Test observable behavior (inputs → expected outputs)
- Use descriptive test names (`test_parse_debian_dependency_with_version_constraint`)
- Test edge cases (empty strings, Unicode, extreme values)
- Keep tests independent (no shared state between tests)
- Use real-world test data where possible

**DON'T**:

- Test implementation details (private functions, internal state)
- Write tests just to hit coverage targets
- Copy-paste tests (use helper functions for common patterns)
- Ignore failing tests (fix or remove them)
- Skip error cases (test both success and failure paths)

### When to Use Each Test Type

| Scenario                               | Test Type                            |
| -------------------------------------- | ------------------------------------ |
| Public API function with complex usage | Doctest                              |
| New parser function                    | Unit test                            |
| Edge case discovered                   | Unit test                            |
| Parser fully implemented               | Golden test                          |
| Scanner feature added                  | Integration test                     |
| Bug found in production                | Unit test (reproduce) + fix + verify |
| Refactoring parser internals           | Unit tests should still pass         |
| Changing API signature                 | Doctests will break (expected)       |
| Changing output format                 | Golden tests will break (expected)   |

---

## Test Organization

### File Structure

```text
src/parsers/
├── npm.rs                    # Implementation
├── npm_test.rs               # Unit tests (co-located)
├── npm_scan_test.rs          # Scanner/assembly contract tests
└── npm_golden_test.rs        # Golden tests (separate file)

tests/
├── scanner_integration.rs    # Cross-parser integration tests
├── progress_cli_integration.rs
├── scanner_copyright_credits.rs
└── output_format_golden.rs   # Fixture-backed output contract tests

testdata/
├── npm/                      # Unit test data
│   ├── package.json
│   └── package-lock.json
├── npm-golden/               # Golden test data with .expected files
│   ├── simple/
│   │   ├── package.json
│   │   └── package.json.expected
│   └── complex/
│       ├── yarn.lock
│       └── yarn.lock.expected
└── integration/              # Integration test data
    └── multi-parser/
        ├── package.json
        ├── pyproject.toml
        └── Cargo.toml
```

### Naming Conventions

**Unit Tests**:

- `test_<function_name>_<scenario>` (e.g., `test_parse_dependency_with_alternatives`)
- `test_<component>_<edge_case>` (e.g., `test_rfc822_parser_handles_empty_fields`)

**Golden Tests**:

- `test_golden_<ecosystem>_<format>` (e.g., `test_golden_npm_package_json`)

**Scanner/Assembly Contract Tests**:

- `test_<behavior>_<scanner_or_assembly_scenario>`

**System Integration Tests**:

- `test_<scanner_feature>_<scenario>` (e.g., `test_scanner_discovers_all_registered_parsers`)

---

## Running Tests

### All Tests

```bash
cargo test                    # Run all tests except golden tests
cargo test --lib              # Run only library tests (faster, excludes integration)
cargo test --doc              # Run only doctests
cargo test --test scanner_integration  # Run one top-level system integration suite
cargo test --features golden-tests  # Include golden tests (slower, compares against Python ScanCode)
```

> **Note**: Golden tests (comparing output against Python ScanCode reference) are gated behind the `golden-tests` feature flag because they are slow and require the reference submodule. They run automatically in CI but are excluded from `cargo test` by default for faster local development.

We do **not** feature-gate scanner/assembly contract tests or system integration tests. Those layers are
still part of the normal test surface; CI selects them with explicit Cargo test targets/filters rather
than hiding them behind additional features.

### Specific Test Categories

```bash
cargo test npm_test           # All npm unit tests
cargo test --lib _scan_test:: # All parser-local scanner/assembly contract tests
cargo test golden             # All golden tests
cargo test --test scanner_integration  # Cross-parser integration suite
cargo test --doc              # All API documentation examples
```

### Golden Fixture Maintenance Commands

Use distinct commands for the two golden fixture domains:

```bash
# Parser golden snapshots (.expected.json)
cargo run --manifest-path xtask/Cargo.toml --bin update-parser-golden -- --list
cargo run --manifest-path xtask/Cargo.toml --bin update-parser-golden -- <ParserType> <input_file> <output_file>
./scripts/update_parser_golden.sh <ParserType> <input_file> <output_file>

# Copyright golden YAML fixtures (authors / ics / copyrights)
# Note: "ics" here means Android Ice Cream Sandwich (Android 4.0) fixture corpus from
# the ScanCode reference test data.
cargo run --manifest-path xtask/Cargo.toml --bin update-copyright-golden -- copyrights --list-mismatches --show-diff
cargo run --manifest-path xtask/Cargo.toml --bin update-copyright-golden -- copyrights --filter <pattern> --write
./scripts/update_copyright_golden.sh copyrights --list-mismatches --show-diff
```

For copyright golden fixtures, this repository's YAML files are treated as Rust-owned expectations. During updates, `update-copyright-golden` strips legacy `expected_failures` keys so Python reference sync does not reintroduce Python-only xfail metadata.

`update-copyright-golden --list-mismatches` is a Python-reference parity precheck (Python expected values vs current Rust detector output). This is different from golden tests, which validate Rust output against local Rust-owned fixture expectations.

Recommended maintenance flow for copyright fixtures:

1. Run `--list-mismatches --show-diff` to identify Python parity gaps.
2. Use default `--write` mode (optionally with `--filter`) only for parity-safe syncs from Python reference fixture YAML.
3. Use `--sync-actual --write` for intentional Rust-specific expectations.
4. Run golden tests to validate local Rust-owned expectations.

Parser golden snapshot maintenance is separate: `update-parser-golden` does not sync from Python reference; it always writes expected JSON from current Rust parser output.

For canonical script purpose and full CLI argument reference, see [`scripts/README.md`](../scripts/README.md).

### Single Test

```bash
cargo test test_parse_dependency_with_alternatives
```

### Ignored Tests

Golden tests are gated behind the `golden-tests` feature flag:

```bash
cargo test --features golden-tests             # Run all tests including golden tests
cargo test --lib --features golden-tests golden # Run only golden tests
```

### CI/CD

Quality gates run automatically on:

- Every commit (via Lefthook pre-commit hooks: formatting, linting, and docs/file-quality checks)
- Every push to main
- Every pull request

The full test suites run in CI on pushes and pull requests. All tests must pass before merging. CI uses a minimal split so the heaviest Rust test layers no
longer sit on the same critical path as the main Rust quality job, without introducing lots of tiny shards.
Commands:

- **Rust Quality**
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo check --all --verbose`
  - `cargo test --doc --release --verbose`
- **Rust Library Tests**
  - `cargo test --lib --release --verbose -- --skip _scan_test::`
- **Rust Scan/Integration Tests**
  - `cargo test --lib --release --verbose _scan_test::`
  - `cargo test --test scanner_integration --release --verbose`
  - `cargo test --test scanner_copyright_credits --release --verbose`
  - `cargo test --test progress_cli_integration --release --verbose`
  - `cargo test --test output_format_golden --release --verbose`
- **Golden Tests**
  - `cargo test --all --verbose --features golden-tests` via the existing golden-test shard matrix

---

## Quality Gates

Before marking a parser complete, verify:

- [ ] **Unit tests** cover all public functions and edge cases
- [ ] **Golden tests** exist for at least one real-world file per format
- [ ] **Layer 3 scan/assembly contract test** verifies parser data survives scanner wiring and assembly when applicable
- [ ] **Layer 4 integration test** verifies parser is discovered and invoked correctly (if adding new ecosystem)
- [ ] All tests pass (`cargo test`)
- [ ] No clippy warnings (`cargo clippy`)
- [ ] Code formatted (`cargo fmt`)

---

## Related Documentation

- **[ADR 0003: Golden Test Strategy](adr/0003-golden-test-strategy.md)** - Why and how we use golden tests
- **[HOW_TO_ADD_A_PARSER.md](HOW_TO_ADD_A_PARSER.md)** - Step-by-step parser implementation guide
- **[ARCHITECTURE.md](ARCHITECTURE.md)** - System design and test infrastructure

---

## Summary

**Testing is about confidence, not coverage.**

Write tests that:

1. Verify meaningful behavior
2. Catch real bugs
3. Survive refactoring
4. Provide fast feedback

**Our multi-layered approach ensures**:

- Doctests verify API documentation examples actually work
- Unit tests verify components work correctly
- Scanner/assembly contract tests verify parser data survives real scan wiring and assembly
- Golden tests ensure feature parity with Python reference
- System integration tests validate end-to-end and user-facing behavior
- Fast CI/CD feedback loop (parallel execution, instant failure isolation)

**Result**: High-quality, maintainable test suite that gives developers confidence to refactor and evolve the codebase.
