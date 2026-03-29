//! Test infrastructure for license detection.
//!
//! This module provides test utilities including:
//! - A static rule cache for creating test rules with `'static` lifetimes
//! - `TestMatchBuilder` for constructing `LicenseMatch` instances in tests

use std::collections::HashMap;
use std::sync::RwLock;

use once_cell::sync::Lazy;

use crate::license_detection::models::rule::{Rule, RuleKind};

pub mod builder;

pub use builder::TestMatchBuilder;

static RULE_CACHE: Lazy<RwLock<HashMap<String, &'static Rule>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

pub fn get_or_create_rule(identifier: &str, license_expression: &str) -> &'static Rule {
    let cache_key = format!("{}:{}", identifier, license_expression);

    {
        let cache = RULE_CACHE.read().unwrap();
        if let Some(rule) = cache.get(&cache_key) {
            return rule;
        }
    }

    let rule = Box::new(Rule {
        identifier: identifier.to_string(),
        license_expression: license_expression.to_string(),
        text: String::new(),
        tokens: Vec::new(),
        rule_kind: RuleKind::None,
        is_false_positive: false,
        is_required_phrase: false,
        is_from_license: false,
        relevance: 100,
        minimum_coverage: None,
        has_stored_minimum_coverage: false,
        is_continuous: false,
        required_phrase_spans: Vec::new(),
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
        other_spdx_license_keys: Vec::new(),
    });

    let rule_ref: &'static Rule = Box::leak(rule);

    {
        let mut cache = RULE_CACHE.write().unwrap();
        cache.insert(cache_key, rule_ref);
    }

    rule_ref
}
