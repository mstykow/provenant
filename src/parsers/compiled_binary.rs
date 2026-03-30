use std::fs;
use std::io::Read;
use std::path::Path;

use flate2::read::ZlibDecoder;
use object::{Object, ObjectSection};
use packageurl::PackageUrl;
use serde::Deserialize;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};
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

pub(crate) fn try_parse_compiled_file(path: &Path) -> Option<ParsePackagesResult> {
    let mut packages = parse_rust_binary(path);
    packages.extend(parse_go_binary(path));

    (!packages.is_empty()).then_some(ParsePackagesResult {
        packages,
        scan_errors: Vec::new(),
    })
}

fn parse_rust_binary(path: &Path) -> Vec<PackageData> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return Vec::new(),
    };

    let object = match object::File::parse(&*bytes) {
        Ok(object) => object,
        Err(_) => return Vec::new(),
    };
    let Some(section) = object.section_by_name(".dep-v0") else {
        return Vec::new();
    };
    let Ok(compressed) = section.data() else {
        return Vec::new();
    };

    let mut decoder = ZlibDecoder::new(compressed);
    let mut decoded = Vec::new();
    if decoder.read_to_end(&mut decoded).is_err() || decoded.len() > MAX_RUST_AUDIT_JSON_SIZE {
        return Vec::new();
    }

    let audit_data: RustBinaryAuditData = match serde_json::from_slice(&decoded) {
        Ok(data) => data,
        Err(_) => return Vec::new(),
    };

    audit_data
        .packages
        .iter()
        .map(|package| build_rust_binary_package(package, &audit_data.packages))
        .collect()
}

fn build_rust_binary_package(
    package: &RustBinaryAuditPackage,
    packages: &[RustBinaryAuditPackage],
) -> PackageData {
    let purl = create_cargo_purl(&package.name, &package.version);
    let dependencies = package
        .dependencies
        .iter()
        .filter_map(|index| packages.get(*index))
        .filter_map(|dependency| {
            let purl = create_cargo_purl(&dependency.name, &dependency.version)?;
            Some(Dependency {
                purl: Some(purl),
                extracted_requirement: Some(dependency.version.clone()),
                scope: dependency.kind.clone(),
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
        name: Some(package.name.clone()),
        version: Some(package.version.clone()),
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

fn parse_go_binary(path: &Path) -> Vec<PackageData> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return Vec::new(),
    };
    let Some(header_offset) = find_aligned_magic(&bytes) else {
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
    let Some((_go_version, modinfo)) = decode_go_build_info_inline(&bytes, header_offset) else {
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
        String::from_utf8(modinfo[16..modinfo.len() - 16].to_vec()).ok()?
    } else {
        String::new()
    };

    Some((go_version, modinfo))
}

fn decode_varint_string(bytes: &[u8]) -> Option<(String, &[u8])> {
    let (length, consumed) = decode_uvarint(bytes)?;
    let start = consumed;
    let end = start.checked_add(length)?;
    let value = std::str::from_utf8(bytes.get(start..end)?)
        .ok()?
        .to_string();
    Some((value, &bytes[end..]))
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
    for line in modinfo.lines() {
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
        Some((namespace, name)) => (Some(namespace.to_string()), name.to_string()),
        None => (None, module_path.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    #[test]
    fn parse_rust_binary_reads_cargo_auditable_packages() {
        let path = Path::new(
            "reference/scancode-toolkit/tests/packagedcode/data/cargo/binary/cargo_dependencies",
        );

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
    fn parse_go_binary_reads_module_path_from_go_tooling() {
        if Command::new("go").arg("version").output().is_err() {
            return;
        }

        let temp = tempfile::tempdir().expect("temp dir");
        fs::write(
            temp.path().join("go.mod"),
            "module example.com/demo\n\ngo 1.23.0\n",
        )
        .expect("write go.mod");
        fs::write(
            temp.path().join("main.go"),
            "package main\nfunc main() {}\n",
        )
        .expect("write main.go");
        let binary = temp.path().join("demo");
        let status = Command::new("go")
            .current_dir(temp.path())
            .args(["build", "-o"])
            .arg(&binary)
            .status()
            .expect("run go build");
        assert!(status.success());

        let packages = parse_go_binary(&binary);

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
}
