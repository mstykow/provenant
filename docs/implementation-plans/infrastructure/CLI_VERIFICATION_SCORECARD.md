# CLI Workflow Verification Scorecard

> **Status**: 🟡 Maintained mixed backlog — current high-value parity rows are verified, and the follow-up rows below track newer parity and Provenant-specific CLI workflows as they gain dedicated verification coverage
> **Current contract owner**: [`../../CLI_GUIDE.md`](../../CLI_GUIDE.md) for evergreen user workflows, [`CLI_PLAN.md`](CLI_PLAN.md) for the completed compatibility ledger, and [`../../xtask/README.md`](../../xtask/README.md#compare-outputs) for the maintained compare harness

This scorecard tracks **end-to-end CLI workflow verification targets** that are implemented in Provenant but are not already covered by the maintained parser-family checklist in [`../package-detection/PARSER_VERIFICATION_SCORECARD.md`](../package-detection/PARSER_VERIFICATION_SCORECARD.md) or the output-format contract in [`../output/PARITY_SCORECARD.md`](../output/PARITY_SCORECARD.md).

Unlike package-family verification, these rows are primarily **parity-first workflow checks**, not a durable “Provenant advantages” benchmark program. The maintained record here is therefore the row status plus the saved `compare-outputs` artifacts, PR descriptions, and CI logs for representative runs; only intentional durable divergences should graduate into evergreen docs or `docs/improvements/`.

The focus here is the **workflow surface**: imported-JSON replay, file-info shaping, native include selection, post-scan classification/tallies, package-only scans, policy/clue post-processing, and similar user-facing modes where the CLI materially changes what gets scanned or how the final ScanCode-style output is produced.

## Reference sources

- Evergreen user workflow guide: [`../../CLI_GUIDE.md`](../../CLI_GUIDE.md)
- Completed CLI parity ledger: [`CLI_PLAN.md`](CLI_PLAN.md)
- Compare harness and cache behavior: [`../../xtask/README.md`](../../xtask/README.md#compare-outputs)
- Maintained parser-family compare checklist: [`../package-detection/PARSER_VERIFICATION_SCORECARD.md`](../package-detection/PARSER_VERIFICATION_SCORECARD.md)
- Maintained output-format parity contract: [`../output/PARITY_SCORECARD.md`](../output/PARITY_SCORECARD.md)

## Required verification methodology

Use the repository-supported `xtask compare-outputs` workflow whenever both scanners can be exercised with the **same effective CLI flags and same input shape**.

Native repository-backed lane:

```bash
cargo run --manifest-path xtask/Cargo.toml --bin compare-outputs -- --repo-url https://github.com/org/repo.git --repo-ref <ref> -- <shared-cli-flags>
```

Imported-JSON replay lane using an existing shared ScanCode raw artifact:

```bash
cargo run --manifest-path xtask/Cargo.toml --bin compare-outputs -- \
  --target-path .provenant/scancode-cache/<cache-key>/scancode.json \
  --scancode-cache-identity <cache-key>-from-json \
  -- --from-json <shared-post-scan-flags>
```

Method rules:

- Prefer the existing shared inputs under [`.provenant/repo-cache/`](../../../.provenant/repo-cache/) and [`.provenant/scancode-cache/`](../../../.provenant/scancode-cache/) before fetching or generating new targets.
- For imported-JSON rows, point `--target-path` at the cached `scancode.json` file itself. The current compare harness already materializes single-file targets correctly for both scanners, so **single-input `--from-json` parity runs work today without a new helper**.
- Use `--scancode-cache-identity` for imported-JSON file targets so replay runs can reuse shared ScanCode artifacts intentionally instead of rerunning ad hoc local-file scans.
- If a replay row needs fields that are missing from the current cached JSON inputs — especially `--info`-gated file-info fields — seed a fresh shared ScanCode raw artifact first with a native `compare-outputs` run using the desired flags, then reuse the resulting cached `scancode.json` as the replay input.
- Treat any “more output” from either scanner as a claim to verify, not as proof by itself. Apply the same triage rigor used by the parser-family scorecard to top-level summary, tally, file-info, package, license, author, email, URL, and clue-filtering deltas.
- Keep detailed diff analysis and representative verified-run references in PR descriptions, CI logs, and saved `.provenant/compare-runs/` artifacts rather than bloating this checklist.
- If a CLI workflow needs durable prose beyond the status flip — for example a deliberate non-parity choice or a user-facing semantics note — document that in the evergreen CLI docs or in `docs/improvements/`, not in a benchmark-style table.
- When a row is verified, update the **Status** cell only. Keep the notes column stable unless the planned scope genuinely changes.

## Current local target pool

These inputs are already on disk and can seed the first CLI workflow verification runs without additional network fetches.

| Local target                      | Available source                                                                                  | Current ScanCode shape                 | Good first use                                                                    |
| --------------------------------- | ------------------------------------------------------------------------------------------------- | -------------------------------------- | --------------------------------------------------------------------------------- |
| `octocat/Hello-World @ 7fd1a60`   | matching repo mirror + `.provenant/scancode-cache/Hello-World-14e56786c31f9a0c/scancode.json`     | `-l --strip-root`                      | tiny `--from-json` smoke lane and simple shaping checks                           |
| `boostorg/boost @ 4f1cbeb`        | matching repo mirror + `.provenant/scancode-cache/boost-32e7ae6f522cac7d/scancode.json`           | `-clupe --strip-root`                  | medium imported-JSON replay, summary/tally, and clue-filtering lanes              |
| `boostorg/json @ 70efd4b`         | matching repo mirror + `.provenant/scancode-cache/json-d49c56e484abf068/scancode.json`            | `-clupe --system-package --strip-root` | medium mixed workflow lane with package-adjacent and installed-package coverage   |
| `kubernetes/kubernetes @ d3b9c54` | matching repo mirror + `.provenant/scancode-cache/kubernetes-9d287fcb5974bb1c/scancode.json`      | `-clupe --strip-root`                  | large imported-JSON and package-only stress lane                                  |
| local copyright fixture           | `.provenant/scancode-cache/anonymized-lint-directive-not-absorbed-e358ac074dc0c3df/scancode.json` | `--copyright`                          | tiny whole-resource filter smoke lane; not rich enough for package or policy rows |

Known gap in the current cache pool: none of the shared imported JSON artifacts were captured with `--info`, so any replay row that depends on `mime_type`, `file_type`, `programming_language`, `is_source`, `is_script`, `is_binary`, or `is_text` should first seed one small and one medium `--info` cache entry.

## Status model

- `⚪ Planned` — candidate targets and verification shape are known, but the workflow has not been fully compared and triaged yet.
- `🟡 Needs harness or seed input` — the workflow is worth verifying, but current local inputs or current `xtask` affordances are not quite enough yet.
- `🟢 Verified` — at least one representative compare run has been completed and any ScanCode-better findings for that workflow were fixed or triaged as not actually worse.

## Ranked verification backlog

The ranking below is ordered by **practical parity value first**: strongest direct ScanCode CLI equivalence, highest likelihood of exposing real user-visible workflow gaps, and best reuse of the current local cache pool.

| Priority | Workflow                                                                                                       | Status      | Candidate local targets                                                                                                                                                        | Priority and scope notes                                                                                                                                                                                                                                                                                                                                                               |
| -------- | -------------------------------------------------------------------------------------------------------------- | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0a       | Single-input `--from-json` replay and shaping                                                                  | 🟢 Verified | `Hello-World-14e56786c31f9a0c/scancode.json`<br>`boost-32e7ae6f522cac7d/scancode.json`<br>`json-d49c56e484abf068/scancode.json`<br>`kubernetes-9d287fcb5974bb1c/scancode.json` | Cleanest direct ScanCode CLI parity lane because both scanners explicitly support `--from-json`. Start here. Verify imported file/package retention, root-flag reshaping, top-level license-output recomputation, `--only-findings`, and shaping-time include/ignore behavior without conflating those semantics with fresh-scan gaps.                                                 |
| 0b       | Multi-input `--from-json` merge and top-level recomputation                                                    | 🟢 Verified | combine two or more cached `scancode.json` inputs from the rows above                                                                                                          | This is high-value because Provenant supports multi-input replay, but the current `compare-outputs` interface accepts only one target path and appends only one input argument. Add a small helper or extend `compare-outputs` to stage multiple imported JSON files before marking this lane verified.                                                                                |
| 1        | Native repeated-flag `--include` file-glob parity                                                              | 🟢 Verified | `boostorg/json @ 70efd4b`                                                                                                                                                      | Direct native parity lane for include-only scans when the shared intent is “keep files matching these globs”. Verified with compare run `20260422T144852Z-json-55918` using `--include '*.hpp' --include '*.cpp' --include '*.md'`; all 184 kept file paths matched on both scanners, and remaining deltas were ordinary license/holder differences rather than path-selection misses. |
| 2        | Native `--info` and `--mark-source` parity                                                                     | 🟢 Verified | `octocat/Hello-World @ 7fd1a60`<br>`boostorg/json @ 70efd4b`<br>`boostorg/boost @ 4f1cbeb`                                                                                     | Highest-value native-scan gap called out explicitly in [`CLI_PLAN.md`](CLI_PLAN.md#residual---info--file-info-parity-gaps). Start with direct repo comparisons using explicit `--info` runs. If replay verification is also desired later, first seed matching imported JSON inputs that include `--info`.                                                                             |
| 3        | `--classify`, `--summary`, `--tallies*`, `--facet`, and `--license-clarity-score` on imported JSON             | 🟢 Verified | `boost-32e7ae6f522cac7d/scancode.json`<br>`json-d49c56e484abf068/scancode.json`<br>`kubernetes-9d287fcb5974bb1c/scancode.json`                                                 | Strong user-facing post-scan workflow lane. Use imported JSON first so classification and tally deltas can be triaged separately from raw scan differences. If a chosen row depends on language/source booleans for facet or tally detail quality, seed an `--info` cache entry first.                                                                                                 |
| 4        | Native `--package-only` package-data-only scans                                                                | 🟢 Verified | `kubernetes/kubernetes @ d3b9c54`<br>`boostorg/json @ 70efd4b` as a lower-density contrast                                                                                     | Important because it changes runtime behavior substantially and is documented as a distinct workflow in [`../../CLI_GUIDE.md`](../../CLI_GUIDE.md). External adoption signals are weaker than for `--from-json`, so rank it after the imported replay and file-info lanes, but still keep it ahead of low-value Rust-only conveniences.                                                |
| 5        | `--license-policy` and `--filter-clues` post-scan workflows                                                    | 🟢 Verified | `Hello-World-14e56786c31f9a0c/scancode.json` for smoke<br>`boost-32e7ae6f522cac7d/scancode.json` for broader review                                                            | Both scanners expose post-scan workflow controls here, and imported JSON is the cleanest way to isolate policy/clue semantics from fresh detection drift. Add small checked-in policy fixtures for durable reruns if current ad hoc policy files are not already stable.                                                                                                               |
| 6        | Imported-JSON shaping filters with whole-resource suppression (`--ignore-author`, `--ignore-copyright-holder`) | 🟢 Verified | `boost-32e7ae6f522cac7d/scancode.json`<br>`anonymized-lint-directive-not-absorbed-e358ac074dc0c3df/scancode.json`                                                              | Lower breadth than rows `0a`–`5`, but still a real end-user workflow because it changes whole-resource visibility after scan import. Keep this explicit instead of letting it hide inside general `--from-json` smoke runs.                                                                                                                                                            |

## Recommended first execution slice

If starting from the current local cache pool, use this narrow order:

1. `0a` on `Hello-World-14e56786c31f9a0c/scancode.json` as the smallest `--from-json` smoke lane.
2. `0a` on `json-d49c56e484abf068/scancode.json` as the first medium imported-JSON parity lane.
3. `1` on `boostorg/json @ 70efd4b` with repeated file-glob `--include` flags as the native multi-include smoke lane.
4. `2` on `boostorg/json @ 70efd4b` with an explicit native `--info` compare run to seed the first reusable file-info-rich raw artifact.
5. `3` on the newly seeded `--info` imported JSON plus one existing `boost` or `kubernetes` imported JSON target.

## Provenant-specific follow-up (not direct ScanCode CLI parity)

These workflows matter, but they should be validated as **local contract tests** rather than forced into compare-to-ScanCode lanes:

| Workflow                                                  | Why it is out of the main parity backlog                                                                                                                                                                                                                                                                                                                                              |
| --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--show-attribution`                                      | Rust-specific convenience flag with no direct ScanCode CLI peer                                                                                                                                                                                                                                                                                                                       |
| `--no-assemble`                                           | Provenant-only convenience; Python ScanCode always assembles                                                                                                                                                                                                                                                                                                                          |
| `--incremental` repeated native reruns and speedup checks | 🟢 Locally verified on `boostorg/json @ 70efd4b` with `benchmark-target` run `20260422T161401` semantics: uncached-repeat and incremental-repeat outputs are identical after stripping volatile header timestamps, the incremental cache manifest recorded 705 entries, and the warm incremental rerun reused 705 unchanged files with a 3.04× wall-clock speedup (29.818s → 9.811s). |
| `--paths-file` rooted selected-path scans                 | 🟢 Locally verified on a tiny rooted fixture under `.provenant/manual-verification/selection-fixture`: stdin mode with CRLF input selected one exact file plus one directory subtree, ignored blank lines, surfaced the missing-entry warning in both header warnings and stderr, and preserved cwd independence by running from outside the scan root.                               |
| Explicit recursive native `--include` subtree semantics   | 🟢 Locally verified on the same fixture with the current CLI binary: `--include 'src/**'` kept only the `src` subtree files, while bare `--include 'src'` kept only the literal `src` directory entry and did not recurse into descendants.                                                                                                                                           |
| `--reindex`, `--no-license-index-cache`                   | Rust-specific cache/runtime controls rather than shared parity requirements                                                                                                                                                                                                                                                                                                           |
| `--custom-output --custom-template`                       | Template contract verification belongs with local output contract tests, not Python fixture parity                                                                                                                                                                                                                                                                                    |

Keep those flows visible in top-level CLI tests, but do not let them outrank the higher-value shared ScanCode workflow rows above.
