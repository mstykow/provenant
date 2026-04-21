// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::dependency::OutputDependency;
use super::file_reference::OutputFileReference;
use super::license_detection::OutputLicenseDetection;
use super::party::OutputParty;
use super::serde_helpers::serialize_optional_map_as_object;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct OutputPackageData {
    #[serde(rename = "type")]
    pub package_type: Option<crate::models::PackageType>,
    pub namespace: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    #[serde(default, serialize_with = "serialize_optional_map_as_object")]
    pub qualifiers: Option<HashMap<String, String>>,
    pub subpath: Option<String>,
    pub primary_language: Option<String>,
    pub description: Option<String>,
    pub release_date: Option<String>,
    #[serde(default)]
    pub parties: Vec<OutputParty>,
    #[serde(default)]
    pub keywords: Vec<String>,
    pub homepage_url: Option<String>,
    pub download_url: Option<String>,
    pub size: Option<u64>,
    pub sha1: Option<String>,
    pub md5: Option<String>,
    pub sha256: Option<String>,
    pub sha512: Option<String>,
    pub bug_tracking_url: Option<String>,
    pub code_view_url: Option<String>,
    pub vcs_url: Option<String>,
    pub copyright: Option<String>,
    pub holder: Option<String>,
    pub declared_license_expression: Option<String>,
    pub declared_license_expression_spdx: Option<String>,
    #[serde(default)]
    pub license_detections: Vec<OutputLicenseDetection>,
    pub other_license_expression: Option<String>,
    pub other_license_expression_spdx: Option<String>,
    #[serde(default)]
    pub other_license_detections: Vec<OutputLicenseDetection>,
    pub extracted_license_statement: Option<String>,
    pub notice_text: Option<String>,
    #[serde(default)]
    pub source_packages: Vec<String>,
    #[serde(default)]
    pub file_references: Vec<OutputFileReference>,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default)]
    pub is_virtual: bool,
    #[serde(default, serialize_with = "serialize_optional_map_as_object")]
    pub extra_data: Option<HashMap<String, serde_json::Value>>,
    #[serde(default)]
    pub dependencies: Vec<OutputDependency>,
    pub repository_homepage_url: Option<String>,
    pub repository_download_url: Option<String>,
    pub api_data_url: Option<String>,
    pub datasource_id: Option<crate::models::DatasourceId>,
    pub purl: Option<String>,
}

impl From<&crate::models::PackageData> for OutputPackageData {
    fn from(value: &crate::models::PackageData) -> Self {
        Self {
            package_type: value.package_type,
            namespace: value.namespace.clone(),
            name: value.name.clone(),
            version: value.version.clone(),
            qualifiers: value.qualifiers.clone(),
            subpath: value.subpath.clone(),
            primary_language: value.primary_language.clone(),
            description: value.description.clone(),
            release_date: value.release_date.clone(),
            parties: value.parties.iter().map(OutputParty::from).collect(),
            keywords: value.keywords.clone(),
            homepage_url: value.homepage_url.clone(),
            download_url: value.download_url.clone(),
            size: value.size,
            sha1: value.sha1.as_ref().map(|d| d.as_hex()),
            md5: value.md5.as_ref().map(|d| d.as_hex()),
            sha256: value.sha256.as_ref().map(|d| d.as_hex()),
            sha512: value.sha512.as_ref().map(|d| d.as_hex()),
            bug_tracking_url: value.bug_tracking_url.clone(),
            code_view_url: value.code_view_url.clone(),
            vcs_url: value.vcs_url.clone(),
            copyright: value.copyright.clone(),
            holder: value.holder.clone(),
            declared_license_expression: value.declared_license_expression.clone(),
            declared_license_expression_spdx: value.declared_license_expression_spdx.clone(),
            license_detections: value
                .license_detections
                .iter()
                .map(OutputLicenseDetection::from)
                .collect(),
            other_license_expression: value.other_license_expression.clone(),
            other_license_expression_spdx: value.other_license_expression_spdx.clone(),
            other_license_detections: value
                .other_license_detections
                .iter()
                .map(OutputLicenseDetection::from)
                .collect(),
            extracted_license_statement: value.extracted_license_statement.clone(),
            notice_text: value.notice_text.clone(),
            source_packages: value.source_packages.clone(),
            file_references: value
                .file_references
                .iter()
                .map(OutputFileReference::from)
                .collect(),
            is_private: value.is_private,
            is_virtual: value.is_virtual,
            extra_data: value.extra_data.clone(),
            dependencies: value
                .dependencies
                .iter()
                .map(OutputDependency::from)
                .collect(),
            repository_homepage_url: value.repository_homepage_url.clone(),
            repository_download_url: value.repository_download_url.clone(),
            api_data_url: value.api_data_url.clone(),
            datasource_id: value.datasource_id,
            purl: value.purl.clone(),
        }
    }
}

impl TryFrom<&OutputPackageData> for crate::models::PackageData {
    type Error = String;
    fn try_from(value: &OutputPackageData) -> Result<Self, Self::Error> {
        let mut parties = Vec::with_capacity(value.parties.len());
        for p in &value.parties {
            parties.push(crate::models::Party::try_from(p)?);
        }
        let mut license_detections = Vec::with_capacity(value.license_detections.len());
        for d in &value.license_detections {
            license_detections.push(crate::models::LicenseDetection::try_from(d)?);
        }
        let mut other_license_detections = Vec::with_capacity(value.other_license_detections.len());
        for d in &value.other_license_detections {
            other_license_detections.push(crate::models::LicenseDetection::try_from(d)?);
        }
        let mut file_references = Vec::with_capacity(value.file_references.len());
        for f in &value.file_references {
            file_references.push(crate::models::FileReference::try_from(f)?);
        }
        let mut dependencies = Vec::with_capacity(value.dependencies.len());
        for d in &value.dependencies {
            dependencies.push(crate::models::Dependency::try_from(d)?);
        }
        Ok(Self {
            package_type: value.package_type,
            namespace: value.namespace.clone(),
            name: value.name.clone(),
            version: value.version.clone(),
            qualifiers: value.qualifiers.clone(),
            subpath: value.subpath.clone(),
            primary_language: value.primary_language.clone(),
            description: value.description.clone(),
            release_date: value.release_date.clone(),
            parties,
            keywords: value.keywords.clone(),
            homepage_url: value.homepage_url.clone(),
            download_url: value.download_url.clone(),
            size: value.size,
            sha1: value
                .sha1
                .as_ref()
                .map(|s| crate::models::Sha1Digest::from_hex(s))
                .transpose()
                .map_err(|e| format!("invalid sha1: {}", e))?,
            md5: value
                .md5
                .as_ref()
                .map(|s| crate::models::Md5Digest::from_hex(s))
                .transpose()
                .map_err(|e| format!("invalid md5: {}", e))?,
            sha256: value
                .sha256
                .as_ref()
                .map(|s| crate::models::Sha256Digest::from_hex(s))
                .transpose()
                .map_err(|e| format!("invalid sha256: {}", e))?,
            sha512: value
                .sha512
                .as_ref()
                .map(|s| crate::models::Sha512Digest::from_hex(s))
                .transpose()
                .map_err(|e| format!("invalid sha512: {}", e))?,
            bug_tracking_url: value.bug_tracking_url.clone(),
            code_view_url: value.code_view_url.clone(),
            vcs_url: value.vcs_url.clone(),
            copyright: value.copyright.clone(),
            holder: value.holder.clone(),
            declared_license_expression: value.declared_license_expression.clone(),
            declared_license_expression_spdx: value.declared_license_expression_spdx.clone(),
            license_detections,
            other_license_expression: value.other_license_expression.clone(),
            other_license_expression_spdx: value.other_license_expression_spdx.clone(),
            other_license_detections,
            extracted_license_statement: value.extracted_license_statement.clone(),
            notice_text: value.notice_text.clone(),
            source_packages: value.source_packages.clone(),
            file_references,
            is_private: value.is_private,
            is_virtual: value.is_virtual,
            extra_data: value.extra_data.clone(),
            dependencies,
            repository_homepage_url: value.repository_homepage_url.clone(),
            repository_download_url: value.repository_download_url.clone(),
            api_data_url: value.api_data_url.clone(),
            datasource_id: value.datasource_id,
            purl: value.purl.clone(),
        })
    }
}
