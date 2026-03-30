# PositionSpan Unification Plan

## Background

The codebase has multiple span-like types with overlapping purposes:

| Type                  | Location           | Representation       | End Convention           | Usage                        |
| --------------------- | ------------------ | -------------------- | ------------------------ | ---------------------------- |
| `query::PositionSpan` | `query/mod.rs`     | `{start, end}`       | Inclusive `[start, end]` | Query subtraction            |
| `SpanIter<'a>`        | `license_match.rs` | Enum: Slice or Range | Exclusive `[start, end)` | Zero-copy iteration          |
| `spans::Span`         | `spans.rs`         | `Vec<Range<usize>>`  | Exclusive `[start, end)` | Multi-range union/intersects |
| `PositionSet`         | `position_set.rs`  | `BitSet` + bounds    | N/A                      | O(1) membership, set ops     |

Additionally, `LicenseMatch` has four `Option<Vec<usize>>` fields that follow the pattern:

- `None` means contiguous range `[start, end)`
- `Some(positions)` means discrete non-contiguous positions

This pattern is duplicated across `qspan_positions`, `ispan_positions`, `hispan_positions`, and `matched_token_positions`.

## Goal

Unify into a single `PositionSpan` type that:

1. Encapsulates "range or discrete positions" semantics
2. Uses exclusive end convention (Rust-idiomatic)
3. Provides zero-copy iteration via `SpanIter`
4. Can be owned by `LicenseMatch` instead of `Option<Vec<usize>>`

## Migration Steps

### Step 1: Remove `matched_token_positions`

**Location**: `src/license_detection/models/license_match.rs` and test files

**Rationale**: This field is never populated in production code (always `None`). It only appears in tests and serves as a redundant fallback in `len()` and `qregion_len()`. Removing it simplifies the data model before we introduce `PositionSpan`.

**Changes**:

1. Remove the field from `LicenseMatch` struct
2. Remove fallback logic in `len()` and `qregion_len()` that checks `matched_token_positions`
3. Remove `is_continuous()` check for `matched_token_positions.is_some()`
4. Update tests to use `qspan_positions` instead

**Files to update**:

- `src/license_detection/models/license_match.rs` — remove field, update methods
- `src/license_detection/models/mod_tests.rs` — update tests (lines 699, 738, 788)
- `src/license_detection/match_refine/filter_low_quality.rs` — update tests (lines 725, 741, 757, 773)
- `src/license_detection/tests.rs` — update `make_test_match()` helper (line 25)

**Migration checklist for methods referencing `matched_token_positions`**:

| Method                | Line | Change                       |
| --------------------- | ---- | ---------------------------- |
| `Default::default()`  | 389  | Remove field initialization  |
| `LicenseMatch::new()` | 424  | Remove field initialization  |
| `len()`               | 495  | Remove fallback branch       |
| `qregion_len()`       | 510  | Remove fallback branch       |
| `is_continuous()`     | 668  | Remove the `is_some()` check |

**Before/After in `len()`**:

```rust
// Before
pub(crate) fn len(&self) -> usize {
    if let Some(positions) = &self.qspan_positions {
        positions.len()
    } else if let Some(positions) = &self.matched_token_positions {
        positions.len()
    } else {
        self.end_token.saturating_sub(self.start_token)
    }
}

// After
pub(crate) fn len(&self) -> usize {
    if let Some(positions) = &self.qspan_positions {
        positions.len()
    } else {
        self.end_token.saturating_sub(self.start_token)
    }
}
```

### Step 2: Make `query::PositionSpan` End-Exclusive

**Location**: `src/license_detection/query/mod.rs`

**Changes**:

1. Update `contains()` from `pos <= self.end` to `pos < self.end`
2. Update `iter()` from `start..=end` to `start..end`
3. Update all call sites to remove `end - 1` conversions

**Call sites to update** (13 total):

**Production code (4):**

```
src/license_detection/mod.rs:121        - query::PositionSpan::new(m.start_token, m.end_token - 1)
src/license_detection/mod.rs:407        - query::PositionSpan::new(m.start_token, m.end_token - 1)
src/license_detection/query/mod.rs:1059 - PositionSpan::new(start, end.unwrap_or(usize::MAX))
src/license_detection/query/mod.rs:1074 - PositionSpan::new(start, end.unwrap_or(usize::MAX))
```

**Test code (9):**

```
src/license_detection/query/test.rs:370, 392, 477, 516, 850, 866, 869, 881, 902
```

**Note on `usize::MAX` edge case**: The `end.unwrap_or(usize::MAX)` pattern is a hack for empty ranges. Keeping it as-is with exclusive end is acceptable — being off-by-one on a hack doesn't make it worse.

### Step 3: Extend PositionSpan to Support Range + Discrete

**Location**: Move from `query/mod.rs` to `models/position_span.rs`

**New type definition**:

```rust
/// A span of positions - either contiguous or discrete.
///
/// Uses exclusive end convention: Range { start, end } represents [start, end).
#[derive(Debug, Clone, PartialEq)]
pub enum PositionSpan {
    /// Contiguous range [start, end) - zero allocation.
    Range { start: usize, end: usize },
    /// Non-contiguous discrete positions.
    Discrete(Vec<usize>),
}

impl PositionSpan {
    /// Create a contiguous range [start, end).
    pub fn range(start: usize, end: usize) -> Self {
        Self::Range { start, end }
    }

    /// Create from discrete positions (may convert to Range if contiguous).
    pub fn from_positions(positions: Vec<usize>) -> Self;

    /// Create an empty span.
    pub fn empty() -> Self;

    /// Iterator over all positions (zero-copy for Range).
    pub fn iter(&self) -> SpanIter<'_>;

    /// Number of positions.
    pub fn len(&self) -> usize;

    /// Check if empty.
    pub fn is_empty(&self) -> bool;

    /// Bounds as (min, max+1). Returns (0, 0) for empty.
    pub fn bounds(&self) -> (usize, usize);

    /// Check if position is contained.
    pub fn contains(&self, pos: usize) -> bool;

    /// Convert to PositionSet for O(1) membership testing.
    pub fn to_position_set(&self) -> PositionSet;

    /// Convert to Vec (allocates, use sparingly).
    pub fn to_vec(&self) -> Vec<usize>;

    /// Check if this span is contiguous (no gaps).
    /// Range is always contiguous; Discrete is contiguous only if positions form a single range.
    pub fn is_contiguous(&self) -> bool;
}
```

**SpanIter integration**:

Move `SpanIter` enum from `license_match.rs` into `position_span.rs`:

```rust
pub enum SpanIter<'a> {
    Range(std::ops::Range<usize>),
    Slice(std::iter::Copied<std::slice::Iter<'a, usize>>),
}
```

**Migration**:

1. Create `models/position_span.rs` with the new type
2. Add `mod position_span` to `models/mod.rs`
3. Update all `PositionSpan::new(a, b)` calls to `PositionSpan::range(a, b)`
4. Keep backward-compatible `new()` constructor that delegates to `range()`
5. Update imports across the codebase

### Step 4: Apply to LicenseMatch Fields

**Location**: `src/license_detection/models/license_match.rs`

**Changes**:

Replace the `Option<Vec<usize>>` fields:

```rust
// Before
pub qspan_positions: Option<Vec<usize>>,
pub ispan_positions: Option<Vec<usize>>,
pub hispan_positions: Option<Vec<usize>>,

// After
pub qspan: PositionSpan,
pub ispan: PositionSpan,
pub hispan: PositionSpan,
```

**Update call sites** (approximately 200+):

Pattern matching on `Option<Vec<usize>>`:

```rust
// Before
if let Some(positions) = &self.qspan_positions {
    positions.len()
} else {
    self.end_token.saturating_sub(self.start_token)
}

// After
self.qspan.len()
```

Bounds computation:

```rust
// Before
if let Some(positions) = &self.qspan_positions {
    (*positions.iter().min().unwrap(), *positions.iter().max().unwrap() + 1)
} else {
    (self.start_token, self.end_token)
}

// After
self.qspan.bounds()
```

Set operations:

```rust
// Before
let set: HashSet<usize> = m.qspan().into_iter().collect();

// After
let set = m.qspan.to_position_set();
```

**Methods to remove**:

- `qspan()` → use `qspan.iter()` or `qspan.to_vec()`
- `ispan()` → use `ispan.iter()` or `ispan.to_vec()`
- `hispan()` → use `hispan.iter()` or `hispan.to_vec()`
- `qspan_iter()` → use `qspan.iter()` directly
- `ispan_iter()` → use `ispan.iter()` directly

**Methods to keep/update**:

- `qspan_bounds()` → `qspan.bounds()`
- `ispan_bounds()` → `ispan.bounds()`
- `is_continuous()` → replace `matched_token_positions.is_some()` check with `!self.qspan.is_contiguous()`

### Step 5: Clean Up (Optional)

After migration, consider:

1. **Deprecate `spans::Span`**: Check if it can be replaced by `PositionSet` or kept for multi-range union operations
2. **Test helper updates**: Update `make_test_match()` in `tests.rs`
3. **Constructor simplification**: Remove `start_token`/`end_token` fields if fully derivable from `qspan`

## Open Questions

1. **Empty span representation**: Should empty be `Range { start: 0, end: 0 }` or a dedicated `Empty` variant? `Range { start: 0, end: 0 }` is simpler.

2. **Serialization**: `PositionSpan` fields don't appear in JSON output, so no serialization changes needed.

3. **`spans::Span` deprecation**: After migration, evaluate if `spans::Span` can be replaced by `PositionSet`. It's used for multi-range union operations in `handle_overlaps.rs` and `hash_match.rs`.

## Estimated Scope

| Step   | Files Touched | Call Sites |
| ------ | ------------- | ---------- |
| Step 1 | ~4            | ~45        |
| Step 2 | ~3            | 13         |
| Step 3 | ~10           | ~15        |
| Step 4 | ~20           | ~200       |
| Step 5 | ~5            | ~10        |

## Success Criteria

1. All tests pass
2. No `clippy` warnings
3. No performance regression (benchmark with `./scripts/benchmark.sh`)
4. Cleaner API: callers use `m.qspan.iter()` instead of `m.qspan().into_iter().collect()`
