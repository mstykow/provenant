# Copyright Detection Implementation Plan

> **Status**: 🟢 Historical rollout record — implemented with scanner/runtime ingestion parity complete
> **Current contract owner**: [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) for the current subsystem description and [`../../TESTING_STRATEGY.md`](../../TESTING_STRATEGY.md) for golden-fixture ownership and verification guidance
> **Historical priority / effort**: P1, completed
> **Dependencies at implementation time**: None (independent of license detection)

## Table of Contents

- [Overview](#overview)
- [Python Reference Analysis](#python-reference-analysis)
- [Rust Architecture](#rust-architecture)
- [Implementation Summary](#implementation-summary)
- [Beyond-Parity Improvements](#beyond-parity-improvements)
- [Golden Test Results](#golden-test-results)
- [Known Gaps and Follow-up Work](#known-gaps-and-follow-up-work)
- [Future Enhancements](#future-enhancements)

---

## Overview

This file is retained as a completed implementation record. Remaining gaps, future enhancements, and rollout summaries below are historical notes and should not be read as the live maintainer contract.

Copyright detection extracts copyright statements, holder names, author information, and year ranges from text-bearing scan inputs. This primarily means source and text files, and in Rust it additionally includes supported-image EXIF/XMP metadata as a beyond-parity clue source. It is the second most important text detection feature after license detection, and is completely independent — it can be implemented in parallel with license detection.

### Scope

**In Scope:**

- Copyright statement detection (`© 2024 Company Name`, `Copyright (c) 2024`, etc.)
- Copyright holder extraction (company names, person names)
- Author detection (`@author`, `Written by`, `Developed by`, etc.)
- Year and year-range parsing (`2020`, `2020-2024`, `2020, 2021, 2022`)
- Multi-line copyright statement handling
- Linux CREDITS file parsing (structured `N:/E:/W:` format)
- SPDX-FileCopyrightText and SPDX-FileContributor support
- Email and URL extraction within copyright context
- "All Rights Reserved" handling (multiple languages)
- Junk/false-positive filtering
- Scanner pipeline integration

**Out of Scope:**

- General email/URL extraction from source code (see `EMAIL_URL_DETECTION_PLAN.md`)
- Copyright policy evaluation
- License-copyright correlation (post-processing)

### Current State

**Implemented core detector and golden harness:**

- ✅ Copyright pattern matching engine
- ✅ Grammar parser
- ✅ Holder name extraction
- ✅ Year and year-range parsing (1960-2099)
- ✅ Multi-line statement handling
- ✅ Author detection
- ✅ Scanner integration now routes decoded non-UTF text, PDFs with extractable text, printable strings from `.dll` / `.exe` inputs, and supported-image EXIF/XMP metadata through a shared runtime ingestion helper before clue extraction
- ✅ Unicode name preservation (no transliteration — names like "François Müller" kept intact)
- ✅ Linux CREDITS file parsing
- ✅ Junk/false-positive filtering
- ✅ Thread-safe design via `LazyLock`
- ✅ Performance optimizations (long-line skip, encoded-data detection)
- ✅ Golden test infrastructure with Rust-owned fixtures
- ✅ API-level include filters (`include_copyrights`, `include_holders`, `include_authors`)
- ✅ Optional per-file detection runtime deadline (`Duration`-based)
- ✅ Improved Office/HTML demarkup for `<o:...>` tags (strips noisy Office tags)
- ✅ First-pass ICS false `(c)` junk filtering expansion (ternary/bitwise/cast-like code patterns)
- ✅ Multi-line span boundary fixes for `copyrighted by ...` + trailing `Copyright (c)` merges
- ✅ Multi-line HTML anchor span tracking (match start/end line mapping)
- ✅ Parenthesized obfuscated-email continuation merge for multi-line copyright notices

---

## Python Reference Analysis

### Architecture Overview

The Python implementation (`reference/scancode-toolkit/src/cluecode/copyrights.py`) uses a **four-stage pipeline**:

```text
┌──────────────┐    ┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│  1. Text     │───>│  2. Candidate│───>│  3. Lex +    │───>│  4. Tree     │
│  Preparation │    │  Selection   │    │  Parse       │    │  Walk +      │
│              │    │              │    │  (pygmars)   │    │  Refinement  │
└──────────────┘    └──────────────┘    └──────────────┘    └──────────────┘
```

#### Stage 1: Text Preparation (`prepare_text_line`)

Normalizes raw text lines before detection:

- **Copyright symbol normalization**: `©`, `(C)`, `(c)`, `&#169;`, `&#xa9;`, `\251`, `&copy;`, `u00a9` → all become `(c)`
- **HTML entity decoding**: `&amp;`, `&lt;`, `&gt;`, `&quot;`, `&#13;`, `&#10;`, `&ensp;`, `&emsp;`, etc.
- **Comment marker removal**: `/*`, `*/`, `*`, `#`, `%`, `rem`, `dnl`, `."` (man pages)
- **Markup stripping**: Debian `<s></s>` tags, HTML tags, RST `|copy|`
- **Quote normalization**: backticks and double quotes → single quotes
- **Escape handling**: `\t`, `\n`, `\r`, `\0` → spaces
- **Punctuation cleanup**: Remove `*#"%[]{}` backtick, fold consecutive quotes
- **ASCII normalization**: `toascii()` with transliteration (e.g., `ñ` → `n`) — **we intentionally skip this step** to preserve Unicode names in output
- **Emdash normalization**: `–` → `-`
- **Placeholder removal**: `<insert`, `year>`, `<year>`, `<name>`

#### Stage 2: Candidate Line Selection (`collect_candidate_lines`)

Filters lines to only those likely containing copyright information:

- **Hint-based filtering** (`copyrights_hint.py`): Lines containing any of ~25 markers:
  - `©`, `(c)`, `|copy|`, `&#169;`, `opyr`, `opyl`, `copr`, `right`, `reserv`, `auth`, `devel`, `<s>`, `</s>`, `by`, `@`, etc.
- **Year detection**: Lines containing years 1960-present
- **Gibberish detection**: Filters out binary/garbled text
- **Digit-only filtering**: Lines with only digits and punctuation are skipped
- **Multi-line grouping**: Groups consecutive candidate lines, with special handling for:
  - Lines ending with `copyright`, `and`, `by`, `,`, or a year → continue to next line
  - "All rights reserved" → end of statement marker
  - Empty lines → group boundary (unless inside a copyright statement)

#### Stage 3: Lexing + Parsing (pygmars)

Uses a **two-pass NLP-inspired approach**:

**Pass 1 — Lexing (POS Tagging):**

Tokenizes text on `[\t =;]+` and assigns Part-of-Speech (POS) tags via an ordered regex pattern set. Key tag categories:

| Tag              | Meaning                    | Examples                                              |
| ---------------- | -------------------------- | ----------------------------------------------------- |
| `COPY`           | Copyright keyword          | `Copyright`, `(c)`, `Copr.`, `SPDX-FileCopyrightText` |
| `YR`             | Year                       | `2024`, `1999,`                                       |
| `YR-RANGE`       | Year range (grammar-built) | `2020-2024`                                           |
| `NNP`            | Proper noun                | `John`, `Smith`, `California`                         |
| `CAPS`           | All-caps word              | `MIT`, `IBM`, `GOOGLE`                                |
| `COMP`           | Company suffix             | `Inc.`, `Ltd.`, `GmbH`, `Foundation`                  |
| `UNI`            | University                 | `University`, `College`, `Academy`                    |
| `NAME`           | Name (grammar-built)       | `John Smith`                                          |
| `COMPANY`        | Company (grammar-built)    | `Google Inc.`                                         |
| `AUTH` / `AUTH2` | Author keyword             | `Author`, `Written`, `Developed`                      |
| `EMAIL`          | Email address              | `foo@bar.com`                                         |
| `URL` / `URL2`   | URL                        | `http://example.com`, `example.com`                   |
| `CC`             | Conjunction                | `and`, `&`, `,`                                       |
| `VAN`            | Name particle              | `van`, `von`, `de`, `du`                              |
| `NN`             | Common noun                | (catch-all for unrecognized words)                    |
| `JUNK`           | Junk to ignore             | Programming keywords, HTML tags, etc.                 |

The PATTERNS list is **order-dependent** — first match wins. This is critical for correctness.

**Pass 2 — Grammar Parsing:**

A context-free grammar builds a parse tree from tagged tokens. Key grammar productions:

- `YR-RANGE`: Combines `YR`, `DASH`, `TO`, `CC` tokens into year ranges
- `NAME`: Combines `NNP`, `VAN`, `PN`, `CAPS` into person names
- `COMPANY`: Combines `NNP`, `COMP`, `UNI`, `CAPS` into organization names
- `COPYRIGHT`: Combines `COPY`, `YR-RANGE`, `NAME`, `COMPANY` into full statements
- `AUTHOR`: Combines `AUTH`, `NAME`, `COMPANY` into author attributions
- `ALLRIGHTRESERVED`: Matches "All Rights Reserved" patterns (multiple languages)

#### Stage 4: Tree Walk + Refinement

Walks the parse tree and yields `Detection` objects:

- **CopyrightDetection**: Full copyright statement with start/end lines
- **HolderDetection**: Extracted holder name (strips years, emails, URLs)
- **AuthorDetection**: Extracted author name

**Refinement functions** clean up detected strings:

- `refine_copyright()`: Strips punctuation, unbalanced parens, duplicate "Copyright" words, junk prefixes/suffixes
- `refine_holder()`: Strips dates, "All Rights Reserved", junk holders, normalizes names
- `refine_author()`: Strips author keywords, junk authors, normalizes names
- `is_junk_copyright()`: regex-based false-positive filtering (e.g., "copyright holder or simply", "full copyright statement")

#### Special Cases

**Linux CREDITS files** (`linux_credits.py`):

- Detects structured `N:/E:/W:` format used by Linux kernel, LLVM, Botan, etc.
- Yields `AuthorDetection` objects directly (bypasses main pipeline)
- Checks first 50 lines for structured format; bails out if none found

**"All Rights Reserved" in multiple languages:**

- English: "All Rights Reserved"
- German: "Alle Rechte vorbehalten"
- French: "Tous droits réservés"
- Spanish: "Reservados todos los derechos"
- Dutch: "Alle rechten voorbehouden"

### Known Bugs and Issues in Python

1. ✅ **FIXED — Suspicious regex in `_YEAR_YEAR`**: Underscore `_` in year-year separator removed
2. ✅ **FIXED — Duplicate patterns**: 3 duplicates removed
3. ✅ **FIXED — Global mutable state**: Replaced with `LazyLock` (thread-safe)
4. ✅ **FIXED — Year range `_YEAR`**: Extended from 2039 to 2099
5. ✅ **FIXED — `_YEAR_SHORT` typo**: `[0-][0-9]` → `[0-2][0-9]`
6. ✅ **FIXED — French/Spanish case bugs**: `[Dr]roits?` → `[Dd]roits?`, `[Dr]erechos` → `[Dd]erechos`
7. **Duplicate grammar rules**: Some rules have identical numbers — preserved for compatibility
8. **Order-dependent PATTERNS**: First-match-wins semantics preserved (same as Python)
9. **Excessive post-processing**: Grammar is permissive by design; refiners compensate
10. **Hardcoded names**: NNP exceptions preserved from Python (e.g., `suzuki`, `karsten`, `wiese`)

### Python Dependencies

The Python implementation depends on:

- **pygmars**: Custom lexer/parser library (regex-based POS tagger + CFG parser)
- **commoncode.text**: `toascii()` (Unicode→ASCII transliteration), `unixlinesep()` (line ending normalization)
- **textcode.gibberish**: `Gibberish` detector (filters binary/garbled text)
- **textcode.markup**: `strip_known_markup_from_text()` (HTML/XML tag removal)
- **textcode.analysis**: `numbered_text_lines()` (file reading with line numbers + optional demarkup)

---

## Rust Architecture

### Module Structure

```text
src/copyright/
├── mod.rs            # Public API: detect_copyrights(), re-exports
├── types.rs          # Core types: CopyrightDetection, HolderDetection, AuthorDetection, PosTag enum, Token, ParseNode, TreeLabel
├── prepare.rs        # 13-stage text normalization pipeline (Unicode preserved, no transliteration)
├── hints.rs          # Hint markers, year detection, gibberish filter
├── candidates.rs     # Candidate line selection, multi-line grouping, long-line + encoded-data skip
├── lexer.rs          # POS tagger — tokenize + classify via patterns
├── patterns.rs       # Ordered regex patterns for POS tagging
├── grammar.rs        # Grammar rules as Rust data structures
├── parser.rs         # Bottom-up grammar parser with max_iterations=50
├── detector.rs       # Full pipeline orchestrator + span-based extraction
├── refiner.rs        # Post-processing cleanup including junk-pattern filtering
├── credits.rs        # Linux CREDITS file parser
└── golden_test.rs    # Golden test harness using Rust-owned fixture expectations
```

### Key Design Decisions

#### 1. Enum-Based POS Tags (vs Python's String Tags)

**Python**: Tags are strings like `'COPY'`, `'NNP'`, `'JUNK'` — typos compile fine, no exhaustive matching.

**Rust**: `PosTag` enum — compiler enforces correctness, `match` is exhaustive, zero-cost abstraction.

#### 2. Sequential Pattern Matching with LazyLock (not RegexSet)

The original plan proposed `RegexSet` for parallel matching. In practice, we use `LazyLock<Vec<(Regex, PosTag)>>` — patterns compiled once at startup, matched sequentially per token with first-match-wins semantics.

**Why not RegexSet**: RegexSet cannot preserve match order, which is critical for first-match-wins semantics where pattern priority matters (e.g., JUNK exceptions must match before JUNK patterns).

#### 3. Thread-Safe via LazyLock (vs Python's Global Singleton)

**Python**: `DETECTOR = None` global singleton, not thread-safe.

**Rust**: All pattern data compiled once via `LazyLock` (thread-safe, zero-cost after init). Detection is a pure function — no global mutable state.

#### 4. Grammar as Rust Data Structures (vs Python's String Grammar)

**Python**: Grammar is a multi-line string parsed at runtime by pygmars.

**Rust**: Grammar rules encoded as `Vec<GrammarRule>` with `Matcher` enum variants (`Tag`, `Label`, `AnyTag`, `TagOrLabel`). Single-pass bottom-up parser with `max_iterations = 50` safety limit.

#### 5. Unicode Name Preservation (Beyond Parity)

**Python**: Calls `toascii(line, translit=True)` using `text_unidecode`, destroying Unicode characters (François→Francois, Müller→Muller).

**Rust**: Preserves original Unicode in output. The `deunicode` transliteration step was removed because it's unnecessary — the POS tagger uses Unicode-aware regex patterns (`\p{Lu}`, `\p{Ll}`) that correctly tag accented names as proper nouns. This is a beyond-parity improvement: names like "François Müller" appear in output with original accents intact.

#### 6. Performance Optimizations (Beyond Parity)

Two early-skip optimizations in candidate selection prevent pathological performance on non-copyright content:

- **Long-line skip**: Lines >2,000 chars without strong copyright indicators (`opyr`/`auth`/`(c)DIGIT`) are skipped before expensive regex processing. Protects against minified JS/CSS (e.g., 624KB single-line files).
- **Encoded-data detection**: Uuencode and base64 data lines are identified and skipped, preventing large clusters of false-positive candidates from weak hint markers like `@`.

#### 7. Scanner Integration — All Files

Copyright and license detection run on ALL files, including package manifests. Package parsing and text-based detection are independent (matching Python's plugin architecture where plugins run independently on every file).

---

## Implementation Summary

All 6 phases are complete:

| Phase                         | Deliverables                                                        | Status      |
| ----------------------------- | ------------------------------------------------------------------- | ----------- |
| **1. Core Types & Text Prep** | `types.rs`, `hints.rs`, `prepare.rs`, `candidates.rs`, `credits.rs` | ✅ Complete |
| **2. Lexer (POS Tagger)**     | `patterns.rs`, `lexer.rs`                                           | ✅ Complete |
| **3. Grammar Parser**         | `grammar.rs`, `parser.rs`                                           | ✅ Complete |
| **4. Detection & Refinement** | `detector.rs`, `refiner.rs`                                         | ✅ Complete |
| **5. Scanner Integration**    | `mod.rs` public API, `process.rs` integration, output format        | ✅ Complete |
| **6. Testing & Golden Tests** | Comprehensive unit tests and golden tests                           | ✅ Complete |

---

## Beyond-Parity Improvements

Documented in detail in [`docs/improvements/copyright-detection.md`](../../improvements/copyright-detection.md).

| #   | Type           | Improvement                                                              |
| --- | -------------- | ------------------------------------------------------------------------ |
| 1   | 🐛 Bug Fix     | Extended year range from 2039 to 2099                                    |
| 2   | 🐛 Bug Fix     | Fixed `_YEAR_SHORT` regex typo (`[0-][0-9]` → `[0-2][0-9]`)              |
| 3   | 🐛 Bug Fix     | Fixed French/Spanish case-sensitivity (`[Dr]roits?` → `[Dd]roits?`)      |
| 4   | 🐛 Bug Fix     | Fixed suspicious underscore in `_YEAR_YEAR` separator                    |
| 5   | ✨ Enhanced    | Unicode name preservation in output (Python outputs ASCII-only)          |
| 6   | 🔍 Enhanced    | Type-safe POS tags (enum vs strings)                                     |
| 7   | 🔍 Enhanced    | Thread-safe design (`LazyLock` vs global mutable singleton)              |
| 8   | 🔍 Enhanced    | Deduplicated 3 duplicate patterns from Python reference                  |
| 9   | ⚡ Performance | Long-line skip for minified JS/CSS (avoids pathological regex)           |
| 10  | ⚡ Performance | Encoded-data detection (uuencode/base64 skip for high-noise inputs)      |
| 11  | 🔍 Enhanced    | Rust-owned copyright golden fixtures with deterministic sync/update flow |
| 12  | 🛡️ Security    | No code execution, no global mutable state                               |
| 13  | ✨ Enhanced    | EXIF/XMP image metadata clue extraction for supported image formats      |

---

## Golden Test Results

### Current Baseline

Golden test suites (copyrights, holders, authors, ICS) validate output against the Python reference while preserving intentional Rust improvements. Here, **ICS** refers to the Android Ice Cream Sandwich (Android 4.0) fixture corpus used by the upstream ScanCode copyright tests. Fixtures in this repository are treated as Rust-owned expectations, and update tooling is designed to keep them stable and deterministic.

### Important Scope Note About Golden Coverage

Current Rust golden coverage primarily validates the **detector over normalized text content**, not the full scanner ingestion path.

- `src/copyright/golden_test.rs` loads fixture bytes through `src/copyright/golden_utils.rs::read_input_content()` and then calls `detect_copyrights(&content)` directly.
- The golden harness now shares the same path-aware ingestion helper as the live scanner for decoded non-UTF text, PDFs with extractable text, printable `.dll` / `.exe` strings, and supported-image EXIF/XMP metadata.
- Rust currently ports the upstream `copyrights`, `holders`, `authors`, and `ics` fixture families into local copyright golden tests, but does **not** yet provide equivalent scanner-level parity coverage for upstream `credits`, `years`, `generated`, and `copyright_fossology` families.

As a result, passing copyright golden tests is strong evidence that the detector logic works on the same normalized input classes as the live scanner, but it is **not sufficient evidence on its own** to replace scanner-level integration coverage.

### Behavioral Contract vs Python Reference

This implementation follows a simple compatibility contract:

1. **Parity by default**: Match Python ScanCode behavior for mainstream copyright/holder/author patterns.
2. **Intentional improvements are explicit**: Any deliberate behavioral differences are documented as improvements (for example, Unicode preservation and bug fixes).
3. **Remaining gaps are tracked**: Non-intentional divergences are documented as known gaps with follow-up priority.
4. **Rust-owned expectations**: Local golden fixtures are the authoritative expected output for this repository.

For users migrating from Python ScanCode, the expected experience is high compatibility with occasional differences in edge cases, each either documented as an intentional improvement or tracked as a parity gap.

Run golden tests with: `cargo test --features golden-tests copyright::golden_test -- --nocapture`

---

## Known Gaps and Follow-up Work

### Runtime Text-Ingestion Parity: Closed

The Rust detector and the **live scanner text-ingestion path now cover the same major text-bearing input classes that mattered for this plan**:

- decoded non-UTF text files,
- PDFs with extractable text, and
- printable strings from `.dll` / `.exe` binaries.

Rust also now supports a **beyond-parity** image metadata path for supported image formats, extracting clue-bearing EXIF/XMP text and feeding it into the same downstream detectors.

The scanner now derives text through a shared helper in `src/utils/file.rs`, and the copyright golden harness reuses the same helper via `src/copyright/golden_utils.rs::read_input_content()`. This keeps runtime and fixture ingestion aligned for the inputs exercised by this plan.

- **Python reference behavior**: `textcode.analysis.numbered_text_lines()` is path-aware and routes extractable PDFs and text-bearing binaries into the same downstream clue detectors.
- **Current Rust runtime behavior**: the scanner process pipeline under `src/scanner/process/` now uses the shared path-aware ingestion helper before copyright/email/url/license text detection, so the live scanner no longer misses these input classes.
- **Not part of this parity plan**: image/media metadata extraction. The Python reference `tests/textcode/test_analysis.py` explicitly asserts that files in `media_without_text/` yield no numbered text lines, so lack of image metadata scanning is not a parity shortfall here. Rust now implements EXIF/XMP image metadata clue extraction for supported image formats as a separate beyond-parity capability.
- **Separate intentional divergence**: the scanner process special-case handling under `src/scanner/process/` still short-circuits PEM certificate files before clue extraction. This differs from the Python reference, but it remains an explicit product decision made to resolve Rust issue `#222`, not a hidden parity gap in the copyright/email/url scanner path.

**Classification**: Closed parity gap.

**User impact**: The Rust CLI/runtime scanner now reaches the same copyright/email/url text-detection surfaces as the Python reference for regular text, non-UTF text, extractable PDFs, and text-bearing DLL/EXE inputs, while additionally surfacing clue-bearing EXIF/XMP metadata from supported image formats.

**Status**: Closed.

The original parity-gap buckets below are now considered **closed for this plan**.

We intentionally moved to a Rust-owned golden baseline and accepted superior, deterministic behavior where Python reference outputs are inconsistent or lower quality. Differences against Python are now treated as either:

1. documented intentional divergence, or
2. future optional tuning opportunities (not release blockers).

### Category 1: Complex Multi-line Copyrights with URLs

Long copyright statements spanning many lines with inline URLs and multiple holders. The span collection collects slightly more or less text than Python.

**Example**: `partial_detection.txt` — multi-line file with Debian markup, inline emails, and multi-holder copyrights. Python detects duplicate `(c)` variants; our refiner deduplicates more aggressively.

**Classification**: Closed in this plan (intentional improved behavior accepted).

**User impact**: Users get cleaner, deduplicated long-span notices with stable boundaries.

**Status**: Closed for this phase.

### Category 2: ICS False `(c)` Code Patterns

ICS source files containing `(c)` in C code contexts (type casts, ternary operators) can be falsely detected as copyright symbols. Junk-pattern filtering catches most cases; a first expanded batch for cast/ternary/bitwise forms is now implemented, but additional tails remain.

**Example**: `iptables-extensions/libxt_LED.c` — `(c)` appears in bitwise/ternary code expressions.

**Classification**: Closed in this plan (false-positive reduction prioritized over Python quirks).

**User impact**: Reduced false positives on code-heavy files.

**Status**: Closed for this phase.

### Category 3: HTML/Markup-Heavy Files

Files with heavy HTML markup (credits pages, documentation) where tag stripping produces slightly different whitespace or token boundaries than Python. Multi-line anchor span tracking and selected Office/markup cleanup fixes are implemented, but broader demarkup parity remains.

**Example**: `bzip2/manual.html`, `sonivox-docs/JET_*.html` — complex HTML with copyright notices embedded in markup.

**Classification**: Closed in this plan (deterministic + semantic canonicalization).

**Intentional divergence (resolved conflict class)**: `url_in_html-detail_9_html.html` and `html_incorrect-detail_9_html.html` are byte-identical inputs with conflicting Python reference expectations. Rust now enforces content-deterministic output for identical bytes and uses a canonical, cleaner extraction (`(c) 2004-2009 pudn.com`, holder `pudn.com`) instead of fixture-name-dependent behavior.

**User impact**: More deterministic and semantically cleaner markup-derived detections.

**Status**: Closed for this phase.

### Category 4: Edge-Case Copyright Phrasings

Unusual copyright formats that the grammar doesn't fully cover.

**Example**: "copyrighted by [holder]" phrasing, "Copyright or Copr." variants, copyright statements split across comment decorators.

**Classification**: Closed in this plan.

**User impact**: Edge phrasing support is now covered by detector heuristics and regression tests.

**Status**: Closed for this phase.

### Category 5: Remaining Author/Holder Gaps

Specific holder/author detections that differ from Python.

**Example**: "Originally by [Name]" not detected as author (tagged as Junk), address continuation after "Inc." truncated.

**Classification**: Closed in this plan.

**User impact**: Author/holder extraction is now consistently preserved in the tracked bucket fixtures.

**Status**: Closed for this phase.

---

## Testing

### Test Coverage

Every module has comprehensive unit tests covering its core functionality. Golden test suites (copyrights, holders, authors, ICS) validate end-to-end output against the Rust-owned deterministic baseline, with Python reference used as a comparison input rather than a strict blocker.

Key coverage areas:

- **Text preparation**: All normalization paths, Unicode preservation, HTML entity decoding
- **Scanner ingestion**: end-to-end coverage for non-UTF text, PDFs, DLL/EXE printable strings, and EXIF/XMP image metadata
- **Candidate selection**: Line grouping, multi-line handling, long-line skip, encoded-data detection
- **Refinement**: String cleanup, junk filtering, edge cases
- **Hints**: Hint markers, year detection, gibberish filtering
- **Patterns**: POS tag categories, regex correctness
- **Detection**: End-to-end pipeline, Unicode holders, span extraction
- **Grammar/Parser**: Rule existence, pattern matching, tree building
- **Credits**: CREDITS file parsing, structured format

### Testing Workflow

```bash
# Unit tests:
cargo test --all

# Golden tests:
cargo test --features golden-tests copyright::golden_test -- --nocapture

# Specific golden suite:
cargo test --features golden-tests copyright::golden_test::tests::test_golden_copyrights -- --nocapture
```

---

## Success Criteria

- [x] Detects all standard copyright formats (`©`, `(c)`, `Copyright`, `Copr.`, `SPDX-FileCopyrightText`)
- [x] Extracts holder names accurately (companies, persons, organizations)
- [x] Parses year ranges correctly (single years, ranges, comma-separated)
- [x] Handles multi-line copyright statements
- [x] Detects authors (`@author`, `Written by`, `Developed by`, etc.)
- [x] Parses Linux CREDITS files
- [x] Handles "All Rights Reserved" in English, German, French, Spanish, Dutch
- [x] Filters junk/false positives effectively
- [x] Golden tests validate behavior against Python reference while preserving intentional Rust differences
- [x] Thread-safe (no global mutable state)
- [x] All known Python bugs are fixed
- [x] `cargo clippy` clean, `cargo fmt` clean
- [x] Comprehensive test coverage (unit tests across all modules + golden tests)

---

## Future Enhancements

### Priority 1: Optional Quality Tuning (Post-Closure)

Core bucket closure is complete. Remaining work is optional and quality-oriented:

1. **Additional demarkup normalization** for unusually noisy markup sources.
2. **Further false-positive hardening** for rare code/comment hybrids.
3. **Selective readability normalization** of long extracted statements where user clarity can be improved.

### Priority 2: Performance Optimizations

- **RegexSet pre-filter**: The sequential regex pattern set per token could potentially use a pre-filter to reduce matching overhead while preserving first-match-wins semantics.
- ✅ **Per-file deadline/timeout**: Implemented wall-clock runtime limit plumbing (`max_runtime`) and parser-aware deadline checks.

### Priority 3: Feature Enhancements

- ✅ **`include_*` filtering parameters**: Implemented via `CopyrightDetectionOptions` and `detect_copyrights_with_options()`.
- **Full `demarkup` preprocessing**: Python calls `strip_known_markup_from_text` which handles HTML, RST, roff, and other markup formats more aggressively. Our `prepare_text_line` does basic normalization but doesn't strip full document markup. Would improve detection on heavily marked-up files.
- **Grammar top-level rules**: The grammar's COPYRIGHT/AUTHOR top-level rules don't always fire, so detection relies on span-based extraction as fallback. Investigating why these rules don't match could improve structural accuracy.

### Documentation Maintenance Rules for This Plan

- Prefer stable, qualitative language over volatile metrics.
- Document user-visible behavior and rationale before low-level implementation details.
- When behavior changes intentionally, record the reason and expected user impact.
- When behavior differs unintentionally from Python, classify it as a parity gap and track follow-up priority.

---

## Related Documents

- **Architecture**: [`docs/ARCHITECTURE.md`](../../ARCHITECTURE.md) — Scanner pipeline, copyright detection section
- **Improvements**: [`docs/improvements/copyright-detection.md`](../../improvements/copyright-detection.md) — Beyond-parity improvements over Python
- **Email/URL Detection**: [`EMAIL_URL_DETECTION_PLAN.md`](EMAIL_URL_DETECTION_PLAN.md) — Related text extraction
- **License Detection Architecture**: [`docs/LICENSE_DETECTION_ARCHITECTURE.md`](../../LICENSE_DETECTION_ARCHITECTURE.md) — Implemented license-detection engine and related matching approach
- **Testing Strategy**: [`docs/TESTING_STRATEGY.md`](../../TESTING_STRATEGY.md) — Testing approach
- **Python Reference**: `reference/scancode-toolkit/src/cluecode/copyrights.py` — Original implementation

---

## Appendix: Python File Inventory

| File                  | Purpose                                                                                            |
| --------------------- | -------------------------------------------------------------------------------------------------- |
| `copyrights.py`       | Main detection module: lexer patterns, grammar, detector, refiners, candidate selection, text prep |
| `copyrights_hint.py`  | Hint markers for candidate line selection, year regex, copyright symbol variants                   |
| `plugin_copyright.py` | Scanner plugin integration (thin wrapper)                                                          |
| `linux_credits.py`    | Linux CREDITS file parser (structured N:/E:/W: format)                                             |
| `finder.py`           | Email/URL finding (shared with email/URL detection — out of scope for this plan)                   |
| `finder_data.py`      | Junk email/URL/host classification data (shared — out of scope)                                    |
