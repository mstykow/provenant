// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::super::license_normalization::{
    DeclaredLicenseMatchMetadata, build_declared_license_data, normalize_spdx_declared_license,
    normalize_spdx_expression,
};
use super::PythonParser;
use super::archive::{parse_file_list, parse_record_csv};
use super::utils::{
    ProjectUrls, apply_project_url_mappings, build_pypi_urls, extract_rfc822_dependencies,
    parse_requires_txt,
};
use crate::models::{DatasourceId, FileReference, PackageData, Party};
use crate::parser_warn as warn;
use crate::parsers::PackageParser;
use crate::parsers::utils::{read_file_to_string, truncate_field};
use packageurl::PackageUrl;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
struct InstalledWheelMetadata {
    wheel_tags: Vec<String>,
    wheel_version: Option<String>,
    wheel_generator: Option<String>,
    root_is_purelib: Option<bool>,
    compressed_tag: Option<String>,
}

pub(super) fn extract_from_rfc822_metadata(
    path: &Path,
    datasource_id: DatasourceId,
) -> Vec<PackageData> {
    let content = match read_file_to_string(path, None) {
        Ok(content) => content,
        Err(e) => {
            warn!("Failed to read metadata at {:?}: {}", path, e);
            return super::utils::default_package_data(path);
        }
    };

    let metadata = super::super::rfc822::parse_rfc822_content(&content);
    let mut package_data = build_package_data_from_rfc822(&metadata, datasource_id);
    merge_sibling_metadata_dependencies(path, &mut package_data);
    merge_sibling_metadata_file_references(path, &mut package_data);
    if datasource_id == DatasourceId::PypiWheelMetadata {
        merge_sibling_wheel_metadata(path, &mut package_data);
    }
    vec![package_data]
}

fn merge_sibling_metadata_dependencies(path: &Path, package_data: &mut PackageData) {
    let mut extra_dependencies = Vec::new();

    if let Some(parent) = path.parent() {
        let direct_requires = parent.join("requires.txt");
        if direct_requires.exists()
            && let Ok(content) = read_file_to_string(&direct_requires, None)
        {
            extra_dependencies.extend(parse_requires_txt(&content));
        }

        let sibling_egg_info_requires = parent
            .read_dir()
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .find_map(|entry| {
                let child_path = entry.path();
                if child_path.is_dir()
                    && child_path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.ends_with(".egg-info"))
                {
                    let requires = child_path.join("requires.txt");
                    requires.exists().then_some(requires)
                } else {
                    None
                }
            });

        if let Some(requires_path) = sibling_egg_info_requires
            && let Ok(content) = read_file_to_string(&requires_path, None)
        {
            extra_dependencies.extend(parse_requires_txt(&content));
        }
    }

    for dependency in extra_dependencies {
        if !package_data.dependencies.iter().any(|existing| {
            existing.purl == dependency.purl
                && existing.scope == dependency.scope
                && existing.extracted_requirement == dependency.extracted_requirement
                && existing.extra_data == dependency.extra_data
        }) {
            package_data.dependencies.push(dependency);
        }
    }
}

fn merge_sibling_metadata_file_references(path: &Path, package_data: &mut PackageData) {
    let mut extra_refs = Vec::new();

    if let Some(parent) = path.parent() {
        let record_path = parent.join("RECORD");
        if record_path.exists()
            && let Ok(content) = read_file_to_string(&record_path, None)
        {
            extra_refs.extend(parse_record_csv(&content));
        }

        let installed_files_path = parent.join("installed-files.txt");
        if installed_files_path.exists()
            && let Ok(content) = read_file_to_string(&installed_files_path, None)
        {
            extra_refs.extend(parse_file_list(&content));
        }

        let sources_path = parent.join("SOURCES.txt");
        if sources_path.exists()
            && let Ok(content) = read_file_to_string(&sources_path, None)
        {
            extra_refs.extend(parse_file_list(&content));
        }
    }

    for file_ref in extra_refs {
        if !package_data
            .file_references
            .iter()
            .any(|existing| existing.path == file_ref.path)
        {
            package_data.file_references.push(file_ref);
        }
    }
}

fn merge_sibling_wheel_metadata(path: &Path, package_data: &mut PackageData) {
    let Some(parent) = path.parent() else {
        return;
    };

    if !parent
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".dist-info"))
    {
        return;
    }

    let wheel_path = parent.join("WHEEL");
    if !wheel_path.exists() {
        return;
    }

    let Ok(content) = read_file_to_string(&wheel_path, None) else {
        warn!("Failed to read sibling WHEEL file at {:?}", wheel_path);
        return;
    };

    let Some(wheel_metadata) = parse_installed_wheel_metadata(&content) else {
        return;
    };

    apply_installed_wheel_metadata(package_data, &wheel_metadata);
}

fn parse_installed_wheel_metadata(content: &str) -> Option<InstalledWheelMetadata> {
    use super::super::rfc822::{get_header_all, get_header_first};

    let metadata = super::super::rfc822::parse_rfc822_content(content);
    let wheel_tags = get_header_all(&metadata.headers, "tag");
    if wheel_tags.is_empty() {
        return None;
    }

    let wheel_version = get_header_first(&metadata.headers, "wheel-version");
    let wheel_generator = get_header_first(&metadata.headers, "generator");
    let root_is_purelib =
        get_header_first(&metadata.headers, "root-is-purelib").and_then(|value| {
            match value.to_ascii_lowercase().as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            }
        });

    let compressed_tag = compress_wheel_tags(&wheel_tags);

    Some(InstalledWheelMetadata {
        wheel_tags,
        wheel_version,
        wheel_generator,
        root_is_purelib,
        compressed_tag,
    })
}

fn compress_wheel_tags(tags: &[String]) -> Option<String> {
    if tags.is_empty() {
        return None;
    }

    if tags.len() == 1 {
        return Some(tags[0].clone());
    }

    let mut python_tags = Vec::new();
    let mut abi_tag: Option<&str> = None;
    let mut platform_tag: Option<&str> = None;

    for tag in tags {
        let mut parts = tag.splitn(3, '-');
        let python = parts.next()?;
        let abi = parts.next()?;
        let platform = parts.next()?;

        if abi_tag.is_some_and(|existing| existing != abi)
            || platform_tag.is_some_and(|existing| existing != platform)
        {
            return None;
        }

        abi_tag = Some(abi);
        platform_tag = Some(platform);
        python_tags.push(python.to_string());
    }

    Some(format!(
        "{}-{}-{}",
        python_tags.join("."),
        abi_tag?,
        platform_tag?
    ))
}

fn apply_installed_wheel_metadata(
    package_data: &mut PackageData,
    wheel_metadata: &InstalledWheelMetadata,
) {
    let extra_data = package_data.extra_data.get_or_insert_with(HashMap::new);
    extra_data.insert(
        "wheel_tags".to_string(),
        JsonValue::Array(
            wheel_metadata
                .wheel_tags
                .iter()
                .cloned()
                .map(JsonValue::String)
                .collect(),
        ),
    );

    if let Some(wheel_version) = &wheel_metadata.wheel_version {
        extra_data.insert(
            "wheel_version".to_string(),
            JsonValue::String(wheel_version.clone()),
        );
    }

    if let Some(wheel_generator) = &wheel_metadata.wheel_generator {
        extra_data.insert(
            "wheel_generator".to_string(),
            JsonValue::String(wheel_generator.clone()),
        );
    }

    if let Some(root_is_purelib) = wheel_metadata.root_is_purelib {
        extra_data.insert(
            "root_is_purelib".to_string(),
            JsonValue::Bool(root_is_purelib),
        );
    }

    if let (Some(name), Some(version), Some(extension)) = (
        package_data.name.as_deref(),
        package_data.version.as_deref(),
        wheel_metadata.compressed_tag.as_deref(),
    ) {
        package_data.purl = build_pypi_purl_with_extension(name, Some(version), extension);
    }
}

pub(super) fn python_parse_rfc822_content(
    content: &str,
    datasource_id: DatasourceId,
) -> PackageData {
    let metadata = super::super::rfc822::parse_rfc822_content(content);
    build_package_data_from_rfc822(&metadata, datasource_id)
}

fn build_package_data_from_rfc822(
    metadata: &super::super::rfc822::Rfc822Metadata,
    datasource_id: DatasourceId,
) -> PackageData {
    use super::super::rfc822::{get_header_all, get_header_first};

    let name = get_header_first(&metadata.headers, "name").map(truncate_field);
    let version = get_header_first(&metadata.headers, "version").map(truncate_field);
    let summary = get_header_first(&metadata.headers, "summary").map(truncate_field);
    let homepage_url = get_header_first(&metadata.headers, "home-page").map(truncate_field);
    let author = get_header_first(&metadata.headers, "author").map(truncate_field);
    let author_email = get_header_first(&metadata.headers, "author-email").map(truncate_field);
    let license = get_header_first(&metadata.headers, "license").map(truncate_field);
    let license_expression = get_header_first(&metadata.headers, "license-expression");
    let download_url = get_header_first(&metadata.headers, "download-url");
    let platform = get_header_first(&metadata.headers, "platform");
    let requires_python = get_header_first(&metadata.headers, "requires-python");
    let classifiers = get_header_all(&metadata.headers, "classifier");
    let license_files = get_header_all(&metadata.headers, "license-file");

    let description_body = if metadata.body.is_empty() {
        get_header_first(&metadata.headers, "description").unwrap_or_default()
    } else {
        metadata.body.clone()
    };

    let description = build_description(summary.as_deref(), &description_body).map(truncate_field);

    let mut parties = Vec::new();
    if author.is_some() || author_email.is_some() {
        parties.push(Party::person("author", author, author_email));
    }

    let (keywords, license_classifiers) = split_classifiers(&classifiers);
    let referenced_license_files: Vec<&str> = license_files.iter().map(String::as_str).collect();
    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        license_expression
            .as_deref()
            .and_then(normalize_spdx_expression)
            .map(|normalized| {
                build_declared_license_data(
                    normalized,
                    DeclaredLicenseMatchMetadata::single_line(
                        license_expression.as_deref().unwrap_or_default(),
                    )
                    .with_referenced_filenames(&referenced_license_files),
                )
            })
            .unwrap_or_else(|| normalize_spdx_declared_license(license_expression.as_deref()));

    let extracted_license_statement = license_expression
        .clone()
        .or_else(|| build_extracted_license_statement(license.as_deref(), &license_classifiers));

    let mut extra_data = HashMap::new();
    if let Some(platform_value) = platform
        && !platform_value.eq_ignore_ascii_case("unknown")
        && !platform_value.is_empty()
    {
        extra_data.insert(
            "platform".to_string(),
            serde_json::Value::String(platform_value),
        );
    }

    if let Some(requires_python_value) = requires_python
        && !requires_python_value.is_empty()
    {
        extra_data.insert(
            "requires_python".to_string(),
            serde_json::Value::String(requires_python_value),
        );
    }

    if !license_files.is_empty() {
        extra_data.insert(
            "license_files".to_string(),
            serde_json::Value::Array(
                license_files
                    .iter()
                    .cloned()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
    }

    let file_references = license_files
        .iter()
        .map(|path| FileReference::from_path(path.clone()))
        .collect();

    let project_urls = get_header_all(&metadata.headers, "project-url");
    let dependencies = extract_rfc822_dependencies(&metadata.headers);
    let mut urls = ProjectUrls {
        homepage_url,
        download_url: None,
        bug_tracking_url: None,
        code_view_url: None,
        vcs_url: None,
        changelog_url: None,
    };

    if !project_urls.is_empty() {
        let parsed_urls = parse_project_urls(&project_urls);
        apply_project_url_mappings(&parsed_urls, &mut urls, &mut extra_data);
    }

    let extra_data = if extra_data.is_empty() {
        None
    } else {
        Some(extra_data)
    };

    let pypi_urls = build_pypi_urls(name.as_deref(), version.as_deref());

    PackageData {
        package_type: Some(PythonParser::PACKAGE_TYPE),
        name,
        version,
        primary_language: Some("Python".to_string()),
        description,
        parties,
        keywords,
        homepage_url: urls.homepage_url,
        download_url,
        bug_tracking_url: urls.bug_tracking_url,
        code_view_url: urls.code_view_url,
        vcs_url: urls.vcs_url,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        extracted_license_statement,
        file_references,
        extra_data,
        dependencies,
        repository_homepage_url: pypi_urls.repository_homepage_url,
        repository_download_url: pypi_urls.repository_download_url,
        api_data_url: pypi_urls.api_data_url,
        datasource_id: Some(datasource_id),
        purl: pypi_urls.purl,
        ..Default::default()
    }
}

fn parse_project_urls(project_urls: &[String]) -> Vec<(String, String)> {
    project_urls
        .iter()
        .filter_map(|url_entry| {
            if let Some((label, url)) = url_entry.split_once(", ") {
                let label_trimmed = label.trim();
                let url_trimmed = url.trim();
                if !label_trimmed.is_empty() && !url_trimmed.is_empty() {
                    return Some((label_trimmed.to_string(), url_trimmed.to_string()));
                }
            }
            None
        })
        .collect()
}

fn build_description(summary: Option<&str>, body: &str) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(summary_value) = summary
        && !summary_value.trim().is_empty()
    {
        parts.push(summary_value.trim().to_string());
    }

    if !body.trim().is_empty() {
        parts.push(body.trim().to_string());
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

pub(super) fn split_classifiers(classifiers: &[String]) -> (Vec<String>, Vec<String>) {
    let mut keywords = Vec::new();
    let mut license_classifiers = Vec::new();

    for classifier in classifiers {
        if classifier.starts_with("License ::") {
            license_classifiers.push(classifier.to_string());
        } else {
            keywords.push(classifier.to_string());
        }
    }

    (keywords, license_classifiers)
}

pub(super) fn build_extracted_license_statement(
    license: Option<&str>,
    license_classifiers: &[String],
) -> Option<String> {
    let mut lines = Vec::new();

    if let Some(value) = license
        && !value.trim().is_empty()
    {
        lines.push(format!("license: {}", value.trim()));
    }

    if !license_classifiers.is_empty() {
        lines.push("classifiers:".to_string());
        for classifier in license_classifiers {
            lines.push(format!("  - '{}'", classifier));
        }
    }

    if lines.is_empty() {
        None
    } else {
        Some(format!("{}\n", lines.join("\n")))
    }
}

fn build_pypi_purl_with_extension(
    name: &str,
    version: Option<&str>,
    extension: &str,
) -> Option<String> {
    let mut package_url = PackageUrl::new(PythonParser::PACKAGE_TYPE.as_str(), name).ok()?;
    if let Some(ver) = version {
        package_url.with_version(ver).ok()?;
    }
    package_url.add_qualifier("extension", extension).ok()?;
    Some(package_url.to_string())
}
