// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::extra_data::OutputExtraData;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputHeader {
    pub tool_name: String,
    pub tool_version: String,
    pub options: Map<String, Value>,
    pub notice: String,
    pub start_timestamp: String,
    pub end_timestamp: String,
    pub output_format_version: String,
    pub duration: f64,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub extra_data: OutputExtraData,
}

impl From<&crate::models::Header> for OutputHeader {
    fn from(value: &crate::models::Header) -> Self {
        Self {
            tool_name: value.tool_name.clone(),
            tool_version: value.tool_version.clone(),
            options: value.options.clone(),
            notice: value.notice.clone(),
            start_timestamp: value.start_timestamp.clone(),
            end_timestamp: value.end_timestamp.clone(),
            output_format_version: value.output_format_version.clone(),
            duration: value.duration,
            errors: value.errors.clone(),
            warnings: value.warnings.clone(),
            extra_data: OutputExtraData::from(&value.extra_data),
        }
    }
}
