# LicenseMatch Lifetime Refactoring Plan

## Goal

Eliminate string copies in license detection by changing `LicenseMatch` to borrow from `Rule` instead of cloning strings.

## Problem Statement

Currently, `LicenseMatch` clones strings from `Rule`:

- `license_expression: String`
- `rule_identifier: String`
- `rule_url: String`
- `license_expression_spdx: Option<String>`
- `referenced_filenames: Option<Vec<String>>`

These clones happen:

1. When creating matches in matching functions
2. When merging matches
3. When filtering/processing matches

Profiling shows string copies account for ~20-40% of allocation overhead in license detection.

## Solution Overview

Change `LicenseMatch` to hold a `&'a Rule` reference instead of owned strings:

```rust
pub struct LicenseMatch<'a> {
    pub rule: &'a Rule,  // Borrow instead of owning strings
    // Computed fields remain owned:
    pub start_token: usize,
    pub end_token: usize,
    pub qspan_positions: Option<Vec<usize>>,
    // ...
}
```

Strings are accessed via methods: `m.license_expression()` returns `&'a str`.

---

## Phase 1: Test Infrastructure Refactoring

**Do this first** to minimize impact of the lifetime changes on test code.

### 1.1 Create Tests Submodule Structure

Location: `src/license_detection/tests/`

```text
src/license_detection/tests/
├── mod.rs           # Re-exports, get_or_create_rule, make_test_match
├── builder.rs       # TestMatchBuilder using derive_builder
└── engine_tests.rs  # Engine integration tests
```

### 1.2 Implement TestMatchBuilder

```rust
// src/license_detection/tests/builder.rs
use derive_builder::Builder;

#[derive(Builder, Clone)]
#[builder(build_fn(skip), setter(into))]
pub struct TestMatchBuilder {
    #[builder(default = "\"MIT\".to_string()")]
    license_expression: String,

    #[builder(default = "\"test-rule\".to_string()")]
    rule_identifier: String,

    #[builder(default)]
    start_line: usize,

    #[builder(default)]
    end_line: usize,

    // ... other fields with defaults
}

impl TestMatchBuilder {
    pub fn build_match(self) -> LicenseMatch<'static> {
        let rule = get_or_create_rule(&self.rule_identifier, &self.license_expression);
        LicenseMatch {
            rule,
            start_line: self.start_line,
            // ...
        }
    }
}
```

### 1.3 Fix Static Rule Cache

**Critical bug to avoid:** Do NOT return a pointer to a HashMap entry. The HashMap can relocate entries when growing, invalidating the pointer.

**Wrong (undefined behavior):**

```rust
let entry = cache.entry(key).or_insert(rule);
let ptr = entry as *const Rule;
unsafe { &*ptr }  // UB when HashMap grows!
```

**Correct (Box::leak):**

```rust
static CACHE: Lazy<RwLock<HashMap<String, &'static Rule>>> = ...;

let boxed = Box::new(rule);
let leaked: &'static Rule = Box::leak(boxed);
cache.insert(key, leaked);
leaked
```

### 1.4 Update Test Files to Use Builder

Files with `LicenseMatch` construction in tests:

| File                                 | Estimated Changes |
| ------------------------------------ | ----------------- |
| `models/mod_tests.rs`                | ~30               |
| `tests.rs` (make_test_match helper)  | ~20               |
| `match_refine/mod.rs`                | ~15               |
| `match_refine/merge.rs`              | ~10               |
| `match_refine/handle_overlaps.rs`    | ~8                |
| `match_refine/filter_low_quality.rs` | ~10               |
| `match_refine/false_positive.rs`     | ~10               |
| `detection/analysis.rs`              | ~8                |
| `detection/identifier.rs`            | ~5                |
| `detection/types.rs`                 | ~5                |
| `detection/mod.rs`                   | ~5                |
| `unknown_match.rs`                   | ~8                |
| `detection/grouping.rs`              | ~5                |
| `scanner/process.rs`                 | ~5                |
| `spdx_lid/mod.rs`                    | ~5                |

**Before:**

```rust
let m = LicenseMatch {
    license_expression: "MIT".to_string(),
    rule_identifier: "mit".to_string(),
    start_line: 1,
    end_line: 10,
    // ... 15 more fields
};
```

**After:**

```rust
use crate::license_detection::tests::TestMatchBuilder;

let m = TestMatchBuilder::default()
    .license_expression("MIT")
    .rule_identifier("mit")
    .start_line(1)
    .end_line(10)
    .build_match();
```

### 1.5 Phase 1 Deliverables

- [ ] Create `tests/` submodule structure
- [ ] Implement `TestMatchBuilder` with `derive_builder`
- [ ] Fix all `get_or_create_test_rule` functions to use `Box::leak`
- [ ] Update all test files to use `TestMatchBuilder`
- [ ] All tests pass

---

## Phase 2: LicenseMatch Lifetime Refactoring

**Prerequisite:** Phase 1 complete (tests use builder, not direct construction)

### 2.1 Update LicenseMatch Struct

**Remove fields (now borrowed from Rule):**

- `rid: usize` — Use `rule.is_false_positive` instead of `index.false_positive_rids.contains(&m.rid)`
- `license_expression: String`
- `license_expression_spdx: Option<String>`
- `rule_identifier: String`
- `rule_url: String`
- `rule_relevance: u8`
- `rule_length: usize`
- `rule_kind: RuleKind`
- `referenced_filenames: Option<Vec<String>>`

**Add field:**

- `rule: &'a Rule`

**Keep fields (computed during matching):**

- `start_token`, `end_token`
- `start_line`, `end_line`
- `score`, `match_coverage`, `matched_length`
- `matcher`
- `from_file: Option<String>` — **Stays owned**, computed during matching (not from Rule)
- `matched_text`
- `qspan_positions`, `ispan_positions`, `hispan_positions`
- `hilen`, `rule_start_token`
- `candidate_resemblance`, `candidate_containment`
- `is_from_license`, `matched_token_positions`

### 2.2 Add Accessor Methods

```rust
impl<'a> LicenseMatch<'a> {
    pub fn license_expression(&self) -> &'a str {
        &self.rule.license_expression
    }

    pub fn rule_identifier(&self) -> &'a str {
        &self.rule.identifier
    }

    pub fn rule_length(&self) -> usize {
        self.rule.tokens.len()
    }

    pub fn rule_relevance(&self) -> u8 {
        self.rule.relevance
    }

    pub fn rule_kind(&self) -> RuleKind {
        self.rule.rule_kind
    }

    pub fn is_false_positive(&self) -> bool {
        self.rule.is_false_positive
    }

    pub fn rule_url(&self) -> Option<String> {
        // Computed from rule.is_from_license and identifier
        if self.rule.is_from_license {
            Some(format!("{}/{}", SCANCODE_LICENSE_URL_BASE, self.rule.identifier))
        } else {
            Some(format!("{}/{}", SCANCODE_RULE_URL_BASE, self.rule.identifier))
        }
    }

    pub fn referenced_filenames(&self) -> Option<&'a Vec<String>> {
        self.rule.referenced_filenames.as_ref()
    }

    pub fn license_expression_spdx(&self) -> Option<&'a str> {
        self.rule.license_expression_spdx.as_deref()
    }
}
```

### 2.3 Eliminate `rid` Usage

The `rid` field is no longer needed. Update code that uses it:

| Before                                       | After                      |
| -------------------------------------------- | -------------------------- |
| `index.false_positive_rids.contains(&m.rid)` | `m.rule.is_false_positive` |
| `index.rules_by_rid.get(m.rid)`              | Use `m.rule` directly      |

**Files with `rid` usage:**

- `match_refine/mod.rs:95` — false positive check
- `match_refine/handle_overlaps.rs:102` — false positive check
- `match_refine/filter_low_quality.rs:80,114,161` — false positive check
- `mod.rs:355` — `rules_by_rid.get(m.rid)`

### 2.4 Remove Deserialize from LicenseMatch

`LicenseMatch<'a>` cannot implement `Deserialize` because it needs a `&'a Rule` to borrow from, and deserialized JSON has no Rule to reference.

**Current state:**

- `Deserialize` impl exists (line 353-398 in `license_match.rs`)
- It deserializes into `DeserializableLicenseMatch` then converts to `LicenseMatch`
- The conversion sets `rid: 0`, which is already broken for any `rid`-based lookups

**Solution: Remove `Deserialize` from `LicenseMatch<'a>`**

- `DeserializableLicenseMatch` already exists and handles deserialization
- Tests that deserialize should use `DeserializableLicenseMatch` directly
- No conversion to `LicenseMatch<'a>` is needed for golden tests

**Affected code:**

- `src/license_detection/models/license_match.rs:353-398` — Remove `impl Deserialize`
- Any tests deserializing `LicenseMatch` — Use `DeserializableLicenseMatch` instead

### 2.5 Update Production Code

**Matching functions** (`seq_match/matching.rs`, `aho_match.rs`, `hash_match.rs`, etc.):

```rust
// Before
LicenseMatch {
    license_expression: rule.license_expression.clone(),
    rule_identifier: rule.identifier.clone(),
    // ...
}

// After
LicenseMatch {
    rule,
    start_line,
    end_line,
    // ...
}
```

**Merge functions** (`match_refine/merge.rs`):

```rust
// combine_matches needs special handling
// Both matches must have the same rule (asserted)
fn combine_matches(a: &LicenseMatch<'_>, b: &LicenseMatch<'_>) -> LicenseMatch<'_> {
    assert_eq!(a.rule.identifier, b.rule.identifier);
    LicenseMatch {
        rule: a.rule,  // Same rule as both inputs
        start_line: a.start_line.min(b.start_line),
        // ...
    }
}
```

### 2.6 Update Serialization

`SerializableLicenseMatch<'a>` already borrows strings. Update to borrow from `LicenseMatch<'a>`:

```rust
impl<'a> Serialize for LicenseMatch<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        SerializableLicenseMatch {
            license_expression: self.license_expression(),  // &str
            rule_identifier: self.rule_identifier(),         // &str
            // ...
        }.serialize(serializer)
    }
}
```

### 2.7 Update Output Conversion

In `src/scanner/process.rs`, `convert_match_to_model` creates the output `Match`:

```rust
fn convert_match_to_model(m: LicenseMatch<'_>, ...) -> Match {
    Match {
        license_expression: m.license_expression().to_string(),  // Clone here
        rule_identifier: Some(m.rule_identifier().to_string()),  // Clone here
        // Single clone at output time
    }
}
```

This is the **only place** strings are cloned - once per match, at output time.

### 2.8 Phase 2 Deliverables

- [x] Update `LicenseMatch` struct with lifetime parameter
- [x] Add accessor methods for all borrowed fields
- [x] Eliminate `rid` field and update all `rid` usages
- [x] Remove `Deserialize` impl from `LicenseMatch<'a>`
- [x] Update all matching functions
- [x] Update merge/refine functions
- [x] Update serialization
- [x] Update `convert_match_to_model` for output
- [x] All tests pass
- [x] Profile and measure performance improvement (~12% faster, ~13% less memory)

---

## Measured Impact (Phase 2 Complete)

Benchmarked on opossum-file.rs (78 files):

| Metric                  | Before (main) | After (Phase 2) | Improvement     |
| ----------------------- | ------------- | --------------- | --------------- |
| Scan time               | 20.9s         | 18.4s           | **~12% faster** |
| Peak memory             | 2017 MB       | 1744 MB         | **~13% less**   |
| String copies per match | 5-7           | 2 (at output)   | ~70% reduction  |
| Match struct size       | ~224 bytes    | ~80 bytes       | ~65% reduction  |

---

## Rollback Plan

If issues arise:

1. `LicenseMatch<'a>` can be reverted to owned strings
2. Accessor methods become owned getters instead of borrowed references
3. Tests continue to use `TestMatchBuilder` (unaffected)

---

## References

- Previous optimization: `optimize/candidate-lifetime` - Candidate borrows Rule
- Profiling data: `scripts/profile-setup.sh` + samply MCP
- Related: `docs/improvements/` for other optimization opportunities
