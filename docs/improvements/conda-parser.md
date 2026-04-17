# Conda Parser Improvements

## Summary

Rust now goes beyond the current Python ScanCode Conda handling in several concrete ways:

1. assembles `conda-meta/*.json` installed-package metadata together with sibling recipe `meta.yaml` data so installed files are assigned back to the Conda package
2. resolves channel-prefix ambiguity by keeping symbolic channel names as PURL namespace while preserving URL-like channel prefixes separately as `channel_url`
3. treats top-level `environment.yml` dependency strings as Conda requirements by default, reserving PyPI classification for the explicit nested `pip:` subsection
4. keeps Conda environment filename aliases consistent across parser matching, assembly, and supported-format metadata
5. parses modern conda-forge `recipe/recipe.yaml` feedstocks with bounded `schema_version: 1` context substitution and sibling assembly support

## Python Status

- Current Python ScanCode already has dedicated `conda-meta/*.json` handling and rootfs assembly logic to assign installed files from `files`/`extracted_package_dir`.
- Upstream still tracks two Conda gaps relevant here:
  - installed file assignment from `conda-meta/*.json`
  - ambiguity between symbolic channel namespace and URL-like channel prefixes
- Conda environment specifications define plain `dependencies:` strings as Conda requirements, while subsection dictionaries like `pip:` are reserved for non-Conda installers.

## Rust Improvements

### Installed file assignment from `conda-meta`

- Rust now assembles `conda-meta/*.json` with matching `pkgs/.../info/recipe/meta.yaml` recipe data using shared package identity.
- `conda-meta` parsing now emits `file_references` for:
  - installed file paths from `files[]`
  - extracted package directory under `pkgs/...`
  - package tarball path when present
- The generic file-reference resolver then assigns those installed files to the assembled Conda package.

### Channel namespace vs repository URL disambiguation

- Symbolic channel prefixes like `conda-forge::numpy` continue to become Conda PURL namespace.
- URL-like channel prefixes such as `https://...::flask=1.0.2` are no longer treated as namespace.
- Instead, URL-like prefixes are preserved in dependency `extra_data.channel_url`, while symbolic prefixes are preserved in `extra_data.channel`.

### Environment dependency classification

- Top-level string entries under `dependencies:` in `environment.yml` / `conda.yaml` are now treated as Conda package specs by default.
- Only entries explicitly nested under a `pip:` subsection are classified as PyPI dependencies.
- This avoids misclassifying ordinary Conda environment entries such as `numpy`, `pandas>=2`, or `magma-cuda101` as `pkg:pypi/...` simply because they are also valid Python-style requirement strings.

### Environment filename alias parity

- Rust now keeps Conda environment aliases aligned across parser matching, assembly, and generated supported-format docs.
- That includes `.yml` / `.yaml` forms and hyphenated alias names such as `conda-env.yaml`, rather than recognizing them in parser-only code paths while leaving assembly and supported-format reporting behind.

### Modern conda-forge `recipe/recipe.yaml` feedstocks

- Rust now recognizes `recipe/recipe.yaml` and `recipe/recipe.yml` as Conda recipe inputs for current conda-forge feedstocks.
- Bounded `schema_version: 1` support resolves simple `${{ context_var }}` substitutions for package identity, source URLs, and about metadata without attempting full expression evaluation.
- Static requirement extraction walks `build`, `host`, `run`, and conditional `if` / `then` blocks, while intentionally skipping dynamic helper expressions such as `${{ compiler('c') }}` that require build-context evaluation.
- Sibling assembly now hoists those feedstock recipe files into top-level Conda packages instead of leaving them as file-level manifest-only records.

## Coverage

Coverage spans installed-package file assignment, `conda-meta` parsing, top-level environment dependency classification, environment filename alias parity, modern conda-forge `recipe/recipe.yaml` feedstocks, and the distinction between symbolic channel names and URL-like channel prefixes.
