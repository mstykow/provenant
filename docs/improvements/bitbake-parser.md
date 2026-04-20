# BitBake Parser Improvements

## Summary

Rust now ships static BitBake support for both `.bb` recipe files and `.bbappend` append files even
though the Python ScanCode reference does not currently provide a production BitBake parser.

## Rust Improvements

### Recipe and append-file coverage

- Rust recognizes both `.bb` and `.bbappend` files.
- `.bbappend` files participate in sibling assembly with their matching recipe instead of remaining
  standalone parser-only output.
- Filename-derived identity supports the common `name_version.bb` shape and bounded `%`
  wildcards in append filenames.

### Bounded BitBake variable semantics

- Rust extracts `PN`, `PV`, `SUMMARY`, `DESCRIPTION`, `HOMEPAGE`, `BUGTRACKER`, `SECTION`, and
  inherited classes.
- Rust supports ordinary assignment operators (`=`, `?=`, `??=`, `:=`, `+=`, `=+`, `.=` , `=.`)
  plus bounded override-style handling for `:append`, `:prepend`, `:remove`, and their legacy
  `_append`, `_prepend`, `_remove` forms.
- Variable references such as `${EXTRA_DEPENDS}` are preserved in raw metadata surfaces where
  appropriate, but unresolved dependency placeholders are not promoted into dependency purls.

### License and dependency fidelity

- Rust preserves the raw `LICENSE` value as `extracted_license_statement`.
- Rust prefers package-specific `LICENSE:${PN}` / `LICENSE_${PN}` declarations when present and
  falls back to recipe-level `LICENSE` otherwise.
- BitBake-specific `&` / `|` operators are normalized for declared-license expression generation.
- `DEPENDS` and `RDEPENDS` variants are extracted as build/runtime dependencies, and versioned
  requirements such as `foo (>= 1.2)` are preserved in `extracted_requirement`.

### Source metadata fidelity

- When a recipe declares exactly one non-local source URI, Rust promotes that entry to top-level
  `download_url` metadata.
- Rust extracts source checksum metadata from both inline `SRC_URI` parameters and documented
  varflag forms such as `SRC_URI[sha256sum]` and `SRC_URI[name.sha256sum]`.

### Local file references and scanner ownership

- Local `LIC_FILES_CHKSUM` and `SRC_URI` `file://...` entries are emitted as `file_references`
  instead of being dropped.
- Scanner assembly resolves those references relative to the manifest directory, so sibling files
  like patches and license texts can attach back to the assembled BitBake package.

## Guardrails

- Rust does **not** execute BitBake, evaluate full override context, or resolve fetcher search
  paths such as `FILESPATH` dynamically.
- The current implementation is intentionally bounded static parsing aimed at trustworthy manifest
  metadata recovery rather than full BitBake execution semantics.
