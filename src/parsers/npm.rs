// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Parser for npm package.json manifests.
//!
//! Extracts package metadata, dependencies, and license information from
//! package.json files used by Node.js/npm projects.
//!
//! # Supported Formats
//! - package.json (manifest)
//!
//! # Key Features
//! - Full dependency extraction (dependencies, devDependencies, peerDependencies, optionalDependencies, bundledDependencies)
//! - Package URL (purl) generation for scoped and unscoped packages
//! - VCS repository URL extraction
//! - Distribution integrity hash extraction (sha1, sha512)
//! - Support for legacy formats (licenses array, license objects)
//!
//! # Implementation Notes
//! - Uses serde_json for JSON parsing
//! - Namespace format: `@org` for scoped packages (e.g., `@babel/core`)
//! - Graceful error handling: logs warnings and returns default on parse failure

use crate::models::{
    DatasourceId, Dependency, PackageData, PackageType, Party, Sha1Digest, Sha256Digest,
    Sha512Digest,
};
use crate::parser_warn as warn;
use crate::parsers::utils::{MAX_ITERATION_COUNT, npm_purl, parse_sri, truncate_field};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

use super::PackageParser;
use super::license_normalization::normalize_spdx_declared_license;

const FIELD_NAME: &str = "name";
const FIELD_VERSION: &str = "version";
const FIELD_LICENSE: &str = "license";
const FIELD_LICENSES: &str = "licenses";
const FIELD_HOMEPAGE: &str = "homepage";
const FIELD_REPOSITORY: &str = "repository";
const FIELD_AUTHOR: &str = "author";
const FIELD_CONTRIBUTORS: &str = "contributors";
const FIELD_MAINTAINERS: &str = "maintainers";
const FIELD_DEPENDENCIES: &str = "dependencies";
const FIELD_DEV_DEPENDENCIES: &str = "devDependencies";
const FIELD_PEER_DEPENDENCIES: &str = "peerDependencies";
const FIELD_OPTIONAL_DEPENDENCIES: &str = "optionalDependencies";
const FIELD_BUNDLED_DEPENDENCIES: &str = "bundledDependencies";
const FIELD_RESOLUTIONS: &str = "resolutions";
const FIELD_DESCRIPTION: &str = "description";
const FIELD_KEYWORDS: &str = "keywords";
const FIELD_ENGINES: &str = "engines";
const FIELD_OS: &str = "os";
const FIELD_CPU: &str = "cpu";
const FIELD_LIBC: &str = "libc";
const FIELD_DEPRECATED: &str = "deprecated";
const FIELD_HAS_BIN: &str = "hasBin";
const FIELD_PACKAGE_MANAGER: &str = "packageManager";
const FIELD_WORKSPACES: &str = "workspaces";
const FIELD_PRIVATE: &str = "private";
const FIELD_BUGS: &str = "bugs";
const FIELD_DIST: &str = "dist";
const FIELD_OVERRIDES: &str = "overrides";
const FIELD_PEER_DEPENDENCIES_META: &str = "peerDependenciesMeta";
const FIELD_DEPENDENCIES_META: &str = "dependenciesMeta";

/// npm package parser for package.json manifests.
///
/// Supports all npm dependency types (dependencies, devDependencies, peerDependencies,
/// optionalDependencies, bundledDependencies) and workspace configurations.
pub struct NpmParser;

impl PackageParser for NpmParser {
    const PACKAGE_TYPE: PackageType = PackageType::Npm;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let (json, _field_lines) = match read_and_parse_json_with_lines(path) {
            Ok((json, lines)) => (json, lines),
            Err(e) => {
                warn!("Failed to read or parse package.json at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        let name = extract_non_empty_string(&json, FIELD_NAME);
        let version = extract_non_empty_string(&json, FIELD_VERSION);
        let namespace = extract_namespace(&name);
        let package_name = extract_package_name(&name);
        let description = extract_description(&json);

        let extracted_license_statement = extract_license_statement(&json);
        let (declared_license_expression, declared_license_expression_spdx, license_detections) =
            normalize_spdx_declared_license(extract_declared_license_candidate(&json).as_deref());
        let peer_dependencies_meta = extract_peer_dependencies_meta(&json);
        let dependencies = extract_dependencies(&json, false);
        let dev_dependencies = extract_dependencies(&json, true);
        let peer_dependencies = extract_peer_dependencies(&json, &peer_dependencies_meta);
        let optional_dependencies = extract_optional_dependencies(&json);
        let bundled_dependencies = extract_bundled_dependencies(&json);
        let purl = create_package_url(&name, &version, &namespace);
        let keywords_vec = extract_keywords_as_vec(&json);

        let mut extra_data_map = HashMap::new();

        if let Some(resolutions) = extract_resolutions(&json) {
            extra_data_map = combine_extra_data(Some(extra_data_map), resolutions);
        }

        if let Some(engines) = extract_engines(&json) {
            extra_data_map.insert("engines".to_string(), engines);
        }

        for field in [
            FIELD_OS,
            FIELD_CPU,
            FIELD_LIBC,
            FIELD_DEPRECATED,
            FIELD_HAS_BIN,
        ] {
            if let Some(value) = extract_raw_extra_data_field(&json, field) {
                extra_data_map.insert(field.to_string(), value);
            }
        }

        if let Some(package_manager) = extract_package_manager(&json) {
            extra_data_map.insert(
                "packageManager".to_string(),
                serde_json::Value::String(package_manager),
            );
        }

        if let Some(workspaces) = extract_workspaces(&json) {
            extra_data_map.insert("workspaces".to_string(), workspaces);
        }

        if let Some(overrides) = extract_overrides(&json) {
            extra_data_map.insert("overrides".to_string(), overrides);
        }

        if let Some(private) = extract_private(&json) {
            extra_data_map.insert("private".to_string(), serde_json::Value::Bool(private));
        }

        if let Some(dependencies_meta) = extract_dependencies_meta(&json) {
            extra_data_map.insert("dependenciesMeta".to_string(), dependencies_meta);
        }

        let extra_data = if extra_data_map.is_empty() {
            None
        } else {
            Some(extra_data_map)
        };

        let (dist_sha1, dist_sha256, dist_sha512) = match json.get(FIELD_DIST) {
            Some(dist) => extract_dist_hashes(dist),
            None => (None, None, None),
        };

        let download_url = json
            .get(FIELD_DIST)
            .and_then(extract_dist_tarball)
            .or_else(|| generate_registry_download_url(&namespace, &package_name, &version));

        let api_data_url = generate_npm_api_url(&namespace, &package_name, &version);
        let repository_homepage_url = generate_repository_homepage_url(&namespace, &package_name);
        let repository_download_url =
            generate_repository_download_url(&namespace, &package_name, &version);
        let vcs_url = extract_vcs_url(&json);

        vec![PackageData {
            package_type: Some(Self::PACKAGE_TYPE),
            namespace,
            name: package_name,
            version,
            qualifiers: None,
            subpath: None,
            primary_language: Some("JavaScript".to_string()),
            description,
            release_date: None,
            parties: extract_parties(&json),
            keywords: keywords_vec,
            homepage_url: extract_homepage_url(&json),
            download_url,
            size: None,
            sha1: dist_sha1.and_then(|h| Sha1Digest::from_hex(&h).ok()),
            md5: None,
            sha256: dist_sha256.and_then(|h| Sha256Digest::from_hex(&h).ok()),
            sha512: dist_sha512.and_then(|h| Sha512Digest::from_hex(&h).ok()),
            bug_tracking_url: extract_bugs(&json),
            code_view_url: None,
            vcs_url,
            copyright: None,
            holder: None,
            declared_license_expression,
            declared_license_expression_spdx,
            license_detections,
            other_license_expression: None,
            other_license_expression_spdx: None,
            other_license_detections: Vec::new(),
            extracted_license_statement,
            notice_text: None,
            source_packages: Vec::new(),
            file_references: Vec::new(),
            is_private: json
                .get("private")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            is_virtual: false,
            extra_data,
            dependencies: [
                dependencies,
                dev_dependencies,
                peer_dependencies,
                optional_dependencies,
                bundled_dependencies,
            ]
            .concat(),
            repository_homepage_url,
            repository_download_url,
            api_data_url,
            datasource_id: Some(DatasourceId::NpmPackageJson),
            purl,
        }]
    }

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "package.json")
    }
}

/// Reads and parses a JSON file while tracking line numbers of fields
fn read_and_parse_json_with_lines(path: &Path) -> Result<(Value, HashMap<String, usize>), String> {
    // Read file once into string
    let content = crate::parsers::utils::read_file_to_string(path, None)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    // Parse JSON
    let json: Value =
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse JSON: {}", e))?;

    // Track line numbers for each field by iterating over lines
    let mut field_lines = HashMap::new();
    for (line_num, line) in content.lines().enumerate().take(MAX_ITERATION_COUNT) {
        let trimmed = line.trim();
        if let Some(field_name) = extract_field_name(trimmed) {
            field_lines.insert(field_name, line_num + 1);
        }
    }

    Ok((json, field_lines))
}

/// Extracts field name from a JSON line
fn extract_field_name(line: &str) -> Option<String> {
    // Simple regex-free parsing for field names
    let line = line.trim();
    if line.is_empty() || !line.starts_with("\"") {
        return None;
    }

    // Find the closing quote of the field name
    let mut chars = line.chars();
    chars.next(); // Skip opening quote

    let mut field_name = String::new();
    for c in chars {
        if c == '"' {
            break;
        }
        field_name.push(c);
    }

    if field_name.is_empty() {
        None
    } else {
        Some(field_name)
    }
}

fn extract_namespace(name: &Option<String>) -> Option<String> {
    name.as_ref().and_then(|n| {
        if n.contains('/') {
            n.split('/').next().map(String::from)
        } else {
            None
        }
    })
}

fn extract_package_name(name: &Option<String>) -> Option<String> {
    name.as_ref().map(|n| {
        if n.contains('/') {
            n.split('/').nth(1).unwrap_or(n).to_string()
        } else {
            n.clone()
        }
    })
}

fn create_package_url(
    name: &Option<String>,
    version: &Option<String>,
    _namespace: &Option<String>,
) -> Option<String> {
    // Note: We extract and store namespace in PackageData for metadata purposes,
    // but the full package name (e.g., "@babel/core") is used for PURL generation.
    let name = name.as_ref()?;
    npm_purl(name, version.as_deref())
}

fn extract_license_statement(json: &Value) -> Option<String> {
    let mut statements = Vec::new();

    if let Some(license_value) = json.get(FIELD_LICENSE) {
        if let Some(license_str) = license_value.as_str() {
            statements.push(format!("- {}", license_str));
        } else if let Some(license_obj) = license_value.as_object()
            && let Some(type_val) = license_obj.get("type").and_then(|v| v.as_str())
        {
            statements.push(format!("- type: {}", type_val));
            if let Some(url_val) = license_obj.get("url").and_then(|v| v.as_str()) {
                statements.push(format!("  url: {}", url_val));
            }
        }
    }

    if let Some(licenses) = json.get(FIELD_LICENSES).and_then(|v| v.as_array()) {
        for license in licenses.iter().take(MAX_ITERATION_COUNT) {
            if let Some(license_obj) = license.as_object()
                && let Some(type_val) = license_obj.get("type").and_then(|v| v.as_str())
            {
                statements.push(format!("- type: {}", type_val));
                if let Some(url_val) = license_obj.get("url").and_then(|v| v.as_str()) {
                    statements.push(format!("  url: {}", url_val));
                }
            }
        }
    }

    if statements.is_empty() {
        None
    } else {
        Some(truncate_field(format!("{}\n", statements.join("\n"))))
    }
}

fn extract_declared_license_candidate(json: &Value) -> Option<String> {
    json.get(FIELD_LICENSE)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|s| truncate_field(s.to_string()))
}

/// Extracts the repository URL from the repository field.
/// Extracts and normalizes VCS URL from the repository field.
/// Supports both string and object formats with optional 'type' and 'directory' fields.
fn extract_vcs_url(json: &Value) -> Option<String> {
    let (vcs_tool, vcs_repository) = match json.get(FIELD_REPOSITORY) {
        Some(Value::String(url)) => {
            let normalized = normalize_repo_url(url);
            if normalized.is_empty() {
                return None;
            }
            (None, normalized)
        }
        Some(Value::Object(obj)) => {
            let repo_url = obj.get("url").and_then(|u| u.as_str()).unwrap_or("");
            let normalized = normalize_repo_url(repo_url);
            if normalized.is_empty() {
                return None;
            }
            let tool = obj
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("git")
                .to_string();
            let tool_for_prefix = if normalized.starts_with("git://")
                || normalized.starts_with("git+")
                || normalized.starts_with("hg://")
                || normalized.starts_with("hg+")
                || normalized.starts_with("svn://")
                || normalized.starts_with("svn+")
            {
                None
            } else {
                Some(tool)
            };
            (tool_for_prefix, normalized)
        }
        _ => return None,
    };

    if vcs_repository.is_empty() {
        return None;
    }

    let mut vcs_url = vcs_tool.map_or_else(
        || vcs_repository.clone(),
        |tool| format!("{}+{}", tool, vcs_repository),
    );

    if let Some(vcs_revision) = json
        .get("gitHead")
        .and_then(|v| v.as_str())
        .and_then(normalize_non_empty_string)
    {
        vcs_url.push('@');
        vcs_url.push_str(&vcs_revision);
    }

    if let Some(Value::Object(obj)) = json.get(FIELD_REPOSITORY)
        && let Some(directory) = obj.get("directory").and_then(|d| d.as_str())
    {
        vcs_url.push('#');
        vcs_url.push_str(directory);
    }

    Some(truncate_field(vcs_url))
}

/// Normalizes repository URLs by converting various formats to a standard HTTPS URL.
/// Based on normalize_vcs_url() from Python reference.
fn normalize_repo_url(url: &str) -> String {
    let url = url.trim();

    if url.is_empty() {
        return String::new();
    }

    let normalized_schemes = [
        "https://",
        "http://",
        "git://",
        "git+git://",
        "git+https://",
        "git+http://",
        "hg://",
        "hg+http://",
        "hg+https://",
        "svn://",
        "svn+http://",
        "svn+https://",
    ];
    if normalized_schemes
        .iter()
        .any(|scheme| url.starts_with(scheme))
    {
        return url.to_string();
    }

    if let Some((host, repo)) = url
        .strip_prefix("git@")
        .and_then(|rest| rest.split_once(':'))
    {
        return format!("https://{}/{}", host, repo);
    }

    if let Some((platform, repo)) = url.split_once(':') {
        let host_url = match platform {
            "github" => "https://github.com/",
            "gitlab" => "https://gitlab.com/",
            "bitbucket" => "https://bitbucket.org/",
            "gist" => "https://gist.github.com/",
            _ => return url.to_string(),
        };
        return format!("{}{}", host_url, repo);
    }

    if !url.contains(':') && url.chars().filter(|&c| c == '/').count() == 1 {
        return format!("https://github.com/{}", url);
    }

    url.to_string()
}

/// Extracts party information (emails) from the `author`, `contributors`, and `maintainers` fields.
fn extract_parties(json: &Value) -> Vec<Party> {
    let mut parties = Vec::new();

    // Extract author field (can be single value or array)
    if let Some(author) = json.get(FIELD_AUTHOR) {
        if let Some(author_list) = extract_parties_from_array(author) {
            // Author is an array
            for mut party in author_list {
                if party.role.is_none() {
                    party.role = Some("author".to_string());
                }
                parties.push(party);
            }
        } else if let Some(mut party) = extract_party_from_field(author) {
            // Author is a single value
            party.role = Some("author".to_string());
            parties.push(party);
        }
    }

    // Extract contributors field
    if let Some(contributors) = json.get(FIELD_CONTRIBUTORS)
        && let Some(mut party_list) = extract_parties_from_array(contributors)
    {
        for party in &mut party_list {
            if party.role.is_none() {
                party.role = Some("contributor".to_string());
            }
        }
        parties.extend(party_list);
    }

    // Extract maintainers field
    if let Some(maintainers) = json.get(FIELD_MAINTAINERS)
        && let Some(mut party_list) = extract_parties_from_array(maintainers)
    {
        for party in &mut party_list {
            if party.role.is_none() {
                party.role = Some("maintainer".to_string());
            }
        }
        parties.extend(party_list);
    }

    parties
}

/// Extracts a party from a JSON field, which can be a string or an object with name/email fields.
fn extract_party_from_field(field: &Value) -> Option<Party> {
    match field {
        Value::String(s) => {
            if let Some(email) = extract_email_from_string(s) {
                Some(Party {
                    r#type: Some("person".to_string()),
                    role: None,
                    name: extract_name_from_author_string(s).map(truncate_field),
                    email: Some(truncate_field(email)),
                    url: None,
                    organization: None,
                    organization_url: None,
                    timezone: None,
                })
            } else {
                Some(Party {
                    r#type: Some("person".to_string()),
                    role: None,
                    name: Some(truncate_field(s.clone())),
                    email: None,
                    url: None,
                    organization: None,
                    organization_url: None,
                    timezone: None,
                })
            }
        }
        Value::Object(obj) => Some(Party {
            r#type: Some("person".to_string()),
            role: obj
                .get("role")
                .and_then(|v| v.as_str())
                .map(|s| truncate_field(s.to_string())),
            name: obj
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| truncate_field(s.to_string())),
            email: obj
                .get("email")
                .and_then(|v| v.as_str())
                .map(|s| truncate_field(s.to_string())),
            url: obj
                .get("url")
                .and_then(|v| v.as_str())
                .and_then(normalize_optional_party_url)
                .map(truncate_field),
            organization: None,
            organization_url: None,
            timezone: None,
        }),
        _ => None,
    }
}

/// Extracts multiple parties from a JSON array.
fn extract_parties_from_array(array: &Value) -> Option<Vec<Party>> {
    if let Value::Array(items) = array {
        let parties = items
            .iter()
            .take(MAX_ITERATION_COUNT)
            .filter_map(extract_party_from_field)
            .collect::<Vec<_>>();
        if !parties.is_empty() {
            return Some(parties);
        }
    }
    None
}

/// Extracts email from a string in the format "Name <email@example.com>".
fn extract_email_from_string(author_str: &str) -> Option<String> {
    if let Some(email_start) = author_str.find('<')
        && let Some(email_end) = author_str.find('>')
        && email_start < email_end
    {
        return Some(author_str[email_start + 1..email_end].to_string());
    }
    None
}

/// Extracts name from a string in the format "Name <email@example.com>" or returns full string as name.
fn extract_name_from_author_string(author_str: &str) -> Option<String> {
    if let Some(end_idx) = author_str.find('<') {
        let name = author_str[..end_idx].trim();
        if !name.is_empty() {
            return Some(name.to_string());
        }
    } else {
        return Some(author_str.trim().to_string());
    }
    None
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(NpmParser::PACKAGE_TYPE),
        primary_language: Some("JavaScript".to_string()),
        datasource_id: Some(DatasourceId::NpmPackageJson),
        ..Default::default()
    }
}

fn parse_alias_adapter(version_str: &str) -> Option<(&str, &str)> {
    if version_str.contains(':') && version_str.contains('@') {
        let (aliased_package_part, constraint) = version_str.rsplit_once('@')?;
        let (_, actual_package_name) = aliased_package_part.rsplit_once(':')?;
        return Some((actual_package_name, constraint));
    }
    None
}

fn extract_non_empty_string(json: &Value, field: &str) -> Option<String> {
    json.get(field)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|s| truncate_field(s.to_string()))
}

fn generate_npm_api_url(
    namespace: &Option<String>,
    name: &Option<String>,
    version: &Option<String>,
) -> Option<String> {
    const REGISTRY: &str = "https://registry.npmjs.org";
    name.as_ref()?;

    let ns_name = if let Some(ns) = namespace {
        format!("{}/{}", ns, name.as_ref()?).replace('/', "%2f")
    } else {
        name.as_ref()?.clone()
    };

    let url = if let Some(ver) = version {
        format!("{}/{}/{}", REGISTRY, ns_name, ver)
    } else {
        format!("{}/{}", REGISTRY, ns_name)
    };

    Some(url)
}

fn build_registry_package_path(
    namespace: &Option<String>,
    name: &Option<String>,
) -> Option<String> {
    match (namespace.as_ref(), name.as_ref()) {
        (Some(namespace), Some(name)) => Some(format!("{namespace}/{name}")),
        (None, Some(name)) => Some(name.clone()),
        _ => None,
    }
}

fn generate_repository_homepage_url(
    namespace: &Option<String>,
    name: &Option<String>,
) -> Option<String> {
    build_registry_package_path(namespace, name)
        .map(|package_path| format!("https://www.npmjs.com/package/{package_path}"))
}

fn generate_registry_download_url(
    namespace: &Option<String>,
    name: &Option<String>,
    version: &Option<String>,
) -> Option<String> {
    match (
        build_registry_package_path(namespace, name),
        name.as_ref(),
        version.as_ref(),
    ) {
        (Some(package_path), Some(name), Some(version)) => Some(format!(
            "https://registry.npmjs.org/{}/-/{}-{}.tgz",
            package_path, name, version
        )),
        _ => None,
    }
}

fn generate_repository_download_url(
    namespace: &Option<String>,
    name: &Option<String>,
    version: &Option<String>,
) -> Option<String> {
    generate_registry_download_url(namespace, name, version)
}

fn extract_dependency_group(
    json: &Value,
    field: &str,
    scope: &str,
    is_runtime: bool,
    is_optional: bool,
    optional_meta: Option<&HashMap<String, bool>>,
) -> Vec<Dependency> {
    json.get(field)
        .and_then(|deps| deps.as_object())
        .map_or_else(Vec::new, |deps| {
            deps.iter()
                .take(MAX_ITERATION_COUNT)
                .filter_map(|(name, version)| {
                    let version_str = version.as_str()?;

                    if version_str.starts_with("workspace:") {
                        let package_url = npm_purl(name, None)?;
                        let is_opt = if let Some(meta) = optional_meta {
                            meta.get(name).copied()
                        } else {
                            Some(is_optional)
                        };
                        return Some(Dependency {
                            purl: Some(package_url),
                            extracted_requirement: Some(truncate_field(version_str.to_string())),
                            scope: Some(scope.to_string()),
                            is_runtime: Some(is_runtime),
                            is_optional: is_opt,
                            is_pinned: Some(false),
                            is_direct: Some(true),
                            resolved_package: None,
                            extra_data: None,
                        });
                    }

                    let actual_package_name = if let Some((actual_package_name, _constraint)) =
                        parse_alias_adapter(version_str)
                    {
                        actual_package_name
                    } else {
                        name.as_str()
                    };

                    let package_url = npm_purl(actual_package_name, None)?;

                    let is_opt = if let Some(meta) = optional_meta {
                        meta.get(name).copied()
                    } else {
                        Some(is_optional)
                    };

                    Some(Dependency {
                        purl: Some(package_url),
                        extracted_requirement: Some(truncate_field(version_str.to_string())),
                        scope: Some(scope.to_string()),
                        is_runtime: Some(is_runtime),
                        is_optional: is_opt,
                        is_pinned: Some(false),
                        is_direct: Some(true),
                        resolved_package: None,
                        extra_data: None,
                    })
                })
                .collect()
        })
}

/// Extracts dependencies from the `dependencies` or `devDependencies` field in the JSON.
fn extract_dependencies(json: &Value, is_optional: bool) -> Vec<Dependency> {
    let field = if is_optional {
        FIELD_DEV_DEPENDENCIES
    } else {
        FIELD_DEPENDENCIES
    };

    let scope = if is_optional {
        "devDependencies"
    } else {
        "dependencies"
    };

    extract_dependency_group(json, field, scope, !is_optional, is_optional, None)
}

fn extract_peer_dependencies(json: &Value, meta: &HashMap<String, bool>) -> Vec<Dependency> {
    extract_dependency_group(
        json,
        FIELD_PEER_DEPENDENCIES,
        "peerDependencies",
        true,
        false,
        Some(meta),
    )
}

/// Extracts optional dependencies from the `optionalDependencies` field in the JSON.
/// Optional dependencies are marked with is_optional: true, is_runtime: true, and scope "optionalDependencies".
fn extract_optional_dependencies(json: &Value) -> Vec<Dependency> {
    extract_dependency_group(
        json,
        FIELD_OPTIONAL_DEPENDENCIES,
        "optionalDependencies",
        true,
        true,
        None,
    )
}

fn extract_bundled_dependencies(json: &Value) -> Vec<Dependency> {
    if let Some(bundled) = json
        .get(FIELD_BUNDLED_DEPENDENCIES)
        .and_then(|v| v.as_array())
    {
        extract_bundled_list(bundled)
    } else {
        Vec::new()
    }
}

/// Helper function to extract bundled dependencies from an array of package names.
fn extract_bundled_list(bundled_array: &[Value]) -> Vec<Dependency> {
    bundled_array
        .iter()
        .take(MAX_ITERATION_COUNT)
        .filter_map(|value| {
            let name = value.as_str()?;
            // Create PURL without version for bundled dependencies
            let package_url = npm_purl(name, None)?;

            Some(Dependency {
                purl: Some(package_url),
                extracted_requirement: None,
                scope: Some("bundledDependencies".to_string()),
                is_runtime: Some(true),
                is_optional: Some(false),
                is_pinned: Some(false),
                is_direct: Some(true),
                resolved_package: None,
                extra_data: None,
            })
        })
        .collect()
}

/// Extracts Yarn resolutions from the `resolutions` field.
/// Returns resolutions as a HashMap to be stored in extra_data.
fn extract_resolutions(json: &Value) -> Option<HashMap<String, serde_json::Value>> {
    json.get(FIELD_RESOLUTIONS)
        .and_then(|resolutions| resolutions.as_object())
        .map(|resolutions_obj| {
            let mut extra_data = HashMap::new();
            extra_data.insert(
                "resolutions".to_string(),
                serde_json::Value::Object(resolutions_obj.clone()),
            );
            extra_data
        })
}

fn extract_peer_dependencies_meta(json: &Value) -> HashMap<String, bool> {
    json.get(FIELD_PEER_DEPENDENCIES_META)
        .and_then(|meta| meta.as_object())
        .map_or_else(HashMap::new, |meta_obj| {
            meta_obj
                .iter()
                .take(MAX_ITERATION_COUNT)
                .filter_map(|(package_name, meta_value)| {
                    meta_value.as_object().and_then(|obj| {
                        obj.get("optional")
                            .and_then(|opt| opt.as_bool())
                            .map(|optional| (package_name.clone(), optional))
                    })
                })
                .collect()
        })
}

fn extract_dependencies_meta(json: &Value) -> Option<serde_json::Value> {
    json.get(FIELD_DEPENDENCIES_META).cloned()
}

fn extract_overrides(json: &Value) -> Option<serde_json::Value> {
    json.get(FIELD_OVERRIDES).cloned()
}

fn extract_description(json: &Value) -> Option<String> {
    json.get(FIELD_DESCRIPTION)
        .and_then(|v| v.as_str())
        .map(|s| truncate_field(s.to_string()))
}

fn extract_homepage_url(json: &Value) -> Option<String> {
    match json.get(FIELD_HOMEPAGE) {
        Some(Value::String(homepage)) => normalize_non_empty_string(homepage).map(truncate_field),
        _ => None,
    }
}

fn normalize_non_empty_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_optional_party_url(value: &str) -> Option<String> {
    let normalized = normalize_non_empty_string(value)?;

    if normalized.eq_ignore_ascii_case("none") {
        None
    } else {
        Some(normalized)
    }
}

fn extract_keywords_as_vec(json: &Value) -> Vec<String> {
    json.get(FIELD_KEYWORDS)
        .and_then(|v| {
            if let Some(str) = v.as_str() {
                Some(vec![str.to_string()])
            } else if let Some(arr) = v.as_array() {
                let keywords: Vec<String> = arr
                    .iter()
                    .take(MAX_ITERATION_COUNT)
                    .filter_map(|kw| kw.as_str())
                    .map(|s| truncate_field(s.to_string()))
                    .collect();
                if keywords.is_empty() {
                    None
                } else {
                    Some(keywords)
                }
            } else {
                None
            }
        })
        .unwrap_or_default()
}

fn extract_engines(json: &Value) -> Option<serde_json::Value> {
    json.get(FIELD_ENGINES).cloned()
}

fn extract_raw_extra_data_field(json: &Value, field: &str) -> Option<serde_json::Value> {
    json.get(field).cloned()
}

fn extract_package_manager(json: &Value) -> Option<String> {
    json.get(FIELD_PACKAGE_MANAGER)
        .and_then(|v| v.as_str())
        .map(|s| truncate_field(s.to_string()))
}

fn extract_workspaces(json: &Value) -> Option<serde_json::Value> {
    json.get(FIELD_WORKSPACES).cloned()
}

fn extract_private(json: &Value) -> Option<bool> {
    json.get(FIELD_PRIVATE).and_then(|v| v.as_bool())
}

fn extract_bugs(json: &Value) -> Option<String> {
    match json.get(FIELD_BUGS) {
        Some(bugs) => {
            if let Some(url) = bugs.as_str() {
                normalize_non_empty_string(url).map(truncate_field)
            } else if let Some(obj) = bugs.as_object() {
                obj.get("url")
                    .and_then(|v| v.as_str())
                    .and_then(normalize_non_empty_string)
                    .map(truncate_field)
            } else {
                None
            }
        }
        None => None,
    }
}

fn extract_dist_hashes(dist: &Value) -> (Option<String>, Option<String>, Option<String>) {
    let mut sha1 = dist
        .get("shasum")
        .and_then(|v| v.as_str())
        .and_then(normalize_non_empty_string);
    let mut sha256 = None;
    let mut sha512 = None;

    if let Some(integrity) = dist.get("integrity").and_then(|v| v.as_str())
        && let Some((algo, hex_digest)) = parse_sri(integrity)
    {
        match algo.as_str() {
            "sha1" if sha1.is_none() => sha1 = Some(hex_digest),
            "sha1" => {}
            "sha256" => sha256 = Some(hex_digest),
            "sha512" => sha512 = Some(hex_digest),
            _ => {}
        }
    }

    (sha1, sha256, sha512)
}

fn extract_dist_tarball(dist: &Value) -> Option<String> {
    dist.get("tarball")
        .or_else(|| dist.get("dnl_url"))
        .and_then(|v| v.as_str())
        .map(normalize_npm_registry_tarball_url)
        .map(truncate_field)
}

fn normalize_npm_registry_tarball_url(url: &str) -> String {
    if let Some(path) = url.strip_prefix("http://registry.npmjs.org/") {
        format!("https://registry.npmjs.org/{path}")
    } else {
        url.to_string()
    }
}

fn combine_extra_data(
    extra_data: Option<HashMap<String, serde_json::Value>>,
    additional_data: HashMap<String, serde_json::Value>,
) -> HashMap<String, serde_json::Value> {
    let mut combined = extra_data.unwrap_or_default();
    for (key, value) in additional_data {
        combined.insert(key, value);
    }
    combined
}

crate::register_parser!(
    "npm package.json manifest",
    &["**/package.json"],
    "npm",
    "JavaScript",
    Some("https://docs.npmjs.com/cli/v10/configuring-npm/package-json"),
);
