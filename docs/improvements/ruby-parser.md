# Ruby Parser Improvements

## Summary

Rust now goes beyond the current Python ScanCode Ruby handling in several concrete ways:

1. resolves gemspec constants from required local Ruby files instead of leaving all external constants unresolved
2. preserves Bundler `Gemfile` and `Gemfile.lock` source metadata at parser level and proves it with parser goldens
3. merges extracted gem metadata layouts without duplicate package/dependency emission and assigns nested extracted files to the assembled gem package
4. tags nested Ruby legal/readme/manifest files as `key_file`, promotes package metadata from them, and computes a top-level `license_clarity_score`

## Reference limitation

The Python reference already handles some Bundler and gemspec data, but constant resolution, direct `Gemfile` provenance retention, extracted-gem deduplication, false-dependency protection, and nested key-file attribution remain incomplete.

## Rust Improvements

### Required-file constant resolution

- Gemspec parsing now loads required local Ruby files when resolving constant-backed gemspec fields.
- This resolves values like:
  - `ProviderDSL::GemDescription::NAME`
  - `ProviderDSL::GemDescription::VERSION`
  - `ProviderDSL::GemDescription::AUTHORS`
  - `ProviderDSL::GemDescription::EMAIL`
  - `ProviderDSL::GemDescription::PAGE`
- The resolver stays intentionally narrow: it only looks at local required Ruby files adjacent to the gemspec, not arbitrary Ruby load paths.

### Gemfile.lock source metadata

- Bundler `GIT` and `PATH` metadata is now preserved for:
  - git `remote`
  - `revision`
  - `branch`
  - `ref`
  - source-type tagging
  - PATH primary-package identity behavior

### Gemfile manifest provenance metadata

- Direct `Gemfile` dependencies now preserve declared manifest provenance in dependency `extra_data` instead of dropping it on parse.
- Preserved metadata includes:
  - `git`
  - `path`
  - `branch`
  - `ref`
  - `tag`
  - per-dependency `source`
  - inherited top-level `source` URLs for plain registry dependencies
- The parser also records top-level Gemfile `source` declarations in package `extra_data.sources`, so manifest-only scans keep registry provenance even without a lockfile.

### False-dependency protection

- Gemspec parser coverage now explicitly proves that description text mentioning `add_dependency`-like strings does not create fake dependencies.

### Extracted gem assembly

- `metadata.gz-extract` now assembles with sibling extracted gemspec/Gemfile/Gemfile.lock layouts instead of standing alone.
- Extracted gem dependency duplication is deduped during nested merge.
- Ruby package-root resource assignment now attaches nested files under the gem root — including subdirectory `LICENSE` files and Ruby source files — to the assembled gem package.

### Key-file tagging and summary scoring

- File-level classification now tags package-associated Ruby legal/readme/manifest files with:
  - `is_legal`
  - `is_manifest`
  - `is_readme`
  - `is_top_level`
  - `is_key_file`
- Nested files listed in Ruby package `file_references` are treated as top-level for that package even when they are not at filesystem depth 1.
- Package metadata is promoted from key files when missing, including:
  - `copyright`
  - `holder`
- Key-file license clues now stay in summary/tally outputs rather than mutating package declared-license provenance.
- Output now includes a top-level `summary.license_clarity_score` block derived from key files, plus the combined summary declared license expression and core top-level tallies.

## Why this matters

- **Better gemspec fidelity**: narrow constant resolution recovers real package metadata without widening into arbitrary Ruby loading
- **Stronger Gemfile-only provenance**: manifest scans retain where dependencies came from even before a `Gemfile.lock` is available
- **Cleaner extracted-gem results**: nested metadata layouts merge without duplicate package noise
- **Better attribution**: nested legal and manifest files can contribute to package metadata and summary-level license clarity
