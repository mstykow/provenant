//! Token string to integer ID mapping.
//!
//! TokenDictionary maps token strings to unique integer IDs. This enables
//! efficient token-based matching and indexing.

use std::collections::HashMap;

use rkyv::Archive;
use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[rkyv(derive(Hash, Eq, PartialEq, PartialOrd, Ord))]
pub struct TokenId(u16);

impl TokenId {
    pub const fn new(raw: u16) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u16 {
        self.0
    }

    pub const fn as_usize(self) -> usize {
        self.0 as usize
    }

    pub const fn to_le_bytes(self) -> [u8; 2] {
        self.0.to_le_bytes()
    }
}

#[cfg(test)]
pub const fn tid(raw: u16) -> TokenId {
    TokenId::new(raw)
}

impl From<u16> for TokenId {
    fn from(value: u16) -> Self {
        Self(value)
    }
}

impl From<TokenId> for u16 {
    fn from(value: TokenId) -> Self {
        value.0
    }
}

impl PartialEq<u16> for TokenId {
    fn eq(&self, other: &u16) -> bool {
        self.0 == *other
    }
}

impl PartialOrd<u16> for TokenId {
    fn partial_cmp(&self, other: &u16) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(other)
    }
}

impl PartialEq<TokenId> for u16 {
    fn eq(&self, other: &TokenId) -> bool {
        *self == other.0
    }
}

impl PartialOrd<TokenId> for u16 {
    fn partial_cmp(&self, other: &TokenId) -> Option<std::cmp::Ordering> {
        self.partial_cmp(&other.0)
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum TokenKind {
    Legalese,
    Regular,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct KnownToken {
    pub id: TokenId,
    pub kind: TokenKind,
    pub is_digit_only: bool,
    pub is_short_or_digit: bool,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum QueryToken {
    Known(KnownToken),
    Unknown,
    Stopword,
}

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct TokenMetadata {
    pub kind: TokenKind,
    pub is_digit_only: bool,
    pub is_short_or_digit: bool,
}

/// Token dictionary mapping token strings to unique integer IDs.
///
/// Token IDs are assigned as follows:
/// - IDs 0 to len_legalese-1: Reserved for legalese tokens (high-value words)
/// - IDs len_legalese and above: Assigned to other tokens as encountered
///
/// The `len_legalese` delimiter allows the matching engine to distinguish
/// between high-value (legalese) tokens and regular tokens.
///
/// Based on the Python ScanCode Toolkit implementation at:
/// reference/scancode-toolkit/src/licensedcode/index.py
#[derive(Debug, Clone, Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct TokenDictionary {
    /// Mapping from token string to token ID
    tokens_to_ids: HashMap<String, TokenId>,

    token_metadata: Vec<Option<TokenMetadata>>,

    /// Number of legalese tokens (lower IDs = higher value)
    len_legalese: usize,

    /// Next token ID to assign (for non-legalese tokens)
    next_id: TokenId,
}

impl TokenDictionary {
    const DEFAULT_METADATA: TokenMetadata = TokenMetadata {
        kind: TokenKind::Regular,
        is_digit_only: false,
        is_short_or_digit: false,
    };

    /// Create a new token dictionary initialized with legalese tokens.
    ///
    /// This follows the Python ScanCode TorchToolkit pattern where the dictionary
    /// starts with pre-defined legalese words that get low IDs (high value).
    ///
    /// # Arguments
    /// * `legalese_entries` - Slice of (word, token_id) pairs for legalese words
    ///
    /// # Returns
    /// A new TokenDictionary instance with legalese tokens pre-populated
    pub fn new_with_legalese(legalese_entries: &[(&str, u16)]) -> Self {
        let mut tokens_to_ids = HashMap::new();
        let max_existing_id = legalese_entries
            .iter()
            .map(|(_, token_id)| *token_id as usize)
            .max()
            .unwrap_or(0);
        let mut token_metadata = vec![None; max_existing_id.saturating_add(1)];

        for (word, token_id) in legalese_entries {
            let id = TokenId::from(*token_id);
            tokens_to_ids.insert(word.to_string(), id);
            token_metadata[id.as_usize()] = Some(TokenMetadata {
                kind: TokenKind::Legalese,
                is_digit_only: word.chars().all(|c| c.is_ascii_digit()),
                is_short_or_digit: word.len() == 1 || word.chars().all(|c| c.is_ascii_digit()),
            });
        }

        let len_legalese = legalese_entries.len();
        let next_id = TokenId::new((max_existing_id + 1).max(len_legalese) as u16);

        Self {
            tokens_to_ids,
            token_metadata,
            len_legalese,
            next_id,
        }
    }

    /// Create a new empty token dictionary (for testing).
    ///
    /// # Arguments
    /// * `legalese_count` - Number of reserved legalese token IDs
    ///
    /// # Returns
    /// A new TokenDictionary instance
    pub fn new(legalese_count: usize) -> Self {
        Self {
            tokens_to_ids: HashMap::new(),
            token_metadata: Vec::new(),
            len_legalese: legalese_count,
            next_id: TokenId::new(legalese_count as u16),
        }
    }

    fn metadata_for(&self, id: TokenId) -> TokenMetadata {
        self.token_metadata
            .get(id.as_usize())
            .and_then(|meta| *meta)
            .unwrap_or(Self::DEFAULT_METADATA)
    }

    fn build_known_token(&self, id: TokenId) -> KnownToken {
        let metadata = self.metadata_for(id);
        KnownToken {
            id,
            kind: metadata.kind,
            is_digit_only: metadata.is_digit_only,
            is_short_or_digit: metadata.is_short_or_digit,
        }
    }

    fn insert_metadata(&mut self, id: TokenId, kind: TokenKind, token: &str) {
        let raw = id.as_usize();
        if self.token_metadata.len() <= raw {
            self.token_metadata.resize(raw + 1, None);
        }
        self.token_metadata[raw] = Some(TokenMetadata {
            kind,
            is_digit_only: token.chars().all(|c| c.is_ascii_digit()),
            is_short_or_digit: token.len() == 1 || token.chars().all(|c| c.is_ascii_digit()),
        });
    }

    pub fn intern(&mut self, token: &str) -> KnownToken {
        if let Some(&id) = self.tokens_to_ids.get(token) {
            return self.build_known_token(id);
        }

        let id = self.next_id;
        self.next_id = TokenId::new(self.next_id.raw() + 1);
        self.tokens_to_ids.insert(token.to_string(), id);
        self.insert_metadata(id, TokenKind::Regular, token);
        self.build_known_token(id)
    }

    pub fn lookup(&self, token: &str) -> Option<KnownToken> {
        self.tokens_to_ids
            .get(token)
            .copied()
            .map(|id| self.build_known_token(id))
    }

    pub fn classify_query_token(&self, token: &str) -> QueryToken {
        self.lookup(token)
            .map_or(QueryToken::Unknown, QueryToken::Known)
    }

    pub fn token_kind(&self, token_id: TokenId) -> TokenKind {
        self.metadata_for(token_id).kind
    }

    pub fn is_digit_only_token(&self, token_id: TokenId) -> bool {
        self.metadata_for(token_id).is_digit_only
    }

    #[cfg(test)]
    pub fn get_or_assign(&mut self, token: &str) -> TokenId {
        self.intern(token).id
    }

    /// Get the token ID for a token string if it exists.
    ///
    /// # Arguments
    /// * `token` - The token string
    ///
    /// # Returns
    /// Some(token_id) if the token exists, None otherwise
    pub fn get_token_id(&self, token: &str) -> Option<TokenId> {
        self.lookup(token).map(|token| token.id)
    }

    /// Get the token ID (alias for backward compatibility).
    #[inline]
    pub fn get(&self, token: &str) -> Option<TokenId> {
        self.get_token_id(token)
    }

    /// Get the number of legalese tokens.
    pub const fn legalese_count(&self) -> usize {
        self.len_legalese
    }

    /// Get an iterator over all token string and ID pairs.
    #[cfg(test)]
    pub fn tokens_to_ids(&self) -> impl Iterator<Item = (&String, &TokenId)> {
        self.tokens_to_ids.iter()
    }

    /// Get the number of tokens in the dictionary.
    // This method will be used by the embedded index roundtrip tests in upcoming phases.
    #[allow(dead_code)]
    pub fn tokens_to_ids_len(&self) -> usize {
        self.tokens_to_ids.len()
    }
}

impl Default for TokenDictionary {
    fn default() -> Self {
        Self::new(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_dictionary_new() {
        let dict = TokenDictionary::new(10);
        assert_eq!(dict.legalese_count(), 10);
        assert_eq!(dict.tokens_to_ids.len(), 0);
        assert!(dict.tokens_to_ids.is_empty());
    }

    #[test]
    fn test_new_with_legalese() {
        let legalese = [
            ("license".to_string(), 0u16),
            ("copyright".to_string(), 1u16),
            ("permission".to_string(), 2u16),
        ];

        let mut dict = TokenDictionary::new_with_legalese(
            &legalese
                .iter()
                .map(|(s, i)| (s.as_str(), *i))
                .collect::<Vec<_>>(),
        );

        assert_eq!(dict.legalese_count(), 3);
        assert_eq!(dict.tokens_to_ids.len(), 3);
        assert!(!dict.tokens_to_ids.is_empty());

        // Check that legalese tokens are registered
        assert_eq!(dict.get_token_id("license"), Some(tid(0)));
        assert_eq!(dict.get_token_id("copyright"), Some(tid(1)));
        assert_eq!(dict.get_token_id("permission"), Some(tid(2)));

        // Check that new tokens get IDs starting after legalese
        let test_id = dict.get_or_assign("test");
        assert_eq!(test_id, 3);
    }

    #[test]
    fn test_new_with_legalese_sorted() {
        let legalese = [
            ("copyright".to_string(), 5u16),
            ("license".to_string(), 0u16),
            ("permission".to_string(), 10u16),
        ];

        let mut dict = TokenDictionary::new_with_legalese(
            &legalese
                .iter()
                .map(|(s, i)| (s.as_str(), *i))
                .collect::<Vec<_>>(),
        );

        assert_eq!(dict.legalese_count(), 3);
        assert_eq!(dict.tokens_to_ids.len(), 3);

        // Check legalese IDs are correct regardless of input order
        assert_eq!(dict.get_token_id("copyright"), Some(tid(5)));
        assert_eq!(dict.get_token_id("license"), Some(tid(0)));
        assert_eq!(dict.get_token_id("permission"), Some(tid(10)));

        // Next ID should advance past the highest explicit legalese token ID.
        let test_id = dict.get_or_assign("test");
        assert_eq!(test_id, tid(11));
    }

    #[test]
    fn test_get_or_assign_new_token() {
        let mut dict = TokenDictionary::new(5);

        let id1 = dict.get_or_assign("hello");
        let id2 = dict.get_or_assign("world");

        // Should assign IDs starting at legalese_count (5)
        assert_eq!(id1, 5);
        assert_eq!(id2, 6);
        assert_eq!(dict.tokens_to_ids.len(), 2);
    }

    #[test]
    fn test_get_or_assign_existing_token() {
        let mut dict = TokenDictionary::new(5);

        let id1 = dict.get_or_assign("hello");
        let id2 = dict.get_or_assign("hello");

        // Should return the same ID for the same token
        assert_eq!(id1, id2);
        assert_eq!(dict.tokens_to_ids.len(), 1);
    }

    #[test]
    fn test_get_or_assign_with_preexisting_legalese() {
        let legalese = [("license".to_string(), 0u16)];
        let mut dict = TokenDictionary::new_with_legalese(
            &legalese
                .iter()
                .map(|(s, i)| (s.as_str(), *i))
                .collect::<Vec<_>>(),
        );

        // Legalese tokens should already exist
        let id = dict.get_or_assign("license");
        assert_eq!(id, 0);
        assert_eq!(dict.tokens_to_ids.len(), 1);

        // New tokens should get IDs after legalese
        let new_id = dict.get_or_assign("new");
        assert_eq!(new_id, 1);
        assert_eq!(dict.tokens_to_ids.len(), 2);
    }

    #[test]
    fn test_get_existing_token() {
        let mut dict = TokenDictionary::new(5);

        dict.get_or_assign("hello");
        assert_eq!(dict.get_token_id("hello"), Some(tid(5)));
    }

    #[test]
    fn test_get_nonexistent_token() {
        let dict = TokenDictionary::new(5);
        assert_eq!(dict.get_token_id("hello"), None);
    }

    #[test]
    fn test_legalese_range() {
        let dict = TokenDictionary::new(10);

        // IDs 0-9 are legalese
        assert!(0 < dict.legalese_count() as u16);
        assert!(5 < dict.legalese_count() as u16);
        assert!(9 < dict.legalese_count() as u16);

        // ID 10+ are not legalese
        assert!(10 >= dict.legalese_count() as u16);
        assert!(100 >= dict.legalese_count() as u16);
    }

    #[test]
    fn test_legalese_range_with_actual_legalese() {
        let legalese = [
            ("license".to_string(), 0u16),
            ("copyright".to_string(), 1u16),
        ];

        let mut dict = TokenDictionary::new_with_legalese(
            &legalese
                .iter()
                .map(|(s, i)| (s.as_str(), *i))
                .collect::<Vec<_>>(),
        );

        // Legalese tokens should have IDs in the legalese range
        assert!(dict.get_token_id("license").unwrap() < dict.legalese_count() as u16);
        assert!(dict.get_token_id("copyright").unwrap() < dict.legalese_count() as u16);

        // Regular tokens should not be legalese
        let regular_id = dict.get_or_assign("regular");
        assert!(regular_id >= dict.legalese_count() as u16);
    }

    #[test]
    fn test_token_dictionary_default() {
        let dict = TokenDictionary::default();
        assert_eq!(dict.legalese_count(), 0);
        assert!(dict.tokens_to_ids.is_empty());
    }

    #[test]
    fn test_get_alias() {
        let mut dict = TokenDictionary::new(5);
        dict.get_or_assign("hello");

        // get() should be an alias for get_token_id()
        assert_eq!(dict.get("hello"), dict.get_token_id("hello"));
    }

    #[test]
    fn test_with_actual_legalese_module() {
        use crate::license_detection::rules::legalese;

        let legalese_words = legalese::get_legalese_words();
        assert!(!legalese_words.is_empty(), "Should have legalese words");

        let mut dict = TokenDictionary::new_with_legalese(&legalese_words);

        // Verify dictionary has the right structure
        assert_eq!(dict.legalese_count(), legalese_words.len());
        assert_eq!(dict.tokens_to_ids.len(), legalese_words.len());

        // Verify some legalese words are correctly registered
        let license_id = dict.get_token_id("license");
        assert!(license_id.is_some(), "License should be in dictionary");
        assert!(
            license_id.unwrap() < dict.legalese_count() as u16,
            "License should be a legalese token"
        );

        // Note: Standalone "copyright" is NOT in the Python reference dictionary
        // Only compound words like "copyrighted", "copyrights" are present
        let copyrighted_id = dict.get_token_id("copyrighted");
        assert!(
            copyrighted_id.is_some(),
            "Copyrighted should be in dictionary"
        );
        assert!(
            copyrighted_id.unwrap() < dict.legalese_count() as u16,
            "Copyrighted should be a legalese token"
        );

        // New tokens should get IDs after legalese
        let hello_id = dict.get_or_assign("hello");
        assert!(hello_id >= dict.legalese_count() as u16);
        assert!(hello_id >= dict.legalese_count() as u16);
    }
}
