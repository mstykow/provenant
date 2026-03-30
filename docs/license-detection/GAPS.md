# License Detection Gaps

There are currently no open license-detection parity gaps tracked in this file.

The previously documented items here have been closed:

- `lic2/bsd-new_156.pdf` now extracts usable embedded PDF text in the Rust path
  and is no longer skipped in the golden suite.
- Python-compatible license and rule metadata is now carried through the Rust
  loader/index/output pipeline where it is relevant to current parity work.
- Expression key-set helpers are now used in live Rust reference-handling and
  license-reference collection paths, so they are no longer test-only building
  blocks.

Notes:

- Review-oriented `--todo` workflow parity from Python remains intentionally out
  of Provenant scope. That is a product-scope decision, not a license-detection
  engine gap. See
  `docs/implementation-plans/infrastructure/CLI_PLAN.md` and
  `docs/implementation-plans/post-processing/SUMMARIZATION_PLAN.md`.
- If a new parity regression is found, add it here only when it is an active,
  implementation-bound license-detection gap.
