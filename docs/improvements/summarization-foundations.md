# Summarization Foundations Improvements

## Summary

Rust now has the first reusable post-processing summary foundation layer, even though full Python `summarycode` parity is still not complete.

This incremental layer adds:

1. file-level key-file classification flags
2. package metadata promotion from key files
3. top-level `summary` output structure
4. top-level codebase `tallies` output for detected license expressions, copyrights, holders, authors, and programming languages
5. initial non-license-dependent summary fields such as `declared_holder`, `primary_language`, and `other_languages`

## Why This Matters

Historically, parser/assembly parity work could associate files to packages, but there was no generic summary layer that turned those relationships into higher-level scan output.

That meant ecosystem-specific fixes like Ruby nested `LICENSE` handling had nowhere to publish their effect except ad hoc package fields.

This foundation creates the missing bridge between:

- parser/assembly/package relationships
- file classification
- package metadata promotion
- top-level scan summary output

## Implemented Foundations

### Key-file tagging

Files can now be tagged with:

- `is_legal`
- `is_manifest`
- `is_readme`
- `is_top_level`
- `is_key_file`

These flags are driven by package association, file references, and package-root context rather than only raw filesystem depth.

### Package metadata promotion

When package metadata is missing, key files can now backfill:

- `copyright`
- `holder`

Key-file license clues now stay in summary/tally outputs rather than mutating package declared-license provenance.

### Summary output model

Top-level output now supports a `summary` block that can evolve incrementally without forcing all ecosystems to wait for full summarizer parity.

### Core top-level tallies

Top-level output now also supports a `tallies` block for codebase-wide aggregation of:

- `detected_license_expression`
- `copyrights`
- `holders`
- `authors`
- `programming_language`

### Initial non-license-dependent summary fields

The current incremental layer now computes:

- `declared_holder`
- `primary_language`
- `other_languages`

These use already-available package/file metadata and do not depend on full license-tally parity.

## Not Full Python Parity Yet

This does **not** mean Rust now matches Python `summarycode` completely.

Still open:

- package tallies
- `tallies_of_key_files`
- `tallies_with_details`
- `tallies_by_facet`
- full `license_clarity_score` heuristic parity
- facets
- generated code detection
- broader summary output parity

## Coverage

Coverage spans package-to-summary promotion, key-file tagging, and the top-level summary fields described above.

## Related Plans

- `docs/implementation-plans/post-processing/SUMMARIZATION_PLAN.md`
- `docs/implementation-plans/package-detection/PARSER_PLAN.md`
