# Package Detection Benchmarks

> **Status**: 🟢 Canonical package-detection verification reference for recorded `compare-outputs` runs, timing snapshots, and notable Provenant-vs-ScanCode outcomes.
> **Canonical workflow**: [`xtask/README.md`](../xtask/README.md#compare-outputs)

This document records explicit `compare-outputs` runs with high-level timing metrics, verification context, and notable end-state Provenant-vs-ScanCode outcomes.

It is the maintained package-detection verification record for what was compared, how it performed, and why the result matters.

## Timing methodology

- Use the repository-supported `compare-outputs` workflow with `--profile common`.
- Record same-host wall-clock timings for Provenant and ScanCode, plus relative speedup.
- Record machine information per row. If `run-manifest.json` reports `scancode.cache_hit: true`, use the cached ScanCode raw timing for that target/ref/runtime.

## Current benchmark examples

| Target                                                                                                                      | Files | Machine info                                           | Provenant total | ScanCode total | Relative result            | Notable Provenant advantages                                                                                                                                                |
| --------------------------------------------------------------------------------------------------------------------------- | ----: | ------------------------------------------------------ | --------------: | -------------: | -------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [`boostorg/boost @ 4f1cbeb`](https://github.com/boostorg/boost/tree/4f1cbeb724d9f3c08a826fbcee5a3db2f5480441)               |   236 | `macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc` |        `10.60s` |       `58.14s` | `5.47×` faster (`-81.7%`)  | More real copyright/author detections and cleaner copyright/author normalization                                                                                            |
| [`boostorg/json @ 70efd4b`](https://github.com/boostorg/json/tree/70efd4b032b7f3e718bb4ca4ae144c3171b21568)                 |   701 | `macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc` |        `29.11s` |      `139.57s` | `4.79×` faster (`-79.1%`)  | Better structured-metadata handling, cleaner GSoC name normalization, and correct alternative-license interpretation for the embedded Ryu headers                           |
| [`kubernetes/kubernetes @ d3b9c54`](https://github.com/kubernetes/kubernetes/tree/d3b9c54bd952117924fb0790f6989c0d30715b19) | 29080 | `macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc` |       `180.54s` |     `2573.77s` | `14.26×` faster (`-93.0%`) | Broader Dockerfile and `go.work` package coverage, cleaner local-license-reference resolution, and fewer noisy license-expression artifacts                                 |
| `debian:bookworm-slim` rootfs `sha256:f06537653ac770703bc45b4b113475bd402f451e85223f0f2837acbf89ab020a`                     |  3267 | `macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc` |        `21.05s` |      `156.25s` | `7.42×` faster (`-86.5%`)  | Better Debian dependency relationships from `dpkg/status`, source-faithful local-license resolution, and cleaner author/email/url results under the shared `common` profile |

## How to extend this document

For each new benchmark example, record:

1. target URL or artifact identity, with the resolved ref/SHA embedded in the target link when applicable
2. machine information for that specific benchmark row
3. scan profile and any important scan args
4. Provenant total time, ScanCode total time, and relative speedup
5. a short table-cell summary of notable Provenant advantages or accepted non-regression deltas, written as an end-state tool comparison rather than implementation history
