// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use super::tallies::OutputTallies;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct OutputFacetTallies {
    pub facet: String,
    pub tallies: OutputTallies,
}

impl From<&crate::models::FacetTallies> for OutputFacetTallies {
    fn from(value: &crate::models::FacetTallies) -> Self {
        Self {
            facet: value.facet.clone(),
            tallies: OutputTallies::from(&value.tallies),
        }
    }
}
