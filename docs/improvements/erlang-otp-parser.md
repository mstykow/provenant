# Erlang / OTP Parser Improvements

## Summary

Rust now ships static Erlang/OTP package metadata support for `*.app.src` application resource
files, `rebar.config` build configuration, and `rebar.lock` lockfiles. Python ScanCode does not
currently provide a production Erlang/OTP parser.

## Rust Improvements

### Application resource file coverage (`*.app.src`)

- Rust parses OTP application resource files using a native Erlang term parser.
- Extracts package identity from the `{application, Name, Props}` tuple, including `vsn`,
  `description`, `licenses`, and `links` fields.
- Filters OTP standard library applications (`kernel`, `stdlib`, `sasl`, `crypto`, etc.) from the
  `applications` dependency list so only third-party dependencies appear in parser output.
- Handles `runtime_dependencies` entries with embedded version requirements (e.g., `"cowboy-2.10.0"`).
- Skips template version strings like `"%VSN%"` that are replaced at build time.
- Extracts `maintainers` and `keywords` metadata when present.

### Rebar3 configuration coverage (`rebar.config`)

- Rust parses `rebar.config` files and extracts dependencies from the `deps` field.
- Supports Hex package dependencies (`{Name, Version}`), git dependencies with tag/branch/ref
  references, and version-constrained git dependencies (`{Name, Version, {git, URL, Ref}}`).
- Extracts profile-scoped dependencies from the `profiles` field (e.g., test dependencies).
- Preserves git source URLs in dependency `extra_data` for provenance tracking.

### Rebar3 lockfile coverage (`rebar.lock`)

- Rust parses both v1 (flat list) and v2 (`{"1.2.0", [deps]}`) rebar.lock formats.
- Extracts resolved package versions and git commit references as pinned dependencies.
- Resolves SHA256 checksums from the `pkg_hash` section into `resolved_package` metadata.
- Produces `ResolvedPackage` entries with Hex registry homepage and API URLs.

### Sibling assembly

- `rebar.config` and `rebar.lock` participate in sibling merge assembly so manifest and lockfile
  data combine into one logical package when both files are present.
- `*.app.src` files remain standalone since they describe individual OTP applications rather than
  project-level build configuration.

## Guardrails

- Rust does **not** evaluate Erlang expressions, resolve variables, or execute rebar3 plugins.
- Conditional dependency wrappers like `{if_var_true, ...}` are skipped rather than guessed at.
- The Erlang term parser handles atoms, strings, binaries (`<<"...">>`), tuples, lists, integers,
  floats, and Erlang-style `%` comments but does not attempt full Erlang syntax coverage.
