use serde::{Deserialize, Serialize};

use crate::license_detection::models::{LoadedLicense, LoadedRule};

pub const SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddedLoaderSnapshot {
    pub schema_version: u32,
    pub rules: Vec<LoadedRule>,
    pub licenses: Vec<LoadedLicense>,
}
