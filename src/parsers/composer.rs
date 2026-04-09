//!
//! Extracts package metadata and dependencies from PHP Composer manifests
//! (composer.json) and lockfiles (composer.lock).
//!
//! # Supported Formats
//! - composer.json (manifest)
//! - composer.lock (lockfile)
//!
//! # Key Features
//! - Dependency extraction from require and require-dev
//! - PSR-4 autoload and repository metadata capture
//! - Locked dependency versions with dist/source hashes
//!
//! # Implementation Notes
//! - Uses serde_json for parsing
//! - Graceful error handling with warn!()
//! - Package URL (purl) generation via packageurl
//!
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use serde_json::Value;

use crate::models::{
    DatasourceId, Dependency, LicenseDetection, PackageData, PackageType, Party, ResolvedPackage,
    Sha1Digest, Sha256Digest, Sha512Digest,
};

use super::PackageParser;
use super::license_normalization::{
    DeclaredLicenseMatchMetadata, build_declared_license_data_from_pair,
    normalize_spdx_declared_license,
};

const FIELD_NAME: &str = "name";
const FIELD_VERSION: &str = "version";
const FIELD_DESCRIPTION: &str = "description";
const FIELD_HOMEPAGE: &str = "homepage";
const FIELD_TYPE: &str = "type";
const FIELD_LICENSE: &str = "license";
const FIELD_AUTHORS: &str = "authors";
const FIELD_KEYWORDS: &str = "keywords";
const FIELD_REQUIRE: &str = "require";
const FIELD_REQUIRE_DEV: &str = "require-dev";
const FIELD_PROVIDE: &str = "provide";
const FIELD_CONFLICT: &str = "conflict";
const FIELD_REPLACE: &str = "replace";
const FIELD_SUGGEST: &str = "suggest";
const FIELD_SUPPORT: &str = "support";
const FIELD_AUTOLOAD: &str = "autoload";
const FIELD_PSR4: &str = "psr-4";
const FIELD_REPOSITORIES: &str = "repositories";

const FIELD_PACKAGES: &str = "packages";
const FIELD_PACKAGES_DEV: &str = "packages-dev";
const FIELD_SOURCE: &str = "source";
const FIELD_DIST: &str = "dist";

/// Composer manifest parser for composer.json files.
pub struct ComposerJsonParser;

impl PackageParser for ComposerJsonParser {
    const PACKAGE_TYPE: PackageType = PackageType::Composer;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let json_content = match read_json_file(path) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read composer.json at {:?}: {}", path, e);
                return vec![default_package_data(Some(DatasourceId::PhpComposerJson))];
            }
        };

        let full_name = json_content
            .get(FIELD_NAME)
            .and_then(|value| value.as_str())
            .map(|value| value.trim())
            .filter(|value| !value.is_empty());

        let (namespace, name) = split_optional_namespace_name(full_name);
        let is_private = name.is_none();

        let version = json_content
            .get(FIELD_VERSION)
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string());

        let description = json_content
            .get(FIELD_DESCRIPTION)
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        let homepage_url = json_content
            .get(FIELD_HOMEPAGE)
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        let keywords = extract_keywords(&json_content);

        let (
            extracted_license_statement,
            declared_license_expression,
            declared_license_expression_spdx,
            license_detections,
        ) = extract_license_data(&json_content, is_private);

        let dependencies =
            extract_dependencies(&json_content, FIELD_REQUIRE, "require", true, false);
        let dev_dependencies =
            extract_dependencies(&json_content, FIELD_REQUIRE_DEV, "require-dev", false, true);
        let provide_dependencies =
            extract_dependencies(&json_content, FIELD_PROVIDE, "provide", true, false);
        let conflict_dependencies =
            extract_dependencies(&json_content, FIELD_CONFLICT, "conflict", true, true);
        let replace_dependencies =
            extract_dependencies(&json_content, FIELD_REPLACE, "replace", true, true);
        let suggest_dependencies =
            extract_dependencies(&json_content, FIELD_SUGGEST, "suggest", true, true);

        let (bug_tracking_url, code_view_url) = extract_support(&json_content);
        let vcs_url = extract_source_vcs_url(&json_content);
        let download_url = extract_dist_download_url(&json_content);
        let extra_data = build_extra_data(&json_content);
        let parties = extract_parties(&json_content, &namespace);

        vec![PackageData {
            package_type: Some(Self::PACKAGE_TYPE),
            namespace: namespace.clone(),
            name: name.clone(),
            version: version.clone(),
            qualifiers: None,
            subpath: None,
            primary_language: Some("PHP".to_string()),
            description,
            release_date: None,
            parties,
            keywords,
            homepage_url,
            download_url,
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            bug_tracking_url,
            code_view_url,
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
            is_private,
            is_virtual: false,
            extra_data,
            dependencies: [
                dependencies,
                dev_dependencies,
                provide_dependencies,
                conflict_dependencies,
                replace_dependencies,
                suggest_dependencies,
            ]
            .concat(),
            repository_homepage_url: build_repository_homepage_url(&namespace, &name),
            repository_download_url: None,
            api_data_url: build_api_data_url(&namespace, &name),
            datasource_id: Some(DatasourceId::PhpComposerJson),
            purl: build_package_purl(&namespace, &name, &version),
        }]
    }

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_composer_manifest_filename)
    }
}

/// Composer lockfile parser for composer.lock files.
pub struct ComposerLockParser;

impl PackageParser for ComposerLockParser {
    const PACKAGE_TYPE: PackageType = PackageType::Composer;

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let json_content = match read_json_file(path) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read composer.lock at {:?}: {}", path, e);
                return vec![default_package_data(Some(DatasourceId::PhpComposerLock))];
            }
        };

        let dependencies = extract_lock_dependencies(&json_content);

        let mut package_data = default_package_data(Some(DatasourceId::PhpComposerLock));
        package_data.dependencies = dependencies;
        vec![package_data]
    }

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_composer_lock_filename)
    }
}

fn is_composer_manifest_filename(name: &str) -> bool {
    name == "composer.json"
        || name.ends_with(".composer.json")
        || (name.starts_with("composer.") && name.ends_with(".json"))
}

fn is_composer_lock_filename(name: &str) -> bool {
    name == "composer.lock"
        || name.ends_with(".composer.lock")
        || (name.starts_with("composer.") && name.ends_with(".lock"))
}

fn read_json_file(path: &Path) -> Result<Value, String> {
    let mut file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| format!("Failed to read file: {}", e))?;
    serde_json::from_str(&content).map_err(|e| format!("Failed to parse JSON: {}", e))
}

fn extract_dependencies(
    json_content: &Value,
    field: &str,
    scope: &str,
    is_runtime: bool,
    is_optional: bool,
) -> Vec<Dependency> {
    json_content
        .get(field)
        .and_then(|value| value.as_object())
        .map_or_else(Vec::new, |deps| {
            deps.iter()
                .filter_map(|(name, requirement)| {
                    let requirement_str = requirement.as_str()?;
                    let (namespace, package_name) = split_namespace_name(name);
                    let is_pinned = is_composer_version_pinned(requirement_str);
                    let version_for_purl = if is_pinned {
                        Some(normalize_requirement_version(requirement_str))
                    } else {
                        None
                    };

                    let purl = build_dependency_purl(
                        namespace.as_deref(),
                        &package_name,
                        version_for_purl.as_deref(),
                    );

                    Some(Dependency {
                        purl,
                        extracted_requirement: Some(requirement_str.to_string()),
                        scope: Some(scope.to_string()),
                        is_runtime: Some(is_runtime),
                        is_optional: Some(is_optional),
                        is_pinned: Some(is_pinned),
                        is_direct: Some(true),
                        resolved_package: None,
                        extra_data: None,
                    })
                })
                .collect()
        })
}

fn extract_lock_dependencies(json_content: &Value) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    let packages = json_content
        .get(FIELD_PACKAGES)
        .and_then(|value| value.as_array())
        .map(|packages| packages.as_slice())
        .unwrap_or(&[]);
    let packages_dev = json_content
        .get(FIELD_PACKAGES_DEV)
        .and_then(|value| value.as_array())
        .map(|packages| packages.as_slice())
        .unwrap_or(&[]);

    dependencies.reserve(packages.len() + packages_dev.len());
    dependencies.extend(extract_lock_package_list(packages, "require", true, false));
    dependencies.extend(extract_lock_package_list(
        packages_dev,
        "require-dev",
        false,
        true,
    ));

    dependencies
}

fn extract_lock_package_list(
    packages: &[Value],
    scope: &str,
    is_runtime: bool,
    is_optional: bool,
) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    for package in packages {
        if let Some(dependency) = build_lock_dependency(package, scope, is_runtime, is_optional) {
            dependencies.push(dependency);
        }
    }

    dependencies
}

fn build_lock_dependency(
    package: &Value,
    scope: &str,
    is_runtime: bool,
    is_optional: bool,
) -> Option<Dependency> {
    let name = package.get(FIELD_NAME).and_then(|value| value.as_str())?;
    let version = package
        .get(FIELD_VERSION)
        .and_then(|value| value.as_str())?;
    let package_type = package.get(FIELD_TYPE).and_then(|value| value.as_str());

    let (namespace, package_name) = split_namespace_name(name);
    let purl = build_dependency_purl(namespace.as_deref(), &package_name, Some(version));

    let source = package
        .get(FIELD_SOURCE)
        .and_then(|value| value.as_object());
    let dist = package.get(FIELD_DIST).and_then(|value| value.as_object());

    let (sha1, sha256, sha512, dist_shasum) = extract_dist_hashes(dist);
    let dist_url = dist
        .and_then(|map| map.get("url"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());

    let mut extra_data = HashMap::new();

    if let Some(package_type) = package_type {
        extra_data.insert("type".to_string(), Value::String(package_type.to_string()));
    }

    if let Some(source_map) = source {
        if let Some(source_reference) = source_map.get("reference").and_then(|value| value.as_str())
        {
            extra_data.insert(
                "source_reference".to_string(),
                Value::String(source_reference.to_string()),
            );
        }

        if let Some(source_url) = source_map.get("url").and_then(|value| value.as_str()) {
            extra_data.insert(
                "source_url".to_string(),
                Value::String(source_url.to_string()),
            );
        }

        if let Some(source_type) = source_map.get("type").and_then(|value| value.as_str()) {
            extra_data.insert(
                "source_type".to_string(),
                Value::String(source_type.to_string()),
            );
        }
    }

    if let Some(dist_map) = dist {
        if let Some(dist_reference) = dist_map.get("reference").and_then(|value| value.as_str()) {
            extra_data.insert(
                "dist_reference".to_string(),
                Value::String(dist_reference.to_string()),
            );
        }

        if let Some(dist_url) = dist_map.get("url").and_then(|value| value.as_str()) {
            extra_data.insert("dist_url".to_string(), Value::String(dist_url.to_string()));
        }

        if let Some(dist_type) = dist_map.get("type").and_then(|value| value.as_str()) {
            extra_data.insert(
                "dist_type".to_string(),
                Value::String(dist_type.to_string()),
            );
        }
    }

    if let Some(shasum) = dist_shasum {
        extra_data.insert("dist_shasum".to_string(), Value::String(shasum));
    }

    let extra_data = if extra_data.is_empty() {
        None
    } else {
        Some(extra_data)
    };

    let resolved_package = ResolvedPackage {
        primary_language: Some("PHP".to_string()),
        download_url: dist_url,
        sha1: sha1.and_then(|h| Sha1Digest::from_hex(&h).ok()),
        sha256: sha256.and_then(|h| Sha256Digest::from_hex(&h).ok()),
        sha512: sha512.and_then(|h| Sha512Digest::from_hex(&h).ok()),
        md5: None,
        is_virtual: true,
        extra_data: None,
        dependencies: Vec::new(),
        repository_homepage_url: None,
        repository_download_url: None,
        api_data_url: None,
        datasource_id: Some(DatasourceId::PhpComposerLock),
        purl: None,
        ..ResolvedPackage::new(
            ComposerLockParser::PACKAGE_TYPE,
            namespace.clone().unwrap_or_default(),
            package_name.clone(),
            version.to_string(),
        )
    };

    Some(Dependency {
        purl,
        extracted_requirement: None,
        scope: Some(scope.to_string()),
        is_runtime: Some(is_runtime),
        is_optional: Some(is_optional),
        is_pinned: Some(true),
        is_direct: Some(true),
        resolved_package: Some(Box::new(resolved_package)),
        extra_data,
    })
}

fn extract_dist_hashes(
    dist: Option<&serde_json::Map<String, Value>>,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    let mut sha1 = None;
    let mut sha256 = None;
    let mut sha512 = None;
    let mut raw_shasum = None;

    if let Some(dist) = dist {
        if let Some(shasum) = dist.get("shasum").and_then(|value| value.as_str()) {
            let trimmed = shasum.trim();
            if !trimmed.is_empty() {
                raw_shasum = Some(trimmed.to_string());
                let (parsed_sha1, parsed_sha256, parsed_sha512) = parse_hash_value(trimmed);
                sha1 = parsed_sha1;
                sha256 = parsed_sha256;
                sha512 = parsed_sha512;
            }
        }

        if let Some(value) = dist.get("sha1").and_then(|value| value.as_str())
            && is_hex_hash(value)
        {
            sha1 = Some(value.to_string());
        }
        if let Some(value) = dist.get("sha256").and_then(|value| value.as_str())
            && is_hex_hash(value)
        {
            sha256 = Some(value.to_string());
        }
        if let Some(value) = dist.get("sha512").and_then(|value| value.as_str())
            && is_hex_hash(value)
        {
            sha512 = Some(value.to_string());
        }
    }

    (sha1, sha256, sha512, raw_shasum)
}

fn parse_hash_value(hash: &str) -> (Option<String>, Option<String>, Option<String>) {
    let trimmed = hash.trim();
    if trimmed.is_empty() || !is_hex_hash(trimmed) {
        return (None, None, None);
    }

    match trimmed.len() {
        40 => (Some(trimmed.to_string()), None, None),
        64 => (None, Some(trimmed.to_string()), None),
        128 => (None, None, Some(trimmed.to_string())),
        _ => (None, None, None),
    }
}

fn is_hex_hash(value: &str) -> bool {
    value.chars().all(|c| c.is_ascii_hexdigit())
}

fn extract_license_statement(json_content: &Value) -> Option<String> {
    let mut licenses = Vec::new();

    if let Some(license_value) = json_content.get(FIELD_LICENSE) {
        match license_value {
            Value::String(value) => {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    licenses.push(trimmed.to_string());
                }
            }
            Value::Array(values) => {
                for value in values {
                    if let Some(license_str) = value.as_str() {
                        let trimmed = license_str.trim();
                        if !trimmed.is_empty() {
                            licenses.push(trimmed.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if licenses.is_empty() {
        return None;
    }

    if licenses.len() == 1 {
        Some(licenses[0].clone())
    } else {
        Some(licenses.join(" OR "))
    }
}

fn extract_license_data(
    json_content: &Value,
    is_private: bool,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Vec<LicenseDetection>,
) {
    let extracted_license_statement = extract_license_statement(json_content)
        .or_else(|| is_private.then(|| "proprietary-license".to_string()));
    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        normalize_composer_license_data(extracted_license_statement.as_deref());

    (
        extracted_license_statement,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
    )
}

fn normalize_composer_license_data(
    extracted_license_statement: Option<&str>,
) -> (Option<String>, Option<String>, Vec<LicenseDetection>) {
    let Some(extracted_license_statement) = extracted_license_statement
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return super::license_normalization::empty_declared_license_data();
    };

    if extracted_license_statement.eq_ignore_ascii_case("proprietary") {
        return build_declared_license_data_from_pair(
            "proprietary-license",
            "LicenseRef-scancode-proprietary-license",
            DeclaredLicenseMatchMetadata::single_line(extracted_license_statement),
        );
    }

    if extracted_license_statement.eq_ignore_ascii_case("proprietary-license") {
        return build_declared_license_data_from_pair(
            "proprietary-license",
            "LicenseRef-scancode-proprietary-license",
            DeclaredLicenseMatchMetadata::single_line(extracted_license_statement),
        );
    }

    normalize_spdx_declared_license(Some(extracted_license_statement))
}

fn extract_keywords(json_content: &Value) -> Vec<String> {
    json_content
        .get(FIELD_KEYWORDS)
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(|value| value.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn extract_parties(json_content: &Value, namespace: &Option<String>) -> Vec<Party> {
    let mut parties = Vec::new();

    if let Some(authors) = json_content
        .get(FIELD_AUTHORS)
        .and_then(|value| value.as_array())
    {
        for author in authors {
            if let Some(author) = author.as_object() {
                let name = author
                    .get("name")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string());
                let role = author
                    .get("role")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string())
                    .or(Some("author".to_string()));
                let email = author
                    .get("email")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string());
                let url = author
                    .get("homepage")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string());

                if name.is_some() || email.is_some() || url.is_some() {
                    parties.push(Party {
                        r#type: Some("person".to_string()),
                        role,
                        name,
                        email,
                        url,
                        organization: None,
                        organization_url: None,
                        timezone: None,
                    });
                }
            }
        }
    }

    if let Some(vendor) = namespace
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        parties.push(Party {
            r#type: Some("person".to_string()),
            role: Some("vendor".to_string()),
            name: Some(vendor.to_string()),
            email: None,
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }

    parties
}

fn extract_support(json_content: &Value) -> (Option<String>, Option<String>) {
    let support = json_content.get(FIELD_SUPPORT).and_then(|v| v.as_object());

    if let Some(support_obj) = support {
        let bug_tracking_url = support_obj
            .get("issues")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let code_view_url = support_obj
            .get("source")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        (bug_tracking_url, code_view_url)
    } else {
        (None, None)
    }
}

fn build_extra_data(json_content: &Value) -> Option<HashMap<String, Value>> {
    let mut extra_data = HashMap::new();

    if let Some(package_type) = json_content
        .get(FIELD_TYPE)
        .and_then(|value| value.as_str())
    {
        extra_data.insert("type".to_string(), Value::String(package_type.to_string()));
    }

    if let Some(autoload) = json_content
        .get(FIELD_AUTOLOAD)
        .and_then(|value| value.as_object())
        && let Some(psr4) = autoload.get(FIELD_PSR4)
    {
        extra_data.insert("autoload_psr4".to_string(), psr4.clone());
    }

    if let Some(repositories) = json_content.get(FIELD_REPOSITORIES) {
        extra_data.insert("repositories".to_string(), repositories.clone());
    }

    if extra_data.is_empty() {
        None
    } else {
        Some(extra_data)
    }
}

fn extract_source_vcs_url(json_content: &Value) -> Option<String> {
    let source = json_content.get(FIELD_SOURCE)?.as_object()?;
    let source_type = source.get("type")?.as_str()?.trim();
    let source_url = source.get("url")?.as_str()?.trim();
    let source_reference = source
        .get("reference")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());

    if source_type.is_empty() || source_url.is_empty() {
        return None;
    }

    Some(match source_reference {
        Some(reference) => format!("{}+{}@{}", source_type, source_url, reference),
        None => format!("{}+{}", source_type, source_url),
    })
}

fn extract_dist_download_url(json_content: &Value) -> Option<String> {
    json_content
        .get(FIELD_DIST)
        .and_then(|value| value.as_object())
        .and_then(|dist| dist.get("url"))
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn build_repository_homepage_url(
    namespace: &Option<String>,
    name: &Option<String>,
) -> Option<String> {
    match (
        namespace.as_ref().filter(|value| !value.is_empty()),
        name.as_ref(),
    ) {
        (Some(ns), Some(name)) => Some(format!("https://packagist.org/packages/{}/{}", ns, name)),
        (None, Some(name)) => Some(format!("https://packagist.org/packages/{}", name)),
        _ => None,
    }
}

fn build_api_data_url(namespace: &Option<String>, name: &Option<String>) -> Option<String> {
    match (namespace.as_ref(), name.as_ref()) {
        (Some(ns), Some(name)) if !ns.is_empty() => Some(format!(
            "https://packagist.org/p/packages/{}/{}.json",
            ns, name
        )),
        (None, Some(name)) => Some(format!("https://packagist.org/p/packages/{}.json", name)),
        (Some(_), Some(name)) => Some(format!("https://packagist.org/p/packages/{}.json", name)),
        _ => None,
    }
}

fn build_package_purl(
    namespace: &Option<String>,
    name: &Option<String>,
    version: &Option<String>,
) -> Option<String> {
    let name = name.as_ref()?;
    let mut package_url = match PackageUrl::new(ComposerJsonParser::PACKAGE_TYPE.as_str(), name) {
        Ok(purl) => purl,
        Err(e) => {
            warn!(
                "Failed to create PackageUrl for composer package '{}': {}",
                name, e
            );
            return None;
        }
    };

    if let Some(namespace) = namespace.as_ref().filter(|value| !value.is_empty())
        && let Err(e) = package_url.with_namespace(namespace)
    {
        warn!(
            "Failed to set namespace '{}' for composer package '{}': {}",
            namespace, name, e
        );
        return None;
    }

    if let Some(version) = version.as_ref()
        && let Err(e) = package_url.with_version(version)
    {
        warn!(
            "Failed to set version '{}' for composer package '{}': {}",
            version, name, e
        );
        return None;
    }

    Some(package_url.to_string())
}

fn build_dependency_purl(
    namespace: Option<&str>,
    name: &str,
    version: Option<&str>,
) -> Option<String> {
    let mut package_url = match PackageUrl::new(ComposerJsonParser::PACKAGE_TYPE.as_str(), name) {
        Ok(purl) => purl,
        Err(e) => {
            warn!(
                "Failed to create PackageUrl for composer package '{}': {}",
                name, e
            );
            return None;
        }
    };

    if let Some(namespace) = namespace.filter(|value| !value.is_empty())
        && let Err(e) = package_url.with_namespace(namespace)
    {
        warn!(
            "Failed to set namespace '{}' for composer package '{}': {}",
            namespace, name, e
        );
        return None;
    }

    if let Some(version) = version
        && let Err(e) = package_url.with_version(version)
    {
        warn!(
            "Failed to set version '{}' for composer package '{}': {}",
            version, name, e
        );
        return None;
    }

    Some(package_url.to_string())
}

fn split_optional_namespace_name(full_name: Option<&str>) -> (Option<String>, Option<String>) {
    match full_name {
        Some(full_name) => {
            let (namespace, name) = split_namespace_name(full_name);
            (namespace, Some(name))
        }
        None => (None, None),
    }
}

fn split_namespace_name(full_name: &str) -> (Option<String>, String) {
    let mut iter = full_name.splitn(2, '/');
    let first = iter.next().unwrap_or("");
    let second = iter.next();

    if let Some(name) = second {
        (Some(first.to_string()), name.to_string())
    } else {
        (None, first.to_string())
    }
}

fn normalize_requirement_version(requirement: &str) -> String {
    let trimmed = requirement.trim();
    trimmed.trim_start_matches('=').trim().to_string()
}

fn is_composer_version_pinned(version: &str) -> bool {
    let trimmed = version.trim();
    if trimmed.is_empty() {
        return false;
    }

    if trimmed.contains(" - ")
        || trimmed.contains('|')
        || trimmed.contains(',')
        || trimmed.contains('^')
        || trimmed.contains('~')
        || trimmed.contains('>')
        || trimmed.contains('<')
        || trimmed.contains('*')
    {
        return false;
    }

    let without_prefix = trimmed.trim_start_matches('=').trim();
    let without_prefix = without_prefix.strip_prefix('v').unwrap_or(without_prefix);
    if without_prefix.is_empty() {
        return false;
    }

    let lower = without_prefix.to_lowercase();
    if lower.contains("dev") {
        return false;
    }

    if without_prefix
        .chars()
        .any(|c| !c.is_ascii_digit() && c != '.' && c != '-' && c != '+')
    {
        return false;
    }

    without_prefix.matches('.').count() >= 2
}

fn default_package_data(datasource_id: Option<DatasourceId>) -> PackageData {
    PackageData {
        package_type: Some(ComposerJsonParser::PACKAGE_TYPE),
        primary_language: Some("PHP".to_string()),
        datasource_id,
        ..Default::default()
    }
}

crate::register_parser!(
    "PHP composer manifest",
    &["**/*composer.json", "**/composer.*.json"],
    "composer",
    "PHP",
    Some("https://getcomposer.org/doc/04-schema.md"),
);

crate::register_parser!(
    "PHP composer lockfile",
    &["**/*composer.lock", "**/composer.*.lock"],
    "composer",
    "PHP",
    Some("https://getcomposer.org/doc/01-basic-usage.md#composer-lock-the-lock-file"),
);
