//! Parser for Cargo.lock lockfiles.
//!
//! Extracts resolved dependency information including exact versions and
//! checksums from Rust Cargo.lock files.
//!
//! # Supported Formats
//! - Cargo.lock (lockfile)
//!
//! # Key Features
//! - Exact version resolution from lockfile
//! - Direct vs transitive dependency tracking (`is_direct`)
//! - Checksum extraction for verification
//! - Package URL (purl) generation
//! - Dependency graph with source tracking (crates.io, git, path)
//!
//! # Implementation Notes
//! - All lockfile versions are pinned (`is_pinned: Some(true)`)
//! - Direct dependencies determined from root package's dependency list
//! - Uses TOML parsing for structured data extraction

use crate::models::{DatasourceId, Dependency, PackageData, PackageType, Sha256Digest};
use crate::parser_warn as warn;
use packageurl::PackageUrl;
use serde_json::json;
use std::collections::{HashMap, hash_map::Entry};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use toml::Value;

use super::PackageParser;

/// Rust Cargo.lock lockfile parser.
///
/// Extracts pinned dependency versions with checksums from Cargo-managed Rust projects.
pub struct CargoLockParser;

impl PackageParser for CargoLockParser {
    const PACKAGE_TYPE: PackageType = PackageType::Cargo;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("cargo.lock"))
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_cargo_lock(path) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read or parse Cargo.lock at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        let packages = match content.get("package").and_then(|v| v.as_array()) {
            Some(pkgs) => pkgs,
            None => {
                warn!("No 'package' array found in Cargo.lock at {:?}", path);
                return vec![default_package_data()];
            }
        };

        let root_package = select_root_package(packages);

        let name = root_package
            .and_then(|p| p.get("name"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let version = root_package
            .and_then(|p| p.get("version"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let checksum = root_package
            .and_then(|p| p.get("checksum"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let (sha256, extra_data) = match checksum.as_deref() {
            Some(h) if h.len() == 64 && Sha256Digest::from_hex(h).is_ok() => {
                (Sha256Digest::from_hex(h).ok(), None)
            }
            Some(h) if hex::decode(h).is_ok() => {
                let mut map = HashMap::new();
                map.insert("checksum".to_string(), json!(h));
                (None, Some(map))
            }
            _ => (None, None),
        };

        let dependencies = extract_all_dependencies(packages, root_package);

        let purl = match (&name, &version) {
            (Some(n), Some(v)) => PackageUrl::new("cargo", n).ok().and_then(|mut p| {
                p.with_version(v.as_str()).ok()?;
                Some(p.to_string())
            }),
            _ => None,
        };

        let api_data_url = match (&name, &version) {
            (Some(n), Some(v)) => Some(format!("https://crates.io/api/v1/crates/{}/{}", n, v)),
            (Some(n), None) => Some(format!("https://crates.io/api/v1/crates/{}", n)),
            _ => None,
        };

        vec![PackageData {
            package_type: Some(Self::PACKAGE_TYPE),
            namespace: None,
            name,
            version,
            qualifiers: None,
            subpath: None,
            primary_language: None,
            description: None,
            release_date: None,
            parties: Vec::new(),
            keywords: Vec::new(),
            homepage_url: None,
            download_url: None,
            size: None,
            sha1: None,
            md5: None,
            sha256,
            sha512: None,
            bug_tracking_url: None,
            code_view_url: None,
            vcs_url: None,
            copyright: None,
            holder: None,
            declared_license_expression: None,
            declared_license_expression_spdx: None,
            license_detections: Vec::new(),
            other_license_expression: None,
            other_license_expression_spdx: None,
            other_license_detections: Vec::new(),
            extracted_license_statement: None,
            notice_text: None,
            source_packages: Vec::new(),
            file_references: Vec::new(),
            is_private: false,
            is_virtual: false,
            extra_data,
            dependencies,
            repository_homepage_url: None,
            repository_download_url: None,
            api_data_url,
            datasource_id: Some(DatasourceId::CargoLock),
            purl,
        }]
    }
}

fn read_cargo_lock(path: &Path) -> Result<Value, String> {
    let mut file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| format!("Failed to read file: {}", e))?;
    toml::from_str(&content).map_err(|e| format!("Failed to parse TOML: {}", e))
}

fn select_root_package(packages: &[Value]) -> Option<&toml::map::Map<String, Value>> {
    packages
        .iter()
        .filter_map(|package| package.as_table())
        .find(|table| table.get("source").is_none())
        .or_else(|| packages.first().and_then(|package| package.as_table()))
}

fn extract_all_dependencies(
    packages: &[Value],
    root_package: Option<&toml::map::Map<String, Value>>,
) -> Vec<Dependency> {
    let mut all_dependencies: HashMap<CargoDependencyKey, Dependency> = HashMap::new();

    let package_versions = build_package_versions(packages);
    let package_provenance = build_package_provenance(packages);
    let root_package_key = root_package.and_then(package_key_from_table);

    for package in packages {
        if let Some(pkg_table) = package.as_table() {
            let is_root_package = package_key_from_table(pkg_table)
                .zip(root_package_key)
                .is_some_and(|(package_key, root_key)| package_key == root_key);

            if let Some(deps) = pkg_table.get("dependencies").and_then(|v| v.as_array()) {
                for dep in deps {
                    if let Some(dep_str) = dep.as_str() {
                        let parsed_dependency = parse_dependency_string(dep_str);
                        let name = parsed_dependency.name;
                        let resolved_version = if parsed_dependency.version.is_empty() {
                            package_versions
                                .get(name)
                                .and_then(|versions| (versions.len() == 1).then_some(versions[0]))
                                .unwrap_or("")
                        } else {
                            parsed_dependency.version
                        };

                        if !name.is_empty() {
                            let purl = if resolved_version.is_empty() {
                                PackageUrl::new("cargo", name).ok().map(|p| p.to_string())
                            } else {
                                PackageUrl::new("cargo", name).ok().and_then(|mut p| {
                                    p.with_version(resolved_version).ok()?;
                                    Some(p.to_string())
                                })
                            };

                            let extra_data = build_dependency_extra_data(
                                name,
                                resolved_version,
                                parsed_dependency.source,
                                &package_provenance,
                            );

                            let dependency = Dependency {
                                purl,
                                extracted_requirement: if resolved_version.is_empty() {
                                    None
                                } else {
                                    Some(resolved_version.to_string())
                                },
                                scope: Some("dependencies".to_string()),
                                is_runtime: Some(true),
                                is_optional: Some(false),
                                is_pinned: Some(true),
                                is_direct: Some(is_root_package),
                                resolved_package: None,
                                extra_data,
                            };

                            let key = CargoDependencyKey::from_dependency(&dependency);
                            match all_dependencies.entry(key) {
                                Entry::Vacant(entry) => {
                                    entry.insert(dependency);
                                }
                                Entry::Occupied(mut entry) => {
                                    if is_root_package {
                                        entry.get_mut().is_direct = Some(true);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let mut dependencies: Vec<_> = all_dependencies.into_values().collect();
    dependencies.sort_by(|left, right| {
        left.purl
            .as_deref()
            .cmp(&right.purl.as_deref())
            .then_with(|| {
                left.extracted_requirement
                    .as_deref()
                    .cmp(&right.extracted_requirement.as_deref())
            })
    });
    dependencies
}

#[derive(Hash, PartialEq, Eq)]
struct CargoDependencyKey {
    purl: Option<String>,
    extracted_requirement: Option<String>,
    source: Option<String>,
}

impl CargoDependencyKey {
    fn from_dependency(dependency: &Dependency) -> Self {
        let source = dependency
            .extra_data
            .as_ref()
            .and_then(|extra_data| extra_data.get("source"))
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned);

        Self {
            purl: dependency.purl.clone(),
            extracted_requirement: dependency.extracted_requirement.clone(),
            source,
        }
    }
}

fn build_package_versions(packages: &[Value]) -> HashMap<&str, Vec<&str>> {
    packages
        .iter()
        .filter_map(|package| package.as_table())
        .filter_map(|table| {
            Some((
                table.get("name")?.as_str()?,
                table.get("version")?.as_str()?,
            ))
        })
        .fold(HashMap::new(), |mut acc, (name, version)| {
            acc.entry(name).or_default().push(version);
            acc
        })
}

fn build_package_provenance<'a>(
    packages: &'a [Value],
) -> HashMap<(&'a str, &'a str), Vec<DependencyProvenance<'a>>> {
    packages
        .iter()
        .filter_map(|package| package.as_table())
        .filter_map(|table| {
            Some((
                (
                    table.get("name")?.as_str()?,
                    table.get("version")?.as_str()?,
                ),
                DependencyProvenance {
                    source: table.get("source").and_then(|value| value.as_str()),
                    checksum: table.get("checksum").and_then(|value| value.as_str()),
                },
            ))
        })
        .fold(HashMap::new(), |mut acc, (key, provenance)| {
            acc.entry(key).or_default().push(provenance);
            acc
        })
}

fn build_dependency_extra_data(
    name: &str,
    resolved_version: &str,
    source_hint: Option<&str>,
    package_provenance: &HashMap<(&str, &str), Vec<DependencyProvenance<'_>>>,
) -> Option<HashMap<String, serde_json::Value>> {
    let mut extra_data = HashMap::new();

    if !resolved_version.is_empty()
        && let Some(provenance) = package_provenance
            .get(&(name, resolved_version))
            .and_then(|candidates| select_dependency_provenance(candidates, source_hint))
    {
        if let Some(source) = provenance.source {
            extra_data.insert("source".to_string(), json!(source));
        }
        if let Some(checksum) = provenance.checksum {
            extra_data.insert("checksum".to_string(), json!(checksum));
        }
    }

    if !extra_data.contains_key("source")
        && let Some(source) = source_hint
    {
        extra_data.insert("source".to_string(), json!(source));
    }

    if extra_data.is_empty() {
        None
    } else {
        Some(extra_data)
    }
}

fn select_dependency_provenance<'a>(
    candidates: &'a [DependencyProvenance<'a>],
    source_hint: Option<&str>,
) -> Option<DependencyProvenance<'a>> {
    if let Some(source_hint) = source_hint {
        return candidates
            .iter()
            .copied()
            .find(|candidate| candidate.source == Some(source_hint));
    }

    (candidates.len() == 1).then_some(candidates[0])
}

fn package_key_from_table(table: &toml::map::Map<String, Value>) -> Option<(&str, &str)> {
    Some((
        table.get("name")?.as_str()?,
        table.get("version")?.as_str()?,
    ))
}

fn parse_dependency_string(dep_str: &str) -> ParsedDependency<'_> {
    let trimmed = dep_str.trim();
    let source = trimmed
        .find(" (")
        .and_then(|source_start| trimmed[source_start + 2..].strip_suffix(')'));
    let without_source = trimmed
        .find(" (")
        .map(|source_start| &trimmed[..source_start])
        .unwrap_or(trimmed);

    let mut parts = without_source.split_whitespace();
    let name = parts.next().unwrap_or("");
    let version = parts.next().unwrap_or("");

    ParsedDependency {
        name,
        version,
        source,
    }
}

#[derive(Clone, Copy)]
struct ParsedDependency<'a> {
    name: &'a str,
    version: &'a str,
    source: Option<&'a str>,
}

#[derive(Clone, Copy)]
struct DependencyProvenance<'a> {
    source: Option<&'a str>,
    checksum: Option<&'a str>,
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(CargoLockParser::PACKAGE_TYPE),
        datasource_id: Some(DatasourceId::CargoLock),
        ..Default::default()
    }
}

crate::register_parser!(
    "Rust Cargo.lock lockfile",
    &["**/Cargo.lock", "**/cargo.lock"],
    "cargo",
    "Rust",
    Some("https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html"),
);
