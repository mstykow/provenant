# Nix Parser — ADR 0004 Audit

**Status**: DONE
**Parser**: `nix.rs`
**ADR**: [0004-security-first-parsing](../adr/0004-security-first-parsing.md)

## Findings

| #   | Principle                  | Finding                                                         |
| --- | -------------------------- | --------------------------------------------------------------- |
| 1   | Recursion Depth            | No recursion depth limit on AST evaluation or wrapper traversal |
| 2   | File Size                  | No file size check before reading                               |
| 3   | Iteration Caps             | No iteration limit on tokenizer or parser loops                 |
| 4   | String Truncation          | No field-length truncation for extracted strings                |
| 5   | UTF-8                      | No explicit UTF-8 validation on file reads                      |
| 6   | No `.expect()`/`.unwrap()` | Some `.expect()` calls in parser paths                          |

## Remediation

All findings addressed in commit on branch `fix/adr0004-batch3-nix-meson-hexlock-debian-clojure` (PR #666).

| #   | Principle                  | Fix Applied                                                                                                    |
| --- | -------------------------- | -------------------------------------------------------------------------------------------------------------- |
| 1   | Recursion Depth            | Added `MAX_RECURSION_DEPTH=50` in `Parser` struct; AST eval functions check and return early on depth overflow |
| 2   | File Size                  | Replaced raw file reads with `read_file_to_string` which enforces the 100 MB size limit                        |
| 3   | Iteration Caps             | Added `MAX_ITERATION_COUNT` constant; tokenizer and parser loops break early with warning when exceeded        |
| 4   | String Truncation          | Applied `truncate_field` to all extracted string fields (description, URL, license, etc.)                      |
| 5   | UTF-8                      | `read_file_to_string` performs validated UTF-8 decode; malformed input returns an error instead of panicking   |
| 6   | No `.expect()`/`.unwrap()` | Replaced `.expect()` calls with proper `Result` propagation and safe match arms                                |
