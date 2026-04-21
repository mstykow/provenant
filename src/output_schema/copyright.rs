// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputCopyright {
    pub copyright: String,
    pub start_line: u64,
    pub end_line: u64,
}

impl From<&crate::models::Copyright> for OutputCopyright {
    fn from(value: &crate::models::Copyright) -> Self {
        Self {
            copyright: value.copyright.clone(),
            start_line: value.start_line.get() as u64,
            end_line: value.end_line.get() as u64,
        }
    }
}

impl TryFrom<&OutputCopyright> for crate::models::Copyright {
    type Error = String;
    fn try_from(value: &OutputCopyright) -> Result<Self, Self::Error> {
        use crate::models::LineNumber;
        let start_line = LineNumber::new(value.start_line as usize)
            .ok_or_else(|| format!("invalid start_line: {}", value.start_line))?;
        let end_line = LineNumber::new(value.end_line as usize)
            .ok_or_else(|| format!("invalid end_line: {}", value.end_line))?;
        Ok(Self {
            copyright: value.copyright.clone(),
            start_line,
            end_line,
        })
    }
}
