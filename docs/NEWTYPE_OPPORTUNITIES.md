# Newtype Opportunities for Provenant

This document catalogs raw types across the codebase that represent domain concepts and would benefit from newtype wrappers for improved type safety and semantics. Each opportunity includes the current state, the proposed change, and the benefits.

---

## Tier 1: Highest Impact — Prevents Real Bugs

### 1. Cryptographic Digests — **DONE** (PR #590)

**Implemented in `src/models/digest.rs`.** Fixed-size `[u8; N]` byte arrays with serde hex-string serialization. ~15% scan performance improvement from eliminating heap allocations. `EMPTY_SHA1_DIGEST` const replaces string literal.

**Current State:** Hash strings are `Option<String>` across ~20 fields in 5 structs (`FileInfo`, `PackageData`, `ResolvedPackage`, `Package`, `FileReference`) plus `cache/incremental.rs`. A SHA-1 value silently accepted where SHA-256 is expected is an undetectable data integrity bug.

**Fields Affected:**

- `FileInfo`: `sha1`, `md5`, `sha256`, `sha1_git`
- `PackageData`: `sha1`, `md5`, `sha256`, `sha512`
- `ResolvedPackage`: `sha1`, `md5`, `sha256`, `sha512`
- `Package`: `sha1`, `md5`, `sha256`, `sha512`
- `FileReference`: `sha1`, `md5`, `sha256`, `sha512`
- `cache::FileStateFingerprint`: `content_sha256`

**Proposed Newtypes:**

```rust
/// SHA-1 digest as raw 20 bytes (40 hex chars when displayed).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Sha1Digest([u8; 20]);

/// MD5 digest as raw 16 bytes (32 hex chars when displayed).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Md5Digest([u8; 16]);

/// SHA-256 digest as raw 32 bytes (64 hex chars when displayed).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Sha256Digest([u8; 32]);

/// SHA-512 digest as raw 64 bytes (128 hex chars when displayed).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Sha512Digest([u8; 64]);

/// Git object SHA-1 (blob format, 20 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GitSha1([u8; 20]);
```

**Benefits:**

- Compile-time guarantee: `Sha1Digest` cannot be assigned to `Sha256Digest` field
- Fixed-size: `[u8; N]` is stack-allocated, no heap allocation
- Validation at construction: malformed hex strings rejected early
- Serde serialization to lowercase hex string (JSON-compatible)
- Enables `Sha1Digest::EMPTY` constant to replace `EMPTY_SHA1` string literal
- Enables `HashAlgorithm` enum for CycloneDX output (`"SHA-1"`, `"SHA-256"`, etc.)

**Implementation Notes:**

- Store as raw bytes (`[u8; N]`), serialize/deserialize as hex string
- `Display` and `Debug` impls show hex representation
- Constructor from hex string: `Sha256Digest::from_hex("abc123...")`
- Constructor from bytes: `Sha256Digest::from_bytes([u8; 32])`
- Serde with `#[serde(try_from = "String", into = "String")]` for JSON compatibility

---

### 2. Line Numbers and Spans — **PARTIALLY DONE** (PR #596)

**Implemented in `src/models/line_number.rs`.** `LineNumber(NonZeroUsize)` enforces 1-based invariant at the type level. All `start_line`/`end_line` fields across output, detection, and internal types migrated from `usize` to `LineNumber`. Eliminated runtime `start_line > 0` guards. `serde(transparent)` preserves JSON backward compatibility.

**Deferred: `LineSpan`** — Not implemented because it is not a simple newtype migration. All current structs use separate `start_line`/`end_line` fields rather than a combined span, so adopting `LineSpan` would require restructuring output structs (replacing two fields with one), changing JSON serialization shape, and updating all construction/deconstruction sites. This is a deeper refactoring than a newtype wrapper and should be evaluated as a separate effort when the output schema is open to structural changes.

**Current State:** `start_line`/`end_line` are raw `usize` across 12+ fields in 6+ structs. Nothing prevents 0-based indexing bugs or swapping start/end.

**Fields Affected:**

- `Match`: `start_line`, `end_line`
- `Copyright`: `start_line`, `end_line`
- `Holder`: `start_line`, `end_line`
- `Author`: `start_line`, `end_line`
- `OutputEmail`: `start_line`, `end_line`
- `OutputURL`: `start_line`, `end_line`
- `copyright::CopyrightDetection`: `start_line`, `end_line`
- `copyright::HolderDetection`: `start_line`, `end_line`
- `copyright::AuthorDetection`: `start_line`, `end_line`
- `finder::UrlDetection`: `start_line`, `end_line`
- `finder::EmailDetection`: `start_line`, `end_line`

**Proposed Newtypes:**

```rust
/// 1-based line number in a source file. Invariant: value >= 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LineNumber(usize);

/// Contiguous span of 1-based source lines (inclusive on both ends).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineSpan {
    pub start: LineNumber,
    pub end: LineNumber,
}
```

**Benefits:**

- Enforces 1-based invariant at construction
- Prevents swapping start/end (struct with named fields)
- Self-documenting: always clear it's a line number, not a count or byte offset
- Can add `len()` and `is_empty()` methods to `LineSpan`

---

### 3. Package and Dependency Identifiers — **PARTIALLY DONE**

**Implemented in `src/models/package_uid.rs` and `src/models/dependency_uid.rs`.** `PackageUid(String)` and `DependencyUid(String)` newtypes with `#[serde(transparent)]`, `Display`, `Deref<Target=str>`, `AsRef<str>`, `Borrow<str>`, `Clone`, `PartialEq`, `Eq`, `Hash`. Key methods: `new(purl)` appends UUID, `from_raw(s)` wraps without validation, `empty()` returns empty sentinel, `replace_base(new_purl)` preserves UUID suffix. `PackageUid` also has `stable_key()` for deterministic sorting. Deduplicated `build_package_uid` (was in both `file_info.rs` and `swift_merge.rs`) into `PackageUid::new()`. Deduplicated `replace_uid_base` into `.replace_base()` methods. Deduplicated `stable_uid_key` into `.stable_key()`.

**Migrated fields:**

- `Package::package_uid: String` → `PackageUid`
- `TopLevelDependency::dependency_uid: String` → `DependencyUid`
- `TopLevelDependency::for_package_uid: Option<String>` → `Option<PackageUid>`
- `FileInfo::for_packages: Vec<String>` → `Vec<PackageUid>`
- Assembly HashMap keys: `HashMap<String, ...>` → `HashMap<PackageUid, ...>` in `package_file_index.rs`, `reference_following.rs`, `conda_rootfs_merge.rs`, `npm_resource_assign.rs`
- `HashSet<String>` → `HashSet<PackageUid>` in `bazel_prune.rs`
- `Vec<String>` → `Vec<PackageUid>` in `npm_workspace_merge.rs`, `cargo_workspace_merge.rs`, `swift_merge.rs`
- Output schema conversions use `.to_string()` for `From` and `::from_raw()` for `TryFrom`

**Deferred: `Purl` newtype** — Not yet implemented. The `purl` field is `Option<String>` on `PackageData`, `Package`, `Dependency`, and `TopLevelDependency`. There are many construction sites across all parsers that set `purl` directly from parsed strings. Migrating to a `Purl` newtype would require updating every parser's purl construction, which is high surface area with moderate benefit (purl strings come from trusted parser output, not arbitrary user input). Should be evaluated as a separate effort.

**Current State (for Purl):** `purl` is `Option<String>` across 4 structs. Nothing validates the `pkg:` prefix or purl-spec format at construction.

**Fields Affected (for Purl):**

- `PackageData::purl`, `Package::purl`, `Dependency::purl`, `TopLevelDependency::purl`

**Proposed Newtype (for Purl):**

```rust
/// Package URL (purl) per the purl-spec. Format: pkg:type/namespace/name@version?qualifiers#subpath
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Purl(String);
```

**Benefits (for Purl):**

- Validation at construction: `Purl::new("pkg:npm/lodash@4.17.0")` validates format
- Type system prevents confusing PURL with URL or arbitrary string
- Can add accessors for type, namespace, name, version, qualifiers

---

### 4. License Expression Strings — **DEFERRED**

**Decision:** Not worth implementing. The existing `_spdx` suffix naming convention already distinguishes ScanCode and SPDX expressions clearly. Expressions are constructed from trusted sources (detection, normalization) that already produce correct output, so validation would add complexity for no real payoff. Only ~2-3 call sites use sentinel string comparisons (not enough to justify helper methods). The cost is high (~50+ field instances, 15+ structs, 20+ test files) for marginal benefit. The existing `LicenseExpression` AST enum in `src/license_detection/expression/mod.rs` also makes naming awkward — a string newtype would need a distinct name like `ScancodeExpression`, adding further indirection. `extracted_license_statement` is pure passthrough (never used in logic) and does not need a newtype either.

---

## Tier 2: High Impact — Prevents Category Errors

### 5. Match Scores and Percentages — **PARTIALLY DONE**

**Implemented in `src/models/match_score.rs`.** `MatchScore(f64)` with sealed API: no `From<f64>`/`Into<f64>`, only semantic constructors (`from_percentage()`, `MAX` constant, `GOOD_THRESHOLD`). Widened `LicenseMatch::score` from `f32` to `MatchScore` so the same type spans the entire detection→model→output pipeline, eliminating f32/f64 boundary conversions. Also fixed `Match::rule_relevance` from `Option<usize>` to `Option<u8>`.

**Bug fixes included:** Normalized `LicenseMatch::score` range inconsistency where aho_match and unknown_match produced 0.0–1.0 scores while hash/spdx/seq matchers produced 0.0–100.0. Fixed doc comment from "0.0-1.0" to "0.0-100.0".

**Deferred newtypes:**

- `MatchCoverage(f64)` — same 0.0–100.0 domain as MatchScore; would require same full migration across detection+model+output layers. Lower priority since coverage is less semantically overloaded than score.
- `Percentage(f64)` — for `FileInfo::percentage_of_license_text`; single-field newtype with low reuse.
- `Score100(u8)` / `RuleRelevance(u8)` / `MinCoverage(u8)` — integer 0–100 domain; `Rule::relevance` is already `u8`, `Rule::minimum_coverage` is `Option<u8>`. The `usize`→`u8` fix for `Match::rule_relevance` already addressed the main type confusion here.
- `LicenseClarityScore::score` uses `usize` but is naturally bounded 0–100 by construction; low risk of misuse.

**Fields still using raw types:**

- `Match::match_coverage: Option<f64>` (0.0–100.0)
- `FileInfo::percentage_of_license_text: Option<f64>` (0.0–100.0)
- `LicenseClarityScore::score: usize` (0–100)
- `LicenseRuleReference::relevance: Option<u8>` (0–100)
- `LicenseReference::minimum_coverage: Option<u8>` (0–100)
- `LicenseRuleReference::minimum_coverage: Option<u8>` (0–100)
- `LicenseScanOptions::min_score: u8` (0–100)

---

### 6. URLs — **DEFERRED**

**Decision:** Not worth implementing as a newtype at this time. The ~40 URL fields across model + output_schema layers are overwhelmingly passthrough (constructed once, cloned between structs, serialized). Only ~6 sites consume URLs in logic. The category-error risk is concentrated in 2-3 parsers (Python `apply_project_url_mappings` label-based routing, `download_url` fallback from VCS URL) and is low-frequency. The `TryFrom` cascade from making URL fields fallible (affecting `--from-json` workflows) is high cost for moderate benefit. License index URLs are all from trusted constants; `ignorable_urls` fields are pattern-matching strings (not navigable URLs) and must stay `Vec<String>`. A `url::Url` wrapper would normalize URLs and break ScanCode output parity.

**Bug fix included:** Fixed `src/parsers/opam.rs:191` where `repository_homepage_url` produced the literal string `"{https://opam.ocaml.org/packages}/{name}"` instead of interpolating the package name via `format!()`. Updated golden expected files for sample2 and sample5.

**Current State:** 18+ URL fields are `Option<String>`. No validation, easily confused with paths or PURLs.

**Fields Affected:**

- `PackageData`: `homepage_url`, `download_url`, `bug_tracking_url`, `code_view_url`, `vcs_url`, `repository_homepage_url`, `repository_download_url`, `api_data_url`
- `Match::rule_url`
- `Party::url`, `organization_url`
- `LicenseReference`: `homepage_url`, `text_urls`, `osi_url`, `faq_url`, `other_urls`, `scancode_url`, `licensedb_url`, `spdx_url`
- `LicenseRuleReference::rule_url`

**Proposed Newtype:**

```rust
/// A validated URL string.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Url(String);
```

**Benefits:**

- Validates scheme (http/https/git+https/etc.) at construction
- Prevents storing a file path or PURL in a URL field
- Display/Debug impls always show the URL scheme

---

### 7. File Paths

**Current State:** The most ubiquitous string in the codebase. No distinction between scan-root-relative paths, file names, and extensions.

**Fields Affected:**

- `FileInfo::path`, `FileInfo::name`, `FileInfo::base_name`, `FileInfo::extension`
- `FileReference::path`
- `Package::datafile_paths`
- `Match::from_file`
- `TopLevelDependency::datafile_path`
- `PackageData::subpath`

**Proposed Newtypes:**

```rust
/// File path relative to the scan root (forward-slash separated).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ScanPath(String);

/// File name component (last segment of a path).
pub struct FileName(String);

/// Base name without extension.
pub struct BaseName(String);

/// File extension (with or without leading dot).
pub struct FileExtension(String);
```

**Benefits:**

- Self-documenting: type tells you it's scan-root-relative
- Can add `is_absolute()`, `parent()`, `join()` methods
- Prevents confusion with URLs, PURLs, license expressions

---

### 8. Rule and Detection Identifiers

**Current State:** `RuleId` is `usize` used as HashMap key throughout license detection. `RuleIdentifier` and `DetectionId` are both `String` but represent different concepts.

**Fields Affected:**

- `license_detection::LicenseMatch::rid` — internal rule index
- `license_detection::index` — 10+ HashMaps keyed by `usize` rule ID
- `Match::rule_identifier` — string ID like "mit.LICENSE"
- `LicenseRuleReference::identifier`
- `LicenseDetection::identifier` — format: `<expr>-<uuid>`
- `TopLevelLicenseDetection::identifier`

**Proposed Newtypes:**

```rust
/// Internal rule index ID (used as HashMap/HashSet key in license index).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RuleId(usize);

/// Unique rule identifier string (e.g., "mit.LICENSE", "gpl-2.0-plus_4.RULE").
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RuleIdentifier(String);

/// Unique detection instance identifier (format: "<expression>-<uuid>").
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DetectionId(String);
```

**Benefits:**

- `RuleId` cannot be confused with `PatternId` (Aho-Corasich) or `LineNumber`
- `RuleIdentifier` vs `DetectionId` distinction enforced by type system
- Better `Debug`/`Display` output for logs

---

### 9. Package Domain Identifiers

**Current State:** Package names, versions, namespaces, and version requirements are all `Option<String>` or `String`. `extracted_requirement` (constraint) and `version` (resolved) are both `Option<String>`.

**Fields Affected:**

- `PackageData::name`, `PackageData::version`, `PackageData::namespace`
- `ResolvedPackage::name`, `ResolvedPackage::version`, `ResolvedPackage::namespace`
- `Package::name`, `Package::version`, `Package::namespace`
- `Dependency::extracted_requirement`, `Dependency::scope`

**Proposed Newtypes:**

```rust
pub struct PackageName(String);
pub struct PackageVersion(String);
pub struct PackageNamespace(String);
pub struct VersionRequirement(String); // Constraint, not resolved version
pub struct DependencyScope(String);    // Ecosystem-specific, not a closed enum
```

**Benefits:**

- `PackageName` cannot be passed where `PackageVersion` expected
- `VersionRequirement` distinct from `PackageVersion` (constraint vs resolved)
- `DependencyScope` signals "not an arbitrary string" even though it's open-ended

---

### 10. Tristate for Optional Booleans

**Current State:** `Dependency::is_runtime`, `is_optional`, `is_pinned`, `is_direct` are `Option<bool>`. The three states (unknown/true/false) are implicit.

**Fields Affected:**

- `Dependency::is_runtime`, `is_optional`, `is_pinned`, `is_direct`

**Proposed Newtype:**

```rust
/// Three-valued logic for optional boolean fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tristate {
    Unknown,
    True,
    False,
}
```

**Benefits:**

- Explicit `Unknown` variant instead of `None` semantics
- Prevents boolean logic errors (`unwrap_or(false)` vs `== Some(true)`)
- Self-documenting in struct definitions

---

## Tier 3: Medium Impact — Improves Semantics

### 11. Stringly-Typed Enums

| Field                            | Current Type     | Proposed Type                         |
| -------------------------------- | ---------------- | ------------------------------------- |
| `Match::matcher`                 | `Option<String>` | `Option<MatcherKind>` enum            |
| `Party::r#type`                  | `Option<String>` | `Option<PartyType>` enum              |
| `Party::role`                    | `Option<String>` | `Option<PartyRole>` enum              |
| `LicenseReference::category`     | `Option<String>` | `Option<LicenseCategory>` enum        |
| `FileInfo::mime_type`            | `Option<String>` | `Option<MimeType>` newtype            |
| `FileInfo::programming_language` | `Option<String>` | `Option<ProgrammingLanguage>` newtype |
| `PackageData::primary_language`  | `Option<String>` | `Option<ProgrammingLanguage>`         |

**Detection Log Constants** (in `license_detection/detection/mod.rs`):

```rust
// Current: 9 string constants
const DETECTION_LOG_LICENSE_CLUES: &str = "license-clues";
const DETECTION_LOG_FALSE_POSITIVE: &str = "false-positive";
// ... etc

// Proposed: single enum
pub enum DetectionLogCategory {
    LicenseClues,
    FalsePositive,
    LowQualityMatchFragments,
    NotLicenseCluesAsMoreDetectionsPresent,
    ImperfectCoverage,
    UnknownMatch,
    ExtraWords,
    UndetectedLicense,
    UnknownIntroFollowedByMatch,
}
```

---

### 12. File Size and Count Types

**Fields Affected:**

- `FileInfo::size`, `FileReference::size`, `PackageData::size`
- `FileInfo::files_count`, `ExtraData::files_count`
- `FileInfo::dirs_count`, `ExtraData::directories_count`
- `FileInfo::size_count`
- `FileInfo::source_count`
- `ExtraData::excluded_count`
- `TopLevelLicenseDetection::detection_count`
- `TallyEntry::count`

**Proposed Newtypes:**

```rust
pub struct FileSize(u64);       // Bytes
pub struct FileCount(usize);
pub struct DirCount(usize);
pub struct SourceCount(usize);
pub struct ExcludedCount(usize);
pub struct DetectionCount(usize);
```

---

### 13. Email Addresses

**Fields Affected:**

- `Party::email`
- `OutputEmail::email`
- `finder::EmailDetection::email`

**Proposed Newtype:**

```rust
pub struct EmailAddress(String);  // Validates @ presence
```

---

### 14. Timestamps

**Fields Affected:**

- `FileInfo::date`
- `PackageData::release_date`, `ResolvedPackage::release_date`
- `Header::start_timestamp`, `Header::end_timestamp`

**Proposed Newtype:**

```rust
pub struct Timestamp(String);  // ISO 8601 format, could validate
```

---

### 15. Token Positions (0-indexed)

**Fields Affected** (in `license_detection/models/license_match.rs`):

- `start_token`, `end_token`, `rule_start_token`
- Aho-Corasich `Match::start`, `end` (byte positions)

**Proposed Newtypes:**

```rust
pub struct TokenPosition(usize);  // 0-indexed, distinct from LineNumber
pub struct PatternId(usize);      // Aho-Corasick pattern ID
```

---

## Tier 4: Lower Priority

### 16. Type Alias Replacements

| Current Alias                                                                                        | Proposed Struct                                                           |
| ---------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| `NumberedLine = (usize, String)` in `copyright/candidates.rs`                                        | `struct NumberedLine { line: usize, text: String }`                       |
| `DirectoryMergeOutput = (Option<Package>, Vec<TopLevelDependency>, Vec<usize>)` in `assembly/mod.rs` | `struct DirectoryMergeOutput { package, dependencies, affected_indices }` |

---

### 17. Other String Newtypes

- `LicenseKey(String)` — ScanCode license key, used as HashMap key
- `SpdxLicenseKey(String)` — SPDX identifier
- `HashAlgorithm` enum — replaces `"SHA-1"`, `"SHA-256"`, etc. in CycloneDX output
- `NpmLockfileVersion` enum — replaces raw `i64` (values 1, 2, 3)

---

## Summary Statistics

| Metric                                 | Count |
| -------------------------------------- | ----- |
| Distinct newtype definitions suggested | ~30   |
| Raw `Option<String>` fields covered    | ~70   |
| Raw `usize`/`u64` fields covered       | ~35   |
| Raw `f64`/`f32` fields covered         | ~8    |
| Stringly-typed enums suggested         | ~6    |
| Type aliases to replace                | 2     |
| Detection log constants → enum         | 9     |

---

## Implementation Priority

1. **Digests** — DONE (PR #590)
2. **Line numbers** — PARTIALLY DONE (PR #596, `LineSpan` deferred)
3. **Package identifiers** — PARTIALLY DONE (`PackageUid`/`DependencyUid` implemented; `Purl` deferred)
4. **Match scores** — PARTIALLY DONE (`MatchScore` implemented on `LicenseMatch::score` and `Match::score`; `MatchCoverage`, `Percentage`, `Score100`, `RuleRelevance`, `MinCoverage` deferred)
5. **Scores and percentages** — Remaining: `MatchCoverage`, `Percentage`, `Score100`, `RuleRelevance`, `MinCoverage`
6. **URLs** — DEFERRED (bug fix only: opam `repository_homepage_url`)
7. Everything else in order of diminishing returns

---

## Deferred and Rejected

| Item                                                               | Decision | Rationale                                                                                                                                                                                                                                                                                                    |
| ------------------------------------------------------------------ | -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `LineSpan` (from #2)                                               | Deferred | Not a simple newtype migration — requires replacing two separate `start_line`/`end_line` fields with one combined struct, changing JSON serialization shape. Evaluate when output schema is open to structural changes.                                                                                      |
| `Purl` (from #3)                                                   | Deferred | High surface area across all parsers with moderate benefit. Purl strings come from trusted parser output. Evaluate as separate effort.                                                                                                                                                                       |
| License expression strings (#4)                                    | Rejected | `_spdx` suffix naming convention sufficient. Expressions from trusted sources. Only 2-3 sentinel comparison sites. High cost (~50+ fields, 15+ structs, 20+ test files) for marginal benefit. Naming collision with existing `LicenseExpression` AST enum.                                                   |
| `MatchCoverage(f64)` (from #5)                                     | Deferred | Same 0.0–100.0 domain as MatchScore but lower semantic urgency. Coverage is less overloaded than score. Full migration across detection+model+output layers required.                                                                                                                                        |
| `Percentage(f64)` (from #5)                                        | Deferred | Single-field newtype (`FileInfo::percentage_of_license_text`) with low reuse.                                                                                                                                                                                                                                |
| `Score100(u8)` / `RuleRelevance(u8)` / `MinCoverage(u8)` (from #5) | Deferred | Integer 0–100 fields already use `u8` internally; `Match::rule_relevance` fixed from `usize` to `u8`. Low risk of remaining type confusion.                                                                                                                                                                  |
| URLs (#6)                                                          | Deferred | ~40 fields but overwhelmingly passthrough. Only ~6 logic consumption sites. Category-error risk concentrated in 2-3 parsers, low-frequency. `TryFrom` cascade makes `--from-json` fallible. `ignorable_urls` must stay `Vec<String>`. `url::Url` normalizes and breaks parity. Opam bug fixed independently. |
