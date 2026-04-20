use std::io::Read;
#[cfg(test)]
use std::path::Path;

use flate2::read::ZlibDecoder;
use object::{Object, ObjectSection};
use packageurl::PackageUrl;
use serde::Deserialize;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};
use crate::parser_warn as warn;
use crate::parsers::utils::{MAX_ITERATION_COUNT, truncate_field};
use crate::register_parser;

use super::ParsePackagesResult;
use super::go::create_golang_purl;

register_parser!(
    "Go compiled binary with embedded build info",
    &["<compiled Go binaries with Go build info>"],
    "golang",
    "Go",
    Some("https://pkg.go.dev/runtime/debug#BuildInfo"),
);

register_parser!(
    "Rust compiled binary with cargo-auditable dependency section",
    &["<compiled Rust binaries with .dep-v0 sections>"],
    "cargo",
    "Rust",
    Some("https://github.com/rust-secure-code/cargo-auditable"),
);

#[derive(Debug, Deserialize)]
struct RustBinaryAuditData {
    packages: Vec<RustBinaryAuditPackage>,
}

#[derive(Debug, Deserialize)]
struct RustBinaryAuditPackage {
    name: String,
    version: String,
    source: String,
    #[serde(default)]
    dependencies: Vec<usize>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    root: bool,
}

const MAX_RUST_AUDIT_JSON_SIZE: usize = 8 * 1024 * 1024;
const GO_BUILD_INFO_MAGIC: &[u8] = b"\xff Go buildinf:";
const GO_BUILD_INFO_ALIGN: usize = 16;
const GO_BUILD_INFO_HEADER_SIZE: usize = 32;

const ELF_MAGIC: &[u8] = b"\x7FELF";
const PE_MAGIC: &[u8] = b"MZ";
const MACHO_MAGICS: &[[u8; 4]] = &[
    [0xFE, 0xED, 0xFA, 0xCE],
    [0xCE, 0xFA, 0xED, 0xFE],
    [0xFE, 0xED, 0xFA, 0xCF],
    [0xCF, 0xFA, 0xED, 0xFE],
    [0xCA, 0xFE, 0xBA, 0xBE],
    [0xBE, 0xBA, 0xFE, 0xCA],
    [0xCA, 0xFE, 0xBA, 0xBF],
    [0xBF, 0xBA, 0xFE, 0xCA],
];

pub(crate) fn is_supported_compiled_binary_format(bytes: &[u8]) -> bool {
    bytes.starts_with(ELF_MAGIC)
        || bytes.starts_with(PE_MAGIC)
        || MACHO_MAGICS.iter().any(|magic| bytes.starts_with(magic))
}

pub(crate) fn try_parse_compiled_bytes(bytes: &[u8]) -> Option<ParsePackagesResult> {
    let mut packages = parse_rust_binary_bytes(bytes);
    packages.extend(parse_go_binary_bytes(bytes));

    (!packages.is_empty()).then_some(ParsePackagesResult {
        packages,
        scan_diagnostics: Vec::new(),
        scan_errors: Vec::new(),
    })
}

#[cfg(test)]
fn parse_rust_binary(path: &Path) -> Vec<PackageData> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return Vec::new(),
    };

    parse_rust_binary_bytes(&bytes)
}

fn parse_rust_binary_bytes(bytes: &[u8]) -> Vec<PackageData> {
    let object = match object::File::parse(bytes) {
        Ok(object) => object,
        Err(_) => return Vec::new(),
    };
    let Some(section) = object.section_by_name(".dep-v0") else {
        return Vec::new();
    };
    let Ok(compressed) = section.data() else {
        return Vec::new();
    };

    let Some(audit_data) = decode_rust_audit_data(compressed) else {
        return Vec::new();
    };

    audit_data
        .packages
        .iter()
        .take(MAX_ITERATION_COUNT)
        .map(|package| build_rust_binary_package(package, &audit_data.packages))
        .collect()
}

fn decode_rust_audit_data(compressed: &[u8]) -> Option<RustBinaryAuditData> {
    let decoder = ZlibDecoder::new(compressed);
    let mut decoded = Vec::new();
    let mut limited = decoder.take((MAX_RUST_AUDIT_JSON_SIZE as u64) + 1);
    if limited.read_to_end(&mut decoded).is_err() || decoded.len() > MAX_RUST_AUDIT_JSON_SIZE {
        return None;
    };

    serde_json::from_slice(&decoded).ok()
}

fn build_rust_binary_package(
    package: &RustBinaryAuditPackage,
    packages: &[RustBinaryAuditPackage],
) -> PackageData {
    let purl = create_cargo_purl(&package.name, &package.version);
    let dependencies = package
        .dependencies
        .iter()
        .take(MAX_ITERATION_COUNT)
        .filter_map(|index| packages.get(*index))
        .filter_map(|dependency| {
            let purl = create_cargo_purl(&dependency.name, &dependency.version)?;
            Some(Dependency {
                purl: Some(purl),
                extracted_requirement: Some(truncate_field(dependency.version.clone())),
                scope: dependency.kind.as_ref().map(|k| truncate_field(k.clone())),
                is_runtime: Some(dependency.kind.as_deref() != Some("build")),
                is_optional: Some(false),
                is_pinned: Some(true),
                is_direct: Some(true),
                resolved_package: None,
                extra_data: None,
            })
        })
        .collect();
    let is_private = package.source == "local" || package.root;
    let (repository_homepage_url, repository_download_url, api_data_url) =
        if package.source == "crates.io" {
            (
                Some(format!("https://crates.io/crates/{}", package.name)),
                Some(format!(
                    "https://crates.io/api/v1/crates/{}/{}/download",
                    package.name, package.version
                )),
                Some(format!("https://crates.io/api/v1/crates/{}", package.name)),
            )
        } else {
            (None, None, None)
        };

    PackageData {
        package_type: Some(PackageType::Cargo),
        datasource_id: Some(DatasourceId::RustBinary),
        primary_language: Some("Rust".to_string()),
        name: Some(truncate_field(package.name.clone())),
        version: Some(truncate_field(package.version.clone())),
        is_private,
        repository_homepage_url,
        repository_download_url,
        api_data_url,
        purl,
        dependencies,
        file_references: Vec::new(),
        extra_data: None,
        size: None,
        sha1: None,
        md5: None,
        sha256: None,
        ..Default::default()
    }
}

fn create_cargo_purl(name: &str, version: &str) -> Option<String> {
    let mut purl = PackageUrl::new(PackageType::Cargo.as_str(), name).ok()?;
    purl.with_version(version).ok()?;
    Some(purl.to_string())
}

#[cfg(test)]
fn parse_go_binary(path: &Path) -> Vec<PackageData> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return Vec::new(),
    };

    parse_go_binary_bytes(&bytes)
}

fn parse_go_binary_bytes(bytes: &[u8]) -> Vec<PackageData> {
    let Some(header_offset) = find_aligned_magic(bytes) else {
        return Vec::new();
    };
    let header_end = match header_offset.checked_add(GO_BUILD_INFO_HEADER_SIZE) {
        Some(end) if end <= bytes.len() => end,
        _ => return Vec::new(),
    };
    let header = &bytes[header_offset..header_end];
    if header.get(15).copied().unwrap_or_default() & 0x2 == 0 {
        return Vec::new();
    }
    let Some((_go_version, modinfo)) = decode_go_build_info_inline(bytes, header_offset) else {
        return Vec::new();
    };

    parse_go_modinfo_packages(&modinfo)
}

fn find_aligned_magic(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(GO_BUILD_INFO_MAGIC.len())
        .enumerate()
        .find_map(|(offset, window)| {
            (offset % GO_BUILD_INFO_ALIGN == 0 && window == GO_BUILD_INFO_MAGIC).then_some(offset)
        })
}

fn decode_go_build_info_inline(bytes: &[u8], header_offset: usize) -> Option<(String, String)> {
    let payload = bytes.get(header_offset + GO_BUILD_INFO_HEADER_SIZE..)?;
    let (go_version, payload) = decode_varint_string(payload)?;
    let (modinfo, _) = decode_varint_bytes(payload)?;

    let modinfo = if modinfo.len() >= 33 && modinfo.get(modinfo.len() - 17) == Some(&b'\n') {
        let slice = &modinfo[16..modinfo.len() - 16];
        let lossy = String::from_utf8_lossy(slice);
        if lossy.is_ascii() {
            lossy.into_owned()
        } else {
            warn!("invalid UTF-8 in Go build info modinfo");
            lossy.into_owned()
        }
    } else {
        String::new()
    };

    Some((go_version, modinfo))
}

fn decode_varint_string(bytes: &[u8]) -> Option<(String, &[u8])> {
    let (length, consumed) = decode_uvarint(bytes)?;
    let start = consumed;
    let end = start.checked_add(length)?;
    let raw = bytes.get(start..end)?;
    let lossy = String::from_utf8_lossy(raw);
    if !raw.is_ascii() {
        warn!("invalid UTF-8 in varint string field");
    }
    Some((lossy.into_owned(), &bytes[end..]))
}

fn decode_varint_bytes(bytes: &[u8]) -> Option<(Vec<u8>, &[u8])> {
    let (length, consumed) = decode_uvarint(bytes)?;
    let start = consumed;
    let end = start.checked_add(length)?;
    Some((bytes.get(start..end)?.to_vec(), &bytes[end..]))
}

fn decode_uvarint(bytes: &[u8]) -> Option<(usize, usize)> {
    let mut value = 0usize;
    let mut shift = 0usize;

    for (index, byte) in bytes.iter().copied().enumerate() {
        value |= usize::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some((value, index + 1));
        }
        shift += 7;
        if shift >= usize::BITS as usize {
            return None;
        }
    }

    None
}

fn parse_go_modinfo_packages(modinfo: &str) -> Vec<PackageData> {
    let mut packages = Vec::new();
    for line in modinfo.lines().take(MAX_ITERATION_COUNT) {
        if let Some(rest) = line.strip_prefix("path\t") {
            packages.push(build_go_binary_package(rest, None, true));
            continue;
        }
        if let Some(rest) = line.strip_prefix("mod\t") {
            if let Some((module_path, version)) = parse_go_module_line(rest) {
                packages.push(build_go_binary_package(module_path, Some(version), false));
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("dep\t")
            && let Some((module_path, version)) = parse_go_module_line(rest)
        {
            packages.push(build_go_binary_package(module_path, Some(version), false));
        }
    }
    packages
}

fn parse_go_module_line(line: &str) -> Option<(&str, String)> {
    let mut parts = line.split('\t');
    let module_path = parts.next()?;
    let version = parts.next()?.to_string();
    Some((module_path, version))
}

fn build_go_binary_package(
    module_path: &str,
    version: Option<String>,
    is_private: bool,
) -> PackageData {
    let (namespace, name) = split_module_path(module_path);
    let name = truncate_field(name);
    let version = version.map(truncate_field);
    let repository_homepage_url = Some(format!("https://pkg.go.dev/{module_path}"));
    let purl = create_golang_purl(module_path, version.as_deref());

    PackageData {
        package_type: Some(PackageType::Golang),
        datasource_id: Some(DatasourceId::GoBinary),
        primary_language: Some("Go".to_string()),
        namespace,
        name: Some(name),
        version,
        homepage_url: repository_homepage_url.clone(),
        repository_homepage_url,
        purl,
        is_private,
        ..Default::default()
    }
}

fn split_module_path(module_path: &str) -> (Option<String>, String) {
    match module_path.rsplit_once('/') {
        Some((namespace, name)) => (
            Some(truncate_field(namespace.to_string())),
            name.to_string(),
        ),
        None => (None, module_path.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use flate2::Compression;
    use flate2::write::ZlibEncoder;

    use super::*;

    const RUST_COMPILED_BINARY_FIXTURE: &str =
        "testdata/compiled-binary-golden/rust/cargo_dependencies";
    const GO_COMPILED_BINARY_FIXTURE: &str = "testdata/compiled-binary-golden/go-basic/demo";

    #[test]
    fn parse_rust_binary_reads_cargo_auditable_packages() {
        let path = Path::new(RUST_COMPILED_BINARY_FIXTURE);
        assert!(path.exists(), "missing fixture: {}", path.display());

        let packages = parse_rust_binary(path);

        assert!(!packages.is_empty());
        assert!(
            packages
                .iter()
                .any(|pkg| pkg.name.as_deref() == Some("aho-corasick"))
        );
        assert!(
            packages
                .iter()
                .any(|pkg| pkg.name.as_deref() == Some("cargo_dependencies"))
        );
        assert!(
            packages
                .iter()
                .all(|pkg| pkg.datasource_id == Some(DatasourceId::RustBinary))
        );
    }

    #[test]
    fn parse_go_binary_reads_module_path_from_fixture() {
        let path = Path::new(GO_COMPILED_BINARY_FIXTURE);
        assert!(path.exists(), "missing fixture: {}", path.display());

        let packages = parse_go_binary(path);

        assert!(
            packages
                .iter()
                .any(|pkg| pkg.name.as_deref() == Some("demo"))
        );
        assert!(
            packages
                .iter()
                .all(|pkg| pkg.datasource_id == Some(DatasourceId::GoBinary))
        );
    }

    #[test]
    fn decode_rust_audit_data_rejects_oversized_payloads() {
        let oversized_json = format!(
            "{{\"packages\":[{{\"name\":\"pkg\",\"version\":\"1.0.0\",\"source\":\"crates.io\",\"dependencies\":[],\"padding\":\"{}\"}}]}}",
            "a".repeat(MAX_RUST_AUDIT_JSON_SIZE)
        );
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        std::io::Write::write_all(&mut encoder, oversized_json.as_bytes())
            .expect("write oversized audit payload");
        let compressed = encoder.finish().expect("finish audit payload compression");

        assert!(decode_rust_audit_data(&compressed).is_none());
    }

    #[test]
    fn detects_supported_compiled_binary_formats_by_magic() {
        assert!(is_supported_compiled_binary_format(b"\x7FELF\x02\x01"));
        assert!(is_supported_compiled_binary_format(b"MZ\x90\x00"));
        assert!(is_supported_compiled_binary_format(&[
            0xFE, 0xED, 0xFA, 0xCF, 0x00
        ]));
        assert!(!is_supported_compiled_binary_format(
            b"#!/usr/bin/env bash\n"
        ));
        assert!(!is_supported_compiled_binary_format(b"{\"name\":\"demo\"}"));
    }
}
