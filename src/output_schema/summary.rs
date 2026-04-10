use serde::{Deserialize, Serialize};

use super::license_clarity_score::OutputLicenseClarityScore;
use super::tally_entry::OutputTallyEntry;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct OutputSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declared_license_expression: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license_clarity_score: Option<OutputLicenseClarityScore>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declared_holder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_language: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub other_license_expressions: Vec<OutputTallyEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub other_holders: Vec<OutputTallyEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub other_languages: Vec<OutputTallyEntry>,
}

impl From<&crate::models::Summary> for OutputSummary {
    fn from(value: &crate::models::Summary) -> Self {
        Self {
            declared_license_expression: value.declared_license_expression.clone(),
            license_clarity_score: value
                .license_clarity_score
                .as_ref()
                .map(OutputLicenseClarityScore::from),
            declared_holder: value.declared_holder.clone(),
            primary_language: value.primary_language.clone(),
            other_license_expressions: value
                .other_license_expressions
                .iter()
                .map(OutputTallyEntry::from)
                .collect(),
            other_holders: value
                .other_holders
                .iter()
                .map(OutputTallyEntry::from)
                .collect(),
            other_languages: value
                .other_languages
                .iter()
                .map(OutputTallyEntry::from)
                .collect(),
        }
    }
}
