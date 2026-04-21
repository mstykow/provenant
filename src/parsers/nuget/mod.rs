// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Parser for NuGet package manifests and configuration files.
//!
//! Extracts package metadata and dependencies from .NET/NuGet ecosystem files:
//! - packages.config (legacy .NET Framework format)
//! - .nuspec (NuGet package specification)
//! - packages.lock.json (NuGet lock file)
//! - .nupkg (NuGet package archive — metadata extraction)
//!
//! # Supported Formats
//! - packages.config (XML)
//! - *.nuspec (XML)
//! - packages.lock.json (JSON)
//! - *.nupkg (ZIP archive with .nuspec inside)
//!
//! # Key Features
//! - Dependency extraction with targetFramework support
//! - Dependency groups by framework version
//! - Package URL (purl) generation
//!
//! # Implementation Notes
//! - Uses quick-xml for XML parsing
//! - Graceful error handling with warn!()
//! - No unwrap/expect in library code

mod deps_json;
mod directory_props;
mod nupkg;
mod nuspec;
mod packages_config;
mod packages_lock;
mod project_file;
mod project_json;
mod utils;

#[cfg(test)]
mod nuget_scan_test;
#[cfg(test)]
mod nuget_test;

use std::fs;
use std::path::Path;

use crate::models::{DatasourceId, PackageData, PackageType, Party};
use packageurl::PackageUrl;

use super::utils::MAX_MANIFEST_SIZE;

pub use self::deps_json::DotNetDepsJsonParser;
pub use self::directory_props::{CentralPackageManagementPropsParser, DirectoryBuildPropsParser};
pub use self::nupkg::NupkgParser;
pub use self::nuspec::NuspecParser;
pub use self::packages_config::PackagesConfigParser;
pub use self::packages_lock::PackagesLockParser;
pub use self::project_file::PackageReferenceProjectParser;
pub use self::project_json::{ProjectJsonParser, ProjectLockJsonParser};

pub(super) fn check_file_size(path: &Path) -> Result<(), String> {
    match fs::metadata(path) {
        Ok(metadata) => {
            if metadata.len() > MAX_MANIFEST_SIZE {
                return Err(format!(
                    "File {:?} is {} bytes, exceeding the {} byte limit",
                    path,
                    metadata.len(),
                    MAX_MANIFEST_SIZE
                ));
            }
            Ok(())
        }
        Err(e) => Err(format!("Cannot stat file {:?}: {}", path, e)),
    }
}

pub(super) const PROJECT_FILE_EXTENSIONS: [&str; 3] = ["csproj", "vbproj", "fsproj"];

#[derive(Default)]
pub(super) struct RepositoryMetadata {
    pub(super) vcs_url: Option<String>,
    pub(super) branch: Option<String>,
    pub(super) commit: Option<String>,
}

pub(super) fn build_nuget_party(role: &str, name: String) -> Party {
    Party {
        r#type: Some("person".to_string()),
        role: Some(role.to_string()),
        name: Some(name),
        email: None,
        url: None,
        organization: None,
        organization_url: None,
        timezone: None,
    }
}

pub(super) fn insert_extra_string(
    extra_data: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<String>,
) {
    if let Some(value) = value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        extra_data.insert(key.to_string(), serde_json::Value::String(value));
    }
}

pub(super) fn parse_repository_metadata(
    element: &quick_xml::events::BytesStart,
) -> RepositoryMetadata {
    let mut repo_type = None;
    let mut repo_url = None;
    let mut branch = None;
    let mut commit = None;

    for attr in element.attributes().filter_map(|a| a.ok()) {
        match attr.key.as_ref() {
            b"type" => repo_type = String::from_utf8(attr.value.to_vec()).ok(),
            b"url" => repo_url = String::from_utf8(attr.value.to_vec()).ok(),
            b"branch" => branch = String::from_utf8(attr.value.to_vec()).ok(),
            b"commit" => commit = String::from_utf8(attr.value.to_vec()).ok(),
            _ => {}
        }
    }

    RepositoryMetadata {
        vcs_url: repo_url.map(|url| match repo_type {
            Some(vcs_type) if !vcs_type.trim().is_empty() => format!("{}+{}", vcs_type, url),
            _ => url,
        }),
        branch,
        commit,
    }
}

pub(super) fn build_nuget_urls(
    name: Option<&str>,
    version: Option<&str>,
) -> (Option<String>, Option<String>, Option<String>) {
    let repository_homepage_url = name.and_then(|name| {
        version.map(|version| format!("https://www.nuget.org/packages/{}/{}", name, version))
    });

    let repository_download_url = name.and_then(|name| {
        version.map(|version| format!("https://www.nuget.org/api/v2/package/{}/{}", name, version))
    });

    let api_data_url = name.and_then(|name| {
        version.map(|version| {
            format!(
                "https://api.nuget.org/v3/registration3/{}/{}.json",
                name.to_lowercase(),
                version
            )
        })
    });

    (
        repository_homepage_url,
        repository_download_url,
        api_data_url,
    )
}

pub(super) fn build_nuget_purl(name: Option<&str>, version: Option<&str>) -> Option<String> {
    let name = name?;
    let mut package_url = PackageUrl::new("nuget", name).ok()?;

    if let Some(version) = version {
        package_url.with_version(version).ok()?;
    }

    Some(package_url.to_string())
}

pub(super) fn project_file_datasource_id(path: &Path) -> Option<DatasourceId> {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("csproj") => Some(DatasourceId::NugetCsproj),
        Some("vbproj") => Some(DatasourceId::NugetVbproj),
        Some("fsproj") => Some(DatasourceId::NugetFsproj),
        _ => None,
    }
}

pub(super) fn build_nuget_description(
    summary: Option<&str>,
    description: Option<&str>,
    title: Option<&str>,
    name: Option<&str>,
) -> Option<String> {
    let summary = summary.map(|s| s.trim()).filter(|s| !s.is_empty());
    let description = description.map(|s| s.trim()).filter(|s| !s.is_empty());
    let title = title.map(|s| s.trim()).filter(|s| !s.is_empty());

    let mut result = match (summary, description) {
        (None, None) => return None,
        (Some(s), None) => s.to_string(),
        (None, Some(d)) => d.to_string(),
        (Some(s), Some(d)) => {
            if d.contains(s) {
                d.to_string()
            } else {
                format!("{}\n{}", s, d)
            }
        }
    };

    if let Some(t) = title {
        if let Some(n) = name {
            if t != n {
                result = format!("{}\n{}", t, result);
            }
        } else {
            result = format!("{}\n{}", t, result);
        }
    }

    Some(result)
}

pub(super) fn default_package_data(datasource_id: Option<DatasourceId>) -> PackageData {
    PackageData {
        package_type: Some(PackageType::Nuget),
        datasource_id,
        ..Default::default()
    }
}
