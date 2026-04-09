use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputHolder {
    pub holder: String,
    pub start_line: u64,
    pub end_line: u64,
}

impl From<&crate::models::Holder> for OutputHolder {
    fn from(value: &crate::models::Holder) -> Self {
        Self {
            holder: value.holder.clone(),
            start_line: value.start_line.get() as u64,
            end_line: value.end_line.get() as u64,
        }
    }
}

impl TryFrom<&OutputHolder> for crate::models::Holder {
    type Error = String;
    fn try_from(value: &OutputHolder) -> Result<Self, Self::Error> {
        use crate::models::LineNumber;
        let start_line = LineNumber::new(value.start_line as usize)
            .ok_or_else(|| format!("invalid start_line: {}", value.start_line))?;
        let end_line = LineNumber::new(value.end_line as usize)
            .ok_or_else(|| format!("invalid end_line: {}", value.end_line))?;
        Ok(Self {
            holder: value.holder.clone(),
            start_line,
            end_line,
        })
    }
}
