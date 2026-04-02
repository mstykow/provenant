# Structured Metadata Parsers: `CITATION.cff` and `publiccode.yml`

## Summary

Provenant now parses two structured project-metadata surfaces that were previously only tracked in planning docs:

- `CITATION.cff`
- `publiccode.yml` / `publiccode.yaml`

These files are lower-value than dependency manifests and lockfiles, but they still carry useful package-adjacent identity, provenance, and license metadata that users expect a package scan to preserve.

## Python Reference Status

The Python reference does not currently ship these parsers in released behavior. Upstream work exists as open PRs for both `CITATION.cff` and `publiccode.yml`, which made them good candidates for bounded Rust-first implementation.

## Rust Improvements

### 1. Parse `CITATION.cff` as bounded generic metadata

Rust now recognizes `CITATION.cff`, requires a `cff-version`, and extracts:

- `title` → `name`
- `version` → `version`
- `abstract` / `message` → `description`
- `url` → `homepage_url`
- `repository-code` → `vcs_url`
- `license` → raw + normalized declared-license fields when SPDX-compatible
- `authors` → `parties`

Malformed YAML or files missing `cff-version` degrade gracefully to an identified parser row instead of panicking.

### 2. Parse `publiccode.yml` as bounded public-service metadata

Rust now recognizes `publiccode.yml` and `publiccode.yaml`, requires `publiccodeYmlVersion`, and extracts:

- localized `name`
- `softwareVersion` → `version`
- `url` → `vcs_url`
- `landingURL` → `homepage_url`
- localized `longDescription` / `shortDescription` → `description`
- `legal.license` → raw + normalized declared-license fields when SPDX-compatible
- `legal.mainCopyrightOwner` / `repoOwner` → `copyright`
- `maintenance.contacts` → maintainer parties

Files that fail schema-minimum checks remain identified but empty rather than crashing the scan.

## Primary Areas Affected

- generic project metadata extraction
- public-sector metadata extraction
- standalone datasource coverage for non-assembled metadata files

## Coverage

This enhancement set is covered by:

- parser unit tests for `CITATION.cff`
- parser unit tests for `publiccode.yml`
- parser goldens for both formats
- datasource accounting tests via assembly classification
