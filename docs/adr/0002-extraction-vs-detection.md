# ADR 0002: Extraction vs Detection Separation

**Status**: Accepted  
**Authors**: Provenant team
**Supersedes**: None

> **Current contract owner**: [`../ARCHITECTURE.md`](../ARCHITECTURE.md) and [`../HOW_TO_ADD_A_PARSER.md`](../HOW_TO_ADD_A_PARSER.md) describe the live parser/detection boundary. This ADR records the separation decision itself.

## Context

ScanCode Toolkit performs several distinct operations on code:

1. **Package metadata extraction** - Reading manifests, lockfiles, archives
2. **License detection** - Analyzing license text and normalizing to SPDX identifiers
3. **Copyright detection** - Finding copyright statements and extracting holder names
4. **Email/author detection** - Parsing author information from file content

In the Python reference implementation, these responsibilities are mixed within parser code, leading to:

- **Unclear boundaries** - Hard to tell what's extraction vs analysis
- **Complex parsers** - Single parser does too many things
- **Coupled testing** - Can't test extraction without detection logic
- **Inconsistent behavior** - Some parsers normalize licenses, others don't
- **Code duplication** - License normalization logic spread across parsers

**Critical Question**: Should parsers handle license detection and copyright extraction, or just extract raw data?

## Decision

**Parsers MUST remain separate from file-content detection, but they MAY normalize trustworthy declared package-license metadata. License text detection, copyright detection, and file-content email parsing remain separate pipeline stages.**

### Parser Responsibilities (✅ ALLOWED)

| Operation                                       | Example                                                               | Rationale                                   |
| ----------------------------------------------- | --------------------------------------------------------------------- | ------------------------------------------- |
| **Extract license statements**                  | `"license": "MIT"` → `extracted_license_statement: "MIT"`             | Raw data from manifest                      |
| **Normalize trustworthy declared license data** | `"license": "MIT OR Apache-2.0"` → declared fields + parser detection | Bounded declared package metadata semantics |
| **Extract license URLs**                        | `"license": {"url": "https://..."}` → store URL                       | Preserve metadata                           |
| **Extract license text**                        | `"license": {"text": "Permission is..."}` → store text                | Preserve full content                       |
| **Parse author/email from manifests**           | `"author": "Jane <jane@example.com>"` → `Party` object                | Structured manifest data                    |
| **Extract dependencies with scopes**            | `dependencies: {...}` → `Dependency` array                            | Manifest structure                          |
| **Extract checksums**                           | `"sha256": "abc123..."` → `sha256: "abc123..."`                       | Direct field mapping                        |

### Parser Prohibitions (❌ FORBIDDEN)

| Operation                                        | Why Forbidden                                           | Handled By / Alternative           |
| ------------------------------------------------ | ------------------------------------------------------- | ---------------------------------- |
| **Run fuzzy file-content license matching**      | Content scanning, confidence scoring, heavy rule search | License detection engine           |
| **Resolve arbitrary license URLs automatically** | Requires broader URL → identifier policy                | Future enrichment / detection flow |
| **Partially normalize mixed ambiguous inputs**   | Produces misleading declared package licenses           | Keep raw-only in parser            |
| **Backfill declared licenses from file scans**   | Conflates declared and discovered sources               | Separate enrichment stage          |
| **Extract copyright holders**                    | Requires grammar-based pattern matching                 | Copyright detection engine         |
| **Populate `holder` field**                      | Requires ClueCODE-style analysis                        | Copyright detection engine         |
| **Parse emails from file content**               | Requires regex patterns and scanning                    | Email detection engine             |

### Responsibility Matrix

#### License Detection

| Parser Responsibility                                               | Detection Engine Responsibility                                  |
| ------------------------------------------------------------------- | ---------------------------------------------------------------- |
| ✅ Populate `extracted_license_statement` with raw data             | ✅ Detect licenses from file text and broader content inputs     |
| ✅ Extract license URLs, text, fields AS-IS                         | ✅ Populate discovered/content-driven detections                 |
| ✅ Normalize trustworthy declared manifest license expressions      | ✅ Handle fuzzy matching, confidence scoring, and rule search    |
| ✅ Populate deterministic parser-side declared `license_detections` | ✅ Map file text to rules/identifiers                            |
| ❌ NEVER scan file content for parser-side license normalization    | ✅ Analyze license text with confidence scoring                  |
| ❌ NEVER backfill from sibling files silently in the parser         | ✅ Future enrichment may reconcile declared vs discovered inputs |

#### Copyright & Holder Detection

| Parser Responsibility                          | Detection Engine Responsibility                     |
| ---------------------------------------------- | --------------------------------------------------- |
| ✅ Extract raw copyright text (if in manifest) | ✅ Populate `holder` field from copyright analysis  |
| ❌ NEVER parse/extract holder names            | ✅ Use grammar-based copyright detection (ClueCODE) |
| ❌ NEVER populate `holder` field               | ✅ Scan file content for copyright statements       |
| ✅ Set `holder: None`                          | ✅ Extract holder names with pattern matching       |

#### Author/Email Parsing

| Parser Responsibility (Manifests)                | Detection Engine Responsibility (File Content) |
| ------------------------------------------------ | ---------------------------------------------- |
| ✅ Parse author/email from manifests (e.g., npm) | ✅ Scan source files for email patterns        |
| ✅ Create `Party` objects with name/email/role   | ✅ Parse Linux CREDITS files for authors       |
| ✅ Use utilities like `parse_name_email()`       | ✅ Separate plugin for email/author detection  |

### Data Flow Architecture

```text
┌─────────────────┐     ┌──────────────────┐     ┌─────────────┐
│ manifest.json   │────>│ License Engine   │────>│ PackageData │
│                 │     │                  │     │             │
│ "license": "MIT"│     │ normalize()      │     │ declared:   │
│                 │     │ confidence: 1.0  │     │   "mit"     │
│                 │     │                  │     │ spdx: "MIT" │
└─────────────────┘     └──────────────────┘     └─────────────┘
     PARSER                 DETECTION                 OUTPUT
     (extraction)           (analysis)                (combined)

extracted_license_statement = "MIT"
                                    ──> declared_license_expression = "mit"
                                    ──> declared_license_expression_spdx = "MIT"
```

## Consequences

### Benefits

1. **Clear Separation of Concerns**
   - Parsers stay focused on manifest/package metadata
   - File-content detection engines can be tested independently
   - Easier to understand and maintain

2. **Better Testing**
   - Test extraction logic without detection complexity
   - Mock detection engines for parser tests
   - Verify detection accuracy independently

3. **Consistency**
   - Trustworthy declared-license normalization uses one shared parser-side path
   - File-content license detection stays on the runtime detection pipeline
   - Copyright detection uses the separate copyright pipeline

4. **Performance Optimization**
   - Content detection can be parallelized separately
   - Manifest-only scans can still surface normalized declared package licenses
   - Detection results can be cached across files

5. **Feature Parity**
   - Matches Python's separation (licensedcode/, cluecode/)
   - Easier to verify against reference implementation
   - Clear migration path for detection algorithms

### Trade-offs

1. **Two-Stage Processing**
   - Must run extraction, then detection
   - Slight complexity in orchestration
   - Acceptable: mirrors Python architecture

2. **Bounded Parser Special Cases**
   - Parsers now have one intentional exception to pure extraction: trustworthy declared-license normalization
   - This adds a small amount of parser-side policy surface
   - Acceptable: it preserves clean declared-vs-discovered provenance while improving package metadata quality early

3. **Dual Provenance Paths**
   - Declared metadata and discovered file-content detections now have distinct normalization paths
   - That requires documentation to stay explicit about which path populated which fields
   - Acceptable: the boundary is clearer and closer to real package-scanning semantics

## Alternatives Considered

### 1. Unified Parser/Detector

**Approach**: Each parser handles extraction AND detection.

**Rejected because**:

- Mixes concerns (extraction + analysis)
- Harder to test independently
- Duplicates detection logic across parsers
- Inconsistent normalization behavior
- Doesn't match Python architecture

### 2. Detection in Post-Processing Hook

**Approach**: Parsers extract, then call hook for detection.

**Rejected because**:

- Still couples detection to parser lifecycle
- Hard to control when detection runs
- Difficult to parallelize effectively

### 3. Detection During Scan Orchestration

**Approach**: Scanner coordinates extraction, then detection.

**✅ ACCEPTED**: This is our approach (matches Python).

## Python Reference Comparison

The Python reference implementation already follows this separation:

**Extraction** (packagedcode/):

- `src/packagedcode/*.py` - Parser modules
- Extract raw data from manifests
- Populate `extracted_license_statement`

**Detection** (licensedcode/, cluecode/):

- `src/licensedcode/` - License detection engine
- `src/cluecode/copyrights.py` - Copyright detection
- `src/cluecode/plugin_email.py` - Email detection
- Separate pipeline stages

**Our Rust implementation mirrors this proven architecture.**

## Related ADRs

- [ADR 0001: Trait-Based Parser Architecture](0001-trait-based-parsers.md) - Parser structure
- [ADR 0003: Golden Test Strategy](0003-golden-test-strategy.md) - Why some tests are blocked on detection engine
- [ADR 0004: Security-First Parsing](0004-security-first-parsing.md) - Why we don't execute code

## References

- Python reference implementation follows the same separation:
  - Extraction in `packagedcode/` modules
  - License detection in `licensedcode/` modules
  - Copyright/email detection in `cluecode/` modules
