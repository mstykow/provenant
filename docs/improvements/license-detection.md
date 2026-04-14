# License Detection: Beyond-Parity Improvements

## Type

- 🐛 Bug Fix + 🔍 Enhanced + 🛡️ Security + ⚡ Performance

## Python Reference Status

- SPDX-LID matcher fans out one SPDX identifier to every rule sharing the same license expression, producing thousands of duplicate matches.
- The whole-query run cache becomes stale after SPDX subtraction, so exact AHO and approximate matchers accidentally observe pre-subtraction token state.
- `qcontains()` and `qoverlap()` use bounds-only comparison, incorrectly reporting containment/overlap when sparse positional spans share no actual positions.
- `surround()` merge in `merge_overlapping_matches()` does not verify positional overlap before combining matches, allowing zero-overlap merges that inflate coverage scores.
- Candidate selection in sequence matching clones entire `Rule` structs (30+ fields including large `text` and `tokens`) per candidate.
- Rule references in `LicenseMatch` require repeated index lookups.

## Rust Improvements

1. **🐛 Bug Fix**: SPDX-LID deduplication — one match per SPDX identifier occurrence instead of one per matching rule
2. **🐛 Bug Fix**: `qcontains()` and `qoverlap()` use actual position-set semantics instead of bounds-only comparison, correctly handling mixed `PositionSpan` representations
3. **🐛 Bug Fix**: `surround()` merge requires `qoverlap > 0` before combining matches, preventing zero-overlap merges from inflating coverage scores
4. **🔍 Enhanced**: Immutable whole-query snapshot for AHO input, avoiding Python's stale-cache coupling while preserving parity output
5. **🔍 Enhanced**: Explicit `aho_extra_matchables` tracking of SPDX-subtracted positions, with post-AHO dedup filtering for straddling reference matches
6. **🔍 Enhanced**: `PositionSpan` as a unified enum (`Range` / `Discrete`) with custom `PartialEq` that compares semantically, not representationally
7. **⚡ Performance**: Candidate struct uses `&'a Rule` borrowed reference instead of cloning entire `Rule` per candidate
8. **🛡️ Security**: Thread-safe design — `Arc<LicenseDetectionEngine>` shared across scanner threads; no global mutable state

## Improvement 1: SPDX-LID Deduplication (Bug Fix)

### Python Implementation

```python
# match_spdx_lid.py — one LicenseMatch per matching rule
for rule in matching_rules:
    yield LicenseMatch(rule=rule, ...)
```

A single `SPDX-License-Identifier: MIT` produces ~1000+ matches (one for every rule with `license_expression: "mit"`).

### Our Rust Implementation

`rid_by_spdx_key` in the index builder maps each SPDX key to a single canonical rule. The SPDX-LID matcher resolves expressions through that map and creates one `LicenseMatch` per identifier occurrence.

**Impact**: Correct SPDX match count. Reduces downstream merge/refine work significantly.

## Improvement 2: Position-Aware `qcontains()` and `qoverlap()` (Bug Fix)

### Python Implementation

```python
# Span.__contains__ — bounds-only, no positional overlap check
```

When one match has `qspan_positions: None` (contiguous range) and another has `qspan_positions: Some([...])` (scattered positions), bounds-only comparison can report false containment or overlap.

### Our Rust Implementation

```rust
pub fn qcontains(&self, other: &LicenseMatch) -> bool {
    other
        .query_span()
        .iter()
        .all(|pos| self.query_span().contains(pos))
}

pub fn qoverlap(&self, other: &LicenseMatch) -> usize {
    other
        .query_span()
        .iter()
        .filter(|&p| self.query_span().contains(p))
        .count()
}
```

Both operate through `PositionSpan::iter()` and `PositionSpan::contains()`, which are representation-agnostic (correct for any combination of `Range` and `Discrete` spans).

**Impact**: Correct positional overlap semantics. Fixes CDDL 1.0 vs 1.1 mis-selection where bounds-only overlap reported 252 tokens of overlap when only 164 positions actually overlapped.

## Improvement 3: Surround Merge Overlap Guard (Bug Fix)

### Python Implementation

`merge_overlapping_matches()` uses `surround()` (bounds-only comparison) to decide whether to merge two matches. When `surround()` returns true, the matches are combined unconditionally — no positional overlap check. For contiguous spans this is harmless, but for sparse `Discrete` spans, bounds can enclose without any actual positions in common.

### Our Rust Implementation

Both surround branches in `merge_overlapping_matches()` require `qoverlap > 0` before merging:

```rust
if current.surround(next) && current.qoverlap(next) > 0 { ... }
if next.surround(current) && next.qoverlap(current) > 0 { ... }
```

This mirrors the existing `qoverlap > 0` guard in the diagonal-overlap merge block and prevents zero-overlap merges that would inflate coverage scores.

**Impact**: Stricter than Python (which has the same gap). No golden test regressions — all 19 license detection golden suites pass.

## Improvement 4: Immutable Whole-Query Snapshot for AHO (Enhanced)

### Python Implementation

`Query.whole_query_run()` is memoized. After SPDX matching calls `Query.subtract()`, the cached `_whole_query_run` retains stale `_high_matchables` / `_low_matchables`. Exact AHO reuses the stale snapshot, accidentally seeing pre-subtraction token state.

### Our Rust Implementation

`WholeQueryRunSnapshot` is an immutable clone taken before any subtraction:

- `whole_query_run` is created at pipeline start with `query: None` (no back-reference to live `Query`)
- All data access dispatches to the frozen snapshot
- SPDX subtraction mutates the live `query.high_matchables` / `query.low_matchables` independently
- AHO sees the pre-subtraction snapshot (matching Python's effective behavior) but through clean architectural separation, not accidental stale cache

**Impact**: Preserves Python-visible parity output while avoiding hidden cache coupling. Makes the intentional "AHO sees SPDX tokens" behavior explicit and auditable.

## Improvement 5: Explicit AHO Extra Matchables (Enhanced)

### Python Implementation

No explicit tracking. The stale cache accidentally makes AHO-eligible tokens visible after SPDX subtraction.

### Our Rust Implementation

`aho_extra_matchables: PositionSet` explicitly records positions subtracted from the live query during SPDX matching. After AHO matching, `aho_match_with_extra_matchables()` applies a dedup filter that drops reference matches straddling both SPDX-only and live positions when a more specific SPDX submatch exists.

`matched_qspans` serves as an explicit overlay for stop checks in later matching stages, not coupled to any cached `QueryRun` internals.

**Impact**: Same effective behavior as Python, but the mechanism is intentional and documented rather than accidental.

## Improvement 6: Unified `PositionSpan` (Enhanced)

### Python Implementation

`Span` is always discrete (set of positions) with no contiguous-range fast path.

### Our Rust Implementation

```rust
pub enum PositionSpan {
    Range { start: usize, end: usize },
    Discrete(Vec<usize>),
}
```

Custom `PartialEq` compares semantically — `Range { start: 2, end: 5 }` equals `Discrete([2, 3, 4])`. Iteration, containment, and overlap work correctly across representations.

**Impact**: Memory-efficient contiguous spans for the common case. Correct mixed-representation comparisons.

## Improvement 7: Borrowed Rule References in Candidates (Performance)

### Python Implementation

Python objects are always heap-allocated and passed by reference — no cloning overhead.

### Our Rust Implementation

```rust
pub(crate) struct Candidate<'a> {
    pub(super) rid: usize,
    pub(super) rule: &'a Rule,
    ...
}
```

Candidates borrow from `LicenseIndex` rather than cloning the entire `Rule` (which contains 30+ fields including `text: String` and `tokens: Vec<u16>` that can be several KB each).

**Impact**: Eliminates per-candidate allocation overhead that a naive `rule: Rule` (owned) field would introduce, matching Python's reference semantics.

## What Users Should Expect

- **Default behavior**: Results are designed to closely match Python ScanCode for common license patterns.
- **Intentional differences**: SPDX match counts are correct (one per identifier, not one per rule). Positional overlap semantics are stricter and more correct than Python's bounds-only approach. Surround merges require actual positional overlap, not just enclosing bounds.
- **Determinism guarantee**: The whole-query snapshot is taken intentionally before SPDX subtraction, not as an accidental stale cache.
