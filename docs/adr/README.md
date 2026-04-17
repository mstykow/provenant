# Architectural Decision Records (ADRs)

This directory contains records of architectural decisions made during the development of Provenant.

## What is an ADR?

An Architectural Decision Record (ADR) is a document that captures an important architectural decision made along with its context and consequences. ADRs help:

- Preserve the reasoning behind key design decisions
- Onboard new contributors by explaining "why" not just "what"
- Avoid revisiting settled decisions without new information
- Document trade-offs and alternatives considered

## Format

Each ADR follows a consistent structure:

- **Status**: Proposed, Accepted, Deprecated, Superseded
- **Current Contract Owner**: Evergreen doc or code path that now owns the live contract
- **Context**: The problem or requirement that prompted the decision
- **Decision**: The architectural choice made
- **Consequences**: Trade-offs, benefits, and implications
- **Alternatives Considered**: Other options evaluated

## Index of ADRs

| ADR                                                      | Title                                           | Status   | Date       |
| -------------------------------------------------------- | ----------------------------------------------- | -------- | ---------- |
| [0001](0001-trait-based-parsers.md)                      | Trait-Based Parser Architecture                 | Accepted | 2026-02-08 |
| [0002](0002-extraction-vs-detection.md)                  | Extraction vs Detection Separation              | Accepted | 2026-02-08 |
| [0003](0003-golden-test-strategy.md)                     | Golden Test Strategy                            | Accepted | 2026-02-08 |
| [0004](0004-security-first-parsing.md)                   | Security-First Parsing                          | Accepted | 2026-02-08 |
| [0005](0005-auto-generated-docs.md)                      | Auto-Generated Documentation                    | Accepted | 2026-02-08 |
| [0006](0006-datasourceid-driven-package-assembly.md)     | DatasourceId-Driven Multi-Pass Package Assembly | Accepted | 2026-03-14 |
| [0007](0007-embedded-license-index-artifact-strategy.md) | Embedded License Index Artifact Strategy        | Accepted | 2026-03-29 |
| [0008](0008-output-schema-separation.md)                 | Output Schema Type Separation                   | Accepted | 2026-04-10 |
| [0009](0009-parser-submodule-structure.md)               | Parser Submodule Structure for Large Ecosystems | Accepted | 2026-04-17 |

## When to Create a New ADR

Create an ADR when a decision is:

- **Cross-cutting** - affects multiple modules, subsystems, or contributor workflows
- **Durable** - expected to stay true long enough that future contributors will need the rationale
- **Constraint-setting** - defines rules, contracts, or invariants other work must follow
- **Trade-off heavy** - reasonable alternatives existed and the choice needs justification

Avoid new ADRs for:

- Single-parser implementation details
- Temporary transition states that are still actively changing
- Small refactors without project-wide consequences

## Creating a New ADR

1. Copy the template: `cp docs/adr/template.md docs/adr/000N-short-title.md`
2. Fill in the sections with your decision context and rationale
3. Update this README with the new entry
4. Submit for review via pull request

## ADR Lifecycle

- **Proposed**: Under discussion, not yet implemented
- **Accepted**: Decision made and being followed
- **Deprecated**: No longer recommended but not yet replaced
- **Superseded**: Replaced by a newer ADR (link to the replacement)

Accepted ADRs are historical decision records, not the primary home for the live maintainer contract. When an accepted ADR starts to mislead contributors because examples, paths, or workflow links have drifted, it may receive a narrowly scoped maintenance update such as:

- a current-contract note pointing to the evergreen owner document
- corrected links to moved files or workflows
- small snippet fixes that prevent obviously stale guidance

Substantive decision changes should still be made with a new or superseding ADR.

## Further Reading

- [Documenting Architecture Decisions](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions) by Michael Nygard
- [ADR GitHub Organization](https://adr.github.io/)
