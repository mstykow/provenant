use serde::{Deserialize, Serialize};

use super::license_match::OutputMatch;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct OutputTopLevelLicenseDetection {
    pub identifier: String,
    pub license_expression: String,
    pub license_expression_spdx: String,
    pub detection_count: usize,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub detection_log: Vec<String>,
    pub reference_matches: Vec<OutputMatch>,
}

impl From<&crate::models::TopLevelLicenseDetection> for OutputTopLevelLicenseDetection {
    fn from(value: &crate::models::TopLevelLicenseDetection) -> Self {
        Self {
            identifier: value.identifier.clone(),
            license_expression: value.license_expression.clone(),
            license_expression_spdx: value.license_expression_spdx.clone(),
            detection_count: value.detection_count,
            detection_log: value.detection_log.clone(),
            reference_matches: value
                .reference_matches
                .iter()
                .map(OutputMatch::from)
                .collect(),
        }
    }
}

impl TryFrom<&OutputTopLevelLicenseDetection> for crate::models::TopLevelLicenseDetection {
    type Error = String;
    fn try_from(value: &OutputTopLevelLicenseDetection) -> Result<Self, Self::Error> {
        let mut reference_matches = Vec::with_capacity(value.reference_matches.len());
        for m in &value.reference_matches {
            reference_matches.push(crate::models::Match::try_from(m)?);
        }
        Ok(Self {
            identifier: value.identifier.clone(),
            license_expression: value.license_expression.clone(),
            license_expression_spdx: value.license_expression_spdx.clone(),
            detection_count: value.detection_count,
            detection_log: value.detection_log.clone(),
            reference_matches,
        })
    }
}
