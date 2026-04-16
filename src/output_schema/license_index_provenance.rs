use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputLicenseIndexProvenance {
    pub source: String,
    pub policy_path: String,
    pub curation_fingerprint: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignored_rules: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignored_licenses: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignored_rules_due_to_licenses: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added_rules: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replaced_rules: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added_licenses: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replaced_licenses: Vec<String>,
}

impl From<&crate::models::LicenseIndexProvenance> for OutputLicenseIndexProvenance {
    fn from(value: &crate::models::LicenseIndexProvenance) -> Self {
        Self {
            source: value.source.clone(),
            policy_path: value.policy_path.clone(),
            curation_fingerprint: value.curation_fingerprint.clone(),
            ignored_rules: value.ignored_rules.clone(),
            ignored_licenses: value.ignored_licenses.clone(),
            ignored_rules_due_to_licenses: value.ignored_rules_due_to_licenses.clone(),
            added_rules: value.added_rules.clone(),
            replaced_rules: value.replaced_rules.clone(),
            added_licenses: value.added_licenses.clone(),
            replaced_licenses: value.replaced_licenses.clone(),
        }
    }
}
