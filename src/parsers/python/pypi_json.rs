// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::super::license_normalization::normalize_spdx_declared_license;
use super::PythonParser;
use super::setup_py::package_data_to_resolved;
use super::utils::{
    ProjectUrls, apply_project_url_mappings, build_pypi_urls, default_package_data,
    extract_requires_dist_dependencies, has_private_classifier, parse_setup_cfg_keywords,
};
use crate::models::{DatasourceId, Dependency, PackageData, Party, Sha256Digest};
use crate::parser_warn as warn;
use crate::parsers::PackageParser;
use crate::parsers::utils::{read_file_to_string, truncate_field};
use packageurl::PackageUrl;
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub(super) fn extract_from_pypi_json(path: &Path) -> Vec<PackageData> {
    let default = PackageData {
        package_type: Some(PythonParser::PACKAGE_TYPE),
        datasource_id: Some(DatasourceId::PypiJson),
        ..Default::default()
    };

    let content = match read_file_to_string(path, None) {
        Ok(content) => content,
        Err(error) => {
            warn!("Failed to read pypi.json at {:?}: {}", path, error);
            return vec![default];
        }
    };

    let root: serde_json::Value = match serde_json::from_str(&content) {
        Ok(value) => value,
        Err(error) => {
            warn!("Failed to parse pypi.json at {:?}: {}", path, error);
            return vec![default];
        }
    };

    let Some(info) = root.get("info").and_then(|value| value.as_object()) else {
        warn!("No info object found in pypi.json at {:?}", path);
        return vec![default];
    };

    let name = info
        .get("name")
        .and_then(|value| value.as_str())
        .map(|v| truncate_field(v.to_owned()));
    let version = info
        .get("version")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned);
    let summary = info
        .get("summary")
        .and_then(|value| value.as_str())
        .map(|v| truncate_field(v.to_owned()));
    let description = info
        .get("description")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(|v| truncate_field(v.to_owned()))
        .or(summary);
    let homepage_url = info
        .get("home_page")
        .and_then(|value| value.as_str())
        .map(|v| truncate_field(v.to_owned()));
    let author = info
        .get("author")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(|v| truncate_field(v.to_owned()));
    let author_email = info
        .get("author_email")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned);
    let license = info
        .get("license")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned);
    let keywords = parse_setup_cfg_keywords(
        info.get("keywords")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
    );
    let classifiers = info
        .get("classifiers")
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut parties = Vec::new();
    if author.is_some() || author_email.is_some() {
        parties.push(Party::person("author", author, author_email));
    }

    let mut project_urls = ProjectUrls {
        homepage_url,
        download_url: None,
        bug_tracking_url: None,
        code_view_url: None,
        vcs_url: None,
        changelog_url: None,
    };
    let mut extra_data = HashMap::new();

    let parsed_project_urls = info
        .get("project_urls")
        .and_then(|value| value.as_object())
        .map(|map| {
            let mut pairs: Vec<(String, String)> = map
                .iter()
                .filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_string())))
                .collect();
            pairs.sort_by(|left, right| left.0.cmp(&right.0));
            pairs
        })
        .unwrap_or_default();

    apply_project_url_mappings(&parsed_project_urls, &mut project_urls, &mut extra_data);

    let artifact = root
        .get("urls")
        .and_then(|value| value.as_array())
        .map(|urls| select_pypi_json_artifact(urls))
        .unwrap_or_else(PypiArtifact::empty);

    let download_url = artifact.download_url;
    let size = artifact.size;
    let sha256 = artifact
        .sha256
        .and_then(|h| Sha256Digest::from_hex(&h).ok());

    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        normalize_spdx_declared_license(license.as_deref());
    let dependencies = info
        .get("requires_dist")
        .and_then(|value| value.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.as_str().map(ToOwned::to_owned))
                .collect::<Vec<_>>()
        })
        .map(|entries| extract_requires_dist_dependencies(&entries))
        .unwrap_or_default();

    let pypi_urls = build_pypi_urls(name.as_deref(), version.as_deref());

    vec![PackageData {
        package_type: Some(PythonParser::PACKAGE_TYPE),
        name,
        version,
        description,
        parties,
        keywords,
        homepage_url: project_urls
            .homepage_url
            .or(pypi_urls.repository_homepage_url.clone()),
        download_url,
        size,
        sha256,
        bug_tracking_url: project_urls.bug_tracking_url,
        code_view_url: project_urls.code_view_url,
        vcs_url: project_urls.vcs_url,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        extracted_license_statement: license,
        is_private: has_private_classifier(&classifiers),
        extra_data: if extra_data.is_empty() {
            None
        } else {
            Some(extra_data)
        },
        dependencies,
        repository_homepage_url: pypi_urls.repository_homepage_url,
        repository_download_url: pypi_urls.repository_download_url,
        api_data_url: pypi_urls.api_data_url,
        datasource_id: Some(DatasourceId::PypiJson),
        purl: pypi_urls.purl,
        ..Default::default()
    }]
}

struct PypiArtifact {
    download_url: Option<String>,
    size: Option<u64>,
    sha256: Option<String>,
}

impl PypiArtifact {
    fn empty() -> Self {
        Self {
            download_url: None,
            size: None,
            sha256: None,
        }
    }
}

fn select_pypi_json_artifact(urls: &[serde_json::Value]) -> PypiArtifact {
    let selected = urls
        .iter()
        .find(|entry| entry.get("packagetype").and_then(|value| value.as_str()) == Some("sdist"))
        .or_else(|| urls.first());

    let Some(entry) = selected else {
        return PypiArtifact::empty();
    };

    PypiArtifact {
        download_url: entry
            .get("url")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        size: entry.get("size").and_then(|value| value.as_u64()),
        sha256: entry
            .get("digests")
            .and_then(|value| value.as_object())
            .and_then(|digests| digests.get("sha256"))
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
    }
}

pub(super) fn extract_from_pip_inspect(path: &Path) -> Vec<PackageData> {
    let content = match read_file_to_string(path, None) {
        Ok(content) => content,
        Err(e) => {
            warn!("Failed to read pip-inspect.deplock at {:?}: {}", path, e);
            return default_package_data(path);
        }
    };

    let root: serde_json::Value = match serde_json::from_str(&content) {
        Ok(value) => value,
        Err(e) => {
            warn!(
                "Failed to parse pip-inspect.deplock JSON at {:?}: {}",
                path, e
            );
            return default_package_data(path);
        }
    };

    let installed = match root.get("installed").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => {
            warn!(
                "No 'installed' array found in pip-inspect.deplock at {:?}",
                path
            );
            return default_package_data(path);
        }
    };

    let pip_version = root
        .get("pip_version")
        .and_then(|v| v.as_str())
        .map(String::from);
    let inspect_version = root
        .get("version")
        .and_then(|v| v.as_str())
        .map(String::from);

    let mut main_package: Option<PackageData> = None;
    let mut dependencies: Vec<Dependency> = Vec::new();

    for package_entry in installed {
        let metadata = match package_entry.get("metadata") {
            Some(m) => m,
            None => continue,
        };

        let is_requested = package_entry
            .get("requested")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let has_direct_url = package_entry.get("direct_url").is_some();

        let name = metadata
            .get("name")
            .and_then(|v| v.as_str())
            .map(|v| truncate_field(v.to_string()));
        let version = metadata
            .get("version")
            .and_then(|v| v.as_str())
            .map(String::from);
        let summary = metadata
            .get("summary")
            .and_then(|v| v.as_str())
            .map(|v| truncate_field(v.to_string()));
        let home_page = metadata
            .get("home_page")
            .and_then(|v| v.as_str())
            .map(|v| truncate_field(v.to_string()));
        let author = metadata
            .get("author")
            .and_then(|v| v.as_str())
            .map(|v| truncate_field(v.to_string()));
        let author_email = metadata
            .get("author_email")
            .and_then(|v| v.as_str())
            .map(String::from);
        let license = metadata
            .get("license")
            .and_then(|v| v.as_str())
            .map(|v| truncate_field(v.to_string()));
        let description = metadata
            .get("description")
            .and_then(|v| v.as_str())
            .map(|v| truncate_field(v.to_string()));
        let keywords = metadata
            .get("keywords")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|k| k.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let mut parties = Vec::new();
        if author.is_some() || author_email.is_some() {
            parties.push(Party::person("author", author, author_email));
        }

        let (declared_license_expression, declared_license_expression_spdx, license_detections) =
            normalize_spdx_declared_license(license.as_deref());
        let extracted_license_statement = license.clone();
        let requires_dist = metadata
            .get("requires_dist")
            .and_then(|v| v.as_array())
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| entry.as_str().map(ToOwned::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let parsed_dependencies = extract_requires_dist_dependencies(&requires_dist);

        let purl = name.as_ref().and_then(|n| {
            let mut package_url = PackageUrl::new(PythonParser::PACKAGE_TYPE.as_str(), n).ok()?;
            if let Some(v) = &version {
                package_url.with_version(v).ok()?;
            }
            Some(package_url.to_string())
        });

        if is_requested && has_direct_url {
            let mut extra_data = HashMap::new();
            if let Some(pv) = &pip_version {
                extra_data.insert(
                    "pip_version".to_string(),
                    serde_json::Value::String(pv.clone()),
                );
            }
            if let Some(iv) = &inspect_version {
                extra_data.insert(
                    "inspect_version".to_string(),
                    serde_json::Value::String(iv.clone()),
                );
            }

            main_package = Some(PackageData {
                package_type: Some(PythonParser::PACKAGE_TYPE),
                name,
                version,
                primary_language: Some("Python".to_string()),
                description: description.or(summary),
                parties,
                keywords,
                homepage_url: home_page,
                declared_license_expression,
                declared_license_expression_spdx,
                license_detections,
                extracted_license_statement,
                is_virtual: true,
                extra_data: if extra_data.is_empty() {
                    None
                } else {
                    Some(extra_data)
                },
                dependencies: parsed_dependencies,
                datasource_id: Some(DatasourceId::PypiInspectDeplock),
                purl,
                ..Default::default()
            });
        } else {
            let resolved_package = PackageData {
                package_type: Some(PythonParser::PACKAGE_TYPE),
                name: name.clone(),
                version: version.clone(),
                primary_language: Some("Python".to_string()),
                description: description.or(summary),
                parties,
                keywords,
                homepage_url: home_page,
                declared_license_expression,
                declared_license_expression_spdx,
                license_detections,
                extracted_license_statement,
                is_virtual: true,
                dependencies: parsed_dependencies,
                datasource_id: Some(DatasourceId::PypiInspectDeplock),
                purl: purl.clone(),
                ..Default::default()
            };

            let resolved = package_data_to_resolved(&resolved_package);
            dependencies.push(Dependency {
                purl,
                extracted_requirement: None,
                scope: None,
                is_runtime: Some(true),
                is_optional: Some(false),
                is_pinned: Some(true),
                is_direct: Some(is_requested),
                resolved_package: Some(Box::new(resolved)),
                extra_data: None,
            });
        }
    }

    if let Some(mut main_pkg) = main_package {
        let direct_requirement_purls: HashSet<String> = main_pkg
            .dependencies
            .iter()
            .filter_map(|dep| dep.purl.as_deref().map(base_dependency_purl))
            .collect();

        let resolved_requirement_purls: HashSet<String> = dependencies
            .iter()
            .filter_map(|dep| dep.purl.as_deref().map(base_dependency_purl))
            .collect();

        let unresolved_dependencies = main_pkg
            .dependencies
            .iter()
            .filter(|dep| {
                dep.purl.as_ref().is_some_and(|purl| {
                    !resolved_requirement_purls.contains(&base_dependency_purl(purl))
                })
            })
            .cloned()
            .collect::<Vec<_>>();

        for dependency in &mut dependencies {
            if dependency
                .purl
                .as_ref()
                .is_some_and(|purl| direct_requirement_purls.contains(&base_dependency_purl(purl)))
            {
                dependency.is_direct = Some(true);
            }
        }

        main_pkg.dependencies = dependencies;
        main_pkg.dependencies.extend(unresolved_dependencies);
        vec![main_pkg]
    } else {
        default_package_data(path)
    }
}

fn base_dependency_purl(purl: &str) -> String {
    purl.split_once('@')
        .map(|(base, _)| base.to_string())
        .unwrap_or_else(|| purl.to_string())
}
