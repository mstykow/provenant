use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputSystemEnvironment {
    pub operating_system: String,
    pub cpu_architecture: String,
    pub platform: String,
    pub platform_version: String,
    pub rust_version: String,
}

impl From<&crate::models::SystemEnvironment> for OutputSystemEnvironment {
    fn from(value: &crate::models::SystemEnvironment) -> Self {
        Self {
            operating_system: value.operating_system.clone(),
            cpu_architecture: value.cpu_architecture.clone(),
            platform: value.platform.clone(),
            platform_version: value.platform_version.clone(),
            rust_version: value.rust_version.clone(),
        }
    }
}
