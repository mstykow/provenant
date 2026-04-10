use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct OutputLicensePolicyEntry {
    pub license_key: String,
    pub label: String,
    pub color_code: String,
    pub icon: String,
}

impl From<&crate::models::LicensePolicyEntry> for OutputLicensePolicyEntry {
    fn from(value: &crate::models::LicensePolicyEntry) -> Self {
        Self {
            license_key: value.license_key.clone(),
            label: value.label.clone(),
            color_code: value.color_code.clone(),
            icon: value.icon.clone(),
        }
    }
}

impl TryFrom<&OutputLicensePolicyEntry> for crate::models::LicensePolicyEntry {
    type Error = String;
    fn try_from(value: &OutputLicensePolicyEntry) -> Result<Self, Self::Error> {
        Ok(Self {
            license_key: value.license_key.clone(),
            label: value.label.clone(),
            color_code: value.color_code.clone(),
            icon: value.icon.clone(),
        })
    }
}
