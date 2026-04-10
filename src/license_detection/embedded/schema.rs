use serde::{Deserialize, Serialize};

use crate::license_detection::models::{LoadedLicense, LoadedRule};

pub const SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmbeddedArtifactMetadata {
    pub spdx_license_list_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddedLoaderSnapshot {
    pub schema_version: u32,
    pub metadata: EmbeddedArtifactMetadata,
    pub rules: Vec<LoadedRule>,
    pub licenses: Vec<LoadedLicense>,
}
