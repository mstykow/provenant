use serde::{Deserialize, Serialize, Serializer};
use serde_json::Map;

use super::author::OutputAuthor;
use super::copyright::OutputCopyright;
use super::email::OutputEmail;
use super::holder::OutputHolder;
use super::license_detection::OutputLicenseDetection;
use super::license_match::OutputMatch;
use super::license_policy_entry::OutputLicensePolicyEntry;
use super::package_data::OutputPackageData;
use super::serde_helpers::insert_json;
use super::tallies::OutputTallies;
use super::url::OutputURL;

#[derive(Debug, Clone, Deserialize)]
pub struct OutputFileInfo {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub base_name: String,
    #[serde(default)]
    pub extension: String,
    pub path: String,
    #[serde(rename = "type")]
    pub file_type: crate::models::FileType,
    pub mime_type: Option<String>,
    pub file_type_label: Option<String>,
    #[serde(default)]
    pub size: u64,
    pub date: Option<String>,
    pub sha1: Option<String>,
    pub md5: Option<String>,
    pub sha256: Option<String>,
    pub sha1_git: Option<String>,
    pub programming_language: Option<String>,
    #[serde(default)]
    pub package_data: Vec<OutputPackageData>,
    #[serde(rename = "detected_license_expression_spdx")]
    pub license_expression: Option<String>,
    #[serde(default)]
    pub license_detections: Vec<OutputLicenseDetection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub license_clues: Vec<OutputMatch>,
    pub percentage_of_license_text: Option<f64>,
    #[serde(default)]
    pub copyrights: Vec<OutputCopyright>,
    #[serde(default)]
    pub holders: Vec<OutputHolder>,
    #[serde(default)]
    pub authors: Vec<OutputAuthor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emails: Vec<OutputEmail>,
    #[serde(default)]
    pub urls: Vec<OutputURL>,
    #[serde(default)]
    pub for_packages: Vec<String>,
    #[serde(default)]
    pub scan_errors: Vec<String>,
    pub license_policy: Option<Vec<OutputLicensePolicyEntry>>,
    pub is_generated: Option<bool>,
    pub is_binary: Option<bool>,
    pub is_text: Option<bool>,
    pub is_archive: Option<bool>,
    pub is_media: Option<bool>,
    pub is_source: Option<bool>,
    pub is_script: Option<bool>,
    pub files_count: Option<usize>,
    pub dirs_count: Option<usize>,
    pub size_count: Option<u64>,
    pub source_count: Option<usize>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_legal: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_manifest: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_readme: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_top_level: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_key_file: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_community: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facets: Vec<String>,
    pub tallies: Option<OutputTallies>,
}

impl OutputFileInfo {
    pub(crate) fn should_serialize_info_surface(&self) -> bool {
        self.date.is_some()
            || self.sha1.is_some()
            || self.md5.is_some()
            || self.sha256.is_some()
            || self.sha1_git.is_some()
            || self.mime_type.is_some()
            || self.file_type_label.is_some()
            || self.programming_language.is_some()
            || self.is_binary.is_some()
            || self.is_text.is_some()
            || self.is_archive.is_some()
            || self.is_media.is_some()
            || self.is_source.is_some()
            || self.is_script.is_some()
            || self.files_count.is_some()
            || self.dirs_count.is_some()
            || self.size_count.is_some()
    }

    pub(crate) fn should_serialize_license_surface(&self) -> bool {
        self.license_expression.is_some()
            || !self.license_detections.is_empty()
            || !self.license_clues.is_empty()
            || self.percentage_of_license_text.is_some()
    }

    pub(crate) fn detected_license_expression_spdx(&self) -> Option<String> {
        crate::utils::spdx::combine_license_expressions(
            self.license_detections
                .iter()
                .filter(|detection| !detection.license_expression_spdx.is_empty())
                .map(|detection| detection.license_expression_spdx.clone()),
        )
        .or_else(|| {
            crate::utils::spdx::combine_license_expressions(
                self.package_data
                    .iter()
                    .flat_map(|package_data| package_data.license_detections.iter())
                    .filter(|detection| !detection.license_expression_spdx.is_empty())
                    .map(|detection| detection.license_expression_spdx.clone()),
            )
        })
        .or_else(|| {
            self.license_expression
                .clone()
                .filter(|expression| !expression.is_empty())
        })
    }
}

impl Serialize for OutputFileInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = Map::new();
        insert_json(&mut map, "path", &self.path)?;
        insert_json(&mut map, "type", &self.file_type)?;
        insert_json(&mut map, "name", &self.name)?;
        insert_json(&mut map, "base_name", &self.base_name)?;
        insert_json(&mut map, "extension", &self.extension)?;
        insert_json(&mut map, "size", self.size)?;

        if self.should_serialize_info_surface() {
            insert_json(&mut map, "date", &self.date)?;
            insert_json(&mut map, "sha1", self.sha1.as_ref())?;
            insert_json(&mut map, "md5", self.md5.as_ref())?;
            insert_json(&mut map, "sha256", self.sha256.as_ref())?;
            insert_json(&mut map, "sha1_git", self.sha1_git.as_ref())?;
            insert_json(&mut map, "mime_type", &self.mime_type)?;
            insert_json(&mut map, "file_type", &self.file_type_label)?;
            insert_json(&mut map, "programming_language", &self.programming_language)?;
            insert_json(&mut map, "is_binary", self.is_binary)?;
            insert_json(&mut map, "is_text", self.is_text)?;
            insert_json(&mut map, "is_archive", self.is_archive)?;
            insert_json(&mut map, "is_media", self.is_media)?;
            insert_json(&mut map, "is_source", self.is_source)?;
            insert_json(&mut map, "is_script", self.is_script)?;
            insert_json(&mut map, "files_count", self.files_count)?;
            insert_json(&mut map, "dirs_count", self.dirs_count)?;
            insert_json(&mut map, "size_count", self.size_count)?;
        }

        insert_json(&mut map, "package_data", &self.package_data)?;
        insert_json(
            &mut map,
            "detected_license_expression_spdx",
            self.detected_license_expression_spdx(),
        )?;
        insert_json(&mut map, "license_detections", &self.license_detections)?;
        if self.should_serialize_license_surface() {
            insert_json(&mut map, "license_clues", &self.license_clues)?;
        }
        if self.percentage_of_license_text.is_some() {
            insert_json(
                &mut map,
                "percentage_of_license_text",
                self.percentage_of_license_text,
            )?;
        }
        insert_json(&mut map, "copyrights", &self.copyrights)?;
        insert_json(&mut map, "holders", &self.holders)?;
        insert_json(&mut map, "authors", &self.authors)?;
        if !self.emails.is_empty() {
            insert_json(&mut map, "emails", &self.emails)?;
        }
        insert_json(&mut map, "urls", &self.urls)?;
        insert_json(&mut map, "for_packages", &self.for_packages)?;
        insert_json(&mut map, "scan_errors", &self.scan_errors)?;
        if self.license_policy.is_some() {
            insert_json(&mut map, "license_policy", &self.license_policy)?;
        }
        if self.is_generated.is_some() {
            insert_json(&mut map, "is_generated", self.is_generated)?;
        }
        if self.source_count.is_some() {
            insert_json(&mut map, "source_count", self.source_count)?;
        }
        if self.is_legal {
            insert_json(&mut map, "is_legal", self.is_legal)?;
        }
        if self.is_manifest {
            insert_json(&mut map, "is_manifest", self.is_manifest)?;
        }
        if self.is_readme {
            insert_json(&mut map, "is_readme", self.is_readme)?;
        }
        if self.is_top_level {
            insert_json(&mut map, "is_top_level", self.is_top_level)?;
        }
        if self.is_key_file {
            insert_json(&mut map, "is_key_file", self.is_key_file)?;
        }
        if self.is_community {
            insert_json(&mut map, "is_community", self.is_community)?;
        }
        if !self.facets.is_empty() {
            insert_json(&mut map, "facets", &self.facets)?;
        }
        if self.tallies.is_some() {
            insert_json(&mut map, "tallies", &self.tallies)?;
        }

        map.serialize(serializer)
    }
}

impl From<&crate::models::FileInfo> for OutputFileInfo {
    fn from(value: &crate::models::FileInfo) -> Self {
        Self {
            name: value.name.clone(),
            base_name: value.base_name.clone(),
            extension: value.extension.clone(),
            path: value.path.clone(),
            file_type: value.file_type.clone(),
            mime_type: value.mime_type.clone(),
            file_type_label: value.file_type_label.clone(),
            size: value.size,
            date: value.date.clone(),
            sha1: value.sha1.as_ref().map(|d| d.as_hex()),
            md5: value.md5.as_ref().map(|d| d.as_hex()),
            sha256: value.sha256.as_ref().map(|d| d.as_hex()),
            sha1_git: value.sha1_git.as_ref().map(|d| d.as_hex()),
            programming_language: value.programming_language.clone(),
            package_data: value
                .package_data
                .iter()
                .map(OutputPackageData::from)
                .collect(),
            license_expression: value.license_expression.clone(),
            license_detections: value
                .license_detections
                .iter()
                .map(OutputLicenseDetection::from)
                .collect(),
            license_clues: value.license_clues.iter().map(OutputMatch::from).collect(),
            percentage_of_license_text: value.percentage_of_license_text,
            copyrights: value.copyrights.iter().map(OutputCopyright::from).collect(),
            holders: value.holders.iter().map(OutputHolder::from).collect(),
            authors: value.authors.iter().map(OutputAuthor::from).collect(),
            emails: value.emails.iter().map(OutputEmail::from).collect(),
            urls: value.urls.iter().map(OutputURL::from).collect(),
            for_packages: value
                .for_packages
                .iter()
                .map(|uid| uid.to_string())
                .collect(),
            scan_errors: value.scan_errors.clone(),
            license_policy: value
                .license_policy
                .as_ref()
                .map(|v| v.iter().map(OutputLicensePolicyEntry::from).collect()),
            is_generated: value.is_generated,
            is_binary: value.is_binary,
            is_text: value.is_text,
            is_archive: value.is_archive,
            is_media: value.is_media,
            is_source: value.is_source,
            is_script: value.is_script,
            files_count: value.files_count,
            dirs_count: value.dirs_count,
            size_count: value.size_count,
            source_count: value.source_count,
            is_legal: value.is_legal,
            is_manifest: value.is_manifest,
            is_readme: value.is_readme,
            is_top_level: value.is_top_level,
            is_key_file: value.is_key_file,
            is_community: value.is_community,
            facets: value.facets.clone(),
            tallies: value.tallies.as_ref().map(OutputTallies::from),
        }
    }
}

impl TryFrom<&OutputFileInfo> for crate::models::FileInfo {
    type Error = String;
    fn try_from(value: &OutputFileInfo) -> Result<Self, Self::Error> {
        let mut package_data = Vec::with_capacity(value.package_data.len());
        for p in &value.package_data {
            package_data.push(crate::models::PackageData::try_from(p)?);
        }
        let mut license_detections = Vec::with_capacity(value.license_detections.len());
        for d in &value.license_detections {
            license_detections.push(crate::models::LicenseDetection::try_from(d)?);
        }
        let mut license_clues = Vec::with_capacity(value.license_clues.len());
        for m in &value.license_clues {
            license_clues.push(crate::models::Match::try_from(m)?);
        }
        let mut copyrights = Vec::with_capacity(value.copyrights.len());
        for c in &value.copyrights {
            copyrights.push(crate::models::Copyright::try_from(c)?);
        }
        let mut holders = Vec::with_capacity(value.holders.len());
        for h in &value.holders {
            holders.push(crate::models::Holder::try_from(h)?);
        }
        let mut authors = Vec::with_capacity(value.authors.len());
        for a in &value.authors {
            authors.push(crate::models::Author::try_from(a)?);
        }
        let mut emails = Vec::with_capacity(value.emails.len());
        for e in &value.emails {
            emails.push(crate::models::OutputEmail::try_from(e)?);
        }
        let mut urls = Vec::with_capacity(value.urls.len());
        for u in &value.urls {
            urls.push(crate::models::OutputURL::try_from(u)?);
        }
        let license_policy = value
            .license_policy
            .as_ref()
            .map(|v| {
                v.iter()
                    .map(crate::models::LicensePolicyEntry::try_from)
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        Ok(Self {
            name: value.name.clone(),
            base_name: value.base_name.clone(),
            extension: value.extension.clone(),
            path: value.path.clone(),
            file_type: value.file_type.clone(),
            mime_type: value.mime_type.clone(),
            file_type_label: value.file_type_label.clone(),
            size: value.size,
            date: value.date.clone(),
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
            sha1_git: value
                .sha1_git
                .as_ref()
                .map(|s| crate::models::GitSha1::from_hex(s))
                .transpose()
                .map_err(|e| format!("invalid sha1_git: {}", e))?,
            programming_language: value.programming_language.clone(),
            package_data,
            license_expression: value.license_expression.clone(),
            license_detections,
            license_clues,
            percentage_of_license_text: value.percentage_of_license_text,
            copyrights,
            holders,
            authors,
            emails,
            urls,
            for_packages: value
                .for_packages
                .iter()
                .map(|s| crate::models::PackageUid::from_raw(s.clone()))
                .collect(),
            scan_errors: value.scan_errors.clone(),
            scan_diagnostics: crate::models::diagnostics_from_legacy_scan_errors(
                &value.scan_errors,
            ),
            license_policy,
            is_generated: value.is_generated,
            is_binary: value.is_binary,
            is_text: value.is_text,
            is_archive: value.is_archive,
            is_media: value.is_media,
            is_source: value.is_source,
            is_script: value.is_script,
            files_count: value.files_count,
            dirs_count: value.dirs_count,
            size_count: value.size_count,
            source_count: value.source_count,
            is_legal: value.is_legal,
            is_manifest: value.is_manifest,
            is_readme: value.is_readme,
            is_top_level: value.is_top_level,
            is_key_file: value.is_key_file,
            is_community: value.is_community,
            facets: value.facets.clone(),
            tallies: value
                .tallies
                .as_ref()
                .map(crate::models::Tallies::try_from)
                .transpose()?,
        })
    }
}
