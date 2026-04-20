# Carthage Parser Improvements

## Summary

Rust now ships static Carthage support for `Cartfile`, `Cartfile.private`, and `Cartfile.resolved` even though the Python ScanCode reference still has no production Carthage parser.
The supported surface covers dependency declaration parsing across all three Carthage origin types (`github`, `git`, `binary`) and pinned dependency state from the resolved lockfile.

## Python Status

- Python ScanCode does not currently ship a Carthage packagedcode parser.
- Upstream issue `aboutcode-org/scancode-toolkit#2656` remains open with no implementation.
- This gives Rust direct packagedcode support for Carthage dependency metadata that the Python reference does not currently provide.

## Rust Improvements

### `Cartfile` and `Cartfile.private` dependency extraction

- Rust now recognizes `Cartfile` and `Cartfile.private` and extracts direct dependency declarations.
- All three Carthage origin types are supported: `github` entries produce `pkg:github/` purls, while `git` and `binary` entries preserve source identity in dependency metadata.
- Version requirement operators (`>=`, `~>`, `==`) and branch/tag references are preserved as `extracted_requirement`.
- Inline comments are stripped from version specifications.

### Pinned dependency state from `Cartfile.resolved`

- Rust now parses `Cartfile.resolved` for locked dependency versions.
- Resolved versions are included in `pkg:github/` purls and dependencies are marked `is_pinned: true`.
- Sibling assembly keeps both the declared dependency view from `Cartfile` and the pinned dependency view from `Cartfile.resolved`.

## Guardrails

- Rust does **not** resolve or fetch dependencies, evaluate Xcode build settings, or inspect built framework artifacts.
- `git` and `binary` origin types do not produce purls because there is no standard purl type for arbitrary git URLs or binary distribution specs.

## References

- [Carthage Artifacts documentation](https://github.com/Carthage/Carthage/blob/master/Documentation/Artifacts.md)
- [Upstream ScanCode issue](https://github.com/aboutcode-org/scancode-toolkit/issues/2656)
