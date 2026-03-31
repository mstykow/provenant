//! Position span types for license detection.

use crate::license_detection::position_set::PositionSet;

pub enum SpanIter<'a> {
    Range(std::ops::Range<usize>),
    Slice(std::iter::Copied<std::slice::Iter<'a, usize>>),
}

impl<'a> Iterator for SpanIter<'a> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            SpanIter::Range(range) => range.next(),
            SpanIter::Slice(iter) => iter.next(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            SpanIter::Range(range) => range.size_hint(),
            SpanIter::Slice(iter) => iter.size_hint(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum PositionSpan {
    Range { start: usize, end: usize },
    Discrete(Vec<usize>),
}

impl PartialEq for PositionSpan {
    /// Compare two PositionSpans for semantic equality.
    ///
    /// Returns true if both spans contain exactly the same positions,
    /// regardless of representation (Range vs Discrete).
    ///
    /// Performance:
    /// - Range vs Range: O(1)
    /// - Discrete vs Discrete: O(n)
    /// - Range vs Discrete: O(n) with early exit on length mismatch
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                PositionSpan::Range { start: s1, end: e1 },
                PositionSpan::Range { start: s2, end: e2 },
            ) => s1 == s2 && e1 == e2,
            (PositionSpan::Discrete(p1), PositionSpan::Discrete(p2)) => p1 == p2,
            (PositionSpan::Range { start, end }, PositionSpan::Discrete(positions)) => {
                let range_len = end.saturating_sub(*start);
                if range_len != positions.len() {
                    return false;
                }
                if positions.is_empty() {
                    return true;
                }
                positions.iter().all(|&p| *start <= p && p < *end)
            }
            (PositionSpan::Discrete(_), PositionSpan::Range { .. }) => other == self,
        }
    }
}

impl PositionSpan {
    pub fn range(start: usize, end: usize) -> Self {
        Self::Range { start, end }
    }

    pub fn new(start: usize, end: usize) -> Self {
        Self::range(start, end)
    }

    pub fn from_positions(positions: Vec<usize>) -> Self {
        if positions.is_empty() {
            return Self::empty();
        }

        if positions.len() == 1 {
            return Self::Range {
                start: positions[0],
                end: positions[0] + 1,
            };
        }

        let mut sorted = positions.clone();
        sorted.sort_unstable();
        sorted.dedup();

        let is_contiguous = sorted.windows(2).all(|w| w[1] == w[0] + 1);

        if is_contiguous {
            Self::Range {
                start: sorted[0],
                end: sorted[sorted.len() - 1] + 1,
            }
        } else {
            Self::Discrete(sorted)
        }
    }

    pub fn empty() -> Self {
        Self::Range { start: 0, end: 0 }
    }

    pub fn iter(&self) -> SpanIter<'_> {
        match self {
            PositionSpan::Range { start, end } => SpanIter::Range(*start..*end),
            PositionSpan::Discrete(positions) => SpanIter::Slice(positions.iter().copied()),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            PositionSpan::Range { start, end } => end.saturating_sub(*start),
            PositionSpan::Discrete(positions) => positions.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn bounds(&self) -> (usize, usize) {
        match self {
            PositionSpan::Range { start, end } => (*start, *end),
            PositionSpan::Discrete(positions) => {
                if positions.is_empty() {
                    return (0, 0);
                }
                let min = positions.iter().copied().min().unwrap_or(0);
                let max = positions.iter().copied().max().unwrap_or(0);
                (min, max + 1)
            }
        }
    }

    pub fn contains(&self, pos: usize) -> bool {
        match self {
            PositionSpan::Range { start, end } => *start <= pos && pos < *end,
            PositionSpan::Discrete(positions) => positions.binary_search(&pos).is_ok(),
        }
    }

    pub fn to_position_set(&self) -> PositionSet {
        match self {
            PositionSpan::Range { start, end } => (*start..*end).collect(),
            PositionSpan::Discrete(positions) => positions.iter().copied().collect(),
        }
    }

    pub fn to_vec(&self) -> Vec<usize> {
        match self {
            PositionSpan::Range { start, end } => (*start..*end).collect(),
            PositionSpan::Discrete(positions) => positions.clone(),
        }
    }

    /// Returns true if the positions form a contiguous range with no gaps.
    /// This is always true for `Range` variants, and checks adjacency for `Discrete`.
    pub fn is_contiguous(&self) -> bool {
        match self {
            PositionSpan::Range { .. } => true,
            PositionSpan::Discrete(positions) => {
                if positions.len() <= 1 {
                    return true;
                }
                positions.windows(2).all(|w| w[1] == w[0] + 1)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_range_creation() {
        let span = PositionSpan::range(5, 10);
        assert_eq!(span.len(), 5);
        assert!(!span.is_empty());
        assert_eq!(span.bounds(), (5, 10));
        assert!(span.is_contiguous());
    }

    #[test]
    fn test_new_backwards_compatible() {
        let span = PositionSpan::new(3, 7);
        assert_eq!(span.len(), 4);
        assert_eq!(span.to_vec(), vec![3, 4, 5, 6]);
    }

    #[test]
    fn test_empty() {
        let span = PositionSpan::empty();
        assert_eq!(span.len(), 0);
        assert!(span.is_empty());
        assert_eq!(span.bounds(), (0, 0));
    }

    #[test]
    fn test_from_positions_empty() {
        let span = PositionSpan::from_positions(vec![]);
        assert!(span.is_empty());
    }

    #[test]
    fn test_from_positions_single() {
        let span = PositionSpan::from_positions(vec![5]);
        assert_eq!(span.len(), 1);
        assert!(span.is_contiguous());
        assert!(span.contains(5));
    }

    #[test]
    fn test_from_positions_contiguous() {
        let span = PositionSpan::from_positions(vec![3, 4, 5]);
        assert!(span.is_contiguous());
        assert_eq!(span.len(), 3);
        assert_eq!(span.bounds(), (3, 6));
    }

    #[test]
    fn test_from_positions_non_contiguous() {
        let span = PositionSpan::from_positions(vec![1, 3, 5]);
        assert!(!span.is_contiguous());
        assert_eq!(span.len(), 3);
        assert_eq!(span.bounds(), (1, 6));
    }

    #[test]
    fn test_from_positions_unsorted_with_duplicates() {
        let span = PositionSpan::from_positions(vec![5, 3, 4, 3, 5]);
        assert!(span.is_contiguous());
        assert_eq!(span.to_vec(), vec![3, 4, 5]);
    }

    #[test]
    fn test_contains_range() {
        let span = PositionSpan::range(5, 10);
        assert!(!span.contains(4));
        assert!(span.contains(5));
        assert!(span.contains(7));
        assert!(span.contains(9));
        assert!(!span.contains(10));
    }

    #[test]
    fn test_contains_discrete() {
        let span = PositionSpan::from_positions(vec![1, 3, 5]);
        assert!(span.contains(1));
        assert!(!span.contains(2));
        assert!(span.contains(3));
        assert!(!span.contains(4));
        assert!(span.contains(5));
    }

    #[test]
    fn test_iter_range() {
        let span = PositionSpan::range(2, 5);
        let positions: Vec<_> = span.iter().collect();
        assert_eq!(positions, vec![2, 3, 4]);
    }

    #[test]
    fn test_iter_discrete() {
        let span = PositionSpan::from_positions(vec![1, 3, 5]);
        let positions: Vec<_> = span.iter().collect();
        assert_eq!(positions, vec![1, 3, 5]);
    }

    #[test]
    fn test_to_vec_range() {
        let span = PositionSpan::range(1, 4);
        assert_eq!(span.to_vec(), vec![1, 2, 3]);
    }

    #[test]
    fn test_to_vec_discrete() {
        let span = PositionSpan::from_positions(vec![2, 4, 6]);
        assert_eq!(span.to_vec(), vec![2, 4, 6]);
    }

    #[test]
    fn test_to_position_set() {
        let span = PositionSpan::range(1, 4);
        let set = span.to_position_set();
        assert_eq!(set.len(), 3);
        assert!(set.contains(1));
        assert!(set.contains(2));
        assert!(set.contains(3));
    }

    #[test]
    fn test_is_contiguous_discrete_single() {
        let span = PositionSpan::from_positions(vec![5]);
        assert!(span.is_contiguous());
    }

    #[test]
    fn test_is_contiguous_discrete_gap() {
        let span = PositionSpan::from_positions(vec![1, 2, 4]);
        assert!(!span.is_contiguous());
    }

    // ============================================================
    // Comprehensive equality tests: all 2x2 combinations
    // ============================================================

    // ---- Range vs Range ----

    #[test]
    fn test_eq_range_range_both_empty() {
        let a = PositionSpan::range(0, 0);
        let b = PositionSpan::range(0, 0);
        assert_eq!(a, b);
    }

    #[test]
    fn test_eq_range_range_one_empty() {
        let a = PositionSpan::range(0, 0);
        let b = PositionSpan::range(0, 3);
        assert_ne!(a, b);
    }

    #[test]
    fn test_eq_range_range_equal() {
        let a = PositionSpan::range(2, 5);
        let b = PositionSpan::range(2, 5);
        assert_eq!(a, b);
    }

    #[test]
    fn test_eq_range_range_disjoint() {
        let a = PositionSpan::range(0, 3);
        let b = PositionSpan::range(5, 8);
        assert_ne!(a, b);
    }

    #[test]
    fn test_eq_range_range_overlapping() {
        let a = PositionSpan::range(0, 5);
        let b = PositionSpan::range(3, 8);
        assert_ne!(a, b);
    }

    // ---- Discrete vs Discrete ----

    #[test]
    fn test_eq_discrete_discrete_both_empty() {
        let a = PositionSpan::Discrete(vec![]);
        let b = PositionSpan::Discrete(vec![]);
        assert_eq!(a, b);
    }

    #[test]
    fn test_eq_discrete_discrete_one_empty() {
        let a = PositionSpan::Discrete(vec![]);
        let b = PositionSpan::Discrete(vec![0, 1, 2]);
        assert_ne!(a, b);
    }

    #[test]
    fn test_eq_discrete_discrete_equal() {
        let a = PositionSpan::Discrete(vec![0, 1, 2]);
        let b = PositionSpan::Discrete(vec![0, 1, 2]);
        assert_eq!(a, b);
    }

    #[test]
    fn test_eq_discrete_discrete_disjoint() {
        let a = PositionSpan::Discrete(vec![0, 1, 2]);
        let b = PositionSpan::Discrete(vec![5, 6, 7]);
        assert_ne!(a, b);
    }

    #[test]
    fn test_eq_discrete_discrete_overlapping() {
        let a = PositionSpan::Discrete(vec![0, 1, 2, 3]);
        let b = PositionSpan::Discrete(vec![2, 3, 4, 5]);
        assert_ne!(a, b);
    }

    // ---- Range vs Discrete ----

    #[test]
    fn test_eq_range_discrete_both_empty() {
        let range = PositionSpan::range(0, 0);
        let discrete = PositionSpan::Discrete(vec![]);
        assert_eq!(range, discrete);
        assert_eq!(discrete, range);
    }

    #[test]
    fn test_eq_range_discrete_range_empty() {
        let range = PositionSpan::range(0, 0);
        let discrete = PositionSpan::Discrete(vec![0, 1, 2]);
        assert_ne!(range, discrete);
        assert_ne!(discrete, range);
    }

    #[test]
    fn test_eq_range_discrete_discrete_empty() {
        let range = PositionSpan::range(0, 3);
        let discrete = PositionSpan::Discrete(vec![]);
        assert_ne!(range, discrete);
        assert_ne!(discrete, range);
    }

    #[test]
    fn test_eq_range_discrete_equal() {
        let range = PositionSpan::range(2, 5);
        let discrete = PositionSpan::Discrete(vec![2, 3, 4]);
        assert_eq!(range, discrete);
        assert_eq!(discrete, range);
    }

    #[test]
    fn test_eq_range_discrete_disjoint() {
        let range = PositionSpan::range(0, 3);
        let discrete = PositionSpan::Discrete(vec![5, 6, 7]);
        assert_ne!(range, discrete);
        assert_ne!(discrete, range);
    }

    #[test]
    fn test_eq_range_discrete_overlapping_partial() {
        let range = PositionSpan::range(0, 5);
        let discrete = PositionSpan::Discrete(vec![2, 3, 4, 5, 6]);
        assert_ne!(range, discrete);
        assert_ne!(discrete, range);
    }

    #[test]
    fn test_eq_range_discrete_subset() {
        // Discrete is subset of range
        let range = PositionSpan::range(0, 10);
        let discrete = PositionSpan::Discrete(vec![2, 3, 4]);
        assert_ne!(range, discrete);
        assert_ne!(discrete, range);
    }

    #[test]
    fn test_eq_range_discrete_superset() {
        // Discrete extends beyond range
        let range = PositionSpan::range(5, 10);
        let discrete = PositionSpan::Discrete(vec![3, 4, 5, 6, 7]);
        assert_ne!(range, discrete);
        assert_ne!(discrete, range);
    }

    // ---- Mixed empty tests ----

    #[test]
    fn test_eq_empty_different_representation() {
        let a = PositionSpan::range(0, 0);
        let b = PositionSpan::Discrete(vec![]);
        assert_eq!(a, b);
        assert_eq!(b, a);
    }
}
