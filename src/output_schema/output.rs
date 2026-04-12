use serde::{Deserialize, Serialize};

use super::facet_tallies::OutputFacetTallies;
use super::file_info::OutputFileInfo;
use super::header::OutputHeader;
use super::license_reference::OutputLicenseReference;
use super::license_rule_reference::OutputLicenseRuleReference;
use super::package::OutputPackage;
use super::summary::OutputSummary;
use super::tallies::OutputTallies;
use super::top_level_dependency::OutputTopLevelDependency;
use super::top_level_license_detection::OutputTopLevelLicenseDetection;

#[derive(Serialize, Deserialize, Debug)]
pub struct Output {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<OutputSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tallies: Option<OutputTallies>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tallies_of_key_files: Option<OutputTallies>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tallies_by_facet: Option<Vec<OutputFacetTallies>>,
    pub headers: Vec<OutputHeader>,
    pub packages: Vec<OutputPackage>,
    pub dependencies: Vec<OutputTopLevelDependency>,
    #[serde(default)]
    pub license_detections: Vec<OutputTopLevelLicenseDetection>,
    pub files: Vec<OutputFileInfo>,
    pub license_references: Vec<OutputLicenseReference>,
    pub license_rule_references: Vec<OutputLicenseRuleReference>,
}

impl From<&crate::models::Output> for Output {
    fn from(value: &crate::models::Output) -> Self {
        Self {
            summary: value.summary.as_ref().map(OutputSummary::from),
            tallies: value.tallies.as_ref().map(OutputTallies::from),
            tallies_of_key_files: value.tallies_of_key_files.as_ref().map(OutputTallies::from),
            tallies_by_facet: value
                .tallies_by_facet
                .as_ref()
                .map(|v| v.iter().map(OutputFacetTallies::from).collect()),
            headers: value.headers.iter().map(OutputHeader::from).collect(),
            packages: value.packages.iter().map(OutputPackage::from).collect(),
            dependencies: value
                .dependencies
                .iter()
                .map(OutputTopLevelDependency::from)
                .collect(),
            license_detections: value
                .license_detections
                .iter()
                .map(OutputTopLevelLicenseDetection::from)
                .collect(),
            files: value.files.iter().map(OutputFileInfo::from).collect(),
            license_references: value
                .license_references
                .iter()
                .map(OutputLicenseReference::from)
                .collect(),
            license_rule_references: value
                .license_rule_references
                .iter()
                .map(OutputLicenseRuleReference::from)
                .collect(),
        }
    }
}
