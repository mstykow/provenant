// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Parser for CPAN dist.ini files.
//!
//! Extracts Perl package metadata from `dist.ini` files used by Dist::Zilla.
//!
//! # Supported Formats
//! - `dist.ini` - CPAN Dist::Zilla configuration
//!
//! # Implementation Notes
//! - Format: INI-style configuration file
//! - Spec: https://metacpan.org/pod/Dist::Zilla::Tutorial
//! - Extracts: name, version, author, license, copyright_holder, abstract
//! - Dependencies from [Prereq] sections (beyond Python which has no parser)

use std::collections::HashMap;
use std::path::Path;

use crate::parser_warn as warn;
use crate::parsers::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};
use serde_json::json;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType, Party};

use super::PackageParser;
use super::license_normalization::{
    DeclaredLicenseMatchMetadata, NormalizedDeclaredLicense, build_declared_license_data,
    empty_declared_license_data, normalize_declared_license_key, normalize_spdx_expression,
};

const PACKAGE_TYPE: PackageType = PackageType::Cpan;

pub struct CpanDistIniParser;

impl PackageParser for CpanDistIniParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.to_str().is_some_and(|p| p.ends_with("/dist.ini"))
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read dist.ini file {:?}: {}", path, e);
                return vec![PackageData {
                    package_type: Some(PACKAGE_TYPE),
                    primary_language: Some("Perl".to_string()),
                    datasource_id: Some(DatasourceId::CpanDistIni),
                    ..Default::default()
                }];
            }
        };

        vec![parse_dist_ini(&content)]
    }
}

pub(crate) fn parse_dist_ini(content: &str) -> PackageData {
    let (root_fields, sections) = parse_ini_structure(content);

    let name = root_fields
        .get("name")
        .map(|s| truncate_field(s.replace('-', "::")));
    let version = root_fields.get("version").cloned().map(truncate_field);
    let description = root_fields.get("abstract").cloned().map(truncate_field);
    let extracted_license_statement = root_fields.get("license").cloned().map(truncate_field);
    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        extracted_license_statement
            .as_deref()
            .and_then(normalize_cpan_dist_ini_license)
            .map(|normalized| {
                build_declared_license_data(
                    normalized,
                    DeclaredLicenseMatchMetadata::single_line(
                        extracted_license_statement.as_deref().unwrap_or_default(),
                    ),
                )
            })
            .unwrap_or_else(empty_declared_license_data);
    let copyright_holder = root_fields
        .get("copyright_holder")
        .cloned()
        .map(truncate_field);

    let parties = parse_author(&root_fields);
    let dependencies = parse_dependencies(&sections);

    let mut extra_data = HashMap::new();
    if let Some(holder) = copyright_holder {
        extra_data.insert("copyright_holder".to_string(), json!(holder));
    }
    if let Some(year) = root_fields.get("copyright_year") {
        extra_data.insert("copyright_year".to_string(), json!(year));
    }

    PackageData {
        package_type: Some(PACKAGE_TYPE),
        namespace: Some("cpan".to_string()),
        name,
        version,
        description,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        extracted_license_statement,
        parties,
        dependencies,
        extra_data: if extra_data.is_empty() {
            None
        } else {
            Some(extra_data)
        },
        datasource_id: Some(DatasourceId::CpanDistIni),
        primary_language: Some("Perl".to_string()),
        ..Default::default()
    }
}

fn normalize_cpan_dist_ini_license(value: &str) -> Option<NormalizedDeclaredLicense> {
    match value.trim() {
        "Perl_5" => Some(NormalizedDeclaredLicense::new(
            "gpl-1.0-plus OR artistic-perl-1.0",
            "GPL-1.0-or-later OR Artistic-1.0-Perl",
        )),
        other => normalize_spdx_expression(other).or_else(|| normalize_declared_license_key(other)),
    }
}

fn parse_ini_structure(
    content: &str,
) -> (
    HashMap<String, String>,
    HashMap<String, HashMap<String, String>>,
) {
    let mut root_fields = HashMap::new();
    let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut current_section: Option<String> = None;

    for line in content.lines().take(MAX_ITERATION_COUNT) {
        let line = line.trim();

        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            current_section = Some(line[1..line.len() - 1].to_string());
            continue;
        }

        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim().to_string();
            let value = truncate_field(value.trim().to_string());

            if let Some(section_name) = &current_section {
                sections
                    .entry(section_name.clone())
                    .or_default()
                    .insert(key, value);
            } else {
                root_fields.insert(key, value);
            }
        }
    }

    (root_fields, sections)
}

fn parse_author(fields: &HashMap<String, String>) -> Vec<Party> {
    fields
        .get("author")
        .map(|author_str| {
            if let Some((name, email)) = parse_author_string(author_str) {
                vec![Party {
                    role: Some("author".to_string()),
                    name: Some(name),
                    email: Some(email),
                    r#type: None,
                    url: None,
                    organization: None,
                    organization_url: None,
                    timezone: None,
                }]
            } else {
                vec![Party {
                    role: Some("author".to_string()),
                    name: Some(truncate_field(author_str.clone())),
                    r#type: None,
                    email: None,
                    url: None,
                    organization: None,
                    organization_url: None,
                    timezone: None,
                }]
            }
        })
        .unwrap_or_default()
}

fn parse_author_string(s: &str) -> Option<(String, String)> {
    if let Some(start) = s.find('<')
        && let Some(end) = s.find('>')
    {
        let name = truncate_field(s[..start].trim().to_string());
        let email = truncate_field(s[start + 1..end].trim().to_string());
        return Some((name, email));
    }
    None
}

fn parse_dependencies(sections: &HashMap<String, HashMap<String, String>>) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    let mut sorted_sections: Vec<_> = sections.iter().collect();
    sorted_sections.sort_by(|(left_name, _), (right_name, _)| left_name.cmp(right_name));

    for (section_name, fields) in sorted_sections.iter().take(MAX_ITERATION_COUNT) {
        let Some(scope) = classify_prereq_scope(section_name) else {
            continue;
        };

        let mut sorted_fields: Vec<_> = fields.iter().collect();
        sorted_fields.sort_by(|(left_name, _), (right_name, _)| left_name.cmp(right_name));

        for (module_name, version_req) in sorted_fields.iter().take(MAX_ITERATION_COUNT) {
            let purl = truncate_field(format!("pkg:cpan/{}", module_name));
            let extracted_requirement = if version_req.as_str() == "0" || version_req.is_empty() {
                None
            } else {
                Some(truncate_field(version_req.to_string()))
            };

            dependencies.push(Dependency {
                purl: Some(purl),
                scope: Some(scope.clone()),
                extracted_requirement,
                is_runtime: Some(scope == "runtime"),
                is_optional: Some(false),
                is_pinned: None,
                is_direct: None,
                resolved_package: None,
                extra_data: None,
            });
        }
    }

    dependencies
}

fn classify_prereq_scope(section_name: &str) -> Option<String> {
    if !section_name.starts_with("Prereq") {
        return None;
    }

    if section_name.contains("TestRequires") || section_name.contains("Test") {
        Some("test".to_string())
    } else if section_name.contains("BuildRequires") || section_name.contains("Build") {
        Some("build".to_string())
    } else if section_name.contains("ConfigureRequires") || section_name.contains("Configure") {
        Some("configure".to_string())
    } else {
        Some("runtime".to_string())
    }
}

crate::register_parser!(
    "CPAN Perl dist.ini",
    &["*/dist.ini"],
    "cpan",
    "Perl",
    Some("https://metacpan.org/pod/Dist::Zilla::Tutorial"),
);
