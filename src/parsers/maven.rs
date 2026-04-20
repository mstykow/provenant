//! Parser for Apache Maven pom.xml files.
//!
//! Extracts package metadata, dependencies, and license information from
//! Maven Project Object Model (POM) files.
//!
//! # Supported Formats
//! - pom.xml (Project Object Model)
//! - pom.properties
//! - MANIFEST.MF (JAR manifest)
//!
//! # Key Features
//! - Property value substitution (`${project.version}`)
//! - `is_pinned` analysis (exact version vs ranges like `[1.0,2.0)`)
//! - Dependency scope handling (compile, test, provided, runtime, system)
//! - Package URL (purl) generation
//! - Multiple license support (combined with " OR ")
//!
//! # Implementation Notes
//! - Uses quick-xml for XML parsing
//! - Version pinning: `"1.0.0"` is pinned, `"[1.0,2.0)"` is not
//! - Property substitution limited to prevent infinite loops
//! - Direct dependencies: all in pom.xml are direct

use crate::models::{DatasourceId, Dependency, PackageData, PackageType, Party};
use crate::parser_warn as warn;
use crate::parsers::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};
use quick_xml::Reader;
use quick_xml::events::Event;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::PackageParser;
use super::license_normalization::{
    DeclaredLicenseMatchMetadata, NormalizedDeclaredLicense, build_declared_license_data,
    combine_normalized_licenses, empty_declared_license_data, normalize_declared_license_key,
};

#[derive(Clone, Default)]
struct MavenDependencyData {
    group_id: Option<String>,
    artifact_id: Option<String>,
    version: Option<String>,
    classifier: Option<String>,
    type_: Option<String>,
    scope: Option<String>,
    optional: Option<String>,
    system_path: Option<String>,
    message: Option<String>,
}

#[derive(Clone, Default)]
struct MavenLicenseEntry {
    name: Option<String>,
    url: Option<String>,
    comments: Option<String>,
}

/// Resolves Maven property placeholders (`${property.name}`) with cycle and DoS protection.
///
/// Maven properties can reference other properties, creating dependency graphs. This resolver:
/// - Resolves nested placeholders: `${outer.${inner}}`
/// - Detects circular references: `${a}` → `${b}` → `${a}`
/// - Enforces depth limits to prevent stack overflow
/// - Enforces substitution limits to prevent DoS on pathological inputs
///
/// # Algorithm
///
/// Uses byte-level parsing for efficient placeholder extraction. Tracks:
/// - `resolving_set`: For cycle detection (hash set lookup)
/// - `resolving_stack`: For error reporting (preserves path)
/// - `cache`: Memoizes resolved values to avoid redundant work
struct PropertyResolver {
    raw: HashMap<String, String>,
    builtins: HashMap<String, String>,
    cache: HashMap<String, String>,
    resolving_set: HashSet<String>,
    resolving_stack: Vec<String>,
    max_depth: usize,
    max_output_len: usize,
    max_substitutions: usize,
    warned_keys: HashSet<String>,
}

impl PropertyResolver {
    fn new(raw: HashMap<String, String>, builtins: HashMap<String, String>) -> Self {
        Self {
            raw,
            builtins,
            cache: HashMap::new(),
            resolving_set: HashSet::new(),
            resolving_stack: Vec::new(),
            max_depth: 10,
            max_output_len: 100_000,
            max_substitutions: 1000,
            warned_keys: HashSet::new(),
        }
    }

    fn resolve_key(&mut self, key: &str, depth: usize) -> Option<String> {
        if let Some(value) = self.cache.get(key) {
            return Some(value.clone());
        }

        if depth >= self.max_depth {
            self.warn_once(
                "depth",
                key,
                format!("Maven property depth limit hit resolving {key}"),
            );
            return None;
        }

        if self.resolving_set.contains(key) {
            if self
                .resolving_stack
                .last()
                .is_some_and(|current| current == key)
            {
                return None;
            }

            self.warn_once(
                "cycle",
                key,
                format!(
                    "Maven property cycle detected at {key}: {:?}",
                    self.resolving_stack
                ),
            );
            return None;
        }

        let raw_val = if let Some(value) = self.raw.get(key).or_else(|| self.builtins.get(key)) {
            value.clone()
        } else {
            return None;
        };

        self.resolving_set.insert(key.to_string());
        self.resolving_stack.push(key.to_string());

        let resolved = self.resolve_text(&raw_val, depth + 1);

        self.resolving_stack.pop();
        self.resolving_set.remove(key);

        self.cache.insert(key.to_string(), resolved.clone());
        Some(resolved)
    }

    fn resolve_text(&mut self, text: &str, depth: usize) -> String {
        if !text.contains("${") {
            return text.to_string();
        }

        if depth >= self.max_depth {
            warn!("Maven property depth limit hit resolving text");
            return text.to_string();
        }

        let bytes = text.as_bytes();
        let mut output: Vec<u8> = Vec::with_capacity(bytes.len());
        let mut index = 0;
        let mut substitutions = 0;

        while index < bytes.len() {
            if bytes[index] == b'$' && index + 1 < bytes.len() && bytes[index + 1] == b'{' {
                if substitutions >= self.max_substitutions {
                    warn!("Maven property substitution limit hit resolving {text}");
                    return text.to_string();
                }

                let placeholder_start = index;
                let Some((content, closing_index)) =
                    self.parse_placeholder_content(text, index + 2)
                else {
                    warn!("Maven property malformed placeholder in {text}");
                    return text.to_string();
                };

                substitutions += 1;
                let resolved_key = if content.contains("${") {
                    self.resolve_text(content, depth + 1)
                } else {
                    content.to_string()
                };

                if let Some(resolved) = self.resolve_key(&resolved_key, depth) {
                    if output.len() + resolved.len() > self.max_output_len {
                        warn!("Maven property output length limit hit resolving {text}");
                        return text.to_string();
                    }
                    output.extend_from_slice(resolved.as_bytes());
                } else {
                    let placeholder_bytes = &bytes[placeholder_start..=closing_index];
                    if output.len() + placeholder_bytes.len() > self.max_output_len {
                        warn!("Maven property output length limit hit resolving {text}");
                        return text.to_string();
                    }
                    output.extend_from_slice(placeholder_bytes);
                }

                index = closing_index + 1;
                continue;
            }

            if output.len() + 1 > self.max_output_len {
                warn!("Maven property output length limit hit resolving {text}");
                return text.to_string();
            }

            output.push(bytes[index]);
            index += 1;
        }

        String::from_utf8(output).unwrap_or_else(|_| text.to_string())
    }

    fn parse_placeholder_content<'a>(
        &self,
        text: &'a str,
        start_index: usize,
    ) -> Option<(&'a str, usize)> {
        let bytes = text.as_bytes();
        let mut index = start_index;
        let mut depth = 0;

        while index < bytes.len() {
            if bytes[index] == b'$' && index + 1 < bytes.len() && bytes[index + 1] == b'{' {
                depth += 1;
                index += 2;
                continue;
            }

            if bytes[index] == b'}' {
                if depth == 0 {
                    return Some((&text[start_index..index], index));
                }
                depth -= 1;
            }

            index += 1;
        }

        None
    }

    fn warn_once(&mut self, kind: &str, key: &str, message: String) {
        let token = format!("{kind}:{key}");
        if self.warned_keys.insert(token) {
            warn!("{message}");
        }
    }
}

fn sanitize_template_directives(content: &str) -> Cow<'_, str> {
    if !content.contains("<%") {
        return Cow::Borrowed(content);
    }

    let mut sanitized = String::with_capacity(content.len());
    let mut remaining = content;

    while let Some(start) = remaining.find("<%") {
        let (before, after_start) = remaining.split_at(start);
        sanitized.push_str(before);

        let Some(end) = after_start.find("%>") else {
            return Cow::Borrowed(content);
        };

        let directive = &after_start[..end + 2];
        for ch in directive.chars() {
            if matches!(ch, '\n' | '\r') {
                sanitized.push(ch);
            } else {
                sanitized.push(' ');
            }
        }

        remaining = &after_start[end + 2..];
    }

    sanitized.push_str(remaining);
    Cow::Owned(sanitized)
}

fn resolve_option(resolver: &mut PropertyResolver, value: &mut Option<String>) {
    if let Some(current) = value.clone() {
        *value = Some(resolver.resolve_text(&current, 0));
    }
}

fn resolve_vec(resolver: &mut PropertyResolver, values: &mut [String]) {
    for value in values.iter_mut() {
        *value = resolver.resolve_text(value, 0);
    }
}

fn resolve_map_strings(
    resolver: &mut PropertyResolver,
    values: &mut serde_json::Map<String, serde_json::Value>,
) {
    for value in values.values_mut() {
        if let serde_json::Value::String(current) = value {
            let resolved = resolver.resolve_text(current, 0);
            *current = resolved;
        }
    }
}

fn resolve_maps(
    resolver: &mut PropertyResolver,
    values: &mut [serde_json::Map<String, serde_json::Value>],
) {
    for value in values.iter_mut() {
        resolve_map_strings(resolver, value);
    }
}

fn resolve_dependency_data(resolver: &mut PropertyResolver, dependency: &mut MavenDependencyData) {
    resolve_option(resolver, &mut dependency.group_id);
    resolve_option(resolver, &mut dependency.artifact_id);
    resolve_option(resolver, &mut dependency.version);
    resolve_option(resolver, &mut dependency.classifier);
    resolve_option(resolver, &mut dependency.type_);
    resolve_option(resolver, &mut dependency.scope);
    resolve_option(resolver, &mut dependency.optional);
    resolve_option(resolver, &mut dependency.system_path);
    resolve_option(resolver, &mut dependency.message);
}

fn parse_maven_bool(value: Option<&str>) -> bool {
    value.is_some_and(|value| value.trim().eq_ignore_ascii_case("true"))
}

fn normalize_maven_packaging(packaging: Option<&str>) -> Option<&str> {
    match packaging.map(str::trim).filter(|value| !value.is_empty()) {
        Some(
            "ejb3" | "ear" | "aar" | "apk" | "gem" | "jar" | "nar" | "pom" | "so" | "swc" | "tar"
            | "tar.gz" | "war" | "xar" | "zip",
        ) => packaging.map(str::trim),
        Some(_) => Some("jar"),
        None => None,
    }
}

fn resolve_license_entry(resolver: &mut PropertyResolver, license: &mut MavenLicenseEntry) {
    resolve_option(resolver, &mut license.name);
    resolve_option(resolver, &mut license.url);
    resolve_option(resolver, &mut license.comments);
}

fn build_maven_qualifiers(
    classifier: Option<&str>,
    packaging: Option<&str>,
) -> Option<HashMap<String, String>> {
    let mut qualifiers = HashMap::new();

    if let Some(classifier) = classifier.filter(|value| !value.trim().is_empty()) {
        qualifiers.insert("classifier".to_string(), classifier.to_string());
    }

    if let Some(packaging) = normalize_maven_packaging(packaging)
        .filter(|value| !value.is_empty() && *value != "jar" && *value != "pom")
    {
        qualifiers.insert("type".to_string(), packaging.to_string());
    }

    (!qualifiers.is_empty()).then_some(qualifiers)
}

fn build_maven_purl(
    group_id: &str,
    artifact_id: &str,
    version: Option<&str>,
    classifier: Option<&str>,
    packaging: Option<&str>,
) -> String {
    let mut purl = format!(
        "pkg:maven/{}/{}",
        percent_encode_purl_component(group_id),
        percent_encode_purl_component(artifact_id)
    );

    if let Some(version) = version.filter(|value| !value.trim().is_empty()) {
        purl.push('@');
        purl.push_str(&percent_encode_purl_component(version));
    }

    let qualifiers = build_maven_qualifiers(classifier, packaging);
    if let Some(qualifiers) = qualifiers {
        let mut query_parts = Vec::new();
        if let Some(classifier) = qualifiers.get("classifier") {
            query_parts.push(format!(
                "classifier={}",
                percent_encode_purl_component(classifier)
            ));
        }
        if let Some(type_) = qualifiers.get("type") {
            query_parts.push(format!("type={}", percent_encode_purl_component(type_)));
        }

        if !query_parts.is_empty() {
            purl.push('?');
            purl.push_str(&query_parts.join("&"));
        }
    }

    purl
}

fn percent_encode_purl_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());

    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }

    encoded
}

fn build_maven_download_url(
    group_id: &str,
    artifact_id: &str,
    version: &str,
    classifier: Option<&str>,
    packaging: Option<&str>,
) -> String {
    const BASE_URL: &str = "https://repo1.maven.org/maven2";
    let group_path = group_id.replace('.', "/");
    let extension = normalize_maven_packaging(packaging)
        .filter(|value| *value != "pom")
        .unwrap_or("jar");
    let classifier_suffix = classifier
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("-{value}"))
        .unwrap_or_default();

    format!(
        "{}/{}/{}/{}/{}-{}{}.{}",
        BASE_URL,
        group_path,
        artifact_id,
        version,
        artifact_id,
        version,
        classifier_suffix,
        extension
    )
}

fn build_maven_source_package(namespace: &str, name: &str, version: &str) -> String {
    build_maven_purl(namespace, name, Some(version), Some("sources"), None)
}

fn build_license_statement(licenses: &[MavenLicenseEntry]) -> Option<String> {
    let rendered_entries: Vec<String> = licenses
        .iter()
        .filter_map(|license| {
            let mut lines = Vec::new();

            if let Some(name) = license
                .name
                .as_ref()
                .filter(|value| !value.trim().is_empty())
            {
                lines.push(format!("    name: {name}"));
            }
            if let Some(url) = license
                .url
                .as_ref()
                .filter(|value| !value.trim().is_empty())
            {
                lines.push(format!("    url: {url}"));
            }
            if let Some(comments) = license
                .comments
                .as_ref()
                .filter(|value| !value.trim().is_empty())
            {
                lines.push(format!("    comments: {comments}"));
            }

            (!lines.is_empty()).then(|| format!("- license:\n{}", lines.join("\n")))
        })
        .collect();

    if rendered_entries.is_empty() {
        None
    } else {
        Some(format!("{}\n", rendered_entries.join("\n")))
    }
}

fn is_license_like_comment(comment: &str) -> bool {
    let lowered = comment.to_ascii_lowercase();
    [
        "license",
        "licensed",
        "copyright",
        "spdx",
        "apache",
        "mit",
        "bsd",
        "gpl",
        "lgpl",
        "mozilla public",
        "eclipse public",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
}

fn dependency_extra_data(
    dependency: &MavenDependencyData,
) -> Option<HashMap<String, serde_json::Value>> {
    let mut extra_data = HashMap::new();

    if let Some(classifier) = dependency
        .classifier
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        extra_data.insert(
            "classifier".to_string(),
            serde_json::Value::String(classifier.clone()),
        );
    }
    if let Some(type_) = dependency
        .type_
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        extra_data.insert("type".to_string(), serde_json::Value::String(type_.clone()));
    }
    if let Some(system_path) = dependency
        .system_path
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        extra_data.insert(
            "system_path".to_string(),
            serde_json::Value::String(system_path.clone()),
        );
    }
    if let Some(message) = dependency
        .message
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        extra_data.insert(
            "message".to_string(),
            serde_json::Value::String(message.clone()),
        );
    }

    (!extra_data.is_empty()).then_some(extra_data)
}

fn dependency_management_entry_to_value(
    dependency: &MavenDependencyData,
) -> serde_json::Map<String, serde_json::Value> {
    let mut dep_obj = serde_json::Map::new();

    if let Some(group_id) = dependency.group_id.as_ref() {
        dep_obj.insert(
            "groupId".to_string(),
            serde_json::Value::String(group_id.clone()),
        );
    }
    if let Some(artifact_id) = dependency.artifact_id.as_ref() {
        dep_obj.insert(
            "artifactId".to_string(),
            serde_json::Value::String(artifact_id.clone()),
        );
    }
    if let Some(version) = dependency.version.as_ref() {
        dep_obj.insert(
            "version".to_string(),
            serde_json::Value::String(version.clone()),
        );
    }
    if let Some(scope) = dependency.scope.as_ref() {
        dep_obj.insert(
            "scope".to_string(),
            serde_json::Value::String(scope.clone()),
        );
    }
    if let Some(type_) = dependency.type_.as_ref() {
        dep_obj.insert("type".to_string(), serde_json::Value::String(type_.clone()));
    }
    if let Some(classifier) = dependency.classifier.as_ref() {
        dep_obj.insert(
            "classifier".to_string(),
            serde_json::Value::String(classifier.clone()),
        );
    }
    if let Some(optional) = dependency.optional.as_deref() {
        dep_obj.insert(
            "optional".to_string(),
            serde_json::Value::Bool(parse_maven_bool(Some(optional))),
        );
    }
    if let Some(message) = dependency.message.as_ref() {
        dep_obj.insert(
            "message".to_string(),
            serde_json::Value::String(message.clone()),
        );
    }

    dep_obj
}

fn maven_dependency_to_dependency(
    dependency_data: &MavenDependencyData,
    fallback_scope: Option<&str>,
    force_non_runtime: bool,
) -> Option<Dependency> {
    let group_id = dependency_data.group_id.as_ref()?;
    let artifact_id = dependency_data.artifact_id.as_ref()?;
    let version = dependency_data.version.clone();
    let scope = dependency_data
        .scope
        .clone()
        .or_else(|| fallback_scope.map(str::to_string));
    let explicit_optional = parse_maven_bool(dependency_data.optional.as_deref());

    let (is_runtime, is_optional) = if force_non_runtime {
        (Some(false), Some(explicit_optional))
    } else {
        match scope.as_deref() {
            Some("test") | Some("provided") => (Some(false), Some(true)),
            Some(_) => (Some(true), Some(explicit_optional)),
            None => (None, Some(explicit_optional)),
        }
    };

    Some(Dependency {
        purl: Some(build_maven_purl(
            group_id,
            artifact_id,
            version.as_deref(),
            dependency_data.classifier.as_deref(),
            dependency_data.type_.as_deref(),
        )),
        extracted_requirement: version.clone(),
        scope,
        is_runtime,
        is_optional,
        is_pinned: version.as_deref().map(is_maven_version_pinned),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: dependency_extra_data(dependency_data),
    })
}

/// Determines if a Maven version specifier is pinned to an exact version.
///
/// A version is considered pinned if it specifies an exact version without
/// range syntax or dynamic keywords. Examples:
/// - Pinned: "1.0.0", "1.2.3"
/// - NOT pinned: "[1.0.0,2.0.0)" (range), "[1.0.0,)" (open-ended), "LATEST", "RELEASE"
fn is_maven_version_pinned(version_str: &str) -> bool {
    let trimmed = version_str.trim();

    // Empty version is not pinned
    if trimmed.is_empty() {
        return false;
    }

    // Check for range syntax (brackets and parentheses)
    if trimmed.contains('[')
        || trimmed.contains(']')
        || trimmed.contains('(')
        || trimmed.contains(')')
    {
        return false;
    }

    // Check for dynamic version keywords
    if trimmed.eq_ignore_ascii_case("LATEST") || trimmed.eq_ignore_ascii_case("RELEASE") {
        return false;
    }

    // If none of the unpinned indicators are present, it's pinned
    true
}

struct MavenBuiltinPropertyInputs<'a> {
    namespace: &'a Option<String>,
    name: &'a Option<String>,
    version: &'a Option<String>,
    parent_group_id: &'a Option<String>,
    parent_artifact_id: &'a Option<String>,
    parent_version: &'a Option<String>,
    project_name: &'a Option<String>,
    project_packaging: &'a Option<String>,
}

fn build_builtin_properties(inputs: MavenBuiltinPropertyInputs<'_>) -> HashMap<String, String> {
    let mut builtins = HashMap::new();
    let effective_group_id = inputs
        .namespace
        .clone()
        .or_else(|| inputs.parent_group_id.clone());
    let effective_version = inputs
        .version
        .clone()
        .or_else(|| inputs.parent_version.clone());

    if let Some(group_id) = effective_group_id.clone() {
        builtins.insert("project.groupId".to_string(), group_id.clone());
        builtins.insert("pom.groupId".to_string(), group_id);
    }

    if let Some(artifact_id) = inputs.name.clone() {
        builtins.insert("project.artifactId".to_string(), artifact_id.clone());
        builtins.insert("pom.artifactId".to_string(), artifact_id);
    }

    if let Some(ver) = effective_version.clone() {
        builtins.insert("project.version".to_string(), ver.clone());
        builtins.insert("pom.version".to_string(), ver);
    }

    if let Some(group_id) = inputs.parent_group_id.clone() {
        builtins.insert("project.parent.groupId".to_string(), group_id);
    }

    if let Some(artifact_id) = inputs.parent_artifact_id.clone() {
        builtins.insert("project.parent.artifactId".to_string(), artifact_id.clone());
        builtins.insert("pom.parent.artifactId".to_string(), artifact_id.clone());
        builtins.insert("parent.artifactId".to_string(), artifact_id);
    }

    if let Some(ver) = inputs.parent_version.clone() {
        builtins.insert("project.parent.version".to_string(), ver.clone());
        builtins.insert("pom.parent.version".to_string(), ver.clone());
        builtins.insert("parent.version".to_string(), ver);
    }

    if let Some(packaging) = inputs.project_packaging.clone() {
        builtins.insert("project.packaging".to_string(), packaging);
    }

    if let Some(name) = inputs.project_name.clone() {
        builtins.insert("project.name".to_string(), name);
    }

    builtins
}

/// Maven package parser supporting pom.xml, pom.properties, and MANIFEST.MF files.
///
/// Handles Maven property resolution (`${property.name}` syntax) with cycle detection
/// and depth limits. See `PropertyResolver` for property substitution algorithm details.
pub struct MavenParser;

impl PackageParser for MavenParser {
    const PACKAGE_TYPE: PackageType = PackageType::Maven;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        if let Some(filename) = path.file_name().and_then(|name| name.to_str()) {
            if filename == "pom.properties" {
                return vec![parse_pom_properties(path)];
            } else if filename == "MANIFEST.MF" {
                return vec![parse_manifest_mf(path)];
            }
        }

        let content = match read_file_to_string(path, None).map_err(|e| e.to_string()) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to open pom.xml at {:?}: {}", path, e);
                return vec![default_package_data(DatasourceId::MavenPom)];
            }
        };

        let sanitized_content = sanitize_template_directives(&content);
        let mut reader = Reader::from_str(sanitized_content.as_ref());
        reader.config_mut().trim_text(true);

        let mut buf = Vec::new();
        let mut package_data = default_package_data(DatasourceId::MavenPom);
        package_data.package_type = Some(Self::PACKAGE_TYPE);
        package_data.primary_language = Some("Java".to_string());
        package_data.datasource_id = Some(DatasourceId::MavenPom);

        let mut current_element = Vec::new();
        let mut in_dependencies = false;
        let mut current_dependency: Option<Dependency> = None;
        let mut dependency_data: Vec<MavenDependencyData> = Vec::new();
        let mut current_dependency_data: Option<MavenDependencyData> = None;

        let mut licenses: Vec<MavenLicenseEntry> = Vec::new();
        let mut xml_license_comments: Vec<String> = Vec::new();
        let mut current_license: Option<MavenLicenseEntry> = None;
        let mut inception_year = None;
        let mut scm_connection = None;
        let mut scm_developer_connection = None;
        let mut scm_url = None;
        let mut scm_tag = None;
        let mut organization_name = None;
        let mut organization_url = None;
        let mut in_developers = false;
        let mut in_contributors = false;
        let mut current_party: Option<Party> = None;
        let mut issue_management_system = None;
        let mut issue_management_url = None;
        let mut ci_management_system = None;
        let mut ci_management_url = None;
        let mut in_distribution_management = false;
        let mut in_dist_repository = false;
        let mut in_dist_snapshot_repository = false;
        let mut in_dist_site = false;
        let mut dist_download_url = None;
        let mut dist_repository_id = None;
        let mut dist_repository_name = None;
        let mut dist_repository_url = None;
        let mut dist_repository_layout = None;
        let mut dist_snapshot_repository_id = None;
        let mut dist_snapshot_repository_name = None;
        let mut dist_snapshot_repository_url = None;
        let mut dist_snapshot_repository_layout = None;
        let mut dist_site_id = None;
        let mut dist_site_name = None;
        let mut dist_site_url = None;
        let mut in_repositories = false;
        let mut in_plugin_repositories = false;
        let mut in_repository = false;
        let mut repositories: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();
        let mut plugin_repositories: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();
        let mut current_repository_id = None;
        let mut current_repository_name = None;
        let mut current_repository_url = None;
        let mut in_modules = false;
        let mut modules: Vec<String> = Vec::new();
        let mut in_mailing_lists = false;
        let mut in_mailing_list = false;
        let mut mailing_lists: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();
        let mut current_mailing_list_name = None;
        let mut current_mailing_list_subscribe = None;
        let mut current_mailing_list_unsubscribe = None;
        let mut current_mailing_list_post = None;
        let mut current_mailing_list_archive = None;
        let mut in_dependency_management = false;
        let mut dependency_management_entries: Vec<MavenDependencyData> = Vec::new();
        let mut current_dep_mgmt_dependency: Option<MavenDependencyData> = None;
        let mut in_dep_mgmt_dependency = false;
        let mut in_parent = false;
        let mut parent_group_id = None;
        let mut parent_artifact_id = None;
        let mut parent_version = None;
        let mut parent_relative_path = None;
        let mut in_properties = false;
        let mut properties: HashMap<String, String> = HashMap::new();
        let mut project_name = None;
        let mut project_description = None;
        let mut project_packaging = None;
        let mut project_classifier = None;
        let mut in_relocation = false;
        let mut relocation = MavenDependencyData::default();

        let mut iteration_count: usize = 0;
        loop {
            iteration_count += 1;
            if iteration_count > MAX_ITERATION_COUNT {
                warn!(
                    "Exceeded MAX_ITERATION_COUNT ({}) parsing pom.xml at {:?}; stopping early",
                    MAX_ITERATION_COUNT, path
                );
                break;
            }
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    let element_name = e.name().as_ref().to_vec();
                    current_element.push(element_name.clone());

                    match element_name.as_slice() {
                        b"parent" => in_parent = true,
                        b"dependencyManagement" => in_dependency_management = true,
                        b"dependencies" if in_dependency_management => {}
                        b"dependencies" => in_dependencies = true,
                        b"dependency" if in_dependency_management => {
                            in_dep_mgmt_dependency = true;
                            current_dep_mgmt_dependency = Some(MavenDependencyData::default());
                        }
                        b"dependency" if in_dependencies => {
                            current_dependency = Some(Dependency {
                                purl: None,
                                extracted_requirement: None,
                                scope: None,
                                is_runtime: None,
                                is_optional: Some(false),
                                is_pinned: None,
                                is_direct: Some(true),
                                resolved_package: None,
                                extra_data: None,
                            });
                            current_dependency_data = Some(MavenDependencyData::default());
                        }
                        b"properties" => in_properties = true,
                        b"developers" => in_developers = true,
                        b"developer" if in_developers => {
                            current_party = Some(Party {
                                r#type: Some("person".to_string()),
                                role: Some("developer".to_string()),
                                name: None,
                                email: None,
                                url: None,
                                organization: None,
                                organization_url: None,
                                timezone: None,
                            });
                        }
                        b"contributors" => in_contributors = true,
                        b"contributor" if in_contributors => {
                            current_party = Some(Party {
                                r#type: Some("person".to_string()),
                                role: Some("contributor".to_string()),
                                name: None,
                                email: None,
                                url: None,
                                organization: None,
                                organization_url: None,
                                timezone: None,
                            });
                        }
                        b"license" => current_license = Some(MavenLicenseEntry::default()),
                        b"distributionManagement" => in_distribution_management = true,
                        b"relocation" if in_distribution_management => {
                            in_relocation = true;
                            relocation = MavenDependencyData::default();
                        }
                        b"repository" if in_distribution_management => in_dist_repository = true,
                        b"snapshotRepository" if in_distribution_management => {
                            in_dist_snapshot_repository = true
                        }
                        b"site" if in_distribution_management => in_dist_site = true,
                        b"repositories" => in_repositories = true,
                        b"pluginRepositories" => in_plugin_repositories = true,
                        b"repository" if in_repositories && !in_distribution_management => {
                            in_repository = true;
                            current_repository_id = None;
                            current_repository_name = None;
                            current_repository_url = None;
                        }
                        b"pluginRepository" if in_plugin_repositories => {
                            in_repository = true;
                            current_repository_id = None;
                            current_repository_name = None;
                            current_repository_url = None;
                        }
                        b"modules" => in_modules = true,
                        b"mailingLists" => in_mailing_lists = true,
                        b"mailingList" if in_mailing_lists => {
                            in_mailing_list = true;
                            current_mailing_list_name = None;
                            current_mailing_list_subscribe = None;
                            current_mailing_list_unsubscribe = None;
                            current_mailing_list_post = None;
                            current_mailing_list_archive = None;
                        }
                        _ => {}
                    }
                }
                Ok(Event::Text(e)) => {
                    let text = match e.decode() {
                        Ok(Cow::Borrowed(s)) => s.to_string(),
                        Ok(Cow::Owned(s)) => s,
                        Err(_) => {
                            warn!(
                                "Invalid UTF-8 in XML text content in {:?}; using lossy conversion",
                                path
                            );
                            String::from_utf8_lossy(e.as_ref()).into_owned()
                        }
                    };
                    let current_path = current_element.last().map(|v| v.as_slice());
                    let current_parent = current_element
                        .len()
                        .checked_sub(2)
                        .map(|index| current_element[index].as_slice());

                    if in_properties
                        && current_element.len() >= 2
                        && current_element[current_element.len() - 2] == b"properties"
                    {
                        if let Some(property_name) = current_element
                            .last()
                            .and_then(|name| std::str::from_utf8(name).ok())
                        {
                            properties.insert(property_name.to_string(), truncate_field(text));
                        } else {
                            warn!("Failed to decode Maven property name in {:?}", path);
                        }
                    } else if in_dep_mgmt_dependency {
                        if let Some(dep_mgmt) = current_dep_mgmt_dependency.as_mut() {
                            match current_path {
                                Some(b"groupId") if current_parent == Some(b"dependency") => {
                                    dep_mgmt.group_id = Some(text)
                                }
                                Some(b"artifactId") if current_parent == Some(b"dependency") => {
                                    dep_mgmt.artifact_id = Some(text)
                                }
                                Some(b"version") if current_parent == Some(b"dependency") => {
                                    dep_mgmt.version = Some(text)
                                }
                                Some(b"scope") if current_parent == Some(b"dependency") => {
                                    dep_mgmt.scope = Some(text)
                                }
                                Some(b"type") if current_parent == Some(b"dependency") => {
                                    dep_mgmt.type_ = Some(text)
                                }
                                Some(b"classifier") if current_parent == Some(b"dependency") => {
                                    dep_mgmt.classifier = Some(text)
                                }
                                Some(b"optional") if current_parent == Some(b"dependency") => {
                                    dep_mgmt.optional = Some(text)
                                }
                                _ => {}
                            }
                        }
                    } else if let Some(license) = &mut current_license {
                        match current_path {
                            Some(b"name") => license.name = Some(text),
                            Some(b"url") => license.url = Some(text),
                            Some(b"comments") => license.comments = Some(text),
                            _ => {}
                        }
                    } else if let Some(party) = &mut current_party {
                        match current_path {
                            Some(b"name") => party.name = Some(text),
                            Some(b"email") => party.email = Some(text),
                            Some(b"url") => party.url = Some(text),
                            Some(b"organization") => party.organization = Some(text),
                            Some(b"organizationUrl") => party.organization_url = Some(text),
                            Some(b"timezone") => party.timezone = Some(text),
                            _ => {}
                        }
                    } else if let Some(dep) = &mut current_dependency {
                        match current_path {
                            Some(b"groupId") => {
                                if current_parent == Some(b"dependency")
                                    && let Some(coords) = current_dependency_data.as_mut()
                                {
                                    coords.group_id = Some(text);
                                }
                            }
                            Some(b"artifactId") => {
                                if current_parent == Some(b"dependency")
                                    && let Some(coords) = current_dependency_data.as_mut()
                                {
                                    coords.artifact_id = Some(text);
                                }
                            }
                            Some(b"version") => {
                                if current_parent == Some(b"dependency")
                                    && let Some(coords) = current_dependency_data.as_mut()
                                {
                                    coords.version = Some(text);
                                }
                            }
                            Some(b"scope") => {
                                if current_parent == Some(b"dependency") {
                                    dep.scope = Some(text.clone());
                                    dep.is_optional = Some(text == "test" || text == "provided");
                                    dep.is_runtime = Some(text != "test" && text != "provided");
                                }
                                if current_parent == Some(b"dependency")
                                    && let Some(coords) = current_dependency_data.as_mut()
                                {
                                    coords.scope = Some(text);
                                }
                            }
                            Some(b"optional") => {
                                if current_parent == Some(b"dependency")
                                    && let Some(coords) = current_dependency_data.as_mut()
                                {
                                    coords.optional = Some(text);
                                }
                            }
                            Some(b"type") => {
                                if current_parent == Some(b"dependency")
                                    && let Some(coords) = current_dependency_data.as_mut()
                                {
                                    coords.type_ = Some(text);
                                }
                            }
                            Some(b"classifier") => {
                                if current_parent == Some(b"dependency")
                                    && let Some(coords) = current_dependency_data.as_mut()
                                {
                                    coords.classifier = Some(text);
                                }
                            }
                            Some(b"systemPath") => {
                                if current_parent == Some(b"dependency")
                                    && let Some(coords) = current_dependency_data.as_mut()
                                {
                                    coords.system_path = Some(text);
                                }
                            }
                            _ => {}
                        }
                    } else if in_relocation {
                        match current_path {
                            Some(b"groupId") => relocation.group_id = Some(text),
                            Some(b"artifactId") => relocation.artifact_id = Some(text),
                            Some(b"version") => relocation.version = Some(text),
                            Some(b"classifier") => relocation.classifier = Some(text),
                            Some(b"type") => relocation.type_ = Some(text),
                            Some(b"message") => relocation.message = Some(text),
                            _ => {}
                        }
                    } else if in_parent {
                        match current_path {
                            Some(b"groupId") => {
                                parent_group_id = Some(text);
                            }
                            Some(b"artifactId") => {
                                parent_artifact_id = Some(text);
                            }
                            Some(b"version") => {
                                parent_version = Some(text);
                            }
                            Some(b"relativePath") => {
                                parent_relative_path = Some(text);
                            }
                            _ => {}
                        }
                    } else {
                        match current_path {
                            Some(b"groupId") if current_element.len() == 2 => {
                                package_data.namespace = Some(text)
                            }
                            Some(b"artifactId") if current_element.len() == 2 => {
                                package_data.name = Some(text)
                            }
                            Some(b"version") if current_element.len() == 2 => {
                                package_data.version = Some(text)
                            }
                            Some(b"name") if current_element.len() == 2 => {
                                project_name = Some(text)
                            }
                            Some(b"description") if current_element.len() == 2 => {
                                project_description = Some(text)
                            }
                            Some(b"packaging") if current_element.len() == 2 => {
                                project_packaging = Some(text)
                            }
                            Some(b"classifier") if current_element.len() == 2 => {
                                project_classifier = Some(text)
                            }
                            Some(b"url") if current_element.len() == 2 => {
                                package_data.homepage_url = Some(text)
                            }
                            Some(b"inceptionYear") if current_element.len() == 2 => {
                                inception_year = Some(text)
                            }
                            Some(b"connection")
                                if current_element.len() >= 3
                                    && current_element[current_element.len() - 2] == b"scm" =>
                            {
                                scm_connection = if text.starts_with("scm:git:") {
                                    Some(text.replacen("scm:git:", "git+", 1))
                                } else if text.starts_with("scm:") {
                                    Some(text.replacen("scm:", "", 1))
                                } else {
                                    Some(text)
                                };
                            }
                            Some(b"developerConnection")
                                if current_element.len() >= 3
                                    && current_element[current_element.len() - 2] == b"scm" =>
                            {
                                scm_developer_connection = if text.starts_with("scm:git:") {
                                    Some(text.replacen("scm:git:", "git+", 1))
                                } else if text.starts_with("scm:") {
                                    Some(text.replacen("scm:", "", 1))
                                } else {
                                    Some(text)
                                };
                            }
                            Some(b"url")
                                if current_element.len() >= 3
                                    && current_element[current_element.len() - 2] == b"scm" =>
                            {
                                scm_url = Some(text);
                            }
                            Some(b"tag")
                                if current_element.len() >= 3
                                    && current_element[current_element.len() - 2] == b"scm" =>
                            {
                                scm_tag = Some(text);
                            }
                            Some(b"name")
                                if current_element.len() >= 2
                                    && current_element[current_element.len() - 2]
                                        == b"organization" =>
                            {
                                organization_name = Some(text);
                            }
                            Some(b"url")
                                if current_element.len() >= 2
                                    && current_element[current_element.len() - 2]
                                        == b"organization" =>
                            {
                                organization_url = Some(text);
                            }
                            Some(b"system")
                                if current_element.len() >= 2
                                    && current_element[current_element.len() - 2]
                                        == b"issueManagement" =>
                            {
                                issue_management_system = Some(text);
                            }
                            Some(b"url")
                                if current_element.len() >= 2
                                    && current_element[current_element.len() - 2]
                                        == b"issueManagement" =>
                            {
                                issue_management_url = Some(text);
                            }
                            Some(b"system")
                                if current_element.len() >= 2
                                    && current_element[current_element.len() - 2]
                                        == b"ciManagement" =>
                            {
                                ci_management_system = Some(text);
                            }
                            Some(b"url")
                                if current_element.len() >= 2
                                    && current_element[current_element.len() - 2]
                                        == b"ciManagement" =>
                            {
                                ci_management_url = Some(text);
                            }
                            Some(b"downloadUrl")
                                if current_element.len() >= 2
                                    && current_element[current_element.len() - 2]
                                        == b"distributionManagement" =>
                            {
                                dist_download_url = Some(text);
                            }
                            Some(b"id") if in_dist_repository => {
                                dist_repository_id = Some(text);
                            }
                            Some(b"name") if in_dist_repository => {
                                dist_repository_name = Some(text);
                            }
                            Some(b"url") if in_dist_repository => {
                                dist_repository_url = Some(text);
                            }
                            Some(b"layout") if in_dist_repository => {
                                dist_repository_layout = Some(text);
                            }
                            Some(b"id") if in_dist_snapshot_repository => {
                                dist_snapshot_repository_id = Some(text);
                            }
                            Some(b"name") if in_dist_snapshot_repository => {
                                dist_snapshot_repository_name = Some(text);
                            }
                            Some(b"url") if in_dist_snapshot_repository => {
                                dist_snapshot_repository_url = Some(text);
                            }
                            Some(b"layout") if in_dist_snapshot_repository => {
                                dist_snapshot_repository_layout = Some(text);
                            }
                            Some(b"id") if in_dist_site => {
                                dist_site_id = Some(text);
                            }
                            Some(b"name") if in_dist_site => {
                                dist_site_name = Some(text);
                            }
                            Some(b"url") if in_dist_site => {
                                dist_site_url = Some(text);
                            }
                            Some(b"id") if in_repository => {
                                current_repository_id = Some(text);
                            }
                            Some(b"name") if in_repository => {
                                current_repository_name = Some(text);
                            }
                            Some(b"url") if in_repository => {
                                current_repository_url = Some(text);
                            }
                            Some(b"module") if in_modules => {
                                modules.push(text);
                            }
                            Some(b"name") if in_mailing_list => {
                                current_mailing_list_name = Some(text);
                            }
                            Some(b"subscribe") if in_mailing_list => {
                                current_mailing_list_subscribe = Some(text);
                            }
                            Some(b"unsubscribe") if in_mailing_list => {
                                current_mailing_list_unsubscribe = Some(text);
                            }
                            Some(b"post") if in_mailing_list => {
                                current_mailing_list_post = Some(text);
                            }
                            Some(b"archive") if in_mailing_list => {
                                current_mailing_list_archive = Some(text);
                            }
                            _ => {}
                        }
                    }
                }
                Ok(Event::Comment(e)) => {
                    let comment = match e.decode() {
                        Ok(Cow::Borrowed(s)) => s.trim().to_string(),
                        Ok(Cow::Owned(s)) => s.trim().to_string(),
                        Err(_) => {
                            warn!(
                                "Invalid UTF-8 in XML comment in {:?}; using lossy conversion",
                                path
                            );
                            String::from_utf8_lossy(e.as_ref())
                                .into_owned()
                                .trim()
                                .to_string()
                        }
                    };
                    if current_element.is_empty()
                        && !comment.is_empty()
                        && is_license_like_comment(&comment)
                    {
                        xml_license_comments.push(comment);
                    }
                }
                Ok(Event::End(e)) => {
                    if !current_element.is_empty() {
                        current_element.pop();
                    }

                    match e.name().as_ref() {
                        b"parent" => in_parent = false,
                        b"dependencyManagement" => in_dependency_management = false,
                        b"dependencies" => in_dependencies = false,
                        b"dependency" if in_dep_mgmt_dependency => {
                            in_dep_mgmt_dependency = false;
                            if let Some(dep_mgmt) = current_dep_mgmt_dependency.take()
                                && (dep_mgmt.group_id.is_some()
                                    || dep_mgmt.artifact_id.is_some()
                                    || dep_mgmt.version.is_some())
                            {
                                dependency_management_entries.push(dep_mgmt);
                            }
                        }
                        b"dependency" => {
                            if let (Some(dep), Some(coords)) =
                                (current_dependency.take(), current_dependency_data.take())
                            {
                                package_data.dependencies.push(dep);
                                dependency_data.push(coords);
                            } else if let Some(dep) = current_dependency.take() {
                                package_data.dependencies.push(dep);
                            }
                        }
                        b"license" => {
                            if let Some(license) = current_license.take()
                                && (license.name.is_some()
                                    || license.url.is_some()
                                    || license.comments.is_some())
                            {
                                licenses.push(license);
                            }
                        }
                        b"developers" => in_developers = false,
                        b"developer" => {
                            if let Some(party) = current_party.take() {
                                package_data.parties.push(party);
                            }
                        }
                        b"contributors" => in_contributors = false,
                        b"contributor" => {
                            if let Some(party) = current_party.take() {
                                package_data.parties.push(party);
                            }
                        }
                        b"distributionManagement" => in_distribution_management = false,
                        b"relocation" => in_relocation = false,
                        b"repository" if !in_dependencies && in_distribution_management => {
                            in_dist_repository = false
                        }
                        b"repository" if !in_dependencies && in_repositories => {
                            in_repository = false;
                            if current_repository_id.is_some()
                                || current_repository_name.is_some()
                                || current_repository_url.is_some()
                            {
                                let mut repo = serde_json::Map::new();
                                if let Some(id) = current_repository_id.take() {
                                    repo.insert("id".to_string(), serde_json::Value::String(id));
                                }
                                if let Some(name) = current_repository_name.take() {
                                    repo.insert(
                                        "name".to_string(),
                                        serde_json::Value::String(name),
                                    );
                                }
                                if let Some(url) = current_repository_url.take() {
                                    repo.insert("url".to_string(), serde_json::Value::String(url));
                                }
                                repositories.push(repo);
                            }
                        }
                        b"pluginRepository" if in_plugin_repositories => {
                            in_repository = false;
                            if current_repository_id.is_some()
                                || current_repository_name.is_some()
                                || current_repository_url.is_some()
                            {
                                let mut repo = serde_json::Map::new();
                                if let Some(id) = current_repository_id.take() {
                                    repo.insert("id".to_string(), serde_json::Value::String(id));
                                }
                                if let Some(name) = current_repository_name.take() {
                                    repo.insert(
                                        "name".to_string(),
                                        serde_json::Value::String(name),
                                    );
                                }
                                if let Some(url) = current_repository_url.take() {
                                    repo.insert("url".to_string(), serde_json::Value::String(url));
                                }
                                plugin_repositories.push(repo);
                            }
                        }
                        b"repositories" => in_repositories = false,
                        b"properties" => in_properties = false,
                        b"pluginRepositories" => in_plugin_repositories = false,
                        b"modules" => in_modules = false,
                        b"mailingLists" => in_mailing_lists = false,
                        b"mailingList" => {
                            in_mailing_list = false;
                            if current_mailing_list_name.is_some()
                                || current_mailing_list_subscribe.is_some()
                                || current_mailing_list_unsubscribe.is_some()
                                || current_mailing_list_post.is_some()
                                || current_mailing_list_archive.is_some()
                            {
                                let mut ml = serde_json::Map::new();
                                if let Some(name) = current_mailing_list_name.take() {
                                    ml.insert("name".to_string(), serde_json::Value::String(name));
                                }
                                if let Some(subscribe) = current_mailing_list_subscribe.take() {
                                    ml.insert(
                                        "subscribe".to_string(),
                                        serde_json::Value::String(subscribe),
                                    );
                                }
                                if let Some(unsubscribe) = current_mailing_list_unsubscribe.take() {
                                    ml.insert(
                                        "unsubscribe".to_string(),
                                        serde_json::Value::String(unsubscribe),
                                    );
                                }
                                if let Some(post) = current_mailing_list_post.take() {
                                    ml.insert("post".to_string(), serde_json::Value::String(post));
                                }
                                if let Some(archive) = current_mailing_list_archive.take() {
                                    ml.insert(
                                        "archive".to_string(),
                                        serde_json::Value::String(archive),
                                    );
                                }
                                mailing_lists.push(ml);
                            }
                        }
                        b"snapshotRepository" => in_dist_snapshot_repository = false,
                        b"site" => in_dist_site = false,
                        _ => {}
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    warn!("Error parsing pom.xml at {:?}: {}", path, e);
                    return vec![package_data];
                }
                _ => {}
            }
            buf.clear();
        }

        let builtins = build_builtin_properties(MavenBuiltinPropertyInputs {
            namespace: &package_data.namespace,
            name: &package_data.name,
            version: &package_data.version,
            parent_group_id: &parent_group_id,
            parent_artifact_id: &parent_artifact_id,
            parent_version: &parent_version,
            project_name: &project_name,
            project_packaging: &project_packaging,
        });
        let mut resolver = PropertyResolver::new(properties, builtins);

        resolve_option(&mut resolver, &mut package_data.namespace);
        resolve_option(&mut resolver, &mut package_data.name);
        resolve_option(&mut resolver, &mut package_data.version);
        resolve_option(&mut resolver, &mut package_data.homepage_url);
        resolve_option(&mut resolver, &mut inception_year);
        resolve_option(&mut resolver, &mut scm_connection);
        resolve_option(&mut resolver, &mut scm_developer_connection);
        resolve_option(&mut resolver, &mut scm_url);
        resolve_option(&mut resolver, &mut scm_tag);
        resolve_option(&mut resolver, &mut organization_name);
        resolve_option(&mut resolver, &mut organization_url);
        resolve_option(&mut resolver, &mut issue_management_system);
        resolve_option(&mut resolver, &mut issue_management_url);
        resolve_option(&mut resolver, &mut ci_management_system);
        resolve_option(&mut resolver, &mut ci_management_url);
        resolve_option(&mut resolver, &mut dist_download_url);
        resolve_option(&mut resolver, &mut dist_repository_id);
        resolve_option(&mut resolver, &mut dist_repository_name);
        resolve_option(&mut resolver, &mut dist_repository_url);
        resolve_option(&mut resolver, &mut dist_repository_layout);
        resolve_option(&mut resolver, &mut dist_snapshot_repository_id);
        resolve_option(&mut resolver, &mut dist_snapshot_repository_name);
        resolve_option(&mut resolver, &mut dist_snapshot_repository_url);
        resolve_option(&mut resolver, &mut dist_snapshot_repository_layout);
        resolve_option(&mut resolver, &mut dist_site_id);
        resolve_option(&mut resolver, &mut dist_site_name);
        resolve_option(&mut resolver, &mut dist_site_url);
        resolve_option(&mut resolver, &mut parent_group_id);
        resolve_option(&mut resolver, &mut parent_artifact_id);
        resolve_option(&mut resolver, &mut parent_version);
        resolve_option(&mut resolver, &mut parent_relative_path);
        resolve_option(&mut resolver, &mut project_name);
        resolve_option(&mut resolver, &mut project_description);
        resolve_option(&mut resolver, &mut project_packaging);
        resolve_option(&mut resolver, &mut project_classifier);
        resolve_vec(&mut resolver, &mut modules);
        resolve_maps(&mut resolver, &mut repositories);
        resolve_maps(&mut resolver, &mut plugin_repositories);
        resolve_maps(&mut resolver, &mut mailing_lists);
        for comment in &mut xml_license_comments {
            *comment = resolver.resolve_text(comment, 0);
        }
        for dependency in &mut dependency_management_entries {
            resolve_dependency_data(&mut resolver, dependency);
        }
        resolve_dependency_data(&mut resolver, &mut relocation);
        for license in &mut licenses {
            resolve_license_entry(&mut resolver, license);
        }
        for comment in xml_license_comments {
            if !comment.trim().is_empty() {
                licenses.push(MavenLicenseEntry {
                    comments: Some(comment),
                    ..Default::default()
                });
            }
        }

        for (dependency, coords) in package_data
            .dependencies
            .iter_mut()
            .zip(dependency_data.iter_mut())
        {
            resolve_dependency_data(&mut resolver, coords);
            dependency.scope = coords.scope.clone();
            dependency.extracted_requirement = coords.version.clone();
            dependency.extra_data = dependency_extra_data(coords);
            dependency.is_optional = Some(parse_maven_bool(coords.optional.as_deref()));

            match dependency.scope.as_deref() {
                Some("test") | Some("provided") => {
                    dependency.is_runtime = Some(false);
                    dependency.is_optional = Some(true);
                }
                Some(_) => {
                    dependency.is_runtime = Some(true);
                }
                None => {
                    dependency.is_runtime = None;
                }
            }

            if let Some(version) = &coords.version {
                dependency.is_pinned = Some(is_maven_version_pinned(version));
            }

            if let (Some(group_id), Some(artifact_id)) = (&coords.group_id, &coords.artifact_id) {
                dependency.purl = Some(build_maven_purl(
                    group_id,
                    artifact_id,
                    coords.version.as_deref(),
                    coords.classifier.as_deref(),
                    coords.type_.as_deref(),
                ));
            }
        }

        if package_data.namespace.is_none() {
            package_data.namespace = parent_group_id.clone();
        }
        if package_data.version.is_none() {
            package_data.version = parent_version.clone();
        }

        package_data.qualifiers =
            build_maven_qualifiers(project_classifier.as_deref(), project_packaging.as_deref());

        package_data.description = match (
            project_name.as_deref().filter(|value| !value.is_empty()),
            project_description
                .as_deref()
                .filter(|value| !value.is_empty()),
        ) {
            (Some(name), Some(description)) if name == description => Some(name.to_string()),
            (Some(name), Some(description)) => Some(format!("{name}\n{description}")),
            (Some(name), None) => Some(name.to_string()),
            (None, Some(description)) => Some(description.to_string()),
            (None, None) => None,
        };

        if path.to_string_lossy().contains("META-INF/maven/") {
            let path_str = path.to_string_lossy();
            if let Some(meta_inf_pos) = path_str.find("META-INF/maven/") {
                let after_maven = &path_str[meta_inf_pos + "META-INF/maven/".len()..];
                let parts: Vec<&str> = after_maven.split('/').collect();
                if parts.len() >= 2 {
                    if package_data.namespace.is_none() {
                        package_data.namespace = Some(parts[0].to_string());
                    }
                    if package_data.name.is_none() {
                        package_data.name = Some(parts[1].to_string());
                    }
                }
            }
        }

        // Construct PURL from parsed data
        if let (Some(group_id), Some(artifact_id), Some(version)) = (
            &package_data.namespace,
            &package_data.name,
            &package_data.version,
        ) {
            package_data.purl = Some(build_maven_purl(
                group_id,
                artifact_id,
                Some(version),
                project_classifier.as_deref(),
                project_packaging.as_deref(),
            ));
            if project_classifier.is_none() {
                package_data
                    .source_packages
                    .push(build_maven_source_package(group_id, artifact_id, version));
            }
        }

        if let (Some(group_id), Some(artifact_id)) = (&package_data.namespace, &package_data.name) {
            package_data.repository_homepage_url = build_maven_url(
                &package_data.namespace,
                &package_data.name,
                &package_data.version,
                None,
            );

            package_data.repository_download_url = package_data.version.as_ref().map(|ver| {
                build_maven_download_url(
                    group_id,
                    artifact_id,
                    ver,
                    project_classifier.as_deref(),
                    project_packaging.as_deref(),
                )
            });

            if let Some(ver) = &package_data.version {
                let pom_filename = format!("{}-{}.pom", artifact_id, ver);
                package_data.api_data_url = build_maven_url(
                    &package_data.namespace,
                    &package_data.name,
                    &package_data.version,
                    Some(&pom_filename),
                );
            }
        }

        package_data.vcs_url = scm_connection
            .or_else(|| scm_developer_connection.clone())
            .or_else(|| scm_url.clone());

        // Set code_view_url from scm/url (human-browseable URL)
        if let Some(url) = &scm_url {
            package_data.code_view_url = Some(url.clone());
        }

        // Set bug_tracking_url from issueManagement/url
        if let Some(url) = &issue_management_url {
            package_data.bug_tracking_url = Some(url.clone());
        }

        // Map downloadUrl to download_url field
        if let Some(url) = &dist_download_url {
            package_data.download_url = Some(url.clone());
        }

        if organization_name.is_some() || organization_url.is_some() {
            package_data.parties.push(Party {
                r#type: Some("organization".to_string()),
                role: Some("owner".to_string()),
                name: organization_name.clone(),
                email: None,
                url: organization_url.clone(),
                organization: None,
                organization_url: None,
                timezone: None,
            });
        }

        for dependency in &dependency_management_entries {
            if dependency.scope.as_deref() == Some("import")
                && let Some(import_dependency) =
                    maven_dependency_to_dependency(dependency, Some("import"), true)
            {
                package_data.dependencies.push(import_dependency);
            }

            // ScanCode surfaces import-scoped BOM entries twice: once as an
            // import edge and again as a dependencyManagement record. Preserve
            // that shape so compare-output parity does not depend on target-
            // specific workarounds.
            let mut dependency_management_copy = dependency.clone();
            dependency_management_copy.scope = Some("dependencymanagement".to_string());

            if let Some(converted) = maven_dependency_to_dependency(
                &dependency_management_copy,
                Some("dependencymanagement"),
                true,
            ) {
                package_data.dependencies.push(converted);
            }
        }

        if (relocation.group_id.is_some()
            || relocation.artifact_id.is_some()
            || relocation.version.is_some())
            && let Some(converted) =
                maven_dependency_to_dependency(&relocation, Some("relocation"), true)
        {
            package_data.dependencies.push(converted);
        }

        if inception_year.is_some()
            || organization_name.is_some()
            || organization_url.is_some()
            || scm_tag.is_some()
            || scm_developer_connection.is_some()
            || issue_management_system.is_some()
            || ci_management_system.is_some()
            || ci_management_url.is_some()
            || dist_download_url.is_some()
            || dist_repository_id.is_some()
            || dist_snapshot_repository_id.is_some()
            || dist_site_id.is_some()
            || !repositories.is_empty()
            || !plugin_repositories.is_empty()
            || !modules.is_empty()
            || !mailing_lists.is_empty()
            || !dependency_management_entries.is_empty()
            || parent_group_id.is_some()
            || relocation.group_id.is_some()
            || relocation.artifact_id.is_some()
            || relocation.version.is_some()
            || relocation.message.is_some()
        {
            let mut extra_data = package_data.extra_data.take().unwrap_or_default();
            if let Some(year) = inception_year {
                extra_data.insert(
                    "inception_year".to_string(),
                    serde_json::Value::String(year),
                );
            }
            if let Some(name) = organization_name {
                extra_data.insert(
                    "organization_name".to_string(),
                    serde_json::Value::String(name),
                );
            }
            if let Some(url) = organization_url {
                extra_data.insert(
                    "organization_url".to_string(),
                    serde_json::Value::String(url),
                );
            }
            if let Some(tag) = scm_tag {
                extra_data.insert("scm_tag".to_string(), serde_json::Value::String(tag));
            }
            if let Some(dev_conn) = scm_developer_connection {
                extra_data.insert(
                    "scm_developer_connection".to_string(),
                    serde_json::Value::String(dev_conn),
                );
            }
            if let Some(system) = issue_management_system {
                extra_data.insert(
                    "issue_tracking_system".to_string(),
                    serde_json::Value::String(system),
                );
            }
            if let Some(system) = ci_management_system {
                extra_data.insert("ci_system".to_string(), serde_json::Value::String(system));
            }
            if let Some(url) = ci_management_url {
                extra_data.insert("ci_url".to_string(), serde_json::Value::String(url));
            }

            // Add distribution management data
            if let Some(url) = dist_download_url {
                extra_data.insert(
                    "distribution_download_url".to_string(),
                    serde_json::Value::String(url),
                );
            }

            // Build repository object
            if dist_repository_id.is_some()
                || dist_repository_name.is_some()
                || dist_repository_url.is_some()
                || dist_repository_layout.is_some()
            {
                let mut repo = serde_json::Map::new();
                if let Some(id) = dist_repository_id {
                    repo.insert("id".to_string(), serde_json::Value::String(id));
                }
                if let Some(name) = dist_repository_name {
                    repo.insert("name".to_string(), serde_json::Value::String(name));
                }
                if let Some(url) = dist_repository_url {
                    repo.insert("url".to_string(), serde_json::Value::String(url));
                }
                if let Some(layout) = dist_repository_layout {
                    repo.insert("layout".to_string(), serde_json::Value::String(layout));
                }
                extra_data.insert(
                    "distribution_repository".to_string(),
                    serde_json::Value::Object(repo),
                );
            }

            // Build snapshotRepository object
            if dist_snapshot_repository_id.is_some()
                || dist_snapshot_repository_name.is_some()
                || dist_snapshot_repository_url.is_some()
                || dist_snapshot_repository_layout.is_some()
            {
                let mut repo = serde_json::Map::new();
                if let Some(id) = dist_snapshot_repository_id {
                    repo.insert("id".to_string(), serde_json::Value::String(id));
                }
                if let Some(name) = dist_snapshot_repository_name {
                    repo.insert("name".to_string(), serde_json::Value::String(name));
                }
                if let Some(url) = dist_snapshot_repository_url {
                    repo.insert("url".to_string(), serde_json::Value::String(url));
                }
                if let Some(layout) = dist_snapshot_repository_layout {
                    repo.insert("layout".to_string(), serde_json::Value::String(layout));
                }
                extra_data.insert(
                    "distribution_snapshot_repository".to_string(),
                    serde_json::Value::Object(repo),
                );
            }

            // Build site object
            if dist_site_id.is_some() || dist_site_name.is_some() || dist_site_url.is_some() {
                let mut site = serde_json::Map::new();
                if let Some(id) = dist_site_id {
                    site.insert("id".to_string(), serde_json::Value::String(id));
                }
                if let Some(name) = dist_site_name {
                    site.insert("name".to_string(), serde_json::Value::String(name));
                }
                if let Some(url) = dist_site_url {
                    site.insert("url".to_string(), serde_json::Value::String(url));
                }
                extra_data.insert(
                    "distribution_site".to_string(),
                    serde_json::Value::Object(site),
                );
            }

            if !repositories.is_empty() {
                extra_data.insert(
                    "repositories".to_string(),
                    serde_json::Value::Array(
                        repositories
                            .into_iter()
                            .map(serde_json::Value::Object)
                            .collect(),
                    ),
                );
            }

            if !plugin_repositories.is_empty() {
                extra_data.insert(
                    "plugin_repositories".to_string(),
                    serde_json::Value::Array(
                        plugin_repositories
                            .into_iter()
                            .map(serde_json::Value::Object)
                            .collect(),
                    ),
                );
            }

            if !modules.is_empty() {
                extra_data.insert(
                    "modules".to_string(),
                    serde_json::Value::Array(
                        modules.into_iter().map(serde_json::Value::String).collect(),
                    ),
                );
            }

            if !mailing_lists.is_empty() {
                extra_data.insert(
                    "mailing_lists".to_string(),
                    serde_json::Value::Array(
                        mailing_lists
                            .into_iter()
                            .map(serde_json::Value::Object)
                            .collect(),
                    ),
                );
            }

            if !dependency_management_entries.is_empty() {
                extra_data.insert(
                    "dependency_management".to_string(),
                    serde_json::Value::Array(
                        dependency_management_entries
                            .into_iter()
                            .map(|dependency| {
                                serde_json::Value::Object(dependency_management_entry_to_value(
                                    &dependency,
                                ))
                            })
                            .collect(),
                    ),
                );
            }

            if relocation.group_id.is_some()
                || relocation.artifact_id.is_some()
                || relocation.version.is_some()
                || relocation.message.is_some()
            {
                extra_data.insert(
                    "relocation".to_string(),
                    serde_json::Value::Object(dependency_management_entry_to_value(&relocation)),
                );
            }

            if parent_group_id.is_some()
                || parent_artifact_id.is_some()
                || parent_version.is_some()
                || parent_relative_path.is_some()
            {
                let mut parent_obj = serde_json::Map::new();
                if let Some(group_id) = parent_group_id {
                    parent_obj.insert("groupId".to_string(), serde_json::Value::String(group_id));
                }
                if let Some(artifact_id) = parent_artifact_id {
                    parent_obj.insert(
                        "artifactId".to_string(),
                        serde_json::Value::String(artifact_id),
                    );
                }
                if let Some(version) = parent_version {
                    parent_obj.insert("version".to_string(), serde_json::Value::String(version));
                }
                if let Some(relative_path) = parent_relative_path {
                    parent_obj.insert(
                        "relativePath".to_string(),
                        serde_json::Value::String(relative_path),
                    );
                }
                extra_data.insert("parent".to_string(), serde_json::Value::Object(parent_obj));
            }

            package_data.extra_data = Some(extra_data);
        }

        package_data.extracted_license_statement =
            build_license_statement(&licenses).map(truncate_field);
        let (declared_license_expression, declared_license_expression_spdx, license_detections) =
            build_maven_declared_license_data(
                &licenses,
                package_data.extracted_license_statement.as_deref(),
            );
        package_data.declared_license_expression = declared_license_expression;
        package_data.declared_license_expression_spdx = declared_license_expression_spdx;
        package_data.license_detections = license_detections;

        package_data.namespace = package_data.namespace.map(truncate_field);
        package_data.name = package_data.name.map(truncate_field);
        package_data.version = package_data.version.map(truncate_field);
        package_data.description = package_data.description.map(truncate_field);
        package_data.homepage_url = package_data.homepage_url.map(truncate_field);
        package_data.vcs_url = package_data.vcs_url.map(truncate_field);
        package_data.purl = package_data.purl.map(truncate_field);
        package_data.code_view_url = package_data.code_view_url.map(truncate_field);
        package_data.bug_tracking_url = package_data.bug_tracking_url.map(truncate_field);
        package_data.download_url = package_data.download_url.map(truncate_field);
        package_data.repository_homepage_url =
            package_data.repository_homepage_url.map(truncate_field);
        package_data.repository_download_url =
            package_data.repository_download_url.map(truncate_field);
        package_data.api_data_url = package_data.api_data_url.map(truncate_field);
        for dep in &mut package_data.dependencies {
            dep.purl = dep.purl.take().map(truncate_field);
            dep.extracted_requirement = dep.extracted_requirement.take().map(truncate_field);
        }

        vec![package_data]
    }

    fn is_match(path: &Path) -> bool {
        if let Some(filename) = path.file_name().and_then(|name| name.to_str()) {
            filename == "pom.xml"
                || filename.ends_with(".pom.xml")
                || filename.ends_with("-pom.xml")
                || filename == "pom.properties"
                || filename == "MANIFEST.MF"
                || filename.ends_with(".pom")
        } else {
            false
        }
    }
}

fn build_maven_url(
    group_id: &Option<String>,
    artifact_id: &Option<String>,
    version: &Option<String>,
    filename: Option<&str>,
) -> Option<String> {
    const BASE_URL: &str = "https://repo1.maven.org/maven2";

    let group_id = group_id.as_ref()?;
    let artifact_id = artifact_id.as_ref()?;

    let group_path = group_id.replace('.', "/");
    let filename_str = filename.unwrap_or("");

    let url = if let Some(ver) = version {
        format!(
            "{}/{}/{}/{}/{}",
            BASE_URL, group_path, artifact_id, ver, filename_str
        )
    } else {
        format!(
            "{}/{}/{}/{}",
            BASE_URL, group_path, artifact_id, filename_str
        )
    };

    Some(url)
}

fn build_maven_declared_license_data(
    licenses: &[MavenLicenseEntry],
    matched_text: Option<&str>,
) -> (
    Option<String>,
    Option<String>,
    Vec<crate::models::LicenseDetection>,
) {
    let normalized: Vec<_> = licenses
        .iter()
        .filter_map(|license| license.name.as_deref())
        .filter_map(normalize_maven_license_name)
        .collect();

    if normalized.is_empty() {
        return empty_declared_license_data();
    }

    let Some(combined) = combine_normalized_licenses(normalized, " OR ") else {
        return empty_declared_license_data();
    };

    build_declared_license_data(
        combined,
        DeclaredLicenseMatchMetadata::single_line(matched_text.unwrap_or_default()),
    )
}

fn normalize_maven_license_name(name: &str) -> Option<NormalizedDeclaredLicense> {
    match name.trim() {
        "Public Domain" | "public domain" => Some(NormalizedDeclaredLicense::new(
            "public-domain",
            "LicenseRef-provenant-public-domain",
        )),
        other => normalize_declared_license_key(other),
    }
}

/// Parse pom.properties file (Java properties format)
fn parse_pom_properties(path: &Path) -> PackageData {
    let content = match read_file_to_string(path, None).map_err(|e| e.to_string()) {
        Ok(content) => content,
        Err(e) => {
            warn!("Failed to read pom.properties at {:?}: {}", path, e);
            return PackageData {
                package_type: Some(PackageType::Maven),
                primary_language: Some("Java".to_string()),
                datasource_id: Some(DatasourceId::MavenPomProperties),
                ..Default::default()
            };
        }
    };

    let mut package_data = default_package_data(DatasourceId::MavenPomProperties);
    package_data.package_type = Some(PackageType::Maven);
    package_data.primary_language = Some("Java".to_string());
    package_data.datasource_id = Some(DatasourceId::MavenPomProperties);

    let mut group_id: Option<String> = None;
    let mut artifact_id: Option<String> = None;
    let mut version: Option<String> = None;

    // Parse Java properties format
    let mut continuation = String::new();

    for line in content.lines() {
        let current_line = if continuation.is_empty() {
            line.to_string()
        } else {
            format!("{}{}", continuation, line)
        };
        continuation.clear();

        // Check for line continuation (backslash at end)
        if current_line.ends_with('\\') {
            continuation = current_line[..current_line.len() - 1].to_string();
            continue;
        }

        // Skip comments and empty lines
        let trimmed = current_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
            continue;
        }

        // Parse key=value
        if let Some(eq_pos) = current_line.find('=') {
            let key = current_line[..eq_pos].trim();
            let value = current_line[eq_pos + 1..].trim();

            match key {
                "groupId" => group_id = Some(value.to_string()),
                "artifactId" => artifact_id = Some(value.to_string()),
                "version" => version = Some(value.to_string()),
                _ => {}
            }
        }
    }

    package_data.namespace = group_id.map(truncate_field);
    package_data.name = artifact_id.map(truncate_field);
    package_data.version = version.map(truncate_field);

    // Generate PURL
    if let (Some(group_id), Some(artifact_id), Some(version)) = (
        &package_data.namespace,
        &package_data.name,
        &package_data.version,
    ) {
        package_data.purl = Some(truncate_field(format!(
            "pkg:maven/{}/{}@{}",
            group_id, artifact_id, version
        )));
    }

    package_data
}

/// Parse MANIFEST.MF file (JAR manifest format)
///
/// Detects and handles both regular JAR manifests and OSGi bundle manifests.
/// If Bundle-SymbolicName is present, treats the manifest as an OSGi bundle
/// and extracts OSGi-specific metadata including Import-Package and Require-Bundle
/// dependencies.
fn parse_manifest_mf(path: &Path) -> PackageData {
    let content = match read_file_to_string(path, None).map_err(|e| e.to_string()) {
        Ok(content) => content,
        Err(e) => {
            warn!("Failed to read MANIFEST.MF at {:?}: {}", path, e);
            return default_package_data(DatasourceId::JavaJarManifest);
        }
    };

    let mut package_data = default_package_data(DatasourceId::JavaJarManifest);

    // Parse manifest headers (RFC822-style with space continuations)
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut current_key: Option<String> = None;
    let mut current_value = String::new();

    for line in content.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            // Continuation line
            current_value.push_str(line.trim());
        } else if let Some(colon_pos) = line.find(':') {
            // Save previous header
            if let Some(key) = current_key.take() {
                headers.push((key, current_value.trim().to_string()));
                current_value.clear();
            }

            // Start new header
            let key = line[..colon_pos].trim().to_string();
            let value = line[colon_pos + 1..].trim().to_string();
            current_key = Some(key);
            current_value = value;
        }
    }

    // Save last header
    if let Some(key) = current_key {
        headers.push((key, current_value.trim().to_string()));
    }

    // Convert headers to HashMap for easier lookup
    let headers_map: HashMap<String, String> = headers.iter().cloned().collect();

    // Check if this is an OSGi bundle by looking for Bundle-SymbolicName
    let bundle_symbolic_name = headers_map.get("Bundle-SymbolicName");
    let is_osgi = bundle_symbolic_name.is_some();

    if is_osgi {
        // OSGi bundle - extract OSGi-specific metadata
        package_data.package_type = Some(PackageType::Osgi);
        package_data.datasource_id = Some(DatasourceId::JavaOsgiManifest);

        // Bundle-SymbolicName is the canonical name for OSGi bundles
        // Strip directives after semicolon: "org.example.bundle;singleton:=true" -> "org.example.bundle"
        if let Some(bsn) = bundle_symbolic_name {
            let name = if let Some(semicolon_pos) = bsn.find(';') {
                bsn[..semicolon_pos].trim().to_string()
            } else {
                bsn.clone()
            };
            package_data.name = Some(name);
        }

        // Bundle-Version
        package_data.version = headers_map.get("Bundle-Version").cloned();

        // Bundle-Description takes priority over Bundle-Name for description
        if let Some(desc) = headers_map.get("Bundle-Description") {
            package_data.description = Some(desc.clone());
        } else if let Some(name) = headers_map.get("Bundle-Name") {
            package_data.description = Some(name.clone());
        }

        // Bundle-Vendor
        if let Some(vendor) = headers_map.get("Bundle-Vendor") {
            package_data.parties.push(Party {
                r#type: Some("organization".to_string()),
                role: Some("vendor".to_string()),
                name: Some(vendor.clone()),
                email: None,
                url: None,
                organization: None,
                organization_url: None,
                timezone: None,
            });
        }

        // Bundle-DocURL
        package_data.homepage_url = headers_map.get("Bundle-DocURL").cloned();

        // Bundle-License
        package_data.extracted_license_statement = headers_map.get("Bundle-License").cloned();

        // Import-Package -> dependencies with scope "import"
        if let Some(import_pkg) = headers_map.get("Import-Package") {
            let deps = parse_osgi_package_list(import_pkg, "import");
            package_data.dependencies.extend(deps);
        }

        // Require-Bundle -> dependencies with scope "require-bundle"
        if let Some(require_bundle) = headers_map.get("Require-Bundle") {
            let deps = parse_osgi_bundle_list(require_bundle, "require-bundle");
            package_data.dependencies.extend(deps);
        }

        // Export-Package -> store in extra_data
        if let Some(export_pkg) = headers_map.get("Export-Package") {
            let mut extra_data = package_data.extra_data.take().unwrap_or_default();
            extra_data.insert(
                "export_packages".to_string(),
                serde_json::Value::String(export_pkg.clone()),
            );
            package_data.extra_data = Some(extra_data);
        }

        // Build OSGi PURL: pkg:osgi/{bundle_symbolic_name}@{bundle_version}
        if let (Some(name), Some(version)) = (&package_data.name, &package_data.version) {
            package_data.purl = Some(format!("pkg:osgi/{}@{}", name, version));
        }
    } else {
        // Regular JAR manifest
        package_data.package_type = Some(PackageType::Maven);
        package_data.datasource_id = Some(DatasourceId::JavaJarManifest);

        // Extract fields with priority order for non-OSGi JARs
        let mut name: Option<String> = None;
        let mut version: Option<String> = None;
        let mut vendor: Option<String> = None;

        for (key, value) in &headers {
            match key.as_str() {
                "Bundle-Name" if name.is_none() => {
                    name = Some(value.clone());
                }
                "Implementation-Title" if name.is_none() => {
                    name = Some(value.clone());
                }
                "Bundle-Version" if version.is_none() => {
                    version = Some(value.clone());
                }
                "Implementation-Version" if version.is_none() => {
                    version = Some(value.clone());
                }
                "Implementation-Vendor" | "Bundle-Vendor" if vendor.is_none() => {
                    vendor = Some(value.clone());
                }
                _ => {}
            }
        }

        package_data.name = name;
        package_data.version = version;

        // Add vendor to parties if present
        if let Some(vendor_name) = vendor {
            package_data.parties.push(Party {
                r#type: Some("organization".to_string()),
                role: Some("vendor".to_string()),
                name: Some(vendor_name),
                email: None,
                url: None,
                organization: None,
                organization_url: None,
                timezone: None,
            });
        }

        // Try to extract groupId from path (META-INF/maven/{groupId}/{artifactId}/)
        if let Some(path_str) = path.to_str()
            && let Some(meta_inf_pos) = path_str.find("META-INF/maven/")
        {
            let after_maven = &path_str[meta_inf_pos + "META-INF/maven/".len()..];
            let parts: Vec<&str> = after_maven.split('/').collect();
            if parts.len() >= 2 {
                package_data.namespace = Some(parts[0].to_string());
            }
        }

        // Generate Maven PURL if we have enough information
        if let (Some(group_id), Some(artifact_id), Some(version)) = (
            &package_data.namespace,
            &package_data.name,
            &package_data.version,
        ) {
            package_data.purl = Some(format!(
                "pkg:maven/{}/{}@{}",
                group_id, artifact_id, version
            ));
        } else if package_data.name.is_none() && package_data.version.is_none() {
            // A bare MANIFEST.MF without Maven coordinates or implementation
            // identity is only evidence of a generic JAR manifest, not a Maven
            // package. Keep the Java manifest datasource so assembly can still
            // merge richer sibling metadata when present.
            package_data.package_type = Some(PackageType::Jar);
        }
    }

    package_data.name = package_data.name.map(truncate_field);
    package_data.version = package_data.version.map(truncate_field);
    package_data.namespace = package_data.namespace.map(truncate_field);
    package_data.description = package_data.description.map(truncate_field);
    package_data.homepage_url = package_data.homepage_url.map(truncate_field);
    package_data.extracted_license_statement =
        package_data.extracted_license_statement.map(truncate_field);
    package_data.purl = package_data.purl.map(truncate_field);
    for dep in &mut package_data.dependencies {
        dep.purl = dep.purl.take().map(truncate_field);
        dep.extracted_requirement = dep.extracted_requirement.take().map(truncate_field);
    }

    package_data
}

/// Parse OSGi Import-Package header into dependencies.
///
/// Format: comma-separated list of packages with optional directives:
/// "org.osgi.framework;version=\"[1.6,2)\",javax.servlet;version=\"[3.0,4)\""
pub(crate) fn parse_osgi_package_list(package_list: &str, scope: &str) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    // Split by comma, but be careful not to split within quoted strings
    for package_entry in split_osgi_list(package_list)
        .into_iter()
        .take(MAX_ITERATION_COUNT)
    {
        let package_entry = package_entry.trim();
        if package_entry.is_empty() {
            continue;
        }

        // Extract package name (before first semicolon)
        let package_name = if let Some(semicolon_pos) = package_entry.find(';') {
            package_entry[..semicolon_pos].trim()
        } else {
            package_entry
        };

        if package_name.is_empty() {
            continue;
        }

        // Extract version directive if present
        let version_requirement = extract_osgi_version(package_entry);
        let is_optional = package_entry.contains("resolution:=optional");

        dependencies.push(Dependency {
            purl: Some(format!("pkg:osgi/{}", package_name)),
            extracted_requirement: version_requirement,
            scope: Some(scope.to_string()),
            is_runtime: Some(true),
            is_optional: Some(is_optional),
            is_pinned: None,
            is_direct: Some(true),
            resolved_package: None,
            extra_data: None,
        });
    }

    dependencies
}

/// Parse OSGi Require-Bundle header into dependencies.
///
/// Format: comma-separated list of bundle symbolic names with optional directives:
/// "org.eclipse.core.runtime;bundle-version=\"3.7.0\",org.eclipse.ui;resolution:=optional"
pub(crate) fn parse_osgi_bundle_list(bundle_list: &str, scope: &str) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    for bundle_entry in split_osgi_list(bundle_list)
        .into_iter()
        .take(MAX_ITERATION_COUNT)
    {
        let bundle_entry = bundle_entry.trim();
        if bundle_entry.is_empty() {
            continue;
        }

        // Extract bundle symbolic name (before first semicolon)
        let bundle_name = if let Some(semicolon_pos) = bundle_entry.find(';') {
            bundle_entry[..semicolon_pos].trim()
        } else {
            bundle_entry
        };

        if bundle_name.is_empty() {
            continue;
        }

        // Extract bundle-version directive if present
        let version_requirement = extract_osgi_bundle_version(bundle_entry);

        // Check if optional
        let is_optional = bundle_entry.contains("resolution:=optional");

        dependencies.push(Dependency {
            purl: Some(format!("pkg:osgi/{}", bundle_name)),
            extracted_requirement: version_requirement,
            scope: Some(scope.to_string()),
            is_runtime: Some(!is_optional),
            is_optional: Some(is_optional),
            is_pinned: None,
            is_direct: Some(true),
            resolved_package: None,
            extra_data: None,
        });
    }

    dependencies
}

/// Split OSGi comma-separated list, respecting quoted strings.
///
/// OSGi headers can contain commas within quoted strings:
/// "foo;version=\"[1.0,2.0)\",bar;version=\"3.0\""
pub(crate) fn split_osgi_list(list: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in list.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                current.push(ch);
            }
            ',' if !in_quotes => {
                if !current.trim().is_empty() {
                    result.push(current.trim().to_string());
                }
                current.clear();
            }
            _ => {
                current.push(ch);
            }
        }
    }

    if !current.trim().is_empty() {
        result.push(current.trim().to_string());
    }

    result
}

fn extract_osgi_directive(entry: &str, directive: &str) -> Option<String> {
    let needle = format!("{}=", directive);
    let version_pos = entry.find(&needle)?;
    let after_value = &entry[version_pos + needle.len()..];

    if let Some(stripped) = after_value.strip_prefix('"') {
        stripped.find('"').map(|end| stripped[..end].to_string())
    } else {
        let end = after_value.find(';').unwrap_or(after_value.len());
        Some(after_value[..end].trim().to_string())
    }
}

pub(crate) fn extract_osgi_version(entry: &str) -> Option<String> {
    extract_osgi_directive(entry, "version")
}

pub(crate) fn extract_osgi_bundle_version(entry: &str) -> Option<String> {
    extract_osgi_directive(entry, "bundle-version")
}

fn default_package_data(datasource_id: DatasourceId) -> PackageData {
    PackageData {
        package_type: Some(PackageType::Maven),
        datasource_id: Some(datasource_id),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_organization_extraction() {
        let temp_dir = TempDir::new().unwrap();
        let pom_path = temp_dir.path().join("pom.xml");

        let pom_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<project>
    <modelVersion>4.0.0</modelVersion>
    <groupId>com.example</groupId>
    <artifactId>my-app</artifactId>
    <version>1.0.0</version>
    <organization>
        <name>Example Corporation</name>
        <url>https://example.com</url>
    </organization>
</project>"#;

        fs::write(&pom_path, pom_content).unwrap();

        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(package_data.name, Some("my-app".to_string()));
        assert_eq!(package_data.namespace, Some("com.example".to_string()));
        assert_eq!(package_data.version, Some("1.0.0".to_string()));

        let extra_data = package_data.extra_data.unwrap();
        assert_eq!(
            extra_data.get("organization_name"),
            Some(&serde_json::Value::String(
                "Example Corporation".to_string()
            ))
        );
        assert_eq!(
            extra_data.get("organization_url"),
            Some(&serde_json::Value::String(
                "https://example.com".to_string()
            ))
        );
    }

    #[test]
    fn test_scm_metadata_extraction() {
        let temp_dir = TempDir::new().unwrap();
        let pom_path = temp_dir.path().join("pom.xml");

        let pom_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 http://maven.apache.org/xsd/maven-4.0.0.xsd">
    <modelVersion>4.0.0</modelVersion>
    <groupId>org.springframework.boot</groupId>
    <artifactId>spring-boot-starter-web</artifactId>
    <version>3.0.0</version>
    <scm>
        <connection>scm:git:https://github.com/spring-projects/spring-boot.git</connection>
        <developerConnection>scm:git:git@github.com:spring-projects/spring-boot.git</developerConnection>
        <url>https://github.com/spring-projects/spring-boot</url>
        <tag>v3.0.0</tag>
    </scm>
</project>"#;

        fs::write(&pom_path, pom_content).unwrap();

        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(
            package_data.name,
            Some("spring-boot-starter-web".to_string())
        );
        assert_eq!(
            package_data.namespace,
            Some("org.springframework.boot".to_string())
        );
        assert_eq!(package_data.version, Some("3.0.0".to_string()));

        assert_eq!(
            package_data.code_view_url,
            Some("https://github.com/spring-projects/spring-boot".to_string())
        );

        // vcs_url prefers connection over developerConnection
        assert_eq!(
            package_data.vcs_url,
            Some("git+https://github.com/spring-projects/spring-boot.git".to_string())
        );

        let extra_data = package_data.extra_data.unwrap();
        assert_eq!(
            extra_data.get("scm_tag"),
            Some(&serde_json::Value::String("v3.0.0".to_string()))
        );
        // developerConnection stored separately in extra_data
        assert_eq!(
            extra_data.get("scm_developer_connection"),
            Some(&serde_json::Value::String(
                "git+git@github.com:spring-projects/spring-boot.git".to_string()
            ))
        );
    }

    #[test]
    fn test_developers_and_contributors_extraction() {
        let temp_dir = TempDir::new().unwrap();
        let pom_path = temp_dir.path().join("pom.xml");

        let pom_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 http://maven.apache.org/xsd/maven-4.0.0.xsd">
    <modelVersion>4.0.0</modelVersion>
    <groupId>com.example</groupId>
    <artifactId>test-app</artifactId>
    <version>1.0.0</version>
    <developers>
        <developer>
            <id>jdoe</id>
            <name>John Doe</name>
            <email>john@example.com</email>
            <url>https://example.com/jdoe</url>
            <organization>Example Corp</organization>
            <organizationUrl>https://example.com</organizationUrl>
            <timezone>America/New_York</timezone>
        </developer>
        <developer>
            <name>Jane Smith</name>
            <email>jane@example.com</email>
        </developer>
    </developers>
    <contributors>
        <contributor>
            <name>Bob Wilson</name>
            <email>bob@example.com</email>
            <url>https://example.com/bob</url>
        </contributor>
    </contributors>
</project>"#;

        fs::write(&pom_path, pom_content).unwrap();

        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(package_data.name, Some("test-app".to_string()));
        assert_eq!(package_data.parties.len(), 3);

        let dev1 = &package_data.parties[0];
        assert_eq!(dev1.r#type, Some("person".to_string()));
        assert_eq!(dev1.role, Some("developer".to_string()));
        assert_eq!(dev1.name, Some("John Doe".to_string()));
        assert_eq!(dev1.email, Some("john@example.com".to_string()));
        assert_eq!(dev1.url, Some("https://example.com/jdoe".to_string()));
        assert_eq!(dev1.organization, Some("Example Corp".to_string()));
        assert_eq!(
            dev1.organization_url,
            Some("https://example.com".to_string())
        );
        assert_eq!(dev1.timezone, Some("America/New_York".to_string()));

        let dev2 = &package_data.parties[1];
        assert_eq!(dev2.r#type, Some("person".to_string()));
        assert_eq!(dev2.role, Some("developer".to_string()));
        assert_eq!(dev2.name, Some("Jane Smith".to_string()));
        assert_eq!(dev2.email, Some("jane@example.com".to_string()));

        let contrib = &package_data.parties[2];
        assert_eq!(contrib.r#type, Some("person".to_string()));
        assert_eq!(contrib.role, Some("contributor".to_string()));
        assert_eq!(contrib.name, Some("Bob Wilson".to_string()));
        assert_eq!(contrib.email, Some("bob@example.com".to_string()));
        assert_eq!(contrib.url, Some("https://example.com/bob".to_string()));
    }

    #[test]
    fn test_issue_management_extraction() {
        let temp_dir = TempDir::new().unwrap();
        let pom_path = temp_dir.path().join("pom.xml");

        let pom_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 http://maven.apache.org/xsd/maven-4.0.0.xsd">
    <modelVersion>4.0.0</modelVersion>
    <groupId>com.example</groupId>
    <artifactId>test-app</artifactId>
    <version>1.0.0</version>
    <issueManagement>
        <system>GitHub</system>
        <url>https://github.com/example/test-app/issues</url>
    </issueManagement>
</project>"#;

        fs::write(&pom_path, pom_content).unwrap();

        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(package_data.name, Some("test-app".to_string()));
        assert_eq!(
            package_data.bug_tracking_url,
            Some("https://github.com/example/test-app/issues".to_string())
        );

        let extra_data = package_data.extra_data.unwrap();
        assert_eq!(
            extra_data.get("issue_tracking_system"),
            Some(&serde_json::Value::String("GitHub".to_string()))
        );
    }

    #[test]
    fn test_ci_management_extraction() {
        let temp_dir = TempDir::new().unwrap();
        let pom_path = temp_dir.path().join("pom.xml");

        let pom_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 http://maven.apache.org/xsd/maven-4.0.0.xsd">
    <modelVersion>4.0.0</modelVersion>
    <groupId>com.example</groupId>
    <artifactId>test-app</artifactId>
    <version>1.0.0</version>
    <ciManagement>
        <system>Jenkins</system>
        <url>https://ci.example.com/job/test-app</url>
    </ciManagement>
</project>"#;

        fs::write(&pom_path, pom_content).unwrap();

        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(package_data.name, Some("test-app".to_string()));

        let extra_data = package_data.extra_data.unwrap();
        assert_eq!(
            extra_data.get("ci_system"),
            Some(&serde_json::Value::String("Jenkins".to_string()))
        );
        assert_eq!(
            extra_data.get("ci_url"),
            Some(&serde_json::Value::String(
                "https://ci.example.com/job/test-app".to_string()
            ))
        );
    }

    #[test]
    fn test_distribution_management_extraction() {
        let temp_dir = TempDir::new().unwrap();
        let pom_path = temp_dir.path().join("pom.xml");

        let pom_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 http://maven.apache.org/xsd/maven-4.0.0.xsd">
    <modelVersion>4.0.0</modelVersion>
    <groupId>com.example</groupId>
    <artifactId>test-app</artifactId>
    <version>1.0.0</version>
    <distributionManagement>
        <downloadUrl>https://example.com/downloads</downloadUrl>
        <repository>
            <id>releases</id>
            <name>Release Repository</name>
            <url>https://repo.example.com/releases</url>
            <layout>default</layout>
        </repository>
        <snapshotRepository>
            <id>snapshots</id>
            <name>Snapshot Repository</name>
            <url>https://repo.example.com/snapshots</url>
            <layout>default</layout>
        </snapshotRepository>
        <site>
            <id>site-deploy</id>
            <name>Project Site</name>
            <url>https://example.com/site</url>
        </site>
    </distributionManagement>
</project>"#;

        fs::write(&pom_path, pom_content).unwrap();

        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(package_data.name, Some("test-app".to_string()));
        assert_eq!(
            package_data.download_url,
            Some("https://example.com/downloads".to_string())
        );

        let extra_data = package_data.extra_data.unwrap();

        assert_eq!(
            extra_data.get("distribution_download_url"),
            Some(&serde_json::Value::String(
                "https://example.com/downloads".to_string()
            ))
        );

        let repo = extra_data
            .get("distribution_repository")
            .unwrap()
            .as_object()
            .unwrap();
        assert_eq!(
            repo.get("id"),
            Some(&serde_json::Value::String("releases".to_string()))
        );
        assert_eq!(
            repo.get("name"),
            Some(&serde_json::Value::String("Release Repository".to_string()))
        );
        assert_eq!(
            repo.get("url"),
            Some(&serde_json::Value::String(
                "https://repo.example.com/releases".to_string()
            ))
        );
        assert_eq!(
            repo.get("layout"),
            Some(&serde_json::Value::String("default".to_string()))
        );

        let snapshot_repo = extra_data
            .get("distribution_snapshot_repository")
            .unwrap()
            .as_object()
            .unwrap();
        assert_eq!(
            snapshot_repo.get("id"),
            Some(&serde_json::Value::String("snapshots".to_string()))
        );
        assert_eq!(
            snapshot_repo.get("name"),
            Some(&serde_json::Value::String(
                "Snapshot Repository".to_string()
            ))
        );
        assert_eq!(
            snapshot_repo.get("url"),
            Some(&serde_json::Value::String(
                "https://repo.example.com/snapshots".to_string()
            ))
        );
        assert_eq!(
            snapshot_repo.get("layout"),
            Some(&serde_json::Value::String("default".to_string()))
        );

        let site = extra_data
            .get("distribution_site")
            .unwrap()
            .as_object()
            .unwrap();
        assert_eq!(
            site.get("id"),
            Some(&serde_json::Value::String("site-deploy".to_string()))
        );
        assert_eq!(
            site.get("name"),
            Some(&serde_json::Value::String("Project Site".to_string()))
        );
        assert_eq!(
            site.get("url"),
            Some(&serde_json::Value::String(
                "https://example.com/site".to_string()
            ))
        );
    }
}

crate::register_parser!(
    "Apache Maven POM",
    &[
        "**/*.pom",
        "**/pom.xml",
        "**/pom.properties",
        "**/META-INF/MANIFEST.MF"
    ],
    "maven",
    "Java",
    Some("https://maven.apache.org/pom.html"),
);
