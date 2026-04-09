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

impl TryFrom<&OutputTallies> for crate::models::Tallies {
    type Error = String;
    fn try_from(value: &OutputTallies) -> Result<Self, Self::Error> {
        let mut detected_license_expression =
            Vec::with_capacity(value.detected_license_expression.len());
        for e in &value.detected_license_expression {
            detected_license_expression.push(crate::models::TallyEntry::from(e));
        }
        let mut copyrights = Vec::with_capacity(value.copyrights.len());
        for e in &value.copyrights {
            copyrights.push(crate::models::TallyEntry::from(e));
        }
        let mut holders = Vec::with_capacity(value.holders.len());
        for e in &value.holders {
            holders.push(crate::models::TallyEntry::from(e));
        }
        let mut authors = Vec::with_capacity(value.authors.len());
        for e in &value.authors {
            authors.push(crate::models::TallyEntry::from(e));
        }
        let mut programming_language = Vec::with_capacity(value.programming_language.len());
        for e in &value.programming_language {
            programming_language.push(crate::models::TallyEntry::from(e));
        }
        Ok(Self {
            detected_license_expression,
            copyrights,
            holders,
            authors,
            programming_language,
        })
    }
}
