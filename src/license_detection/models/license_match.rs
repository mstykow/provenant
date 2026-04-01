//! License match result from a matching strategy.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;

use crate::license_detection::models::RuleKind;
use crate::license_detection::position_set::PositionSet;

fn default_rule_length() -> usize {
    0
}

pub enum SpanIter<'a> {
    Slice(std::iter::Copied<std::slice::Iter<'a, usize>>),
    Range(std::ops::Range<usize>),
}

impl<'a> Iterator for SpanIter<'a> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            SpanIter::Slice(iter) => iter.next(),
            SpanIter::Range(range) => range.next(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            SpanIter::Slice(iter) => iter.size_hint(),
            SpanIter::Range(range) => range.size_hint(),
        }
    }
}

/// Internal matcher kind used to create a license match.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
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

    /// Token positions matched by this license (for span subtraction).
    ///
    /// Populated during matching to enable double-match prevention.
    /// None means contiguous range [start_token, end_token).
    /// Some(positions) contains the exact positions for non-contiguous matches.
    pub matched_token_positions: Option<Vec<usize>>,

    /// Count of matched high-value legalese tokens (token IDs < len_legalese).
    ///
    /// Corresponds to Python's `len(self.hispan)` - the number of matched positions
    /// where the token ID is a high-value legalese token.
    pub hilen: usize,

    /// Rule-side start position (where in the rule text the match starts).
    ///
    /// This is Python's "istart" - the position in the rule, not the query.
    /// Used by `ispan()` to return rule-side positions for required phrase checking.
    ///
    /// For exact matches (hash, aho), this is always 0.
    /// For approximate matches (seq), this is the position in the rule where alignment begins.
    pub rule_start_token: usize,

    /// Token positions matched in the query text.
    /// None means contiguous range [start_token, end_token).
    /// Some(positions) contains exact positions for non-contiguous matches (after merge).
    pub qspan_positions: Option<Vec<usize>>,

    /// Token positions matched in the rule text.
    /// None means contiguous range [rule_start_token, rule_start_token + matched_length).
    /// Some(positions) contains exact positions for non-contiguous matches (after merge).
    pub ispan_positions: Option<Vec<usize>>,

    /// Token positions in the rule that are high-value legalese tokens.
    /// None means hispan can be computed from rule_start_token (contiguous case).
    /// Some(positions) contains exact positions for non-contiguous hispans (after merge).
    pub hispan_positions: Option<Vec<usize>>,

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

/// Deserializable form of LicenseMatch for JSON parsing.
///
/// Note: `is_license_notice` is included for deserialization completeness but
/// is not passed to `from_match_flags` because match objects cannot have this
/// flag (only rule objects can). This is correct behavior matching Python.
#[derive(Deserialize)]
struct DeserializableLicenseMatch {
    #[serde(default)]
    license_expression: String,
    #[serde(default)]
    license_expression_spdx: Option<String>,
    #[serde(default)]
    from_file: Option<String>,
    start_line: usize,
    end_line: usize,
    #[serde(default)]
    start_token: usize,
    #[serde(default)]
    end_token: usize,
    matcher: MatcherKind,
    score: f32,
    matched_length: usize,
    #[serde(default = "default_rule_length")]
    rule_length: usize,
    match_coverage: f32,
    rule_relevance: u8,
    #[serde(default)]
    rule_identifier: String,
    #[serde(default)]
    rule_url: String,
    #[serde(default)]
    matched_text: Option<String>,
    #[serde(default)]
    referenced_filenames: Option<Vec<String>>,
    #[serde(default)]
    is_license_text: bool,
    #[serde(default)]
    #[allow(dead_code)] // see comment above
    is_license_notice: bool,
    #[serde(default)]
    is_license_intro: bool,
    #[serde(default)]
    is_license_clue: bool,
    #[serde(default)]
    is_license_reference: bool,
    #[serde(default)]
    is_license_tag: bool,
    #[serde(default)]
    is_from_license: bool,
    #[serde(default)]
    hilen: usize,
    #[serde(default)]
    rule_start_token: usize,
    #[serde(default)]
    candidate_resemblance: f32,
    #[serde(default)]
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
            hilen: self.hilen,
            rule_start_token: self.rule_start_token,
            candidate_resemblance: self.candidate_resemblance,
            candidate_containment: self.candidate_containment,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for LicenseMatch {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = DeserializableLicenseMatch::deserialize(deserializer)?;
        let rule_kind = RuleKind::from_match_flags(
            value.is_license_text,
            value.is_license_reference,
            value.is_license_tag,
            value.is_license_intro,
            value.is_license_clue,
        )
        .map_err(D::Error::custom)?;

        Ok(Self {
            rid: 0,
            license_expression: value.license_expression,
            license_expression_spdx: value.license_expression_spdx,
            from_file: value.from_file,
            start_line: value.start_line,
            end_line: value.end_line,
            start_token: value.start_token,
            end_token: value.end_token,
            matcher: value.matcher,
            score: value.score,
            matched_length: value.matched_length,
            rule_length: value.rule_length,
            match_coverage: value.match_coverage,
            rule_relevance: value.rule_relevance,
            rule_identifier: value.rule_identifier,
            rule_url: value.rule_url,
            matched_text: value.matched_text,
            referenced_filenames: value.referenced_filenames,
            rule_kind,
            is_from_license: value.is_from_license,
            matched_token_positions: None,
            hilen: value.hilen,
            rule_start_token: value.rule_start_token,
            qspan_positions: None,
            ispan_positions: None,
            hispan_positions: None,
            candidate_resemblance: value.candidate_resemblance,
            candidate_containment: value.candidate_containment,
        })
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
        self.hilen
    }

    pub fn qstart(&self) -> usize {
        if let Some(positions) = &self.qspan_positions {
            positions.iter().copied().min().unwrap_or(self.start_token)
        } else {
            self.start_token
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
        if let Some(positions) = &self.qspan_positions {
            positions.len()
        } else if let Some(positions) = &self.matched_token_positions {
            positions.len()
        } else {
            self.end_token.saturating_sub(self.start_token)
        }
    }

    fn qregion_len(&self) -> usize {
        if let Some(positions) = &self.qspan_positions {
            if positions.is_empty() {
                return 0;
            }
            let min_pos = *positions.iter().min().unwrap_or(&0);
            let max_pos = *positions.iter().max().unwrap_or(&0);
            max_pos - min_pos + 1
        } else if let Some(positions) = &self.matched_token_positions {
            if positions.is_empty() {
                return 0;
            }
            let min_pos = *positions.iter().min().unwrap_or(&0);
            let max_pos = *positions.iter().max().unwrap_or(&0);
            max_pos - min_pos + 1
        } else {
            self.end_token.saturating_sub(self.start_token)
        }
    }

    pub fn qmagnitude(&self, query: &crate::license_detection::query::Query) -> usize {
        let qregion_len = self.qregion_len();
        let positions: Vec<usize> = if let Some(qspan_positions) = &self.qspan_positions {
            qspan_positions.clone()
        } else {
            (self.start_token..self.end_token).collect()
        };
        if positions.is_empty() {
            return qregion_len;
        }
        let max_pos = *positions.iter().max().unwrap_or(&0);
        let unknowns_in_match: usize = positions
            .iter()
            .filter(|&&pos| pos != max_pos)
            .filter_map(|&pos| query.unknowns_by_pos.get(&Some(pos as i32)))
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
        let ispan_len = if let Some(positions) = &self.ispan_positions {
            positions.len()
        } else {
            self.matched_length
        };
        if ispan_len == 0 {
            return 0.0;
        }
        let ispan_magnitude = if let Some(positions) = &self.ispan_positions {
            if positions.is_empty() {
                return 0.0;
            }
            let min_pos = *positions.iter().min().unwrap();
            let max_pos = *positions.iter().max().unwrap();
            max_pos - min_pos + 1
        } else {
            self.matched_length
        };
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
        if let (Some(self_positions), Some(other_positions)) =
            (&self.qspan_positions, &other.qspan_positions)
        {
            let self_set: HashSet<usize> = self_positions.iter().copied().collect();
            return other_positions.iter().all(|p| self_set.contains(p));
        }

        if let (Some(self_positions), None) = (&self.qspan_positions, &other.qspan_positions) {
            let self_set: HashSet<usize> = self_positions.iter().copied().collect();
            return (other.start_token..other.end_token).all(|p| self_set.contains(&p));
        }

        if let (None, Some(other_positions)) = (&self.qspan_positions, &other.qspan_positions) {
            return other_positions
                .iter()
                .all(|&p| p >= self.start_token && p < self.end_token);
        }

        if self.start_token == 0
            && self.end_token == 0
            && other.start_token == 0
            && other.end_token == 0
        {
            return self.start_line <= other.start_line && self.end_line >= other.end_line;
        }
        self.start_token <= other.start_token && self.end_token >= other.end_token
    }

    pub fn qoverlap(&self, other: &LicenseMatch) -> usize {
        if let (Some(self_positions), Some(other_positions)) =
            (&self.qspan_positions, &other.qspan_positions)
        {
            let self_set: HashSet<usize> = self_positions.iter().copied().collect();
            return other_positions
                .iter()
                .filter(|p| self_set.contains(p))
                .count();
        }

        if let (Some(self_positions), None) = (&self.qspan_positions, &other.qspan_positions) {
            let self_set: HashSet<usize> = self_positions.iter().copied().collect();
            return (other.start_token..other.end_token)
                .filter(|p| self_set.contains(p))
                .count();
        }

        if let (None, Some(other_positions)) = (&self.qspan_positions, &other.qspan_positions) {
            return other_positions
                .iter()
                .filter(|&&p| p >= self.start_token && p < self.end_token)
                .count();
        }

        if self.start_token == 0
            && self.end_token == 0
            && other.start_token == 0
            && other.end_token == 0
        {
            let start = self.start_line.max(other.start_line);
            let end = self.end_line.min(other.end_line);
            return if start <= end { end - start + 1 } else { 0 };
        }
        let start = self.start_token.max(other.start_token);
        let end = self.end_token.min(other.end_token);
        end.saturating_sub(start)
    }

    pub fn qspan_overlap(&self, other: &LicenseMatch) -> usize {
        let self_qspan: HashSet<usize> = self.qspan().into_iter().collect();
        let other_qspan: HashSet<usize> = other.qspan().into_iter().collect();
        self_qspan.intersection(&other_qspan).count()
    }

    /// Return true if all matched tokens are continuous without gaps or unknowns.
    /// Python: len() == qregion_len() == qmagnitude()
    pub fn is_continuous(&self, query: &crate::license_detection::query::Query) -> bool {
        if self.matched_token_positions.is_some() {
            return false;
        }
        let len = self.len();
        let qregion_len = self.qregion_len();
        let qmagnitude = self.qmagnitude(query);
        len == qregion_len && qregion_len == qmagnitude
    }

    pub fn ispan(&self) -> Vec<usize> {
        if let Some(positions) = &self.ispan_positions {
            positions.clone()
        } else {
            (self.rule_start_token..self.rule_start_token + self.matched_length).collect()
        }
    }

    pub fn ispan_iter(&self) -> SpanIter<'_> {
        match &self.ispan_positions {
            Some(positions) => SpanIter::Slice(positions.iter().copied()),
            None => {
                SpanIter::Range(self.rule_start_token..self.rule_start_token + self.matched_length)
            }
        }
    }

    pub fn hispan(&self) -> Vec<usize> {
        if let Some(positions) = &self.hispan_positions {
            positions.clone()
        } else {
            (self.rule_start_token..self.rule_start_token + self.hilen).collect()
        }
    }

    pub fn qspan(&self) -> Vec<usize> {
        if let Some(positions) = &self.qspan_positions {
            positions.clone()
        } else {
            (self.start_token..self.end_token).collect()
        }
    }

    pub fn qspan_iter(&self) -> SpanIter<'_> {
        match &self.qspan_positions {
            Some(positions) => SpanIter::Slice(positions.iter().copied()),
            None => SpanIter::Range(self.start_token..self.end_token),
        }
    }

    pub fn overlaps_with(&self, other: &PositionSet) -> bool {
        if let Some(positions) = &self.qspan_positions {
            // positions is sorted, so first/last give us bounds
            if positions.is_empty() {
                return false;
            }
            let my_min = positions[0];
            let my_max = positions[positions.len() - 1];

            // Bounds pre-check using PositionSet's cached bounds
            if !other.may_overlap_range(my_min, my_max + 1) {
                return false;
            }

            positions.iter().any(|&p| other.contains(p))
        } else {
            // Contiguous range
            if !other.may_overlap_range(self.start_token, self.end_token) {
                return false;
            }
            (self.start_token..self.end_token).any(|p| other.contains(p))
        }
    }

    pub fn qspan_eq(&self, other: &LicenseMatch) -> bool {
        match (&self.qspan_positions, &other.qspan_positions) {
            (Some(self_positions), Some(other_positions)) => {
                self_positions.len() == other_positions.len()
                    && self_positions.iter().collect::<HashSet<_>>()
                        == other_positions.iter().collect::<HashSet<_>>()
            }
            (Some(self_positions), None) => {
                let range_len = other.end_token.saturating_sub(other.start_token);
                self_positions.len() == range_len
                    && self_positions
                        .iter()
                        .all(|&p| p >= other.start_token && p < other.end_token)
            }
            (None, Some(other_positions)) => {
                let range_len = self.end_token.saturating_sub(self.start_token);
                other_positions.len() == range_len
                    && other_positions
                        .iter()
                        .all(|&p| p >= self.start_token && p < self.end_token)
            }
            (None, None) => {
                if self.start_token == 0
                    && self.end_token == 0
                    && other.start_token == 0
                    && other.end_token == 0
                {
                    self.start_line == other.start_line && self.end_line == other.end_line
                } else {
                    self.start_token == other.start_token && self.end_token == other.end_token
                }
            }
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
        if let Some(positions) = &self.qspan_positions {
            if positions.is_empty() {
                return (0, 0);
            }
            (
                *positions.iter().min().unwrap(),
                *positions.iter().max().unwrap() + 1,
            )
        } else {
            (self.start_token, self.end_token)
        }
    }

    pub fn qspan_magnitude(&self) -> usize {
        let (start, end) = self.qspan_bounds();
        end.saturating_sub(start)
    }

    pub fn ispan_bounds(&self) -> (usize, usize) {
        if let Some(positions) = &self.ispan_positions {
            if positions.is_empty() {
                return (0, 0);
            }
            (
                *positions.iter().min().unwrap(),
                *positions.iter().max().unwrap() + 1,
            )
        } else {
            (
                self.rule_start_token,
                self.rule_start_token + self.matched_length,
            )
        }
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
        if let (Some(self_positions), Some(other_positions)) =
            (&self.ispan_positions, &other.ispan_positions)
        {
            let self_set: HashSet<usize> = self_positions.iter().copied().collect();
            return other_positions
                .iter()
                .filter(|p| self_set.contains(p))
                .count();
        }

        if let (Some(self_positions), None) = (&self.ispan_positions, &other.ispan_positions) {
            let self_set: HashSet<usize> = self_positions.iter().copied().collect();
            return (other.rule_start_token..other.rule_start_token + other.matched_length)
                .filter(|p| self_set.contains(p))
                .count();
        }

        if let (None, Some(other_positions)) = (&self.ispan_positions, &other.ispan_positions) {
            return other_positions
                .iter()
                .filter(|&&p| {
                    p >= self.rule_start_token && p < self.rule_start_token + self.matched_length
                })
                .count();
        }

        let (self_start, self_end) = self.ispan_bounds();
        let (other_start, other_end) = other.ispan_bounds();

        let overlap_start = self_start.max(other_start);
        let overlap_end = self_end.min(other_end);

        overlap_end.saturating_sub(overlap_start)
    }

    pub fn has_unknown(&self) -> bool {
        self.license_expression.contains("unknown")
    }
}
