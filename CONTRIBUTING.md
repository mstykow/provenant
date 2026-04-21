# Contributing to Provenant

Thanks for contributing to Provenant.

This document is the main entry point for contributor workflow. Keep `README.md` focused on project overview and user-facing setup, and use this file for contributor-specific guidance.

## Start here

- Read [`README.md`](README.md) for installation, usage, and the high-level project overview.
- Use [`docs/DOCUMENTATION_INDEX.md`](docs/DOCUMENTATION_INDEX.md) as the map for the rest of the docs set.
- For security issues, follow [`SECURITY.md`](SECURITY.md) instead of opening a public issue or pull request first.

## Local setup

Before running the bootstrap flow, install:

- Git
- A Rust toolchain with `cargo` available on your `PATH` (see [`rust-toolchain.toml`](rust-toolchain.toml))
- Node.js `>=24.0.0` with `npm` available on your `PATH` (see [`package.json`](package.json))

A typical local setup on Linux, macOS, or WSL is:

```sh
git clone https://github.com/mstykow/provenant.git
cd provenant
npm run setup
```

That command runs `npm install`, installs the Rust CLI helper tools used by local hooks and checks, and then runs `./setup.sh` to initialize submodules and hooks.

Useful follow-up commands:

- `./setup.sh` to re-run submodule and hook setup
- `npm run hooks:install` to re-install hooks manually
- `npm run hooks:run` to run the pre-commit hook suite on all files
- `npm run check:docs` to validate Markdown formatting and linting for documentation changes

These setup and helper commands are currently shell-oriented, so Windows contributors should prefer WSL2.

## Contributor workflow

Before making non-trivial changes, read the docs that own the part of the system you are touching:

- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for overall system design
- [`docs/LICENSE_DETECTION_ARCHITECTURE.md`](docs/LICENSE_DETECTION_ARCHITECTURE.md) for license-index and detection internals
- [`docs/TESTING_STRATEGY.md`](docs/TESTING_STRATEGY.md) for test layers and command guidance
- [`docs/HOW_TO_ADD_A_PARSER.md`](docs/HOW_TO_ADD_A_PARSER.md) for parser-specific implementation rules
- [`xtask/README.md`](xtask/README.md) for maintainer commands such as compare runs, golden maintenance, benchmarks, and generated artifacts

Repo-specific expectations:

- Preserve behavior and parity, especially when using the ScanCode reference as a behavioral spec.
- Keep parsing static and bounded. Do not execute package-manager code, project code, or shell commands to recover metadata.
- Use `cargo add`, `cargo remove`, and targeted `cargo update` instead of editing Rust dependencies by hand.
- Do not add dependencies lightly; make sure they clearly justify their maintenance cost.
- Keep PR scope disciplined. For ecosystem or parser work, prefer one ecosystem family per PR.

## Inbound licensing policy

Provenant uses the Developer Certificate of Origin (DCO) 1.1 for inbound
contributions. The DCO text is stored in [`DCO`](DCO).

Unless a path says otherwise, contributions are accepted under the same license
terms that already apply to the material you are changing in this repository.
For the main project code that is Apache-2.0. Third-party and reference
material kept in-tree continues to use its existing notices and licenses.

Every commit you author must include a `Signed-off-by:` trailer that matches
your commit author identity. The easiest way to do that is to use:

```sh
git commit -s
```

If you forgot to sign off the latest commit, fix it with:

```sh
git commit --amend -s --no-edit
```

If you rewrite or squash commits before merge, make sure the resulting commits
still carry the sign-off. The local git hook can catch missing sign-offs early,
and the GitHub DCO app enforces PR-level compliance.

## Testing and validation

Keep local validation tightly scoped. This repository has many slow and specialized tests, so the default is the smallest command that proves your change.

Prefer:

- `cargo test --doc` for doctests
- `cargo test --test <suite_name>` for a top-level integration suite
- `cargo test --lib <filter>` for a focused library/parser target
- `cargo test --features golden-tests <filter>` only when the change directly affects golden-covered behavior

Avoid broad local commands such as `cargo test`, `cargo test --all`, `cargo test --lib`, or unfiltered golden suites unless there is no narrower way to validate the change.

Important testing rules:

- Prefer exact test paths or narrowly owned suites over broad substring filters.
- Do not update golden expected files just to make a failing test pass; fix the implementation unless the output change is intentional and documented.
- If you touch fixture-maintenance workflows or generated docs, use the canonical command reference in [`xtask/README.md`](xtask/README.md).
- All checks must pass before merging, even if CI is the place that runs the full matrix.

## Parser and ecosystem work

For package parser work, treat [`docs/HOW_TO_ADD_A_PARSER.md`](docs/HOW_TO_ADD_A_PARSER.md) as the canonical guide.

That guide covers parser invariants, registration, datasource wiring, assembly integration, test expectations, and validation against the Python reference or an authoritative format spec. It also links back to the project-wide setup and testing docs instead of duplicating them.

## Pull requests

- Write commit messages in Conventional Commits format: `type(scope): short summary` when a scope helps, or `type: short summary` otherwise.
- Use the same Conventional Commits format for pull request titles.
- Sign off every commit with `git commit -s` so the branch satisfies the DCO policy.
- Follow the structure in [`.github/pull_request_template.md`](.github/pull_request_template.md) and omit sections that do not apply.
- Keep summaries focused on why the change exists, not just what changed.

## Documentation and maintenance notes

- Keep evergreen contributor and architecture docs under [`docs/`](docs/).
- Do not edit generated files such as [`docs/SUPPORTED_FORMATS.md`](docs/SUPPORTED_FORMATS.md) by hand; use the owning generation command.
- For release, benchmark, compare-output, and artifact-generation workflows, use [`xtask/README.md`](xtask/README.md).

## Questions and security

If you are unsure which document owns a topic, start with [`docs/DOCUMENTATION_INDEX.md`](docs/DOCUMENTATION_INDEX.md).

If you believe you found a security issue, follow [`SECURITY.md`](SECURITY.md) and avoid public disclosure first.
