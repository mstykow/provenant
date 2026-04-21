// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use derive_builder::Builder;
use packageurl::PackageUrl;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::str::FromStr;

use super::DatasourceId;
use super::DependencyUid;
use super::DiagnosticSeverity;
use super::GitSha1;
use super::LineNumber;
use super::MatchScore;
use super::Md5Digest;
use super::PackageType;
use super::PackageUid;
use super::ScanDiagnostic;
use super::Sha1Digest;
use super::Sha256Digest;
use super::Sha512Digest;
use super::diagnostics_from_legacy_scan_errors;
use crate::license_detection::tokenize::tokenize_without_stopwords;
use crate::models::output::Tallies;
use crate::utils::spdx::combine_license_expressions;

#[derive(Debug, Builder, Serialize, Deserialize, Clone)]
#[builder(build_fn(skip))]
/// File-level scan result containing metadata and detected findings.
pub struct FileInfo {
    pub name: String,
    pub base_name: String,
    pub extension: String,
    pub path: String,
    #[serde(rename = "type")] // name used by ScanCode
    pub file_type: FileType,
    #[builder(default)]
    #[serde(default)]
    pub mime_type: Option<String>,
    #[builder(default)]
    #[serde(rename = "file_type", default)]
    pub file_type_label: Option<String>,
    pub size: u64,
    #[builder(default)]
    #[serde(default)]
    pub date: Option<String>,
    #[builder(default)]
    #[serde(default)]
    pub sha1: Option<Sha1Digest>,
    #[builder(default)]
    #[serde(default)]
    pub md5: Option<Md5Digest>,
    #[builder(default)]
    #[serde(default)]
    pub sha256: Option<Sha256Digest>,
    #[builder(default)]
    #[serde(default)]
    pub sha1_git: Option<GitSha1>,
    #[builder(default)]
    #[serde(default)]
    pub programming_language: Option<String>,
    #[builder(default)]
    #[serde(default)]
    pub package_data: Vec<PackageData>,
    #[serde(rename = "detected_license_expression_spdx")] // name used by ScanCode
    #[builder(default)]
    pub license_expression: Option<String>,
    #[builder(default)]
    #[serde(default)]
    pub license_detections: Vec<LicenseDetection>,
    #[builder(default)]
    #[serde(default)]
    pub license_clues: Vec<Match>,
    #[builder(default)]
    #[serde(default)]
    pub percentage_of_license_text: Option<f64>,
    #[builder(default)]
    #[serde(default)]
    pub copyrights: Vec<Copyright>,
    #[builder(default)]
    #[serde(default)]
    pub holders: Vec<Holder>,
    #[builder(default)]
    #[serde(default)]
    pub authors: Vec<Author>,
    #[builder(default)]
    #[serde(default)]
    pub emails: Vec<OutputEmail>,
    #[builder(default)]
    #[serde(default)]
    pub urls: Vec<OutputURL>,
    #[builder(default)]
    #[serde(default)]
    pub for_packages: Vec<PackageUid>,
    #[builder(default)]
    #[serde(default)]
    pub scan_errors: Vec<String>,
    #[builder(default)]
    #[serde(default)]
    pub scan_diagnostics: Vec<ScanDiagnostic>,
    #[builder(default)]
    #[serde(default)]
    pub license_policy: Option<Vec<LicensePolicyEntry>>,
    #[builder(default)]
    #[serde(default)]
    pub is_generated: Option<bool>,
    #[builder(default)]
    #[serde(default)]
    pub is_binary: Option<bool>,
    #[builder(default)]
    #[serde(default)]
    pub is_text: Option<bool>,
    #[builder(default)]
    #[serde(default)]
    pub is_archive: Option<bool>,
    #[builder(default)]
    #[serde(default)]
    pub is_media: Option<bool>,
    #[builder(default)]
    #[serde(default)]
    pub is_source: Option<bool>,
    #[builder(default)]
    #[serde(default)]
    pub is_script: Option<bool>,
    #[builder(default)]
    #[serde(default)]
    pub files_count: Option<usize>,
    #[builder(default)]
    #[serde(default)]
    pub dirs_count: Option<usize>,
    #[builder(default)]
    #[serde(default)]
    pub size_count: Option<u64>,
    #[builder(default)]
    #[serde(default)]
    pub source_count: Option<usize>,
    #[builder(default)]
    #[serde(default)]
    pub is_legal: bool,
    #[builder(default)]
    #[serde(default)]
    pub is_manifest: bool,
    #[builder(default)]
    #[serde(default)]
    pub is_readme: bool,
    #[builder(default)]
    #[serde(default)]
    pub is_top_level: bool,
    #[builder(default)]
    #[serde(default)]
    pub is_key_file: bool,
    #[builder(default)]
    #[serde(default)]
    pub is_community: bool,
    #[builder(default)]
    #[serde(default)]
    pub facets: Vec<String>,
    #[builder(default)]
    #[serde(default)]
    pub tallies: Option<Tallies>,
}

impl FileInfoBuilder {
    /// Build a [`FileInfo`] from the current builder state.
    pub fn build(&self) -> Result<FileInfo, String> {
        let mut file_info = FileInfo::new(
            self.name.clone().ok_or("Missing field: name")?,
            self.base_name.clone().ok_or("Missing field: base_name")?,
            self.extension.clone().ok_or("Missing field: extension")?,
            self.path.clone().ok_or("Missing field: path")?,
            self.file_type.clone().ok_or("Missing field: file_type")?,
            self.mime_type.clone().flatten(),
            self.file_type_label.clone().flatten(),
            self.size.ok_or("Missing field: size")?,
            self.date.clone().flatten(),
            self.sha1.flatten(),
            self.md5.flatten(),
            self.sha256.flatten(),
            self.programming_language.clone().flatten(),
            self.package_data.clone().unwrap_or_default(),
            self.license_expression.clone().flatten(),
            self.license_detections.clone().unwrap_or_default(),
            self.license_clues.clone().unwrap_or_default(),
            self.copyrights.clone().unwrap_or_default(),
            self.holders.clone().unwrap_or_default(),
            self.authors.clone().unwrap_or_default(),
            self.emails.clone().unwrap_or_default(),
            self.urls.clone().unwrap_or_default(),
            self.for_packages.clone().unwrap_or_default(),
            self.scan_errors.clone().unwrap_or_default(),
        );
        file_info.scan_diagnostics = if let Some(diagnostics) = &self.scan_diagnostics {
            diagnostics.clone()
        } else {
            diagnostics_from_legacy_scan_errors(&file_info.scan_errors)
        };
        file_info.scan_errors = file_info
            .scan_diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.clone())
            .collect();
        file_info.license_policy = self.license_policy.clone().flatten();
        file_info.sha1_git = self.sha1_git.flatten();
        file_info.is_binary = self.is_binary.flatten();
        file_info.is_text = self.is_text.flatten();
        file_info.is_archive = self.is_archive.flatten();
        file_info.is_media = self.is_media.flatten();
        file_info.is_script = self.is_script.flatten();
        file_info.files_count = self.files_count.flatten();
        file_info.dirs_count = self.dirs_count.flatten();
        file_info.size_count = self.size_count.flatten();
        Ok(file_info)
    }
}

impl FileInfo {
    #[allow(clippy::too_many_arguments)]
    /// Construct a [`FileInfo`] from fully resolved scanner fields.
    pub fn new(
        name: String,
        base_name: String,
        extension: String,
        path: String,
        file_type: FileType,
        mime_type: Option<String>,
        file_type_label: Option<String>,
        size: u64,
        date: Option<String>,
        sha1: Option<Sha1Digest>,
        md5: Option<Md5Digest>,
        sha256: Option<Sha256Digest>,
        programming_language: Option<String>,
        package_data: Vec<PackageData>,
        mut license_expression: Option<String>,
        mut license_detections: Vec<LicenseDetection>,
        license_clues: Vec<Match>,
        copyrights: Vec<Copyright>,
        holders: Vec<Holder>,
        authors: Vec<Author>,
        emails: Vec<OutputEmail>,
        urls: Vec<OutputURL>,
        for_packages: Vec<PackageUid>,
        scan_errors: Vec<String>,
    ) -> Self {
        let mut package_data = package_data;
        for package in &mut package_data {
            enrich_package_data_license_provenance(package, &path);
        }

        // Combine license expressions from package data if license_expression is None
        license_expression = license_expression.or_else(|| {
            let expressions = package_data
                .iter()
                .filter_map(|pkg| pkg.get_license_expression());
            combine_license_expressions(expressions)
        });

        // Combine license detections from package data if none are provided
        if license_detections.is_empty() {
            for pkg in &package_data {
                license_detections.extend(pkg.license_detections.clone());
            }
        }

        // Combine license expressions from license detections if license_expression is still None
        if license_expression.is_none() && !license_detections.is_empty() {
            let expressions = license_detections
                .iter()
                .map(|detection| detection.license_expression.clone());
            license_expression = combine_license_expressions(expressions);
        }

        let mut file_info = FileInfo {
            name,
            base_name,
            extension,
            path,
            file_type,
            mime_type,
            file_type_label,
            size,
            date,
            sha1,
            md5,
            sha256,
            sha1_git: None,
            programming_language,
            package_data,
            license_expression,
            license_detections,
            license_clues,
            percentage_of_license_text: None,
            copyrights,
            holders,
            authors,
            emails,
            urls,
            for_packages,
            scan_diagnostics: diagnostics_from_legacy_scan_errors(&scan_errors),
            scan_errors,
            license_policy: None,
            is_generated: None,
            is_binary: None,
            is_text: None,
            is_archive: None,
            is_media: None,
            is_source: None,
            is_script: None,
            files_count: None,
            dirs_count: None,
            size_count: None,
            source_count: None,
            is_legal: false,
            is_manifest: false,
            is_readme: false,
            is_top_level: false,
            is_key_file: false,
            is_community: false,
            facets: vec![],
            tallies: None,
        };

        file_info.backfill_license_provenance();
        file_info
    }

    pub fn backfill_license_provenance(&mut self) {
        for detection in &mut self.license_detections {
            enrich_license_detection_provenance(detection, &self.path);
        }

        for package in &mut self.package_data {
            enrich_package_data_license_provenance(package, &self.path);
        }
    }
}

impl FileInfo {
    pub fn warning_diagnostics(&self) -> impl Iterator<Item = &ScanDiagnostic> {
        self.scan_diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Warning)
    }

    pub fn error_diagnostics(&self) -> impl Iterator<Item = &ScanDiagnostic> {
        self.scan_diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    }
}

fn enrich_package_data_license_provenance(package_data: &mut PackageData, path: &str) {
    for detection in &mut package_data.license_detections {
        enrich_license_detection_provenance(detection, path);
    }
    for detection in &mut package_data.other_license_detections {
        enrich_license_detection_provenance(detection, path);
    }
}

pub(crate) fn enrich_license_detection_provenance(detection: &mut LicenseDetection, path: &str) {
    for detection_match in &mut detection.matches {
        if detection_match.from_file.is_none() {
            detection_match.from_file = Some(path.to_string());
        }

        if detection_match.rule_identifier.is_none() {
            detection_match.rule_identifier = detection_match.matcher.clone();
        }
    }

    if detection.identifier.is_none() {
        detection.identifier = Some(compute_public_detection_identifier(detection));
    }
}

fn compute_public_detection_identifier(detection: &LicenseDetection) -> String {
    let expression = python_safe_name(&detection.license_expression);
    let mut hasher = Sha1::new();
    hasher.update(format_public_detection_content(detection).as_bytes());
    let hex_str = hex::encode(hasher.finalize());
    let uuid_hex = &hex_str[..32];
    let content_uuid = uuid::Uuid::parse_str(uuid_hex)
        .map(|uuid| uuid.to_string())
        .unwrap_or_else(|_| uuid_hex.to_string());

    format!("{}-{}", expression, content_uuid)
}

fn format_public_detection_content(detection: &LicenseDetection) -> String {
    let mut result = String::from("(");

    for (index, detection_match) in detection.matches.iter().enumerate() {
        if index > 0 {
            result.push_str(", ");
        }
        result.push_str(&format!(
            "({}, {}, {})",
            python_str_repr(
                detection_match
                    .rule_identifier
                    .as_deref()
                    .or(detection_match.matcher.as_deref())
                    .unwrap_or("parser-declared-license")
            ),
            detection_match.score.value() as f32,
            python_token_tuple_repr(&tokenize_without_stopwords(
                detection_match.matched_text.as_deref().unwrap_or_default(),
            )),
        ));
    }

    if detection.matches.len() == 1 {
        result.push(',');
    }
    result.push(')');
    result
}

fn python_safe_name(value: &str) -> String {
    let mut result = String::new();
    let mut prev_underscore = false;

    for character in value.chars() {
        if character.is_alphanumeric() {
            result.push(character);
            prev_underscore = false;
        } else if !prev_underscore {
            result.push('_');
            prev_underscore = true;
        }
    }

    let trimmed = result.trim_matches('_');
    if trimmed.is_empty() {
        String::new()
    } else {
        trimmed.to_string()
    }
}

fn python_str_repr(value: &str) -> String {
    if value.contains('\'') && !value.contains('"') {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\\'"))
    }
}

fn python_token_tuple_repr(tokens: &[String]) -> String {
    if tokens.is_empty() {
        return String::from("()");
    }

    let mut result = String::from("(");
    for (index, token) in tokens.iter().enumerate() {
        if index > 0 {
            result.push_str(", ");
        }
        result.push_str(&python_str_repr(token));
    }

    if tokens.len() == 1 {
        result.push(',');
    }
    result.push(')');
    result
}

/// Package metadata extracted from manifest files.
///
/// Compatible with ScanCode Toolkit output format. Contains standardized package
/// information including name, version, dependencies, licenses, and other metadata.
/// This is the primary data structure returned by all parsers.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct PackageData {
    #[serde(rename = "type")] // name used by ScanCode
    pub package_type: Option<PackageType>,
    pub namespace: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    #[serde(default)]
    pub qualifiers: Option<HashMap<String, String>>,
    pub subpath: Option<String>,
    pub primary_language: Option<String>,
    pub description: Option<String>,
    pub release_date: Option<String>,
    #[serde(default)]
    pub parties: Vec<Party>,
    #[serde(default)]
    pub keywords: Vec<String>,
    pub homepage_url: Option<String>,
    pub download_url: Option<String>,
    pub size: Option<u64>,
    pub sha1: Option<Sha1Digest>,
    pub md5: Option<Md5Digest>,
    pub sha256: Option<Sha256Digest>,
    pub sha512: Option<Sha512Digest>,
    pub bug_tracking_url: Option<String>,
    pub code_view_url: Option<String>,
    pub vcs_url: Option<String>,
    pub copyright: Option<String>,
    pub holder: Option<String>,
    pub declared_license_expression: Option<String>,
    pub declared_license_expression_spdx: Option<String>,
    #[serde(default)]
    pub license_detections: Vec<LicenseDetection>,
    pub other_license_expression: Option<String>,
    pub other_license_expression_spdx: Option<String>,
    #[serde(default)]
    pub other_license_detections: Vec<LicenseDetection>,
    pub extracted_license_statement: Option<String>,
    pub notice_text: Option<String>,
    #[serde(default)]
    pub source_packages: Vec<String>,
    #[serde(default)]
    pub file_references: Vec<FileReference>,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default)]
    pub is_virtual: bool,
    #[serde(default)]
    pub extra_data: Option<HashMap<String, serde_json::Value>>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    pub repository_homepage_url: Option<String>,
    pub repository_download_url: Option<String>,
    pub api_data_url: Option<String>,
    pub datasource_id: Option<DatasourceId>,
    pub purl: Option<String>,
}

impl PackageData {
    /// Extracts a single license expression from all license detections in this package.
    /// Returns None if there are no license detections.
    pub fn get_license_expression(&self) -> Option<String> {
        if self.license_detections.is_empty() {
            return None;
        }

        let expressions = self
            .license_detections
            .iter()
            .map(|detection| detection.license_expression.clone());
        combine_license_expressions(expressions)
    }
}

/// License detection result containing matched license expressions.
///
/// Aggregates multiple license matches into a single SPDX license expression.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct LicenseDetection {
    pub license_expression: String,
    pub license_expression_spdx: String,
    pub matches: Vec<Match>,
    #[serde(default)]
    pub detection_log: Vec<String>,
    pub identifier: Option<String>,
}

/// Individual license text match with location and confidence score.
///
/// Represents a specific region of text that matched a known license pattern.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Match {
    pub license_expression: String,
    pub license_expression_spdx: String,
    pub from_file: Option<String>,
    pub start_line: LineNumber,
    pub end_line: LineNumber,
    pub matcher: Option<String>,
    pub score: MatchScore,
    pub matched_length: Option<usize>,
    pub match_coverage: Option<f64>,
    pub rule_relevance: Option<u8>,
    pub rule_identifier: Option<String>,
    pub rule_url: Option<String>,
    pub matched_text: Option<String>,
    pub matched_text_diagnostics: Option<String>,
    #[serde(default)]
    pub referenced_filenames: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Copyright {
    pub copyright: String,
    pub start_line: LineNumber,
    pub end_line: LineNumber,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Holder {
    pub holder: String,
    pub start_line: LineNumber,
    pub end_line: LineNumber,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Author {
    pub author: String,
    pub start_line: LineNumber,
    pub end_line: LineNumber,
}

/// Package dependency information with version constraints.
///
/// Represents a declared dependency with scope (e.g., runtime, dev, optional)
/// and optional resolved package details.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Dependency {
    pub purl: Option<String>,
    pub extracted_requirement: Option<String>,
    pub scope: Option<String>,
    pub is_runtime: Option<bool>,
    pub is_optional: Option<bool>,
    pub is_pinned: Option<bool>,
    pub is_direct: Option<bool>,
    pub resolved_package: Option<Box<ResolvedPackage>>,
    #[serde(default)]
    pub extra_data: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ResolvedPackage {
    #[serde(rename = "type")]
    pub package_type: PackageType,
    pub namespace: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub qualifiers: Option<HashMap<String, String>>,
    pub subpath: Option<String>,
    pub primary_language: Option<String>,
    pub description: Option<String>,
    pub release_date: Option<String>,
    #[serde(default)]
    pub parties: Vec<Party>,
    #[serde(default)]
    pub keywords: Vec<String>,
    pub homepage_url: Option<String>,
    pub download_url: Option<String>,
    pub size: Option<u64>,
    pub sha1: Option<Sha1Digest>,
    pub md5: Option<Md5Digest>,
    pub sha256: Option<Sha256Digest>,
    pub sha512: Option<Sha512Digest>,
    pub bug_tracking_url: Option<String>,
    pub code_view_url: Option<String>,
    pub vcs_url: Option<String>,
    pub copyright: Option<String>,
    pub holder: Option<String>,
    pub declared_license_expression: Option<String>,
    pub declared_license_expression_spdx: Option<String>,
    #[serde(default)]
    pub license_detections: Vec<LicenseDetection>,
    pub other_license_expression: Option<String>,
    pub other_license_expression_spdx: Option<String>,
    #[serde(default)]
    pub other_license_detections: Vec<LicenseDetection>,
    pub extracted_license_statement: Option<String>,
    pub notice_text: Option<String>,
    #[serde(default)]
    pub source_packages: Vec<String>,
    #[serde(default)]
    pub file_references: Vec<FileReference>,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default)]
    pub is_virtual: bool,
    #[serde(default)]
    pub extra_data: Option<HashMap<String, serde_json::Value>>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    pub repository_homepage_url: Option<String>,
    pub repository_download_url: Option<String>,
    pub api_data_url: Option<String>,
    pub datasource_id: Option<DatasourceId>,
    pub purl: Option<String>,
}

impl ResolvedPackage {
    pub fn new(
        package_type: PackageType,
        namespace: String,
        name: String,
        version: String,
    ) -> Self {
        Self {
            package_type,
            namespace,
            name,
            version,
            qualifiers: None,
            subpath: None,
            primary_language: None,
            description: None,
            release_date: None,
            parties: vec![],
            keywords: vec![],
            homepage_url: None,
            download_url: None,
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            bug_tracking_url: None,
            code_view_url: None,
            vcs_url: None,
            copyright: None,
            holder: None,
            declared_license_expression: None,
            declared_license_expression_spdx: None,
            license_detections: vec![],
            other_license_expression: None,
            other_license_expression_spdx: None,
            other_license_detections: vec![],
            extracted_license_statement: None,
            notice_text: None,
            source_packages: vec![],
            file_references: vec![],
            is_private: false,
            is_virtual: false,
            extra_data: None,
            dependencies: vec![],
            repository_homepage_url: None,
            repository_download_url: None,
            api_data_url: None,
            datasource_id: None,
            purl: None,
        }
    }

    pub fn from_package_data(package_data: &PackageData, fallback_type: PackageType) -> Self {
        Self {
            package_type: package_data.package_type.unwrap_or(fallback_type),
            namespace: package_data.namespace.clone().unwrap_or_default(),
            name: package_data.name.clone().unwrap_or_default(),
            version: package_data.version.clone().unwrap_or_default(),
            qualifiers: package_data.qualifiers.clone(),
            subpath: package_data.subpath.clone(),
            primary_language: package_data.primary_language.clone(),
            description: package_data.description.clone(),
            release_date: package_data.release_date.clone(),
            parties: package_data.parties.clone(),
            keywords: package_data.keywords.clone(),
            homepage_url: package_data.homepage_url.clone(),
            download_url: package_data.download_url.clone(),
            size: package_data.size,
            sha1: package_data.sha1,
            md5: package_data.md5,
            sha256: package_data.sha256,
            sha512: package_data.sha512,
            bug_tracking_url: package_data.bug_tracking_url.clone(),
            code_view_url: package_data.code_view_url.clone(),
            vcs_url: package_data.vcs_url.clone(),
            copyright: package_data.copyright.clone(),
            holder: package_data.holder.clone(),
            declared_license_expression: package_data.declared_license_expression.clone(),
            declared_license_expression_spdx: package_data.declared_license_expression_spdx.clone(),
            license_detections: package_data.license_detections.clone(),
            other_license_expression: package_data.other_license_expression.clone(),
            other_license_expression_spdx: package_data.other_license_expression_spdx.clone(),
            other_license_detections: package_data.other_license_detections.clone(),
            extracted_license_statement: package_data.extracted_license_statement.clone(),
            notice_text: package_data.notice_text.clone(),
            source_packages: package_data.source_packages.clone(),
            file_references: package_data.file_references.clone(),
            is_private: package_data.is_private,
            is_virtual: package_data.is_virtual,
            extra_data: package_data.extra_data.clone(),
            dependencies: package_data.dependencies.clone(),
            repository_homepage_url: package_data.repository_homepage_url.clone(),
            repository_download_url: package_data.repository_download_url.clone(),
            api_data_url: package_data.api_data_url.clone(),
            datasource_id: package_data.datasource_id,
            purl: package_data.purl.clone(),
        }
    }
}

/// Author, maintainer, or contributor information.
///
/// Represents a person or organization associated with a package.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Party {
    pub r#type: Option<String>,
    pub role: Option<String>,
    pub name: Option<String>,
    pub email: Option<String>,
    pub url: Option<String>,
    pub organization: Option<String>,
    pub organization_url: Option<String>,
    pub timezone: Option<String>,
}

impl Party {
    pub(crate) fn person(role: &str, name: Option<String>, email: Option<String>) -> Self {
        Self {
            r#type: Some("person".to_string()),
            role: Some(role.to_string()),
            name,
            email,
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        }
    }
}

/// Reference to a file within a package archive with checksums.
///
/// Used in SBOM generation to track files within distribution archives.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileReference {
    pub path: String,
    pub size: Option<u64>,
    pub sha1: Option<Sha1Digest>,
    pub md5: Option<Md5Digest>,
    pub sha256: Option<Sha256Digest>,
    pub sha512: Option<Sha512Digest>,
    pub extra_data: Option<std::collections::HashMap<String, serde_json::Value>>,
}

impl FileReference {
    pub(crate) fn from_path(path: String) -> Self {
        Self {
            path,
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            extra_data: None,
        }
    }
}

/// Top-level assembled package, created by merging one or more `PackageData`
/// objects from related manifest/lockfiles (e.g., package.json + package-lock.json).
///
/// Compatible with ScanCode Toolkit output format. The key differences from
/// `PackageData` are:
/// - `package_uid`: unique identifier (PURL with UUID qualifier)
/// - `datafile_paths`: list of all contributing files
/// - `datasource_ids`: list of all contributing parsers
/// - Excludes `dependencies` and `file_references` (hoisted to top-level)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Package {
    #[serde(rename = "type")]
    pub package_type: Option<PackageType>,
    pub namespace: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    #[serde(default)]
    pub qualifiers: Option<HashMap<String, String>>,
    pub subpath: Option<String>,
    pub primary_language: Option<String>,
    pub description: Option<String>,
    pub release_date: Option<String>,
    #[serde(default)]
    pub parties: Vec<Party>,
    #[serde(default)]
    pub keywords: Vec<String>,
    pub homepage_url: Option<String>,
    pub download_url: Option<String>,
    pub size: Option<u64>,
    pub sha1: Option<Sha1Digest>,
    pub md5: Option<Md5Digest>,
    pub sha256: Option<Sha256Digest>,
    pub sha512: Option<Sha512Digest>,
    pub bug_tracking_url: Option<String>,
    pub code_view_url: Option<String>,
    pub vcs_url: Option<String>,
    pub copyright: Option<String>,
    pub holder: Option<String>,
    pub declared_license_expression: Option<String>,
    pub declared_license_expression_spdx: Option<String>,
    #[serde(default)]
    pub license_detections: Vec<LicenseDetection>,
    pub other_license_expression: Option<String>,
    pub other_license_expression_spdx: Option<String>,
    #[serde(default)]
    pub other_license_detections: Vec<LicenseDetection>,
    pub extracted_license_statement: Option<String>,
    pub notice_text: Option<String>,
    #[serde(default)]
    pub source_packages: Vec<String>,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default)]
    pub is_virtual: bool,
    #[serde(default)]
    pub extra_data: Option<HashMap<String, serde_json::Value>>,
    pub repository_homepage_url: Option<String>,
    pub repository_download_url: Option<String>,
    pub api_data_url: Option<String>,
    pub purl: Option<String>,
    /// Unique identifier for this package instance (PURL with UUID qualifier).
    pub package_uid: PackageUid,
    /// Paths to all datafiles that contributed to this package.
    pub datafile_paths: Vec<String>,
    /// Datasource identifiers for all parsers that contributed to this package.
    pub datasource_ids: Vec<DatasourceId>,
}

impl Package {
    /// Create a `Package` from a `PackageData` and its source file path.
    ///
    /// Generates a unique `package_uid` from the package PURL when available.
    /// For packages without a PURL but with enough manifest identity to assemble,
    /// falls back to an opaque UID derived from datasource/name/version.
    pub fn from_package_data(package_data: &PackageData, datafile_path: String) -> Self {
        let mut package_data = package_data.clone();
        enrich_package_data_license_provenance(&mut package_data, &datafile_path);

        let mut package = Package {
            package_type: package_data.package_type,
            namespace: package_data.namespace.clone(),
            name: package_data.name.clone(),
            version: package_data.version.clone(),
            qualifiers: package_data.qualifiers.clone(),
            subpath: package_data.subpath.clone(),
            primary_language: package_data.primary_language.clone(),
            description: package_data.description.clone(),
            release_date: package_data.release_date.clone(),
            parties: package_data.parties.clone(),
            keywords: package_data.keywords.clone(),
            homepage_url: package_data.homepage_url.clone(),
            download_url: package_data.download_url.clone(),
            size: package_data.size,
            sha1: package_data.sha1,
            md5: package_data.md5,
            sha256: package_data.sha256,
            sha512: package_data.sha512,
            bug_tracking_url: package_data.bug_tracking_url.clone(),
            code_view_url: package_data.code_view_url.clone(),
            vcs_url: package_data.vcs_url.clone(),
            copyright: package_data.copyright.clone(),
            holder: package_data.holder.clone(),
            declared_license_expression: package_data.declared_license_expression.clone(),
            declared_license_expression_spdx: package_data.declared_license_expression_spdx.clone(),
            license_detections: package_data.license_detections.clone(),
            other_license_expression: package_data.other_license_expression.clone(),
            other_license_expression_spdx: package_data.other_license_expression_spdx.clone(),
            other_license_detections: package_data.other_license_detections.clone(),
            extracted_license_statement: package_data.extracted_license_statement.clone(),
            notice_text: package_data.notice_text.clone(),
            source_packages: package_data.source_packages.clone(),
            is_private: package_data.is_private,
            is_virtual: package_data.is_virtual,
            extra_data: package_data.extra_data.clone(),
            repository_homepage_url: package_data.repository_homepage_url.clone(),
            repository_download_url: package_data.repository_download_url.clone(),
            api_data_url: package_data.api_data_url.clone(),
            purl: package_data.purl.clone(),
            package_uid: PackageUid::empty(),
            datafile_paths: vec![datafile_path],
            datasource_ids: if let Some(dsid) = package_data.datasource_id {
                vec![dsid]
            } else {
                vec![]
            },
        };

        package.refresh_identity();
        if package.package_uid.is_empty() {
            package.package_uid = package.fallback_package_uid();
        }

        package
    }

    /// Update this package with data from another `PackageData`.
    ///
    /// Merges data from a related file (e.g., lockfile) into this package.
    /// Existing non-empty values are preserved; empty fields are filled from
    /// the new data. Lists (parties, license_detections) are merged.
    pub fn update(&mut self, package_data: &PackageData, datafile_path: String) {
        let mut package_data = package_data.clone();
        enrich_package_data_license_provenance(&mut package_data, &datafile_path);

        if let Some(dsid) = package_data.datasource_id {
            self.datasource_ids.push(dsid);
        }
        self.datafile_paths.push(datafile_path);

        macro_rules! fill_if_empty {
            ($field:ident) => {
                if self.$field.is_none() {
                    self.$field = package_data.$field;
                }
            };
        }

        fill_if_empty!(package_type);
        fill_if_empty!(name);
        fill_if_empty!(namespace);
        fill_if_empty!(version);
        fill_if_empty!(qualifiers);
        fill_if_empty!(subpath);
        fill_if_empty!(primary_language);
        fill_if_empty!(description);
        fill_if_empty!(release_date);
        fill_if_empty!(homepage_url);
        fill_if_empty!(download_url);
        fill_if_empty!(size);
        fill_if_empty!(sha1);
        fill_if_empty!(md5);
        fill_if_empty!(sha256);
        fill_if_empty!(sha512);
        fill_if_empty!(bug_tracking_url);
        fill_if_empty!(code_view_url);
        fill_if_empty!(vcs_url);
        fill_if_empty!(copyright);
        fill_if_empty!(holder);
        fill_if_empty!(declared_license_expression);
        fill_if_empty!(declared_license_expression_spdx);
        fill_if_empty!(other_license_expression);
        fill_if_empty!(other_license_expression_spdx);
        fill_if_empty!(extracted_license_statement);
        fill_if_empty!(notice_text);
        match (&mut self.extra_data, &package_data.extra_data) {
            (None, Some(extra_data)) => {
                self.extra_data = Some(extra_data.clone());
            }
            (Some(existing), Some(incoming)) => {
                for (key, value) in incoming {
                    existing.entry(key.clone()).or_insert_with(|| value.clone());
                }
            }
            _ => {}
        }
        fill_if_empty!(repository_homepage_url);
        fill_if_empty!(repository_download_url);
        fill_if_empty!(api_data_url);

        for party in &package_data.parties {
            if let Some(existing) = self.parties.iter_mut().find(|p| {
                p.role == party.role
                    && ((p.name.is_some() && p.name == party.name)
                        || (p.email.is_some() && p.email == party.email))
            }) {
                if existing.name.is_none() {
                    existing.name = party.name.clone();
                }
                if existing.email.is_none() {
                    existing.email = party.email.clone();
                }
            } else {
                self.parties.push(party.clone());
            }
        }

        for keyword in &package_data.keywords {
            if !self.keywords.contains(keyword) {
                self.keywords.push(keyword.clone());
            }
        }

        for detection in &package_data.license_detections {
            self.license_detections.push(detection.clone());
        }

        for detection in &package_data.other_license_detections {
            self.other_license_detections.push(detection.clone());
        }

        for source_pkg in &package_data.source_packages {
            if !self.source_packages.contains(source_pkg) {
                self.source_packages.push(source_pkg.clone());
            }
        }

        self.refresh_identity();
    }

    pub fn backfill_license_provenance(&mut self) {
        let Some(datafile_path) = self.datafile_paths.first().cloned() else {
            return;
        };

        for detection in &mut self.license_detections {
            enrich_license_detection_provenance(detection, &datafile_path);
        }
        for detection in &mut self.other_license_detections {
            enrich_license_detection_provenance(detection, &datafile_path);
        }
    }

    fn refresh_identity(&mut self) {
        let Some(next_purl) = self.build_current_purl() else {
            return;
        };

        if self.purl.as_deref() != Some(next_purl.as_str()) || self.package_uid.is_empty() {
            self.package_uid = PackageUid::new(&next_purl);
        }

        self.purl = Some(next_purl);
    }

    fn fallback_package_uid(&self) -> PackageUid {
        let name = self
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("unknown");
        let version = self
            .version
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("unknown");
        let datasource = self
            .datasource_ids
            .first()
            .map(DatasourceId::as_str)
            .unwrap_or("unknown");

        PackageUid::new_opaque(&format!("generated-package:{datasource}/{name}@{version}"))
    }

    fn build_current_purl(&self) -> Option<String> {
        if let Some(existing_purl) = self.purl.as_deref() {
            let mut purl = PackageUrl::from_str(existing_purl).ok()?;

            if let Some(version) = self
                .version
                .as_deref()
                .filter(|value| !value.trim().is_empty())
            {
                purl.with_version(version).ok()?;
            } else {
                purl.without_version();
            }

            return Some(purl.to_string());
        }

        if let (Some(package_type), Some(name)) = (
            self.package_type.as_ref(),
            self.name
                .as_deref()
                .filter(|value| !value.trim().is_empty()),
        ) {
            let purl_type = match package_type {
                PackageType::Deno => "generic",
                _ => package_type.as_str(),
            };

            let mut purl = PackageUrl::new(purl_type, name).ok()?;

            if let Some(namespace) = self
                .namespace
                .as_deref()
                .filter(|value| !value.trim().is_empty())
            {
                purl.with_namespace(namespace).ok()?;
            }

            if let Some(version) = self
                .version
                .as_deref()
                .filter(|value| !value.trim().is_empty())
            {
                purl.with_version(version).ok()?;
            }

            if let Some(qualifiers) = &self.qualifiers {
                for (key, value) in qualifiers {
                    purl.add_qualifier(key.as_str(), value.as_str()).ok()?;
                }
            }

            if let Some(subpath) = self
                .subpath
                .as_deref()
                .filter(|value| !value.trim().is_empty())
            {
                purl.with_subpath(subpath).ok()?;
            }

            return Some(purl.to_string());
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_info_new_backfills_package_detection_provenance() {
        let package_data = PackageData {
            package_type: Some(PackageType::Npm),
            license_detections: vec![LicenseDetection {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                matches: vec![Match {
                    license_expression: "mit".to_string(),
                    license_expression_spdx: "MIT".to_string(),
                    from_file: None,
                    start_line: LineNumber::ONE,
                    end_line: LineNumber::ONE,
                    matcher: Some("parser-declared-license".to_string()),
                    score: MatchScore::MAX,
                    matched_length: Some(1),
                    match_coverage: Some(100.0),
                    rule_relevance: Some(100),
                    rule_identifier: None,
                    rule_url: None,
                    matched_text: Some("MIT".to_string()),
                    referenced_filenames: None,
                    matched_text_diagnostics: None,
                }],
                detection_log: vec![],
                identifier: None,
            }],
            ..PackageData::default()
        };

        let file_info = FileInfo::new(
            "package.json".to_string(),
            "package".to_string(),
            ".json".to_string(),
            "project/package.json".to_string(),
            FileType::File,
            None,
            None,
            1,
            None,
            None,
            None,
            None,
            None,
            vec![package_data],
            None,
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        );

        assert_eq!(file_info.license_detections.len(), 1);
        assert_eq!(
            file_info.license_detections[0].matches[0]
                .from_file
                .as_deref(),
            Some("project/package.json")
        );
        assert!(file_info.license_detections[0].identifier.is_some());
        assert_eq!(
            file_info.package_data[0].license_detections[0].matches[0]
                .from_file
                .as_deref(),
            Some("project/package.json")
        );
        assert_eq!(
            file_info.package_data[0].license_detections[0].matches[0]
                .rule_identifier
                .as_deref(),
            Some("parser-declared-license")
        );
        assert!(
            file_info.package_data[0].license_detections[0]
                .identifier
                .is_some()
        );
    }

    #[test]
    fn package_from_package_data_backfills_detection_provenance() {
        let package_data = PackageData {
            package_type: Some(PackageType::Npm),
            license_detections: vec![LicenseDetection {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                matches: vec![Match {
                    license_expression: "mit".to_string(),
                    license_expression_spdx: "MIT".to_string(),
                    from_file: None,
                    start_line: LineNumber::ONE,
                    end_line: LineNumber::ONE,
                    matcher: Some("parser-declared-license".to_string()),
                    score: MatchScore::MAX,
                    matched_length: Some(1),
                    match_coverage: Some(100.0),
                    rule_relevance: Some(100),
                    rule_identifier: None,
                    rule_url: None,
                    matched_text: Some("MIT".to_string()),
                    referenced_filenames: None,
                    matched_text_diagnostics: None,
                }],
                detection_log: vec![],
                identifier: None,
            }],
            ..PackageData::default()
        };

        let package = Package::from_package_data(&package_data, "project/package.json".to_string());

        assert_eq!(
            package.license_detections[0].matches[0]
                .from_file
                .as_deref(),
            Some("project/package.json")
        );
        assert_eq!(
            package.license_detections[0].matches[0]
                .rule_identifier
                .as_deref(),
            Some("parser-declared-license")
        );
        assert!(package.license_detections[0].identifier.is_some());
    }

    #[test]
    fn package_from_package_data_preserves_existing_purl_qualifiers() {
        let package_data = PackageData {
            package_type: Some(PackageType::Alpine),
            namespace: Some("alpine".to_string()),
            name: Some("busybox".to_string()),
            version: Some("1.35.0-r17".to_string()),
            purl: Some("pkg:alpine/busybox@1.35.0-r17?arch=x86_64".to_string()),
            ..PackageData::default()
        };

        let package = Package::from_package_data(&package_data, "lib/apk/db/installed".to_string());

        assert_eq!(
            package.purl.as_deref(),
            Some("pkg:alpine/busybox@1.35.0-r17?arch=x86_64")
        );
        assert!(
            package
                .package_uid
                .starts_with("pkg:alpine/busybox@1.35.0-r17?arch=x86_64&uuid=")
        );
    }
}

/// Top-level dependency instance, created during package assembly.
///
/// Extends the file-level `Dependency` with traceability fields that link
/// each dependency to its owning package and source datafile.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TopLevelDependency {
    pub purl: Option<String>,
    pub extracted_requirement: Option<String>,
    pub scope: Option<String>,
    pub is_runtime: Option<bool>,
    pub is_optional: Option<bool>,
    pub is_pinned: Option<bool>,
    pub is_direct: Option<bool>,
    pub resolved_package: Option<Box<ResolvedPackage>>,
    #[serde(default)]
    pub extra_data: Option<HashMap<String, serde_json::Value>>,
    /// Unique identifier for this dependency instance (PURL with UUID qualifier).
    pub dependency_uid: DependencyUid,
    /// The `package_uid` of the package this dependency belongs to.
    pub for_package_uid: Option<PackageUid>,
    /// Path to the datafile where this dependency was declared.
    pub datafile_path: String,
    /// Datasource identifier for the parser that extracted this dependency.
    pub datasource_id: DatasourceId,
    /// Namespace for the dependency (e.g., distribution name for RPM packages).
    pub namespace: Option<String>,
}

impl TopLevelDependency {
    /// Create a `TopLevelDependency` from a file-level `Dependency`.
    pub fn from_dependency(
        dep: &Dependency,
        datafile_path: String,
        datasource_id: DatasourceId,
        for_package_uid: Option<PackageUid>,
    ) -> Self {
        let dependency_uid = dep
            .purl
            .as_ref()
            .map(|p| DependencyUid::new(p))
            .unwrap_or_else(DependencyUid::empty);

        TopLevelDependency {
            purl: dep.purl.clone(),
            extracted_requirement: dep.extracted_requirement.clone(),
            scope: dep.scope.clone(),
            is_runtime: dep.is_runtime,
            is_optional: dep.is_optional,
            is_pinned: dep.is_pinned,
            is_direct: dep.is_direct,
            resolved_package: dep.resolved_package.clone(),
            extra_data: dep.extra_data.clone(),
            dependency_uid,
            for_package_uid,
            datafile_path,
            datasource_id,
            namespace: None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputEmail {
    pub email: String,
    pub start_line: LineNumber,
    pub end_line: LineNumber,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputURL {
    pub url: String,
    pub start_line: LineNumber,
    pub end_line: LineNumber,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct LicensePolicyEntry {
    pub license_key: String,
    pub label: String,
    pub color_code: String,
    pub icon: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FileType {
    File,
    Directory,
}

impl serde::Serialize for FileType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            FileType::File => serializer.serialize_str("file"),
            FileType::Directory => serializer.serialize_str("directory"),
        }
    }
}

impl<'de> Deserialize<'de> for FileType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "file" => Ok(FileType::File),
            "directory" => Ok(FileType::Directory),
            _ => Err(serde::de::Error::custom("invalid file type")),
        }
    }
}
