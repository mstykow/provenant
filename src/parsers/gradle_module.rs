use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use serde_json::{Map as JsonMap, Value};

use crate::models::{
    DatasourceId, Dependency, FileReference, Md5Digest, PackageData, PackageType, Sha1Digest,
    Sha256Digest, Sha512Digest,
};

use super::PackageParser;

const FIELD_FORMAT_VERSION: &str = "formatVersion";
const FIELD_COMPONENT: &str = "component";
const FIELD_CREATED_BY: &str = "createdBy";
const FIELD_VARIANTS: &str = "variants";
const FIELD_ATTRIBUTES: &str = "attributes";
const FIELD_DEPENDENCIES: &str = "dependencies";
const FIELD_DEPENDENCY_CONSTRAINTS: &str = "dependencyConstraints";
const FIELD_FILES: &str = "files";
const FIELD_AVAILABLE_AT: &str = "available-at";

type ArtifactHashes = (
    Option<u64>,
    Option<Sha1Digest>,
    Option<Md5Digest>,
    Option<Sha256Digest>,
    Option<Sha512Digest>,
);

type ExtractedVariantData = (
    Vec<Dependency>,
    Vec<FileReference>,
    Option<JsonMap<String, Value>>,
    Vec<Value>,
);

pub struct GradleModuleParser;

#[derive(Clone, Debug, Default)]
struct ExtractedDependency {
    purl: Option<String>,
    extracted_requirement: Option<String>,
    scope: Option<String>,
    is_runtime: Option<bool>,
    is_optional: Option<bool>,
    is_pinned: Option<bool>,
    extra_data: Option<HashMap<String, Value>>,
    precedence: u8,
}

impl PackageParser for GradleModuleParser {
    const PACKAGE_TYPE: PackageType = PackageType::Maven;

    fn is_match(path: &Path) -> bool {
        if path.extension().and_then(|ext| ext.to_str()) != Some("module") {
            return false;
        }

        let Ok(file) = File::open(path) else {
            return false;
        };

        let Ok(value) = serde_json::from_reader::<_, Value>(BufReader::new(file)) else {
            return false;
        };

        is_gradle_module_json(&value)
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(e) => {
                warn!("Failed to open Gradle module file at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        let json: Value = match serde_json::from_reader(BufReader::new(file)) {
            Ok(json) => json,
            Err(e) => {
                warn!("Failed to parse Gradle module JSON at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        if !is_gradle_module_json(&json) {
            warn!("File at {:?} is not valid Gradle module metadata", path);
            return vec![default_package_data()];
        }

        vec![parse_gradle_module(&json)]
    }
}

fn is_gradle_module_json(json: &Value) -> bool {
    let Some(component) = json.get(FIELD_COMPONENT).and_then(Value::as_object) else {
        return false;
    };

    json.get(FIELD_FORMAT_VERSION)
        .and_then(Value::as_str)
        .is_some()
        && component.get("group").and_then(Value::as_str).is_some()
        && component.get("module").and_then(Value::as_str).is_some()
        && component.get("version").and_then(Value::as_str).is_some()
}

fn parse_gradle_module(json: &Value) -> PackageData {
    let component = json
        .get(FIELD_COMPONENT)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let namespace = component
        .get("group")
        .and_then(Value::as_str)
        .map(|value| value.to_string());
    let name = component
        .get("module")
        .and_then(Value::as_str)
        .map(|value| value.to_string());
    let version = component
        .get("version")
        .and_then(Value::as_str)
        .map(|value| value.to_string());

    let (dependencies, file_references, top_level_artifact, variant_metadata) =
        extract_variant_data(json.get(FIELD_VARIANTS).and_then(Value::as_array));

    let purl = match (namespace.as_deref(), name.as_deref(), version.as_deref()) {
        (Some(namespace), Some(name), version) => build_maven_purl(namespace, name, version),
        _ => None,
    };

    let mut extra_data = HashMap::new();
    if let Some(format_version) = json.get(FIELD_FORMAT_VERSION).and_then(Value::as_str) {
        extra_data.insert(
            "format_version".to_string(),
            Value::String(format_version.to_string()),
        );
    }

    if let Some(gradle_object) = json
        .get(FIELD_CREATED_BY)
        .and_then(Value::as_object)
        .and_then(|created_by| created_by.get("gradle"))
        .and_then(Value::as_object)
    {
        if let Some(gradle_version) = gradle_object.get("version").and_then(Value::as_str) {
            extra_data.insert(
                "gradle_version".to_string(),
                Value::String(gradle_version.to_string()),
            );
        }
        if let Some(build_id) = gradle_object.get("buildId").and_then(Value::as_str) {
            extra_data.insert("build_id".to_string(), Value::String(build_id.to_string()));
        }
    }

    if let Some(attributes) = component.get(FIELD_ATTRIBUTES).and_then(Value::as_object)
        && !attributes.is_empty()
    {
        extra_data.insert(
            "component_attributes".to_string(),
            Value::Object(attributes.clone()),
        );
    }

    if !variant_metadata.is_empty() {
        extra_data.insert("variants".to_string(), Value::Array(variant_metadata));
    }

    let (size, sha1, md5, sha256, sha512) = top_level_artifact
        .as_ref()
        .map(extract_file_hashes)
        .unwrap_or((None, None, None, None, None));

    PackageData {
        package_type: Some(GradleModuleParser::PACKAGE_TYPE),
        namespace,
        name,
        version,
        qualifiers: None,
        subpath: None,
        primary_language: Some("Java".to_string()),
        description: None,
        release_date: None,
        parties: Vec::new(),
        keywords: Vec::new(),
        homepage_url: None,
        download_url: None,
        size,
        sha1,
        md5,
        sha256,
        sha512,
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
        file_references,
        is_private: false,
        is_virtual: false,
        extra_data: (!extra_data.is_empty()).then_some(extra_data),
        dependencies,
        repository_homepage_url: None,
        repository_download_url: None,
        api_data_url: None,
        datasource_id: Some(DatasourceId::GradleModule),
        purl,
    }
}

fn extract_variant_data(variants: Option<&Vec<Value>>) -> ExtractedVariantData {
    let mut dependencies = Vec::new();
    let mut file_references = Vec::new();
    let mut variant_metadata = Vec::new();
    let mut seen_dependencies: HashMap<(String, String, Option<String>), ExtractedDependency> =
        HashMap::new();
    let mut seen_files: HashSet<String> = HashSet::new();
    let mut top_level_artifact: Option<JsonMap<String, Value>> = None;

    for variant in variants.into_iter().flatten().filter_map(Value::as_object) {
        let category = variant
            .get(FIELD_ATTRIBUTES)
            .and_then(Value::as_object)
            .and_then(|attrs| attrs.get("org.gradle.category"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let is_documentation = category == "documentation";

        let variant_name = variant
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let scope = classify_variant_scope(variant);
        let precedence = scope_precedence(scope.as_deref());
        let is_runtime = Some(scope.as_deref() != Some("test"));
        let is_optional = Some(scope.as_deref() == Some("test"));

        let mut variant_entry = JsonMap::new();
        variant_entry.insert("name".to_string(), Value::String(variant_name.clone()));
        if let Some(attributes) = variant.get(FIELD_ATTRIBUTES) {
            variant_entry.insert(FIELD_ATTRIBUTES.to_string(), attributes.clone());
        }
        if let Some(available_at) = variant.get(FIELD_AVAILABLE_AT) {
            variant_entry.insert("available_at".to_string(), available_at.clone());
        }
        if let Some(constraints) = variant.get(FIELD_DEPENDENCY_CONSTRAINTS) {
            variant_entry.insert("dependency_constraints".to_string(), constraints.clone());
        }
        variant_metadata.push(Value::Object(variant_entry));

        if !is_documentation {
            if top_level_artifact.is_none() {
                top_level_artifact = variant
                    .get(FIELD_FILES)
                    .and_then(Value::as_array)
                    .and_then(|files| files.first())
                    .and_then(Value::as_object)
                    .cloned();
            }

            if let Some(files) = variant.get(FIELD_FILES).and_then(Value::as_array) {
                for file in files.iter().filter_map(Value::as_object) {
                    let file_path = file
                        .get("url")
                        .and_then(Value::as_str)
                        .or_else(|| file.get("name").and_then(Value::as_str))
                        .unwrap_or_default()
                        .to_string();
                    if file_path.is_empty() || !seen_files.insert(file_path.clone()) {
                        continue;
                    }
                    let (size, sha1, md5, sha256, sha512) = extract_file_hashes(file);
                    let mut extra_data = HashMap::new();
                    if let Some(name) = file.get("name").and_then(Value::as_str) {
                        extra_data.insert("name".to_string(), Value::String(name.to_string()));
                    }
                    file_references.push(FileReference {
                        path: file_path,
                        size,
                        sha1,
                        md5,
                        sha256,
                        sha512,
                        extra_data: (!extra_data.is_empty()).then_some(extra_data),
                    });
                }
            }
        }

        if is_documentation {
            continue;
        }

        for dependency in variant
            .get(FIELD_DEPENDENCIES)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_object)
        {
            let Some(group) = dependency.get("group").and_then(Value::as_str) else {
                continue;
            };
            let Some(module) = dependency.get("module").and_then(Value::as_str) else {
                continue;
            };

            let requirement = extract_dependency_requirement(dependency.get("version"));
            let key = (group.to_string(), module.to_string(), requirement.clone());
            let purl = build_maven_purl(group, module, requirement.as_deref());
            let dep_extra_data =
                build_dependency_extra_data(dependency, &variant_name, scope.as_deref());

            let entry = seen_dependencies.entry(key).or_default();
            if precedence < entry.precedence || entry.scope.is_none() {
                entry.scope = scope.clone();
                entry.is_runtime = is_runtime;
                entry.is_optional = is_optional;
                entry.precedence = precedence;
            }
            entry.purl = purl;
            entry.extracted_requirement = requirement.clone();
            entry.is_pinned = Some(requirement.as_deref().is_some_and(is_exact_version));
            entry.extra_data = merge_dependency_extra_data(entry.extra_data.take(), dep_extra_data);
        }
    }

    for dep in seen_dependencies.into_values() {
        dependencies.push(Dependency {
            purl: dep.purl,
            extracted_requirement: dep.extracted_requirement,
            scope: dep.scope,
            is_runtime: dep.is_runtime,
            is_optional: dep.is_optional,
            is_pinned: dep.is_pinned,
            is_direct: Some(true),
            resolved_package: None,
            extra_data: dep.extra_data,
        });
    }

    dependencies.sort_by(|left, right| left.purl.cmp(&right.purl));
    file_references.sort_by(|left, right| left.path.cmp(&right.path));

    (
        dependencies,
        file_references,
        top_level_artifact,
        variant_metadata,
    )
}

fn build_dependency_extra_data(
    dependency: &JsonMap<String, Value>,
    variant_name: &str,
    scope: Option<&str>,
) -> Option<HashMap<String, Value>> {
    let mut extra = HashMap::new();
    extra.insert(
        "variant_names".to_string(),
        Value::Array(vec![Value::String(variant_name.to_string())]),
    );
    if let Some(scope) = scope {
        extra.insert(
            "variant_scopes".to_string(),
            Value::Array(vec![Value::String(scope.to_string())]),
        );
    }

    let mut variant_entry = JsonMap::new();
    variant_entry.insert(
        "variant_name".to_string(),
        Value::String(variant_name.to_string()),
    );
    if let Some(scope) = scope {
        variant_entry.insert(
            "variant_scope".to_string(),
            Value::String(scope.to_string()),
        );
    }
    variant_entry.insert("dependency".to_string(), Value::Object(dependency.clone()));
    extra.insert(
        "variant_dependency_entries".to_string(),
        Value::Array(vec![Value::Object(variant_entry)]),
    );

    for field in [
        FIELD_ATTRIBUTES,
        "reason",
        "requestedCapabilities",
        "excludes",
        "endorseStrictVersions",
        "thirdPartyCompatibility",
        "version",
    ] {
        if let Some(value) = dependency.get(field) {
            extra.insert(field.to_string(), value.clone());
        }
    }
    (!extra.is_empty()).then_some(extra)
}

fn merge_dependency_extra_data(
    current: Option<HashMap<String, Value>>,
    next: Option<HashMap<String, Value>>,
) -> Option<HashMap<String, Value>> {
    match (current, next) {
        (None, None) => None,
        (Some(map), None) | (None, Some(map)) => Some(map),
        (Some(mut current), Some(mut next)) => {
            merge_string_arrays(&mut current, &mut next, "variant_names");
            merge_string_arrays(&mut current, &mut next, "variant_scopes");
            merge_object_arrays(&mut current, &mut next, "variant_dependency_entries");
            for (key, value) in next {
                current.entry(key).or_insert(value);
            }
            Some(current)
        }
    }
}

fn merge_object_arrays(
    current: &mut HashMap<String, Value>,
    next: &mut HashMap<String, Value>,
    key: &str,
) {
    let existing = current
        .remove(key)
        .and_then(|value| value.as_array().cloned());
    let incoming = next.remove(key).and_then(|value| value.as_array().cloned());

    let mut values = Vec::new();
    for array in [existing, incoming].into_iter().flatten() {
        for value in array {
            if !values.contains(&value) {
                values.push(value);
            }
        }
    }

    if !values.is_empty() {
        current.insert(key.to_string(), Value::Array(values));
    }
}

fn merge_string_arrays(
    current: &mut HashMap<String, Value>,
    next: &mut HashMap<String, Value>,
    key: &str,
) {
    let existing = current
        .remove(key)
        .and_then(|value| value.as_array().cloned());
    let incoming = next.remove(key).and_then(|value| value.as_array().cloned());

    let mut values = Vec::new();
    for array in [existing, incoming].into_iter().flatten() {
        for value in array
            .into_iter()
            .filter_map(|value| value.as_str().map(|s| s.to_string()))
        {
            if !values.contains(&value) {
                values.push(value);
            }
        }
    }

    if !values.is_empty() {
        current.insert(
            key.to_string(),
            Value::Array(values.into_iter().map(Value::String).collect()),
        );
    }
}

fn classify_variant_scope(variant: &JsonMap<String, Value>) -> Option<String> {
    let variant_name = variant
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();

    if variant_name.contains("test") {
        return Some("test".to_string());
    }

    let usage = variant
        .get(FIELD_ATTRIBUTES)
        .and_then(Value::as_object)
        .and_then(|attributes| attributes.get("org.gradle.usage"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();

    if usage.contains("api") || (variant_name.contains("api") && !variant_name.contains("runtime"))
    {
        return Some("compile".to_string());
    }
    if usage.contains("runtime") || variant_name.contains("runtime") {
        return Some("runtime".to_string());
    }

    (!variant_name.is_empty()).then_some(variant_name)
}

fn scope_precedence(scope: Option<&str>) -> u8 {
    match scope {
        Some("compile") => 0,
        Some("runtime") => 1,
        Some("test") => 2,
        _ => 3,
    }
}

fn extract_dependency_requirement(version_value: Option<&Value>) -> Option<String> {
    match version_value {
        Some(Value::String(version)) => Some(version.to_string()),
        Some(Value::Object(version)) => version
            .get("strictly")
            .or_else(|| version.get("requires"))
            .or_else(|| version.get("prefers"))
            .and_then(Value::as_str)
            .map(|value| value.to_string()),
        _ => None,
    }
}

fn extract_file_hashes(file: &JsonMap<String, Value>) -> ArtifactHashes {
    let sha256 = file
        .get("sha256")
        .and_then(Value::as_str)
        .and_then(|value| Sha256Digest::from_hex(value).ok());

    let sha512_field = file.get("sha512").and_then(Value::as_str);
    let (sha256, sha512) = match sha512_field {
        Some(hex) if hex.len() == 64 && hex::decode(hex).is_ok() => {
            let misassigned = Sha256Digest::from_hex(hex).ok();
            (sha256.or(misassigned), None)
        }
        Some(hex) => (sha256, Sha512Digest::from_hex(hex).ok()),
        None => (sha256, None),
    };

    (
        file.get("size").and_then(Value::as_u64),
        file.get("sha1")
            .and_then(Value::as_str)
            .and_then(|value| Sha1Digest::from_hex(value).ok()),
        file.get("md5")
            .and_then(Value::as_str)
            .and_then(|value| Md5Digest::from_hex(value).ok()),
        sha256,
        sha512,
    )
}

fn build_maven_purl(namespace: &str, name: &str, version: Option<&str>) -> Option<String> {
    let mut purl = PackageUrl::new("maven", name).ok()?;
    if !namespace.trim().is_empty() {
        purl.with_namespace(namespace).ok()?;
    }
    if let Some(version) = version.filter(|value| !value.trim().is_empty()) {
        purl.with_version(version).ok()?;
    }
    Some(purl.to_string())
}

fn is_exact_version(version: &str) -> bool {
    !version.contains('[')
        && !version.contains(']')
        && !version.contains('(')
        && !version.contains(')')
        && !version.contains(',')
        && !version.contains('+')
        && !version.contains('*')
        && !version.contains('>')
        && !version.contains('<')
        && !version.contains(' ')
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(GradleModuleParser::PACKAGE_TYPE),
        datasource_id: Some(DatasourceId::GradleModule),
        ..Default::default()
    }
}

crate::register_parser!(
    "Gradle module metadata",
    &["**/*.module"],
    "maven",
    "Java",
    Some("https://docs.gradle.org/current/userguide/publishing_gradle_module_metadata.html"),
);
