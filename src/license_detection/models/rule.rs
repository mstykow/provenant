//! Rule metadata loaded from .LICENSE and .RULE files.

use std::collections::HashMap;
use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::license_detection::index::dictionary::TokenId;

const SCANCODE_LICENSE_URL_BASE: &str =
    "https://github.com/aboutcode-org/scancode-toolkit/tree/develop/src/licensedcode/data/licenses";
const SCANCODE_RULE_URL_BASE: &str =
    "https://github.com/aboutcode-org/scancode-toolkit/tree/develop/src/licensedcode/data/rules";

mod range_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::ops::Range;

    pub fn serialize<S>(ranges: &[Range<usize>], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let tuples: Vec<(usize, usize)> = ranges.iter().map(|r| (r.start, r.end)).collect();
        tuples.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Range<usize>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let tuples: Vec<(usize, usize)> = Vec::deserialize(deserializer)?;
        Ok(tuples
            .into_iter()
            .map(|(start, end)| Range { start, end })
            .collect())
    }
}

mod stopwords_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::HashMap;

    pub fn serialize<S>(
        map: &HashMap<Option<usize>, usize>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut entries: Vec<(Option<usize>, usize)> = map.iter().map(|(k, v)| (*k, *v)).collect();
        entries.sort_by_key(|(k, _)| *k);
        entries.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<HashMap<Option<usize>, usize>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let entries: Vec<(Option<usize>, usize)> = Vec::deserialize(deserializer)?;
        Ok(entries.into_iter().collect())
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
pub enum RuleKind {
    #[default]
    None,
    Text,
    Notice,
    Reference,
    Tag,
    Intro,
    Clue,
}

impl RuleKind {
    pub fn from_rule_flags(
        is_license_text: bool,
        is_license_notice: bool,
        is_license_reference: bool,
        is_license_tag: bool,
        is_license_intro: bool,
        is_license_clue: bool,
    ) -> Result<Self, &'static str> {
        let mut active = None;

        for (enabled, kind) in [
            (is_license_text, Self::Text),
            (is_license_notice, Self::Notice),
            (is_license_reference, Self::Reference),
            (is_license_tag, Self::Tag),
            (is_license_intro, Self::Intro),
            (is_license_clue, Self::Clue),
        ] {
            if !enabled {
                continue;
            }

            if active.replace(kind).is_some() {
                return Err("rule has multiple rule kinds set");
            }
        }

        Ok(active.unwrap_or(Self::None))
    }

    pub const fn is_license_text(self) -> bool {
        matches!(self, Self::Text)
    }

    pub const fn is_license_notice(self) -> bool {
        matches!(self, Self::Notice)
    }

    pub const fn is_license_reference(self) -> bool {
        matches!(self, Self::Reference)
    }

    pub const fn is_license_tag(self) -> bool {
        matches!(self, Self::Tag)
    }

    pub const fn is_license_intro(self) -> bool {
        matches!(self, Self::Intro)
    }

    pub const fn is_license_clue(self) -> bool {
        matches!(self, Self::Clue)
    }
}

/// Rule metadata loaded from .LICENSE and .RULE files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    /// Unique identifier for this rule (e.g., "mit.LICENSE", "gpl-2.0_12.RULE")
    /// Used for sorting to match Python's attr.s field order.
    /// This is the primary sort key after rid (which is None at sort time in Python).
    pub identifier: String,

    /// License expression string using SPDX syntax and ScanCode license keys
    pub license_expression: String,

    /// Pattern text to match
    pub text: String,

    /// Token IDs for the text (assigned during indexing)
    #[serde(
        serialize_with = "serialize_token_ids",
        deserialize_with = "deserialize_token_ids"
    )]
    pub tokens: Vec<TokenId>,

    /// Classification of this rule.
    pub rule_kind: RuleKind,

    /// True if exact matches to this rule are false positives
    pub is_false_positive: bool,

    /// True if this rule text is a required phrase.
    /// A required phrase is an essential section of the rule text which must be
    /// present in the case of partial matches.
    pub is_required_phrase: bool,

    /// True if this rule was created from a license file (not a .RULE file)
    pub is_from_license: bool,

    /// Relevance score 0-100 (100 is most relevant)
    pub relevance: u8,

    /// Minimum match coverage percentage (0-100) if specified
    pub minimum_coverage: Option<u8>,

    /// True if minimum_coverage was explicitly stored in source frontmatter
    pub has_stored_minimum_coverage: bool,

    /// Tokens must appear in order if true
    pub is_continuous: bool,

    /// Token position spans for required phrases parsed from {{...}} markers.
    /// Each span represents positions in the rule text that MUST be matched.
    #[serde(with = "range_serde", default)]
    pub required_phrase_spans: Vec<Range<usize>>,

    /// Mapping from token position to count of stopwords at that position.
    /// Used for required phrase validation.
    #[serde(with = "stopwords_serde", default)]
    pub stopwords_by_pos: HashMap<Option<usize>, usize>,

    /// Filenames where this rule should be considered
    pub referenced_filenames: Option<Vec<String>>,

    /// URLs that should be ignored when found in this rule text
    pub ignorable_urls: Option<Vec<String>>,

    /// Emails that should be ignored when found in this rule text
    pub ignorable_emails: Option<Vec<String>>,

    /// Copyrights that should be ignored when found in this rule text
    pub ignorable_copyrights: Option<Vec<String>>,

    /// Holder names that should be ignored when found in this rule text
    pub ignorable_holders: Option<Vec<String>>,

    /// Author names that should be ignored when found in this rule text
    pub ignorable_authors: Option<Vec<String>>,

    /// Programming language for the rule if specified
    pub language: Option<String>,

    /// Free text notes
    pub notes: Option<String>,

    /// Count of unique token IDs in the rule (computed during indexing)
    pub length_unique: usize,

    /// Count of unique legalese token IDs (tokens with ID < len_legalese)
    pub high_length_unique: usize,

    /// Total count of legalese token occurrences (with duplicates)
    pub high_length: usize,

    /// Minimum matched length threshold (occurrences-based)
    pub min_matched_length: usize,

    /// Minimum high-value token matched length threshold (occurrences-based)
    pub min_high_matched_length: usize,

    /// Minimum matched length threshold (unique tokens)
    pub min_matched_length_unique: usize,

    /// Minimum high-value token matched length threshold (unique tokens)
    pub min_high_matched_length_unique: usize,

    /// True if rule length < SMALL_RULE (15 tokens)
    pub is_small: bool,

    /// True if rule length < TINY_RULE (6 tokens)
    pub is_tiny: bool,

    /// True if the rule's first token is "license", "licence", or "licensed"
    pub starts_with_license: bool,

    /// True if the rule's last token is "license", "licence", or "licensed"
    pub ends_with_license: bool,

    /// Whether this rule is deprecated
    pub is_deprecated: bool,

    /// SPDX license identifier if available
    pub spdx_license_key: Option<String>,

    /// Alternative SPDX license identifiers (aliases)
    pub other_spdx_license_keys: Vec<String>,
}

fn serialize_token_ids<S>(token_ids: &[TokenId], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let raw_ids: Vec<u16> = token_ids.iter().map(|id| id.raw()).collect();
    <Vec<u16> as serde::Serialize>::serialize(&raw_ids, serializer)
}

fn deserialize_token_ids<'de, D>(deserializer: D) -> Result<Vec<TokenId>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw_ids: Vec<u16> = Vec::deserialize(deserializer)?;
    Ok(raw_ids.into_iter().map(TokenId::new).collect())
}

impl PartialOrd for Rule {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Rule {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.identifier.cmp(&other.identifier)
    }
}

impl Rule {
    pub fn rule_url(&self) -> Option<String> {
        if self.is_from_license {
            return (!self.license_expression.is_empty()).then(|| {
                format!(
                    "{SCANCODE_LICENSE_URL_BASE}/{}.LICENSE",
                    self.license_expression
                )
            });
        }

        (!self.identifier.is_empty())
            .then(|| format!("{SCANCODE_RULE_URL_BASE}/{}", self.identifier))
    }

    pub const fn kind(&self) -> RuleKind {
        self.rule_kind
    }

    pub const fn is_license_text(&self) -> bool {
        self.rule_kind.is_license_text()
    }

    /// Returns true if this rule is a license notice pattern.
    ///
    /// Note: This method is kept for API completeness and potential future use.
    /// License matches cannot have `is_license_notice` - only rules can.
    #[allow(dead_code)]
    pub const fn is_license_notice(&self) -> bool {
        self.rule_kind.is_license_notice()
    }

    pub const fn is_license_reference(&self) -> bool {
        self.rule_kind.is_license_reference()
    }

    pub const fn is_license_tag(&self) -> bool {
        self.rule_kind.is_license_tag()
    }

    /// Returns true if this rule is a license introduction pattern.
    ///
    /// Note: This method is kept for API completeness and potential future use.
    #[allow(dead_code)]
    pub const fn is_license_intro(&self) -> bool {
        self.rule_kind.is_license_intro()
    }

    pub const fn is_license_clue(&self) -> bool {
        self.rule_kind.is_license_clue()
    }
}
