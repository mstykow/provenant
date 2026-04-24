// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use chrono::{TimeZone, Timelike, Utc};
use std::collections::HashMap;

use super::test_utils::{dir, file};
use super::*;
use crate::assembly;
use crate::license_detection::index::{IndexedRuleMetadata, LicenseIndex};
use crate::license_detection::models::{License as RuntimeLicense, Rule, RuleKind};
use crate::models::{
    Copyright, Holder, LineNumber, Match, MatchScore, Package, PackageData, PackageType,
    PackageUid, Tallies,
};
use crate::scan_result_shaping::normalize_paths;
use serde_json::json;

mod create_output_context_test;
mod create_output_features_test;
mod license_detections_test;
mod license_references_test;
mod reference_following_local_test;
mod reference_following_package_test;

fn sample_runtime_license(key: &str, name: &str, spdx_license_key: Option<&str>) -> RuntimeLicense {
    RuntimeLicense {
        key: key.to_string(),
        short_name: Some(name.to_string()),
        name: name.to_string(),
        language: Some("en".to_string()),
        spdx_license_key: spdx_license_key.map(str::to_string),
        other_spdx_license_keys: vec![],
        category: Some("Permissive".to_string()),
        owner: Some("Example Owner".to_string()),
        homepage_url: Some("https://example.com/license".to_string()),
        text: format!("{name} text"),
        reference_urls: vec!["https://example.com/license".to_string()],
        osi_license_key: spdx_license_key.map(str::to_string),
        text_urls: vec!["https://example.com/license.txt".to_string()],
        osi_url: Some("https://opensource.org/licenses/example".to_string()),
        faq_url: Some("https://example.com/faq".to_string()),
        other_urls: vec!["https://example.com/other".to_string()],
        notes: None,
        is_deprecated: false,
        is_exception: false,
        is_unknown: false,
        is_generic: false,
        replaced_by: vec![],
        minimum_coverage: None,
        standard_notice: Some("Standard notice".to_string()),
        ignorable_copyrights: None,
        ignorable_holders: None,
        ignorable_authors: None,
        ignorable_urls: None,
        ignorable_emails: None,
    }
}

fn sample_rule(identifier: &str, expression: &str, rule_kind: RuleKind) -> Rule {
    Rule {
        identifier: identifier.to_string(),
        license_expression: expression.to_string(),
        text: format!("{identifier} text"),
        tokens: vec![],
        rule_kind,
        is_false_positive: false,
        is_required_phrase: false,
        is_from_license: false,
        relevance: 100,
        minimum_coverage: None,
        has_stored_minimum_coverage: false,
        is_continuous: true,
        required_phrase_spans: vec![],
        stopwords_by_pos: HashMap::new(),
        referenced_filenames: None,
        ignorable_urls: None,
        ignorable_emails: None,
        ignorable_copyrights: None,
        ignorable_holders: None,
        ignorable_authors: None,
        language: None,
        notes: None,
        length_unique: 0,
        high_length_unique: 0,
        high_length: 0,
        min_matched_length: 0,
        min_high_matched_length: 0,
        min_matched_length_unique: 0,
        min_high_matched_length_unique: 0,
        is_small: false,
        is_tiny: false,
        starts_with_license: false,
        ends_with_license: false,
        is_deprecated: false,
        spdx_license_key: None,
        other_spdx_license_keys: vec![],
    }
}
