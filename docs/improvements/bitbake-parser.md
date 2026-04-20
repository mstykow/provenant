# BitBake Parser — Net-New

The `BitbakeRecipeParser` is a net-new parser with no Python ScanCode reference implementation.

## What it does

Parses Yocto / OpenEmbedded BitBake recipe files (`.bb`) to extract:

- **Package identity**: name and version derived from filename convention (`name_version.bb`)
  or explicit `PN` / `PV` variable assignments
- **Metadata**: `SUMMARY`, `DESCRIPTION`, `HOMEPAGE`, `BUGTRACKER`, `SECTION`
- **License**: `LICENSE` variable with BitBake-specific operator normalization (`&` → `AND`,
  `|` → `OR`) and SPDX expression mapping
- **Dependencies**: `DEPENDS` (build-time) and `RDEPENDS` variants (runtime), supporting both
  modern colon-based (`RDEPENDS:${PN}`) and legacy underscore-based (`RDEPENDS_${PN}`) override
  syntax
- **Source URIs**: non-local `SRC_URI` entries (filters out `file://` patches)
- **Inherited classes**: `inherit` directives

## Variable handling

The parser handles all BitBake assignment operators (`=`, `?=`, `??=`, `:=`, `+=`, `=+`, `.=`,
`=.`) with correct precedence semantics and multi-line continuation via `\`.

Variable references like `${EXTRA_DEPENDS}` in dependency lists are filtered out since they
cannot be resolved statically.
