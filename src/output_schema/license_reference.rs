use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputLicenseReference {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub name: String,
    pub short_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage_url: Option<String>,
    pub spdx_license_key: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub other_spdx_license_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub osi_license_key: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text_urls: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub osi_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faq_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub other_urls: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default)]
    pub is_exception: bool,
    #[serde(default)]
    pub is_unknown: bool,
    #[serde(default)]
    pub is_generic: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_coverage: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standard_notice: Option<String>,
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
    pub scancode_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub licensedb_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spdx_url: Option<String>,
    pub text: String,
}

impl From<&crate::models::LicenseReference> for OutputLicenseReference {
    fn from(value: &crate::models::LicenseReference) -> Self {
        Self {
            key: value.key.clone(),
            language: value.language.clone(),
            name: value.name.clone(),
            short_name: value.short_name.clone(),
            owner: value.owner.clone(),
            homepage_url: value.homepage_url.clone(),
            spdx_license_key: value.spdx_license_key.clone(),
            other_spdx_license_keys: value.other_spdx_license_keys.clone(),
            osi_license_key: value.osi_license_key.clone(),
            text_urls: value.text_urls.clone(),
            osi_url: value.osi_url.clone(),
            faq_url: value.faq_url.clone(),
            other_urls: value.other_urls.clone(),
            category: value.category.clone(),
            is_exception: value.is_exception,
            is_unknown: value.is_unknown,
            is_generic: value.is_generic,
            notes: value.notes.clone(),
            minimum_coverage: value.minimum_coverage,
            standard_notice: value.standard_notice.clone(),
            ignorable_copyrights: value.ignorable_copyrights.clone(),
            ignorable_holders: value.ignorable_holders.clone(),
            ignorable_authors: value.ignorable_authors.clone(),
            ignorable_urls: value.ignorable_urls.clone(),
            ignorable_emails: value.ignorable_emails.clone(),
            scancode_url: value.scancode_url.clone(),
            licensedb_url: value.licensedb_url.clone(),
            spdx_url: value.spdx_url.clone(),
            text: value.text.clone(),
        }
    }
}

impl TryFrom<&OutputLicenseReference> for crate::models::LicenseReference {
    type Error = String;
    fn try_from(value: &OutputLicenseReference) -> Result<Self, Self::Error> {
        Ok(Self {
            key: value.key.clone(),
            language: value.language.clone(),
            name: value.name.clone(),
            short_name: value.short_name.clone(),
            owner: value.owner.clone(),
            homepage_url: value.homepage_url.clone(),
            spdx_license_key: value.spdx_license_key.clone(),
            other_spdx_license_keys: value.other_spdx_license_keys.clone(),
            osi_license_key: value.osi_license_key.clone(),
            text_urls: value.text_urls.clone(),
            osi_url: value.osi_url.clone(),
            faq_url: value.faq_url.clone(),
            other_urls: value.other_urls.clone(),
            category: value.category.clone(),
            is_exception: value.is_exception,
            is_unknown: value.is_unknown,
            is_generic: value.is_generic,
            notes: value.notes.clone(),
            minimum_coverage: value.minimum_coverage,
            standard_notice: value.standard_notice.clone(),
            ignorable_copyrights: value.ignorable_copyrights.clone(),
            ignorable_holders: value.ignorable_holders.clone(),
            ignorable_authors: value.ignorable_authors.clone(),
            ignorable_urls: value.ignorable_urls.clone(),
            ignorable_emails: value.ignorable_emails.clone(),
            scancode_url: value.scancode_url.clone(),
            licensedb_url: value.licensedb_url.clone(),
            spdx_url: value.spdx_url.clone(),
            text: value.text.clone(),
        })
    }
}
