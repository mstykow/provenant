// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! License index construction and querying.

pub mod builder;
pub mod dictionary;

// build_index is used by library tests (see spdx_lid/test.rs, index/builder/tests.rs)
// even though the binary doesn't use it directly.
#[allow(unused_imports)]
pub use builder::{
    build_index, build_index_from_loaded, build_index_from_loaded_with_automatons,
    loaded_license_to_license, loaded_rule_to_rule,
};

use crate::license_detection::automaton::{AsBytes, Automaton};
use crate::license_detection::index::dictionary::{TokenDictionary, TokenId};
use crate::license_detection::{TokenMultiset, TokenSet};
use rkyv::Archive;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default, PartialEq, Eq, Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct IndexedRuleMetadata {
    pub license_expression_spdx: Option<String>,
    pub skip_for_required_phrase_generation: bool,
    pub replaced_by: Vec<String>,
}

/// License index containing all data structures for efficient license detection.
///
/// The LicenseIndex holds multiple index structures that enable different matching
/// strategies: hash-based exact matching, Aho-Corasick automaton matching, set-based
/// candidate selection, and sequence matching.
///
/// Based on the Python ScanCode Toolkit implementation at:
/// reference/scancode-toolkit/src/licensedcode/index.py
///
/// # Index Structures
///
/// The index maintains several data structures for different matching strategies:
///
/// - **Hash matching**: `rid_by_hash` for exact hash-based matches
/// - **Automaton matching**: `rules_automaton` and `unknown_automaton` for pattern matching
/// - **Candidate selection**: `sets_by_rid` and `msets_by_rid` for set-based ranking
/// - **Sequence matching**: `high_postings_by_rid` for high-value token position tracking
/// - **Rule classification**: `false_positive_rids`, `approx_matchable_rids`
#[derive(Debug, Clone, Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct LicenseIndex {
    /// Token dictionary mapping token strings to integer IDs.
    ///
    /// IDs 0 to len_legalese-1 are reserved for legalese tokens (high-value words).
    /// IDs len_legalese and above are assigned to other tokens as encountered.
    pub dictionary: TokenDictionary,

    /// Number of legalese tokens.
    ///
    /// Tokens with ID < len_legalese are considered high-value legalese words.
    /// Tokens with ID >= len_legalese are considered low-value tokens.
    ///
    /// Corresponds to Python: `self.len_legalese = 0` (line 185)
    pub len_legalese: usize,

    /// Mapping from rule hash to rule ID for hash-based exact matching.
    ///
    /// This enables fast exact matches using a hash of the rule\'s token IDs.
    /// Each hash maps to exactly one rule ID.
    ///
    /// Note: The hash is a 20-byte SHA1 digest, stored as a key in HashMap.
    /// In practice, we use a HashMap<[u8; 20], usize>.
    ///
    /// Corresponds to Python: `self.rid_by_hash = {}` (line 216)
    pub rid_by_hash: HashMap<[u8; 20], usize>,

    /// Rules indexed by rule ID.
    ///
    /// Maps rule IDs to Rule objects for quick lookup.
    ///
    /// Corresponds to Python: `self.rules_by_rid = []` (line 201)
    pub rules_by_rid: Vec<crate::license_detection::models::Rule>,

    /// Token ID sequences indexed by rule ID.
    ///
    /// Maps rule IDs to their token ID sequences.
    ///
    /// Corresponds to Python: `self.tids_by_rid = []` (line 204)
    pub tids_by_rid: Vec<Vec<TokenId>>,

    /// Aho-Corasick automaton built from all rule token sequences.
    ///
    /// Supports efficient multi-pattern matching of token ID sequences.
    /// Used for exact matching of complete rules or rule fragments in query text.
    ///
    /// Corresponds to Python: `self.rules_automaton = match_aho.get_automaton()` (line 219)
    #[rkyv(with = AsBytes)]
    pub rules_automaton: Automaton,

    /// Aho-Corasick automaton for unknown license detection.
    ///
    /// Separate automaton used to detect license-like text that doesn\'t match
    /// any known rule. Populated with ngrams from all approx-matchable rules.
    ///
    /// Corresponds to Python: `self.unknown_automaton = match_unknown.get_automaton()` (line 222)
    #[rkyv(with = AsBytes)]
    pub unknown_automaton: Automaton,

    /// Token ID sets per rule for candidate selection.
    ///
    /// Maps rule IDs to sets of unique token IDs present in that rule.
    /// Used for efficient candidate selection based on token overlap.
    ///
    /// Corresponds to Python: `self.sets_by_rid = []` (line 212)
    pub sets_by_rid: HashMap<usize, TokenSet>,

    pub rule_metadata_by_identifier: HashMap<String, IndexedRuleMetadata>,

    /// Token ID multisets per rule for candidate ranking.
    ///
    /// Maps rule IDs to multisets (bags) of token IDs with their frequencies.
    /// Used for ranking candidates by token frequency overlap.
    ///
    /// Corresponds to Python: `self.msets_by_rid = []` (line 213)
    pub msets_by_rid: HashMap<usize, TokenMultiset>,

    /// High-value token sets per rule for early candidate rejection.
    ///
    /// Maps rule IDs to sets containing only high-value (legalese) token IDs.
    /// This is a subset of `sets_by_rid` for faster intersection computation
    /// and early rejection of candidates that won't pass the high-token threshold.
    ///
    /// Precomputed during index building to avoid redundant filtering at runtime.
    pub high_sets_by_rid: HashMap<usize, TokenSet>,

    /// Inverted index of high-value token positions per rule.
    ///
    /// Maps rule IDs to a mapping from high-value token IDs to their positions
    /// within the rule. Only contains positions for tokens with IDs < len_legalese.
    ///
    /// This structure speeds up sequence matching by allowing quick lookup of
    /// where high-value tokens appear in each rule.
    ///
    /// Corresponds to Python: `self.high_postings_by_rid = []` (line 209)
    /// In Python: `postings = {tid: array('h', [positions, ...])}`
    pub high_postings_by_rid: HashMap<usize, HashMap<TokenId, Vec<usize>>>,

    /// Set of rule IDs for false positive rules.
    ///
    /// False positive rules are used for exact matching and post-matching
    /// filtering to subtract spurious matches.
    ///
    /// Corresponds to Python: `self.false_positive_rids = set()` (line 230)
    pub false_positive_rids: HashSet<usize>,

    /// Set of rule IDs that can be matched approximately.
    ///
    /// Only rules marked as approx-matchable participate in sequence matching.
    /// Other rules can only be matched exactly using the automaton.
    ///
    /// Note: This field is kept for Python parity documentation and test usage.
    /// The inverted index (`rids_by_high_tid`) now handles candidate filtering
    /// more efficiently, making direct iteration over this set unnecessary.
    ///
    /// Corresponds to Python: `self.approx_matchable_rids = set()` (line 234)
    #[allow(dead_code)]
    pub approx_matchable_rids: HashSet<usize>,

    /// Mapping from ScanCode license key to License object.
    ///
    /// Provides access to license metadata for building SPDX mappings
    /// and validating license expressions.
    ///
    /// Corresponds to Python: `get_licenses_db()` in models.py
    pub licenses_by_key: HashMap<String, crate::license_detection::models::License>,

    /// Maps AhoCorasick pattern_id to rule ids (rids).
    ///
    /// This is needed because the AhoCorasick pattern_id is just the index
    /// in the patterns iterator used to build the automaton, not the actual
    /// rule id. In Python, the automaton stores (rid, start, end) tuples as
    /// values, so the rid is retrieved from the stored value. In Rust, we
    /// maintain this mapping instead.
    ///
    /// Multiple rules can share the same token pattern (e.g., rules that differ
    /// only in license_expression). Each pattern_id maps to a list of all rule IDs
    /// that share that pattern.
    ///
    /// Corresponds to Python: automaton values contain (rid, istart, iend)
    pub pattern_id_to_rid: Vec<Vec<usize>>,

    /// Mapping from SPDX license key to rule ID.
    ///
    /// Enables direct lookup of rules by their SPDX license key,
    /// including aliases like "GPL-2.0+" -> gpl-2.0-plus.
    ///
    /// Keys are stored lowercase for case-insensitive lookup.
    ///
    /// Corresponds to Python: `self.licenses_by_spdx_key` in cache.py
    pub rid_by_spdx_key: HashMap<String, usize>,

    /// Rule ID for the unknown-spdx license.
    ///
    /// Used as a fallback when an SPDX identifier is not recognized.
    ///
    /// Corresponds to Python: `get_unknown_spdx_symbol()` in cache.py
    pub unknown_spdx_rid: Option<usize>,

    /// Inverted index mapping high-value token IDs to rule IDs.
    ///
    /// This enables fast candidate selection by only examining rules
    /// that share at least one high-value (legalese) token with the query.
    /// Without this index, candidate selection would iterate over all 37,000+
    /// rules for every file, making license detection extremely slow.
    ///
    /// Only contains entries for tokens with ID < len_legalese (high-value tokens).
    /// Rules not in approx_matchable_rids are excluded from this index.
    pub rids_by_high_tid: HashMap<TokenId, HashSet<usize>>,

    /// SPDX license list version used to build this index.
    pub spdx_license_list_version: Option<String>,
}

impl LicenseIndex {}

impl LicenseIndex {
    /// Create a new empty license index.
    ///
    /// This constructor initializes all index structures with empty collections.
    /// The index can be populated with rules using the indexing methods (to be
    /// implemented in future phases).
    ///
    /// # Returns
    /// A new LicenseIndex instance with empty index structures
    pub fn new(dictionary: TokenDictionary) -> Self {
        use crate::license_detection::automaton::AutomatonBuilder;

        let len_legalese = dictionary.legalese_count();
        Self {
            dictionary,
            len_legalese,
            rid_by_hash: HashMap::new(),
            rules_by_rid: Vec::new(),
            tids_by_rid: Vec::new(),
            rules_automaton: AutomatonBuilder::new().build(),
            unknown_automaton: AutomatonBuilder::new().build(),
            sets_by_rid: HashMap::new(),
            rule_metadata_by_identifier: HashMap::new(),
            msets_by_rid: HashMap::new(),
            high_sets_by_rid: HashMap::new(),
            high_postings_by_rid: HashMap::new(),
            false_positive_rids: HashSet::new(),
            approx_matchable_rids: HashSet::new(),
            licenses_by_key: HashMap::new(),
            pattern_id_to_rid: Vec::new(),
            rid_by_spdx_key: HashMap::new(),
            unknown_spdx_rid: None,
            rids_by_high_tid: HashMap::new(),
            spdx_license_list_version: None,
        }
    }

    /// Create a new empty license index with the specified legalese count.
    ///
    /// Convenience method that creates a new TokenDictionary and LicenseIndex
    /// in one call.
    ///
    /// # Arguments
    /// * `legalese_count` - Number of reserved legalese token IDs
    ///
    /// # Returns
    /// A new LicenseIndex instance with a new TokenDictionary
    pub fn with_legalese_count(legalese_count: usize) -> Self {
        Self::new(TokenDictionary::new(legalese_count))
    }
}

impl Default for LicenseIndex {
    fn default() -> Self {
        Self::with_legalese_count(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_license(key: &str, name: &str, spdx: &str, category: &str, text: &str) -> License {
        License {
            key: key.to_string(),
            short_name: Some(name.to_string()),
            name: name.to_string(),
            language: Some("en".to_string()),
            spdx_license_key: Some(spdx.to_string()),
            other_spdx_license_keys: vec![],
            category: Some(category.to_string()),
            owner: None,
            homepage_url: None,
            text: text.to_string(),
            reference_urls: vec![],
            osi_license_key: Some(spdx.to_string()),
            text_urls: vec![],
            osi_url: None,
            faq_url: None,
            other_urls: vec![],
            notes: None,
            is_deprecated: false,
            is_exception: false,
            is_unknown: false,
            is_generic: false,
            replaced_by: vec![],
            minimum_coverage: None,
            standard_notice: None,
            ignorable_copyrights: None,
            ignorable_holders: None,
            ignorable_authors: None,
            ignorable_urls: None,
            ignorable_emails: None,
        }
    }
    use crate::license_detection::models::License;

    #[test]
    fn test_license_index_new() {
        let dict = TokenDictionary::new(10);
        let index = LicenseIndex::new(dict);

        assert_eq!(index.dictionary.legalese_count(), 10);
        assert!(index.rid_by_hash.is_empty());
        assert!(index.sets_by_rid.is_empty());
        assert!(index.msets_by_rid.is_empty());
        assert!(index.high_postings_by_rid.is_empty());
        assert!(index.false_positive_rids.is_empty());
        assert!(index.approx_matchable_rids.is_empty());
        assert!(index.licenses_by_key.is_empty());
    }

    #[test]
    fn test_license_index_with_legalese_count() {
        let index = LicenseIndex::with_legalese_count(15);

        assert_eq!(index.dictionary.legalese_count(), 15);
        assert!(index.rid_by_hash.is_empty());
    }

    #[test]
    fn test_license_index_default() {
        let index = LicenseIndex::default();

        assert_eq!(index.dictionary.legalese_count(), 0);
        assert!(index.rid_by_hash.is_empty());
    }

    #[test]
    fn test_automaton_default() {
        use crate::license_detection::automaton::AutomatonBuilder;

        let automaton = AutomatonBuilder::new().build();
        let _ = format!("{:?}", automaton);
    }

    #[test]
    fn test_license_index_clone() {
        let index = LicenseIndex::with_legalese_count(5);
        let cloned = index.clone();

        assert_eq!(cloned.dictionary.legalese_count(), 5);
        assert!(cloned.rid_by_hash.is_empty());
    }

    #[test]
    fn test_license_index_add_license() {
        let mut index = LicenseIndex::default();

        let license = simple_license(
            "test-license",
            "Test License",
            "TEST",
            "Permissive",
            "Test license text",
        );

        index.licenses_by_key.insert(license.key.clone(), license);

        assert_eq!(index.licenses_by_key.len(), 1);
        assert!(index.licenses_by_key.contains_key("test-license"));
    }

    #[test]
    fn test_license_index_add_licenses() {
        let mut index = LicenseIndex::default();

        let licenses = vec![
            simple_license(
                "license-1",
                "License 1",
                "LIC1",
                "Permissive",
                "License 1 text",
            ),
            simple_license(
                "license-2",
                "License 2",
                "LIC2",
                "Copyleft",
                "License 2 text",
            ),
        ];

        for license in licenses {
            index.licenses_by_key.insert(license.key.clone(), license);
        }

        assert_eq!(index.licenses_by_key.len(), 2);
        assert!(index.licenses_by_key.contains_key("license-1"));
        assert!(index.licenses_by_key.contains_key("license-2"));
    }

    #[test]
    fn test_license_index_get_license() {
        let mut index = LicenseIndex::default();

        let license = simple_license(
            "mit",
            "MIT License",
            "MIT",
            "Permissive",
            "MIT License text",
        );

        index.licenses_by_key.insert(license.key.clone(), license);

        let retrieved = index.licenses_by_key.get("mit");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "MIT License");

        assert!(!index.licenses_by_key.contains_key("unknown"));
    }

    #[test]
    fn test_license_index_license_count() {
        let mut index = LicenseIndex::default();

        assert_eq!(index.licenses_by_key.len(), 0);

        let license = simple_license("test", "Test", "TEST", "Permissive", "Text");

        index.licenses_by_key.insert(license.key.clone(), license);

        assert_eq!(index.licenses_by_key.len(), 1);
    }
}
