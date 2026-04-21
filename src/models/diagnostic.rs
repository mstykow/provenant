// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanDiagnostic {
    pub severity: DiagnosticSeverity,
    pub message: String,
}

impl ScanDiagnostic {
    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            message: message.into(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            message: message.into(),
        }
    }
}

pub fn diagnostics_from_legacy_scan_errors(messages: &[String]) -> Vec<ScanDiagnostic> {
    messages
        .iter()
        .cloned()
        .map(|message| {
            if is_legacy_warning_message(&message) {
                ScanDiagnostic::warning(message)
            } else {
                ScanDiagnostic::error(message)
            }
        })
        .collect()
}

pub fn is_legacy_warning_message(message: &str) -> bool {
    let first_line = message.lines().next().unwrap_or(message).trim();
    first_line.starts_with("Maven property ")
        || first_line.starts_with("Skipping Maven template coordinates")
        || first_line.starts_with("Circular include detected")
}
