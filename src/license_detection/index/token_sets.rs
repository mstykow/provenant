//! Token set and multiset utilities for license detection.

use std::collections::HashSet;

use crate::license_detection::TokenMultiset;
use crate::license_detection::index::dictionary::{TokenDictionary, TokenId, TokenKind};

/// Build a token ID set and multiset from a sequence of token IDs.
///
/// The set contains unique token IDs, while the multiset (bag) contains
/// all token IDs with their occurrence counts.
///
/// Corresponds to Python: `build_set_and_tids_mset()` in match_set.py
///
/// # Arguments
///
/// * `token_ids` - Sequence of token IDs
///
/// # Returns
///
/// A tuple of (set of unique token IDs, multiset of token ID -> count)
pub fn build_set_and_mset(token_ids: &[TokenId]) -> (HashSet<TokenId>, TokenMultiset) {
    let tids_mset = TokenMultiset::from_token_ids(token_ids);
    let tids_set: HashSet<TokenId> = tids_mset.keys().copied().collect();

    (tids_set, tids_mset)
}

/// Count unique tokens in a set (equivalent to Python's `len()` for intbitset).
///
/// Corresponds to Python: `tids_set_counter = len` (line 116 in match_set.py)
///
/// # Arguments
///
/// * `tids_set` - Set of unique token IDs
///
/// # Returns
///
/// Number of unique tokens in the set
pub fn tids_set_counter(tids_set: &HashSet<TokenId>) -> usize {
    tids_set.len()
}

/// Get subset of a token set containing only high-value (legalese) tokens.
///
/// High-value tokens are those with IDs less than len_legalese.
///
/// Corresponds to Python: `high_tids_set_subset()` in match_set.py (line 148)
///
/// # Arguments
///
/// * `tids_set` - Set of token IDs
/// * `len_legalese` - Number of legalese tokens (IDs < this are high-value)
///
/// # Returns
///
/// Subset of tids_set containing only token IDs < len_legalese
pub fn high_tids_set_subset(
    tids_set: &HashSet<TokenId>,
    dictionary: &TokenDictionary,
) -> HashSet<TokenId> {
    tids_set
        .iter()
        .filter(|&&tid| dictionary.token_kind(tid) == TokenKind::Legalese)
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license_detection::index::dictionary::{TokenDictionary, TokenId, tid};

    #[test]
    fn test_build_set_and_mset() {
        let token_ids = vec![tid(1), tid(2), tid(3), tid(2), tid(4), tid(1), tid(1)];
        let (tids_set, tids_mset) = build_set_and_mset(&token_ids);

        assert_eq!(tids_set.len(), 4);
        assert!(tids_set.contains(&tid(1)));
        assert!(tids_set.contains(&tid(2)));
        assert!(tids_set.contains(&tid(3)));
        assert!(tids_set.contains(&tid(4)));

        assert_eq!(tids_mset.get(&tid(1)), Some(&3));
        assert_eq!(tids_mset.get(&tid(2)), Some(&2));
        assert_eq!(tids_mset.get(&tid(3)), Some(&1));
        assert_eq!(tids_mset.get(&tid(4)), Some(&1));
    }

    #[test]
    fn test_build_set_and_mset_empty() {
        let token_ids: Vec<TokenId> = vec![];
        let (tids_set, tids_mset) = build_set_and_mset(&token_ids);

        assert_eq!(tids_set.len(), 0);
        assert_eq!(tids_mset.len(), 0);
    }

    #[test]
    fn test_tids_set_counter() {
        let mut set = HashSet::new();
        set.insert(tid(1));
        set.insert(tid(2));
        set.insert(tid(3));
        assert_eq!(tids_set_counter(&set), 3);
    }

    #[test]
    fn test_high_tids_set_subset() {
        let mut set = HashSet::new();
        set.insert(tid(1));
        set.insert(tid(2));
        set.insert(tid(5));
        set.insert(tid(10));

        let dict = TokenDictionary::new_with_legalese(&[("one", 1), ("two", 2)]);

        let high_set = high_tids_set_subset(&set, &dict);
        assert_eq!(high_set.len(), 2);
        assert!(high_set.contains(&tid(1)));
        assert!(high_set.contains(&tid(2)));
        assert!(!high_set.contains(&tid(5)));
        assert!(!high_set.contains(&tid(10)));
    }
}
