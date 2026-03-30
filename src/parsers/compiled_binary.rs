use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::Command;

use flate2::read::ZlibDecoder;
use object::{Object, ObjectSection};
use packageurl::PackageUrl;
use serde::Deserialize;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};

use super::ParsePackagesResult;
use super::go::create_golang_purl;

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
    let output = match Command::new("go")
        .arg("version")
        .arg("-m")
        .arg(path)
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut packages = Vec::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("path\t") {
            packages.push(build_go_binary_package(rest, None, true));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("mod\t") {
            let mut parts = rest.split('\t');
            if let Some(module_path) = parts.next() {
                let version = parts.next().map(str::to_string);
                packages.push(build_go_binary_package(module_path, version, false));
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("dep\t") {
            let mut parts = rest.split('\t');
            if let Some(module_path) = parts.next() {
                let version = parts.next().map(str::to_string);
                packages.push(build_go_binary_package(module_path, version, false));
            }
        }
    }

    packages
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
