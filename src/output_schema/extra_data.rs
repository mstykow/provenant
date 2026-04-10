use serde::{Deserialize, Serialize};

use super::system_environment::OutputSystemEnvironment;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputExtraData {
    pub files_count: usize,
    pub directories_count: usize,
    pub excluded_count: usize,
    pub system_environment: OutputSystemEnvironment,
}

impl From<&crate::models::ExtraData> for OutputExtraData {
    fn from(value: &crate::models::ExtraData) -> Self {
        Self {
            files_count: value.files_count,
            directories_count: value.directories_count,
            excluded_count: value.excluded_count,
            system_environment: OutputSystemEnvironment::from(&value.system_environment),
        }
    }
}
