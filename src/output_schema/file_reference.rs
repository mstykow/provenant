// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::serde_helpers::serialize_optional_map_as_object;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputFileReference {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    pub sha1: Option<String>,
    pub md5: Option<String>,
    pub sha256: Option<String>,
    pub sha512: Option<String>,
    #[serde(default, serialize_with = "serialize_optional_map_as_object")]
    pub extra_data: Option<HashMap<String, serde_json::Value>>,
}

impl From<&crate::models::FileReference> for OutputFileReference {
    fn from(value: &crate::models::FileReference) -> Self {
        Self {
            path: value.path.clone(),
            size: value.size,
            sha1: value.sha1.as_ref().map(|d| d.as_hex()),
            md5: value.md5.as_ref().map(|d| d.as_hex()),
            sha256: value.sha256.as_ref().map(|d| d.as_hex()),
            sha512: value.sha512.as_ref().map(|d| d.as_hex()),
            extra_data: value.extra_data.clone(),
        }
    }
}

impl TryFrom<&OutputFileReference> for crate::models::FileReference {
    type Error = String;
    fn try_from(value: &OutputFileReference) -> Result<Self, Self::Error> {
        Ok(Self {
            path: value.path.clone(),
            size: value.size,
            sha1: value
                .sha1
                .as_ref()
                .map(|s| crate::models::Sha1Digest::from_hex(s))
                .transpose()
                .map_err(|e| format!("invalid sha1: {}", e))?,
            md5: value
                .md5
                .as_ref()
                .map(|s| crate::models::Md5Digest::from_hex(s))
                .transpose()
                .map_err(|e| format!("invalid md5: {}", e))?,
            sha256: value
                .sha256
                .as_ref()
                .map(|s| crate::models::Sha256Digest::from_hex(s))
                .transpose()
                .map_err(|e| format!("invalid sha256: {}", e))?,
            sha512: value
                .sha512
                .as_ref()
                .map(|s| crate::models::Sha512Digest::from_hex(s))
                .transpose()
                .map_err(|e| format!("invalid sha512: {}", e))?,
            extra_data: value.extra_data.clone(),
        })
    }
}
