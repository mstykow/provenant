// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::{FileInfo, Match, Package, TopLevelDependency};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const OUTPUT_FORMAT_VERSION: &str = "4.1.0";
pub const TOOL_NAME: &str = "provenant";
pub const HEADER_NOTICE: &str = "Generated with Provenant and provided on an \"AS IS\" basis, without warranties or conditions of any kind, either express or implied. Provenant and its authors/providers do not provide legal advice, and are not responsible for how this output is used. Consult qualified legal counsel for legal advice.";

#[derive(Debug)]
/// Top-level ScanCode-compatible JSON payload.
pub struct Output {
    pub summary: Option<Summary>,
    pub tallies: Option<Tallies>,
    pub tallies_of_key_files: Option<Tallies>,
    pub tallies_by_facet: Option<Vec<FacetTallies>>,
    pub headers: Vec<Header>,
    pub packages: Vec<Package>,
    pub dependencies: Vec<TopLevelDependency>,
    pub license_detections: Vec<TopLevelLicenseDetection>,
    pub files: Vec<FileInfo>,
    pub license_references: Vec<LicenseReference>,
    pub license_rule_references: Vec<LicenseRuleReference>,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct TopLevelLicenseDetection {
    pub identifier: String,
    pub license_expression: String,
    pub license_expression_spdx: String,
    pub detection_count: usize,
    #[serde(default)]
    pub detection_log: Vec<String>,
    pub reference_matches: Vec<Match>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub declared_license_expression: Option<String>,
    pub license_clarity_score: Option<LicenseClarityScore>,
    pub declared_holder: Option<String>,
    pub primary_language: Option<String>,
    pub other_license_expressions: Vec<TallyEntry>,
    pub other_holders: Vec<TallyEntry>,
    pub other_languages: Vec<TallyEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LicenseClarityScore {
    pub score: usize,
    pub declared_license: bool,
    pub identification_precision: bool,
    pub has_license_text: bool,
    pub declared_copyrights: bool,
    pub conflicting_license_categories: bool,
    pub ambiguous_compound_licensing: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TallyEntry {
    pub value: Option<String>,
    pub count: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct Tallies {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub detected_license_expression: Vec<TallyEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub copyrights: Vec<TallyEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holders: Vec<TallyEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<TallyEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub programming_language: Vec<TallyEntry>,
}

impl Tallies {
    pub fn is_empty(&self) -> bool {
        self.detected_license_expression.is_empty()
            && self.copyrights.is_empty()
            && self.holders.is_empty()
            && self.authors.is_empty()
            && self.programming_language.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FacetTallies {
    pub facet: String,
    pub tallies: Tallies,
}

#[derive(Debug)]
/// Scan execution metadata stored in `output.headers`.
pub struct Header {
    pub tool_name: String,
    pub tool_version: String,
    pub options: Map<String, Value>,
    pub notice: String,
    pub start_timestamp: String,
    pub end_timestamp: String,
    pub output_format_version: String,
    pub duration: f64,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub extra_data: ExtraData,
}

#[derive(Debug)]
/// Additional counters and environment details for a scan run.
pub struct ExtraData {
    pub system_environment: SystemEnvironment,
    pub spdx_license_list_version: String,
    pub files_count: usize,
    pub directories_count: usize,
    pub excluded_count: usize,
    pub license_index_provenance: Option<LicenseIndexProvenance>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct LicenseIndexProvenance {
    pub source: String,
    pub dataset_fingerprint: String,
    #[serde(default)]
    pub ignored_rules: Vec<String>,
    #[serde(default)]
    pub ignored_licenses: Vec<String>,
    #[serde(default)]
    pub ignored_rules_due_to_licenses: Vec<String>,
    #[serde(default)]
    pub added_rules: Vec<String>,
    #[serde(default)]
    pub replaced_rules: Vec<String>,
    #[serde(default)]
    pub added_licenses: Vec<String>,
    #[serde(default)]
    pub replaced_licenses: Vec<String>,
}

#[derive(Debug)]
/// Host environment information captured during scan execution.
pub struct SystemEnvironment {
    pub operating_system: String,
    pub cpu_architecture: String,
    pub platform: String,
    pub platform_version: String,
    pub rust_version: String,
}

#[derive(Deserialize, Debug)]
/// Reference entry for a detected license.
pub struct LicenseReference {
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    pub name: String,
    pub short_name: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub homepage_url: Option<String>,
    pub spdx_license_key: String,
    #[serde(default)]
    pub other_spdx_license_keys: Vec<String>,
    #[serde(default)]
    pub osi_license_key: Option<String>,
    #[serde(default)]
    pub text_urls: Vec<String>,
    #[serde(default)]
    pub osi_url: Option<String>,
    #[serde(default)]
    pub faq_url: Option<String>,
    #[serde(default)]
    pub other_urls: Vec<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub is_exception: bool,
    #[serde(default)]
    pub is_unknown: bool,
    #[serde(default)]
    pub is_generic: bool,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub minimum_coverage: Option<u8>,
    #[serde(default)]
    pub standard_notice: Option<String>,
    #[serde(default)]
    pub ignorable_copyrights: Vec<String>,
    #[serde(default)]
    pub ignorable_holders: Vec<String>,
    #[serde(default)]
    pub ignorable_authors: Vec<String>,
    #[serde(default)]
    pub ignorable_urls: Vec<String>,
    #[serde(default)]
    pub ignorable_emails: Vec<String>,
    #[serde(default)]
    pub scancode_url: Option<String>,
    #[serde(default)]
    pub licensedb_url: Option<String>,
    #[serde(default)]
    pub spdx_url: Option<String>,
    pub text: String,
}

#[derive(Deserialize, Debug)]
/// Reference metadata for a license detection rule.
pub struct LicenseRuleReference {
    pub identifier: String,
    pub license_expression: String,
    pub is_license_text: bool,
    pub is_license_notice: bool,
    pub is_license_reference: bool,
    pub is_license_tag: bool,
    pub is_license_clue: bool,
    pub is_license_intro: bool,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub rule_url: Option<String>,
    #[serde(default)]
    pub is_required_phrase: bool,
    #[serde(default)]
    pub skip_for_required_phrase_generation: bool,
    #[serde(default)]
    pub replaced_by: Vec<String>,
    #[serde(default)]
    pub is_continuous: bool,
    #[serde(default)]
    pub is_synthetic: bool,
    #[serde(default)]
    pub is_from_license: bool,
    #[serde(default)]
    pub length: usize,
    #[serde(default)]
    pub relevance: Option<u8>,
    #[serde(default)]
    pub minimum_coverage: Option<u8>,
    #[serde(default)]
    pub referenced_filenames: Vec<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub ignorable_copyrights: Vec<String>,
    #[serde(default)]
    pub ignorable_holders: Vec<String>,
    #[serde(default)]
    pub ignorable_authors: Vec<String>,
    #[serde(default)]
    pub ignorable_urls: Vec<String>,
    #[serde(default)]
    pub ignorable_emails: Vec<String>,
    #[serde(default)]
    pub text: Option<String>,
}
