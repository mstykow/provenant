// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputURL {
    pub url: String,
    pub start_line: u64,
    pub end_line: u64,
}

impl From<&crate::models::OutputURL> for OutputURL {
    fn from(value: &crate::models::OutputURL) -> Self {
        Self {
            url: value.url.clone(),
            start_line: value.start_line.get() as u64,
            end_line: value.end_line.get() as u64,
        }
    }
}

impl TryFrom<&OutputURL> for crate::models::OutputURL {
    type Error = String;
    fn try_from(value: &OutputURL) -> Result<Self, Self::Error> {
        use crate::models::LineNumber;
        let start_line = LineNumber::new(value.start_line as usize)
            .ok_or_else(|| format!("invalid start_line: {}", value.start_line))?;
        let end_line = LineNumber::new(value.end_line as usize)
            .ok_or_else(|| format!("invalid end_line: {}", value.end_line))?;
        Ok(Self {
            url: value.url.clone(),
            start_line,
            end_line,
        })
    }
}
