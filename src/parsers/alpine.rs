//! Parser for Alpine Linux package metadata files.
//!
//! Extracts installed package metadata from Alpine Linux package database files
//! using the APK package manager format.
//!
//! # Supported Formats
//! - `/lib/apk/db/installed` (Installed package database)
//!
//! # Key Features
//! - Installed package metadata extraction from system database
//! - Dependency tracking from provides/requires fields
//! - Author and maintainer information extraction
//! - License information parsing
//! - Package URL (purl) generation
//!
//! # Implementation Notes
//! - Uses custom case-sensitive key-value parser (not the generic `rfc822` module)
//! - Database stored in text format with multi-paragraph records
//! - Graceful error handling with `warn!()` logs

use std::collections::HashMap;
use std::path::Path;

use crate::parser_warn as warn;
use crate::utils::magic;

use crate::models::{
    DatasourceId, Dependency, FileReference, LicenseDetection, PackageData, PackageType, Party,
    Sha1Digest,
};
use crate::parsers::utils::{read_file_to_string, split_name_email};

use super::PackageParser;
use super::license_normalization::{
    DeclaredLicenseMatchMetadata, NormalizedDeclaredLicense, build_declared_license_data,
    build_declared_license_data_from_pair, combine_normalized_licenses,
    empty_declared_license_data, normalize_declared_license_key,
};

const PACKAGE_TYPE: PackageType = PackageType::Alpine;

fn default_package_data(datasource_id: DatasourceId) -> PackageData {
    PackageData {
        package_type: Some(PACKAGE_TYPE),
        datasource_id: Some(datasource_id),
        ..Default::default()
    }
}

/// Parser for Alpine Linux installed package database
pub struct AlpineInstalledParser;

pub struct AlpineApkbuildParser;

impl PackageParser for AlpineInstalledParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.to_str()
            .map(|p| p.contains("/lib/apk/db/") && p.ends_with("installed"))
            .unwrap_or(false)
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read Alpine installed db {:?}: {}", path, e);
                return vec![default_package_data(DatasourceId::AlpineInstalledDb)];
            }
        };

        parse_alpine_installed_db(&content)
    }
}

impl PackageParser for AlpineApkbuildParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.file_name().and_then(|n| n.to_str()) == Some("APKBUILD")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read APKBUILD {:?}: {}", path, e);
                return vec![default_package_data(DatasourceId::AlpineApkbuild)];
            }
        };

        vec![parse_apkbuild(&content)]
    }
}

fn parse_alpine_installed_db(content: &str) -> Vec<PackageData> {
    let raw_paragraphs: Vec<&str> = content
        .split("\n\n")
        .filter(|p| !p.trim().is_empty())
        .collect();

    let mut all_packages = Vec::new();

    for raw_text in &raw_paragraphs {
        let headers = parse_alpine_headers(raw_text);
        let pkg = parse_alpine_package_paragraph(&headers, raw_text);
        if pkg.name.is_some() {
            all_packages.push(pkg);
        }
    }

    if all_packages.is_empty() {
        return vec![default_package_data(DatasourceId::AlpineInstalledDb)];
    }

    all_packages
}

/// Parse Alpine DB headers preserving case sensitivity.
///
/// Alpine's installed DB uses single-letter case-sensitive keys (e.g., `T:` for
/// description vs `t:` for timestamp, `C:` for checksum vs `c:` for git commit).
/// The generic rfc822 parser lowercases all keys, causing collisions.
fn parse_alpine_headers(content: &str) -> HashMap<String, Vec<String>> {
    let mut headers: HashMap<String, Vec<String>> = HashMap::new();

    for line in content.lines() {
        if line.is_empty() {
            continue;
        }

        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim();
            let value = value.trim();
            if !key.is_empty() && !value.is_empty() {
                headers
                    .entry(key.to_string())
                    .or_default()
                    .push(value.to_string());
            }
        }
    }

    headers
}

fn get_first(headers: &HashMap<String, Vec<String>>, key: &str) -> Option<String> {
    headers
        .get(key)
        .and_then(|values| values.first())
        .map(|v| v.trim().to_string())
}

fn get_all(headers: &HashMap<String, Vec<String>>, key: &str) -> Vec<String> {
    headers
        .get(key)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|v| !v.trim().is_empty())
        .collect()
}

fn parse_alpine_package_paragraph(
    headers: &HashMap<String, Vec<String>>,
    raw_text: &str,
) -> PackageData {
    let name = get_first(headers, "P");
    let version = get_first(headers, "V");
    let description = get_first(headers, "T");
    let homepage_url = get_first(headers, "U");
    let architecture = get_first(headers, "A");

    let is_virtual = description
        .as_ref()
        .is_some_and(|d| d == "virtual meta package");

    let namespace = Some("alpine".to_string());
    let mut parties = Vec::new();

    if let Some(maintainer) = get_first(headers, "m") {
        let (name_opt, email_opt) = split_name_email(&maintainer);
        parties.push(Party {
            r#type: None,
            role: Some("maintainer".to_string()),
            name: name_opt,
            email: email_opt,
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }

    let extracted_license_statement = get_first(headers, "L");
    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        build_alpine_license_data(extracted_license_statement.as_deref());

    let source_packages = if let Some(origin) = get_first(headers, "o") {
        vec![format!("pkg:alpine/{}", origin)]
    } else {
        Vec::new()
    };
    let vcs_url = get_first(headers, "c")
        .map(|commit| format!("git+https://git.alpinelinux.org/aports/commit/?id={commit}"));

    let mut dependencies = Vec::new();
    for dep in get_all(headers, "D") {
        for dep_str in dep.split_whitespace() {
            if dep_str.starts_with("so:") || dep_str.starts_with("cmd:") {
                continue;
            }

            dependencies.push(Dependency {
                purl: Some(format!("pkg:alpine/{}", dep_str)),
                extracted_requirement: None,
                scope: Some("install".to_string()),
                is_runtime: Some(true),
                is_optional: Some(false),
                is_direct: Some(true),
                resolved_package: None,
                extra_data: None,
                is_pinned: Some(false),
            });
        }
    }

    let mut extra_data = HashMap::new();

    if is_virtual {
        extra_data.insert("is_virtual".to_string(), true.into());
    }

    if let Some(checksum) = get_first(headers, "C") {
        extra_data.insert("checksum".to_string(), checksum.into());
    }

    if let Some(size) = get_first(headers, "S") {
        extra_data.insert("compressed_size".to_string(), size.into());
    }

    if let Some(installed_size) = get_first(headers, "I") {
        extra_data.insert("installed_size".to_string(), installed_size.into());
    }

    if let Some(timestamp) = get_first(headers, "t") {
        extra_data.insert("build_timestamp".to_string(), timestamp.into());
    }

    if let Some(commit) = get_first(headers, "c") {
        extra_data.insert("git_commit".to_string(), commit.into());
    }

    let providers = extract_providers(raw_text);
    if !providers.is_empty() {
        let provider_list: Vec<serde_json::Value> =
            providers.into_iter().map(|s| s.into()).collect();
        extra_data.insert("providers".to_string(), provider_list.into());
    }

    let file_references = extract_file_references(raw_text);

    PackageData {
        datasource_id: Some(DatasourceId::AlpineInstalledDb),
        package_type: Some(PACKAGE_TYPE),
        namespace: namespace.clone(),
        name: name.clone(),
        version: version.clone(),
        description,
        homepage_url,
        vcs_url,
        parties,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        extracted_license_statement,
        source_packages,
        dependencies,
        file_references,
        purl: name
            .as_ref()
            .and_then(|n| build_alpine_purl(n, version.as_deref(), architecture.as_deref())),
        extra_data: if extra_data.is_empty() {
            None
        } else {
            Some(extra_data)
        },
        ..Default::default()
    }
}

fn parse_apkbuild(content: &str) -> PackageData {
    let variables = parse_apkbuild_variables(content);

    let name = variables.get("pkgname").cloned();
    let version = match (variables.get("pkgver"), variables.get("pkgrel")) {
        (Some(ver), Some(rel)) => Some(format!("{}-r{}", ver, rel)),
        (Some(ver), None) => Some(ver.clone()),
        _ => None,
    };
    let description = variables.get("pkgdesc").cloned();
    let homepage_url = variables.get("url").cloned();
    let extracted_license_statement = variables.get("license").cloned();
    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        build_alpine_license_data(extracted_license_statement.as_deref());

    let dependencies = parse_apkbuild_dependencies(&variables);

    let mut extra_data = HashMap::new();
    if let Some(source) = variables.get("source") {
        let sources_value: Vec<serde_json::Value> = parse_apkbuild_sources(source)
            .into_iter()
            .map(|(file_name, url)| serde_json::json!({ "file_name": file_name, "url": url }))
            .collect();
        if !sources_value.is_empty() {
            extra_data.insert(
                "sources".to_string(),
                serde_json::Value::Array(sources_value),
            );
        }
    }
    for (field, checksum_key) in [
        ("sha512sums", "sha512"),
        ("sha256sums", "sha256"),
        ("md5sums", "md5"),
    ] {
        if let Some(checksums) = variables.get(field) {
            let checksum_entries: Vec<serde_json::Value> = parse_apkbuild_checksums(checksums)
                .into_iter()
                .map(|(file_name, checksum)| serde_json::json!({ "file_name": file_name, checksum_key: checksum }))
                .collect();
            if !checksum_entries.is_empty() {
                match extra_data.get_mut("checksums") {
                    Some(serde_json::Value::Array(existing)) => existing.extend(checksum_entries),
                    _ => {
                        extra_data.insert(
                            "checksums".to_string(),
                            serde_json::Value::Array(checksum_entries),
                        );
                    }
                }
            }
        }
    }

    PackageData {
        datasource_id: Some(DatasourceId::AlpineApkbuild),
        package_type: Some(PACKAGE_TYPE),
        namespace: None,
        name: name.clone(),
        version: version.clone(),
        description,
        homepage_url,
        extracted_license_statement,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        dependencies,
        purl: name
            .as_deref()
            .and_then(|n| build_alpine_purl(n, version.as_deref(), None)),
        extra_data: (!extra_data.is_empty()).then_some(extra_data),
        ..default_package_data(DatasourceId::AlpineApkbuild)
    }
}

fn parse_apkbuild_variables(content: &str) -> HashMap<String, String> {
    let mut raw = HashMap::new();
    let mut lines = content.lines().peekable();
    let mut brace_depth = 0usize;

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.ends_with("(){") || trimmed.ends_with("() {") {
            brace_depth += 1;
            continue;
        }
        if brace_depth > 0 {
            brace_depth += trimmed.chars().filter(|c| *c == '{').count();
            brace_depth = brace_depth.saturating_sub(trimmed.chars().filter(|c| *c == '}').count());
            continue;
        }
        let Some((name, value)) = trimmed.split_once('=') else {
            continue;
        };
        let mut value = value.trim().to_string();
        if value.starts_with('"') && !value.ends_with('"') {
            while let Some(next) = lines.peek() {
                value.push('\n');
                value.push_str(next);
                let current = lines.next().unwrap();
                if current.trim_end().ends_with('"') {
                    break;
                }
            }
        }
        raw.insert(name.trim().to_string(), value);
    }

    let mut resolved = HashMap::new();
    for key in [
        "pkgname",
        "pkgver",
        "pkgrel",
        "pkgdesc",
        "url",
        "license",
        "source",
        "depends",
        "depends_dev",
        "makedepends",
        "makedepends_build",
        "makedepends_host",
        "checkdepends",
        "sha512sums",
        "sha256sums",
        "md5sums",
    ] {
        if let Some(value) = raw.get(key) {
            resolved.insert(key.to_string(), resolve_apkbuild_value(value, &raw));
        }
    }
    resolved
}

fn resolve_apkbuild_value(value: &str, variables: &HashMap<String, String>) -> String {
    let mut resolved = strip_wrapping_quotes(value.trim()).to_string();
    for _ in 0..8 {
        let previous = resolved.clone();
        for (name, raw_value) in variables {
            let raw_value = strip_wrapping_quotes(raw_value.trim());
            let resolved_raw = resolve_apkbuild_value_no_recursion(raw_value, variables);
            let value_resolved = strip_wrapping_quotes(&resolved_raw);
            resolved = resolved.replace(
                &format!("${{{name}//./-}}"),
                &value_resolved.replace('.', "-"),
            );
            resolved = resolved.replace(
                &format!("${{{name}//./_}}"),
                &value_resolved.replace('.', "_"),
            );
            resolved = resolved.replace(
                &format!("${{{name}::8}}"),
                &value_resolved.chars().take(8).collect::<String>(),
            );
            resolved = resolved.replace(&format!("${{{name}}}"), value_resolved);
            resolved = resolved.replace(&format!("${name}"), value_resolved);
        }
        if resolved == previous {
            break;
        }
    }
    resolved
}

fn resolve_apkbuild_value_no_recursion(value: &str, variables: &HashMap<String, String>) -> String {
    let mut resolved = strip_wrapping_quotes(value.trim()).to_string();
    for (name, raw_value) in variables {
        let raw_value = strip_wrapping_quotes(raw_value.trim());
        resolved = resolved.replace(&format!("${{{name}//./-}}"), &raw_value.replace('.', "-"));
        resolved = resolved.replace(&format!("${{{name}//./_}}"), &raw_value.replace('.', "_"));
        resolved = resolved.replace(
            &format!("${{{name}::8}}"),
            &raw_value.chars().take(8).collect::<String>(),
        );
        resolved = resolved.replace(&format!("${{{name}}}"), raw_value);
        resolved = resolved.replace(&format!("${name}"), raw_value);
    }
    resolved
}

fn strip_wrapping_quotes(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
        .unwrap_or(value)
}

fn parse_apkbuild_sources(value: &str) -> Vec<(Option<String>, Option<String>)> {
    value
        .split_whitespace()
        .filter(|part| !part.is_empty())
        .map(|part| {
            if let Some((file_name, url)) = part.split_once("::") {
                (Some(file_name.to_string()), Some(url.to_string()))
            } else if part.contains("://") {
                (None, Some(part.to_string()))
            } else {
                (Some(part.to_string()), None)
            }
        })
        .collect()
}

fn parse_apkbuild_checksums(value: &str) -> Vec<(String, String)> {
    value
        .lines()
        .flat_map(|line| line.split_whitespace())
        .collect::<Vec<_>>()
        .chunks(2)
        .filter_map(|chunk| {
            if chunk.len() == 2 {
                Some((chunk[1].to_string(), chunk[0].to_string()))
            } else {
                None
            }
        })
        .collect()
}

fn build_alpine_license_data(
    extracted: Option<&str>,
) -> (Option<String>, Option<String>, Vec<LicenseDetection>) {
    let Some(extracted) = extracted.map(str::trim).filter(|s| !s.is_empty()) else {
        return empty_declared_license_data();
    };

    if extracted == "custom:multiple" {
        return build_declared_license_data_from_pair(
            "unknown-license-reference",
            "LicenseRef-provenant-unknown-license-reference",
            DeclaredLicenseMatchMetadata::single_line(extracted),
        );
    }

    let normalized_tokens = extracted
        .split_whitespace()
        .filter(|part| *part != "AND")
        .map(normalize_alpine_license_token)
        .collect::<Option<Vec<_>>>();

    let Some(normalized_tokens) = normalized_tokens else {
        return empty_declared_license_data();
    };

    let Some(combined) = combine_normalized_licenses(normalized_tokens, " AND ") else {
        return empty_declared_license_data();
    };

    build_declared_license_data(
        combined,
        DeclaredLicenseMatchMetadata::single_line(extracted),
    )
}

fn normalize_alpine_license_token(token: &str) -> Option<NormalizedDeclaredLicense> {
    match token {
        "ICU" => Some(NormalizedDeclaredLicense::new("x11", "ICU")),
        "Unicode-TOU" => Some(NormalizedDeclaredLicense::new("unicode-tou", "Unicode-TOU")),
        "Ruby" => Some(NormalizedDeclaredLicense::new("ruby", "Ruby")),
        "BSD-2-Clause" => Some(NormalizedDeclaredLicense::new(
            "bsd-simplified",
            "BSD-2-Clause",
        )),
        "BSD-3-Clause" => Some(NormalizedDeclaredLicense::new("bsd-new", "BSD-3-Clause")),
        other => normalize_declared_license_key(other),
    }
}

fn parse_apkbuild_dependencies(variables: &HashMap<String, String>) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    for (field, scope, is_runtime, is_optional) in [
        ("depends", "depends", true, false),
        ("depends_dev", "depends_dev", false, true),
        ("makedepends", "makedepends", false, true),
        ("makedepends_build", "makedepends_build", false, true),
        ("makedepends_host", "makedepends_host", false, true),
        ("checkdepends", "checkdepends", false, true),
    ] {
        let Some(value) = variables.get(field) else {
            continue;
        };

        for dep_str in value.split_whitespace() {
            let dep_str = dep_str.trim();
            if dep_str.is_empty() {
                continue;
            }

            let dep_name = dep_str
                .split(['<', '>', '=', '!', '~'])
                .next()
                .unwrap_or(dep_str)
                .trim();
            if dep_name.is_empty() {
                continue;
            }

            dependencies.push(Dependency {
                purl: build_alpine_purl(dep_name, None, None),
                extracted_requirement: Some(dep_str.to_string()),
                scope: Some(scope.to_string()),
                is_runtime: Some(is_runtime),
                is_optional: Some(is_optional),
                is_pinned: Some(dep_str.contains('=')),
                is_direct: Some(true),
                resolved_package: None,
                extra_data: None,
            });
        }
    }

    dependencies
}

fn extract_file_references(raw_text: &str) -> Vec<FileReference> {
    let mut file_references = Vec::new();
    let mut current_dir = String::new();
    let mut current_file: Option<FileReference> = None;

    for line in raw_text.lines() {
        if line.is_empty() {
            continue;
        }

        if let Some((field_type, value)) = line.split_once(':') {
            let value = value.trim();
            match field_type {
                "F" => {
                    if let Some(file) = current_file.take() {
                        file_references.push(file);
                    }
                    current_dir = value.to_string();
                }
                "R" => {
                    if let Some(file) = current_file.take() {
                        file_references.push(file);
                    }

                    let path = if current_dir.is_empty() {
                        value.to_string()
                    } else {
                        format!("{}/{}", current_dir, value)
                    };

                    current_file = Some(FileReference {
                        path,
                        size: None,
                        sha1: None,
                        md5: None,
                        sha256: None,
                        sha512: None,
                        extra_data: None,
                    });
                }
                "Z" => {
                    if let Some(ref mut file) = current_file
                        && value.starts_with("Q1")
                    {
                        use base64::Engine;
                        if let Ok(decoded) =
                            base64::engine::general_purpose::STANDARD.decode(&value[2..])
                            && let Ok(digest) = Sha1Digest::from_hex(
                                &decoded
                                    .iter()
                                    .map(|b| format!("{:02x}", b))
                                    .collect::<String>(),
                            )
                        {
                            file.sha1 = Some(digest);
                        }
                    }
                }
                "a" => {
                    if let Some(ref mut file) = current_file {
                        let mut extra = HashMap::new();
                        extra.insert(
                            "attributes".to_string(),
                            serde_json::Value::String(value.to_string()),
                        );
                        file.extra_data = Some(extra);
                    }
                }
                _ => {}
            }
        }
    }

    if let Some(file) = current_file {
        file_references.push(file);
    }

    file_references
}

fn extract_providers(raw_text: &str) -> Vec<String> {
    let mut providers = Vec::new();

    for line in raw_text.lines() {
        if line.is_empty() {
            continue;
        }

        if let Some(value) = line.strip_prefix("p:") {
            providers.extend(value.split_whitespace().map(|s| s.to_string()));
        }
    }

    providers
}

fn build_alpine_purl(
    name: &str,
    version: Option<&str>,
    architecture: Option<&str>,
) -> Option<String> {
    use packageurl::PackageUrl;

    let mut purl = PackageUrl::new(PACKAGE_TYPE.as_str(), name).ok()?;

    if let Some(ver) = version {
        purl.with_version(ver).ok()?;
    }

    if let Some(arch) = architecture {
        purl.add_qualifier("arch", arch).ok()?;
    }

    Some(purl.to_string())
}

/// Parser for Alpine Linux .apk package archives
pub struct AlpineApkParser;

impl PackageParser for AlpineApkParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.extension().and_then(|e| e.to_str()) == Some("apk")
            && magic::is_gzip(path)
            && !magic::is_zip(path)
            && apk_contains_pkginfo(path)
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        vec![match extract_apk_archive(path) {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to extract .apk archive {:?}: {}", path, e);
                PackageData {
                    package_type: Some(PACKAGE_TYPE),
                    datasource_id: Some(DatasourceId::AlpineApkArchive),
                    ..Default::default()
                }
            }
        }]
    }
}

fn apk_contains_pkginfo(path: &Path) -> bool {
    use flate2::read::GzDecoder;

    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };

    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let entries = match archive.entries() {
        Ok(entries) => entries,
        Err(_) => return false,
    };

    for entry_result in entries {
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(_) => return false,
        };
        let entry_path = match entry.path() {
            Ok(path) => path,
            Err(_) => return false,
        };

        if entry_path.ends_with(".PKGINFO") {
            return true;
        }
    }

    false
}

fn extract_apk_archive(path: &Path) -> Result<PackageData, String> {
    use flate2::read::GzDecoder;
    use std::io::Read;

    let file = std::fs::File::open(path).map_err(|e| format!("Failed to open .apk file: {}", e))?;

    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);

    for entry_result in archive
        .entries()
        .map_err(|e| format!("Failed to read tar entries: {}", e))?
    {
        let mut entry = entry_result.map_err(|e| format!("Failed to read tar entry: {}", e))?;

        let entry_path = entry
            .path()
            .map_err(|e| format!("Failed to get entry path: {}", e))?;

        if entry_path.ends_with(".PKGINFO") {
            let mut content = String::new();
            entry
                .read_to_string(&mut content)
                .map_err(|e| format!("Failed to read .PKGINFO: {}", e))?;

            return Ok(parse_pkginfo(&content));
        }
    }

    Err(".apk archive does not contain .PKGINFO file".to_string())
}

fn parse_pkginfo(content: &str) -> PackageData {
    let mut fields: HashMap<&str, Vec<&str>> = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = line.split_once(" = ") {
            fields.entry(key.trim()).or_default().push(value.trim());
        }
    }

    let name = fields
        .get("pkgname")
        .and_then(|v| v.first())
        .map(|s| s.to_string());
    let pkgver = fields.get("pkgver").and_then(|v| v.first());
    let version = pkgver.map(|s| s.to_string());
    let arch = fields
        .get("arch")
        .and_then(|v| v.first())
        .map(|s| s.to_string());
    let license = fields
        .get("license")
        .and_then(|v| v.first())
        .map(|s| s.to_string());
    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        build_alpine_license_data(license.as_deref());
    let description = fields
        .get("pkgdesc")
        .and_then(|v| v.first())
        .map(|s| s.to_string());
    let homepage = fields
        .get("url")
        .and_then(|v| v.first())
        .map(|s| s.to_string());
    let origin = fields
        .get("origin")
        .and_then(|v| v.first())
        .map(|s| s.to_string());
    let maintainer_str = fields.get("maintainer").and_then(|v| v.first());

    let mut parties = Vec::new();
    if let Some(maint) = maintainer_str {
        let (maint_name, maint_email) = split_name_email(maint);
        parties.push(Party {
            r#type: Some("person".to_string()),
            role: Some("maintainer".to_string()),
            name: maint_name,
            email: maint_email,
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }

    let purl = name
        .as_ref()
        .and_then(|n| build_alpine_purl(n, version.as_deref(), arch.as_deref()));

    let mut dependencies = Vec::new();
    if let Some(depends_list) = fields.get("depend") {
        for dep_str in depends_list {
            let dep_name = dep_str.split_whitespace().next().unwrap_or(dep_str);
            dependencies.push(Dependency {
                purl: Some(format!("pkg:alpine/{}", dep_name)),
                extracted_requirement: Some(dep_str.to_string()),
                scope: Some("runtime".to_string()),
                is_runtime: Some(true),
                is_optional: Some(false),
                is_pinned: None,
                is_direct: Some(true),
                resolved_package: None,
                extra_data: None,
            });
        }
    }

    PackageData {
        datasource_id: Some(DatasourceId::AlpineApkArchive),
        package_type: Some(PACKAGE_TYPE),
        namespace: Some("alpine".to_string()),
        name,
        version,
        description,
        homepage_url: homepage,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        extracted_license_statement: license,
        parties,
        dependencies,
        purl,
        extra_data: origin.map(|o| {
            let mut map = HashMap::new();
            map.insert("origin".to_string(), serde_json::Value::String(o));
            map
        }),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Creates a temp file mimicking the Alpine installed db path structure.
    /// Returns the TempDir (must be kept alive) and path to the file.
    fn create_temp_installed_db(content: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db_dir = temp_dir.path().join("lib/apk/db");
        std::fs::create_dir_all(&db_dir).expect("Failed to create db dir");
        let file_path = db_dir.join("installed");
        let mut file = std::fs::File::create(&file_path).expect("Failed to create file");
        file.write_all(content.as_bytes())
            .expect("Failed to write content");
        (temp_dir, file_path)
    }

    #[test]
    fn test_alpine_parser_is_match() {
        assert!(AlpineInstalledParser::is_match(&PathBuf::from(
            "/lib/apk/db/installed"
        )));
        assert!(AlpineInstalledParser::is_match(&PathBuf::from(
            "/var/lib/apk/db/installed"
        )));
        assert!(!AlpineInstalledParser::is_match(&PathBuf::from(
            "/lib/apk/db/status"
        )));
        assert!(!AlpineInstalledParser::is_match(&PathBuf::from(
            "installed"
        )));
    }

    #[test]
    fn test_parse_alpine_package_basic() {
        let content = "C:Q1v4QhLje3kWlC8DJj+ZfJTjlJRSU=
P:alpine-baselayout-data
V:3.2.0-r22
A:x86_64
S:11435
I:73728
T:Alpine base dir structure and init scripts
U:https://git.alpinelinux.org/cgit/aports/tree/main/alpine-baselayout
L:GPL-2.0-only
o:alpine-baselayout
m:Natanael Copa <ncopa@alpinelinux.org>
t:1655134784
c:cb70ca5c6d6db0399d2dd09189c5d57827bce5cd

";
        let (_dir, path) = create_temp_installed_db(content);
        let pkg = AlpineInstalledParser::extract_first_package(&path);
        assert_eq!(pkg.name, Some("alpine-baselayout-data".to_string()));
        assert_eq!(pkg.version, Some("3.2.0-r22".to_string()));
        assert_eq!(pkg.namespace, Some("alpine".to_string()));
        assert_eq!(
            pkg.description,
            Some("Alpine base dir structure and init scripts".to_string())
        );
        assert_eq!(
            pkg.homepage_url,
            Some("https://git.alpinelinux.org/cgit/aports/tree/main/alpine-baselayout".to_string())
        );
        assert_eq!(
            pkg.extracted_license_statement,
            Some("GPL-2.0-only".to_string())
        );
        assert_eq!(pkg.parties.len(), 1);
        assert_eq!(pkg.parties[0].name, Some("Natanael Copa".to_string()));
        assert_eq!(
            pkg.parties[0].email,
            Some("ncopa@alpinelinux.org".to_string())
        );
        assert!(
            pkg.purl
                .as_ref()
                .unwrap()
                .contains("alpine-baselayout-data")
        );
        assert!(pkg.purl.as_ref().unwrap().contains("arch=x86_64"));
    }

    #[test]
    fn test_parse_alpine_with_dependencies() {
        let content = "P:musl
V:1.2.3-r0
A:x86_64
D:scanelf so:libc.musl-x86_64.so.1

";
        let (_dir, path) = create_temp_installed_db(content);
        let pkg = AlpineInstalledParser::extract_first_package(&path);
        assert_eq!(pkg.name, Some("musl".to_string()));
        assert_eq!(pkg.dependencies.len(), 1);
        assert!(
            pkg.dependencies[0]
                .purl
                .as_ref()
                .unwrap()
                .contains("scanelf")
        );
    }

    #[test]
    fn test_build_alpine_purl() {
        let purl = build_alpine_purl("busybox", Some("1.31.1-r9"), Some("x86_64"));
        assert_eq!(
            purl,
            Some("pkg:alpine/busybox@1.31.1-r9?arch=x86_64".to_string())
        );

        let purl_no_arch = build_alpine_purl("package", Some("1.0"), None);
        assert_eq!(purl_no_arch, Some("pkg:alpine/package@1.0".to_string()));
    }

    #[test]
    fn test_parse_alpine_extra_data() {
        let content = "P:test-package
V:1.0
C:base64checksum==
S:12345
I:67890
t:1234567890
c:gitcommithash

";
        let (_dir, path) = create_temp_installed_db(content);
        let pkg = AlpineInstalledParser::extract_first_package(&path);
        assert!(pkg.extra_data.is_some());
        let extra = pkg.extra_data.as_ref().unwrap();
        assert_eq!(extra["checksum"], "base64checksum==");
        assert_eq!(extra["compressed_size"], "12345");
        assert_eq!(extra["installed_size"], "67890");
        assert_eq!(extra["build_timestamp"], "1234567890");
        assert_eq!(extra["git_commit"], "gitcommithash");
    }

    #[test]
    fn test_parse_alpine_case_sensitive_keys() {
        let content = "C:Q1v4QhLje3kWlC8DJj+ZfJTjlJRSU=
P:test-pkg
V:1.0
T:A test description
t:1655134784
c:cb70ca5c6d6db0399d2dd09189c5d57827bce5cd

";
        let (_dir, path) = create_temp_installed_db(content);
        let pkg = AlpineInstalledParser::extract_first_package(&path);
        assert_eq!(pkg.description, Some("A test description".to_string()));
        let extra = pkg.extra_data.as_ref().unwrap();
        assert_eq!(extra["checksum"], "Q1v4QhLje3kWlC8DJj+ZfJTjlJRSU=");
        assert_eq!(extra["build_timestamp"], "1655134784");
        assert_eq!(
            extra["git_commit"],
            "cb70ca5c6d6db0399d2dd09189c5d57827bce5cd"
        );
    }

    #[test]
    fn test_parse_alpine_multiple_packages() {
        let content = "P:package1
V:1.0
A:x86_64

P:package2
V:2.0
A:aarch64

";
        let (_dir, path) = create_temp_installed_db(content);
        let pkgs = AlpineInstalledParser::extract_packages(&path);
        assert_eq!(pkgs.len(), 2);
        assert_eq!(pkgs[0].name, Some("package1".to_string()));
        assert_eq!(pkgs[0].version, Some("1.0".to_string()));
        assert_eq!(pkgs[1].name, Some("package2".to_string()));
        assert_eq!(pkgs[1].version, Some("2.0".to_string()));
    }

    #[test]
    fn test_parse_alpine_file_references() {
        let content = "P:test-pkg
V:1.0
F:usr/bin
R:test
Z:Q1WTc55xfvPogzA0YUV24D0Ym+MKE=
F:etc
R:config
Z:Q1pcfTfDNEbNKQc2s1tia7da05M8Q=

";
        let (_dir, path) = create_temp_installed_db(content);
        let pkg = AlpineInstalledParser::extract_first_package(&path);
        assert_eq!(pkg.file_references.len(), 2);
        assert_eq!(pkg.file_references[0].path, "usr/bin/test");
        assert!(pkg.file_references[0].sha1.is_some());
        assert_eq!(pkg.file_references[1].path, "etc/config");
        assert!(pkg.file_references[1].sha1.is_some());
    }

    #[test]
    fn test_parse_alpine_empty_fields() {
        let content = "P:minimal-package
V:1.0

";
        let (_dir, path) = create_temp_installed_db(content);
        let pkg = AlpineInstalledParser::extract_first_package(&path);
        assert_eq!(pkg.name, Some("minimal-package".to_string()));
        assert_eq!(pkg.version, Some("1.0".to_string()));
        assert!(pkg.description.is_none());
        assert!(pkg.homepage_url.is_none());
        assert_eq!(pkg.dependencies.len(), 0);
    }

    #[test]
    fn test_parse_alpine_origin_field() {
        let content = "P:busybox-ifupdown
V:1.35.0-r13
o:busybox
A:x86_64

";
        let (_dir, path) = create_temp_installed_db(content);
        let pkg = AlpineInstalledParser::extract_first_package(&path);
        assert_eq!(pkg.name, Some("busybox-ifupdown".to_string()));
        assert_eq!(pkg.source_packages.len(), 1);
        assert_eq!(pkg.source_packages[0], "pkg:alpine/busybox");
    }

    #[test]
    fn test_parse_alpine_url_field() {
        let content = "P:openssl
V:1.1.1q-r0
U:https://www.openssl.org
A:x86_64

";
        let (_dir, path) = create_temp_installed_db(content);
        let pkg = AlpineInstalledParser::extract_first_package(&path);
        assert_eq!(
            pkg.homepage_url,
            Some("https://www.openssl.org".to_string())
        );
    }

    #[test]
    fn test_parse_alpine_provider_field() {
        let content = "P:some-package
V:1.0
p:cmd:binary=1.0
p:so:libtest.so.1

";
        let (_dir, path) = create_temp_installed_db(content);
        let pkg = AlpineInstalledParser::extract_first_package(&path);
        assert!(pkg.extra_data.is_some());
        let extra = pkg.extra_data.as_ref().unwrap();
        let providers = extra.get("providers").and_then(|v| v.as_array());
        assert!(providers.is_some());
        let provider_array = providers.unwrap();
        assert_eq!(provider_array.len(), 2);
        assert_eq!(provider_array[0].as_str(), Some("cmd:binary=1.0"));
        assert_eq!(provider_array[1].as_str(), Some("so:libtest.so.1"));
    }

    #[test]
    fn test_alpine_apk_parser_is_match() {
        let apk_path = PathBuf::from("testdata/alpine/apk/basic/test-package-1.0-r0.apk");

        assert!(AlpineApkParser::is_match(&apk_path));
        assert!(!AlpineApkParser::is_match(&PathBuf::from("package.tar.gz")));
        assert!(!AlpineApkParser::is_match(&PathBuf::from("installed")));
    }

    #[test]
    fn test_alpine_apk_parser_rejects_android_and_placeholder_apk_fixtures() {
        let android_apk = PathBuf::from("testdata/misc/test_android.apk");
        let placeholder_alpine_apk = PathBuf::from("testdata/misc/test_alpine.apk");
        let valid_alpine_apk = PathBuf::from("testdata/alpine/apk/basic/test-package-1.0-r0.apk");

        assert!(!AlpineApkParser::is_match(&android_apk));
        assert!(!AlpineApkParser::is_match(&placeholder_alpine_apk));
        assert!(AlpineApkParser::is_match(&valid_alpine_apk));
    }

    #[test]
    fn test_alpine_apkbuild_parser_is_match() {
        assert!(AlpineApkbuildParser::is_match(&PathBuf::from("APKBUILD")));
        assert!(AlpineApkbuildParser::is_match(&PathBuf::from(
            "/path/to/APKBUILD"
        )));
        assert!(!AlpineApkbuildParser::is_match(&PathBuf::from("apkbuild")));
        assert!(!AlpineApkbuildParser::is_match(&PathBuf::from(
            "APKBUILD.txt"
        )));
    }

    #[test]
    fn test_parse_apkbuild_icu_reference() {
        let path = PathBuf::from("testdata/alpine-fixtures/apkbuild/alpine14/main/icu/APKBUILD");
        let pkg = AlpineApkbuildParser::extract_first_package(&path);

        assert_eq!(pkg.datasource_id, Some(DatasourceId::AlpineApkbuild));
        assert_eq!(pkg.name.as_deref(), Some("icu"));
        assert_eq!(pkg.version.as_deref(), Some("67.1-r2"));
        assert_eq!(
            pkg.description.as_deref(),
            Some("International Components for Unicode library")
        );
        assert_eq!(
            pkg.homepage_url.as_deref(),
            Some("http://site.icu-project.org/")
        );
        assert_eq!(
            pkg.extracted_license_statement.as_deref(),
            Some("MIT ICU Unicode-TOU")
        );
        assert_eq!(
            pkg.declared_license_expression_spdx.as_deref(),
            Some("MIT AND ICU AND Unicode-TOU")
        );
        assert_eq!(pkg.dependencies.len(), 3);
        let depends_dev = pkg
            .dependencies
            .iter()
            .find(|dep| dep.scope.as_deref() == Some("depends_dev"))
            .expect("depends_dev dependency missing");
        assert_eq!(depends_dev.purl.as_deref(), Some("pkg:alpine/icu"));
        assert_eq!(depends_dev.is_runtime, Some(false));
        assert_eq!(depends_dev.is_optional, Some(true));

        let check_dep_names: Vec<_> = pkg
            .dependencies
            .iter()
            .filter(|dep| dep.scope.as_deref() == Some("checkdepends"))
            .filter_map(|dep| dep.purl.as_deref())
            .collect();
        assert!(check_dep_names.contains(&"pkg:alpine/diffutils"));
        assert!(check_dep_names.contains(&"pkg:alpine/python3"));
        let extra = pkg.extra_data.as_ref().unwrap();
        assert!(extra.contains_key("sources"));
        assert!(extra.contains_key("checksums"));
    }

    #[test]
    fn test_parse_apkbuild_custom_multiple_license_uses_raw_matched_text() {
        let path = PathBuf::from(
            "testdata/alpine-fixtures/apkbuild/alpine13/main/linux-firmware/APKBUILD",
        );
        let pkg = AlpineApkbuildParser::extract_first_package(&path);

        assert_eq!(pkg.name.as_deref(), Some("linux-firmware"));
        assert_eq!(pkg.version.as_deref(), Some("20201218-r0"));
        assert_eq!(
            pkg.extracted_license_statement.as_deref(),
            Some("custom:multiple")
        );
        assert_eq!(
            pkg.declared_license_expression.as_deref(),
            Some("unknown-license-reference")
        );
        assert_eq!(
            pkg.declared_license_expression_spdx.as_deref(),
            Some("LicenseRef-provenant-unknown-license-reference")
        );
        let matched = pkg.license_detections[0].matches[0].matched_text.as_deref();
        assert_eq!(matched, Some("custom:multiple"));
    }

    #[test]
    fn test_parse_alpine_no_files_package_still_detected() {
        let path = PathBuf::from("testdata/alpine-fixtures/full-installed/installed");
        let content = std::fs::read_to_string(&path).expect("read installed db fixture");
        let packages = parse_alpine_installed_db(&content);
        let libc_utils = packages
            .into_iter()
            .find(|pkg| pkg.name.as_deref() == Some("libc-utils"))
            .expect("libc-utils package should exist");

        assert_eq!(libc_utils.file_references.len(), 0);
        assert!(
            libc_utils
                .purl
                .as_deref()
                .is_some_and(|p| p.contains("libc-utils"))
        );
    }

    #[test]
    fn test_parse_alpine_commit_generates_https_vcs_url() {
        let content =
            "P:test-package\nV:1.0-r0\nA:x86_64\nc:cb70ca5c6d6db0399d2dd09189c5d57827bce5cd\n";
        let (_dir, path) = create_temp_installed_db(content);
        let pkg = AlpineInstalledParser::extract_first_package(&path);

        assert_eq!(
            pkg.vcs_url.as_deref(),
            Some(
                "git+https://git.alpinelinux.org/aports/commit/?id=cb70ca5c6d6db0399d2dd09189c5d57827bce5cd"
            )
        );
    }

    #[test]
    fn test_parse_alpine_virtual_package() {
        let content = "P:.postgis-rundeps
V:20210104.190748
A:noarch
S:0
I:0
T:virtual meta package
U:
L:
D:json-c geos gdal proj protobuf-c libstdc++

";
        let (_dir, path) = create_temp_installed_db(content);
        let pkg = AlpineInstalledParser::extract_first_package(&path);
        assert_eq!(pkg.name, Some(".postgis-rundeps".to_string()));
        assert_eq!(pkg.version, Some("20210104.190748".to_string()));
        assert_eq!(pkg.description, Some("virtual meta package".to_string()));
        assert!(pkg.extra_data.is_some());
        let extra = pkg.extra_data.as_ref().unwrap();
        assert_eq!(
            extra.get("is_virtual").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(pkg.dependencies.len(), 6);
        assert!(pkg.homepage_url.is_none());
        assert!(pkg.extracted_license_statement.is_none());
    }

    #[test]
    fn test_installed_db_license_normalization() {
        let content = "P:test-package\nV:1.0-r0\nA:x86_64\nL:MIT\n\n";
        let (_dir, path) = create_temp_installed_db(content);
        let pkg = AlpineInstalledParser::extract_first_package(&path);

        assert_eq!(pkg.extracted_license_statement.as_deref(), Some("MIT"));
        assert_eq!(pkg.declared_license_expression.as_deref(), Some("mit"));
        assert_eq!(pkg.declared_license_expression_spdx.as_deref(), Some("MIT"));
        assert_eq!(pkg.license_detections.len(), 1);
    }

    #[test]
    fn test_apk_archive_license_normalization() {
        let path = PathBuf::from("testdata/alpine/apk/basic/test-package-1.0-r0.apk");
        let pkg = AlpineApkParser::extract_first_package(&path);

        assert_eq!(pkg.extracted_license_statement.as_deref(), Some("MIT"));
        assert_eq!(pkg.declared_license_expression.as_deref(), Some("mit"));
        assert_eq!(pkg.declared_license_expression_spdx.as_deref(), Some("MIT"));
        assert_eq!(pkg.license_detections.len(), 1);
    }
}

crate::register_parser!(
    "Alpine Linux package (installed db and .apk archive)",
    &["**/lib/apk/db/installed", "**/*.apk"],
    "alpine",
    "",
    Some("https://wiki.alpinelinux.org/wiki/Apk_spec"),
);

crate::register_parser!(
    "Alpine Linux APKBUILD recipe",
    &["**/APKBUILD"],
    "alpine",
    "Shell",
    Some("https://wiki.alpinelinux.org/wiki/APKBUILD_Reference"),
);
