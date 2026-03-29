//! License metadata loaded from .LICENSE files.

use serde::{Deserialize, Serialize};

/// License metadata loaded from .LICENSE files.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct License {
    pub key: String,
    pub short_name: Option<String>,
    pub name: String,
    pub language: Option<String>,
    pub spdx_license_key: Option<String>,
    pub other_spdx_license_keys: Vec<String>,
    pub category: Option<String>,
    pub owner: Option<String>,
    pub homepage_url: Option<String>,
    pub text: String,
    pub reference_urls: Vec<String>,
    pub osi_license_key: Option<String>,
    pub text_urls: Vec<String>,
    pub osi_url: Option<String>,
    pub faq_url: Option<String>,
    pub other_urls: Vec<String>,
    pub notes: Option<String>,
    pub is_deprecated: bool,
    pub is_exception: bool,
    pub is_unknown: bool,
    pub is_generic: bool,
    pub replaced_by: Vec<String>,
    pub minimum_coverage: Option<u8>,
    pub standard_notice: Option<String>,
    pub ignorable_copyrights: Option<Vec<String>>,
    pub ignorable_holders: Option<Vec<String>>,
    pub ignorable_authors: Option<Vec<String>>,
    pub ignorable_urls: Option<Vec<String>>,
    pub ignorable_emails: Option<Vec<String>>,
}
