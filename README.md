# Provenant

[![CI](https://github.com/mstykow/provenant/actions/workflows/check.yml/badge.svg?branch=main)](https://github.com/mstykow/provenant/actions/workflows/check.yml)
[![Crates.io](https://img.shields.io/crates/v/provenant-cli.svg)](https://crates.io/crates/provenant-cli)
[![License](https://img.shields.io/crates/l/provenant-cli.svg)](LICENSE)

Provenant is a Rust-based code scanner for licenses, package metadata, file metadata, and related provenance data. It is built as an independent Rust implementation for ScanCode-aligned workflows, with a strong focus on correctness, feature parity, safe static parsing, and native execution.

Provenant reimplements the scanning engine in Rust while continuing to rely on the upstream [ScanCode Toolkit](https://github.com/aboutcode-org/scancode-toolkit) license and rule data. That upstream dataset is extraordinarily valuable and maintained by domain experts; Provenant's goal is to preserve and build on that work, not replace it.

## Quick Start

```sh
cargo install provenant-cli
provenant --json-pp - --license --package /path/to/repo
```

Prefer release binaries? Download precompiled archives from [GitHub Releases](https://github.com/mstykow/provenant/releases).

## Why Provenant?

- Native scanning with parallel execution and a single self-contained binary
- ScanCode-compatible workflows and output formats, including ScanCode-style JSON, SPDX, CycloneDX, YAML, JSON Lines, HTML, and custom templates
- Broad package-manifest and lockfile coverage across many ecosystems
- Security-first static parsing with explicit safeguards and compatibility-focused tradeoffs where needed
- Built on upstream ScanCode license and rule data maintained by experts

## Project Status

> **Status:** active, usable, and under rapid development.
> Provenant already supports production-style scanning workflows and many ScanCode-compatible outputs, while compatibility gaps and edge cases are still being closed.

## Overview

Today the repository covers high-level scanning workflows for:

- License detection and ScanCode-style license-result output
- Package and dependency metadata extraction across many ecosystems
- Package assembly for related manifests and lockfiles
- File metadata and scan environment metadata
- Optional copyright, holder, and author detection
- Optional email and URL extraction
- Multiple output formats, including ScanCode-style JSON, YAML, JSON Lines, SPDX, CycloneDX, HTML, Debian copyright, and custom templates

For architecture, comparison notes, supported formats, testing, and contributor guidance, start with the [Documentation Index](docs/DOCUMENTATION_INDEX.md).

## Relationship to ScanCode

- Provenant is an independent Rust implementation inspired by ScanCode Toolkit.
- It aims for strong compatibility with ScanCode workflows and output semantics where practical.
- It continues to use the upstream ScanCode license and rule data.
- Provenant does not replace the value of upstream rule curation; it provides a Rust scanning engine around that expert-maintained knowledge base.
- For a concise side-by-side overview, see [Provenant and ScanCode Toolkit](docs/SCANCODE_COMPARISON.md).

## Features

- Single, self-contained binary
- Parallel scanning with native concurrency
- ScanCode-compatible JSON output and broad output-format support
- Broad package-manifest and lockfile coverage across many ecosystems
- Package assembly for sibling, nested, and workspace-style inputs
- Include and exclude filtering, path normalization, and scan-result filtering
- Incremental reuse for repeated scans of the same tree
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

> **Note**: The binary includes a built-in compact license index. The `reference/scancode-toolkit/` submodule is only needed for developers updating the embedded license data, using maintainer commands that depend on it, or working with custom license rules.

## Usage

```sh
provenant --json-pp <FILE> [OPTIONS] <INPUT>...
```

At least one output option is required.

For the complete CLI surface, run:

```sh
provenant --help
```

For guided workflows and important flag combinations, see the [CLI Guide](docs/CLI_GUIDE.md).

### Example

```sh
provenant --json-pp scan-results.json --license --package ~/projects/my-codebase --ignore "*.git*" --ignore "target/*" --ignore "node_modules/*"
```

Use `-` as `FILE` to write an output stream to stdout, for example `--json-pp -`.
Multiple output flags can be used in a single run, matching ScanCode CLI behavior.
When using `--from-json`, you can pass multiple JSON inputs. Native directory scans also support multiple input paths, matching ScanCode's common-prefix behavior.
Use `--incremental` for repeated scans of the same tree. After a completed scan, Provenant keeps
an incremental manifest and uses it on the next run to skip unchanged files. That is useful for
local iteration, CI-style reruns, and retrying after a later failed or interrupted scan. The
incremental cache root can be controlled with `PROVENANT_CACHE` or `--cache-dir`, and `--cache-clear`
resets it before a run.

For the generated package-format support matrix, see [Supported Formats](docs/SUPPORTED_FORMATS.md).

## Performance

`Provenant` is designed for efficient native scanning and parallel processing. See [Architecture: Performance Characteristics](docs/ARCHITECTURE.md#performance-characteristics) for implementation details.

## Output Formats

Implemented output formats include:

- JSON, including ScanCode-compatible output
- YAML
- JSON Lines
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
- **[CLI Guide](docs/CLI_GUIDE.md)** - Common workflows and important flag combinations
- **[Provenant and ScanCode Toolkit](docs/SCANCODE_COMPARISON.md)** - Relationship, trust model, and high-level comparison notes
- **[Architecture](docs/ARCHITECTURE.md)** - System design, processing pipeline, and design decisions
- **[Supported Formats](docs/SUPPORTED_FORMATS.md)** - Generated support matrix for package ecosystems and file formats
- **[How to Add a Parser](docs/HOW_TO_ADD_A_PARSER.md)** - Step-by-step guide for adding new parsers
- **[Testing Strategy](docs/TESTING_STRATEGY.md)** - Testing approach and guidelines
- **[ADRs](docs/adr/)** - Architectural decision records
- **[Beyond-Parity Improvements](docs/improvements/)** - Features where Rust exceeds the Python original

## Contributing

Contributions are welcome. Please feel free to submit a pull request.

For contributor guidance, start with the [Documentation Index](docs/DOCUMENTATION_INDEX.md), [How to Add a Parser](docs/HOW_TO_ADD_A_PARSER.md), and [Testing Strategy](docs/TESTING_STRATEGY.md).

Before running the local setup flow, install these prerequisites:

- Git
- A Rust toolchain with `cargo` available on your `PATH` (see `rust-toolchain.toml`)
- Node.js with `npm` available on your `PATH` (see `package.json` `engines`)

A typical local setup on Linux, macOS, or WSL is:

```sh
git clone https://github.com/mstykow/provenant.git
cd provenant
npm run setup
```

That command runs `npm install`, installs the Rust CLI helper tools used by local hooks/checks, and then runs `./setup.sh` to initialize submodules and hooks.

The embedded license index is checked into the repository directly. If you only need to re-run submodule and hook setup after the initial bootstrap, `./setup.sh` is sufficient:

```sh
./setup.sh
```

Use the generator only when intentionally refreshing embedded license data:

```sh
cargo run --manifest-path xtask/Cargo.toml --bin generate-index-artifact
```

For normal local development, `npm run setup` is the one-command bootstrap path. `npm run hooks:install` is available if you need to re-install hooks manually. These setup and helper commands are currently shell-oriented, so Windows contributors should prefer running them inside WSL2.

## Upstream Data and Attribution

`Provenant` is an independent Rust implementation inspired by [ScanCode Toolkit](https://github.com/aboutcode-org/scancode-toolkit). It uses the upstream ScanCode Toolkit project by nexB Inc. and the AboutCode community as a reference for compatibility, behavior, and parity validation, and it continues to rely on the upstream ScanCode license and rule data maintained by that ecosystem. Provenant code is licensed under Apache-2.0; included ScanCode-derived rule and license data remains subject to upstream attribution and CC-BY-4.0 terms where applicable. We are grateful to nexB Inc. and the AboutCode community for the reference implementation and the extensive license and copyright research behind it. See [`NOTICE`](NOTICE) for preserved upstream attribution notices applicable to materials included in this repository and to distributions that include ScanCode-derived data.

## License

Copyright (c) 2026 Provenant contributors.

The Provenant project code is licensed under the [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0). See [`NOTICE`](NOTICE) for preserved upstream attribution notices for included ScanCode Toolkit materials.
