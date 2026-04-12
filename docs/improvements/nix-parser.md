# Nix Parser Improvements

## Summary

Rust now ships static Nix package support for `flake.nix`, `flake.lock`, and a bounded `default.nix` `mkDerivation` surface even though the Python ScanCode reference still has no production Nix packagedcode parser.
The supported surface focuses on the highest-value official and commonly used Nix repository metadata: flake identity, pinned flake dependency state, and literal derivation metadata that can be recovered safely without evaluation.

## Python Status

- Python ScanCode does not currently ship a production Nix packagedcode parser.
- Python ScanCode does not currently ship packagedcode support for these Nix surfaces beyond general file recognition.
- This gives Rust direct packagedcode support for flake and bounded derivation metadata that the Python reference does not currently provide.

## Rust Improvements

### Static `flake.nix` metadata extraction

- Rust now recognizes `flake.nix` and extracts literal top-level `description` plus direct `inputs` metadata.
- Literal URL-style flake inputs are emitted as direct Nix dependencies, and `inputs.*.follows` relationships are preserved as dependency metadata instead of being guessed away.
- Shallow `let ... in` bindings and direct alias references are resolved when they stay within local literal values, so `description = descriptionText;` and `inputs.nixpkgs.url = nixpkgsUrl;` can still be recovered statically.
- Root packages fall back to the containing directory name when the flake does not declare a literal package identity of its own, producing stable Nix package identities such as `pkg:nix/flake-demo`.

### Official `flake.lock` root-input support

- Rust now parses `flake.lock` as strict JSON and extracts the pinned root-input view from `root` plus `nodes`.
- Locked flake inputs preserve pinned revisions, fetcher metadata, and non-flake markers, while emitted dependency PURLs use the root input names users see in the repository.
- This gives scans a deterministic, evaluation-free view of pinned flake dependencies such as `pkg:nix/flake-utils@def456`.

### Bounded `default.nix` `mkDerivation` support

- Rust now recognizes `default.nix` files containing a direct `mkDerivation` call and extracts literal `pname`, `name`, `version`, `homepage`, `meta.description`, and `meta.license` values.
- Literal dependency lists from `nativeBuildInputs`, `buildInputs`, `propagatedBuildInputs`, and `checkInputs` are emitted as Nix dependencies with preserved native-vs-runtime intent.
- The bounded surface now also covers shallow `let` aliasing, plain `inherit` / `inherit (expr) ...` forwarding, local `import ./file.nix { ... }` wrappers, local `callPackage ./file.nix { ... }` wrappers, and selected local attrset exports such as `(callPackage ./release.nix { }).defaultNix`.
- This slice intentionally stays narrow: it targets the most common literal derivation metadata without trying to interpret arbitrary Nix evaluation semantics.

### Flake sibling assembly

- Sibling assembly now merges `flake.nix` identity data with `flake.lock` dependency state when both files appear in the same directory.
- That keeps repository-level Nix scans from emitting separate root packages for the manifest and lockfile views of the same flake.

## Guardrails

- Rust does **not** evaluate Nix expressions, fetch remote inputs, execute `builtins`, interpret generic `shell.nix`, or attempt full derivation normalization.
- Local wrapper traversal is limited to explicit relative `.nix` paths and a shallow recursion depth; remote fetchers and computed import paths remain out of scope.
- Non-literal or unsupported constructs fall back safely with datasource identity preserved instead of being guessed.
- The bounded `default.nix` slice is intentionally restricted to direct `mkDerivation` metadata recovery and does not claim general Nix-language coverage.

## Coverage

Coverage spans `flake.nix`, `flake.lock`, bounded `default.nix` derivations, parser regression fixtures, and flake sibling assembly wiring.

## References

- [Nix flake reference](https://nix.dev/manual/nix/latest/command-ref/new-cli/nix3-flake.html)
- [Nix flake concepts](https://nix.dev/concepts/flakes.html)
- [Nix derivation language reference](https://nix.dev/manual/nix/latest/language/derivations.html)
