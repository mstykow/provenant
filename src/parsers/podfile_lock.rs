//! Parser for CocoaPods Podfile.lock lockfiles.
//!
//! Extracts resolved dependency information from Podfile.lock files which maintain
//! the exact versions of all dependencies used by a CocoaPods project.
//!
//! # Supported Formats
//! - Podfile.lock (YAML-based lockfile with multiple sections)
//!
//! # Key Features
//! - Direct vs transitive dependency tracking
//! - Exact version resolution from lockfile
//! - Pod source and repository information
//! - Spec repository tracking
//! - YAML multi-section aggregation (PODS, DEPENDENCIES, SPEC REPOS, PODFILE LOCK)
//!
//! # Implementation Notes
//! - Uses YAML parsing via `yaml_serde`
//! - All lockfile versions are pinned (`is_pinned: Some(true)`)
//! - Data aggregation across PODS, DEPENDENCIES, and metadata sections
//! - Graceful error handling with `warn!()` logs

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::parser_warn as warn;
use yaml_serde::Value;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType, ResolvedPackage};

use super::PackageParser;

const PRIMARY_LANGUAGE: &str = "Objective-C";

/// Parses CocoaPods lockfiles (Podfile.lock).
///
/// Extracts pinned dependency versions from Podfile.lock using data aggregation
/// across multiple YAML sections.
///
/// # Data Aggregation
/// Correlates information from 5 sections:
/// - **PODS**: Dependency tree with versions
/// - **DEPENDENCIES**: Direct dependencies
/// - **SPEC REPOS**: Source repositories
/// - **CHECKSUMS**: SHA1 hashes
/// - **EXTERNAL SOURCES**: Git/path sources
///
/// Uses `PodfileLockDataByPurl` pattern to aggregate data by package URL.
pub struct PodfileLockParser;

impl PackageParser for PodfileLockParser {
    const PACKAGE_TYPE: PackageType = PackageType::Cocoapods;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "Podfile.lock")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read Podfile.lock at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        let data: Value = match yaml_serde::from_str(&content) {
            Ok(d) => d,
            Err(e) => {
                warn!("Failed to parse Podfile.lock at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        vec![parse_podfile_lock(&data)]
    }
}

struct DependencyDataByPurl {
    versions_by_base_purl: HashMap<String, String>,
    direct_dependency_purls: Vec<String>,
    spec_by_base_purl: HashMap<String, String>,
    checksum_by_base_purl: HashMap<String, String>,
    external_sources_by_base_purl: HashMap<String, String>,
}

impl DependencyDataByPurl {
    fn collect(data: &Value) -> Self {
        let mut dep_data = DependencyDataByPurl {
            versions_by_base_purl: HashMap::new(),
            direct_dependency_purls: Vec::new(),
            spec_by_base_purl: HashMap::new(),
            checksum_by_base_purl: HashMap::new(),
            external_sources_by_base_purl: HashMap::new(),
        };

        if let Some(pods) = data.get("PODS").and_then(|v| v.as_sequence()) {
            for pod in pods {
                let main_pod_str = match pod {
                    Value::String(s) => Some(s.as_str()),
                    Value::Mapping(m) => m.keys().next().and_then(|k| k.as_str()),
                    _ => None,
                };
                if let Some(main_pod_str) = main_pod_str {
                    let (base_purl, version) = parse_dep_to_base_purl_and_version(main_pod_str);
                    if let Some(version) = version {
                        dep_data.versions_by_base_purl.insert(base_purl, version);
                    }
                }
            }
        }

        if let Some(deps) = data.get("DEPENDENCIES").and_then(|v| v.as_sequence()) {
            for dep in deps {
                if let Some(dep_str) = dep.as_str() {
                    let (base_purl, _) = parse_dep_to_base_purl_and_version(dep_str);
                    dep_data.direct_dependency_purls.push(base_purl);
                }
            }
        }

        if let Some(spec_repos) = data.get("SPEC REPOS").and_then(|v| v.as_mapping()) {
            for (repo_key, packages) in spec_repos {
                let repo_name = match repo_key.as_str() {
                    Some(s) => s.to_string(),
                    None => continue,
                };
                if let Some(packages) = packages.as_sequence() {
                    for package in packages {
                        if let Some(pkg_str) = package.as_str() {
                            let (base_purl, _) = parse_dep_to_base_purl_and_version(pkg_str);
                            dep_data
                                .spec_by_base_purl
                                .insert(base_purl, repo_name.clone());
                        }
                    }
                }
            }
        }

        if let Some(checksums) = data.get("SPEC CHECKSUMS").and_then(|v| v.as_mapping()) {
            for (name_key, checksum_val) in checksums {
                if let (Some(name), Some(checksum)) = (name_key.as_str(), checksum_val.as_str()) {
                    let (base_purl, _) = parse_dep_to_base_purl_and_version(name);
                    dep_data
                        .checksum_by_base_purl
                        .insert(base_purl, checksum.to_string());
                }
            }
        }

        if let Some(checkout_opts) = data.get("CHECKOUT OPTIONS").and_then(|v| v.as_mapping()) {
            for (name_key, source) in checkout_opts {
                if let (Some(name), Some(mapping)) = (name_key.as_str(), source.as_mapping()) {
                    let base_purl = make_base_purl(name);
                    let processed = process_external_source(mapping);
                    dep_data
                        .external_sources_by_base_purl
                        .insert(base_purl, processed);
                }
            }
        }

        if let Some(ext_sources) = data.get("EXTERNAL SOURCES").and_then(|v| v.as_mapping()) {
            for (name_key, source) in ext_sources {
                if let (Some(name), Some(mapping)) = (name_key.as_str(), source.as_mapping()) {
                    let base_purl = make_base_purl(name);
                    if dep_data
                        .external_sources_by_base_purl
                        .contains_key(&base_purl)
                    {
                        continue;
                    }
                    let processed = process_external_source(mapping);
                    dep_data
                        .external_sources_by_base_purl
                        .insert(base_purl, processed);
                }
            }
        }

        dep_data
    }
}

fn parse_podfile_lock(data: &Value) -> PackageData {
    let dep_data = DependencyDataByPurl::collect(data);
    let mut dependencies = Vec::new();

    if let Some(pods) = data.get("PODS").and_then(|v| v.as_sequence()) {
        for pod in pods {
            match pod {
                Value::Mapping(m) => {
                    for (main_pod_key, dep_pods_val) in m {
                        if let Some(main_pod_str) = main_pod_key.as_str() {
                            let dep_pods: Vec<&str> = dep_pods_val
                                .as_sequence()
                                .map(|seq| seq.iter().filter_map(|v| v.as_str()).collect())
                                .unwrap_or_default();

                            let nested_deps = build_dependencies_for_resolved(&dep_data, &dep_pods);
                            let dep = build_pod_dependency(&dep_data, main_pod_str, nested_deps);
                            dependencies.push(dep);
                        }
                    }
                }
                Value::String(s) => {
                    let dep = build_pod_dependency(&dep_data, s, Vec::new());
                    dependencies.push(dep);
                }
                _ => {}
            }
        }
    }

    let cocoapods_version = data
        .get("COCOAPODS")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let podfile_checksum = data
        .get("PODFILE CHECKSUM")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut extra_data = HashMap::new();
    if let Some(v) = cocoapods_version {
        extra_data.insert("cocoapods".to_string(), serde_json::Value::String(v));
    }
    if let Some(v) = podfile_checksum {
        extra_data.insert("podfile_checksum".to_string(), serde_json::Value::String(v));
    }

    let mut pkg = default_package_data();
    pkg.dependencies = dependencies;
    pkg.extra_data = if extra_data.is_empty() {
        None
    } else {
        Some(extra_data)
    };
    pkg
}

fn build_pod_dependency(
    dep_data: &DependencyDataByPurl,
    main_pod: &str,
    nested_deps: Vec<Dependency>,
) -> Dependency {
    let (namespace, name, version, requirement) = parse_dep_requirements(main_pod);
    let base_purl = make_base_purl_from_parts(namespace.as_deref(), &name);

    let is_direct = dep_data.direct_dependency_purls.contains(&base_purl);

    let checksum = dep_data.checksum_by_base_purl.get(&base_purl).cloned();
    let spec_repo = dep_data.spec_by_base_purl.get(&base_purl).cloned();
    let external_source = dep_data
        .external_sources_by_base_purl
        .get(&base_purl)
        .cloned();

    let mut resolved_extra_data: HashMap<String, serde_json::Value> = HashMap::new();
    if let Some(repo) = spec_repo {
        resolved_extra_data.insert("spec_repo".to_string(), serde_json::Value::String(repo));
    }
    if let Some(source) = external_source {
        resolved_extra_data.insert(
            "external_source".to_string(),
            serde_json::Value::String(source),
        );
    }

    let resolved_package = ResolvedPackage {
        primary_language: Some(PRIMARY_LANGUAGE.to_string()),
        download_url: None,
        sha1: checksum,
        sha256: None,
        sha512: None,
        md5: None,
        is_virtual: true,
        extra_data: if resolved_extra_data.is_empty() {
            None
        } else {
            Some(resolved_extra_data)
        },
        dependencies: nested_deps,
        repository_homepage_url: None,
        repository_download_url: None,
        api_data_url: None,
        datasource_id: Some(DatasourceId::CocoapodsPodfileLock),
        purl: None,
        ..ResolvedPackage::new(
            PodfileLockParser::PACKAGE_TYPE,
            namespace.clone().unwrap_or_default(),
            name.clone(),
            version.clone().unwrap_or_default(),
        )
    };

    let purl = create_cocoapods_purl(namespace.as_deref(), &name, version.as_deref());

    Dependency {
        purl,
        extracted_requirement: requirement,
        scope: Some("dependencies".to_string()),
        is_runtime: None,
        is_optional: None,
        is_pinned: Some(true),
        is_direct: Some(is_direct),
        resolved_package: Some(Box::new(resolved_package)),
        extra_data: None,
    }
}

fn build_dependencies_for_resolved(
    dep_data: &DependencyDataByPurl,
    dep_pods: &[&str],
) -> Vec<Dependency> {
    dep_pods
        .iter()
        .map(|dep_pod| {
            let (namespace, name, version, requirement) = parse_dep_requirements(dep_pod);
            let base_purl = make_base_purl_from_parts(namespace.as_deref(), &name);

            let resolved_version = dep_data.versions_by_base_purl.get(&base_purl);

            let final_version = resolved_version.cloned().or(version);
            let final_requirement = requirement.or_else(|| resolved_version.cloned());

            let purl = create_cocoapods_purl(namespace.as_deref(), &name, final_version.as_deref());

            Dependency {
                purl,
                extracted_requirement: final_requirement,
                scope: Some("dependencies".to_string()),
                is_runtime: None,
                is_optional: None,
                is_pinned: Some(true),
                is_direct: Some(true),
                resolved_package: None,
                extra_data: None,
            }
        })
        .collect()
}

pub(crate) fn parse_dep_requirements(
    dep: &str,
) -> (Option<String>, String, Option<String>, Option<String>) {
    let dep = dep.trim();
    let (name_part, version, requirement) = if let Some(paren_idx) = dep.find('(') {
        let name_part = dep[..paren_idx].trim();
        let version_part = dep[paren_idx..].trim_matches(|c| c == '(' || c == ')' || c == ' ');
        let requirement = version_part.to_string();
        let version = version_part.trim_start_matches(|c: char| !c.is_ascii_digit() && c != '.');
        let version = version.trim();
        (
            name_part.to_string(),
            if version.is_empty() {
                None
            } else {
                Some(version.to_string())
            },
            Some(requirement),
        )
    } else {
        (dep.trim_end_matches(')').to_string(), None, None)
    };

    let (namespace, name) = if name_part.contains('/') {
        let (ns, n) = name_part.split_once('/').unwrap_or(("", &name_part));
        (Some(ns.trim().to_string()), n.trim().to_string())
    } else {
        (None, name_part.trim().to_string())
    };

    (namespace, name, version, requirement)
}

fn parse_dep_to_base_purl_and_version(dep: &str) -> (String, Option<String>) {
    let (namespace, name, _version, requirement) = parse_dep_requirements(dep);
    let base_purl = make_base_purl_from_parts(namespace.as_deref(), &name);
    (base_purl, requirement)
}

fn make_base_purl(name: &str) -> String {
    format!("pkg:cocoapods/{}", name)
}

fn make_base_purl_from_parts(namespace: Option<&str>, name: &str) -> String {
    match namespace {
        Some(ns) if !ns.is_empty() => format!("pkg:cocoapods/{}/{}", ns, name),
        _ => make_base_purl(name),
    }
}

fn create_cocoapods_purl(
    namespace: Option<&str>,
    name: &str,
    version: Option<&str>,
) -> Option<String> {
    let ns_part = match namespace {
        Some(ns) if !ns.is_empty() => format!("{}/", ns),
        _ => String::new(),
    };
    let version_part = match version {
        Some(v) if !v.is_empty() => format!("@{}", v),
        _ => String::new(),
    };
    Some(format!("pkg:cocoapods/{}{}{}", ns_part, name, version_part))
}

fn process_external_source(mapping: &yaml_serde::Mapping) -> String {
    let get_str = |key: &str| -> Option<String> {
        mapping
            .get(Value::String(key.to_string()))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    };

    if mapping.len() == 1 {
        return mapping
            .values()
            .next()
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
    }

    if mapping.len() == 2
        && let Some(git_url) = get_str(":git")
    {
        let repo_url = git_url
            .replace(".git", "")
            .replace("git@", "https://")
            .trim_end_matches('/')
            .to_string();

        if let Some(commit) = get_str(":commit") {
            return format!("{}/tree/{}", repo_url, commit);
        }
        if let Some(branch) = get_str(":branch") {
            return format!("{}/tree/{}", repo_url, branch);
        }
    }

    format!("{:?}", mapping)
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(PodfileLockParser::PACKAGE_TYPE),
        primary_language: Some(PRIMARY_LANGUAGE.to_string()),
        datasource_id: Some(DatasourceId::CocoapodsPodfileLock),
        ..Default::default()
    }
}

crate::register_parser!(
    "Cocoapods Podfile.lock",
    &["**/Podfile.lock"],
    "cocoapods",
    "Objective-C",
    Some("https://guides.cocoapods.org/using/the-podfile.html"),
);
