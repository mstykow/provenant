# CLI Guide

This guide is for anyone using `provenant`, especially when choosing among common scan workflows or coming back to them later.

Use it to answer practical questions such as:

- "What should my first scan command look like?"
- "How do I scan for licenses?"
- "How do I scan for packages and dependencies?"
- "When should I use JSON, HTML, SPDX, or CycloneDX?"
- "How do I re-use an existing scan instead of rescanning?"

For the complete flag reference, always use:

```sh
provenant --help
```

This guide does **not** try to repeat every flag from `--help`. Instead, it focuses on the workflows most users actually need.

## Start Here: A Strong Default Scan

If you are starting a new scan and want a strong default, start with pretty JSON and explicitly ask for the scan types you care about:

```sh
provenant --json-pp scan.json --license --package /path/to/project
```

Why this is a good first command:

- `--json-pp scan.json` writes a readable JSON file you can inspect, diff, and feed into other tools later.
- `--license` turns on license detection. This is **opt-in**.
- `--package` turns on package and dependency detection from manifests and lockfiles. This is also **opt-in**.

What you get back:

- file-level license findings
- top-level license detections
- assembled top-level packages
- extracted dependencies from supported manifests and lockfiles

If you also want copyright, holder, and author detection, add `--copyright`:

```sh
provenant --json-pp scan.json --license --copyright --package /path/to/project
```

## Important Mental Model: Detections Are Opt-In

Like modern ScanCode, Provenant does not assume every scan should collect every kind of data.

That means you usually choose the scan dimensions you want:

| If you want to learn about...                  | Use                     | What it adds                                                   |
| ---------------------------------------------- | ----------------------- | -------------------------------------------------------------- |
| Licenses in files                              | `--license`             | license detections, expressions, and optional diagnostics/text |
| Package manifests and lockfiles                | `--package`             | top-level packages and dependencies                            |
| Installed system package databases             | `--system-package`      | package data from RPM, dpkg, apk, and similar sources          |
| Embedded package metadata in compiled binaries | `--package-in-compiled` | package data from supported Go and Rust binaries               |
| Copyrights, holders, and authors               | `--copyright`           | copyright statements, holders, and authors                     |
| File metadata such as checksums and type hints | `--info`                | extra file metadata and source/script hints                    |
| Emails or URLs                                 | `--email`, `--url`      | extracted email addresses or URLs                              |

This is the main reason the workflow guide matters: the right command depends on what question you are trying to answer.

## Choose an Output Format First

Every run needs at least one output flag, and you can request more than one in the same run.

For most users, the best default is still pretty JSON:

```sh
provenant --json-pp scan.json --license --package /path/to/project
```

Use other outputs when you need a specific consumer or review format:

- `--json` for compact machine-readable output
- `--json-pp` for human inspection and debugging
- `--json-lines` for streaming-oriented pipelines
- `--yaml` for a human-readable structured format outside JSON
- `--html` for a browsable report
- `--spdx-tv`, `--spdx-rdf`, `--cyclonedx`, `--cyclonedx-xml` for downstream compliance or SBOM workflows
- `--debian` for a machine-readable Debian copyright file
- `--custom-output` with `--custom-template` for custom report generation

You can write more than one output format in the same run. For example:

```sh
provenant --json-pp scan.json --html report.html --license --package /path/to/project
```

That is useful when you want one machine-readable result for automation and one human-readable report for review.

You can also write to stdout by using `-` as the output file:

```sh
provenant --json-pp - --license /path/to/project
```

That is useful when you want to inspect a quick result in the terminal or pipe it to another command.

## Common Workflows

The examples below are organized by the question a user is trying to answer.

### 1. "I want a good first inventory of this codebase"

```sh
provenant --json-pp scan.json --license --copyright --package /path/to/project
```

Use this when you want a broad provenance-oriented view of a repository.

Why it is useful:

- `--license` finds detected license expressions and file-level matches.
- `--copyright` adds copyright statements, holders, and authors.
- `--package` finds manifests/lockfiles and assembles top-level packages and dependencies.

This is the best place to start if you are doing general review or compliance triage.

### 2. "I only care about licenses"

```sh
provenant --json-pp licenses.json --license /path/to/project
```

Use this when your main question is "what licenses were detected in this tree?"

This is especially useful for:

- quick license triage
- comparing license-detection changes between runs
- collecting top-level license results without package-focused noise

If you need to customize the license dataset Provenant uses, first export the built-in effective dataset and then point a scan at the exported dataset root:

```sh
provenant --export-license-dataset /tmp/provenant-license-dataset
provenant --json-pp licenses.json --license --license-dataset-path /tmp/provenant-license-dataset /path/to/project
```

Use this advanced workflow when you want to inspect, edit, or replace the `.RULE` and `.LICENSE` files Provenant uses. The dataset root must contain:

```text
<dataset-root>/
  manifest.json
  rules/
  licenses/
```

When `--license-dataset-path` is set, Provenant uses that dataset as authoritative input instead of the embedded dataset shipped in the binary.

If you need the matched text that triggered a detection, add `--license-text`:

```sh
provenant --json-pp licenses.json --license --license-text /path/to/project
```

Add diagnostics only when you are actively investigating why something matched:

```sh
provenant --json-pp licenses.json --license --license-text --license-text-diagnostics --license-diagnostics /path/to/project
```

Add `--license-references` when you want top-level unique license and rule reference blocks, and add `--unknown-licenses` when you want unmatched license-like text surfaced for review.

If you are troubleshooting PDF extraction specifically, Provenant suppresses noisy `pdf_oxide`
dependency logs by default so normal scan output stays readable. To inspect the raw PDF parser
logs for a debugging run, rerun with `RUST_LOG=pdf_oxide=warn` (or `=error` if you only want
higher-severity dependency logs).

#### License index cache

On first use with `--license`, Provenant builds a license index from the embedded rules and saves
it under the shared cache root (`license-index/embedded/<fingerprint>.rkyv`, ~340 MB). Subsequent
runs load the cache instead of rebuilding the index, reducing startup from ~12s to ~0.8s.

The cache is automatically invalidated when:

- a new provenant binary ships with different embedded rules (detected via SHA-256 fingerprint)
- a custom license dataset loaded with `--license-dataset-path` changes between runs

Three CLI flags control cache behavior:

- `--reindex` — force a cache rebuild, ignoring any existing cache
- `--no-license-index-cache` — build the license index in memory for this run without reading or writing persistent license-cache files
- `--cache-dir <DIR>` — choose the shared cache root for both incremental manifests and license-index cache files

```sh
provenant --json-pp scan.json --license --cache-dir .cache/provenant --reindex /path/to/project
```

### 3. "I want file metadata such as checksums and type hints"

```sh
provenant --json-pp info.json --info /path/to/project
```

Use `--info` when you want file-level metadata rather than legal or package detections.

This is useful for:

- checksums and file sizes
- source/script hints
- output-shaping workflows that depend on file metadata later

You also need `--info` for some related features such as `--mark-source`.

### 4. "I want packages and dependencies"

```sh
provenant --json-pp packages.json --package /path/to/project
```

Use this when you want package manifests, lockfile-derived dependencies, and assembled package records.

This is a strong default for:

- ecosystem inventory
- dependency review
- preparing for SBOM-oriented output later

What to expect in the results:

- top-level `packages`
- top-level `dependencies`
- file-level package data attached to supported manifests and lockfiles

### 5. "I want both packages and licenses together"

```sh
provenant --json-pp scan.json --license --package /path/to/project
```

This is one of the most common real-world scans.

Use it when you want to answer both:

- "What components are here?"
- "What licenses were detected in this codebase?"

This combination is often more useful than a package-only or license-only run because it gives both codebase-level license findings and package/dependency context in one result file.

### 6. "I only want package data, and I want it fast"

```sh
provenant --json-pp packages.json --package-only /path/to/project
```

Use `--package-only` when you explicitly want a narrower package-focused scan and do **not** want license or copyright detection.

This is useful when:

- you are doing package inventory only
- you want a faster specialized scan
- you plan to run a deeper license scan separately

Important: `--package-only` is a special mode, not a synonym for `--package`. It enables both application-manifest and installed-package detection, intentionally skips license/copyright work, skips the normal top-level package assembly path, and does not create the usual top-level `packages` and `dependencies` view you get from `--package`.

If you explicitly ask for non-license detections such as `--email`, `--url`, or `--generated`, those still behave normally in `--package-only` mode.

If you want assembled top-level packages and dependencies, use `--package` instead.

### 7. "I need system package data"

```sh
provenant --json-pp system-packages.json --system-package /path/to/rootfs-or-image-extract
```

Use this when scanning extracted environments or roots that contain installed package databases rather than just source manifests.

This is the right workflow for things like:

- extracted container filesystems
- unpacked root filesystems
- operating-system package metadata trees

### 8. "I want package data from compiled binaries"

```sh
provenant --json-pp compiled-packages.json --package-in-compiled /path/to/project
```

Use this when you want package metadata embedded in supported compiled Go or Rust binaries.

This is useful when:

- the source manifests are missing
- you are auditing built artifacts rather than source
- you want binary-level package provenance in addition to manifest-based scans

If you also want manifest/lockfile package detection, combine it with `--package`.

### 9. "I want a browsable HTML report"

```sh
provenant --html report.html --license --copyright /path/to/project
```

Use this when you want to review findings in a browser rather than inspect JSON directly.

HTML is useful for:

- manual review
- sharing a quick report with someone who does not want raw JSON
- checking whether the scan is generally finding what you expected before moving into machine-readable formats

### 10. "I need SPDX or CycloneDX output"

```sh
provenant --cyclonedx bom.json --package /path/to/project
```

or:

```sh
provenant --spdx-tv sbom.spdx --package /path/to/project
```

Use these formats when another tool or downstream process expects them.

In practice:

- CycloneDX is often the better fit for BOM-oriented pipelines.
- SPDX is often the better fit for compliance-oriented exchange.
- `--package` is usually part of these workflows because package/dependency data is central to SBOM output.

### 11. "I need Debian copyright output"

```sh
provenant --debian debian.copyright --license --copyright --license-text /path/to/project
```

Use this when you need a machine-readable Debian copyright file.

Why the extra flags matter:

- `--license` provides the detected license expressions
- `--copyright` provides copyright holders and statements
- `--license-text` provides matched text blocks used in the Debian output

This workflow is more specialized than JSON or HTML, so it is usually something you generate after you already know you need Debian-format output.

### 12. "I want to ignore obvious noise"

```sh
provenant --json-pp scan.json --license --package /path/to/project --ignore "*.min.js" --ignore "node_modules/*"
```

Use ignore patterns when you want to:

- skip vendored or generated content
- reduce scan time on very large trees
- keep results focused on the code you actually care about

Use quotes around glob patterns so your shell does not expand them before Provenant sees them.

### 13. "I want to inspect results in the terminal first"

```sh
provenant --json-pp - --license --package /path/to/project
```

Use stdout when you are trying to validate a command quickly before saving a file or when you want to pipe the result elsewhere.

### 14. "I already have a scan and only want to reshape it"

```sh
provenant --json-pp reshaped.json --from-json scan.json --only-findings
```

Use `--from-json` when you want to reuse an existing ScanCode-style JSON result instead of rescanning the original inputs.

This is especially useful for:

- applying output filters after the fact
- producing a different output view from the same base scan
- merging or reshaping multiple prior JSON scans

Important: `--from-json` is for reshaping existing results. It is not a second scan pass, and scan-time options such as fresh detection flags are intentionally restricted in this mode.

### 15. "I want a codebase-level summary instead of reading raw file-by-file results"

```sh
provenant --json-pp summary.json --license --package --classify --summary /path/to/project
```

Use this when the raw scan output is correct but too detailed for your immediate question.

Why it is useful:

- `--classify` enables higher-level classification output.
- `--summary` adds codebase-level summary information rather than leaving you with only file-by-file details.

If you want count-oriented review, add `--tallies`:

```sh
provenant --json-pp summary.json --license --package --classify --summary --tallies /path/to/project
```

This is a good second-step workflow after a first broad scan, especially on larger repositories.

### 16. "I run the same scan repeatedly"

```sh
provenant --json-pp scan.json --license --package --incremental /path/to/project
```

Use incremental reuse for repeated native directory scans.

After a completed scan, Provenant stores an incremental manifest under the cache root and uses it
on the next run to skip unchanged files. In practice, this is most useful when you are scanning
the same checkout repeatedly: local iteration, CI retries, or rerunning after a later failed or
interrupted scan.

Good use cases:

- iterative local review on the same repository
- repeated scans in a CI-like workflow
- large trees where rescanning unchanged content is expensive
- retrying a later scan without redoing unchanged work from the last completed run

Important details:

- `--incremental` enables this behavior.
- `--cache-dir PATH` and `PROVENANT_CACHE` choose the shared cache root.
- that root stores both incremental manifests and reusable license-index cache files.
- `--cache-clear` clears that shared cache state before the run.
- if the previous manifest is missing, unreadable, or incompatible, Provenant falls back to a full rescan and rewrites it.
- incremental reuse applies to native scans, not `--from-json` reshaping.

### 17. "I want policy-aware license review"

```sh
provenant --json-pp policy.json --license --license-references --filter-clues --license-policy policy.yml /path/to/project
```

Use this when you want a review-oriented license scan rather than raw low-level findings.

Why it is useful:

- `--license-references` adds top-level license and rule reference blocks.
- `--filter-clues` removes redundant clue output that is usually noisy in broad review workflows.
- `--license-policy policy.yml` evaluates file findings against a YAML policy after the scan.
- `--ignore-author PATTERN` and `--ignore-copyright-holder PATTERN` let you suppress entire resources when those findings match review-specific regexes.

This workflow is also useful with `--from-json` when you want to reshape an existing scan instead of rescanning the original inputs.

### 18. "I want tallies, facets, or clarity scoring"

```sh
provenant --json-pp summary.json --license --package --classify --summary --tallies /path/to/project
```

Build on that baseline when you need more structured review output:

- add `--license-clarity-score` for project-level clarity scoring
- add `--tallies-with-details` for file- and directory-level tallies
- add `--tallies-key-files` for key-file-focused tallies
- add one or more `--facet <facet>=<pattern>` rules, then `--tallies-by-facet`, to split tallies by shipping code vs tests/docs/examples

Example:

```sh
provenant --json-pp summary.json --license --package --classify --summary --tallies --facet core=src/** --facet tests=test/** --tallies-by-facet --license-clarity-score /path/to/project
```

### 19. "I need to scan more than one input path"

```sh
provenant --json-pp scan.json --license dir-a dir-b
```

Use this when you want one result file covering more than one native input path.

This is useful for:

- scanning related repositories together
- scanning split source trees in one run
- collecting one combined report for several directories

These native multi-input paths still follow the current common-prefix behavior. They work best when you can invoke Provenant from a cwd where the relative input paths share a usable common ancestor.

You can also pass multiple JSON inputs with `--from-json`.

### 20. "I have an explicit list of files or directories to scan"

```sh
provenant --json-pp scan.json --license /path/to/repo --paths-file changed-files.txt
```

Use this when you already have a selected path list under one known root, especially for CI and pull-request workflows where cwd cannot be the repo root.

`--paths-file` is the preferred workflow when:

- `git diff --name-only` or another tool already produced the changed-file list
- Provenant must run from a fixed mount location or other non-repo cwd
- you want Provenant itself, not shell `xargs`, to own the selection semantics

v1 semantics:

- pass exactly one native scan root as the positional input
- entries in the paths file are interpreted relative to that root
- one path per line, with blank lines ignored and CRLF tolerated
- directory entries select that subtree
- missing entries are skipped with a warning
- `--paths-file -` reads the list from stdin
- `--paths-file` cannot be combined with `--from-json` in v1

Example with stdin:

```sh
git diff --name-only --diff-filter=d origin/main...HEAD | provenant --json-pp - --license /path/to/repo --paths-file -
```

## Important Flag Combinations

These are worth learning early because they change what the output means:

- `--license-text` requires `--license`
- `--license-text-diagnostics` requires `--license-text`
- `--license-diagnostics` requires `--license`
- `--license-references` requires `--license`
- `--license-clarity-score` requires `--classify`
- `--mark-source` requires `--info`
- `--custom-output <FILE>` requires `--custom-template <FILE>`
- `--tallies-key-files` requires `--tallies` and `--classify`
- `--tallies-by-facet` requires `--facet` and `--tallies`
- `--debian <FILE>` requires `--license`, `--copyright`, and `--license-text`
- `--paths-file <FILE>` requires exactly one native scan root and is native-scan only in v1 (no `--from-json`)
- `--reindex` only matters when the license engine is initialized (`--license` and some `--from-json` reference-recompute flows)
- `--no-license-index-cache` only matters when the license engine is initialized

## A Simple Decision Guide

If you are not sure where to start, use this rule of thumb:

- Want a general first scan? → `--json-pp` + `--license` + `--package`
- Want copyright review too? → add `--copyright`
- Want assembled top-level packages and dependencies? → `--package`
- Want a narrower file-level package-data pass across application and installed-package inputs without normal top-level assembly? → `--package-only`
- Want SBOM-oriented output? → add `--cyclonedx` or `--spdx-*`, usually with `--package`
- Want browser-friendly review? → `--html`
- Want policy-aware license review? → add `--license-references`, `--filter-clues`, and optionally `--license-policy`
- Want summary/tally/facet review? → add `--classify`, `--summary`, and optionally `--tallies*` / `--facet`
- Already have JSON and only want to filter or reshape it? → `--from-json`

## Where to Go Next

- Run `provenant --help` for the full CLI surface
- See [README.md](../README.md) for installation and quick start
- See [SUPPORTED_FORMATS.md](SUPPORTED_FORMATS.md) for supported package and ecosystem coverage
- See [ARCHITECTURE.md](ARCHITECTURE.md) for implementation details
