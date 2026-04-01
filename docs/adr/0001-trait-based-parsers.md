# ADR 0001: Trait-Based Parser Architecture

**Status**: Accepted  
**Authors**: Provenant team
**Supersedes**: None

> **Current contract owner**: [`../HOW_TO_ADD_A_PARSER.md`](../HOW_TO_ADD_A_PARSER.md) and [`../ARCHITECTURE.md`](../ARCHITECTURE.md) describe the live parser interface and data model. This ADR records the architectural decision to use a trait-based parser system.

## Context

Provenant needs a unified, type-safe way to handle multiple package ecosystems and file formats while maintaining:

1. **Compile-time guarantees** - Catch errors before runtime
2. **Easy extensibility** - Adding new parsers should be straightforward
3. **Testability** - Each parser should be independently testable
4. **Clear contracts** - Implementers should know exactly what to provide

The Python reference implementation uses runtime class inspection and dynamic dispatch, which works but lacks compile-time type safety and can lead to subtle runtime errors.

## Decision

We use a **trait-based parser system** where every parser exposes the same compile-time contract:

- a package-type identity
- a path-matching predicate
- an extraction entry point that returns normalized package data

### Implementation Pattern

Each parser is typically a zero-sized type with compile-time registration. The exact trait signature and helper APIs are intentionally owned by the code and contributor guide rather than repeated in this ADR.

### Unified Data Model

All parsers return the same normalized `PackageData` shape. The important decision recorded here is not the exact field list, but that identity, metadata, dependency, license, provenance, and ecosystem-specific extra data all flow through one shared package model instead of per-ecosystem result types.

## Consequences

### Benefits

1. **Type Safety**
   - Rust compiler ensures all parsers implement the required methods
   - Impossible to "forget" to implement a method
   - Refactoring is safe - compiler catches breaking changes

2. **Zero Runtime Cost**
   - Trait methods can be statically dispatched
   - No vtable lookups for performance-critical paths
   - Zero-sized types have no memory overhead

3. **Clear Contract**
   - New parser authors know exactly what to implement
   - Documentation is self-evident from the trait definition
   - IDE autocomplete shows required methods

4. **Easy Testing**
   - Each parser can be tested in isolation
   - No need for complex test harnesses
   - Simple unit tests: `assert!(NpmParser::is_match(path))`

5. **Ecosystem Normalization**
   - Single `PackageData` struct unifies all formats
   - Easier to generate SBOM/SPDX output
   - Consistent JSON serialization

### Trade-offs

1. **Less Dynamic**
   - Cannot add parsers at runtime (but we don't need to)
   - Parser registration is compile-time only
   - Acceptable trade-off for type safety

2. **Boilerplate**
   - Each parser needs a struct declaration + impl block
   - More verbose than Python's class-based approach
   - Mitigated by IDE snippets and clear patterns

3. **Learning Curve**
   - Contributors need to understand Rust traits
   - Not as immediately obvious as Python classes
   - Mitigated by comprehensive documentation and examples

## Alternatives Considered

### 1. Enum-Based Dispatch

**Approach**: one central enum with a variant for every parser.

**Rejected because**:

- Requires modifying central enum for every new parser
- Doesn't scale to 40+ parsers
- Makes testing harder (can't import parsers independently)

### 2. Dynamic Dispatch with `Box<dyn Parser>`

**Approach**: store parsers behind trait objects and dispatch at runtime.

**Rejected because**:

- Runtime overhead (vtable lookups)
- Heap allocation for trait objects
- Less idiomatic for stateless parsers
- Loses `const` benefits

### 3. Function Pointer Registry

**Approach**: register parser entry points as function pointers in a central registry.

**Rejected because**:

- No compile-time guarantees
- Hard to type-check
- Error-prone registration
- Resembles Python too closely (loses Rust advantages)

## Python Reference Comparison

**Python Approach** (from `reference/scancode-toolkit/`): runtime class metadata plus dynamic inspection/registration.

**Key Differences**:

| Aspect              | Python                     | Rust (Our Approach)       |
| ------------------- | -------------------------- | ------------------------- |
| **Type Safety**     | Runtime (duck typing)      | Compile-time (traits)     |
| **Dispatch**        | Dynamic (class inspection) | Static (monomorphization) |
| **Performance**     | Interpreted + vtables      | Zero-cost abstractions    |
| **Extensibility**   | Plugin system (runtime)    | Traits (compile-time)     |
| **Error Detection** | Runtime errors             | Compile-time errors       |

## Related ADRs

- [ADR 0002: Extraction vs Detection Separation](0002-extraction-vs-detection.md) - Why parsers only extract, never detect
- [ADR 0003: Golden Test Strategy](0003-golden-test-strategy.md) - How we validate parser correctness
- [ADR 0005: Auto-Generated Documentation](0005-auto-generated-docs.md) - How parser metadata is documented

## References

- [Rust Book: Traits](https://doc.rust-lang.org/book/ch10-02-traits.html)
- [Zero-Sized Types in Rust](https://doc.rust-lang.org/nomicon/exotic-sizes.html#zero-sized-types-zsts)
