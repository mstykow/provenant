use serde::{Deserialize, Serialize};

use super::tally_entry::OutputTallyEntry;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct OutputTallies {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub detected_license_expression: Vec<OutputTallyEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub copyrights: Vec<OutputTallyEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holders: Vec<OutputTallyEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<OutputTallyEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub programming_language: Vec<OutputTallyEntry>,
}

impl From<&crate::models::Tallies> for OutputTallies {
    fn from(value: &crate::models::Tallies) -> Self {
        Self {
            detected_license_expression: value
                .detected_license_expression
                .iter()
                .map(OutputTallyEntry::from)
                .collect(),
            copyrights: value
                .copyrights
                .iter()
                .map(OutputTallyEntry::from)
                .collect(),
            holders: value.holders.iter().map(OutputTallyEntry::from).collect(),
            authors: value.authors.iter().map(OutputTallyEntry::from).collect(),
            programming_language: value
                .programming_language
                .iter()
                .map(OutputTallyEntry::from)
                .collect(),
        }
    }
}
