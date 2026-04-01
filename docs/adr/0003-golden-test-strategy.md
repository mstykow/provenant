# ADR 0003: Golden Test Strategy

**Status**: Accepted  
**Authors**: Provenant team
**Supersedes**: None

> **Current contract owner**: [`../TESTING_STRATEGY.md`](../TESTING_STRATEGY.md) defines the live test-layer taxonomy, golden-fixture ownership rules, and current CI commands. This ADR records the decision to use golden tests as part of the verification model.

## Context

We need a reliable way to verify that Provenant produces output functionally equivalent to the Python ScanCode Toolkit reference implementation. Key challenges:

1. **Feature Parity Verification** - How do we prove our parsers extract the same data?
2. **Regression Prevention** - How do we catch unintended behavior changes?
3. **Edge Case Coverage** - How do we ensure rare formats and corner cases work?
4. **Architectural Differences** - How do we handle intentional implementation differences?

The Python reference implementation has extensive test data and expected outputs, but our Rust implementation may legitimately differ in structure (e.g., single package vs array, field ordering).

## Decision

We use **golden testing** where parsers are validated against reference outputs from ScanCode Toolkit, with documented exceptions for intentional architectural differences.

### Golden Test Workflow

```text
┌──────────────────┐
│ testdata/        │
│ npm/package.json │
│                  │
└────────┬─────────┘
         │
         ├─────────────────────────┐
         │                         │
         ▼                         ▼
┌──────────────────┐      ┌──────────────────┐
│ Python ScanCode  │      │ Provenant        │
│                  │      │                  │
│ scancode -p ...  │      │ NpmParser::      │
│                  │      │ extract_first_   │
└────────┬─────────┘      └────────┬─────────┘
         │                         │
         ▼                         ▼
┌──────────────────┐      ┌──────────────────┐
│ expected.json    │      │ actual output    │
│ (reference)      │      │                  │
└────────┬─────────┘      └────────┬─────────┘
         │                         │
         └─────────┬───────────────┘
                   │
                   ▼
            ┌─────────────┐
            │ JSON diff   │
            │ comparison  │
            └─────────────┘
```

### Implementation Pattern

**1. Generate Reference Output** (historical example for one-time setup per test case):

Set up the reference submodule, run the corresponding ScanCode command once, and save the resulting reference fixture alongside the Rust-owned test data.

**2. Create Golden Test** (in Rust):

Create a focused test that loads one fixture, runs the owning parser or subsystem entry point, and compares against the expected artifact or semantic projection.

**3. Handle Intentional Differences**:

Document intentional differences directly in the test metadata or fixture ownership notes so the exception is explicit and reviewable.

### Test Organization

```text
src/parsers/
├── npm.rs                    # Implementation
├── npm_test.rs               # Unit tests
└── npm_golden_test.rs        # Golden tests

testdata/
├── npm/
│   ├── package.json          # Test input
│   ├── package-lock.json
│   └── yarn.lock
└── expected/
    ├── npm-package.json      # Reference output
    ├── npm-lockfile.json
    └── npm-yarn.json
```

## Consequences

### Benefits

1. **Feature Parity Proof**
   - Direct comparison with Python reference
   - Catches missing fields or incorrect values
   - Validates edge case handling

2. **Regression Prevention**
   - Any change that breaks compatibility is caught immediately
   - Prevents accidental feature removal
   - Safe refactoring with confidence

3. **Documentation of Differences**
   - Ignored tests document WHY we differ from Python
   - Architectural decisions are explicit
   - Future maintainers understand context

4. **Real-World Test Data**
   - Uses actual package manifests from ecosystems
   - Covers edge cases found in production
   - Validates against proven reference implementation

5. **Continuous Validation**
   - Pre-commit hooks run fast local quality gates (format/lint/docs checks)
   - CI validates on every push
   - Automated regression detection

### Trade-offs

1. **Test Maintenance**
   - Must regenerate expected outputs if Python changes
   - Need to document intentional differences
   - Acceptable: Worth the confidence in correctness

2. **Blocked Tests**
   - Some tests blocked on detection engine (license normalization)
   - Can't validate full output until detection is implemented
   - Acceptable: Unit tests validate extraction correctness

3. **JSON Structure Differences**
   - Must handle field ordering differences
   - Some fields may be legitimately different (e.g., array vs single object)
   - Mitigated: Custom comparison logic, documented exceptions

### Documented Architectural Differences

#### 1. Swift: Package Structure

**Python Approach**: represent the manifest result as multiple package-like records.

**Rust Approach**: normalize the same information into one package record with dependency edges.

**Rationale**: Both are valid representations. Rust uses normalized `PackageData` struct for consistency. Validated via comprehensive unit tests.

**Decision**: Document the difference and rely on the appropriate test layer.

For Swift, parser-only goldens may still need special handling because the Rust implementation intentionally models a graph differently from older Python expectations.

For CocoaPods, parser-only goldens are active again because the current Rust fixtures and expectations now pin the parser contract directly rather than relying on the older ignored-golden workaround.

These examples are historical illustrations of the decision, not the authoritative current command set. For the live test-layer model, fixture ownership rules, and CI commands, follow [`../TESTING_STRATEGY.md`](../TESTING_STRATEGY.md).

#### 2. Alpine: Provider Field (Beyond Parity)

**Python**: Provider field (`p:`) is ignored ("not used yet")

**Rust**: Provider field fully extracted and stored in `extra_data.providers`

**Rationale**: We implement features that Python has marked as TODO. This is intentional improvement.

**Decision**: Document as enhancement, ignore golden test for provider field.

## Alternatives Considered

### 1. Unit Tests Only (No Golden Tests)

**Approach**: test individual parser functions without comparing to Python reference.

**Rejected because**:

- No proof of feature parity with Python reference
- Easy to miss fields or edge cases
- Manual assertion maintenance is error-prone
- Doesn't catch regressions against reference

### 2. Snapshot Testing (insta crate)

**Approach**: generate Rust snapshots and review diffs manually.

**Rejected because**:

- No comparison with Python reference (our source of truth)
- Snapshot becomes the truth (circular validation)
- Harder to verify feature parity
- Doesn't validate against proven reference implementation

### 3. Property-Based Testing (proptest)

**Approach**: generate random inputs and verify coarse-grained properties.

**Partial acceptance**: We use property-based testing for security (DoS protection, invalid input handling), but NOT as primary validation strategy.

**Why not primary**:

- Can't verify feature parity with reference
- Doesn't test real-world manifests
- Hard to generate valid package manifests
- Golden tests are more effective for correctness

### 4. Integration Testing via CLI

**Approach**: run the full CLI and compare emitted artifacts.

**Partial acceptance**: We do this at CI level, but NOT as primary test strategy.

**Why not primary**:

- Slower than unit/golden tests
- Harder to debug failures
- Can't test parsers in isolation
- Golden tests at parser level are more granular

## Implementation Guidelines

### Feature Flag

Golden tests are gated behind the `golden-tests` Cargo feature flag to keep the default `cargo test` fast.

All `*_golden_test.rs` modules are conditionally compiled with `#[cfg(all(test, feature = "golden-tests"))]`. CI always runs with `--features golden-tests`.

### When to Write a Golden Test

✅ **Write golden test when**:

- Parser is complete and stable
- Reference output available from Python ScanCode
- Edge cases covered by real test data

❌ **Don't write golden test when**:

- Feature depends on detection engine (not yet built)
- Architectural difference makes comparison meaningless
- Parser is still experimental/unstable

### When to Ignore a Golden Test

Document with `#[ignore = "reason"]` when:

1. **Detection Engine Dependency**: Test requires license normalization or copyright detection
2. **Architectural Difference**: Intentional implementation difference (e.g., data structure)
3. **Beyond Parity**: We implement features Python has as TODO/missing

**Always document WHY** in the ignore attribute.

### Custom Comparison Logic

Comparison helpers should normalize legitimate differences such as ordering, null-vs-missing representation, and URL normalization before asserting equivalence.

## Quality Gates

Before marking a parser complete:

- ✅ All relevant golden tests passing OR documented as ignored with reason
- ✅ Unit tests cover extraction logic
- ✅ Edge cases validated (empty files, malformed input, etc.)
- ✅ Real-world test data included
- ✅ Performance acceptable (benchmarked)

## Related ADRs

- [ADR 0001: Trait-Based Parser Architecture](0001-trait-based-parsers.md) - Parser structure enables golden testing
- [ADR 0002: Extraction vs Detection Separation](0002-extraction-vs-detection.md) - Why some tests are blocked on detection engine
- [ADR 0004: Security-First Parsing](0004-security-first-parsing.md) - Security property testing complements golden tests

## References

- Python reference test data: `reference/scancode-toolkit/tests/packagedcode/data/`
- Golden test examples: `src/parsers/*_golden_test.rs`
- Test infrastructure: `src/parsers/golden_test_utils.rs`
- CI configuration: `.github/workflows/check.yml`
