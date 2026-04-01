# Output Format Parity Scorecard

This scorecard tracks parity against Python ScanCode output format behavior and fixtures.

Deprecated upstream CSV and HTML-app outputs are intentionally out of current Provenant scope, so they are not part of the maintained parity contract below.

## Reference Sources

- Python implementation:
  [`reference/scancode-toolkit/src/formattedcode/`](../../../reference/scancode-toolkit/src/formattedcode/)
- Python fixture corpus:
  [`reference/scancode-toolkit/tests/formattedcode/data/`](../../../reference/scancode-toolkit/tests/formattedcode/data/)
- Local (decoupled) fixture mirror:
  [`testdata/output-formats/`](../../../testdata/output-formats/)

## Format-by-Format Status

### JSON Lines

- **Reference fixture**:
  [`json/simple-expected.jsonlines`](../../../reference/scancode-toolkit/tests/formattedcode/data/json/simple-expected.jsonlines)
- **Local fixture**:
  [`testdata/output-formats/json-simple-expected.jsonlines`](../../../testdata/output-formats/json-simple-expected.jsonlines)
- **Status**: 🟢 Semantically equivalent (fixture-backed)
- **Acceptance criteria**:
  - First entry contains `headers`
  - Subsequent entries stream one file object per line
  - Deterministic ordering in emitted sections

### Debian copyright

- **Reference fixtures**:
  [`debian/basic/expected.copyright`](../../../reference/scancode-toolkit/tests/formattedcode/data/debian/basic/expected.copyright),
  [`debian/multiple_files/expected.copyright`](../../../reference/scancode-toolkit/tests/formattedcode/data/debian/multiple_files/expected.copyright)
- **Local fixture**:
  [`testdata/output-formats/debian-basic-expected.copyright`](../../../testdata/output-formats/debian-basic-expected.copyright)
- **Status**: 🟢 Semantically equivalent (fixture-backed)
- **Acceptance criteria**:
  - DEP-5 document header and notice block match the supported Provenant contract
  - File paragraphs are path-sorted and omit directory entries
  - `License:` fields prefer detected non-SPDX expressions before fallback file expressions
  - Repeated matched license text blocks are deduplicated deterministically

### SPDX Tag-Value

- **Reference fixtures**:
  [`spdx/empty/expected.tv`](../../../reference/scancode-toolkit/tests/formattedcode/data/spdx/empty/expected.tv),
  [`spdx/simple/expected.tv`](../../../reference/scancode-toolkit/tests/formattedcode/data/spdx/simple/expected.tv)
- **Local fixtures**:
  [`testdata/output-formats/spdx-empty-expected.tv`](../../../testdata/output-formats/spdx-empty-expected.tv),
  [`testdata/output-formats/spdx-simple-expected.tv`](../../../testdata/output-formats/spdx-simple-expected.tv)
- **Status**: 🟡 Partial — baseline fixture coverage exists, but license conclusion/info fields still lag the current ScanCode semantics
- **Acceptance criteria**:
  - Empty scan sentinel matches Python fixture
  - `SPDX-2.2` document baseline with stable package/file blocks
  - Deterministic package verification code and file ordering
  - Package/file license conclusions and info-from-files consume real declared/detected license data instead of placeholder `NOASSERTION` / `NONE`

### SPDX RDF

- **Reference fixture**:
  [`spdx/simple/expected.rdf`](../../../reference/scancode-toolkit/tests/formattedcode/data/spdx/simple/expected.rdf)
- **Local fixture**:
  [`testdata/output-formats/spdx-simple-expected.rdf`](../../../testdata/output-formats/spdx-simple-expected.rdf)
- **Status**: 🟡 Partial — structural baseline is covered, but emitted license semantics still lag the current ScanCode behavior
- **Acceptance criteria**:
  - Equivalent SPDX semantics after structural normalization
  - Deterministic handling of generated identifiers and timestamps
  - RDF license fields consume real declared/detected license data instead of placeholder `noassertion` / `none`

### CycloneDX JSON

- **Reference fixtures**:
  [`cyclonedx/expected-without-packages.json`](../../../reference/scancode-toolkit/tests/formattedcode/data/cyclonedx/expected-without-packages.json),
  [`cyclonedx/simple-expected.json`](../../../reference/scancode-toolkit/tests/formattedcode/data/cyclonedx/simple-expected.json)
- **Local fixtures**:
  [`testdata/output-formats/cyclonedx-expected-without-packages.json`](../../../testdata/output-formats/cyclonedx-expected-without-packages.json),
  [`testdata/output-formats/cyclonedx-expected.json`](../../../testdata/output-formats/cyclonedx-expected.json),
  [`testdata/output-formats/cyclonedx-dependencies-expected.json`](../../../testdata/output-formats/cyclonedx-dependencies-expected.json)
- **Status**: 🟢 Semantically equivalent (fixture-backed)
- **Acceptance criteria**:
  - Empty-scan output matches Python minimal fixture
  - Non-empty output keeps required CycloneDX fields and deterministic ordering

### CycloneDX XML

- **Reference fixture**:
  [`cyclonedx/expected.xml`](../../../reference/scancode-toolkit/tests/formattedcode/data/cyclonedx/expected.xml)
- **Local fixtures**:
  [`testdata/output-formats/cyclonedx-expected.xml`](../../../testdata/output-formats/cyclonedx-expected.xml),
  [`testdata/output-formats/cyclonedx-dependencies-expected.xml`](../../../testdata/output-formats/cyclonedx-dependencies-expected.xml)
- **Status**: 🟢 Semantically equivalent (fixture-backed)
- **Acceptance criteria**:
  - Core metadata/components/dependencies semantically equivalent
  - Deterministic normalization for timestamp + serial number

### YAML

- **Reference fixture**:
  [`yaml/simple-expected.yaml`](../../../reference/scancode-toolkit/tests/formattedcode/data/yaml/simple-expected.yaml)
- **Local fixture**:
  [`testdata/output-formats/yaml-simple-expected.yaml`](../../../testdata/output-formats/yaml-simple-expected.yaml)
- **Status**: 🟢 Semantically equivalent (fixture-backed)
- **Acceptance criteria**:
  - Round-trip semantic equivalence with JSON baseline
  - Stable top-level sections and field naming

### HTML (report)

- **Reference fixture**:
  [`templated/simple-expected.html`](../../../reference/scancode-toolkit/tests/formattedcode/data/templated/simple-expected.html)
- **Local fixture**:
  [`testdata/output-formats/html-templated-simple-expected.html`](../../../testdata/output-formats/html-templated-simple-expected.html)
- **Status**: 🟢 Semantically equivalent (fixture-backed)
- **Acceptance criteria**:
  - Copyright/holders/authors/emails/urls/file/package sections present
  - Deterministic structure for snapshot testing

### Custom template

- **Status**: 🟢 Implemented (template contract, not Python fixture parity)
- **Acceptance criteria**:
  - Controlled context rendering and explicit template-path handling verified
    by unit tests

## Beyond-Parity (Allowed and Encouraged)

## Verification Checklist

- `cargo test --test output_format_golden`
- `cargo test output:: --lib`
- `cargo test --features golden-tests`
- Record verification outcomes in CI logs and PR descriptions.

Improvements are encouraged if both are true:

1. Compatibility tests still pass for required parity contract.
2. Improvement is documented as intentional (spec compliance, correctness, security, or UX).

Examples:

- Stronger schema validation in CycloneDX/SPDX emitters
- Safer template rendering defaults
- Better deterministic output ordering for reproducibility
