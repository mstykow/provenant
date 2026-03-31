# License Detection Architecture

## Overview

The license detection system is a multi-phase, multi-strategy detection engine that identifies license information in source code and text files. It supports exact matching, fuzzy matching, and unknown license detection through a pipeline of increasingly sophisticated algorithms.

---

## Entry Points

### CLI Flags

| Flag                         | Purpose                                                |
| ---------------------------- | ------------------------------------------------------ |
| `--license-rules-path`       | Override to load custom license/rules from a directory |
| `--license`                  | Enable license scanning                                |
| `--license-text`             | Include matched text in output                         |
| `--license-text-diagnostics` | Highlight unmatched words inside matched text          |
| `--license-diagnostics`      | Include detection post-processing diagnostics          |
| `--license-references`       | Emit top-level license and rule reference blocks       |
| `--license-score`            | Filter returned license detections by minimum score    |
| `--license-url-template`     | Customize top-level `licensedb_url` references         |

**Default behavior**: Uses the built-in embedded license index. No external files required.

> This document describes the current public license-detection surface and its stable architecture.

**Custom rules**: Use `--license-rules-path /path/to/rules` to load from a custom directory containing `.LICENSE` and `.RULE` files.

### Current Public Output Surface

When license scanning is enabled, the current ScanCode-style public surface is:

- file-level `license_detections`
- file-level `license_clues`
- file/package `detection_log` when `--license-diagnostics` is enabled
- match-level `matched_text` under `--license-text`
- match-level `matched_text_diagnostics` under `--license-text-diagnostics`
- file-level `percentage_of_license_text`
- top-level unique `license_detections`
- top-level `license_references` and `license_rule_references`

`--from-json` preserves preexisting top-level license reference blocks and can
recompute the top-level license outputs when the loaded scan is reshaped or when
license-reference generation is explicitly requested.

The public-surface parity work is complete. This document is the evergreen
reference for the current public license-detection surface.

### Initialization Flow

```text
main.rs::init_license_engine()
    │
    ├── No --license-rules-path specified (default)
    │       ↓
    │   LicenseDetectionEngine::from_embedded()
    │       ↓
    │   Decompress embedded artifact (zstd)
    │       ↓
    │   Deserialize LoadedRule/LoadedLicense (MessagePack)
    │       ↓
    │   Build LicenseIndex
    │
    └── --license-rules-path specified
            ↓
        LicenseDetectionEngine::from_directory(rules_path)
            ↓
        Load .LICENSE and .RULE files from directory
            ↓
        Parse into LoadedRule/LoadedLicense
            ↓
        Build LicenseIndex
            ↓
Arc<LicenseDetectionEngine> shared across scanner threads
```

---

## Embedded License Index

The binary includes a pre-built license index embedded at compile time:

- **Location**: `resources/license_detection/license_index.zst`
- **Format**: MessagePack serialization, zstd compression
- **Contents**: sorted `LoadedRule` and `LoadedLicense` values derived from the ScanCode rules dataset

### Loader/Build Stage Separation

The loading process is split into two distinct stages:

**Artifact Generation Stage** (when producing `license_index.zst`):

- Parse `.RULE` and `.LICENSE` files
- Normalize rule and license data for embedding
- Sort embedded rules and licenses deterministically
- Serialize the embedded loader snapshot with MessagePack
- Compress the serialized bytes with zstd

**Build Stage** (runtime):

- Validate the embedded artifact payload and schema version
- Deserialize the embedded loader snapshot
- Convert embedded rules/licenses into the runtime `LicenseIndex`
- Apply deprecated filtering policy
- Synthesize license-derived rules
- Build token dictionary and automatons
- Create `LicenseIndex` and `SpdxMapping`

This separation enables:

- Self-contained binaries with no external dependencies
- Self-contained startup without filesystem parsing of the ScanCode rules directory at runtime
- Consistent rule loading across all installations

### Regenerating the Embedded Artifact

When the ScanCode rules dataset is updated, regenerate the embedded artifact:

```sh
# Initialize the reference submodule (contains the rules dataset)
./setup.sh

# Regenerate the artifact
cargo run --manifest-path xtask/Cargo.toml --bin generate-index-artifact

# Commit the updated artifact
git add resources/license_detection/license_index.zst
git commit -m "chore: update embedded license data"
```

---

## Core Components

### LicenseDetectionEngine

**File**: `src/license_detection/mod.rs`

The orchestrator that coordinates the detection pipeline.

```rust
pub struct LicenseDetectionEngine {
    index: Arc<LicenseIndex>,  // Pre-built index of rules/licenses
    spdx_mapping: SpdxMapping, // ScanCode ↔ SPDX key mapping
}
```

### LicenseIndex

**File**: `src/license_detection/index/mod.rs`

Pre-computed data structures for efficient matching:

| Field                   | Purpose                                      |
| ----------------------- | -------------------------------------------- |
| `dictionary`            | Token → ID mapping                           |
| `len_legalese`          | Count of high-value legalese tokens          |
| `digit_only_tids`       | Set of digit-only token IDs                  |
| `rules_by_rid`          | Rules indexed by ID                          |
| `tids_by_rid`           | Token ID sequences per rule                  |
| `rid_by_hash`           | SHA1 hash → rule ID (exact match)            |
| `rules_automaton`       | Aho-Corasick automaton for all rules         |
| `unknown_automaton`     | Automaton for unknown license detection      |
| `sets_by_rid`           | Unique token sets per rule                   |
| `msets_by_rid`          | Token frequency maps per rule                |
| `high_postings_by_rid`  | Inverted index for candidate selection       |
| `regular_rids`          | Set of regular (non-false-positive) rule IDs |
| `false_positive_rids`   | Set of false-positive rule IDs               |
| `approx_matchable_rids` | Set of approx-matchable rule IDs             |
| `licenses_by_key`       | ScanCode key → License mapping               |
| `pattern_id_to_rid`     | AhoCorasick pattern ID → rule ID             |
| `rid_by_spdx_key`       | SPDX license key → rule ID                   |
| `unknown_spdx_rid`      | Rule ID for unknown-spdx fallback            |

### Query

**File**: `src/license_detection/query/mod.rs`

Tokenized input text ready for matching:

```rust
pub struct Query<'a> {
    pub text: String,                          // Original text
    pub tokens: Vec<u16>,                      // Token IDs (known tokens only)
    pub line_by_pos: Vec<usize>,               // Position → line number
    pub unknowns_by_pos: HashMap<Option<i32>, usize>, // Unknown token counts
    pub stopwords_by_pos: HashMap<Option<i32>, usize>, // Stopword counts
    pub shorts_and_digits_pos: HashSet<usize>, // Short/digit token positions
    pub high_matchables: HashSet<usize>,       // Legalese token positions
    pub low_matchables: HashSet<usize>,        // Non-legalese token positions
    pub is_binary: bool,                       // Binary content flag
    pub query_run_ranges: Vec<(usize, Option<usize>)>, // Query run boundaries
    pub spdx_lines: Vec<(String, usize, usize)>, // SPDX identifier lines
    pub index: &'a LicenseIndex,               // Reference to index
}
```

Also includes `QueryRun` values created on demand from stored run ranges.

### Key Data Models

**File**: `src/license_detection/models/mod.rs`

| Struct         | Purpose                                                                 |
| -------------- | ----------------------------------------------------------------------- |
| `License`      | License metadata from .LICENSE files                                    |
| `Rule`         | Matchable pattern with flags (is_license_text, is_license_notice, etc.) |
| `LicenseMatch` | Single match result with score, position, matcher type                  |

---

## Detection Pipeline

```text
INPUT: File text content
        │
        ▼
┌───────────────────┐
│ 1. TOKENIZATION   │  text → Query (tokens, positions, matchables)
└─────────┬─────────┘
          │
          ▼
┌─────────────────────────────────────────────────────────────────────┐
│ 2. MATCHING (Priority Order)                                        │
│                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │ 1a. HASH MATCH (1-hash)                                     │    │
│  │     • SHA1 of token sequence → lookup in rid_by_hash        │    │
│  │     • 100% confidence, immediate return if found            │    │
│  └─────────────────────────────────────────────────────────────┘    │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │ 1b. SPDX-LID MATCH (1-spdx-id)                              │    │
│  │     • Parse SPDX-License-Identifier tags                    │    │
│  │     • Handle AND, OR, WITH expressions                      │    │
│  └─────────────────────────────────────────────────────────────┘    │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │ 1c. AHO-CORASICK MATCH (2-aho)                              │    │
│  │     • Multi-pattern exact matching via automaton            │    │
│  │     • Find all overlapping matches                          │    │
│  └─────────────────────────────────────────────────────────────┘    │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │ 2. NEAR-DUPLICATE MATCH                                     │    │
│  │     • Set similarity >= 0.8 for whole query                 │    │
│  │     • Sequence matching on top candidates                   │    │
│  └─────────────────────────────────────────────────────────────┘    │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │ 3. SEQUENCE MATCH (3-seq)                                   │    │
│  │     • Set-based candidate selection                         │    │
│  │     • Sequence alignment for scoring                        │    │
│  └─────────────────────────────────────────────────────────────┘    │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │ 4. QUERY RUN MATCH                                          │    │
│  │     • Process segmented regions separately                  │    │
│  │     • Skip already-matched regions                          │    │
│  └─────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────┘
          │
          ▼
┌───────────────────┐
│ 3. UNKNOWN MATCH  │  Detect license-like text in unmatched regions
└─────────┬─────────┘
          │
          ▼
┌─────────────────────────────────────────────────────────────────────┐
│ 4. REFINEMENT                                                       │
│     • Merge overlapping matches                                     │
│     • Filter false positives                                        │
│     • Filter too-short matches                                      │
│     • Validate required phrases                                     │
└─────────────────────────────────────────────────────────────────────┘
          │
          ▼
┌───────────────────┐
│ 5. GROUPING       │  Group nearby matches (within 4 lines)
└─────────┬─────────┘
          │
          ▼
┌─────────────────────────────────────────────────────────────────────┐
│ 6. DETECTION CREATION                                               │
│     • Combine and simplify equivalent expressions from matches       │
│     • Convert to SPDX identifiers while preserving SPDX casing       │
│     • Classify detection quality                                    │
└─────────────────────────────────────────────────────────────────────┘
          │
          ▼
OUTPUT: Vec<LicenseDetection>
```

---

## Matching Algorithms

### 1. Hash Match (1-hash)

**File**: `src/license_detection/hash_match.rs`

- **Purpose**: Exact match via SHA1 hash lookup
- **Complexity**: O(n) tokenization + O(1) lookup
- **Confidence**: 100%

### 2. SPDX-LID Match (1-spdx-id)

**File**: `src/license_detection/spdx_lid/mod.rs`

- **Purpose**: Detect `SPDX-License-Identifier:` tags
- **Handles**: Simple identifiers, expressions (AND, OR, WITH), WITH exceptions

### 3. Aho-Corasick Match (2-aho)

**File**: `src/license_detection/aho_match.rs`

- **Purpose**: Multi-pattern exact matching
- **Complexity**: O(n + m) where n = query length, m = matches
- **Process**: Encode tokens as bytes → run through automaton → verify positions

### 4. Sequence Match (3-seq)

**File**: `src/license_detection/seq_match/mod.rs`

- **Purpose**: Approximate/fuzzy matching for modified licenses
- **Phases**:
  1. Candidate selection via set similarity (Jaccard index)
  2. Ranking by containment, resemblance, matched length
  3. Sequence alignment for final scoring

```rust
pub struct ScoresVector {
    pub is_highly_resemblant: bool, // True if resemblance >= threshold
    pub containment: f32,           // Rule coverage in query
    pub resemblance: f32,           // Jaccard similarity (squared)
    pub matched_length: f32,        // Token overlap count
    pub rid: usize,                 // Rule ID for tie-breaking
}
```

### 5. Unknown Match (6-unknown)

**File**: `src/license_detection/unknown_match.rs`

- **Purpose**: Detect license-like text in unmatched regions
- **Process**: Find gaps → search with n-gram automaton (n=6) → count matches

---

## License Data Loading

### Source Files (for custom rules / regeneration)

**Location**: `reference/scancode-toolkit/src/licensedcode/data/`

| Directory   | Contents                                      |
| ----------- | --------------------------------------------- |
| `licenses/` | `.LICENSE` files (full license texts)         |
| `rules/`    | `.RULE` files (patterns, notices, references) |

> **Note**: The reference submodule is optional for end users. The default embedded license index is already included in the binary.

### File Format

Each file has YAML frontmatter:

```yaml
---
key: mit
name: MIT License
spdx_license_key: MIT
category: Permissive
is_license_text: true
---
MIT License text here...
```

### Index Building

**File**: `src/license_detection/index/builder/mod.rs`

Steps:

1. Load legalese tokens (high-value words)
2. Build token dictionary (assign integer IDs)
3. Tokenize each rule text
4. Compute SHA1 hash for each rule → `rid_by_hash`
5. Build Aho-Corasick automaton
6. Build sets/msets for candidate selection
7. Compute match thresholds

### Token Dictionary

**File**: `src/license_detection/index/dictionary.rs`

Token ID assignment order:

1. **Legalese tokens** (IDs 0..N-1): High-value words like "license", "copyright"
2. **Regular tokens** (IDs N..): Other words from rules

---

## Key Files Reference

| File                  | Role                                                  |
| --------------------- | ----------------------------------------------------- |
| `mod.rs`              | Engine orchestration, pipeline coordination           |
| `detection.rs`        | Detection grouping, classification                    |
| `models.rs`           | Core data structures (License, Rule, LicenseMatch)    |
| `query.rs`            | Input tokenization, position tracking                 |
| `tokenize.rs`         | Text → token conversion                               |
| `index/mod.rs`        | Index data structures                                 |
| `index/builder.rs`    | Index construction                                    |
| `index/dictionary.rs` | Token → ID mapping                                    |
| `index/token_sets.rs` | Token set/multiset operations for candidate selection |
| `hash_match.rs`       | Exact hash matching                                   |
| `spdx_lid.rs`         | SPDX-License-Identifier detection                     |
| `aho_match.rs`        | Aho-Corasick matching                                 |
| `seq_match.rs`        | Approximate sequence matching                         |
| `unknown_match.rs`    | Unknown license detection                             |
| `match_refine.rs`     | Match merging, filtering                              |
| `rules/loader.rs`     | .LICENSE and .RULE file parsing                       |
| `rules/legalese.rs`   | High-value token definitions                          |
| `rules/thresholds.rs` | Match threshold calculations                          |
| `spdx_mapping.rs`     | ScanCode ↔ SPDX key conversion                        |
| `expression.rs`       | License expression parsing                            |
| `spans.rs`            | Position span management                              |

---

## Constants and Thresholds

```rust
// Matcher identifiers
const MATCH_HASH: &str = "1-hash";
const MATCH_SPDX_ID: &str = "1-spdx-id";
const MATCH_AHO: &str = "2-aho";
const MATCH_SEQ: &str = "3-seq";
const MATCH_UNKNOWN: &str = "5-undetected";

// Thresholds
const LINES_THRESHOLD: usize = 4;            // Match grouping proximity
const HIGH_RESEMBLANCE_THRESHOLD: f32 = 0.8;  // Near-duplicate detection
const MAX_NEAR_DUPE_CANDIDATES: usize = 10;   // Top candidates to consider
const SMALL_RULE: usize = 15;                 // Rule size classification
const TINY_RULE: usize = 6;                   // Very small rules
```

---

## Output Structure

The engine still carries richer internal detection metadata than the current
public ScanCode-style JSON output. `detection_log`, clue-only serialization, and
matched-text diagnostics are now preserved publicly, and internal detections now
carry real file-region metadata for unique aggregation. File/resource
reference-following now consumes that metadata internally, but some downstream
package/reference consumers are still not fully represented in the current
serialized surfaces.

The current public-output structure and file-region-aware aggregation behavior
are described here.

### Internal Detection Structure

```rust
pub struct LicenseDetection {
    pub license_expression: Option<String>,      // ScanCode key
    pub license_expression_spdx: Option<String>, // SPDX identifier
    pub matches: Vec<LicenseMatch>,              // Individual matches
    pub detection_log: Vec<String>,              // Classification
    pub identifier: Option<String>,              // UUID
    pub file_regions: Vec<FileRegion>,           // Internal aggregated locations
}
```

### JSON Output Example

The current public JSON output still omits `file_region`, but it does preserve
`detection_log` on public detections.

```json
{
  "license_expression": "mit",
  "license_expression_spdx": "MIT",
  "matches": [
    {
      "license_expression": "mit",
      "matcher": "2-aho",
      "score": 1.0,
      "match_coverage": 100.0,
      "start_line": 1,
      "end_line": 20
    }
  ]
}
```
