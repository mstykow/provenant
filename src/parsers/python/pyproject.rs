// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::super::license_normalization::normalize_spdx_declared_license;
use super::PythonParser;
use super::rfc822_meta::{build_extracted_license_statement, split_classifiers};
use super::utils::{
    ProjectUrls, apply_project_url_mappings, build_python_dependency_purl, default_package_data,
    has_private_classifier, normalize_python_dependency_name, normalize_python_package_name,
    read_toml_file,
};
use crate::models::{DatasourceId, Dependency, PackageData, Party};
use crate::parser_warn as warn;
use crate::parsers::PackageParser;
use crate::parsers::pep508::parse_pep508_requirement;
use crate::parsers::utils::{split_name_email, truncate_field};
use packageurl::PackageUrl;
use serde_json::{Map as JsonMap, Value as JsonValue};
use std::collections::HashMap;
use std::path::Path;
use toml::Value as TomlValue;
use toml::map::Map as TomlMap;

const FIELD_PROJECT: &str = "project";
const FIELD_NAME: &str = "name";
const FIELD_VERSION: &str = "version";
const FIELD_DESCRIPTION: &str = "description";
const FIELD_KEYWORDS: &str = "keywords";
const FIELD_LICENSE: &str = "license";
const FIELD_AUTHORS: &str = "authors";
const FIELD_MAINTAINERS: &str = "maintainers";
const FIELD_URLS: &str = "urls";
const FIELD_HOMEPAGE: &str = "homepage";
const FIELD_REPOSITORY: &str = "repository";
const FIELD_DEPENDENCIES: &str = "dependencies";
const FIELD_OPTIONAL_DEPENDENCIES: &str = "optional-dependencies";
const FIELD_EXTRAS: &str = "extras";

const FIELD_DEPENDENCY_GROUPS: &str = "dependency-groups";
const FIELD_DEV_DEPENDENCIES: &str = "dev-dependencies";

pub(super) fn extract(path: &Path) -> Vec<PackageData> {
    let toml_content = match read_toml_file(path) {
        Ok(content) => content,
        Err(e) => {
            warn!(
                "Failed to read or parse pyproject.toml at {:?}: {}",
                path, e
            );
            return default_package_data(path);
        }
    };

    let tool_table = toml_content.get("tool").and_then(|v| v.as_table());
    let is_poetry_pyproject = tool_table
        .and_then(|tool| tool.get("poetry"))
        .and_then(|value| value.as_table())
        .is_some();

    let project_table =
        if let Some(project) = toml_content.get(FIELD_PROJECT).and_then(|v| v.as_table()) {
            project.clone()
        } else if let Some(tool) = tool_table {
            if let Some(poetry) = tool.get("poetry").and_then(|v| v.as_table()) {
                poetry.clone()
            } else {
                return default_package_data(path);
            }
        } else if toml_content.get(FIELD_NAME).is_some() {
            match toml_content.as_table() {
                Some(table) => table.clone(),
                None => {
                    warn!("Failed to convert TOML content to table in {:?}", path);
                    return default_package_data(path);
                }
            }
        } else {
            return default_package_data(path);
        };

    let name = project_table
        .get(FIELD_NAME)
        .and_then(|v| v.as_str())
        .map(|v| truncate_field(v.to_string()));

    let version = project_table
        .get(FIELD_VERSION)
        .and_then(|v| v.as_str())
        .map(String::from);
    let classifiers = project_table
        .get("classifiers")
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let (classifier_keywords, license_classifiers) = split_classifiers(&classifiers);

    let extracted_license_statement = extract_raw_license_string(&project_table);
    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        normalize_spdx_declared_license(extract_license_expression_candidate(&project_table));

    let description = project_table
        .get(FIELD_DESCRIPTION)
        .and_then(|value| value.as_str())
        .map(|value| truncate_field(value.to_string()));
    let mut keywords = project_table
        .get(FIELD_KEYWORDS)
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for classifier in classifier_keywords {
        if !keywords.contains(&classifier) {
            keywords.push(classifier);
        }
    }

    let mut extra_data = extract_pyproject_extra_data(&toml_content).unwrap_or_default();
    let urls = extract_urls(&project_table, &mut extra_data);

    let (dependencies, optional_dependencies) = extract_dependencies(&project_table, &toml_content);

    let purl = name.as_ref().and_then(|n| {
        let mut package_url = match PackageUrl::new(PythonParser::PACKAGE_TYPE.as_str(), n) {
            Ok(p) => p,
            Err(e) => {
                warn!(
                    "Failed to create PackageUrl for Python package '{}': {}",
                    n, e
                );
                return None;
            }
        };

        if let Some(v) = &version
            && let Err(e) = package_url.with_version(v)
        {
            warn!(
                "Failed to set version '{}' for Python package '{}': {}",
                v, n, e
            );
            return None;
        }

        Some(package_url.to_string())
    });

    let api_data_url = name.as_ref().map(|n| {
        if let Some(v) = &version {
            format!("https://pypi.org/pypi/{}/{}/json", n, v)
        } else {
            format!("https://pypi.org/pypi/{}/json", n)
        }
    });

    let pypi_homepage_url = name
        .as_ref()
        .map(|n| format!("https://pypi.org/project/{}", n));

    let pypi_download_url = name.as_ref().and_then(|n| {
        version.as_ref().map(|v| {
            format!(
                "https://pypi.org/packages/source/{}/{}/{}-{}.tar.gz",
                &n[..1.min(n.len())],
                n,
                n,
                v
            )
        })
    });

    vec![PackageData {
        package_type: Some(PythonParser::PACKAGE_TYPE),
        name,
        version,
        description,
        parties: extract_parties(&project_table),
        keywords,
        homepage_url: urls.homepage_url.or(pypi_homepage_url),
        download_url: urls
            .download_url
            .or_else(|| urls.vcs_url.clone())
            .or(pypi_download_url),
        bug_tracking_url: urls.bug_tracking_url,
        code_view_url: urls.code_view_url,
        vcs_url: urls.vcs_url,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        extracted_license_statement: extracted_license_statement
            .or_else(|| build_extracted_license_statement(None, &license_classifiers)),
        is_private: has_private_classifier(&classifiers),
        extra_data: if extra_data.is_empty() {
            None
        } else {
            Some(extra_data)
        },
        dependencies: [dependencies, optional_dependencies].concat(),
        api_data_url,
        datasource_id: Some(if is_poetry_pyproject {
            DatasourceId::PypiPoetryPyprojectToml
        } else {
            DatasourceId::PypiPyprojectToml
        }),
        purl,
        ..Default::default()
    }]
}

fn extract_raw_license_string(project: &TomlMap<String, TomlValue>) -> Option<String> {
    project
        .get(FIELD_LICENSE)
        .and_then(|license_value| match license_value {
            TomlValue::String(license_str) => Some(license_str.clone()),
            TomlValue::Table(license_table) => license_table
                .get("text")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| {
                    license_table
                        .get("expression")
                        .and_then(|v| v.as_str())
                        .map(|expr| expr.to_string())
                }),
            _ => None,
        })
}

fn extract_license_expression_candidate(project: &TomlMap<String, TomlValue>) -> Option<&str> {
    match project.get(FIELD_LICENSE) {
        Some(TomlValue::String(license_str)) => Some(license_str.as_str()),
        Some(TomlValue::Table(license_table)) => license_table
            .get("expression")
            .and_then(|value| value.as_str()),
        _ => None,
    }
}

fn extract_urls(
    project: &TomlMap<String, TomlValue>,
    extra_data: &mut HashMap<String, serde_json::Value>,
) -> ProjectUrls {
    let mut urls = ProjectUrls {
        homepage_url: None,
        download_url: None,
        bug_tracking_url: None,
        code_view_url: None,
        vcs_url: None,
        changelog_url: None,
    };

    if let Some(url_table) = project.get(FIELD_URLS).and_then(|v| v.as_table()) {
        let parsed_urls: Vec<(String, String)> = url_table
            .iter()
            .filter_map(|(label, value)| {
                value
                    .as_str()
                    .map(|url| (label.to_string(), url.to_string()))
            })
            .collect();
        apply_project_url_mappings(&parsed_urls, &mut urls, extra_data);

        urls.download_url = url_table
            .get("Downloads")
            .or_else(|| url_table.get("downloads"))
            .and_then(|v| v.as_str())
            .map(String::from);

        if urls.homepage_url.is_none() {
            urls.homepage_url = url_table
                .get(FIELD_HOMEPAGE)
                .and_then(|v| v.as_str())
                .map(String::from);
        }
        if urls.vcs_url.is_none() {
            urls.vcs_url = url_table
                .get(FIELD_REPOSITORY)
                .and_then(|v| v.as_str())
                .map(String::from);
        }
    }

    if urls.homepage_url.is_none() {
        urls.homepage_url = project
            .get(FIELD_HOMEPAGE)
            .and_then(|v| v.as_str())
            .map(String::from);
    }

    if urls.vcs_url.is_none() {
        urls.vcs_url = project
            .get(FIELD_REPOSITORY)
            .and_then(|v| v.as_str())
            .map(String::from);
    }

    urls
}

fn extract_parties(project: &TomlMap<String, TomlValue>) -> Vec<Party> {
    let mut parties = Vec::new();

    if let Some(authors) = project.get(FIELD_AUTHORS).and_then(|v| v.as_array()) {
        for author in authors {
            if let Some(author_str) = author.as_str() {
                let (name, email) = split_name_email(author_str);
                parties.push(Party::person("author", name, email));
            } else if let Some(author_table) = author.as_table() {
                let name = author_table
                    .get("name")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string());
                let email = author_table
                    .get("email")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string());
                if name.is_some() || email.is_some() {
                    parties.push(Party::person("author", name, email));
                }
            }
        }
    }

    if let Some(maintainers) = project.get(FIELD_MAINTAINERS).and_then(|v| v.as_array()) {
        for maintainer in maintainers {
            if let Some(maintainer_str) = maintainer.as_str() {
                let (name, email) = split_name_email(maintainer_str);
                parties.push(Party::person("maintainer", name, email));
            } else if let Some(maintainer_table) = maintainer.as_table() {
                let name = maintainer_table
                    .get("name")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string());
                let email = maintainer_table
                    .get("email")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string());
                if name.is_some() || email.is_some() {
                    parties.push(Party::person("maintainer", name, email));
                }
            }
        }
    }

    parties
}

fn extract_dependencies(
    project: &TomlMap<String, TomlValue>,
    toml_content: &TomlValue,
) -> (Vec<Dependency>, Vec<Dependency>) {
    let mut dependencies = Vec::new();
    let mut optional_dependencies = Vec::new();

    if let Some(deps_value) = project.get(FIELD_DEPENDENCIES) {
        match deps_value {
            TomlValue::Array(arr) => {
                dependencies = parse_dependency_array(arr, false, None);
            }
            TomlValue::Table(table) => {
                dependencies = parse_dependency_table(table, false, None);
            }
            _ => {}
        }
    }

    if let Some(opt_deps_table) = project
        .get(FIELD_OPTIONAL_DEPENDENCIES)
        .and_then(|v| v.as_table())
    {
        for (extra_name, deps) in opt_deps_table {
            match deps {
                TomlValue::Array(arr) => {
                    optional_dependencies.extend(parse_dependency_array(
                        arr,
                        true,
                        Some(extra_name),
                    ));
                }
                TomlValue::Table(table) => {
                    optional_dependencies.extend(parse_dependency_table(
                        table,
                        true,
                        Some(extra_name),
                    ));
                }
                _ => {}
            }
        }
    }

    if let Some(dev_deps_value) = project.get(FIELD_DEV_DEPENDENCIES) {
        match dev_deps_value {
            TomlValue::Array(arr) => {
                optional_dependencies.extend(parse_dependency_array(
                    arr,
                    true,
                    Some(FIELD_DEV_DEPENDENCIES),
                ));
            }
            TomlValue::Table(table) => {
                optional_dependencies.extend(parse_dependency_table(
                    table,
                    true,
                    Some(FIELD_DEV_DEPENDENCIES),
                ));
            }
            _ => {}
        }
    }

    if let Some(groups_table) = toml_content
        .get("tool")
        .and_then(|value| value.as_table())
        .and_then(|tool| tool.get("poetry"))
        .and_then(|value| value.as_table())
        .and_then(|poetry| poetry.get("group"))
        .and_then(|value| value.as_table())
    {
        for (group_name, group_data) in groups_table {
            if let Some(group_deps) = group_data.as_table().and_then(|t| t.get("dependencies")) {
                match group_deps {
                    TomlValue::Array(arr) => {
                        optional_dependencies.extend(parse_dependency_array(
                            arr,
                            true,
                            Some(group_name),
                        ));
                    }
                    TomlValue::Table(table) => {
                        optional_dependencies.extend(parse_poetry_group_dependency_table(
                            table,
                            true,
                            Some(group_name),
                        ));
                    }
                    _ => {}
                }
            }
        }
    }

    if let Some(groups_table) = toml_content
        .get(FIELD_DEPENDENCY_GROUPS)
        .and_then(|value| value.as_table())
    {
        for (group_name, deps) in groups_table {
            match deps {
                TomlValue::Array(arr) => {
                    optional_dependencies.extend(parse_dependency_array(
                        arr,
                        true,
                        Some(group_name),
                    ));
                }
                TomlValue::Table(table) => {
                    optional_dependencies.extend(parse_dependency_table(
                        table,
                        true,
                        Some(group_name),
                    ));
                }
                _ => {}
            }
        }
    }

    if let Some(dev_deps_value) = toml_content
        .get("tool")
        .and_then(|value| value.as_table())
        .and_then(|tool| tool.get("uv"))
        .and_then(|value| value.as_table())
        .and_then(|uv| uv.get(FIELD_DEV_DEPENDENCIES))
    {
        match dev_deps_value {
            TomlValue::Array(arr) => {
                optional_dependencies.extend(parse_dependency_array(arr, true, Some("dev")));
            }
            TomlValue::Table(table) => {
                optional_dependencies.extend(parse_dependency_table(table, true, Some("dev")));
            }
            _ => {}
        }
    }

    (dependencies, optional_dependencies)
}

fn extract_pyproject_extra_data(toml_content: &TomlValue) -> Option<HashMap<String, JsonValue>> {
    let mut extra_data = HashMap::new();

    if let Some(tool_uv) = toml_content
        .get("tool")
        .and_then(|value| value.as_table())
        .and_then(|tool| tool.get("uv"))
    {
        extra_data.insert("tool_uv".to_string(), toml_value_to_json(tool_uv));
    }

    if extra_data.is_empty() {
        None
    } else {
        Some(extra_data)
    }
}

fn toml_value_to_json(value: &TomlValue) -> JsonValue {
    match value {
        TomlValue::String(value) => JsonValue::String(value.clone()),
        TomlValue::Integer(value) => JsonValue::String(value.to_string()),
        TomlValue::Float(value) => JsonValue::String(value.to_string()),
        TomlValue::Boolean(value) => JsonValue::Bool(*value),
        TomlValue::Datetime(value) => JsonValue::String(value.to_string()),
        TomlValue::Array(values) => {
            JsonValue::Array(values.iter().map(toml_value_to_json).collect())
        }
        TomlValue::Table(values) => JsonValue::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), toml_value_to_json(value)))
                .collect::<JsonMap<String, JsonValue>>(),
        ),
    }
}

fn parse_dependency_table(
    table: &TomlMap<String, TomlValue>,
    is_optional: bool,
    scope: Option<&str>,
) -> Vec<Dependency> {
    table
        .iter()
        .filter_map(|(name, version)| {
            let version_str = version.as_str().map(|s| s.to_string());
            let mut package_url =
                PackageUrl::new(PythonParser::PACKAGE_TYPE.as_str(), name).ok()?;

            if let Some(v) = &version_str {
                package_url.with_version(v).ok()?;
            }

            Some(Dependency {
                purl: Some(package_url.to_string()),
                extracted_requirement: None,
                scope: scope.map(|s| s.to_string()),
                is_runtime: Some(!is_optional),
                is_optional: Some(is_optional),
                is_pinned: None,
                is_direct: Some(true),
                resolved_package: None,
                extra_data: None,
            })
        })
        .collect()
}

fn parse_poetry_group_dependency_table(
    table: &TomlMap<String, TomlValue>,
    is_optional: bool,
    scope: Option<&str>,
) -> Vec<Dependency> {
    table
        .iter()
        .filter_map(|(name, value)| build_poetry_group_dependency(name, value, is_optional, scope))
        .collect()
}

fn build_poetry_group_dependency(
    name: &str,
    value: &TomlValue,
    is_optional: bool,
    scope: Option<&str>,
) -> Option<Dependency> {
    let normalized_name = normalize_python_dependency_name(name);
    let (version_spec, extras, marker) = match value {
        TomlValue::String(spec) => (Some(spec.trim().to_string()), Vec::new(), None),
        TomlValue::Table(table) => {
            let version_spec = table
                .get(FIELD_VERSION)
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            let extras = table
                .get(FIELD_EXTRAS)
                .and_then(|value| value.as_array())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let marker = table
                .get("markers")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);

            (version_spec, extras, marker)
        }
        _ => return None,
    };

    let pinned_version = version_spec
        .as_deref()
        .and_then(extract_exact_pinned_version);
    let purl = build_python_dependency_purl(&normalized_name, pinned_version.as_deref())?;

    let mut extra_data = HashMap::new();
    if let Some(marker) = marker {
        extra_data.insert("marker".to_string(), JsonValue::String(marker));
    }
    if !extras.is_empty() {
        extra_data.insert(
            "extras".to_string(),
            JsonValue::Array(extras.into_iter().map(JsonValue::String).collect()),
        );
    }

    Some(Dependency {
        purl: Some(purl),
        extracted_requirement: version_spec,
        scope: scope.map(|value| value.to_string()),
        is_runtime: Some(!is_optional),
        is_optional: Some(is_optional),
        is_pinned: Some(pinned_version.is_some()),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: if extra_data.is_empty() {
            None
        } else {
            Some(extra_data)
        },
    })
}

fn parse_dependency_array(
    array: &[TomlValue],
    is_optional: bool,
    scope: Option<&str>,
) -> Vec<Dependency> {
    array
        .iter()
        .filter_map(|dep| {
            let dep_str = dep.as_str()?;
            build_pyproject_array_dependency(dep_str, is_optional, scope)
        })
        .collect()
}

fn build_pyproject_array_dependency(
    dep_str: &str,
    is_optional: bool,
    scope: Option<&str>,
) -> Option<Dependency> {
    let parsed = parse_pep508_requirement(dep_str)?;
    let name = normalize_python_package_name(&parsed.name);
    let pinned_version = parsed
        .specifiers
        .as_deref()
        .and_then(extract_exact_pinned_version);

    let purl = build_python_dependency_purl(&name, pinned_version.as_deref())?;

    let mut extra_data = HashMap::new();
    if let Some(marker) = parsed.marker {
        extra_data.insert("marker".to_string(), JsonValue::String(marker));
    }
    if !parsed.extras.is_empty() {
        extra_data.insert(
            "extras".to_string(),
            JsonValue::Array(parsed.extras.into_iter().map(JsonValue::String).collect()),
        );
    }

    let extracted_requirement = parsed.specifiers.or(parsed.url);

    Some(Dependency {
        purl: Some(purl),
        extracted_requirement: extracted_requirement.clone(),
        scope: scope.map(|s| s.to_string()),
        is_runtime: Some(!is_optional),
        is_optional: Some(is_optional),
        is_pinned: Some(pinned_version.is_some()),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: if extra_data.is_empty() {
            None
        } else {
            Some(extra_data)
        },
    })
}

fn extract_exact_pinned_version(specifiers: &str) -> Option<String> {
    let trimmed = specifiers.trim();
    if trimmed.contains(',') {
        return None;
    }

    let stripped = if let Some(version) = trimmed.strip_prefix("===") {
        version
    } else {
        trimmed.strip_prefix("==")?
    };

    let version = stripped.trim();
    if version.is_empty() {
        None
    } else {
        Some(version.to_string())
    }
}
