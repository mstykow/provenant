# Meson Parser — ADR 0004 Audit

**Status**: DONE
**Parser**: `meson.rs`
**ADR**: [0004-security-first-parsing](../adr/0004-security-first-parsing.md)

## Findings

| #   | Principle         | Finding                                               |
| --- | ----------------- | ----------------------------------------------------- |
| 1   | Recursion Depth   | No recursion depth limit on nested expression parsing |
| 2   | File Size         | No file size check before reading                     |
| 3   | Iteration Caps    | No iteration limit on parser loops                    |
| 4   | String Truncation | No field-length truncation for extracted strings      |
| 5   | File Exists       | No pre-read file existence check                      |
| 6   | UTF-8             | No explicit UTF-8 validation on file reads            |

## Remediation

All findings addressed in commit on branch `fix/adr0004-batch3-nix-meson-hexlock-debian-clojure` (PR #666).

| #   | Principle         | Fix Applied                                                                                                    |
| --- | ----------------- | -------------------------------------------------------------------------------------------------------------- |
| 1   | Recursion Depth   | Added `MAX_RECURSION_DEPTH=50` in `Parser` struct; nested expression parsing checks depth before recursing     |
| 2   | File Size         | Replaced raw file reads with `read_file_to_string` which enforces the 100 MB size limit                        |
| 3   | Iteration Caps    | Added `MAX_ITERATION_COUNT` constant; parser loops break early with warning when exceeded                      |
| 4   | String Truncation | Applied `truncate_field` to all extracted string fields (project name, version, license, etc.)                 |
| 5   | File Exists       | `read_file_to_string` performs `fs::metadata()` check before attempting to read; missing files return an error |
| 6   | UTF-8             | `read_file_to_string` performs validated UTF-8 decode; malformed input returns an error instead of panicking   |
