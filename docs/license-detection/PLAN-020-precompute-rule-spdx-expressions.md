# Plan 020: Precompute Rule SPDX Expressions

## Status

Complete.

Rust now precomputes rule SPDX expressions during index construction and keeps
them in index metadata keyed by rule identifier.

## Implemented Design

The implementation preserves the plan's core constraint:

- ScanCode expressions remain the canonical internal rule identity.
- SPDX renderings are precomputed only as optional derived metadata.
- Matchers consume the precomputed SPDX rendering when it is known.
- Detection assembly still retains late conversion behavior as a fallback.

The final implementation stores this derived data in the index metadata layer
rather than adding another canonical field to the rule identity model.

## Resulting Improvements

- exact/hash/Aho/sequence matchers can populate match-level SPDX expressions
  earlier and more consistently,
- rule-level metadata no longer depends on late assembly to be trustworthy, and
- detection/output layers only need fallback SPDX conversion for cases that
  cannot be safely derived ahead of time.

## Relevant Rust Implementation Points

- `src/license_detection/index/mod.rs`
- `src/license_detection/index/builder/mod.rs`
- `src/license_detection/hash_match.rs`
- `src/license_detection/aho_match.rs`
- `src/license_detection/seq_match/matching.rs`
- `src/license_detection/detection/mod.rs`
