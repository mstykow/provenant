// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::path::Path;

use crate::parser_warn as warn;
use crate::parsers::utils::{MAX_ITERATION_COUNT, truncate_field};
use packageurl::PackageUrl;
use serde_json::Value;
use url::Url;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};

use super::PackageParser;

const FIELD_NAME: &str = "name";
const FIELD_VERSION: &str = "version";
const FIELD_EXPORTS: &str = "exports";
const FIELD_IMPORTS: &str = "imports";
const FIELD_SCOPES: &str = "scopes";
const FIELD_LINKS: &str = "links";
const FIELD_TASKS: &str = "tasks";
const FIELD_LOCK: &str = "lock";
const FIELD_NODE_MODULES_DIR: &str = "nodeModulesDir";
const FIELD_WORKSPACE: &str = "workspace";

pub struct DenoParser;

impl PackageParser for DenoParser {
    const PACKAGE_TYPE: PackageType = PackageType::Deno;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "deno.json" || name == "deno.jsonc")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match crate::parsers::utils::read_file_to_string(path, None) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read Deno config at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        let json: Value = match json5::from_str(&content) {
            Ok(json) => json,
            Err(e) => {
                warn!("Failed to parse Deno config at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        vec![parse_deno_config(&json)]
    }
}

fn parse_deno_config(json: &Value) -> PackageData {
    let raw_name = extract_non_empty_string(json, FIELD_NAME);
    let (namespace, name) = raw_name
        .as_deref()
        .map(split_package_identity)
        .map(|(namespace, name)| {
            (
                namespace.map(|value| truncate_field(value.to_string())),
                Some(truncate_field(name.to_string())),
            )
        })
        .unwrap_or((None, None));
    let version = extract_non_empty_string(json, FIELD_VERSION).map(truncate_field);
    let dependencies = extract_import_dependencies(json);
    let extra_data = extract_extra_data(json);
    let purl = match (namespace.as_deref(), name.as_deref(), version.as_deref()) {
        (_, Some(name), version) => create_generic_purl(namespace.as_deref(), name, version),
        _ => None,
    };

    PackageData {
        package_type: Some(DenoParser::PACKAGE_TYPE),
        namespace,
        name,
        version,
        qualifiers: None,
        subpath: None,
        primary_language: Some("TypeScript".to_string()),
        description: None,
        release_date: None,
        parties: Vec::new(),
        keywords: Vec::new(),
        homepage_url: None,
        download_url: None,
        size: None,
        sha1: None,
        md5: None,
        sha256: None,
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
        api_data_url: None,
        datasource_id: Some(DatasourceId::DenoJson),
        purl: purl.map(truncate_field),
    }
}

fn extract_import_dependencies(json: &Value) -> Vec<Dependency> {
    json.get(FIELD_IMPORTS)
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .take(MAX_ITERATION_COUNT)
        .filter_map(|(alias, value)| {
            value
                .as_str()
                .map(|specifier| build_import_dependency(alias, specifier))
        })
        .collect()
}

fn build_import_dependency(alias: &str, specifier: &str) -> Dependency {
    let (purl, is_pinned) = if let Some((namespace, name, version)) = parse_jsr_specifier(specifier)
    {
        (
            create_generic_purl(Some(&format!("jsr.io/{}", namespace)), &name, None),
            Some(version.is_some_and(is_exact_version)),
        )
    } else if let Some((namespace, name, version)) = parse_npm_specifier(specifier) {
        (
            create_npm_purl(namespace.as_deref(), &name, None),
            Some(version.is_some_and(is_exact_version)),
        )
    } else {
        (create_remote_purl(specifier), Some(false))
    };

    Dependency {
        purl: purl.map(truncate_field),
        extracted_requirement: Some(truncate_field(specifier.to_string())),
        scope: Some("imports".to_string()),
        is_runtime: Some(true),
        is_optional: Some(false),
        is_pinned,
        is_direct: Some(true),
        resolved_package: None,
        extra_data: Some(HashMap::from([(
            truncate_field("import_alias".to_string()),
            Value::String(truncate_field(alias.to_string())),
        )])),
    }
}

fn parse_jsr_specifier(specifier: &str) -> Option<(String, String, Option<&str>)> {
    let rest = specifier.strip_prefix("jsr:")?;
    let slash_index = rest.find('/')?;
    let namespace = rest[..slash_index].to_string();
    let name_and_version = &rest[slash_index + 1..];
    let (name, version) = split_name_and_version(name_and_version);
    Some((namespace, name.to_string(), version))
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

fn extract_extra_data(json: &Value) -> Option<HashMap<String, Value>> {
    let mut extra_data = HashMap::new();
    for field in [
        FIELD_EXPORTS,
        FIELD_IMPORTS,
        FIELD_SCOPES,
        FIELD_LINKS,
        FIELD_TASKS,
        FIELD_LOCK,
        FIELD_NODE_MODULES_DIR,
        FIELD_WORKSPACE,
    ] {
        if let Some(value) = json.get(field) {
            extra_data.insert(field.to_string(), value.clone());
        }
    }
    (!extra_data.is_empty()).then_some(extra_data)
}

fn extract_non_empty_string(json: &Value, field: &str) -> Option<String> {
    json.get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

fn create_npm_purl(namespace: Option<&str>, name: &str, version: Option<&str>) -> Option<String> {
    let mut purl = PackageUrl::new("npm", name).ok()?;
    if let Some(namespace) = namespace {
        purl.with_namespace(namespace).ok()?;
    }
    if let Some(version) = version
        && is_exact_version(version)
    {
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
    if let Some(version) = version
        && !version.is_empty()
    {
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

fn split_package_identity(name: &str) -> (Option<&str>, &str) {
    if let Some(scoped) = name.strip_prefix('@')
        && let Some(slash_index) = scoped.find('/')
    {
        return (Some(&name[..slash_index + 1]), &scoped[slash_index + 1..]);
    }
    (None, name)
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
        package_type: Some(DenoParser::PACKAGE_TYPE),
        primary_language: Some("TypeScript".to_string()),
        datasource_id: Some(DatasourceId::DenoJson),
        ..Default::default()
    }
}

crate::register_parser!(
    "Deno configuration",
    &["**/deno.json", "**/deno.jsonc"],
    "deno",
    "TypeScript",
    Some("https://docs.deno.com/runtime/fundamentals/configuration/"),
);
