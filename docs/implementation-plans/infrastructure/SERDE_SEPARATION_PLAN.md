# Serde Separation Plan: Internal Types vs. Output Schema Types

## Status: Implemented (Phases 0–6 complete)

## Problem Statement

Provenant's internal domain types carry `serde` attributes and derive macros that couple them to the JSON output schema. This means:

1. **Internal types depend on serialization details** — field renames (`package_type` → `"type"`), conditional omission (`skip_serializing_if`), and custom serializers (`serialize_optional_map_as_object`) are embedded in domain logic types.
2. **Schema evolution is hard** — changing the JSON output requires modifying internal types, risking breakage in logic that depends on those types.
3. **`serde` is viral** — any type that contains a `Serialize`/`Deserialize` field must itself derive or implement serde, spreading serialization concerns deep into the crate.
4. **Internal types can't evolve freely** — adding, renaming, or restructuring internal fields is constrained by backward compatibility with the JSON output format.

The goal: **no internal type should use serde for JSON output**. Output types should live in a dedicated module with conversion functions bridging internal → output (and output → internal for `--from-json`). Internal types retain `Serialize`/`Deserialize` for non-JSON persistence (MessagePack cache, spill-to-disk) but no longer carry JSON-specific serde attributes for the output path.

## Scope

### In Scope

- Separating serde from all internal types in `src/models/`, `src/copyright/`, `src/assembly/`, and `src/license_detection/`
- Creating a dedicated `src/output_schema/` module with one file per output type
- Writing conversion functions (internal → output, and output → internal where needed)
- Maintaining exact JSON output compatibility (golden tests must pass unchanged)
- Maintaining exact `--from-json` deserialization compatibility

### Out of Scope

- Parser-internal deserialization types (private structs in `src/parsers/*.rs` used only to parse external file formats like `Package.swift`, `conandata.yml`, etc.) — these are already internal-only and don't leak into the output schema
- Test-only `Deserialize` types (golden test fixtures) — test infrastructure, not production types
- Cache types (`src/cache/incremental.rs`) — these use serde for MessagePack persistence, not JSON output; they will be addressed separately (see [Appendix A](#appendix-a-cache-types))
- Embedded license index types (`src/license_detection/`) — these use serde for binary artifact deserialization at load time, not JSON output; they will be addressed separately (see [Appendix B](#appendix-b-license-detection-internal-types))
- CycloneDX, SPDX, Debian, and HTML output formats — these have their own schema types already separate from the core models
- Any changes to the JSON schema itself — the output format must remain identical

## Current State

### Type Inventory

The codebase has ~50+ types with serde attributes. They fall into four categories:

#### Category 1: Pure Output Types (already in `models/output.rs`)

These types exist only to shape the JSON output. They are never used in internal computation logic — only constructed during `create_output()` and consumed by serialization.

| Type                       | Current Location   |
| -------------------------- | ------------------ |
| `Output`                   | `models/output.rs` |
| `TopLevelLicenseDetection` | `models/output.rs` |
| `Summary`                  | `models/output.rs` |
| `LicenseClarityScore`      | `models/output.rs` |
| `TallyEntry`               | `models/output.rs` |
| `Tallies`                  | `models/output.rs` |
| `FacetTallies`             | `models/output.rs` |
| `Header`                   | `models/output.rs` |
| `ExtraData`                | `models/output.rs` |
| `SystemEnvironment`        | `models/output.rs` |
| `LicenseReference`         | `models/output.rs` |
| `LicenseRuleReference`     | `models/output.rs` |

These are already "output types" and just need to move to the new module. No internal-logic separation is needed.

#### Category 2: Dual-Use Types (internal + output, in `models/file_info.rs`)

These types are used throughout the scanning, assembly, and post-processing pipeline AND also appear directly in the JSON output. They carry serde attributes that control JSON formatting. **This is the core of the refactoring.**

| Type                 | Key Serde Behaviors                                                                                                                                                                                         |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `FileInfo`           | Custom hand-written `Serialize` (field ordering, conditional info-surface emission); `file_type` → `"type"`, `file_type_label` → `"file_type"`, `license_expression` → `"detected_license_expression_spdx"` |
| `PackageData`        | `package_type` → `"type"`; `serialize_optional_map_as_object` on `qualifiers`/`extra_data`                                                                                                                  |
| `Package`            | `package_type` → `"type"`; `serialize_optional_map_as_object` on `qualifiers`/`extra_data`                                                                                                                  |
| `ResolvedPackage`    | `package_type` → `"type"`; `serialize_optional_map_as_object` on `qualifiers`/`extra_data`                                                                                                                  |
| `Dependency`         | `serialize_optional_map_as_object` on `extra_data`                                                                                                                                                          |
| `TopLevelDependency` | `serialize_optional_map_as_object` on `extra_data`                                                                                                                                                          |
| `LicenseDetection`   | `skip_serializing_if` on `detection_log` and `identifier`                                                                                                                                                   |
| `Match`              | `skip_serializing_if` on multiple optional fields                                                                                                                                                           |
| `Copyright`          | No serde attributes (plain derive)                                                                                                                                                                          |
| `Holder`             | No serde attributes (plain derive)                                                                                                                                                                          |
| `Author`             | No serde attributes (plain derive)                                                                                                                                                                          |
| `Party`              | `skip_serializing_if` on all optional fields                                                                                                                                                                |
| `FileReference`      | `skip_serializing_if` on optional fields                                                                                                                                                                    |
| `OutputEmail`        | No serde attributes (plain derive)                                                                                                                                                                          |
| `OutputURL`          | No serde attributes (plain derive)                                                                                                                                                                          |
| `LicensePolicyEntry` | No serde attributes (plain derive)                                                                                                                                                                          |
| `FileType`           | Custom `Serialize`/`Deserialize` mapping `File` → `"file"`, `Directory` → `"directory"`                                                                                                                     |

#### Category 3: Shared Enums (reused directly in output types)

| Type           | Current Serde Behavior                                                                                   | Output Representation |
| -------------- | -------------------------------------------------------------------------------------------------------- | --------------------- |
| `DatasourceId` | `rename_all = "snake_case"` + explicit renames + backward-compat aliases (`nuget_nupsec`, `rpm_spefile`) | `DatasourceId` (same) |
| `PackageType`  | `rename_all = "snake_case"` + kebab-case overrides (`jboss-service`, etc.)                               | `PackageType` (same)  |
| `FileType`     | Custom `Serialize`/`Deserialize` mapping `File` → `"file"`, `Directory` → `"directory"`                  | `FileType` (same)     |

These enums are natural domain types with a small, stable variant set. They will be shared between internal and output types rather than widened to `String`. The serde attributes stay on the enums themselves. See [Open Question 1](#open-questions) for the choice between shared enums with serde vs. duplicated output enums.

#### Category 4: Leaf Value Types (will widen to primitives in output)

| Type                                                                 | Current Serde Behavior                                      | Output Representation |
| -------------------------------------------------------------------- | ----------------------------------------------------------- | --------------------- |
| `Sha1Digest`, `Md5Digest`, `Sha256Digest`, `Sha512Digest`, `GitSha1` | Custom `Serialize`/`Deserialize` as hex strings (via macro) | `String`              |
| `LineNumber`                                                         | `#[serde(transparent)]` — serializes as bare number         | `u64`                 |

Digest types and `LineNumber` widen to primitives in output. The internal types lose serde and gain `Display`/`FromStr` instead.

#### Category 5: Detection-Stage Types (copyright detection, `src/copyright/types.rs`)

| Type                 | Serde Behavior   | Usage                                                   |
| -------------------- | ---------------- | ------------------------------------------------------- |
| `CopyrightDetection` | `Serialize` only | Internal detection → converted to `Copyright` in models |
| `HolderDetection`    | `Serialize` only | Internal detection → converted to `Holder` in models    |
| `AuthorDetection`    | `Serialize` only | Internal detection → converted to `Author` in models    |

These types have `Serialize` but are **not serialized directly in the output** — they are converted to the model types (`Copyright`, `Holder`, `Author`) before output. The `Serialize` derive appears unused in production; it may only be used for debug/test serialization. **This serde can likely be removed entirely.**

#### Category 6: `AssemblyResult` (unused Serialize)

`AssemblyResult` derives `serde::Serialize` but is never serialized via serde. It is only constructed and destructured in code. The `Serialize` derive can be removed outright.

### Key Serde Behaviors to Preserve

1. **Field renames**: `package_type` → `"type"`, `file_type` → `"type"`, `license_expression` → `"detected_license_expression_spdx"`, `file_type_label` → `"file_type"`
2. **`serialize_optional_map_as_object`**: `None` serializes as `{}` not `null` for `qualifiers` and `extra_data` fields
3. **Custom `FileInfo` Serialize**: Hand-written impl controlling field ordering and conditional info-surface emission (`should_serialize_info_surface()`)
4. **Digest hex-string serialization**: Currently custom macro-based; after separation, output types use `String` and the internal type's `Display` impl produces the hex string
5. **`LineNumber` transparent**: Currently `#[serde(transparent)]`; after separation, output types use `u64` directly
6. **Backward-compat aliases**: Currently `#[serde(alias = "...")]` on `DatasourceId`; stays on the shared enum (no change)
7. **`FileType` string mapping**: Currently custom `Serialize`/`Deserialize`; stays on the shared enum (no change)
8. **`--from-json` bidirectional deserialization**: Output types own `Serialize`+`Deserialize`; `TryFrom` conversions enforce internal invariants

### The `--from-json` Path

The `--from-json` CLI flag deserializes a previous JSON scan output back into internal types:

```text
JsonScanInput (Deserialize-only struct)
  ├── files: Vec<FileInfo>
  ├── packages: Vec<Package>
  ├── dependencies: Vec<TopLevelDependency>
  ├── license_detections: Vec<TopLevelLicenseDetection>
  ├── license_references: Vec<LicenseReference>
  └── license_rule_references: Vec<LicenseRuleReference>
```

This means `FileInfo`, `Package`, `TopLevelDependency`, `TopLevelLicenseDetection`, `LicenseReference`, `LicenseRuleReference`, `DatasourceId`, `PackageType`, `FileType`, and all digest types must support **both** Serialize (for output) and Deserialize (for `--from-json` input). After separation, the output schema types will own both `Serialize` and `Deserialize` for structs, while the shared enums (`DatasourceId`, `PackageType`, `FileType`) keep their serde derives since they are used directly in output types. Widened primitive types (`u64`, `String`) don't need serde derives — serde derive on the containing output struct handles them. Fallible `TryFrom` conversions enforce internal invariants when mapping output → internal.

### Existing Conversions

Current conversions between model types are manual named methods (no `From`/`Into`):

| Conversion                          | Method                                       | Location                   |
| ----------------------------------- | -------------------------------------------- | -------------------------- |
| `PackageData` → `Package`           | `Package::from_package_data()`               | `models/file_info.rs:1017` |
| `PackageData` → `ResolvedPackage`   | `ResolvedPackage::from_package_data()`       | `models/file_info.rs:853`  |
| `Dependency` → `TopLevelDependency` | `TopLevelDependency::from_dependency()`      | `models/file_info.rs:1578` |
| `Package.update()`                  | Merges `PackageData` into existing `Package` | `models/file_info.rs:1082` |

These will be replaced by the new conversion functions.

## Design

### New Module: `src/output_schema/`

All output-facing types will live in `src/output_schema/`, one file per type, with conversion functions.

```text
src/output_schema/
├── mod.rs                     # Module re-exports
├── output.rs                  # Output (top-level payload)
├── file_info.rs               # OutputFileInfo
├── package_data.rs            # OutputPackageData
├── package.rs                 # OutputPackage
├── resolved_package.rs        # OutputResolvedPackage
├── dependency.rs              # OutputDependency
├── top_level_dependency.rs    # OutputTopLevelDependency
├── license_detection.rs       # OutputLicenseDetection
├── match_type.rs              # OutputMatch
├── copyright.rs               # OutputCopyright
├── holder.rs                  # OutputHolder
├── author.rs                  # OutputAuthor
├── party.rs                   # OutputParty
├── file_reference.rs          # OutputFileReference
├── email.rs                   # OutputEmail
├── url.rs                     # OutputURL
├── license_policy_entry.rs    # OutputLicensePolicyEntry
├── top_level_license_detection.rs  # OutputTopLevelLicenseDetection
├── summary.rs                 # Summary, LicenseClarityScore
├── tallies.rs                 # TallyEntry, Tallies, FacetTallies
├── header.rs                  # Header, ExtraData, SystemEnvironment
├── license_reference.rs       # LicenseReference
├── license_rule_reference.rs  # LicenseRuleReference
└── json_input.rs              # JsonScanInput, JsonHeaderInput (Deserialize-only)
```

Note: No separate files for digest or line_number. These internal value types widen to `String`/`u64` in the output schema and don't need their own output type files. The shared enums (`FileType`, `DatasourceId`, `PackageType`) are reused directly in output types — they live in `src/models/` and output types reference them, so no separate output files are needed for them either.

### Naming Convention

Output types use an `Output` prefix to distinguish them from internal types:

- `FileInfo` (internal) → `OutputFileInfo` (output schema)
- `PackageData` (internal) → `OutputPackageData` (output schema)
- `Package` (internal) → `OutputPackage` (output schema)
- etc.

This avoids name collisions and makes the boundary explicit at every use site.

### Conversion Functions

Each output type file will contain:

1. The output type definition (with serde attributes, using widened primitive types)
2. `From<&InternalType>` for `OutputType` (internal → output, infallible)
3. `TryFrom<&OutputType>` for `InternalType` (output → internal, fallible — enforces invariants)

Example for `OutputCopyright`:

```rust
// src/output_schema/copyright.rs
use serde::{Deserialize, Serialize};
use crate::models::Copyright;
use crate::models::LineNumber;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputCopyright {
    pub copyright: String,
    pub start_line: u64,
    pub end_line: u64,
}

impl From<&Copyright> for OutputCopyright {
    fn from(value: &Copyright) -> Self {
        Self {
            copyright: value.copyright.clone(),
            start_line: value.start_line.get() as u64,
            end_line: value.end_line.get() as u64,
        }
    }
}

impl TryFrom<&OutputCopyright> for Copyright {
    type Error = String;
    fn try_from(value: &OutputCopyright) -> Result<Self, Self::Error> {
        let start_line = LineNumber::new(value.start_line as usize)
            .ok_or_else(|| format!("invalid start_line: {}", value.start_line))?;
        let end_line = LineNumber::new(value.end_line as usize)
            .ok_or_else(|| format!("invalid end_line: {}", value.end_line))?;
        Ok(Self {
            copyright: value.copyright.clone(),
            start_line,
            end_line,
        })
    }
}
```

The reverse conversion uses `TryFrom` because the output type is permissive (e.g., `u64` allows 0) while the internal type has invariants (e.g., `LineNumber` requires `NonZeroUsize`). This makes the validation boundary explicit and prevents silent data loss.

### Widened Output Types

Output schema types use **widened primitive types** for leaf value types that carry invariants. The output schema mirrors what the JSON actually contains — no invariants, no validation, just the wire format. Conversion functions become the validation boundary where internal invariants are enforced.

Shared enums (`FileType`, `DatasourceId`, `PackageType`) are reused directly in output types — they are natural domain types, not invariant-bearing wrappers.

| Internal Type                                                        | Output Type             | Rationale                                                             |
| -------------------------------------------------------------------- | ----------------------- | --------------------------------------------------------------------- |
| `LineNumber(NonZeroUsize)`                                           | `u64`                   | JSON is just a number; `NonZeroUsize` invariant checked at conversion |
| `Sha1Digest`, `Md5Digest`, `Sha256Digest`, `Sha512Digest`, `GitSha1` | `String`                | JSON is just a hex string; `from_hex()` validation at conversion      |
| `FileType` enum                                                      | `FileType` (shared)     | Natural domain enum, reused directly in output types                  |
| `DatasourceId` enum (130+ variants)                                  | `DatasourceId` (shared) | Natural domain enum, reused directly in output types                  |
| `PackageType` enum (~60 variants)                                    | `PackageType` (shared)  | Natural domain enum, reused directly in output types                  |

This approach has these advantages:

1. **No duplication of large enums** — `DatasourceId`, `PackageType`, and `FileType` are shared between internal and output types as natural domain enums.

2. **Backward-compat aliases stay on the shared enums** — `nuget_nupsec` and `rpm_spefile` remain as `#[serde(alias = "...")]` on the enum, where they naturally belong.

3. **`--from-json` is more robust for widened types** — deserialization into permissive output types (`u64`, `String`) can't fail due to invariant violations (e.g., `LineNumber` of 0, invalid hex digest). Instead, conversion returns `Result` with a clear error about what's wrong.

4. **No custom serde impls needed for widened types** — `LineNumber` no longer needs `#[serde(transparent)]`. Digest types no longer need the `define_digest!` macro's serde branches. The output types use plain `String` and `u64` fields — serde derive just works.

The internal widened types (`LineNumber`, digest types) lose their serde derives and gain `Display` and `FromStr` impls instead. The shared enums (`DatasourceId`, `PackageType`, `FileType`) keep their serde attributes since they are used directly in output types.

### The `FileInfo` Custom Serialize

`FileInfo` has a hand-written `Serialize` impl (~100 lines) that:

1. Controls field ordering (path, type, name, base_name, extension, size, then info fields, then detections, etc.)
2. Conditionally omits the entire "info surface" if no info fields are set (`should_serialize_info_surface()`)
3. Only emits boolean classification flags when `true`
4. Uses `insert_json<S, E>()` helper to serialize values to `serde_json::Value`

This logic will move to the `OutputFileInfo` type. The internal `FileInfo` will lose its `Serialize` impl entirely. The `should_serialize_info_surface()` method moves to the output schema module.

### The `serialize_optional_map_as_object` Helper

This helper serializes `Option<HashMap<String, T>>` as `{}` when `None` instead of `null`. Used on 8 fields across 5 types.

After separation, this helper moves to `src/output_schema/` (it's purely a serialization concern).

### The `--from-json` Path After Separation

Currently, `JsonScanInput` deserializes directly into `FileInfo`, `Package`, etc. After separation:

1. `JsonScanInput` deserializes into `OutputFileInfo`, `OutputPackage`, etc. — all with widened primitive types (`String`, `u64`) instead of internal value types
2. Conversion functions (`TryFrom<&OutputFileInfo> for FileInfo`, etc.) convert to internal types, enforcing invariants and returning errors for invalid data
3. The rest of the pipeline (path normalization, post-processing, etc.) operates on internal types as before
4. Backward-compat aliases (e.g., `nuget_nupsec`) are handled in `FromStr` on the internal enum, not in serde attributes on the output type

This is cleaner than the current approach where internal types must be `Deserialize` for this one use case. It's also more robust: deserialization into permissive output types succeeds even when the JSON contains values that violate internal invariants (e.g., `start_line: 0`), and the error is reported at the conversion boundary with a clear message about what's wrong and where.

### `AssemblyResult` and `CopyrightDetection` Types

- `AssemblyResult`: Remove `#[derive(serde::Serialize)]` — it's unused.
- `CopyrightDetection`, `HolderDetection`, `AuthorDetection`: Remove `Serialize` derive — these are internal detection types that are converted to model types before output. The `Serialize` appears unused in production code.

## Implementation Phases

### Phase 0: Preparation

1. **Add `#[non_exhaustive]` to all internal types** — this protects against accidental construction at call sites and makes the conversion boundary explicit. Since no types currently use `#[non_exhaustive]`, this is a clean addition.
2. **Verify that removing `Serialize` from `AssemblyResult`, `CopyrightDetection`, `HolderDetection`, and `AuthorDetection` doesn't break anything** — run `cargo check` and `cargo test` after each removal.
3. **Create `src/output_schema/mod.rs`** with initial module structure.

### Phase 1: Leaf Types (no serde attributes beyond basic derive)

Start with types that have minimal serde coupling — either no serde attributes or simple `Serialize`/`Deserialize` derives with no field-level attributes. These are the safest to separate because the conversion functions are straightforward field-by-field copies.

Types to migrate:

- `Copyright`, `Holder`, `Author` — no serde attributes
- `OutputEmail`, `OutputURL` — no serde attributes
- `LicensePolicyEntry` — no serde attributes
- `TallyEntry`, `FacetTallies`, `LicenseClarityScore` — no serde attributes
- `ExtraData`, `SystemEnvironment` — no serde attributes
- `Header` — no serde attributes

Steps per type:

1. Create output type file in `src/output_schema/`
2. Define `Output<Type>` with serde derives and attributes matching current behavior
3. Implement `From<&InternalType> for OutputType` and `From<&OutputType> for InternalType`
4. Remove serde derives from internal type
5. Update all serialization call sites to use output types
6. Update `models/mod.rs` re-exports
7. Run `cargo check` and targeted tests

### Phase 2: Value Types (internal types widen to primitives in output)

These types have custom serde behavior that disappears entirely when output types use widened primitives instead:

- `LineNumber` → `u64` in output (no more `#[serde(transparent)]`)
- Digest types → `String` in output (no more custom hex-string Serialize/Deserialize macro)

These shared enums are reused directly in output types (not widened to `String`):

- `FileType` — kept as `FileType` enum in output (custom Serialize/Deserialize stays on the enum)
- `DatasourceId` — kept as `DatasourceId` enum in output (serde renames/aliases stay on the enum)
- `PackageType` — kept as `PackageType` enum in output (serde renames stay on the enum)

Steps per widened type:

1. Internal widened types (`LineNumber`, digest types) lose all serde derives and attributes
2. Add `Display` and `FromStr` to internal widened types if not already present
3. Output types that contain these values use `String` or `u64` instead of the internal wrapper type
4. Conversion functions handle parsing: e.g., `LineNumber::new(value.start_line as usize)` for output → internal
5. The `define_digest!` macro in `src/models/digest.rs` loses its serde-related arms; internal digest types become plain byte-array wrappers with `Display` (hex) and `FromStr` (from hex)

No changes needed for the shared enums — they stay as-is with their serde attributes, and output types reference them directly.

### Phase 3: Structured Types (field-level serde attributes)

Types with `skip_serializing_if`, `rename`, `serialize_with`, and other field-level serde attributes:

- `Party` — `skip_serializing_if` on all optional fields
- `FileReference` — `skip_serializing_if` on optional fields
- `Match` — `skip_serializing_if` on many optional fields
- `LicenseDetection` — `skip_serializing_if` on `detection_log` and `identifier`
- `Dependency` — `serialize_optional_map_as_object` on `extra_data`
- `TopLevelDependency` — `serialize_optional_map_as_object` on `extra_data`

Steps per type:

1. Create output type file with all serde attributes
2. Internal type loses all serde derives and attributes
3. Conversion functions handle the mapping
4. Move `serialize_optional_map_as_object` to `src/output_schema/`

### Phase 4: Core Types (PackageData, Package, ResolvedPackage, FileInfo)

These are the most complex types with the most serde attributes, the most usage sites, and the most interdependencies.

**Order**: `PackageData` → `ResolvedPackage` → `Package` → `FileInfo`

#### PackageData

- 30+ fields, many with `serde(default)`, `package_type` → `"type"` rename, `serialize_optional_map_as_object` on `qualifiers`/`extra_data`
- Used by all parsers (return type of `extract_packages()`)
- Used by assembly as input

Steps:

1. Create `OutputPackageData` with all serde attributes
2. Remove serde from internal `PackageData`
3. Update `PackageParser::extract_packages()` to still return internal `PackageData`
4. Add conversion at serialization boundary (in `create_output()` or wherever `PackageData` enters the output)
5. Update `Package::from_package_data()` and `ResolvedPackage::from_package_data()` to work with internal types

#### ResolvedPackage

- Similar to `PackageData` but with additional fields
- Only used in assembly and output

Steps:

1. Create `OutputResolvedPackage`
2. Remove serde from internal `ResolvedPackage`
3. Update assembly code

#### Package

- Similar to `PackageData`/`ResolvedPackage` with additional fields (`package_uid`, `datafile_paths`, `datasource_ids`)
- Used in assembly output and `Output.packages`

Steps:

1. Create `OutputPackage`
2. Remove serde from internal `Package`
3. Update assembly and post-processing code

#### FileInfo

- The most complex type: custom `Serialize` impl (~100 lines), 30+ fields, `should_serialize_info_surface()` logic
- The central type used throughout scanning, assembly, and post-processing

Steps:

1. Create `OutputFileInfo` with the custom `Serialize` impl
2. Remove custom `Serialize` and `Deserialize` from internal `FileInfo`
3. Move `should_serialize_info_surface()` to the output schema module
4. Move `insert_json()` helper to the output schema module
5. Update all serialization call sites
6. The internal `FileInfo` keeps all fields but loses all serde behavior

### Phase 5: Output Container Types

Move the top-level `Output` struct and its supporting types from `models/output.rs` to `src/output_schema/output.rs`:

- `Output` (top-level payload)
- `TopLevelLicenseDetection`
- `Summary`, `LicenseClarityScore`
- `TallyEntry`, `Tallies`, `FacetTallies`
- `Header`, `ExtraData`, `SystemEnvironment`
- `LicenseReference`, `LicenseRuleReference`

Steps:

1. Move types to `src/output_schema/`
2. Update `create_output()` to construct output types instead of internal types
3. Update `src/output/mod.rs` to import from `output_schema`
4. Update `--from-json` path: `JsonScanInput` deserializes into output types, then converts to internal types

### Phase 6: `--from-json` Path Refactoring

Currently, `JsonScanInput` deserializes directly into internal types (`FileInfo`, `Package`, etc.). After separation:

1. `JsonScanInput` deserializes into output schema types (`OutputFileInfo`, `OutputPackage`, etc.) — widened primitive types for `LineNumber`/digests, shared enums for `FileType`/`DatasourceId`/`PackageType`
2. `JsonScanInput::into_parts()` converts output types to internal types using `TryFrom` impls, collecting conversion errors
3. The rest of the pipeline operates on internal types as before
4. Backward-compat aliases (e.g., `nuget_nupsec`) remain as `#[serde(alias)]` on the shared enums — no change needed
5. If a `TryFrom` conversion fails for a specific entry (e.g., an invalid digest string), the error is reported clearly and the entire load fails (no silent data loss)

### Phase 7: Cache Types (Optional, Separate PR)

See [Appendix A](#appendix-a-cache-types).

### Phase 8: License Detection Types (Optional, Separate PR)

See [Appendix B](#appendix-b-license-detection-internal-types).

## Verification Strategy

After each phase:

1. **`cargo check`** — compilation must pass
2. **`cargo clippy`** — no new warnings
3. **Targeted tests** — run tests for affected modules
4. **Golden test verification** — at least one targeted golden test to confirm JSON output is unchanged (use `--release`)
5. **`--from-json` round-trip** — verify that a JSON output can be loaded back via `--from-json`

After all phases:

1. **Full golden test suite** — all golden tests must pass unchanged
2. **`compare-outputs` xtask** — run against a known target to confirm output parity
3. **Performance check** — run `benchmark-target` to confirm no regression (the conversion layer adds allocation cost)

## Risks and Mitigations

### Risk: Performance Regression

Adding conversion functions means allocating new output-type instances for every internal-type instance at serialization time. For large scans with thousands of files, this could be noticeable.

**Mitigation**:

- Measure before and after with `benchmark-target`
- If significant, consider streaming serialization that converts on-the-fly rather than allocating a full output tree
- Note: the current `FileInfo` custom `Serialize` already allocates `serde_json::Map<String, Value>`, so the marginal cost may be small

### Risk: Large Number of Files in `output_schema/`

With ~30 files, the module is large. However, each file is small and self-contained, which aids review and reduces merge conflicts.

**Mitigation**: Accept the file count as a reasonable trade-off for separation of concerns.

### Risk: `From`/`Into` Boilerplate

Conversion functions are repetitive field-by-field copies, especially for the three overlapping structs (`PackageData`, `Package`, `ResolvedPackage` with ~30 shared fields).

**Mitigation**:

- Consider a shared `PackageFields` struct for the common fields, with `Deref`/`DerefMut` or a macro to reduce duplication
- The existing `from_package_data()` methods are already field-by-field copies, so this is not a new problem

### Risk: `TryFrom` Error Handling Complexity

The reverse conversion (`OutputType` → `InternalType`) is fallible, which means the `--from-json` path must handle conversion errors. Currently, deserialization into internal types fails at the serde level (e.g., zero-valued `LineNumber`). After separation, failures happen at the conversion layer with more context (e.g., "invalid start_line: 0 in OutputCopyright at index 5").

**Mitigation**:

- The `TryFrom` error messages should include enough context (field name, struct type, index) to make debugging easy
- Consider a `ConversionError` type that accumulates multiple errors rather than failing on the first one

## Resolved Questions

1. **Reverse conversion scope**: Only implement `OutputType → InternalType` (`TryFrom`) for types actually used in the `--from-json` path: `FileInfo`, `Package`, `TopLevelDependency`, `TopLevelLicenseDetection`, `LicenseReference`, `LicenseRuleReference`, `DatasourceId`, `PackageType`, and digest types. Other output types (like `Summary`, `Header`) are write-only — they are recomputed from the deserialized data, not loaded from JSON.

2. **`FileInfo` builder pattern**: The output `OutputFileInfo` does not need a builder. It is only constructed via `From<&FileInfo>`. The internal `FileInfo` keeps `derive_builder`.

3. **Internal `FileType` representation**: Reuse the `FileType` enum directly in output types (as `FileType`, not widened to `String`). The enum has only two variants (`File`, `Directory`) and is a natural domain type. The custom `Serialize`/`Deserialize` impls stay on the shared enum.

4. **Phase ordering for `PackageData`**: Conversion happens naturally during `FileInfo` → `OutputFileInfo` — the nested `Vec<PackageData>` converts to `Vec<OutputPackageData>` as part of that per-file conversion. There is no separate batch-conversion step.

5. **Error handling in `TryFrom` conversions for `--from-json`**: Fail the entire load on conversion errors — silent data loss is worse than a clear error. The `--from-json` path is for re-processing previous output, which should always be valid.

6. **`--from-json` handling of unknown `DatasourceId`/`PackageType` variants**: Reuse the `DatasourceId` and `PackageType` enums directly in output types (not widened to `String`). The enums stay as the canonical representation in both internal and output types. Backward-compat aliases (`nuget_nupsec`, `rpm_spefile`) remain as `#[serde(alias)]` on the shared enums. Future unknown variants will be handled via serde attributes on the enums if needed.

## Open Questions

None — all questions resolved. The plan is ready for implementation.

## Implementation Status

### Completed Phases (0–6)

All phases have been implemented on the `serde-separation` branch:

**Phase 0**: Removed unused `Serialize` from `AssemblyResult`, `CopyrightDetection`, `HolderDetection`, `AuthorDetection`. Created `src/output_schema/mod.rs`.

**Phase 1**: Created output schema types for leaf types: `OutputCopyright`, `OutputHolder`, `OutputAuthor`, `OutputEmail`, `OutputURL`, `OutputLicensePolicyEntry`, `OutputLicenseClarityScore`, `OutputTallyEntry`, `OutputTallies`, `OutputFacetTallies`, `OutputExtraData`, `OutputSystemEnvironment`. All have `From<&InternalType>` conversions; Group A types (used in --from-json) also have `TryFrom<&OutputType>`.

**Phase 2–3**: Created structured types: `OutputParty`, `OutputFileReference`, `OutputMatch`, `OutputLicenseDetection`, `OutputDependency`, `OutputResolvedPackage`.

**Phase 4**: Created core types: `OutputPackageData`, `OutputPackage`, `OutputFileInfo` (with custom `Serialize` impl replicating the info-surface gating logic). Shared utilities in `serde_helpers.rs`.

**Phase 5**: Created output containers: `OutputTopLevelDependency`, `OutputTopLevelLicenseDetection`, `OutputSummary`, `OutputHeader`, `OutputLicenseReference`, `OutputLicenseRuleReference`, `Output` (top-level). `OUTPUT_FORMAT_VERSION` moved to `output_schema`.

**Phase 6**: Refactored `--from-json` path. `JsonScanInput` now deserializes into output_schema types (`Vec<OutputFileInfo>`, `Vec<OutputPackage>`, etc.). `into_parts()` returns `Result` with `TryFrom` conversions. Output_schema-specific normalization functions (`normalize_output_paths`, `normalize_output_match_paths`, `normalize_output_top_level_paths`) handle path normalization on output types before conversion.

**Cutover wiring**: Updated all format writers (`output/mod.rs`, `jsonl.rs`, `template.rs`, `debian.rs`, `cyclonedx.rs`, `html.rs`, `spdx.rs`, `shared.rs`) to use `output_schema::Output` and output types. `main.rs` converts `models::Output` → `output_schema::Output` at the serialization boundary. `create_output()` still returns `models::Output` with internal types.

### Key Constraint: Cache and Scanner Require Serde on Internal Types

During Phase 6 implementation, removing `Serialize` from internal types broke the cache (`src/cache/incremental.rs`) and scanner spill-to-disk (`src/scanner/process.rs`) subsystems. Both use `rmp_serde` (MessagePack) to persist `FileInfo` and its field types. This means:

- **Internal types in `models/file_info.rs` retain `Serialize` and `Deserialize`** for non-JSON persistence (MessagePack cache, spill-to-disk)
- **JSON output path is fully separated** — all JSON serialization goes through `output_schema` types exclusively
- **`--from-json` path is fully separated** — deserialization goes through `output_schema` types, then `TryFrom` converts to internal types
- **The custom `Serialize` impl on internal `FileInfo` was removed** — it only served JSON output, which is now handled by `OutputFileInfo`
- **`serialize_optional_map_as_object` was removed from `file_info.rs`** — moved to `output_schema/serde_helpers.rs`
- **`should_serialize_info_surface()` was removed from internal `FileInfo`** — now lives on `OutputFileInfo`
- **Serde attributes like `skip_serializing_if` and `rename` remain on internal types** — they're needed for MessagePack serialization and deserialization in the --from-json path (though --from-json now uses output_schema types, the `Deserialize` is still present on internal types for MessagePack)

### Remaining Serde on Internal Types

Internal types that still carry serde derives:

| Type                            | `Serialize` | `Deserialize` | Reason                                    |
| ------------------------------- | :---------: | :-----------: | ----------------------------------------- |
| `FileInfo`                      |      ✓      |       ✓       | MessagePack cache, spill-to-disk          |
| `PackageData`                   |      ✓      |       ✓       | Nested in `FileInfo`                      |
| `Package`                       |      ✓      |       ✓       | Nested in `FileInfo` (via `PackageData`)  |
| `ResolvedPackage`               |      ✓      |       ✓       | Nested in `FileInfo` (via `PackageData`)  |
| `Dependency`                    |      ✓      |       ✓       | Nested in `FileInfo` (via `PackageData`)  |
| `TopLevelDependency`            |      ✓      |       ✓       | Nested in `Output`                        |
| `LicenseDetection`              |      ✓      |       ✓       | Nested in `FileInfo`                      |
| `Match`                         |      ✓      |       ✓       | Nested in `FileInfo`                      |
| `Copyright`, `Holder`, `Author` |      ✓      |       ✓       | Nested in `FileInfo`                      |
| `Party`, `FileReference`        |      ✓      |       ✓       | Nested in `FileInfo` (via `PackageData`)  |
| `OutputEmail`, `OutputURL`      |      ✓      |       ✓       | Nested in `FileInfo`                      |
| `LicensePolicyEntry`            |      ✓      |       ✓       | Nested in `FileInfo`                      |
| `FileType`                      | ✓ (custom)  |  ✓ (custom)   | Shared enum, used in MessagePack          |
| `DatasourceId`                  |      ✓      |       ✓       | Shared enum                               |
| `PackageType`                   |      ✓      |       ✓       | Shared enum                               |
| Digest types                    | ✓ (custom)  |  ✓ (custom)   | Nested in `FileInfo`, used in MessagePack |
| `LineNumber`                    |      ✓      |       ✓       | Nested in `FileInfo`, used in MessagePack |
| `Tallies`, `TallyEntry`         |      ✓      |       ✓       | Nested in `FileInfo`                      |
| `TopLevelLicenseDetection`      |      —      |       ✓       | --from-json path (now uses output_schema) |
| `LicenseReference`              |      —      |       ✓       | --from-json path (now uses output_schema) |
| `LicenseRuleReference`          |      —      |       ✓       | --from-json path (now uses output_schema) |

### Verified

- `cargo check` — passes
- `cargo clippy` — passes
- `cargo test --lib` — 442 tests pass
- `--from-json` round-trip — verified identical JSON output (excluding timestamps) for both simple and complex scans

## Appendix A: Cache Types

`src/cache/incremental.rs` has three types with `Serialize`/`Deserialize`:

- `FileStateFingerprint`
- `IncrementalManifestEntry` (contains `FileInfo` — creates a dependency on `FileInfo`'s serde)
- `IncrementalManifest`

These use serde for MessagePack persistence (`rmp_serde`), not JSON output. Additionally, `src/scanner/process.rs` uses `serde_json::to_vec` for spill-to-disk serialization of `FileInfo` batches.

**Implementation finding**: Removing `Serialize` from internal types broke both the cache and scanner subsystems. Since MessagePack is a serde format, `FileInfo` and all its nested types must retain `Serialize`/`Deserialize` as long as the cache uses MessagePack.

**Current state**: Internal types retain serde for MessagePack. The JSON output path is fully separated through `output_schema` types. The serde on internal types is now solely for non-JSON persistence.

**Future options** for fully removing serde from internal types:

1. **Replace MessagePack with a custom binary format** — e.g., `bincode` without serde derive, or a hand-written binary serializer. This is the cleanest but most work.
2. **Use `serde::Serialize`/`Deserialize` impls without derive** — write custom MessagePack-specific impls for internal types that don't require derive macros on the types themselves. This keeps the serde trait implementations but removes the attribute coupling.
3. **Introduce cache-specific schema types** — define separate types for cache serialization, similar to how `output_schema` handles JSON. This duplicates types but cleanly separates concerns.
4. **Accept the current state** — internal types keep serde derives, but JSON-specific attributes (`skip_serializing_if`, `rename` for JSON compat, `serialize_with`) are progressively removed since they're no longer needed for JSON output.

Option 4 is the pragmatic near-term choice. Options 1–3 are available for a future PR if full serde removal becomes a priority.

## Appendix B: License Detection Internal Types

`src/license_detection/` has ~15 types with serde. These fall into two sub-categories:

1. **Embedded index artifact types** (`Rule`, `LoadedRule`, `LoadedLicense`, `License`, `TokenId`, `TokenKind`, `KnownToken`, `QueryToken`, `TokenMetadata`, `EmbeddedLoaderSnapshot`): These use serde for binary deserialization of the embedded license index at load time. They are not part of the JSON output schema. They should keep serde but be clearly documented as "internal binary serialization, not output."

2. **Match result types** (`LicenseMatch`, `MatcherKind`): `LicenseMatch` has a custom `Serialize` via `SerializableLicenseMatch` helper. `MatcherKind` has serde renames. These are used to construct the `Match` output type. After separation, the conversion from `LicenseMatch` → `OutputMatch` will be explicit.

3. **File-parsing types** (`LicenseFrontmatter`, `RuleFrontmatter`): Private `Deserialize`-only types for parsing rule YAML files. These are already internal-only and don't need changes.

This subsystem should be addressed in a separate PR after the main models separation, since the license detection types have their own internal serialization format (embedded binary index) that's distinct from the JSON output schema.
