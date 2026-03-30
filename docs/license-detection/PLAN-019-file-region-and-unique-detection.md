# Plan 019: File Regions and Unique Detection Metadata

## Current Decision

Rust previously carried a partial `FileRegion` type in `src/license_detection/detection/`
but did not populate it correctly or use it anywhere meaningful. In particular,
`path` was always set to an empty string and the metadata was dropped before output.

We removed that incomplete Rust-only placeholder for now instead of keeping dead
model fields that suggested parity we do not actually have.

## What Python Does

Python defines `FileRegion(path, start_line, end_line)` in
`reference/scancode-toolkit/src/licensedcode/detection.py` and uses it as
internal post-processing metadata.

Important observations from the reference:

- `FileRegion` is built from a real file path plus detection line bounds.
- Normal detection JSON serialization excludes `file_region`.
- Python still uses file-region metadata for later stages such as:
  - unique-detection aggregation across files,
  - reference-following / post-scan logic that reads `file_region.path`,
  - todo / ambiguous-detection reporting.

So this is not a core per-file output feature, but it is real detection metadata
that supports other Python behaviors we do not implement yet.

## Remaining Rust Features

Rust now has detection-level file-region construction with a real source path and
uses that internal metadata to aggregate unique detections across repeated
regions. The remaining missing Python features are:

1. fuller package/file reference-following beyond the currently implemented
   local-file resolution paths,
2. any output surface equivalent to Python's todo / ambiguous-detection flow.

These missing pieces still block fuller provenance-sensitive license-output
parity,
especially:

- the remaining top-level unique `license_detections` edge cases that still
  depend on true file-region consumers rather than just region-aware
  aggregation,
- other post-scan consumers that need detection-to-file provenance.

## Likely Reintroduction Path

When we implement these Python features, reintroduce file-region metadata at the
detection assembly boundary instead of in low-level matchers.

Most likely work items:

1. ✅ Thread source path from `src/scanner/process.rs` into license-detection
   entrypoints.
2. ✅ Reintroduce a detection-level file-region type in
   `src/license_detection/detection/` with real `path`, `start_line`, and
   `end_line` values.
3. ✅ Implement the Python-style unique-detection / file-region aggregation step.
4. ✅ Add the first real consumer for file-resource local-file reference-following.
5. Add whichever additional consumer still needs the metadata before exposing it again.

## Relevant Reference Points

- Python `FileRegion` definition:
  `reference/scancode-toolkit/src/licensedcode/detection.py:150`
- Python `get_file_region()`:
  `reference/scancode-toolkit/src/licensedcode/detection.py:293`
- Python detection serialization excludes `file_region`:
  `reference/scancode-toolkit/src/licensedcode/detection.py:476`
- Python unique-detection serialization excludes `file_regions`:
  `reference/scancode-toolkit/src/licensedcode/detection.py:963`
- Rust detection assembly boundary:
  `src/license_detection/detection/mod.rs`
- Rust scanner call path that still has the source `Path`:
  `src/scanner/process.rs`
