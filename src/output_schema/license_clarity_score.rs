// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct OutputLicenseClarityScore {
    pub score: usize,
    pub declared_license: bool,
    pub identification_precision: bool,
    pub has_license_text: bool,
    pub declared_copyrights: bool,
    pub conflicting_license_categories: bool,
    pub ambiguous_compound_licensing: bool,
}

impl From<&crate::models::LicenseClarityScore> for OutputLicenseClarityScore {
    fn from(value: &crate::models::LicenseClarityScore) -> Self {
        Self {
            score: value.score,
            declared_license: value.declared_license,
            identification_precision: value.identification_precision,
            has_license_text: value.has_license_text,
            declared_copyrights: value.declared_copyrights,
            conflicting_license_categories: value.conflicting_license_categories,
            ambiguous_compound_licensing: value.ambiguous_compound_licensing,
        }
    }
}
