# Compiled Binary Parser Improvements

## Summary

Rust now goes beyond the default Python ScanCode compiled-binary handling in several concrete ways:

1. ships Go build-info and Rust cargo-auditable compiled-binary extraction in core rather than relying on optional external inspector plugins
2. emits native `PackageData` rows for supported compiled binaries with first-class `GoBinary` and `RustBinary` datasource IDs
3. recovers Rust dependency edges directly from cargo-auditable `.dep-v0` metadata instead of limiting output to top-level package identity
4. hardens Rust audit decoding with bounded decompression while keeping compiled-binary extraction on the normal scanner path

## Reference limitation

The Python reference exposes compiled-binary package collection through optional `go_inspector` and
`rust_inspector` integrations rather than built-in packagedcode handlers. That means the default
core packagedcode path does not guarantee Go build-info or cargo-auditable extraction unless those
extra plugins are installed and available.

## Rust Improvements

### Native compiled-binary support in core scanning

- Provenant ships scanner-gated compiled-binary extraction for:
  - Go binaries with embedded build info
  - Rust binaries with cargo-auditable `.dep-v0` sections
- This support is part of the normal Provenant binary and ScanCode-compatible scan path behind
  `--package-in-compiled`; it does not depend on optional external inspector packages.

### Rust dependency graph recovery from cargo-auditable metadata

- Rust compiled binaries now recover the package set embedded in cargo-auditable metadata.
- Each recovered package keeps Cargo identity fields such as:
  - `type: cargo`
  - `datasource_id: rust_binary`
  - Cargo PURLs
- Embedded dependency indices are converted into real dependency edges with pinned versions and
  build-vs-runtime intent.

### Go module recovery from embedded build info

- Go compiled binaries now recover package/module identity directly from embedded build-info data.
- The extracted packages use the dedicated `go_binary` datasource, Go PURLs, and package homepage
  URLs derived from the embedded module path.

### Bounded compiled-binary decoding

- Rust cargo-auditable payload decoding is now bounded before full decompression completes.
- This keeps malformed or hostile oversized `.dep-v0` sections from forcing unbounded in-memory
  inflation while preserving the normal "no package data recovered" fallback for invalid inputs.

## Why this matters

- **Better default artifact coverage**: compiled Go and Rust binaries produce package data out of
  the box, not only when optional inspector plugins happen to be installed
- **Better Rust dependency visibility**: cargo-auditable metadata becomes a usable dependency graph,
  not just a thin binary identity signal
- **Safer binary parsing**: compiled-binary extraction stays bounded even when embedded audit data is
  malformed or adversarial
