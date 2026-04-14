# Provenant and ScanCode Toolkit

Provenant is an independent Rust implementation inspired by [ScanCode Toolkit](https://github.com/aboutcode-org/scancode-toolkit). It is built for users who want ScanCode-aligned scanning workflows with a native Rust engine, simpler installation, and the same respect for correctness and compatibility that made ScanCode valuable in the first place.

## Relationship at a Glance

- Provenant reimplements the scanning engine in Rust.
- Provenant continues to rely on the upstream ScanCode license and rule data maintained by nexB Inc. and the AboutCode community.
- ScanCode Toolkit remains the reference ecosystem Provenant studies for behavior, parity validation, and output semantics.
- Provenant is meant to be a respectful companion in that ecosystem, not a dismissal of the upstream project's domain expertise.

## High-Level Comparison

| Area                      | Provenant                                                                         | ScanCode Toolkit                                                          |
| ------------------------- | --------------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| Engine language           | Rust                                                                              | Python                                                                    |
| Install model             | Single native binary or `cargo install`                                           | Python-based toolkit environment                                          |
| License and rule data     | Uses upstream ScanCode license and rule data                                      | Native upstream source of that data                                       |
| Compatibility goal        | Strong compatibility with ScanCode workflows and output semantics where practical | Reference implementation                                                  |
| Execution model           | Native parallel scanning                                                          | Python toolkit runtime                                                    |
| Primary value proposition | Rust-native engine, simpler deployment, compatibility-focused workflows           | Mature reference implementation plus expert-maintained rules and research |

## Why Provenant Exists

Provenant exists for teams that want a Rust-native scanner without giving up the trust built around the ScanCode ecosystem. The project focuses on:

- native performance and parallelism
- single-binary installation and CI friendliness
- compatibility-focused output and workflow design
- safe static parsing and explicit parser guardrails
- bug fixes and architecture improvements discovered during parity work

In practice, that means Provenant intentionally makes some implementation choices that differ from Python ScanCode while staying respectful to the same ecosystem: static parsers avoid code execution, parser/resource bounds are explicit, and the shipped surface already covers additional ecosystems plus documented improvements in overlapping parser families. The proof lives in [ADR 0004: Security-First Parsing](adr/0004-security-first-parsing.md), [Supported Formats](SUPPORTED_FORMATS.md), and [Beyond-Parity Improvements](improvements/README.md).

## Why ScanCode Still Matters

ScanCode Toolkit remains deeply important to this ecosystem. Its license corpus, rule curation, and long-running domain research are extraordinarily valuable. Provenant keeps that upstream work front-and-center by continuing to use the upstream ScanCode license and rule data rather than attempting to replace that expert-maintained knowledge base.

## When Provenant May Be a Good Fit

Choose Provenant when you want:

- a native binary that is easy to drop into CI or local workflows
- ScanCode-aligned scanning with a Rust implementation
- package, dependency, license, and provenance scanning in one tool
- a project that treats parity and correctness as first-class goals while still pursuing implementation improvements

## Compatibility and Verification Evidence

Provenant does not treat "compatible" as a vague marketing claim. Public verification work and maintained references live in:

- [Output Format Parity Scorecard](implementation-plans/output/PARITY_SCORECARD.md)
- [Package Detection Verification Benchmarks](BENCHMARKS.md)
- [xtask compare-outputs workflow](../xtask/README.md)

Those documents track what has already been compared against ScanCode, provide the maintained package-detection verification record, and point to the saved verification artifacts used to review remaining gaps.

One important nuance in that comparison work is that Provenant distinguishes between ScanCode's
formal schema contract and ScanCode's common emitted defaults. For dependency booleans such as
`is_runtime`, `is_optional`, `is_pinned`, and `is_direct`, the formal ScanCode schema allows
nullable and omitted values. Provenant therefore keeps these fields unset when the datasource does
not actually prove them, rather than coercing output to common ScanCode defaults and overstating
dependency intent.

## Related Docs

- [README](../README.md) for installation, usage, and positioning
- [SECURITY.md](../SECURITY.md) for vulnerability reporting guidance
- [NOTICE](../NOTICE) for upstream attribution, code/data licensing split, and ScanCode-derived data notices
- [Architecture](ARCHITECTURE.md) for system design and performance characteristics
- [Testing Strategy](TESTING_STRATEGY.md) for parity and verification philosophy
- [Supported Formats](SUPPORTED_FORMATS.md) for current format and ecosystem coverage
- [License Detection Architecture](LICENSE_DETECTION_ARCHITECTURE.md) for embedded index and rule-loading details
