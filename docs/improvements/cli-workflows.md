# CLI Workflows

## Type

- ✨ New Feature + 🐛 Bug Fix

## Python Reference Status

- Explicit selected-file scans still lean on argv expansion, include filters, or cwd-sensitive multi-input behavior.
- The upstream issue history shows the same pain point recurring for pull-request and changed-file workflows, especially when tooling must run from a fixed location instead of the repository root.
- There is still no settled first-class rooted path-list input that lets users say “scan this one tree, but only these listed files/directories” without shell glue.

This document is the landing zone for stable, user-visible CLI workflow improvements that go beyond the Python reference implementation.

## Improvement 1: Rooted selected-path scanning

### Rust Improvements

- Added `--paths-file <FILE>` for native scans under one explicit root.
- Entries are interpreted relative to that root instead of relative to the process cwd.
- `--paths-file -` reads the selected path list from stdin, so `git diff --name-only ... | provenant ... --paths-file -` works directly.
- Blank lines are ignored, CRLF line endings are tolerated, and directory entries keep selecting descendant files through the existing rooted include/filter pipeline.
- Missing entries are skipped with warnings instead of silently widening the scan scope.
- The resolved warnings flow into both terminal output and structured header warnings so automated consumers can see the same recoverable issue summary.

### Impact

- Pull-request and changed-file scans no longer depend on `xargs` or cwd-sensitive positional-argument workarounds.
- CI/container workflows can run Provenant from a fixed mount location while still scanning a repository elsewhere through one explicit root.
- Selected-file workflows stay within the existing single-root scan and shaping model, so output remains one coherent rooted tree instead of an ad hoc multi-root result.

## Improvement 2: Incremental rescans

### Python Reference Status

- The Python reference does not offer native unchanged-file reuse across repeated scans of the same tree.
- Re-running a large scan means paying the full collection and file-processing cost again, even when only a small subset changed.

### Rust Improvements

- Added opt-in `--incremental` reuse for repeated native scans of the same rooted tree.
- Provenant persists an incremental manifest under the shared cache root and reuses unchanged file results after validating stored metadata and SHA-256 against the previous completed scan.
- `--cache-dir` and `PROVENANT_CACHE` let users choose that shared cache root explicitly.
- `--cache-clear` clears the shared incremental and license-index cache state before a run without changing the scan contract.
- Incremental reuse stays separate from `--from-json`: replay reshapes an existing result, while `--incremental` accelerates fresh native rescans.

### Impact

- Repeated local and CI reruns can skip unchanged work instead of rescanning everything from scratch.
- Cache-root controls make the workflow usable in containerized and shared-runner environments.
- Users get a beyond-parity repeated-scan workflow without changing the output model or scan semantics.
