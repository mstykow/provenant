// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputLicenseRuleReference {
    pub identifier: String,
    pub license_expression: String,
    pub is_license_text: bool,
    pub is_license_notice: bool,
    pub is_license_reference: bool,
    pub is_license_tag: bool,
    pub is_license_clue: bool,
    pub is_license_intro: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_url: Option<String>,
    #[serde(default)]
    pub is_required_phrase: bool,
    #[serde(default)]
    pub skip_for_required_phrase_generation: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replaced_by: Vec<String>,
    #[serde(default)]
    pub is_continuous: bool,
    #[serde(default)]
    pub is_synthetic: bool,
    #[serde(default)]
    pub is_from_license: bool,
    #[serde(default)]
    pub length: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relevance: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_coverage: Option<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub referenced_filenames: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignorable_copyrights: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignorable_holders: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignorable_authors: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignorable_urls: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignorable_emails: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

impl From<&crate::models::LicenseRuleReference> for OutputLicenseRuleReference {
    fn from(value: &crate::models::LicenseRuleReference) -> Self {
        Self {
            identifier: value.identifier.clone(),
            license_expression: value.license_expression.clone(),
            is_license_text: value.is_license_text,
            is_license_notice: value.is_license_notice,
            is_license_reference: value.is_license_reference,
            is_license_tag: value.is_license_tag,
            is_license_clue: value.is_license_clue,
            is_license_intro: value.is_license_intro,
            language: value.language.clone(),
            rule_url: value.rule_url.clone(),
            is_required_phrase: value.is_required_phrase,
            skip_for_required_phrase_generation: value.skip_for_required_phrase_generation,
            replaced_by: value.replaced_by.clone(),
            is_continuous: value.is_continuous,
            is_synthetic: value.is_synthetic,
            is_from_license: value.is_from_license,
            length: value.length,
            relevance: value.relevance,
            minimum_coverage: value.minimum_coverage,
            referenced_filenames: value.referenced_filenames.clone(),
            notes: value.notes.clone(),
            ignorable_copyrights: value.ignorable_copyrights.clone(),
            ignorable_holders: value.ignorable_holders.clone(),
            ignorable_authors: value.ignorable_authors.clone(),
            ignorable_urls: value.ignorable_urls.clone(),
            ignorable_emails: value.ignorable_emails.clone(),
            text: value.text.clone(),
        }
    }
}

impl TryFrom<&OutputLicenseRuleReference> for crate::models::LicenseRuleReference {
    type Error = String;
    fn try_from(value: &OutputLicenseRuleReference) -> Result<Self, Self::Error> {
        Ok(Self {
            identifier: value.identifier.clone(),
            license_expression: value.license_expression.clone(),
            is_license_text: value.is_license_text,
            is_license_notice: value.is_license_notice,
            is_license_reference: value.is_license_reference,
            is_license_tag: value.is_license_tag,
            is_license_clue: value.is_license_clue,
            is_license_intro: value.is_license_intro,
            language: value.language.clone(),
            rule_url: value.rule_url.clone(),
            is_required_phrase: value.is_required_phrase,
            skip_for_required_phrase_generation: value.skip_for_required_phrase_generation,
            replaced_by: value.replaced_by.clone(),
            is_continuous: value.is_continuous,
            is_synthetic: value.is_synthetic,
            is_from_license: value.is_from_license,
            length: value.length,
            relevance: value.relevance,
            minimum_coverage: value.minimum_coverage,
            referenced_filenames: value.referenced_filenames.clone(),
            notes: value.notes.clone(),
            ignorable_copyrights: value.ignorable_copyrights.clone(),
            ignorable_holders: value.ignorable_holders.clone(),
            ignorable_authors: value.ignorable_authors.clone(),
            ignorable_urls: value.ignorable_urls.clone(),
            ignorable_emails: value.ignorable_emails.clone(),
            text: value.text.clone(),
        })
    }
}
