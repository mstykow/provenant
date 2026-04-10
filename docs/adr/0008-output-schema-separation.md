# ADR 0008: Output Schema Type Separation

**Status**: Accepted  
**Authors**: Provenant team  
**Supersedes**: None  
**Current Contract Owner**: `src/output_schema/` module

## Context

All internal model types in Provenant originally derived both `Serialize` and `Deserialize` from serde and carried output-formatting attributes (`skip_serializing_if`, `rename`, `serialize_with`, custom `Serialize` impls). This conflated two distinct concerns:

1. **Internal domain logic** — scanning, parsing, assembly, and post-processing operate on strongly-typed domain values (`LineNumber`, `Sha1Digest`, `FileType`, `DatasourceId`).
2. **ScanCode-compatible JSON output** — the public JSON schema requires specific field names (`"type"` for `package_type`), conditional field omission (`skip_serializing_if`), `None` serialized as `{}` for maps, and a hand-rolled `FileInfo` serializer with info-surface gating and controlled field ordering.

Mixing these concerns in one type meant:

- Adding a serde attribute for output formatting could silently change cache serialization behavior.
- Custom `Serialize` on `FileInfo` (200+ lines) was hard to maintain and impossible to test in isolation.
- `--from-json` deserialization shared the same field-renaming and type-widening attrs as output serialization, making it unclear which attrs served which purpose.
- The output schema was implicit — scattered across derive attrs and a hand-written `Serialize` impl rather than defined in one place.

## Decision

Separate the ScanCode-compatible output schema from internal types:

1. **Output schema types** (`src/output_schema/`) are dedicated serde-enabled types, one file per type, that define the ScanCode-compatible JSON schema explicitly. They own all output-formatting logic:
   - Field renames (`package_type` → `"type"`, `license_expression` → `"detected_license_expression_spdx"`)
   - Conditional field omission (`skip_serializing_if`)
   - `None` → `{}` serialization for optional maps
   - The `FileInfo` info-surface gating logic
   - Type widening: `LineNumber` → `u64`, `Sha1Digest` → `Option<String>` (hex)

2. **Internal types** (`src/models/`) retain serde for cache round-tripping and `--from-json` deserialization. Output-specific attributes (`skip_serializing_if`, `serialize_with`, custom `Serialize` impls) are removed from internal types.

3. **Conversion boundary** — `From<&InternalType>` converts internal → output at the serialization boundary in `main.rs`. `TryFrom<&OutputType>` converts output → internal for the `--from-json` path, with validation at the conversion boundary.

4. **Shared enums** (`FileType`, `DatasourceId`, `PackageType`) are reused directly in output types — not widened to `String`. These enums have their own serde impls that are shared between internal and output contexts.

5. **Internal types retain `Serialize`/`Deserialize`** for non-JSON-output purposes:
   - Incremental cache stores `FileInfo` as JSON in the manifest (`src/cache/incremental.rs`)
   - Scanner spill-to-disk serializes `FileInfo` during large scans

### Invariants contributors must follow

- **Never add output-formatting serde attrs to internal types.** Output behavior belongs in `src/output_schema/`.
- **Never serialize `models::Output` directly.** Always convert to `output_schema::Output` first via `output_schema::Output::from(&internal_output)`.
- **The `--from-json` path deserializes into output schema types**, then converts to internal types via `TryFrom`. This ensures the same validation rules apply regardless of input direction.
- **Keep `output_schema` types in sync with internal types.** When adding a field to an internal type, add the corresponding field and conversion to the output schema type.

### Scope boundaries

- License detection internal types (`src/license_detection/`) still have serde for the embedded index artifact (MessagePack). That is a separate serialization concern and is out of scope for this ADR.
- Parser-private deserialization types are out of scope — they parse external file formats, not the Provenant output schema.

## Consequences

### Benefits

- **Single source of truth for the JSON schema.** The output schema is defined explicitly in `src/output_schema/`, not scattered across derive attrs and custom impls.
- **Internal types are simpler.** No `skip_serializing_if`, `serialize_with`, or custom `Serialize` on domain types. Serde attrs on internal types are limited to `rename`, `default`, `alias`, and `transparent` — all serving deserialization or cache round-tripping.
- **Output formatting is testable in isolation.** The `OutputFileInfo` custom `Serialize` can be tested without running a full scan.
- **`--from-json` gets explicit validation.** `TryFrom` conversions reject invalid data at the boundary instead of silently deserializing into potentially invalid internal state.

### Trade-offs

- **Two parallel type hierarchies.** Every internal type now has a corresponding output schema type. This is ~30 new types and ~2,500 lines of conversion code.
- **Conversion overhead at the boundary.** Every scan pays the cost of converting `models::Output` → `output_schema::Output`. In practice this is a shallow field-by-field clone with no serialization/deserialization overhead.
- **Drift risk.** Adding a field to an internal type without updating the output schema type will silently drop it from output. Mitigated by the one-file-per-type structure making it easy to compare.

## Alternatives Considered

### 1. Keep serde attrs on internal types, remove only the custom `Serialize` impl

Would simplify the `FileInfo` serializer but leave output formatting scattered across derive attrs. Doesn't solve the core problem of implicit output schema.

### 2. Use `#[serde(with = "...")]` modules on internal types to separate serialization logic

Still mixes output concerns into internal types. The `with` module pattern is better suited for field-level customization than type-level separation.

### 3. Generate output schema types from a schema definition (e.g., JSON Schema)

Would ensure schema consistency but adds a build-time code generation step and makes it harder to maintain custom serialization logic like the `FileInfo` info-surface gating.

## Related ADRs

- [ADR 0001: Trait-Based Parser Architecture](0001-trait-based-parsers.md) — parsers return `PackageData` (internal type), not output types
- [ADR 0006: DatasourceId-Driven Package Assembly](0006-datasourceid-driven-package-assembly.md) — assembly produces internal `Package` and `TopLevelDependency`, which are then converted to output types

## References

- Output schema module: `src/output_schema/`
- Conversion boundary: `src/main.rs` (where `models::Output` → `output_schema::Output`)
- `--from-json` deserialization: `src/scan_result_shaping/json_input.rs`
- Cache serialization: `src/cache/incremental.rs`
