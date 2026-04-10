use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputAuthor {
    pub author: String,
    pub start_line: u64,
    pub end_line: u64,
}

impl From<&crate::models::Author> for OutputAuthor {
    fn from(value: &crate::models::Author) -> Self {
        Self {
            author: value.author.clone(),
            start_line: value.start_line.get() as u64,
            end_line: value.end_line.get() as u64,
        }
    }
}

impl TryFrom<&OutputAuthor> for crate::models::Author {
    type Error = String;
    fn try_from(value: &OutputAuthor) -> Result<Self, Self::Error> {
        use crate::models::LineNumber;
        let start_line = LineNumber::new(value.start_line as usize)
            .ok_or_else(|| format!("invalid start_line: {}", value.start_line))?;
        let end_line = LineNumber::new(value.end_line as usize)
            .ok_or_else(|| format!("invalid end_line: {}", value.end_line))?;
        Ok(Self {
            author: value.author.clone(),
            start_line,
            end_line,
        })
    }
}
