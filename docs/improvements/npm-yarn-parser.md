# npm and Yarn Parser: Local Metadata, Lockfile, and Package-Root Improvements

## Summary

Provenant improves several npm and Yarn behaviors that are missing, incomplete, or buggy in the Python reference:

- `package.json` now preserves npm `overrides` metadata.
- `package.json` now preserves platform and compatibility metadata such as `os`, `cpu`, `libc`, `deprecated`, and `hasBin`.
- malformed or unreadable `package.json` files now still emit an identified npm parser row instead of disappearing into anonymous fallback output.
- Empty npm `name` and `version` values no longer generate synthetic registry URLs or dummy PURLs.
- Scoped npm fallback URLs now use the correct registry path and tarball filename shape, and invalid `homepage`/blank `bugs` metadata is dropped instead of being kept as noisy values.
- `package-lock.json` root package identity now falls back to the `packages[""]` entry, matching modern hidden-lockfile layouts.
- npm lockfiles now preserve link-style and non-version dependency specs without incorrectly treating them as pinned registry versions.
- `yarn.lock` can infer direct dependency scope from a sibling `package.json`.
- Yarn Berry lockfiles now preserve raw non-`@npm:` resolution references plus lock metadata such as `resolution`, `languageName`, `linkType`, `bin`, and `dependenciesMeta`.
- `.pnp.cjs` can now contribute npm-style dependency metadata instead of being treated as package-root ownership only.
- npm package-root resource assignment now skips first-level `node_modules` while allowing nested bundled packages to own their own files, including `.pnp.cjs`-style project files at the package root.
- Workspace assembly now accepts array, string, and object-style npm workspace declarations, preserves unattached lockfile dependencies when a sibling manifest is not packageable, and emits deterministic package/file ordering.

## Python Reference Status

Relevant reference signals from the upstream ScanCode npm/Yarn packagedcode implementation:

- Python still carries TODO notes around newer npm/Yarn lockfile support and `pnp.js` / pnpm lockfile handling.
- Python's npm assembly relies on `walk_npm()` to assign resources while skipping first-level `node_modules`.
- Python updates Yarn/package-lock dependency metadata from nearby manifests during assembly/resolution.
- Python still carries a TODO around `package.json` platform metadata such as `os`, and its Berry handling focuses on `@npm:` locators without preserving richer non-registry resolution detail.

## Rust Improvements

### 1. Preserve npm `overrides`

Rust now stores the raw `overrides` mapping in `PackageData.extra_data`, so override intent is preserved for downstream tooling and future dependency analysis.

### 2. Avoid dummy URLs for empty npm metadata

If `name` or `version` is empty or whitespace, Rust now treats it as missing metadata instead of building malformed registry URLs such as placeholder tarball or API paths.

Rust also now uses the correct scoped npm tarball shape (`/@scope/name/-/name-version.tgz`) for fallback URLs and drops invalid `homepage` arrays / blank `bugs.url` values instead of preserving misleading metadata.

### 3. Preserve npm platform and compatibility metadata

Rust now keeps raw `package.json` compatibility metadata in `PackageData.extra_data` for:

- `os`
- `cpu`
- `libc`
- `deprecated`
- `hasBin`

These fields are useful even when dependency extraction is unchanged because they describe installation eligibility, host compatibility, and package lifecycle state that downstream tooling may need to inspect.

### 4. Preserve parser identity on malformed `package.json`

When `package.json` cannot be read or parsed, Rust now still emits an empty parser row tagged as npm `package.json` data instead of returning an anonymous package record with no parser identity.

This is intentionally more informative than the Python reference for hard parse failures: downstream output can still show that a package manifest was recognized, which parser handled it, and which datasource produced the warning.

### 5. Modern npm lockfile root fallback

For npm v2/v3 lockfiles, Rust now uses `packages[""]` as a fallback source for root `name` and `version`, which is important for hidden lockfiles and other modern layouts where top-level fields may be absent.

### 6. Preserve non-version npm lockfile dependencies

Rust now keeps `link: true` dependencies from modern lockfiles as dependency records with unversioned npm PURLs and source-path metadata in `extra_data`, instead of silently dropping them.

Rust also preserves non-version lockfile specs such as `file:`, `git+...`, tarball URLs, and `npm:` aliases as unpinned dependency requirements. This avoids emitting invalid versioned PURLs for source-based dependencies and prevents alias ranges like `npm:wrap-ansi@^7.0.0` from being misclassified as exact pinned versions.

### 7. Correct directness, preserve Berry resolution metadata, and infer Yarn scope from sibling `package.json`

When a `yarn.lock` sits next to a `package.json`, Rust now uses the manifest to classify direct dependencies as:

- `dependencies`
- `devDependencies`
- `optionalDependencies`
- `peerDependencies`

This improves direct/transitive classification and brings lockfile output closer to the manifest semantics users expect.

For npm lockfiles, Rust also now marks nested duplicate packages under `node_modules/.../node_modules/...` as transitive instead of incorrectly counting them as direct dependencies just because their package name also appears at the root.

For Yarn Berry lockfiles, Rust now also preserves richer resolver detail instead of collapsing non-`@npm:` locators to `*`. Raw lock metadata such as `resolution`, `languageName`, `linkType`, `bin`, `dependenciesMeta`, and lockfile `cacheKey` are kept in structured metadata, while protocol-backed locators such as `patch:` and `workspace:` keep their full reference in the resolved dependency requirement.

### 8. Correct nested npm package ownership

Rust now assigns package-root resources for npm packages while skipping first-level `node_modules` for the parent package. Nested packages under `node_modules` therefore keep their own package identity instead of inheriting the parent package UID.

This directly improves bundled-package scans and also ensures root-level Yarn PnP files such as `.pnp.cjs` attach to the correct package.

### 9. Preserve workspace and unattached lockfile semantics

Workspace assembly now accepts npm workspace declarations from array, string, or `{ "packages": [...] }` forms, which prevents valid monorepos from being skipped during assembly.

When a sibling `package.json` exists but is not packageable, Rust now keeps lockfile dependencies as unattached top-level dependencies instead of incorrectly manufacturing a package from lockfile-only identity.

The assembly phase also sorts `datafile_paths`, `datasource_ids`, and `for_packages`, so workspace and sibling-merge output stays deterministic across runs.

### 10. Parse Yarn Plug-and-Play `.pnp.cjs` runtime state

Rust now treats root-level `.pnp.cjs` files as a bounded npm-family dependency source rather than only as a package-root resource.

The parser extracts dependency entries from the embedded `RAW_RUNTIME_STATE`, emits npm-shaped dependency records, and wires the new datasource into npm sibling assembly so `.pnp.cjs` dependencies hoist into the same top-level package as the sibling `package.json`.

## Primary Areas Affected

- npm manifest parsing and metadata normalization
- malformed npm manifest fallback identity
- npm platform/compatibility metadata preservation
- npm lockfile root identity and dependency-spec extraction
- Yarn lockfile dependency-scope inference and Berry resolution-detail preservation
- npm/Yarn `.pnp.cjs` dependency extraction
- npm/Yarn assembly behavior for sibling manifests, workspaces, deterministic ordering, and nested package ownership

## Coverage

This enhancement set is covered by:

- npm parser-focused unit tests
- malformed `package.json` fallback tests
- npm lockfile-focused unit tests
- Yarn lockfile-focused unit tests
- Yarn Plug-and-Play parser unit, golden, assembly, and scan tests
- focused npm and Yarn parser/assembly tests for the affected behaviors
