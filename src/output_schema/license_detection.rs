use serde::{Deserialize, Serialize};

use super::license_match::OutputMatch;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct OutputLicenseDetection {
    pub license_expression: String,
    pub license_expression_spdx: String,
    pub matches: Vec<OutputMatch>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub detection_log: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
}

impl From<&crate::models::LicenseDetection> for OutputLicenseDetection {
    fn from(value: &crate::models::LicenseDetection) -> Self {
        Self {
            license_expression: value.license_expression.clone(),
            license_expression_spdx: value.license_expression_spdx.clone(),
            matches: value.matches.iter().map(OutputMatch::from).collect(),
            detection_log: value.detection_log.clone(),
            identifier: value.identifier.clone(),
        }
    }
}

impl TryFrom<&OutputLicenseDetection> for crate::models::LicenseDetection {
    type Error = String;
    fn try_from(value: &OutputLicenseDetection) -> Result<Self, Self::Error> {
        let mut matches = Vec::with_capacity(value.matches.len());
        for m in &value.matches {
            matches.push(crate::models::Match::try_from(m)?);
        }
        Ok(Self {
            license_expression: value.license_expression.clone(),
            license_expression_spdx: value.license_expression_spdx.clone(),
            matches,
            detection_log: value.detection_log.clone(),
            identifier: value.identifier.clone(),
        })
    }
}
