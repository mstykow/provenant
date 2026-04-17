use smallvec::SmallVec;
use std::cmp::Ordering;
use std::ops::Deref;

use rkyv::Archive;

use crate::license_detection::index::dictionary::{TokenDictionary, TokenId, TokenKind};

/// A set of token IDs stored as a sorted SmallVec.
///
/// Invariant: elements are always sorted and deduplicated.
/// Construct via `TokenSet::from_token_ids()`, `TokenSet::from_u16_iter()`,
/// or `.collect()` from an iterator of u16.
#[derive(Clone, Debug, PartialEq, Eq, Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct TokenSet(SmallVec<[u16; 64]>);

impl TokenSet {
    /// Create a TokenSet from an iterator of u16 token IDs.
    /// Sorts and deduplicates the input.
    pub fn from_u16_iter<I: IntoIterator<Item = u16>>(iter: I) -> Self {
        let mut inner: SmallVec<[u16; 64]> = iter.into_iter().collect();
        inner.sort_unstable();
        inner.dedup();
        Self(inner)
    }

    /// Create a TokenSet from an iterator of TokenId values.
    pub fn from_token_ids<I: IntoIterator<Item = TokenId>>(iter: I) -> Self {
        Self::from_u16_iter(iter.into_iter().map(|tid| tid.raw()))
    }

    /// Create an empty TokenSet.
    pub fn new() -> Self {
        Self(SmallVec::new())
    }

    /// Number of tokens in the set.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Is the set empty?
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Return true if the set contains the given token ID.
    pub fn contains_token_id(&self, token_id: TokenId) -> bool {
        self.0.contains(&token_id.raw())
    }

    /// Get the subset containing only high-value (legalese) tokens.
    pub fn high_subset(&self, dictionary: &TokenDictionary) -> Self {
        Self::from_u16_iter(
            self.iter()
                .filter(|&tid| dictionary.token_kind(TokenId::new(tid)) == TokenKind::Legalese),
        )
    }

    /// Count intersection with another TokenSet (no allocation).
    pub fn intersection_count(&self, other: &TokenSet) -> usize {
        let (mut i, mut j, mut count) = (0, 0, 0);
        while i < self.0.len() && j < other.0.len() {
            match self.0[i].cmp(&other.0[j]) {
                Ordering::Less => i += 1,
                Ordering::Greater => j += 1,
                Ordering::Equal => {
                    count += 1;
                    i += 1;
                    j += 1;
                }
            }
        }
        count
    }

    /// Materialize intersection with another TokenSet.
    pub fn intersection(&self, other: &TokenSet) -> TokenSet {
        let mut result = SmallVec::new();
        let (mut i, mut j) = (0, 0);
        while i < self.0.len() && j < other.0.len() {
            match self.0[i].cmp(&other.0[j]) {
                Ordering::Less => i += 1,
                Ordering::Greater => j += 1,
                Ordering::Equal => {
                    result.push(self.0[i]);
                    i += 1;
                    j += 1;
                }
            }
        }
        Self(result)
    }

    /// Iterate over the sorted token IDs.
    pub fn iter(&self) -> impl Iterator<Item = u16> + '_ {
        self.0.iter().copied()
    }
}

impl Deref for TokenSet {
    type Target = [u16];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Default for TokenSet {
    fn default() -> Self {
        Self::new()
    }
}

impl std::iter::FromIterator<u16> for TokenSet {
    fn from_iter<T: IntoIterator<Item = u16>>(iter: T) -> Self {
        Self::from_u16_iter(iter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license_detection::index::dictionary::tid;

    #[test]
    fn test_from_token_ids() {
        let set = TokenSet::from_token_ids([tid(4), tid(2), tid(4), tid(1)]);

        assert_eq!(set.iter().collect::<Vec<_>>(), vec![1, 2, 4]);
        assert!(set.contains_token_id(tid(1)));
        assert!(set.contains_token_id(tid(2)));
        assert!(set.contains_token_id(tid(4)));
    }

    #[test]
    fn test_high_subset() {
        let set = TokenSet::from_u16_iter([1, 2, 5, 10]);
        let dict = TokenDictionary::new_with_legalese_pairs(&[("one", 1), ("two", 2)]);

        let high_set = set.high_subset(&dict);

        assert_eq!(high_set.iter().collect::<Vec<_>>(), vec![1, 2]);
    }
}
