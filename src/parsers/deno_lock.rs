use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use serde_json::Value;
use url::Url;

use crate::models::{
    DatasourceId, Dependency, PackageData, PackageType, ResolvedPackage, Sha256Digest, Sha512Digest,
};

use super::PackageParser;
use super::utils::{MAX_ITERATION_COUNT, parse_sri, read_file_to_string, truncate_field};

const FIELD_VERSION: &str = "version";
const FIELD_SPECIFIERS: &str = "specifiers";
const FIELD_JSR: &str = "jsr";
const FIELD_NPM: &str = "npm";
const FIELD_REMOTE: &str = "remote";
const FIELD_REDIRECTS: &str = "redirects";
const FIELD_WORKSPACE: &str = "workspace";
const FIELD_DEPENDENCIES: &str = "dependencies";

pub struct DenoLockParser;

impl PackageParser for DenoLockParser {
    const PACKAGE_TYPE: PackageType = PackageType::Deno;

    fn is_match(path: &Path) -> bool {
        path.file_name().and_then(|name| name.to_str()) == Some("deno.lock")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read deno.lock at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        let json: Value = match serde_json::from_str(&content) {
            Ok(json) => json,
            Err(e) => {
                warn!("Failed to parse deno.lock at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        vec![parse_deno_lock(&json)]
    }
}

fn parse_deno_lock(json: &Value) -> PackageData {
    let lock_version = json.get(FIELD_VERSION).and_then(Value::as_str);
    if lock_version != Some("5") {
        warn!("Unsupported deno.lock version {:?}", lock_version);
        return default_package_data();
    }

    let specifiers = json
        .get(FIELD_SPECIFIERS)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let workspace_direct = extract_workspace_dependencies(json);

    let mut dependencies = Vec::new();
    let mut direct_jsr_keys = HashSet::new();
    let mut direct_npm_keys = HashSet::new();

    for specifier in workspace_direct.iter().take(MAX_ITERATION_COUNT) {
        if let Some(resolved_key) = specifiers.get(specifier).and_then(Value::as_str) {
            if specifier.starts_with("jsr:") {
                if let Some(full_key) = resolve_jsr_full_key(specifier, resolved_key)
                    && let Some(dep) =
                        build_jsr_dependency(&full_key, true, &json[FIELD_JSR], Some(specifier))
                {
                    direct_jsr_keys.insert(full_key);
                    dependencies.push(dep);
                }
            } else if specifier.starts_with("npm:")
                && let Some(full_key) = resolve_npm_full_key(specifier, resolved_key)
                && let Some(dep) =
                    build_npm_dependency(&full_key, true, &json[FIELD_NPM], Some(specifier))
            {
                direct_npm_keys.insert(full_key);
                dependencies.push(dep);
            }
        }
    }

    if let Some(jsr_map) = json.get(FIELD_JSR).and_then(Value::as_object) {
        for key in jsr_map.keys().take(MAX_ITERATION_COUNT) {
            if direct_jsr_keys.contains(key) {
                continue;
            }
            if let Some(dep) = build_jsr_dependency(key, false, &json[FIELD_JSR], None) {
                dependencies.push(dep);
            }
        }
    }

    if let Some(npm_map) = json.get(FIELD_NPM).and_then(Value::as_object) {
        for key in npm_map.keys().take(MAX_ITERATION_COUNT) {
            if direct_npm_keys.contains(key) {
                continue;
            }
            if let Some(dep) = build_npm_dependency(key, false, &json[FIELD_NPM], None) {
                dependencies.push(dep);
            }
        }
    }

    if let Some(redirects) = json.get(FIELD_REDIRECTS).and_then(Value::as_object) {
        for (source, target) in redirects.iter().take(MAX_ITERATION_COUNT) {
            let Some(target_url) = target.as_str() else {
                continue;
            };
            let hash = json
                .get(FIELD_REMOTE)
                .and_then(Value::as_object)
                .and_then(|remote| remote.get(target_url))
                .and_then(Value::as_str)
                .and_then(|value| Sha256Digest::from_hex(value).ok());

            let name =
                truncate_field(remote_name(target_url).unwrap_or_else(|| source.to_string()));
            let purl = create_remote_purl(target_url).map(truncate_field);
            let resolved_package = ResolvedPackage {
                primary_language: Some("TypeScript".to_string()),
                download_url: Some(truncate_field(target_url.to_string())),
                sha1: None,
                sha256: hash,
                sha512: None,
                md5: None,
                is_virtual: true,
                extra_data: Some(HashMap::from([(
                    "redirect_source".to_string(),
                    Value::String(truncate_field(source.to_string())),
                )])),
                dependencies: Vec::new(),
                repository_homepage_url: None,
                repository_download_url: None,
                api_data_url: None,
                datasource_id: Some(DatasourceId::DenoLock),
                purl: purl.clone(),
                ..ResolvedPackage::new(
                    DenoLockParser::PACKAGE_TYPE,
                    String::new(),
                    name.clone(),
                    String::new(),
                )
            };

            dependencies.push(Dependency {
                purl,
                extracted_requirement: Some(truncate_field(source.to_string())),
                scope: Some("imports".to_string()),
                is_runtime: Some(true),
                is_optional: Some(false),
                is_pinned: Some(true),
                is_direct: Some(true),
                resolved_package: Some(Box::new(resolved_package)),
                extra_data: None,
            });
        }
    }

    let mut extra_data = HashMap::new();
    extra_data.insert(FIELD_VERSION.to_string(), Value::String("5".to_string()));
    if !workspace_direct.is_empty() {
        extra_data.insert(
            "workspace_dependencies".to_string(),
            Value::Array(
                workspace_direct
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        );
    }

    PackageData {
        package_type: Some(DenoLockParser::PACKAGE_TYPE),
        primary_language: Some("TypeScript".to_string()),
        dependencies,
        extra_data: Some(extra_data),
        datasource_id: Some(DatasourceId::DenoLock),
        ..Default::default()
    }
}

fn extract_workspace_dependencies(json: &Value) -> Vec<String> {
    json.get(FIELD_WORKSPACE)
        .and_then(Value::as_object)
        .and_then(|workspace| workspace.get(FIELD_DEPENDENCIES))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|value| truncate_field(value.to_string()))
        .collect()
}

fn build_jsr_dependency(
    resolved_key: &str,
    is_direct: bool,
    jsr_section: &Value,
    extracted_requirement: Option<&str>,
) -> Option<Dependency> {
    let jsr_entry = jsr_section.get(resolved_key)?;
    let jsr_object = jsr_entry.as_object()?;
    let (namespace, name, version) = parse_jsr_key(resolved_key)?;
    let namespace = truncate_field(namespace);
    let name = truncate_field(name);
    let version_str = truncate_field(version.to_string());
    let purl = create_generic_purl(
        Some(&format!("jsr.io/{}", namespace)),
        &name,
        Some(&version_str),
    )
    .map(truncate_field);

    Some(Dependency {
        purl: purl.clone(),
        extracted_requirement: extracted_requirement.map(|value| truncate_field(value.to_string())),
        scope: Some("imports".to_string()),
        is_runtime: Some(true),
        is_optional: Some(false),
        is_pinned: Some(true),
        is_direct: Some(is_direct),
        resolved_package: Some(Box::new(ResolvedPackage {
            primary_language: Some("TypeScript".to_string()),
            download_url: None,
            sha1: None,
            sha256: jsr_object
                .get("integrity")
                .and_then(Value::as_str)
                .and_then(|value| {
                    parse_sri(value)
                        .and_then(|(algo, hex)| {
                            (algo == "sha256").then(|| Sha256Digest::from_hex(&hex).ok())
                        })
                        .flatten()
                        .or_else(|| Sha256Digest::from_hex(value).ok())
                }),
            sha512: None,
            md5: None,
            is_virtual: true,
            extra_data: None,
            dependencies: extract_jsr_resolved_dependencies(jsr_object),
            repository_homepage_url: None,
            repository_download_url: None,
            api_data_url: None,
            datasource_id: Some(DatasourceId::DenoLock),
            purl,
            ..ResolvedPackage::new(DenoLockParser::PACKAGE_TYPE, namespace, name, version_str)
        })),
        extra_data: None,
    })
}

fn build_npm_dependency(
    resolved_key: &str,
    is_direct: bool,
    npm_section: &Value,
    extracted_requirement: Option<&str>,
) -> Option<Dependency> {
    let npm_entry = npm_section.get(resolved_key)?;
    let npm_object = npm_entry.as_object()?;
    let (namespace, name, version) = parse_npm_key(resolved_key)?;
    let namespace = namespace.map(truncate_field);
    let name = truncate_field(name);
    let version_str = truncate_field(version.to_string());
    let purl = create_npm_purl(namespace.as_deref(), &name, Some(&version_str)).map(truncate_field);

    Some(Dependency {
        purl: purl.clone(),
        extracted_requirement: extracted_requirement.map(|value| truncate_field(value.to_string())),
        scope: Some("imports".to_string()),
        is_runtime: Some(true),
        is_optional: Some(false),
        is_pinned: Some(true),
        is_direct: Some(is_direct),
        resolved_package: Some(Box::new(ResolvedPackage {
            primary_language: Some("JavaScript".to_string()),
            download_url: npm_object
                .get("tarball")
                .and_then(Value::as_str)
                .map(|value| truncate_field(value.to_string())),
            sha1: None,
            sha256: None,
            sha512: npm_object
                .get("integrity")
                .and_then(Value::as_str)
                .and_then(|value| {
                    parse_sri(value)
                        .and_then(|(algo, hex)| {
                            (algo == "sha512").then(|| Sha512Digest::from_hex(&hex).ok())
                        })
                        .flatten()
                }),
            md5: None,
            is_virtual: true,
            extra_data: None,
            dependencies: npm_object
                .get(FIELD_DEPENDENCIES)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .take(MAX_ITERATION_COUNT)
                .filter_map(|value| {
                    let (namespace, name, version) = parse_npm_key(value)?;
                    Some(Dependency {
                        purl: create_npm_purl(namespace.as_deref(), &name, Some(version))
                            .map(truncate_field),
                        extracted_requirement: Some(truncate_field(value.to_string())),
                        scope: Some("dependencies".to_string()),
                        is_runtime: Some(true),
                        is_optional: Some(false),
                        is_pinned: Some(true),
                        is_direct: Some(true),
                        resolved_package: None,
                        extra_data: None,
                    })
                })
                .collect(),
            repository_homepage_url: None,
            repository_download_url: None,
            api_data_url: None,
            datasource_id: Some(DatasourceId::DenoLock),
            purl,
            ..ResolvedPackage::new(
                PackageType::Npm,
                namespace.unwrap_or_default(),
                name,
                version_str,
            )
        })),
        extra_data: None,
    })
}

fn extract_jsr_resolved_dependencies(
    jsr_object: &serde_json::Map<String, Value>,
) -> Vec<Dependency> {
    jsr_object
        .get(FIELD_DEPENDENCIES)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .take(MAX_ITERATION_COUNT)
        .filter_map(|value| {
            let (namespace, name, version) = parse_jsr_dependency_reference(value)?;
            Some(Dependency {
                purl: create_generic_purl(Some(&format!("jsr.io/{}", namespace)), &name, version)
                    .map(truncate_field),
                extracted_requirement: Some(truncate_field(value.to_string())),
                scope: Some("dependencies".to_string()),
                is_runtime: Some(true),
                is_optional: Some(false),
                is_pinned: Some(version.is_some_and(is_exact_version)),
                is_direct: Some(true),
                resolved_package: None,
                extra_data: None,
            })
        })
        .collect()
}

fn parse_jsr_key(key: &str) -> Option<(String, String, &str)> {
    let scoped = key.strip_prefix('@')?;
    let slash_index = scoped.find('/')?;
    let namespace = format!("@{}", &scoped[..slash_index]);
    let name_and_version = &scoped[slash_index + 1..];
    let at_index = name_and_version.rfind('@')?;
    let name = name_and_version[..at_index].to_string();
    let version = &name_and_version[at_index + 1..];
    Some((namespace, name, version))
}

fn parse_jsr_dependency_reference(value: &str) -> Option<(String, String, Option<&str>)> {
    let rest = value.strip_prefix("jsr:")?;
    let slash_index = rest.find('/')?;
    let namespace = format!("@{}", &rest[1..slash_index]);
    let name_and_version = &rest[slash_index + 1..];
    let (name, version) = split_name_and_version(name_and_version);
    Some((namespace, name.to_string(), version))
}

fn resolve_jsr_full_key(specifier: &str, resolved_version: &str) -> Option<String> {
    let (namespace, name, _) = parse_jsr_dependency_reference(specifier)?;
    Some(format!("{}/{}@{}", namespace, name, resolved_version))
}

fn parse_npm_key(key: &str) -> Option<(Option<String>, String, &str)> {
    if let Some(scoped) = key.strip_prefix('@') {
        let slash_index = scoped.find('/')?;
        let namespace = format!("@{}", &scoped[..slash_index]);
        let name_and_version = &scoped[slash_index + 1..];
        let at_index = name_and_version.rfind('@')?;
        let name = name_and_version[..at_index].to_string();
        let version = &name_and_version[at_index + 1..];
        Some((Some(namespace), name, version))
    } else {
        let at_index = key.rfind('@')?;
        let name = key[..at_index].to_string();
        let version = &key[at_index + 1..];
        Some((None, name, version))
    }
}

fn resolve_npm_full_key(specifier: &str, resolved_version: &str) -> Option<String> {
    let (namespace, name, _) = parse_npm_specifier(specifier)?;
    Some(match namespace {
        Some(namespace) => format!("{}/{}@{}", namespace, name, resolved_version),
        None => format!("{}@{}", name, resolved_version),
    })
}

fn parse_npm_specifier(specifier: &str) -> Option<(Option<String>, String, Option<&str>)> {
    let rest = specifier.strip_prefix("npm:")?;
    let (name_part, version) = split_name_and_version(rest);
    if let Some(scoped) = name_part.strip_prefix('@') {
        let slash_index = scoped.find('/')?;
        let namespace = format!("@{}", &scoped[..slash_index]);
        let name = scoped[slash_index + 1..].to_string();
        Some((Some(namespace), name, version))
    } else {
        Some((None, name_part.to_string(), version))
    }
}

fn split_name_and_version(input: &str) -> (&str, Option<&str>) {
    if let Some(index) = input.rfind('@') {
        let (name, version) = input.split_at(index);
        if !name.is_empty() {
            return (name, Some(&version[1..]));
        }
    }
    (input, None)
}

fn create_npm_purl(namespace: Option<&str>, name: &str, version: Option<&str>) -> Option<String> {
    let mut purl = PackageUrl::new("npm", name).ok()?;
    if let Some(namespace) = namespace {
        purl.with_namespace(namespace).ok()?;
    }
    if let Some(version) = version {
        purl.with_version(version).ok()?;
    }
    Some(purl.to_string())
}

fn create_generic_purl(
    namespace: Option<&str>,
    name: &str,
    version: Option<&str>,
) -> Option<String> {
    let mut purl = PackageUrl::new("generic", name).ok()?;
    if let Some(namespace) = namespace {
        purl.with_namespace(namespace).ok()?;
    }
    if let Some(version) = version {
        purl.with_version(version).ok()?;
    }
    Some(purl.to_string())
}

fn create_remote_purl(specifier: &str) -> Option<String> {
    let url = Url::parse(specifier).ok()?;
    let segments: Vec<&str> = url.path_segments()?.collect();
    let name = segments.last()?.to_string();
    let namespace = if segments.len() > 1 {
        Some(format!(
            "{}/{}",
            url.host_str()?,
            segments[..segments.len() - 1].join("/")
        ))
    } else {
        url.host_str().map(|host| host.to_string())
    };
    create_generic_purl(namespace.as_deref(), &name, None)
}

fn remote_name(url: &str) -> Option<String> {
    let url = Url::parse(url).ok()?;
    url.path_segments()?
        .next_back()
        .map(|value| value.to_string())
}

fn is_exact_version(version: &str) -> bool {
    !version.contains('^')
        && !version.contains('~')
        && !version.contains('*')
        && !version.contains('>')
        && !version.contains('<')
        && !version.contains('|')
        && !version.contains(' ')
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(DenoLockParser::PACKAGE_TYPE),
        primary_language: Some("TypeScript".to_string()),
        datasource_id: Some(DatasourceId::DenoLock),
        ..Default::default()
    }
}

crate::register_parser!(
    "Deno lockfile",
    &["**/deno.lock"],
    "deno",
    "TypeScript",
    Some("https://docs.deno.com/runtime/fundamentals/modules/"),
);
