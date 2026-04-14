# Debian Parser — ADR 0004 Audit

**Status**: DONE
**Parser**: `debian.rs`
**ADR**: [0004-security-first-parsing](../adr/0004-security-first-parsing.md)

## Findings

| #   | Principle         | Finding                                                         |
| --- | ----------------- | --------------------------------------------------------------- |
| 1   | File Size         | File size check already present via `read_file_to_string`       |
| 2   | Iteration Caps    | No iteration limit on paragraph or field parsing loops          |
| 3   | String Truncation | No field-length truncation for extracted strings                |
| 4   | Archive Safety    | No archive bomb protections for `.deb` extraction               |
| 5   | UTF-8             | No explicit lossy UTF-8 handling for control file bytes         |
| 6   | No `.unwrap()`    | `.unwrap()` on `LazyLock` initialization and other parser paths |

## Remediation

All findings addressed in commit on branch `fix/adr0004-batch3-nix-meson-hexlock-debian-clojure` (PR #666).

| #   | Principle         | Fix Applied                                                                                                                                                                      |
| --- | ----------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | File Size         | `read_file_to_string` already enforces the 100 MB size limit; no change needed                                                                                                   |
| 2   | Iteration Caps    | Added `MAX_ITERATION_COUNT` constant; paragraph and field parsing loops break early with warning when exceeded                                                                   |
| 3   | String Truncation | Applied `truncate_field` to all extracted string fields (package name, version, description, etc.)                                                                               |
| 4   | Archive Safety    | Added 1 GB uncompressed limit, 100:1 compression ratio check, path traversal blocking (`../`), per-entry size limit, and bounded decompression reads for `.deb` archive handling |
| 5   | UTF-8             | Replaced raw byte handling with `from_utf8_lossy` for control file content; malformed bytes degrade gracefully                                                                   |
| 6   | No `.unwrap()`    | Replaced `LazyLock` `.unwrap()` with safe `match`/`if let` patterns; parser paths propagate errors via `Result`                                                                  |
