# Migrating from ScanCode Toolkit

This guide is for people who already know ScanCode Toolkit and want to understand what, if anything, changes when they move a workflow to Provenant.

For many users, the answer is: **not much**.

Provenant aims for strong CLI and output compatibility with ScanCode where practical. If you mostly run scans and consume the usual output formats, you can often start with the same broad habits and adjust only a few power-user workflows.

## Who needs this guide?

You will probably care about this document if you:

- edited ScanCode's license and rule data directly in a cloned checkout
- compare raw JSON output fields very closely between tools
- rely on historical quirks or typos in emitted values
- want to understand where Provenant intentionally differs from ScanCode

If you mostly want a ScanCode-aligned scan from a single binary, start with the [CLI Guide](CLI_GUIDE.md) instead.

## What mostly stays the same

- Provenant keeps the ScanCode-aligned scan model and output formats as its primary compatibility target.
- `spdx_license_list_version` stays in the existing ScanCode-style header location.
- `--from-json` continues to target ScanCode-style JSON inputs rather than a Provenant-only format.

For broader positioning and compatibility context, see [Provenant and ScanCode Toolkit](SCANCODE_COMPARISON.md).

## The main migration differences

### 1. Custom license data is now an export/edit/reuse workflow

With ScanCode, power users often edited the license and rule data directly in a cloned source tree.

With Provenant, the equivalent workflow is:

1. export the effective embedded dataset
2. edit the exported `.RULE` and `.LICENSE` files
3. scan with the exported dataset root

```sh
provenant --export-license-dataset /tmp/provenant-license-dataset
provenant --json-pp licenses.json --license \
  --license-dataset-path /tmp/provenant-license-dataset \
  /path/to/project
```

The dataset root uses this shape:

```text
<dataset-root>/
  manifest.json
  rules/
  licenses/
```

When `--license-dataset-path` is set, Provenant uses that dataset as authoritative input instead of the embedded dataset shipped in the binary.

See also:

- [CLI Guide](CLI_GUIDE.md)
- [License Detection Architecture](LICENSE_DETECTION_ARCHITECTURE.md)

### 2. Some historical typos are fixed in canonical output

Provenant emits corrected canonical values in a few places where ScanCode historically carried typos.

Current documented examples:

- Provenant emits `nuget_nuspec`
- ScanCode historically emitted `nuget_nupsec`
- Provenant emits `rpm_specfile`
- ScanCode historically emitted `rpm_spefile`

Important: Provenant still accepts some legacy spellings on input for compatibility, especially under `--from-json`.

So if you compare raw output, you may see corrected values even though old ScanCode JSON still loads.

### 3. Unicode names are preserved more faithfully

Provenant preserves source text and author/copyright names more faithfully in some cases.

Example:

- `François` stays `François`
- not `Francois`

This is an intentional data-quality improvement, not an incompatibility bug.

### 4. Some dependency booleans are left unset unless actually proven

ScanCode's formal schema allows nullable or omitted values for booleans like:

- `is_runtime`
- `is_optional`
- `is_pinned`
- `is_direct`

Provenant keeps these fields unset when the datasource does not actually prove them, rather than coercing output to common ScanCode defaults.

If you diff raw JSON semantically, this is one of the most important intentional differences to know.

### 5. Parser behavior may be better than ScanCode in some ecosystems

Provenant includes many documented parser fixes and beyond-parity improvements, for example in:

- NuGet
- npm/Yarn
- Gradle
- Maven
- copyright detection

These are not random incompatibilities; they are documented behavior improvements.

See [Beyond-Parity Improvements](improvements/README.md) for the full index.

### 6. Path selection is split more explicitly between patterns and exact rooted paths

If you previously relied on `--include` as a rough way to express “scan this subtree”, pay close attention to Provenant's newer split here.

- `--include` is for glob-style path filtering
- recursion should be explicit in the pattern (for example `src/**`)
- `--paths-file` is the explicit rooted workflow for “scan exactly these files or directories under this root”

That means Provenant now prefers:

- `--include '*.rs' --include 'src/**/*.toml'` when you mean pattern filtering
- `--paths-file changed-files.txt /path/to/repo` when you already know the exact rooted file or directory list

This is a workflow-level difference worth knowing when you migrate existing ScanCode habits or shell wrappers.

See also:

- [CLI Guide](CLI_GUIDE.md)
- [CLI Workflows](improvements/cli-workflows.md)

## Practical migration advice

If you are moving an existing ScanCode workflow to Provenant:

1. start with the same broad scan shape you already use
2. compare outputs on one representative codebase
3. check this guide if you see a meaningful delta
4. use the exported dataset workflow if you previously customized license/rule data in a ScanCode checkout
5. if your old workflow used `--include` to approximate explicit path lists, consider switching that part to `--paths-file`

## Other differences worth knowing

- Provenant may resolve some explicit project-root `LICENSE` references a bit differently in nested or vendored trees because it allows a bounded ancestor lookup for clear root-directory notices.
- Provenant may add additive metadata fields of its own, such as `license_index_provenance`, so strict JSON consumers should tolerate extra non-ScanCode fields.

## Related docs

- [CLI Guide](CLI_GUIDE.md)
- [Provenant and ScanCode Toolkit](SCANCODE_COMPARISON.md)
- [License Detection Architecture](LICENSE_DETECTION_ARCHITECTURE.md)
- [Beyond-Parity Improvements](improvements/README.md)
