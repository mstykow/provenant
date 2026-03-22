use super::{FileInfo, Package, TopLevelDependency};
use serde::{Deserialize, Serialize};

pub const OUTPUT_FORMAT_VERSION: &str = "4.0.0";

#[derive(Serialize, Deserialize, Debug)]
/// Top-level ScanCode-compatible JSON payload.
pub struct Output {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<Summary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tallies: Option<Tallies>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tallies_of_key_files: Option<Tallies>,
    pub headers: Vec<Header>,
    pub packages: Vec<Package>,
    pub dependencies: Vec<TopLevelDependency>,
    pub files: Vec<FileInfo>,
    pub license_references: Vec<LicenseReference>,
    pub license_rule_references: Vec<LicenseRuleReference>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declared_license_expression: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license_clarity_score: Option<LicenseClarityScore>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declared_holder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_language: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub other_languages: Vec<TallyEntry>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub count: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
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

#[derive(Serialize, Deserialize, Debug)]
/// Scan execution metadata stored in `output.headers`.
pub struct Header {
    pub start_timestamp: String,
    pub end_timestamp: String,
    pub duration: f64,
    pub extra_data: ExtraData,
    pub errors: Vec<String>,
    pub output_format_version: String,
}

#[derive(Serialize, Deserialize, Debug)]
/// Additional counters and environment details for a scan run.
pub struct ExtraData {
    pub files_count: usize,
    pub directories_count: usize,
    pub excluded_count: usize,
    pub system_environment: SystemEnvironment,
}

#[derive(Serialize, Deserialize, Debug)]
/// Host environment information captured during scan execution.
pub struct SystemEnvironment {
    pub operating_system: Option<String>,
    pub cpu_architecture: String,
    pub platform: String,
    pub rust_version: String,
}

#[derive(Serialize, Deserialize, Debug)]
/// Reference entry for a detected license.
pub struct LicenseReference {
    pub name: String,
    pub short_name: String,
    pub spdx_license_key: String,
    pub text: String,
}

#[derive(Serialize, Deserialize, Debug)]
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
}
