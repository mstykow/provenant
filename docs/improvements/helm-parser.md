# Helm Parser Improvements

## Summary

Rust now ships static Helm chart support for `Chart.yaml` and `Chart.lock` even though the Python ScanCode reference still has no production Helm parser.
The supported surface focuses on the high-value official metadata from Helm itself: chart identity, maintainers, declared chart dependencies, and pinned lockfile dependency state.
That static file-format coverage remains compatible with current Helm v4 chart metadata, including `apiVersion: v3` charts.

## Python Status

- Python ScanCode does not currently ship a Helm packagedcode parser.
- Upstream interest exists, but there is no packagedcode implementation or test suite to port directly.
- This gives Rust direct packagedcode support for Helm chart metadata that the Python reference does not currently provide.

## Rust Improvements

### Static `Chart.yaml` metadata extraction

- Rust now recognizes `Chart.yaml` and extracts chart identity from `name`, `version`, and `apiVersion`.
- The parser also preserves `description`, `home`, `keywords`, `maintainers`, and common Helm metadata such as `appVersion`, `kubeVersion`, `type`, `icon`, `sources`, and `annotations`.
- Root packages are emitted with Helm package identities such as `pkg:helm/nginx@22.1.1`.
- The same static metadata path works for legacy `apiVersion: v1`, Helm 3 `apiVersion: v2`, and current Helm 4 `apiVersion: v3` charts.

### Declared dependency extraction from `Chart.yaml`

- Rust now extracts chart dependencies declared in `Chart.yaml`.
- It preserves dependency metadata including `repository`, `condition`, `tags`, `alias`, and `import-values`.
- Exact dependency versions are treated as pinned; range-style versions remain unpinned requirements.

### Pinned dependency state from `Chart.lock`

- Rust now parses `Chart.lock` for locked dependency versions.
- It preserves top-level lock metadata like `digest` and `generated`.
- Sibling assembly keeps both the declared dependency view from `Chart.yaml` and the pinned dependency view from `Chart.lock`, following the same manifest+lockfile pattern already used in Cargo and Composer.

## Guardrails

- Rust does **not** evaluate templates, parse `values.yaml`, fetch remote chart repositories, inspect packaged chart archives, or resolve charts from OCI registries.
- Legacy `apiVersion: v1` charts still have their core chart metadata parsed from `Chart.yaml`, but this supported surface does not implement `requirements.yaml` / `requirements.lock`.
- Helm 4 operational changes such as OCI workflows, reproducible packaging, and multi-document values files do not change the static `Chart.yaml` / `Chart.lock` schema handled here.
- Malformed dependency entries are skipped instead of causing the whole chart parse to fail.

## Coverage

Coverage spans chart metadata extraction, declared and locked dependency handling, sibling assembly, malformed dependency tolerance, and the documented non-evaluating guardrails.

## References

- [Helm chart format docs](https://helm.sh/docs/topics/charts/)
- [Helm overview](https://helm.sh/docs/overview/)
- [Helm releases](https://github.com/helm/helm/releases)
- [Helm dependency update](https://helm.sh/docs/helm/helm_dependency_update/)
- [Helm dependency build](https://helm.sh/docs/helm/helm_dependency_build/)
