# Provenant

A Rust rewrite of [ScanCode Toolkit](https://github.com/aboutcode-org/scancode-toolkit) for scanning codebases for licenses, package metadata, file metadata, and related provenance data.

## Overview

`Provenant` is built as a ScanCode-compatible alternative with a strong focus on correctness, feature parity, and safe static parsing.

Today the repository covers high-level scanning workflows for:

- License detection and ScanCode-style license-result output, with the completed rollout recorded in [LICENSE_DETECTION_PLAN.md](docs/implementation-plans/text-detection/LICENSE_DETECTION_PLAN.md)
- Package and dependency metadata extraction across many ecosystems
- Package assembly for related manifests and lockfiles
- File metadata and scan environment metadata
- Optional copyright, holder, and author detection
- Optional email and URL extraction
- Multiple output formats, including ScanCode-style JSON, YAML, SPDX, CycloneDX, HTML, and custom templates

For architecture, supported formats, testing, and contributor guidance, start with the [Documentation Index](docs/DOCUMENTATION_INDEX.md).

## Features

- Single, self-contained binary
- Parallel scanning with native concurrency
- ScanCode-compatible JSON output and broad output-format support
- Broad package-manifest and lockfile coverage across many ecosystems
- Package assembly for sibling, nested, and workspace-style inputs
- Include and exclude filtering, path normalization, and scan-result filtering
- Persistent scan-cache controls for repeated runs
- Security-first parsing with explicit safeguards and compatibility-focused tradeoffs where needed

## Installation

### From Crates.io

Install the Provenant package from crates.io under the crate name `provenant-cli`:

```sh
cargo install provenant-cli
```

This installs the `provenant` binary.

### Download Precompiled Binary

Download the release archive for your platform from the [GitHub Releases](https://github.com/mstykow/provenant/releases) page.

Extract the archive and place the binary somewhere on your `PATH`.

On Linux and macOS:

```sh
tar xzf provenant-*.tar.gz
sudo mv provenant /usr/local/bin/
```

On Windows, extract the `.zip` release and add `provenant.exe` to your `PATH`.

### Build from Source

For a normal source build, you only need the Rust toolchain:

```sh
git clone https://github.com/mstykow/provenant.git
cd provenant
cargo build --release
```

Cargo places the compiled binary under `target/release/`.

> **Note**: The binary includes a built-in compact license index. The `reference/scancode-toolkit/` submodule is only needed for developers updating the embedded license data, working with helper scripts that depend on it, or using custom license rules.

## Usage

```sh
provenant --json-pp <FILE> [OPTIONS] <INPUT>...
```

At least one output option is required.

For the complete CLI surface, run:

```sh
provenant --help
```

Commonly used options include:

- `--json`, `--json-pp`, `--json-lines`, `--yaml`, `--html`, `--csv`, `--debian`
- `--spdx-tv`, `--spdx-rdf`, `--cyclonedx`, `--cyclonedx-xml`
- `--custom-output`, `--custom-template`
- `--exclude/--ignore`, `--include`, `--max-depth`, `--processes`, `--info`
- `--cache`, `--cache-dir`, `--cache-clear`, `--from-json`, `--no-assemble`
- `--max-in-memory`, `--package`, `--system-package`, `--package-in-compiled`, `--package-only`
- `--filter-clues`, `--only-findings`, `--mark-source`
- `--license`, `--license-policy`, `--copyright`, `--email`, `--url`
- `--classify`, `--summary`, `--license-clarity-score`, `--tallies`
- `--tallies-key-files`, `--tallies-with-details`, `--facet`, `--tallies-by-facet`, `--generated`

### Example

```sh
provenant --json-pp scan-results.json ~/projects/my-codebase --ignore "*.git*" --ignore "target/*" --ignore "node_modules/*"
```

Use `-` as `FILE` to write an output stream to stdout, for example `--json-pp -`.
Multiple output flags can be used in a single run, matching ScanCode CLI behavior.
When using `--from-json`, you can pass multiple JSON inputs. Native directory scans also support multiple input paths, matching ScanCode's common-prefix behavior.
Persistent caching is opt-in. Use `--cache scan-results` to enable the scan-result cache. The
cache root can also be controlled with the `PROVENANT_CACHE` environment variable or
`--cache-dir`.

For the generated package-format support matrix, see [Supported Formats](docs/SUPPORTED_FORMATS.md).

## Performance

`Provenant` is designed for efficient native scanning and parallel processing. See [Architecture: Performance Characteristics](docs/ARCHITECTURE.md#performance-characteristics) for implementation details.

## Output Formats

Implemented output formats include:

- JSON, including ScanCode-compatible output
- YAML
- JSON Lines
- CSV
- Debian copyright
- SPDX, Tag-Value and RDF/XML
- CycloneDX, JSON and XML
- HTML report
- Custom template rendering

Output architecture and compatibility approach are documented in:

- [Architecture](docs/ARCHITECTURE.md)
- [Testing Strategy](docs/TESTING_STRATEGY.md)

## Documentation

- **[Documentation Index](docs/DOCUMENTATION_INDEX.md)** - Best starting point for navigating the docs set
- **[Architecture](docs/ARCHITECTURE.md)** - System design, processing pipeline, and design decisions
- **[Supported Formats](docs/SUPPORTED_FORMATS.md)** - Generated support matrix for package ecosystems and file formats
- **[How to Add a Parser](docs/HOW_TO_ADD_A_PARSER.md)** - Step-by-step guide for adding new parsers
- **[Testing Strategy](docs/TESTING_STRATEGY.md)** - Testing approach and guidelines
- **[ADRs](docs/adr/)** - Architectural decision records
- **[Beyond-Parity Improvements](docs/improvements/)** - Features where Rust exceeds the Python original

## Contributing

Contributions are welcome. Please feel free to submit a pull request.

For contributor guidance, start with the [Documentation Index](docs/DOCUMENTATION_INDEX.md), [How to Add a Parser](docs/HOW_TO_ADD_A_PARSER.md), and [Testing Strategy](docs/TESTING_STRATEGY.md).

A typical local setup on Linux, macOS, or WSL is:

```sh
git clone https://github.com/mstykow/provenant.git
cd provenant
npm install
./setup.sh
cargo build
cargo test
```

The embedded license index is checked into the repository directly. Most contributors only need:

```sh
./setup.sh
```

Use the generator only when intentionally refreshing embedded license data:

```sh
cargo run --manifest-path xtask/Cargo.toml --bin generate-index-artifact
```

If you use the repository's documentation and hook tooling, install the versions required by `package.json` and the project's `lefthook.yml` configuration. `npm install` fetches the pinned hook/docs tooling and installs hooks via the package `prepare` script; `npm run hooks:install` is available if you need to re-install them manually. Those setup and helper commands are currently shell-oriented, so Windows contributors should prefer running them inside WSL2.

## Credits

`Provenant` is an independent Rust rewrite of [ScanCode Toolkit](https://github.com/aboutcode-org/scancode-toolkit). It uses the upstream ScanCode Toolkit project by nexB Inc. and the AboutCode community as a reference for compatibility, behavior, and parity validation. We are grateful to nexB Inc. and the AboutCode community for the reference implementation and the extensive license and copyright research behind it. See [`NOTICE`](NOTICE) for preserved upstream attribution notices applicable to materials included in this repository and to distributions that include ScanCode-derived data.

## License

Copyright (c) 2026 Provenant contributors.

The Provenant project code is licensed under the [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0). See [`NOTICE`](NOTICE) for preserved upstream attribution notices for included ScanCode Toolkit materials.
