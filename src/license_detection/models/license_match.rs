//! License match result from a matching strategy.

use serde::Serialize;
use std::fmt;
use std::str::FromStr;

use crate::license_detection::models::position_span::PositionSpan;
use crate::license_detection::models::RuleKind;
use crate::license_detection::position_set::PositionSet;

/// Internal matcher kind used to create a license match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize)]
pub enum MatcherKind {
    #[serde(rename = "1-hash")]
    #[default]
    Hash,
    #[serde(rename = "1-spdx-id", alias = "3-spdx")]
    SpdxId,
    #[serde(rename = "2-aho")]
    Aho,
    #[serde(rename = "3-seq", alias = "4-seq")]
    Seq,
    #[serde(rename = "5-undetected", alias = "undetected")]
    Undetected,
    #[serde(rename = "6-unknown")]
    Unknown,
}

impl MatcherKind {
    pub const fn precedence(self) -> u8 {
        match self {
            Self::Hash => 0,
            Self::Aho => 1,
            Self::SpdxId => 2,
            Self::Seq => 3,
            Self::Undetected => 4,
            Self::Unknown => 6,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hash => "1-hash",
            Self::SpdxId => "1-spdx-id",
            Self::Aho => "2-aho",
            Self::Seq => "3-seq",
            Self::Undetected => "5-undetected",
            Self::Unknown => "6-unknown",
        }
    }
}

impl fmt::Display for MatcherKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MatcherKind {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "1-hash" => Ok(Self::Hash),
            "1-spdx-id" | "3-spdx" => Ok(Self::SpdxId),
            "2-aho" => Ok(Self::Aho),
            "3-seq" | "4-seq" => Ok(Self::Seq),
            "5-undetected" | "undetected" => Ok(Self::Undetected),
            "6-unknown" => Ok(Self::Unknown),
            _ => Err("unknown matcher kind"),
        }
    }
}

/// License match result from a matching strategy.
#[derive(Debug, Clone, PartialEq)]
pub struct LicenseMatch {
    /// Internal rule ID for fast lookups (index into rules_by_rid).
    /// Not serialized to JSON output.
    pub rid: usize,

    /// License expression string using ScanCode license keys
    pub license_expression: String,

    /// SPDX rendering of the license expression when it is known.
    pub license_expression_spdx: Option<String>,

    /// File where match was found (if applicable)
    pub from_file: Option<String>,

    /// Start line number (1-indexed)
    pub start_line: usize,

    /// End line number (1-indexed)
    pub end_line: usize,

    /// Start token position (0-indexed in query token stream)
    /// Used for dual-criteria match grouping with token gap threshold.
    pub start_token: usize,

    /// End token position (0-indexed, exclusive)
    /// Used for dual-criteria match grouping with token gap threshold.
    pub end_token: usize,

    /// Matching strategy used to create this match.
    pub matcher: MatcherKind,

    /// Match score 0.0-1.0
    pub score: f32,

    /// Length of matched text in characters
    pub matched_length: usize,

    /// Token count of the matched rule (from rule.tokens.len())
    /// Used for false positive detection instead of matched_length.
    pub rule_length: usize,

    /// Match coverage as percentage 0.0-100.0
    pub match_coverage: f32,

    /// Relevance of the matched rule (0-100)
    pub rule_relevance: u8,

    /// Unique identifier for the matched rule
    pub rule_identifier: String,

    /// URL for the matched rule
    pub rule_url: String,

    /// Matched text snippet (optional for privacy/performance)
    pub matched_text: Option<String>,

    /// Filenames referenced by this match (e.g., ["LICENSE"] for "See LICENSE file")
    /// Populated from rule.referenced_filenames when rule matches
    pub referenced_filenames: Option<Vec<String>>,

    /// Classification of the rule that produced this match.
    pub rule_kind: RuleKind,

    /// True if this match is from a rule created from a license file (not a .RULE file)
    /// Rules from LICENSE files have relevance=100 and should take priority over decomposed expressions.
    pub is_from_license: bool,

    /// Rule-side start position (where in the rule text the match starts).
    ///
    /// This is Python's "istart" - the position in the rule, not the query.
    /// Used by `ispan()` to return rule-side positions for required phrase checking.
    ///
    /// For exact matches (hash, aho), this is always 0.
    /// For approximate matches (seq), this is the position in the rule where alignment begins.
    pub rule_start_token: usize,

    /// Token positions matched in the query text.
    pub qspan: PositionSpan,

    /// Token positions matched in the rule text.
    pub ispan: PositionSpan,

    /// Token positions in the rule that are high-value legalese tokens.
    pub hispan: PositionSpan,

    /// Candidate resemblance score from set similarity.
    /// Used for cross-license tie-breaking when matches overlap.
    /// Higher resemblance means better candidate quality.
    pub candidate_resemblance: f32,

    /// Candidate containment score from set similarity.
    /// Used for cross-license tie-breaking when matches overlap.
    /// Higher containment means more of the rule is matched.
    pub candidate_containment: f32,
}

#[derive(Serialize)]
struct SerializableLicenseMatch<'a> {
    license_expression: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    license_expression_spdx: &'a Option<String>,
    from_file: &'a Option<String>,
    start_line: usize,
    end_line: usize,
    start_token: usize,
    end_token: usize,
    matcher: MatcherKind,
    score: f32,
    matched_length: usize,
    rule_length: usize,
    match_coverage: f32,
    rule_relevance: u8,
    rule_identifier: &'a str,
    rule_url: &'a str,
    matched_text: &'a Option<String>,
    referenced_filenames: &'a Option<Vec<String>>,
    is_license_text: bool,
    is_license_notice: bool,
    is_license_intro: bool,
    is_license_clue: bool,
    is_license_reference: bool,
    is_license_tag: bool,
    is_from_license: bool,
    hilen: usize,
    rule_start_token: usize,
    candidate_resemblance: f32,
    candidate_containment: f32,
}

impl Serialize for LicenseMatch {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        SerializableLicenseMatch {
            license_expression: &self.license_expression,
            license_expression_spdx: &self.license_expression_spdx,
            from_file: &self.from_file,
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
            rule_identifier: &self.rule_identifier,
            rule_url: &self.rule_url,
            matched_text: &self.matched_text,
            referenced_filenames: &self.referenced_filenames,
            is_license_text: self.is_license_text(),
            is_license_notice: self.is_license_notice(),
            is_license_intro: self.is_license_intro(),
            is_license_clue: self.is_license_clue(),
            is_license_reference: self.is_license_reference(),
            is_license_tag: self.is_license_tag(),
            is_from_license: self.is_from_license,
            hilen: self.hilen(),
            rule_start_token: self.rule_start_token,
            candidate_resemblance: self.candidate_resemblance,
            candidate_containment: self.candidate_containment,
        }
        .serialize(serializer)
    }
}

impl Default for LicenseMatch {
    fn default() -> Self {
        LicenseMatch {
            rid: 0,
            license_expression: String::new(),
            license_expression_spdx: None,
            from_file: None,
            start_line: 0,
            end_line: 0,
            start_token: 0,
            end_token: 0,
            matcher: MatcherKind::default(),
            score: 0.0,
            matched_length: 0,
            rule_length: 0,
            match_coverage: 0.0,
            rule_relevance: 0,
            rule_identifier: String::new(),
            rule_url: String::new(),
            matched_text: None,
            referenced_filenames: None,
            rule_kind: RuleKind::None,
            is_from_license: false,
            rule_start_token: 0,
            qspan: PositionSpan::empty(),
            ispan: PositionSpan::empty(),
            hispan: PositionSpan::empty(),
            candidate_resemblance: 0.0,
            candidate_containment: 0.0,
        }
    }
}

impl LicenseMatch {
    pub(crate) fn round_metric(value: f32) -> f32 {
        ((value * 100.0).round() / 100.0).min(100.0)
    }

    pub fn coverage(&self) -> f32 {
        Self::round_metric(self.match_coverage)
    }

    pub fn matcher_order(&self) -> u8 {
        self.matcher.precedence()
    }

    pub const fn is_license_text(&self) -> bool {
        self.rule_kind.is_license_text()
    }

    pub const fn is_license_notice(&self) -> bool {
        self.rule_kind.is_license_notice()
    }

    pub const fn is_license_reference(&self) -> bool {
        self.rule_kind.is_license_reference()
    }

    pub const fn is_license_tag(&self) -> bool {
        self.rule_kind.is_license_tag()
    }

    pub const fn is_license_intro(&self) -> bool {
        self.rule_kind.is_license_intro()
    }

    pub const fn is_license_clue(&self) -> bool {
        self.rule_kind.is_license_clue()
    }

    pub fn hilen(&self) -> usize {
        self.hispan.len()
    }

    pub fn qstart(&self) -> usize {
        let (min, _) = self.effective_span().bounds();
        min
    }

    pub fn effective_span(&self) -> PositionSpan {
        if !self.qspan.is_empty() {
            self.qspan.clone()
        } else if self.start_token < self.end_token {
            PositionSpan::range(self.start_token, self.end_token)
        } else {
            PositionSpan::empty()
        }
    }

    fn effective_ispan(&self) -> PositionSpan {
        if !self.ispan.is_empty() {
            self.ispan.clone()
        } else if self.matched_length > 0 {
            PositionSpan::range(
                self.rule_start_token,
                self.rule_start_token + self.matched_length,
            )
        } else {
            PositionSpan::empty()
        }
    }

    pub fn is_small(
        &self,
        min_matched_len: usize,
        min_high_matched_len: usize,
        rule_is_small: bool,
    ) -> bool {
        if self.matched_length < min_matched_len || self.hilen() < min_high_matched_len {
            return true;
        }
        if rule_is_small && self.coverage() < 80.0 {
            return true;
        }
        false
    }

    pub(crate) fn len(&self) -> usize {
        self.effective_span().len()
    }

    fn qregion_len(&self) -> usize {
        let (min, max) = self.effective_span().bounds();
        max.saturating_sub(min)
    }

    pub fn qmagnitude(&self, query: &crate::license_detection::query::Query) -> usize {
        let span = self.effective_span();
        let qregion_len = self.qregion_len();
        if span.is_empty() {
            return qregion_len;
        }
        let (_, end_exclusive) = span.bounds();
        let max_pos = end_exclusive.saturating_sub(1);
        let unknowns_in_match: usize = span
            .iter()
            .filter(|&pos| pos != max_pos)
            .filter_map(|pos| query.unknowns_by_pos.get(&Some(pos as i32)))
            .sum();
        qregion_len + unknowns_in_match
    }

    pub fn qdensity(&self, query: &crate::license_detection::query::Query) -> f32 {
        let mlen = self.len();
        if mlen == 0 {
            return 0.0;
        }
        let qmag = self.qmagnitude(query);
        if qmag == 0 {
            return 0.0;
        }
        mlen as f32 / qmag as f32
    }

    pub fn idensity(&self) -> f32 {
        let ispan = self.effective_ispan();
        let ispan_len = ispan.len();
        if ispan_len == 0 {
            return 0.0;
        }
        let (min, max) = ispan.bounds();
        let ispan_magnitude = max.saturating_sub(min);
        if ispan_magnitude == 0 {
            return 0.0;
        }
        ispan_len as f32 / ispan_magnitude as f32
    }

    pub fn icoverage(&self) -> f32 {
        if self.rule_length == 0 {
            return 0.0;
        }
        self.len() as f32 / self.rule_length as f32
    }

    pub fn surround(&self, other: &LicenseMatch) -> bool {
        let (self_qstart, self_qend) = self.qspan_bounds();
        let (other_qstart, other_qend) = other.qspan_bounds();
        self_qstart <= other_qstart && self_qend >= other_qend
    }

    pub fn qcontains(&self, other: &LicenseMatch) -> bool {
        let self_span = self.effective_span();
        let other_span = other.effective_span();

        if self_span.is_empty() && other_span.is_empty() {
            return self.start_line <= other.start_line && self.end_line >= other.end_line;
        }

        other_span.iter().all(|pos| self_span.contains(pos))
    }

    pub fn qoverlap(&self, other: &LicenseMatch) -> usize {
        let self_span = self.effective_span();
        let other_span = other.effective_span();

        if self_span.is_empty() && other_span.is_empty() {
            let start = self.start_line.max(other.start_line);
            let end = self.end_line.min(other.end_line);
            return if start <= end { end - start + 1 } else { 0 };
        }

        other_span.iter().filter(|&p| self_span.contains(p)).count()
    }

    pub fn qspan_overlap(&self, other: &LicenseMatch) -> usize {
        let self_set = self.effective_span().to_position_set();
        let other_set = other.effective_span().to_position_set();
        self_set.intersection_len(&other_set)
    }

    /// Return true if all matched tokens are continuous without gaps or unknowns.
    /// Python: len() == qregion_len() == qmagnitude()
    pub fn is_continuous(&self, query: &crate::license_detection::query::Query) -> bool {
        if !self.effective_span().is_contiguous() {
            return false;
        }
        let len = self.len();
        let qregion_len = self.qregion_len();
        let qmagnitude = self.qmagnitude(query);
        len == qregion_len && qregion_len == qmagnitude
    }

    pub fn overlaps_with(&self, other: &PositionSet) -> bool {
        self.effective_span().overlaps_set(other)
    }

    pub fn qspan_eq(&self, other: &LicenseMatch) -> bool {
        let self_span = self.effective_span();
        let other_span = other.effective_span();

        if self_span.is_empty() && other_span.is_empty() {
            self.start_line == other.start_line && self.end_line == other.end_line
        } else {
            self_span == other_span
        }
    }

    pub fn qdistance_to(&self, other: &LicenseMatch) -> usize {
        if self.qoverlap(other) > 0 {
            return 0;
        }

        let (self_start, self_end_exclusive) = self.qspan_bounds();
        let (other_start, other_end_exclusive) = other.qspan_bounds();
        let self_end = self_end_exclusive.saturating_sub(1);
        let other_end = other_end_exclusive.saturating_sub(1);

        if self_end + 1 == other_start || other_end + 1 == self_start {
            return 1;
        }

        if self_end < other_start {
            other_start.saturating_sub(self_end)
        } else {
            self_start.saturating_sub(other_end)
        }
    }

    pub fn qspan_bounds(&self) -> (usize, usize) {
        self.effective_span().bounds()
    }

    pub fn qspan_magnitude(&self) -> usize {
        let (start, end) = self.qspan_bounds();
        end.saturating_sub(start)
    }

    pub fn ispan_bounds(&self) -> (usize, usize) {
        self.effective_ispan().bounds()
    }

    pub fn idistance_to(&self, other: &LicenseMatch) -> usize {
        let (self_start, self_end) = self.ispan_bounds();
        let (other_start, other_end) = other.ispan_bounds();

        if self_start < other_end && other_start < self_end {
            return 0;
        }

        if self_end == other_start || other_end == self_start {
            return 1;
        }

        if self_end <= other_start {
            other_start - self_end
        } else {
            self_start - other_end
        }
    }

    pub fn is_after(&self, other: &LicenseMatch) -> bool {
        let (self_qstart, _self_qend) = self.qspan_bounds();
        let (_other_qstart, other_qend) = other.qspan_bounds();

        let q_after = self_qstart >= other_qend;

        let (self_istart, _self_iend) = self.ispan_bounds();
        let (_other_istart, other_iend) = other.ispan_bounds();

        let i_after = self_istart >= other_iend;

        q_after && i_after
    }

    pub fn ispan_overlap(&self, other: &LicenseMatch) -> usize {
        let self_set = self.effective_ispan().to_position_set();
        other
            .effective_ispan()
            .iter()
            .filter(|&p| self_set.contains(p))
            .count()
    }

    pub fn has_unknown(&self) -> bool {
        self.license_expression.contains("unknown")
    }
}
