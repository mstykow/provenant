use serde::{Deserialize, Serialize};

use super::license_index_provenance::OutputLicenseIndexProvenance;
use super::system_environment::OutputSystemEnvironment;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputExtraData {
    pub system_environment: OutputSystemEnvironment,
    pub spdx_license_list_version: String,
    pub files_count: usize,
    pub directories_count: usize,
    pub excluded_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license_index_provenance: Option<OutputLicenseIndexProvenance>,
}

impl From<&crate::models::ExtraData> for OutputExtraData {
    fn from(value: &crate::models::ExtraData) -> Self {
        Self {
            system_environment: OutputSystemEnvironment::from(&value.system_environment),
            spdx_license_list_version: value.spdx_license_list_version.clone(),
            files_count: value.files_count,
            directories_count: value.directories_count,
            excluded_count: value.excluded_count,
            license_index_provenance: value
                .license_index_provenance
                .as_ref()
                .map(OutputLicenseIndexProvenance::from),
        }
    }
}
