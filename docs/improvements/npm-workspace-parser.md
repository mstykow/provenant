# npm Workspace: Improvements Over Python

## Summary

Provenant improves on the Python reference in two durable ways:

- ✨ **New Feature: pnpm-workspace.yaml metadata extraction** — Python recognizes the file but extracts no metadata (NonAssemblable handler)
- ✨ **Improved Assembly: topology-planned workspace assembly with per-member packages** — Python has basic workspace assembly; Rust adds exclusion patterns, topology-aware pre-claiming, and more robust member discovery

## Parser improvement: `pnpm-workspace.yaml` metadata extraction

### Problem in Python Reference

Python ScanCode has a `PnpmWorkspaceYamlHandler` in `packagedcode/npm.py`, but it is declared as `NonAssemblable` — meaning it only detects the file's presence without extracting any useful data from it.

The handler recognizes `pnpm-workspace.yaml` files but produces no package metadata, no workspace pattern extraction, and no structural information about the monorepo.

### Rust behavior

Rust extracts workspace configuration data from `pnpm-workspace.yaml`, including:

- Workspace package glob patterns (e.g., `packages/*`, `apps/*`)
- Monorepo structure information
- Negation patterns for excluded packages

### Output difference

**Python Output** (stub — NonAssemblable):

```json
{
  "type": "npm",
  "name": null,
  "version": null,
  "extra_data": {}
}
```

**Rust Output** (real extraction):

```json
{
  "type": "npm",
  "extra_data": {
    "datasource_id": "pnpm_workspace_yaml",
    "workspaces": ["packages/*", "apps/*", "tools/*"]
  }
}
```

### Extracted data

| Field                      | Source           | Description                                                 |
| -------------------------- | ---------------- | ----------------------------------------------------------- |
| `package_type`             | hardcoded        | `"npm"` (consistent with ecosystem)                         |
| `extra_data.datasource_id` | hardcoded        | `"pnpm_workspace_yaml"`                                     |
| `extra_data.workspaces`    | `packages` field | Array of glob patterns defining workspace package locations |

### Supported Patterns

The parser handles all pnpm workspace glob patterns:

- `"packages/*"` — Single-level wildcard
- `"**/components/*"` — Recursive wildcard
- `"!packages/excluded"` — Negation patterns
- `"*"` — Root-level wildcard
- Empty or missing `packages` field — Graceful fallback

## Assembly improvement: richer workspace handling

### Python reference behavior

The Python reference handles workspace assembly by:

- Reads `workspaces` from package.json
- Reads `pnpm-workspace.yaml` if present
- Creates separate Package for each workspace member
- Uses `walk_npm()` to assign resources, skipping `node_modules`
- Resolves `workspace:*` version references

### Rust improvement

Rust preserves the same core workspace behavior while adding several user-visible improvements:

| Feature                                  | Python | Rust | Improvement                                                |
| ---------------------------------------- | ------ | ---- | ---------------------------------------------------------- |
| Workspace root detection                 | ✅     | ✅   | Equivalent                                                 |
| Member discovery via globs               | ✅     | ✅   | Three-tier matching (simple, single-star, complex)         |
| Per-member Package creation              | ✅     | ✅   | Equivalent                                                 |
| `workspace:*` version resolution         | ✅     | ✅   | Equivalent                                                 |
| `workspace:^` / `workspace:~` resolution | ✅     | ✅   | Equivalent                                                 |
| `for_packages` assignment                | ✅     | ✅   | Equivalent                                                 |
| pnpm variant handling                    | ✅     | ✅   | Equivalent                                                 |
| **Exclusion patterns**                   | ❌     | ✅   | 🆕 Respects `!pattern` negation in workspace globs         |
| **Sibling-merge cleanup**                | ❌     | ✅   | 🆕 Removes duplicate packages from earlier assembly phases |
| **Explicit dependency cleanup**          | ❌     | ✅   | 🆕 Cleans up root dependencies after hoisting to members   |

### Concrete user-visible differences

1. **Exclusion patterns**: Rust respects `!pattern` entries in workspace globs, so excluded packages do not leak into the workspace package set.
2. **Topology-aware directory claiming**: Rust now plans workspace roots and members before the generic directory loop runs, so member directories skip the default local assembly path instead of being created and repaired later.
3. **Workspace-version fidelity**: Rust resolves workspace versions only once all member versions are known, so `workspace:*`, `workspace:^`, and `workspace:~` references stay grounded in the actual workspace state.
4. **Root-package cleanup**: Rust drops private root packages once all content is reassigned to workspace members, avoiding redundant root-only package entries.
5. **Member validation**: Rust verifies that discovered members actually contain `package.json` files before creating package records.

## Impact

- **Monorepo visibility**: pnpm workspaces are increasingly common; extracting their structure provides context for dependency analysis
- **SBOM completeness**: Workspace configuration files are no longer opaque to the scanner
- **Correct package counts**: No duplicate packages from assembly phase interactions
- **Accurate dependency graphs**: `workspace:*` references resolved to actual versions

## References

### pnpm Documentation

- [pnpm-workspace.yaml](https://pnpm.io/pnpm-workspace_yaml)

The parser and assembly behavior above describe the stable workspace improvements that matter to users.
