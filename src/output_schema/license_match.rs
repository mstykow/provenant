use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct OutputMatch {
    pub license_expression: String,
    pub license_expression_spdx: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_file: Option<String>,
    pub start_line: u64,
    pub end_line: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matcher: Option<String>,
    pub score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_length: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_coverage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_relevance: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_identifier: Option<String>,
    pub rule_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_text_diagnostics: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub referenced_filenames: Option<Vec<String>>,
}

impl From<&crate::models::Match> for OutputMatch {
    fn from(value: &crate::models::Match) -> Self {
        Self {
            license_expression: value.license_expression.clone(),
            license_expression_spdx: value.license_expression_spdx.clone(),
            from_file: value.from_file.clone(),
            start_line: value.start_line.get() as u64,
            end_line: value.end_line.get() as u64,
            matcher: value.matcher.clone(),
            score: value.score,
            matched_length: value.matched_length,
            match_coverage: value.match_coverage,
            rule_relevance: value.rule_relevance,
            rule_identifier: value.rule_identifier.clone(),
            rule_url: value.rule_url.clone(),
            matched_text: value.matched_text.clone(),
            matched_text_diagnostics: value.matched_text_diagnostics.clone(),
            referenced_filenames: value.referenced_filenames.clone(),
        }
    }
}

impl TryFrom<&OutputMatch> for crate::models::Match {
    type Error = String;
    fn try_from(value: &OutputMatch) -> Result<Self, Self::Error> {
        use crate::models::LineNumber;
        let start_line = LineNumber::new(value.start_line as usize)
            .ok_or_else(|| format!("invalid start_line: {}", value.start_line))?;
        let end_line = LineNumber::new(value.end_line as usize)
            .ok_or_else(|| format!("invalid end_line: {}", value.end_line))?;
        Ok(Self {
            license_expression: value.license_expression.clone(),
            license_expression_spdx: value.license_expression_spdx.clone(),
            from_file: value.from_file.clone(),
            start_line,
            end_line,
            matcher: value.matcher.clone(),
            score: value.score,
            matched_length: value.matched_length,
            match_coverage: value.match_coverage,
            rule_relevance: value.rule_relevance,
            rule_identifier: value.rule_identifier.clone(),
            rule_url: value.rule_url.clone(),
            matched_text: value.matched_text.clone(),
            matched_text_diagnostics: value.matched_text_diagnostics.clone(),
            referenced_filenames: value.referenced_filenames.clone(),
        })
    }
}
