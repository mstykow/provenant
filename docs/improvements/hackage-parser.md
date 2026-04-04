# Hackage Parser: Static Cabal + Stack Project Support

## Summary

Rust now parses the three highest-value Haskell project surfaces directly:

- `*.cabal`
- `cabal.project`
- `stack.yaml`

This adds Hackage package detection, project-level dependency visibility, and topology-planned project-root assembly for mixed Cabal/Stack repositories without invoking Cabal, Stack, or any project code.

## Upstream / Reference Context

The Python reference does not ship packagedcode parsers for these three Haskell surfaces and only provides minimal ecosystem classification for `.cabal` files.

## Rust Improvements

### 1. Static `*.cabal` package parsing

Rust now extracts useful package metadata from Cabal manifests, including:

- package name and version
- synopsis and description
- declared license statement
- homepage and bug tracker URLs
- author and maintainer parties
- category/keyword data
- source-repository URL

It also recovers component-level `build-depends` declarations across `library`, `executable`, and `test-suite` sections without evaluating any Cabal logic.

### 2. `cabal.project` dependency-surface recovery

Rust now parses the main project/workspace surfaces from `cabal.project`:

- `packages`
- `optional-packages`
- `extra-packages`
- `import`
- `source-repository-package`

Non-package configuration is preserved in `extra_data` so the project context is retained instead of being discarded.

### 3. `stack.yaml` dependency and config preservation

Rust now parses Stack project manifests for:

- `resolver` / `snapshot`
- `packages`
- `extra-deps`

Other Stack configuration, such as `flags`, `drop-packages`, and `ghc-options`, is preserved in `extra_data` for provenance and follow-on analysis.

### 4. Hackage project-root topology assembly

Rust now routes directories rooted by `cabal.project` or `stack.yaml` through topology planning and then assembles `*.cabal`, `cabal.project`, and `stack.yaml` with the existing Hackage merger.

That means the assembled package can combine:

- root package identity from `.cabal`
- project/workspace provenance from `cabal.project`
- Stack resolver/dependency state from `stack.yaml`

## Scope Boundary

This implementation intentionally covers only the three static surfaces above.

It intentionally does **not** add:

- `stack.yaml.lock`
- Cabal solver evaluation
- Stack command execution
- runtime interpretation of conditional/package code

## Coverage

This improvement is covered by:

- unit tests for Cabal metadata/dependency extraction
- unit tests for `cabal.project` surface parsing and config preservation
- unit tests for `stack.yaml` dependency/config parsing
- parser golden tests for all three surfaces
- an assembly golden fixture covering topology-planned project-root merge across all three Hackage datasource IDs
