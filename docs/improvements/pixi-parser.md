# Pixi Parser Improvements

## Summary

Rust now ships static Pixi workspace support for `pixi.toml` and `pixi.lock` even though the Python ScanCode reference still has no production Pixi parser.
The supported surface focuses on the highest-value official Pixi metadata: workspace identity, direct Conda/PyPI dependencies, feature/environment metadata, and version-gated lockfile dependency state.

## Python Status

- Python ScanCode does not currently ship a Pixi packagedcode parser.
- Upstream interest exists, but there is no packagedcode implementation or test suite to port directly.
- This gives Rust direct packagedcode support for Pixi workspace metadata that the Python reference does not currently provide.

## Rust Improvements

### Static `pixi.toml` workspace metadata extraction

- Rust now recognizes `pixi.toml` and extracts workspace identity from `[workspace]` and `[project]`.
- The parser preserves `name`, `version`, `authors`, `description`, `license`, `homepage`, `repository`, `documentation`, `channels`, `platforms`, `requires-pixi`, and `exclude-newer`.
- Root packages are emitted with Pixi package identities such as `pkg:pixi/pixi-demo@1.2.3`.

### Mixed Conda and PyPI dependency extraction

- Rust now extracts top-level Conda dependencies from `[dependencies]`.
- It also extracts top-level and feature-scoped PyPI dependencies from `[pypi-dependencies]`.
- Feature-level dependencies are preserved as optional scoped dependencies, and non-version PyPI sources like local editable paths stay unpinned instead of being misrepresented as versioned package requirements.

### Version-gated `pixi.lock` support

- Rust now parses current `version = 6` Pixi lockfiles and a bounded legacy `version = 4` shape.
- It preserves lock environment metadata, channels, indexes, and package-reference placement from the lockfile.
- Locked Conda and PyPI packages are emitted as pinned dependencies with preserved source and checksum metadata.

### Topology-planned root assembly

- Topology planning now claims `pixi.toml` root directories before the generic directory loop runs, then reuses the existing Pixi sibling assembler to merge `pixi.toml` identity data with `pixi.lock` dependency state.
- The assembled package keeps both the direct dependency view from the manifest and the pinned lockfile view, while preserving manifest `environments` metadata separately from richer lock-environment/package-placement metadata.

## Guardrails

- Rust does **not** execute tasks, resolve feature/environment inheritance dynamically, run the Pixi solver, or fetch channels/indexes over the network.
- Unsupported or newer lockfile versions fall back safely with datasource metadata preserved instead of being guessed.
- This supported surface does not parse `pyproject.toml` Pixi embedding; it is intentionally focused on native `pixi.toml` plus `pixi.lock`.

## Coverage

Coverage spans `pixi.toml`, supported `pixi.lock` variants, mixed Conda and PyPI dependency extraction, and topology-planned root assembly behavior.

## References

- [Pixi manifest reference](https://pixi.sh/latest/reference/pixi_manifest/)
- [Pixi lockfile overview](https://pixi.sh/latest/workspace/lockfile/)
- [Pixi Conda and PyPI concepts](https://pixi.sh/latest/concepts/conda_pypi/)
