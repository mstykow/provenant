use std::collections::HashMap;
use std::ops::Deref;

use crate::license_detection::index::dictionary::{TokenDictionary, TokenId, TokenKind};

/// A multiset of token IDs stored as token -> occurrence count.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TokenMultiset(HashMap<TokenId, usize>);

impl TokenMultiset {
    /// Create a TokenMultiset from a sequence of token IDs.
    pub fn from_token_ids(token_ids: &[TokenId]) -> Self {
        let mut counts = HashMap::new();

        for &tid in token_ids {
            *counts.entry(tid).or_insert(0) += 1;
        }

        Self(counts)
    }

    /// Total number of token occurrences in the multiset.
    pub fn total_count(&self) -> usize {
        self.0.values().sum()
    }

    /// Get a subset containing only high-value (legalese) tokens.
    pub fn high_subset(&self, dictionary: &TokenDictionary) -> Self {
        self.0
            .iter()
            .filter(|(tid, _)| dictionary.token_kind(**tid) == TokenKind::Legalese)
            .map(|(&tid, &count)| (tid, count))
            .collect()
    }

    /// Materialize the multiset intersection with another TokenMultiset.
    pub fn intersection(&self, other: &Self) -> Self {
        let (smaller, larger) = if self.0.len() < other.0.len() {
            (&self.0, &other.0)
        } else {
            (&other.0, &self.0)
        };

        smaller
            .iter()
            .filter_map(|(&tid, &count)| {
                larger
                    .get(&tid)
                    .map(|&other_count| (tid, count.min(other_count)))
            })
            .collect()
    }
}

impl Deref for TokenMultiset {
    type Target = HashMap<TokenId, usize>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromIterator<(TokenId, usize)> for TokenMultiset {
    fn from_iter<T: IntoIterator<Item = (TokenId, usize)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license_detection::index::dictionary::{TokenDictionary, tid};

    #[test]
    fn test_from_token_ids() {
        let token_ids = vec![tid(1), tid(2), tid(3), tid(2), tid(4), tid(1), tid(1)];
        let multiset = TokenMultiset::from_token_ids(&token_ids);

        assert_eq!(multiset.get(&tid(1)), Some(&3));
        assert_eq!(multiset.get(&tid(2)), Some(&2));
        assert_eq!(multiset.get(&tid(3)), Some(&1));
        assert_eq!(multiset.get(&tid(4)), Some(&1));
    }

    #[test]
    fn test_total_count() {
        let token_ids = vec![tid(1), tid(2), tid(3), tid(2), tid(1), tid(1)];
        let multiset = TokenMultiset::from_token_ids(&token_ids);

        assert_eq!(multiset.total_count(), 6);
    }

    #[test]
    fn test_high_subset() {
        let token_ids = vec![tid(1), tid(1), tid(2), tid(5), tid(10)];
        let multiset = TokenMultiset::from_token_ids(&token_ids);
        let dict = TokenDictionary::new_with_legalese(&[("one", 1), ("two", 2)]);

        let high_multiset = multiset.high_subset(&dict);

        assert_eq!(high_multiset.len(), 2);
        assert_eq!(high_multiset.get(&tid(1)), Some(&2));
        assert_eq!(high_multiset.get(&tid(2)), Some(&1));
        assert!(!high_multiset.contains_key(&tid(5)));
        assert!(!high_multiset.contains_key(&tid(10)));
    }

    #[test]
    fn test_intersection() {
        let left = TokenMultiset::from_token_ids(&[tid(1), tid(1), tid(2), tid(3)]);
        let right = TokenMultiset::from_token_ids(&[tid(1), tid(2), tid(2), tid(4)]);

        let intersection = left.intersection(&right);

        assert_eq!(intersection.get(&tid(1)), Some(&1));
        assert_eq!(intersection.get(&tid(2)), Some(&1));
        assert!(!intersection.contains_key(&tid(3)));
        assert!(!intersection.contains_key(&tid(4)));
        assert_eq!(intersection.total_count(), 2);
    }
}
