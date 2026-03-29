//! Test infrastructure for license detection.
//!
//! This module provides test utilities including:
//! - A static rule cache for creating test rules with `'static` lifetimes
//! - `TestMatchBuilder` for constructing `LicenseMatch` instances in tests

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::RwLock;

use once_cell::sync::Lazy;

use crate::license_detection::index::dictionary::TokenId;
use crate::license_detection::models::rule::{Rule, RuleKind};

pub mod builder;

pub use builder::TestMatchBuilder;

static RULE_CACHE: Lazy<RwLock<HashMap<String, &'static Rule>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

pub fn get_or_create_rule(identifier: &str, license_expression: &str) -> &'static Rule {
    get_or_create_rule_with_flags(
        identifier,
        license_expression,
        RuleKind::None,
        100,
        None,
        false,
    )
}

pub fn get_or_create_false_positive_rule(
    identifier: &str,
    license_expression: &str,
) -> &'static Rule {
    get_or_create_rule_with_flags(
        identifier,
        license_expression,
        RuleKind::None,
        100,
        None,
        true,
    )
}

pub fn get_or_create_rule_with_flags(
    identifier: &str,
    license_expression: &str,
    rule_kind: RuleKind,
    relevance: u8,
    referenced_filenames: Option<Vec<String>>,
    is_false_positive: bool,
) -> &'static Rule {
    get_or_create_rule_ext(
        identifier,
        license_expression,
        rule_kind,
        relevance,
        referenced_filenames,
        false,
        0,
        0,
        None,
        is_false_positive,
        100,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn get_or_create_rule_ext(
    identifier: &str,
    license_expression: &str,
    rule_kind: RuleKind,
    relevance: u8,
    referenced_filenames: Option<Vec<String>>,
    is_small: bool,
    min_matched_length: usize,
    min_high_matched_length: usize,
    minimum_coverage: Option<u8>,
    is_false_positive: bool,
    rule_token_count: usize,
) -> &'static Rule {
    let cache_key = format!(
        "{}:{}:{:?}:{}:{:?}:{}:{}:{}:{:?}:{}:{}",
        identifier,
        license_expression,
        rule_kind,
        relevance,
        referenced_filenames,
        is_small,
        min_matched_length,
        min_high_matched_length,
        minimum_coverage,
        is_false_positive,
        rule_token_count
    );

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
        tokens: vec![TokenId::new(0); rule_token_count.max(1)],
        rule_kind,
        is_false_positive,
        is_required_phrase: false,
        is_from_license: false,
        relevance,
        minimum_coverage,
        has_stored_minimum_coverage: minimum_coverage.is_some(),
        is_continuous: false,
        required_phrase_spans: Vec::new(),
        stopwords_by_pos: HashMap::new(),
        referenced_filenames,
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
        min_matched_length,
        min_high_matched_length,
        min_matched_length_unique: 0,
        min_high_matched_length_unique: 0,
        is_small,
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
