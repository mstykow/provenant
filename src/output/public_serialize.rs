use std::collections::HashMap;

use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Serialize, Serializer};

use crate::output_schema::{
    Output, OutputDependency, OutputFileInfo, OutputFileReference, OutputPackage,
    OutputPackageData, OutputResolvedPackage, OutputTopLevelDependency,
};

pub(crate) struct PublicOutput<'a>(pub(crate) &'a Output);

impl Serialize for PublicOutput<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let output = self.0;
        let mut map = serializer.serialize_map(None)?;

        if let Some(summary) = &output.summary {
            map.serialize_entry("summary", summary)?;
        }
        if let Some(tallies) = &output.tallies {
            map.serialize_entry("tallies", tallies)?;
        }
        if let Some(tallies_of_key_files) = &output.tallies_of_key_files {
            map.serialize_entry("tallies_of_key_files", tallies_of_key_files)?;
        }
        if let Some(tallies_by_facet) = &output.tallies_by_facet {
            map.serialize_entry("tallies_by_facet", tallies_by_facet)?;
        }

        map.serialize_entry("headers", &output.headers)?;
        map.serialize_entry("packages", &PublicPackages(&output.packages))?;
        map.serialize_entry(
            "dependencies",
            &PublicTopLevelDependencies(&output.dependencies),
        )?;
        map.serialize_entry("license_detections", &output.license_detections)?;
        map.serialize_entry("files", &PublicFiles(&output.files))?;
        map.serialize_entry("license_references", &output.license_references)?;
        map.serialize_entry("license_rule_references", &output.license_rule_references)?;
        map.end()
    }
}

pub(crate) struct SingleField<T> {
    key: &'static str,
    value: T,
}

impl<T> SingleField<T> {
    pub(crate) fn new(key: &'static str, value: T) -> Self {
        Self { key, value }
    }
}

impl<T> Serialize for SingleField<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(self.key, &self.value)?;
        map.end()
    }
}

pub(crate) struct SinglePublicFile<'a>(pub(crate) &'a OutputFileInfo);

impl Serialize for SinglePublicFile<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(1))?;
        seq.serialize_element(&PublicFileInfo(self.0))?;
        seq.end()
    }
}

struct NullableMap<'a, T>(&'a Option<HashMap<String, T>>);

impl<T> Serialize for NullableMap<'_, T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.0 {
            Some(map) if !map.is_empty() => map.serialize(serializer),
            _ => serializer.serialize_none(),
        }
    }
}

struct NullableResolvedPackage<'a>(Option<&'a OutputResolvedPackage>);

impl Serialize for NullableResolvedPackage<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.0 {
            Some(package) => PublicResolvedPackage(package).serialize(serializer),
            None => serializer.serialize_none(),
        }
    }
}

pub(crate) struct PublicPackages<'a>(pub(crate) &'a [OutputPackage]);

impl Serialize for PublicPackages<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for package in self.0 {
            seq.serialize_element(&PublicPackage(package))?;
        }
        seq.end()
    }
}

struct PublicPackage<'a>(&'a OutputPackage);

#[derive(Serialize)]
struct PublicPackageFields<'a> {
    #[serde(rename = "type")]
    package_type: &'a Option<crate::models::PackageType>,
    namespace: &'a Option<String>,
    name: &'a Option<String>,
    version: &'a Option<String>,
    qualifiers: NullableMap<'a, String>,
    subpath: &'a Option<String>,
    primary_language: &'a Option<String>,
    description: &'a Option<String>,
    release_date: &'a Option<String>,
    parties: &'a [crate::output_schema::OutputParty],
    keywords: &'a [String],
    homepage_url: &'a Option<String>,
    download_url: &'a Option<String>,
    size: &'a Option<u64>,
    sha1: &'a Option<String>,
    md5: &'a Option<String>,
    sha256: &'a Option<String>,
    sha512: &'a Option<String>,
    bug_tracking_url: &'a Option<String>,
    code_view_url: &'a Option<String>,
    vcs_url: &'a Option<String>,
    copyright: &'a Option<String>,
    holder: &'a Option<String>,
    declared_license_expression: &'a Option<String>,
    declared_license_expression_spdx: &'a Option<String>,
    license_detections: &'a [crate::output_schema::OutputLicenseDetection],
    other_license_expression: &'a Option<String>,
    other_license_expression_spdx: &'a Option<String>,
    other_license_detections: &'a [crate::output_schema::OutputLicenseDetection],
    extracted_license_statement: &'a Option<String>,
    notice_text: &'a Option<String>,
    source_packages: &'a [String],
    is_private: bool,
    is_virtual: bool,
    extra_data: NullableMap<'a, serde_json::Value>,
    repository_homepage_url: &'a Option<String>,
    repository_download_url: &'a Option<String>,
    api_data_url: &'a Option<String>,
    purl: &'a Option<String>,
    package_uid: &'a str,
    datafile_paths: &'a [String],
    datasource_ids: &'a [crate::models::DatasourceId],
}

impl Serialize for PublicPackage<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let package = self.0;
        PublicPackageFields {
            package_type: &package.package_type,
            namespace: &package.namespace,
            name: &package.name,
            version: &package.version,
            qualifiers: NullableMap(&package.qualifiers),
            subpath: &package.subpath,
            primary_language: &package.primary_language,
            description: &package.description,
            release_date: &package.release_date,
            parties: &package.parties,
            keywords: &package.keywords,
            homepage_url: &package.homepage_url,
            download_url: &package.download_url,
            size: &package.size,
            sha1: &package.sha1,
            md5: &package.md5,
            sha256: &package.sha256,
            sha512: &package.sha512,
            bug_tracking_url: &package.bug_tracking_url,
            code_view_url: &package.code_view_url,
            vcs_url: &package.vcs_url,
            copyright: &package.copyright,
            holder: &package.holder,
            declared_license_expression: &package.declared_license_expression,
            declared_license_expression_spdx: &package.declared_license_expression_spdx,
            license_detections: &package.license_detections,
            other_license_expression: &package.other_license_expression,
            other_license_expression_spdx: &package.other_license_expression_spdx,
            other_license_detections: &package.other_license_detections,
            extracted_license_statement: &package.extracted_license_statement,
            notice_text: &package.notice_text,
            source_packages: &package.source_packages,
            is_private: package.is_private,
            is_virtual: package.is_virtual,
            extra_data: NullableMap(&package.extra_data),
            repository_homepage_url: &package.repository_homepage_url,
            repository_download_url: &package.repository_download_url,
            api_data_url: &package.api_data_url,
            purl: &package.purl,
            package_uid: &package.package_uid,
            datafile_paths: &package.datafile_paths,
            datasource_ids: &package.datasource_ids,
        }
        .serialize(serializer)
    }
}

struct PublicPackageDataSeq<'a>(&'a [OutputPackageData]);

impl Serialize for PublicPackageDataSeq<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for package in self.0 {
            seq.serialize_element(&PublicPackageData(package))?;
        }
        seq.end()
    }
}

struct PublicPackageData<'a>(&'a OutputPackageData);

#[derive(Serialize)]
struct PublicPackageDataFields<'a> {
    #[serde(rename = "type")]
    package_type: &'a Option<crate::models::PackageType>,
    namespace: &'a Option<String>,
    name: &'a Option<String>,
    version: &'a Option<String>,
    qualifiers: NullableMap<'a, String>,
    subpath: &'a Option<String>,
    primary_language: &'a Option<String>,
    description: &'a Option<String>,
    release_date: &'a Option<String>,
    parties: &'a [crate::output_schema::OutputParty],
    keywords: &'a [String],
    homepage_url: &'a Option<String>,
    download_url: &'a Option<String>,
    size: &'a Option<u64>,
    sha1: &'a Option<String>,
    md5: &'a Option<String>,
    sha256: &'a Option<String>,
    sha512: &'a Option<String>,
    bug_tracking_url: &'a Option<String>,
    code_view_url: &'a Option<String>,
    vcs_url: &'a Option<String>,
    copyright: &'a Option<String>,
    holder: &'a Option<String>,
    declared_license_expression: &'a Option<String>,
    declared_license_expression_spdx: &'a Option<String>,
    license_detections: &'a [crate::output_schema::OutputLicenseDetection],
    other_license_expression: &'a Option<String>,
    other_license_expression_spdx: &'a Option<String>,
    other_license_detections: &'a [crate::output_schema::OutputLicenseDetection],
    extracted_license_statement: &'a Option<String>,
    notice_text: &'a Option<String>,
    source_packages: &'a [String],
    file_references: PublicFileReferences<'a>,
    is_private: bool,
    is_virtual: bool,
    extra_data: NullableMap<'a, serde_json::Value>,
    dependencies: PublicDependencies<'a>,
    repository_homepage_url: &'a Option<String>,
    repository_download_url: &'a Option<String>,
    api_data_url: &'a Option<String>,
    datasource_id: &'a Option<crate::models::DatasourceId>,
    purl: &'a Option<String>,
}

impl Serialize for PublicPackageData<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let package = self.0;
        PublicPackageDataFields {
            package_type: &package.package_type,
            namespace: &package.namespace,
            name: &package.name,
            version: &package.version,
            qualifiers: NullableMap(&package.qualifiers),
            subpath: &package.subpath,
            primary_language: &package.primary_language,
            description: &package.description,
            release_date: &package.release_date,
            parties: &package.parties,
            keywords: &package.keywords,
            homepage_url: &package.homepage_url,
            download_url: &package.download_url,
            size: &package.size,
            sha1: &package.sha1,
            md5: &package.md5,
            sha256: &package.sha256,
            sha512: &package.sha512,
            bug_tracking_url: &package.bug_tracking_url,
            code_view_url: &package.code_view_url,
            vcs_url: &package.vcs_url,
            copyright: &package.copyright,
            holder: &package.holder,
            declared_license_expression: &package.declared_license_expression,
            declared_license_expression_spdx: &package.declared_license_expression_spdx,
            license_detections: &package.license_detections,
            other_license_expression: &package.other_license_expression,
            other_license_expression_spdx: &package.other_license_expression_spdx,
            other_license_detections: &package.other_license_detections,
            extracted_license_statement: &package.extracted_license_statement,
            notice_text: &package.notice_text,
            source_packages: &package.source_packages,
            file_references: PublicFileReferences(&package.file_references),
            is_private: package.is_private,
            is_virtual: package.is_virtual,
            extra_data: NullableMap(&package.extra_data),
            dependencies: PublicDependencies(&package.dependencies),
            repository_homepage_url: &package.repository_homepage_url,
            repository_download_url: &package.repository_download_url,
            api_data_url: &package.api_data_url,
            datasource_id: &package.datasource_id,
            purl: &package.purl,
        }
        .serialize(serializer)
    }
}

struct PublicResolvedPackage<'a>(&'a OutputResolvedPackage);

#[derive(Serialize)]
struct PublicResolvedPackageFields<'a> {
    #[serde(rename = "type")]
    package_type: &'a crate::models::PackageType,
    namespace: &'a String,
    name: &'a String,
    version: &'a String,
    qualifiers: NullableMap<'a, String>,
    subpath: &'a Option<String>,
    primary_language: &'a Option<String>,
    description: &'a Option<String>,
    release_date: &'a Option<String>,
    parties: &'a [crate::output_schema::OutputParty],
    keywords: &'a [String],
    homepage_url: &'a Option<String>,
    download_url: &'a Option<String>,
    size: &'a Option<u64>,
    sha1: &'a Option<String>,
    md5: &'a Option<String>,
    sha256: &'a Option<String>,
    sha512: &'a Option<String>,
    bug_tracking_url: &'a Option<String>,
    code_view_url: &'a Option<String>,
    vcs_url: &'a Option<String>,
    copyright: &'a Option<String>,
    holder: &'a Option<String>,
    declared_license_expression: &'a Option<String>,
    declared_license_expression_spdx: &'a Option<String>,
    license_detections: &'a [crate::output_schema::OutputLicenseDetection],
    other_license_expression: &'a Option<String>,
    other_license_expression_spdx: &'a Option<String>,
    other_license_detections: &'a [crate::output_schema::OutputLicenseDetection],
    extracted_license_statement: &'a Option<String>,
    notice_text: &'a Option<String>,
    source_packages: &'a [String],
    file_references: PublicFileReferences<'a>,
    is_private: bool,
    is_virtual: bool,
    extra_data: NullableMap<'a, serde_json::Value>,
    dependencies: PublicDependencies<'a>,
    repository_homepage_url: &'a Option<String>,
    repository_download_url: &'a Option<String>,
    api_data_url: &'a Option<String>,
    datasource_id: &'a Option<crate::models::DatasourceId>,
    purl: &'a Option<String>,
}

impl Serialize for PublicResolvedPackage<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let package = self.0;
        PublicResolvedPackageFields {
            package_type: &package.package_type,
            namespace: &package.namespace,
            name: &package.name,
            version: &package.version,
            qualifiers: NullableMap(&package.qualifiers),
            subpath: &package.subpath,
            primary_language: &package.primary_language,
            description: &package.description,
            release_date: &package.release_date,
            parties: &package.parties,
            keywords: &package.keywords,
            homepage_url: &package.homepage_url,
            download_url: &package.download_url,
            size: &package.size,
            sha1: &package.sha1,
            md5: &package.md5,
            sha256: &package.sha256,
            sha512: &package.sha512,
            bug_tracking_url: &package.bug_tracking_url,
            code_view_url: &package.code_view_url,
            vcs_url: &package.vcs_url,
            copyright: &package.copyright,
            holder: &package.holder,
            declared_license_expression: &package.declared_license_expression,
            declared_license_expression_spdx: &package.declared_license_expression_spdx,
            license_detections: &package.license_detections,
            other_license_expression: &package.other_license_expression,
            other_license_expression_spdx: &package.other_license_expression_spdx,
            other_license_detections: &package.other_license_detections,
            extracted_license_statement: &package.extracted_license_statement,
            notice_text: &package.notice_text,
            source_packages: &package.source_packages,
            file_references: PublicFileReferences(&package.file_references),
            is_private: package.is_private,
            is_virtual: package.is_virtual,
            extra_data: NullableMap(&package.extra_data),
            dependencies: PublicDependencies(&package.dependencies),
            repository_homepage_url: &package.repository_homepage_url,
            repository_download_url: &package.repository_download_url,
            api_data_url: &package.api_data_url,
            datasource_id: &package.datasource_id,
            purl: &package.purl,
        }
        .serialize(serializer)
    }
}

pub(crate) struct PublicTopLevelDependencies<'a>(pub(crate) &'a [OutputTopLevelDependency]);

impl Serialize for PublicTopLevelDependencies<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for dependency in self.0 {
            seq.serialize_element(&PublicTopLevelDependency(dependency))?;
        }
        seq.end()
    }
}

struct PublicTopLevelDependency<'a>(&'a OutputTopLevelDependency);

#[derive(Serialize)]
struct PublicTopLevelDependencyFields<'a> {
    purl: &'a Option<String>,
    extracted_requirement: &'a Option<String>,
    scope: &'a Option<String>,
    is_runtime: &'a Option<bool>,
    is_optional: &'a Option<bool>,
    is_pinned: &'a Option<bool>,
    is_direct: &'a Option<bool>,
    resolved_package: NullableResolvedPackage<'a>,
    extra_data: NullableMap<'a, serde_json::Value>,
    dependency_uid: &'a str,
    for_package_uid: &'a Option<String>,
    datafile_path: &'a str,
    datasource_id: &'a crate::models::DatasourceId,
    namespace: &'a Option<String>,
}

impl Serialize for PublicTopLevelDependency<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let dependency = self.0;
        PublicTopLevelDependencyFields {
            purl: &dependency.purl,
            extracted_requirement: &dependency.extracted_requirement,
            scope: &dependency.scope,
            is_runtime: &dependency.is_runtime,
            is_optional: &dependency.is_optional,
            is_pinned: &dependency.is_pinned,
            is_direct: &dependency.is_direct,
            resolved_package: NullableResolvedPackage(
                dependency
                    .resolved_package
                    .as_ref()
                    .map(|package| package.as_ref()),
            ),
            extra_data: NullableMap(&dependency.extra_data),
            dependency_uid: &dependency.dependency_uid,
            for_package_uid: &dependency.for_package_uid,
            datafile_path: &dependency.datafile_path,
            datasource_id: &dependency.datasource_id,
            namespace: &dependency.namespace,
        }
        .serialize(serializer)
    }
}

struct PublicDependencies<'a>(&'a [OutputDependency]);

impl Serialize for PublicDependencies<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for dependency in self.0 {
            seq.serialize_element(&PublicDependency(dependency))?;
        }
        seq.end()
    }
}

struct PublicDependency<'a>(&'a OutputDependency);

#[derive(Serialize)]
struct PublicDependencyFields<'a> {
    purl: &'a Option<String>,
    extracted_requirement: &'a Option<String>,
    scope: &'a Option<String>,
    is_runtime: &'a Option<bool>,
    is_optional: &'a Option<bool>,
    is_pinned: &'a Option<bool>,
    is_direct: &'a Option<bool>,
    resolved_package: NullableResolvedPackage<'a>,
    extra_data: NullableMap<'a, serde_json::Value>,
}

impl Serialize for PublicDependency<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let dependency = self.0;
        PublicDependencyFields {
            purl: &dependency.purl,
            extracted_requirement: &dependency.extracted_requirement,
            scope: &dependency.scope,
            is_runtime: &dependency.is_runtime,
            is_optional: &dependency.is_optional,
            is_pinned: &dependency.is_pinned,
            is_direct: &dependency.is_direct,
            resolved_package: NullableResolvedPackage(
                dependency
                    .resolved_package
                    .as_ref()
                    .map(|package| package.as_ref()),
            ),
            extra_data: NullableMap(&dependency.extra_data),
        }
        .serialize(serializer)
    }
}

struct PublicFileReferences<'a>(&'a [OutputFileReference]);

impl Serialize for PublicFileReferences<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for file_reference in self.0 {
            seq.serialize_element(&PublicFileReference(file_reference))?;
        }
        seq.end()
    }
}

struct PublicFileReference<'a>(&'a OutputFileReference);

#[derive(Serialize)]
struct PublicFileReferenceFields<'a> {
    path: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<u64>,
    sha1: &'a Option<String>,
    md5: &'a Option<String>,
    sha256: &'a Option<String>,
    sha512: &'a Option<String>,
    extra_data: NullableMap<'a, serde_json::Value>,
}

impl Serialize for PublicFileReference<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let file_reference = self.0;
        PublicFileReferenceFields {
            path: &file_reference.path,
            size: file_reference.size,
            sha1: &file_reference.sha1,
            md5: &file_reference.md5,
            sha256: &file_reference.sha256,
            sha512: &file_reference.sha512,
            extra_data: NullableMap(&file_reference.extra_data),
        }
        .serialize(serializer)
    }
}

struct PublicFiles<'a>(&'a [OutputFileInfo]);

impl Serialize for PublicFiles<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for file in self.0 {
            seq.serialize_element(&PublicFileInfo(file))?;
        }
        seq.end()
    }
}

struct PublicFileInfo<'a>(&'a OutputFileInfo);

impl Serialize for PublicFileInfo<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let file = self.0;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("path", &file.path)?;
        map.serialize_entry("type", &file.file_type)?;
        map.serialize_entry("name", &file.name)?;
        map.serialize_entry("base_name", &file.base_name)?;
        map.serialize_entry("extension", &file.extension)?;
        map.serialize_entry("size", &file.size)?;

        if file.should_serialize_info_surface() {
            map.serialize_entry("date", &file.date)?;
            map.serialize_entry("sha1", &file.sha1)?;
            map.serialize_entry("md5", &file.md5)?;
            map.serialize_entry("sha256", &file.sha256)?;
            map.serialize_entry("sha1_git", &file.sha1_git)?;
            map.serialize_entry("mime_type", &file.mime_type)?;
            map.serialize_entry("file_type", &file.file_type_label)?;
            map.serialize_entry("programming_language", &file.programming_language)?;
            map.serialize_entry("is_binary", &file.is_binary)?;
            map.serialize_entry("is_text", &file.is_text)?;
            map.serialize_entry("is_archive", &file.is_archive)?;
            map.serialize_entry("is_media", &file.is_media)?;
            map.serialize_entry("is_source", &file.is_source)?;
            map.serialize_entry("is_script", &file.is_script)?;
            map.serialize_entry("files_count", &file.files_count)?;
            map.serialize_entry("dirs_count", &file.dirs_count)?;
            map.serialize_entry("size_count", &file.size_count)?;
        }

        map.serialize_entry("package_data", &PublicPackageDataSeq(&file.package_data))?;
        map.serialize_entry(
            "detected_license_expression_spdx",
            &file.detected_license_expression_spdx(),
        )?;
        map.serialize_entry("license_detections", &file.license_detections)?;
        if file.should_serialize_license_surface() {
            map.serialize_entry("license_clues", &file.license_clues)?;
        }
        if file.percentage_of_license_text.is_some() {
            map.serialize_entry(
                "percentage_of_license_text",
                &file.percentage_of_license_text,
            )?;
        }
        map.serialize_entry("copyrights", &file.copyrights)?;
        map.serialize_entry("holders", &file.holders)?;
        map.serialize_entry("authors", &file.authors)?;
        if !file.emails.is_empty() {
            map.serialize_entry("emails", &file.emails)?;
        }
        map.serialize_entry("urls", &file.urls)?;
        map.serialize_entry("for_packages", &file.for_packages)?;
        map.serialize_entry("scan_errors", &file.scan_errors)?;
        if file.license_policy.is_some() {
            map.serialize_entry("license_policy", &file.license_policy)?;
        }
        if file.is_generated.is_some() {
            map.serialize_entry("is_generated", &file.is_generated)?;
        }
        if file.source_count.is_some() {
            map.serialize_entry("source_count", &file.source_count)?;
        }
        if file.is_legal {
            map.serialize_entry("is_legal", &file.is_legal)?;
        }
        if file.is_manifest {
            map.serialize_entry("is_manifest", &file.is_manifest)?;
        }
        if file.is_readme {
            map.serialize_entry("is_readme", &file.is_readme)?;
        }
        if file.is_top_level {
            map.serialize_entry("is_top_level", &file.is_top_level)?;
        }
        if file.is_key_file {
            map.serialize_entry("is_key_file", &file.is_key_file)?;
        }
        if file.is_community {
            map.serialize_entry("is_community", &file.is_community)?;
        }
        if !file.facets.is_empty() {
            map.serialize_entry("facets", &file.facets)?;
        }
        if file.tallies.is_some() {
            map.serialize_entry("tallies", &file.tallies)?;
        }

        map.end()
    }
}
