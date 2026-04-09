use serde::{Deserialize, Serialize};

use super::extra_data::OutputExtraData;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputHeader {
    pub start_timestamp: String,
    pub end_timestamp: String,
    pub duration: f64,
    pub extra_data: OutputExtraData,
    pub errors: Vec<String>,
    pub output_format_version: String,
}

impl From<&crate::models::Header> for OutputHeader {
    fn from(value: &crate::models::Header) -> Self {
        Self {
            start_timestamp: value.start_timestamp.clone(),
            end_timestamp: value.end_timestamp.clone(),
            duration: value.duration,
            extra_data: OutputExtraData::from(&value.extra_data),
            errors: value.errors.clone(),
            output_format_version: value.output_format_version.clone(),
        }
    }
}
