# Plan 019: File Regions and Unique Detection Metadata

## Status

Complete.

Rust now has the file-region and unique-detection behavior this plan reopened to
track:

1. source paths are threaded into license-detection entrypoints,
2. detection-level file-region metadata is built with real `path`,
   `start_line`, and `end_line` values,
3. unique detections aggregate file regions across repeated occurrences, and
4. current top-level license-detection/reference flows consume that region-aware
   aggregation for live scan output.

## What Changed

The original concern in this plan was that Rust carried incomplete placeholder
file-region metadata and therefore could not support Python-style unique
detection aggregation or provenance-sensitive post-processing.

That is no longer true. The current Rust implementation builds file-region data
from real match provenance and uses it when computing top-level unique
`license_detections` and synchronized reference matches.

## Scope Clarification

Python's review-oriented `summarycode/todo.py` flow also consumes `FileRegion`,
but Provenant intentionally does not implement `--todo`.

That remaining difference is tracked as intentional product scope elsewhere:

- `docs/implementation-plans/infrastructure/CLI_PLAN.md`
- `docs/implementation-plans/post-processing/SUMMARIZATION_PLAN.md`

It should not be treated as an open license-detection engine gap.

## Relevant Rust Implementation Points

- `src/license_detection/detection/mod.rs`
- `src/license_detection/detection/types.rs`
- `src/models/file_info.rs`
- `src/post_processing/mod.rs`

## Relevant Reference Points

- Python `FileRegion` definition:
  `reference/scancode-toolkit/src/licensedcode/detection.py`
- Python review/TODO consumer that remains intentionally out of scope:
  `reference/scancode-toolkit/src/summarycode/todo.py`
