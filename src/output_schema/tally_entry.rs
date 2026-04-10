use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct OutputTallyEntry {
    pub value: Option<String>,
    pub count: usize,
}

impl From<&crate::models::TallyEntry> for OutputTallyEntry {
    fn from(value: &crate::models::TallyEntry) -> Self {
        Self {
            value: value.value.clone(),
            count: value.count,
        }
    }
}

impl From<&OutputTallyEntry> for crate::models::TallyEntry {
    fn from(value: &OutputTallyEntry) -> Self {
        Self {
            value: value.value.clone(),
            count: value.count,
        }
    }
}
