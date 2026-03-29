//! Test match builder for constructing `LicenseMatch` instances.
//!
//! This module provides a fluent builder API for creating `LicenseMatch`
//! instances in tests with sensible defaults and `'static` rule references.

use derive_builder::Builder;

use crate::license_detection::models::license_match::{LicenseMatch, MatcherKind};
use crate::license_detection::models::rule::RuleKind;

use super::get_or_create_rule;

#[derive(Builder, Clone)]
#[builder(setter(into))]
pub struct TestMatchBuilder {
    pub license_expression: String,
    pub rule_identifier: String,
    pub start_line: usize,
    pub end_line: usize,
    pub start_token: usize,
    pub end_token: usize,
    pub score: f32,
    pub matched_length: usize,
    pub rule_length: usize,
    pub match_coverage: f32,
    pub rule_relevance: u8,
    pub matcher: MatcherKind,
    pub rule_kind: RuleKind,
    pub license_expression_spdx: Option<String>,
    pub from_file: Option<String>,
    pub rule_url: String,
    pub matched_text: Option<String>,
    pub referenced_filenames: Option<Vec<String>>,
    pub is_from_license: bool,
    pub matched_token_positions: Option<Vec<usize>>,
    pub hilen: usize,
    pub rule_start_token: usize,
    pub qspan_positions: Option<Vec<usize>>,
    pub ispan_positions: Option<Vec<usize>>,
    pub hispan_positions: Option<Vec<usize>>,
    pub candidate_resemblance: f32,
    pub candidate_containment: f32,
}

impl Default for TestMatchBuilder {
    fn default() -> Self {
        Self {
            license_expression: "MIT".to_string(),
            rule_identifier: "test-rule".to_string(),
            start_line: 0,
            end_line: 0,
            start_token: 0,
            end_token: 0,
            score: 1.0,
            matched_length: 10,
            rule_length: 10,
            match_coverage: 100.0,
            rule_relevance: 100,
            matcher: MatcherKind::Hash,
            rule_kind: RuleKind::None,
            license_expression_spdx: None,
            from_file: None,
            rule_url: String::new(),
            matched_text: None,
            referenced_filenames: None,
            is_from_license: false,
            matched_token_positions: None,
            hilen: 0,
            rule_start_token: 0,
            qspan_positions: None,
            ispan_positions: None,
            hispan_positions: None,
            candidate_resemblance: 0.0,
            candidate_containment: 0.0,
        }
    }
}

impl TestMatchBuilder {
    pub fn build_match(self) -> LicenseMatch {
        let _rule = get_or_create_rule(&self.rule_identifier, &self.license_expression);
        LicenseMatch {
            rid: 0,
            license_expression: self.license_expression,
            license_expression_spdx: self.license_expression_spdx,
            from_file: self.from_file,
            start_line: self.start_line,
            end_line: self.end_line,
            start_token: self.start_token,
            end_token: self.end_token,
            matcher: self.matcher,
            score: self.score,
            matched_length: self.matched_length,
            rule_length: self.rule_length,
            match_coverage: self.match_coverage,
            rule_relevance: self.rule_relevance,
            rule_identifier: self.rule_identifier,
            rule_url: self.rule_url,
            matched_text: self.matched_text,
            referenced_filenames: self.referenced_filenames,
            rule_kind: self.rule_kind,
            is_from_license: self.is_from_license,
            matched_token_positions: self.matched_token_positions,
            hilen: self.hilen,
            rule_start_token: self.rule_start_token,
            qspan_positions: self.qspan_positions,
            ispan_positions: self.ispan_positions,
            hispan_positions: self.hispan_positions,
            candidate_resemblance: self.candidate_resemblance,
            candidate_containment: self.candidate_containment,
        }
    }

    pub fn license_expression(mut self, value: impl Into<String>) -> Self {
        self.license_expression = value.into();
        self
    }

    pub fn rule_identifier(mut self, value: impl Into<String>) -> Self {
        self.rule_identifier = value.into();
        self
    }

    pub fn start_line(mut self, value: usize) -> Self {
        self.start_line = value;
        self
    }

    pub fn end_line(mut self, value: usize) -> Self {
        self.end_line = value;
        self
    }

    pub fn start_token(mut self, value: usize) -> Self {
        self.start_token = value;
        self
    }

    pub fn end_token(mut self, value: usize) -> Self {
        self.end_token = value;
        self
    }

    pub fn score(mut self, value: f32) -> Self {
        self.score = value;
        self
    }

    pub fn matched_length(mut self, value: usize) -> Self {
        self.matched_length = value;
        self
    }

    pub fn rule_length(mut self, value: usize) -> Self {
        self.rule_length = value;
        self
    }

    pub fn match_coverage(mut self, value: f32) -> Self {
        self.match_coverage = value;
        self
    }

    pub fn rule_relevance(mut self, value: u8) -> Self {
        self.rule_relevance = value;
        self
    }

    pub fn matcher(mut self, value: MatcherKind) -> Self {
        self.matcher = value;
        self
    }

    pub fn rule_kind(mut self, value: RuleKind) -> Self {
        self.rule_kind = value;
        self
    }

    pub fn license_expression_spdx(mut self, value: Option<String>) -> Self {
        self.license_expression_spdx = value;
        self
    }

    pub fn from_file(mut self, value: Option<String>) -> Self {
        self.from_file = value;
        self
    }

    pub fn rule_url(mut self, value: impl Into<String>) -> Self {
        self.rule_url = value.into();
        self
    }

    pub fn matched_text(mut self, value: Option<String>) -> Self {
        self.matched_text = value;
        self
    }

    pub fn referenced_filenames(mut self, value: Option<Vec<String>>) -> Self {
        self.referenced_filenames = value;
        self
    }

    pub fn is_from_license(mut self, value: bool) -> Self {
        self.is_from_license = value;
        self
    }

    pub fn matched_token_positions(mut self, value: Option<Vec<usize>>) -> Self {
        self.matched_token_positions = value;
        self
    }

    pub fn hilen(mut self, value: usize) -> Self {
        self.hilen = value;
        self
    }

    pub fn rule_start_token(mut self, value: usize) -> Self {
        self.rule_start_token = value;
        self
    }

    pub fn qspan_positions(mut self, value: Option<Vec<usize>>) -> Self {
        self.qspan_positions = value;
        self
    }

    pub fn ispan_positions(mut self, value: Option<Vec<usize>>) -> Self {
        self.ispan_positions = value;
        self
    }

    pub fn hispan_positions(mut self, value: Option<Vec<usize>>) -> Self {
        self.hispan_positions = value;
        self
    }

    pub fn candidate_resemblance(mut self, value: f32) -> Self {
        self.candidate_resemblance = value;
        self
    }

    pub fn candidate_containment(mut self, value: f32) -> Self {
        self.candidate_containment = value;
        self
    }
}

impl TestMatchBuilderBuilder {
    pub fn build_match(self) -> LicenseMatch {
        self.build()
            .expect("TestMatchBuilder has all required fields with defaults")
            .build_match()
    }
}
