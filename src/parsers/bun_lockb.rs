use std::collections::HashMap;
use std::path::Path;

use crate::parser_warn as warn;
use base64::Engine;
use serde_json::Value as JsonValue;

use crate::models::{
    DatasourceId, Dependency, PackageData, PackageType, ResolvedPackage, Sha512Digest,
};
use crate::parsers::utils::{npm_purl, parse_sri};

use super::PackageParser;

pub struct BunLockbParser;

const HEADER_BYTES: &[u8] = b"#!/usr/bin/env bun\nbun-lockfile-format-v0\n";
const SUPPORTED_FORMAT_VERSION: u32 = 2;
const FIELD_COUNT_WITHOUT_SCRIPTS: usize = 7;
const FIELD_COUNT_WITH_SCRIPTS: usize = 8;
const PACKAGE_FIELD_LENGTHS: [usize; 8] = [8, 8, 64, 8, 8, 88, 20, 48];
const DEPENDENCY_ENTRY_SIZE: usize = 26;

#[derive(Clone, Copy)]
struct SliceRef {
    off: usize,
    len: usize,
}

#[derive(Clone)]
struct BunLockbPackage {
    name_ref: [u8; 8],
    name: String,
    resolution_raw: [u8; 64],
    resolution: BunLockbResolution,
    dependencies: SliceRef,
    resolutions: SliceRef,
    integrity: Option<String>,
}

#[derive(Clone)]
struct BunLockbResolution {
    version: Option<String>,
    resolved: Option<String>,
}

#[derive(Clone)]
struct BunLockbDependencyEntry {
    name: String,
    literal: String,
    behavior: u8,
}

struct BunLockbBuffers<'a> {
    resolutions: &'a [u8],
    dependencies: &'a [u8],
    string_bytes: &'a [u8],
}

struct LockbCursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl PackageParser for BunLockbParser {
    const PACKAGE_TYPE: PackageType = PackageType::Npm;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "bun.lockb")
            && !path.with_file_name("bun.lock").exists()
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) => {
                warn!("Failed to read bun.lockb at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        match parse_bun_lockb(&bytes) {
            Ok(package_data) => vec![package_data],
            Err(e) => {
                warn!("Failed to parse bun.lockb at {:?}: {}", path, e);
                vec![default_package_data()]
            }
        }
    }
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(BunLockbParser::PACKAGE_TYPE),
        primary_language: Some("JavaScript".to_string()),
        datasource_id: Some(DatasourceId::BunLockb),
        extra_data: Some(HashMap::new()),
        ..Default::default()
    }
}

pub(crate) fn parse_bun_lockb(bytes: &[u8]) -> Result<PackageData, String> {
    let mut cursor = LockbCursor::new(bytes);
    cursor.expect_bytes(HEADER_BYTES)?;

    let format_version = cursor.read_u32()?;
    if format_version != SUPPORTED_FORMAT_VERSION {
        return Err(format!(
            "Unsupported bun.lockb format version {} (supported: {})",
            format_version, SUPPORTED_FORMAT_VERSION
        ));
    }

    let meta_hash = cursor.read_bytes(32)?;
    let total_buffer_size = cursor.read_u64()? as usize;
    if total_buffer_size > bytes.len() {
        return Err("Lockfile is missing data".to_string());
    }

    let list_len = cursor.read_u64()? as usize;
    let input_alignment = cursor.read_u64()?;
    if input_alignment != 8 {
        return Err(format!(
            "Unexpected bun.lockb package alignment {}",
            input_alignment
        ));
    }

    let field_count = cursor.read_u64()? as usize;
    if field_count != FIELD_COUNT_WITHOUT_SCRIPTS && field_count != FIELD_COUNT_WITH_SCRIPTS {
        return Err(format!(
            "Unexpected bun.lockb package field count {} (supported: {} or {})",
            field_count, FIELD_COUNT_WITHOUT_SCRIPTS, FIELD_COUNT_WITH_SCRIPTS
        ));
    }

    let packages_begin = cursor.read_u64()? as usize;
    let packages_end = cursor.read_u64()? as usize;
    if packages_begin > total_buffer_size
        || packages_end > total_buffer_size
        || packages_begin > packages_end
    {
        return Err("Invalid bun.lockb package section bounds".to_string());
    }

    let mut packages = parse_packages(bytes, list_len, field_count, packages_begin, packages_end)?;
    cursor.pos = packages_end;
    let buffers = parse_buffers(bytes, &mut cursor, total_buffer_size)?;
    materialize_packages(&mut packages, buffers.string_bytes)?;

    build_package_data_from_lockb(format_version, meta_hash, &packages, &buffers)
}

fn parse_packages(
    bytes: &[u8],
    list_len: usize,
    field_count: usize,
    packages_begin: usize,
    packages_end: usize,
) -> Result<Vec<BunLockbPackage>, String> {
    let mut packages = vec![
        BunLockbPackage {
            name_ref: [0; 8],
            name: String::new(),
            resolution_raw: [0; 64],
            resolution: BunLockbResolution {
                version: None,
                resolved: None,
            },
            dependencies: SliceRef { off: 0, len: 0 },
            resolutions: SliceRef { off: 0, len: 0 },
            integrity: None,
        };
        list_len
    ];

    let package_region = bytes
        .get(packages_begin..packages_end)
        .ok_or_else(|| "Invalid bun.lockb package region".to_string())?;

    let expected_size: usize =
        PACKAGE_FIELD_LENGTHS[..field_count].iter().sum::<usize>() * list_len;
    if package_region.len() < expected_size {
        return Err("bun.lockb package region is truncated".to_string());
    }

    let mut field_offset = 0usize;

    for package in &mut packages {
        package
            .name_ref
            .copy_from_slice(&package_region[field_offset..field_offset + 8]);
        field_offset += 8;
    }

    field_offset += 8 * list_len;

    for package in &mut packages {
        package
            .resolution_raw
            .copy_from_slice(&package_region[field_offset..field_offset + 64]);
        field_offset += 64;
    }

    for package in &mut packages {
        package.dependencies = parse_slice_ref(&package_region[field_offset..field_offset + 8])?;
        field_offset += 8;
    }

    for package in &mut packages {
        package.resolutions = parse_slice_ref(&package_region[field_offset..field_offset + 8])?;
        field_offset += 8;
    }

    for package in &mut packages {
        package.integrity = parse_integrity(&package_region[field_offset + 20..field_offset + 85]);
        field_offset += 88;
    }

    field_offset += 20 * list_len;
    if field_count == FIELD_COUNT_WITH_SCRIPTS {
        field_offset += 48 * list_len;
    }

    if field_offset != expected_size {
        return Err("bun.lockb package region layout is malformed".to_string());
    }

    Ok(packages)
}

fn materialize_packages(
    packages: &mut [BunLockbPackage],
    string_bytes: &[u8],
) -> Result<(), String> {
    for package in packages {
        package.name = decode_bun_string(&package.name_ref, string_bytes)?;
        package.resolution = parse_resolution(&package.resolution_raw, string_bytes)?;
    }
    Ok(())
}

fn parse_buffers<'a>(
    bytes: &'a [u8],
    cursor: &mut LockbCursor<'a>,
    total_buffer_size: usize,
) -> Result<BunLockbBuffers<'a>, String> {
    let _trees = parse_buffer_range(bytes, cursor, total_buffer_size)?;
    let _hoisted_dependencies = parse_buffer_range(bytes, cursor, total_buffer_size)?;
    let resolutions = parse_buffer_range(bytes, cursor, total_buffer_size)?;
    let dependencies = parse_buffer_range(bytes, cursor, total_buffer_size)?;
    let _extern_strings = parse_buffer_range(bytes, cursor, total_buffer_size)?;
    let string_bytes = parse_buffer_range(bytes, cursor, total_buffer_size)?;

    Ok(BunLockbBuffers {
        resolutions,
        dependencies,
        string_bytes,
    })
}

fn parse_buffer_range<'a>(
    bytes: &'a [u8],
    cursor: &mut LockbCursor<'a>,
    total_buffer_size: usize,
) -> Result<&'a [u8], String> {
    let start = cursor.read_u64()? as usize;
    let end = cursor.read_u64()? as usize;
    if start > total_buffer_size || end > total_buffer_size || start > end {
        return Err("Invalid bun.lockb buffer range".to_string());
    }
    cursor.pos = start;
    let slice = cursor.read_bytes(end - start)?;
    cursor.pos = end;
    bytes
        .get(start..end)
        .or(Some(slice))
        .ok_or_else(|| "Invalid bun.lockb buffer slice".to_string())
}

fn build_package_data_from_lockb(
    format_version: u32,
    meta_hash: &[u8],
    packages: &[BunLockbPackage],
    buffers: &BunLockbBuffers<'_>,
) -> Result<PackageData, String> {
    let root_package = packages
        .first()
        .ok_or_else(|| "bun.lockb contains no packages".to_string())?;

    let mut package_data = default_package_data();
    package_data.name = Some(root_package.name.clone());
    package_data.purl = npm_purl(&root_package.name, None);

    let extra_data = package_data.extra_data.get_or_insert_with(HashMap::new);
    extra_data.insert(
        "lockfileVersion".to_string(),
        JsonValue::from(i64::from(format_version)),
    );
    extra_data.insert(
        "meta_hash".to_string(),
        JsonValue::from(encode_hex(meta_hash)),
    );

    let dependency_entries = parse_dependency_entries(buffers.dependencies, buffers.string_bytes)?;
    let resolution_ids = parse_resolution_ids(buffers.resolutions)?;

    package_data.dependencies = build_dependencies_for_package(
        root_package,
        packages,
        &dependency_entries,
        &resolution_ids,
        buffers.string_bytes,
        true,
    )?;

    Ok(package_data)
}

fn parse_dependency_entries(
    bytes: &[u8],
    string_bytes: &[u8],
) -> Result<Vec<BunLockbDependencyEntry>, String> {
    if !bytes.len().is_multiple_of(DEPENDENCY_ENTRY_SIZE) {
        return Err("bun.lockb dependency buffer is malformed".to_string());
    }

    bytes
        .chunks_exact(DEPENDENCY_ENTRY_SIZE)
        .map(|entry| {
            Ok(BunLockbDependencyEntry {
                name: decode_bun_string(&entry[0..8], string_bytes)?,
                behavior: entry[16],
                literal: decode_bun_string(&entry[18..26], string_bytes)?,
            })
        })
        .collect()
}

fn parse_resolution_ids(bytes: &[u8]) -> Result<Vec<u32>, String> {
    if !bytes.len().is_multiple_of(4) {
        return Err("bun.lockb resolution buffer is malformed".to_string());
    }

    bytes
        .chunks_exact(4)
        .map(|chunk| Ok(u32::from_le_bytes(chunk.try_into().unwrap())))
        .collect()
}

fn build_dependencies_for_package(
    package: &BunLockbPackage,
    packages: &[BunLockbPackage],
    dependency_entries: &[BunLockbDependencyEntry],
    resolution_ids: &[u32],
    string_bytes: &[u8],
    is_direct: bool,
) -> Result<Vec<Dependency>, String> {
    let dep_slice = dependency_entries
        .get(package.dependencies.off..package.dependencies.off + package.dependencies.len)
        .ok_or_else(|| "bun.lockb dependency slice is out of bounds".to_string())?;
    let res_slice = resolution_ids
        .get(package.resolutions.off..package.resolutions.off + package.resolutions.len)
        .ok_or_else(|| "bun.lockb resolution slice is out of bounds".to_string())?;

    dep_slice
        .iter()
        .zip(res_slice.iter())
        .map(|(entry, package_id)| {
            let manifest = behavior_to_manifest(entry.behavior);
            let resolved_package = if (*package_id as usize) < packages.len() {
                let resolved = &packages[*package_id as usize];
                Some(Box::new(build_resolved_package(
                    resolved,
                    packages,
                    dependency_entries,
                    resolution_ids,
                    string_bytes,
                )?))
            } else {
                None
            };

            let version = resolved_package
                .as_ref()
                .and_then(|pkg| (!pkg.version.is_empty()).then_some(pkg.version.as_str()));

            Ok(Dependency {
                purl: npm_purl(&entry.name, version),
                extracted_requirement: Some(entry.literal.clone()),
                scope: Some(manifest.scope.to_string()),
                is_runtime: Some(manifest.is_runtime),
                is_optional: Some(manifest.is_optional),
                is_pinned: version.map(|_| true).or(Some(false)),
                is_direct: Some(is_direct),
                resolved_package,
                extra_data: None,
            })
        })
        .collect()
}

fn build_resolved_package(
    package: &BunLockbPackage,
    packages: &[BunLockbPackage],
    dependency_entries: &[BunLockbDependencyEntry],
    resolution_ids: &[u32],
    string_bytes: &[u8],
) -> Result<ResolvedPackage, String> {
    let (namespace, name) = split_namespace_name(&package.name);

    Ok(ResolvedPackage {
        primary_language: Some("JavaScript".to_string()),
        download_url: package.resolution.resolved.clone(),
        sha1: None,
        sha256: None,
        sha512: package
            .integrity
            .as_ref()
            .and_then(|s| parse_sri(s).and_then(|(alg, hash)| (alg == "sha512").then_some(hash)))
            .and_then(|h| Sha512Digest::from_hex(&h).ok()),
        md5: None,
        is_virtual: true,
        extra_data: None,
        dependencies: build_dependencies_for_package(
            package,
            packages,
            dependency_entries,
            resolution_ids,
            string_bytes,
            false,
        )?,
        repository_homepage_url: None,
        repository_download_url: None,
        api_data_url: None,
        datasource_id: Some(DatasourceId::BunLockb),
        purl: None,
        ..ResolvedPackage::new(
            PackageType::Npm,
            namespace.unwrap_or_default(),
            name.unwrap_or_else(|| package.name.clone()),
            package.resolution.version.clone().unwrap_or_default(),
        )
    })
}

fn parse_slice_ref(bytes: &[u8]) -> Result<SliceRef, String> {
    if bytes.len() != 8 {
        return Err("Invalid bun.lockb slice length".to_string());
    }
    let off = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
    let len = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    Ok(SliceRef { off, len })
}

fn parse_resolution(bytes: &[u8], string_bytes: &[u8]) -> Result<BunLockbResolution, String> {
    if bytes.len() != 64 {
        return Err("Invalid bun.lockb resolution length".to_string());
    }

    let tag = bytes[0];
    match tag {
        1 => Ok(BunLockbResolution {
            version: None,
            resolved: Some(String::new()).filter(|s| !s.is_empty()),
        }),
        2 => {
            let resolved = decode_bun_string(&bytes[8..16], string_bytes)?;
            let major = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
            let minor = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
            let patch = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
            let tag_suffix = decode_version_suffix(&bytes[32..64], string_bytes)?;
            let version = if let Some(suffix) = tag_suffix {
                format!("{}.{}.{}{}", major, minor, patch, suffix)
            } else {
                format!("{}.{}.{}", major, minor, patch)
            };

            Ok(BunLockbResolution {
                version: Some(version),
                resolved: (!resolved.is_empty()).then_some(resolved),
            })
        }
        72 => {
            let workspace = decode_bun_string(&bytes[8..16], string_bytes)?;
            Ok(BunLockbResolution {
                version: None,
                resolved: Some(format!("workspace:{}", workspace)),
            })
        }
        4 | 8 | 16 | 24 | 32 | 64 | 80 | 100 => {
            let resolved = decode_bun_string(&bytes[8..16], string_bytes)?;
            Ok(BunLockbResolution {
                version: None,
                resolved: (!resolved.is_empty()).then_some(resolved),
            })
        }
        _ => Err(format!("Unsupported bun.lockb resolution tag {}", tag)),
    }
}

fn decode_version_suffix(bytes: &[u8], string_bytes: &[u8]) -> Result<Option<String>, String> {
    if bytes.len() != 32 {
        return Err("Invalid bun.lockb version tag length".to_string());
    }
    let pre = decode_bun_string(&bytes[0..8], string_bytes)?;
    let build = decode_bun_string(&bytes[16..24], string_bytes)?;

    let mut suffix = String::new();
    if !pre.is_empty() {
        suffix.push('-');
        suffix.push_str(&pre);
    }
    if !build.is_empty() {
        suffix.push('+');
        suffix.push_str(&build);
    }

    Ok((!suffix.is_empty()).then_some(suffix))
}

fn decode_bun_string(bytes: &[u8], string_bytes: &[u8]) -> Result<String, String> {
    if bytes.len() != 8 {
        return Err("Invalid bun.lockb string width".to_string());
    }

    if bytes[7] & 0x80 == 0 {
        let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
        return std::str::from_utf8(&bytes[..end])
            .map(|s| s.to_string())
            .map_err(|e| format!("Invalid inline bun.lockb UTF-8: {}", e));
    }

    let off = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
    let len_raw = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let len = (len_raw & 0x7fff_ffff) as usize;
    let slice = string_bytes
        .get(off..off + len)
        .ok_or_else(|| "bun.lockb string offset out of bounds".to_string())?;
    std::str::from_utf8(slice)
        .map(|s| s.to_string())
        .map_err(|e| format!("Invalid external bun.lockb UTF-8: {}", e))
}

fn parse_integrity(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }

    let algorithm = match bytes[0] {
        1 => "sha1",
        2 => "sha256",
        3 => "sha384",
        4 => "sha512",
        _ => return None,
    };

    Some(format!(
        "{}-{}",
        algorithm,
        base64::engine::general_purpose::STANDARD.encode(&bytes[1..])
    ))
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn split_namespace_name(full_name: &str) -> (Option<String>, Option<String>) {
    if full_name.starts_with('@') {
        let mut parts = full_name.splitn(2, '/');
        let namespace = parts.next().map(ToOwned::to_owned);
        let name = parts.next().map(ToOwned::to_owned);
        (namespace, name)
    } else {
        (Some(String::new()), Some(full_name.to_string()))
    }
}

struct ManifestBehavior {
    scope: &'static str,
    is_runtime: bool,
    is_optional: bool,
}

fn behavior_to_manifest(behavior: u8) -> ManifestBehavior {
    const NORMAL: u8 = 0b10;
    const OPTIONAL: u8 = 0b100;
    const DEV: u8 = 0b1000;
    const PEER: u8 = 0b1_0000;
    const WORKSPACE: u8 = 0b10_0000;

    if behavior & WORKSPACE != 0 {
        return ManifestBehavior {
            scope: "workspaces",
            is_runtime: false,
            is_optional: false,
        };
    }
    if behavior & DEV != 0 {
        return ManifestBehavior {
            scope: "devDependencies",
            is_runtime: false,
            is_optional: true,
        };
    }
    if behavior & PEER != 0 && behavior & OPTIONAL != 0 {
        return ManifestBehavior {
            scope: "peerDependencies",
            is_runtime: true,
            is_optional: true,
        };
    }
    if behavior & PEER != 0 {
        return ManifestBehavior {
            scope: "peerDependencies",
            is_runtime: true,
            is_optional: false,
        };
    }
    if behavior & OPTIONAL != 0 {
        return ManifestBehavior {
            scope: "optionalDependencies",
            is_runtime: true,
            is_optional: true,
        };
    }
    if behavior & NORMAL != 0 {
        return ManifestBehavior {
            scope: "dependencies",
            is_runtime: true,
            is_optional: false,
        };
    }

    ManifestBehavior {
        scope: "dependencies",
        is_runtime: true,
        is_optional: false,
    }
}

impl<'a> LockbCursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn read_bytes(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| "bun.lockb offset overflow".to_string())?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or_else(|| "bun.lockb is truncated".to_string())?;
        self.pos = end;
        Ok(slice)
    }

    fn expect_bytes(&mut self, expected: &[u8]) -> Result<(), String> {
        let actual = self.read_bytes(expected.len())?;
        if actual == expected {
            Ok(())
        } else {
            Err("Invalid bun.lockb header".to_string())
        }
    }

    fn read_u32(&mut self) -> Result<u32, String> {
        let bytes: [u8; 4] = self
            .read_bytes(4)?
            .try_into()
            .map_err(|_| "Invalid bun.lockb u32".to_string())?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn read_u64(&mut self) -> Result<u64, String> {
        let bytes: [u8; 8] = self
            .read_bytes(8)?
            .try_into()
            .map_err(|_| "Invalid bun.lockb u64".to_string())?;
        Ok(u64::from_le_bytes(bytes))
    }
}

crate::register_parser!(
    "Legacy Bun binary lockfile",
    &["**/bun.lockb"],
    "npm",
    "JavaScript",
    Some("https://bun.sh/docs/pm/lockfile"),
);
