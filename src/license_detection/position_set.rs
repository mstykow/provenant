// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use bit_set::BitSet;

use crate::license_detection::models::position_span::PositionSpan;

/// A set of usize positions stored as a BitSet.
/// Provides O(1) membership testing and efficient set operations.
/// Caches bounds for cheap overlap pre-checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositionSet {
    bitset: BitSet,
    min_pos: usize,
    max_pos: usize,
}

impl PositionSet {
    /// Create a PositionSet from an iterator of usize positions.
    pub fn from_usize_iter<I: IntoIterator<Item = usize>>(iter: I) -> Self {
        let mut bitset = BitSet::new();
        let mut min_pos = usize::MAX;
        let mut max_pos = 0;

        for pos in iter {
            bitset.insert(pos);
            min_pos = min_pos.min(pos);
            max_pos = max_pos.max(pos);
        }

        Self {
            bitset,
            min_pos,
            max_pos,
        }
    }

    /// Create an empty PositionSet.
    pub fn new() -> Self {
        Self {
            bitset: BitSet::new(),
            min_pos: usize::MAX,
            max_pos: 0,
        }
    }

    /// Number of positions in the set.
    pub fn len(&self) -> usize {
        self.bitset.count()
    }

    /// Is the set empty?
    pub fn is_empty(&self) -> bool {
        self.bitset.is_empty()
    }

    /// Returns the minimum position in the set.
    ///
    /// Returns `usize::MAX` for an empty set.
    pub fn min_pos(&self) -> usize {
        self.min_pos
    }

    /// Returns the maximum position in the set.
    ///
    /// Returns `0` for an empty set.
    pub fn max_pos(&self) -> usize {
        self.max_pos
    }

    /// Insert a position.
    pub fn insert(&mut self, pos: usize) -> bool {
        let inserted = self.bitset.insert(pos);
        if inserted {
            self.min_pos = self.min_pos.min(pos);
            self.max_pos = self.max_pos.max(pos);
        }
        inserted
    }

    /// Extend this set from a PositionSpan without allocating an intermediate set.
    pub fn extend_from_span(&mut self, span: &PositionSpan) {
        match span {
            PositionSpan::Range { start, end } => {
                for pos in *start..*end {
                    self.insert(pos);
                }
            }
            PositionSpan::Discrete(positions) => {
                for &pos in positions {
                    self.insert(pos);
                }
            }
        }
    }

    /// Check if position is in the set.
    pub fn contains(&self, pos: usize) -> bool {
        self.bitset.contains(pos)
    }

    /// Remove a position from the set.
    pub fn remove(&mut self, pos: usize) -> bool {
        self.bitset.remove(pos)
    }

    /// Remove all positions in a span from the set.
    pub fn remove_span(&mut self, span: &PositionSpan) {
        for pos in span.iter() {
            self.remove(pos);
        }
    }

    /// Quick check if a range [range_start, range_end) might overlap with this set.
    /// Returns true if the bounding boxes overlap, false if they definitely don't.
    /// This is O(1) and used as a pre-filter before the expensive BitSet check.
    #[inline]
    pub fn may_overlap_range(&self, range_start: usize, range_end: usize) -> bool {
        // min_pos == usize::MAX means empty set (see new())
        if self.min_pos == usize::MAX {
            return false;
        }
        range_end > self.min_pos && range_start <= self.max_pos
    }

    /// Compute the union of this set with another PositionSet.
    ///
    /// Returns a new PositionSet containing all positions from both sets.
    pub fn union(&self, other: &PositionSet) -> PositionSet {
        let mut result = self.clone();
        for pos in other.iter() {
            result.insert(pos);
        }
        result
    }

    /// Return the difference (elements in self but not in other).
    pub fn difference(&self, other: &PositionSet) -> PositionSet {
        let mut result = PositionSet::new();
        for pos in self.bitset.iter() {
            if !other.bitset.contains(pos) {
                result.insert(pos);
            }
        }
        result
    }

    /// Count elements in the intersection of self and other.
    pub fn intersection_len(&self, other: &PositionSet) -> usize {
        self.bitset
            .iter()
            .filter(|&p| other.bitset.contains(p))
            .count()
    }

    /// Check if this set overlaps with a PositionSpan.
    /// Uses O(1) bounds check before the O(n) element-wise check.
    pub fn overlaps_span(&self, span: &PositionSpan) -> bool {
        let (span_min, span_max) = span.bounds();
        if span.is_empty() {
            return false;
        }
        if !self.may_overlap_range(span_min, span_max) {
            return false;
        }
        span.iter().any(|p| self.contains(p))
    }

    /// Check if this set contains all positions in a range.
    /// Returns true for empty ranges.
    pub fn contains_range(&self, range: std::ops::Range<usize>) -> bool {
        if range.is_empty() {
            return true;
        }
        let (start, end) = (range.start, range.end);
        if !self.may_overlap_range(start, end) {
            return false;
        }
        (start..end).all(|pos| self.contains(pos))
    }

    /// Iterate over positions.
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.bitset.iter()
    }

    /// Convert this PositionSet to a PositionSpan.
    ///
    /// If positions are contiguous, returns a Range; otherwise returns Discrete.
    pub fn to_position_span(&self) -> PositionSpan {
        if self.is_empty() {
            return PositionSpan::empty();
        }

        let positions: Vec<usize> = self.iter().collect();
        let is_contiguous = positions.windows(2).all(|w| w[1] == w[0] + 1);

        if is_contiguous {
            PositionSpan::range(self.min_pos, self.max_pos + 1)
        } else {
            PositionSpan::from_positions(positions)
        }
    }
}

impl Default for PositionSet {
    fn default() -> Self {
        Self::new()
    }
}

impl std::iter::FromIterator<usize> for PositionSet {
    fn from_iter<T: IntoIterator<Item = usize>>(iter: T) -> Self {
        Self::from_usize_iter(iter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_empty() {
        let set = PositionSet::new();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn test_from_usize_iter_sorted() {
        let set = PositionSet::from_usize_iter(vec![1, 2, 3]);
        assert_eq!(set.len(), 3);
        assert_eq!(set.iter().collect::<Vec<_>>(), vec![1, 2, 3]);
    }

    #[test]
    fn test_from_usize_iter_unsorted() {
        let set = PositionSet::from_usize_iter(vec![3, 1, 2]);
        assert_eq!(set.iter().collect::<Vec<_>>(), vec![1, 2, 3]);
    }

    #[test]
    fn test_from_usize_iter_dedup() {
        let set = PositionSet::from_usize_iter(vec![1, 2, 2, 3, 3, 3]);
        assert_eq!(set.len(), 3);
        assert_eq!(set.iter().collect::<Vec<_>>(), vec![1, 2, 3]);
    }

    #[test]
    fn test_insert() {
        let mut set = PositionSet::new();
        assert!(set.insert(2));
        assert!(set.insert(1));
        assert!(set.insert(3));
        assert!(!set.insert(2)); // Already present
        assert_eq!(set.iter().collect::<Vec<_>>(), vec![1, 2, 3]);
    }

    #[test]
    fn test_difference() {
        let a = PositionSet::from_usize_iter(vec![1, 2, 3, 4]);
        let b = PositionSet::from_usize_iter(vec![2, 4, 6]);
        let diff = a.difference(&b);
        assert_eq!(diff.iter().collect::<Vec<_>>(), vec![1, 3]);
    }

    #[test]
    fn test_difference_empty() {
        let a = PositionSet::from_usize_iter(vec![1, 2, 3]);
        let b = PositionSet::new();
        let diff = a.difference(&b);
        assert_eq!(diff.iter().collect::<Vec<_>>(), vec![1, 2, 3]);
    }

    #[test]
    fn test_difference_all_overlap() {
        let a = PositionSet::from_usize_iter(vec![1, 2, 3]);
        let b = PositionSet::from_usize_iter(vec![1, 2, 3]);
        let diff = a.difference(&b);
        assert!(diff.is_empty());
    }

    #[test]
    fn test_contains() {
        let set = PositionSet::from_usize_iter(vec![1, 3, 5]);
        assert!(set.contains(1));
        assert!(set.contains(3));
        assert!(set.contains(5));
        assert!(!set.contains(0));
        assert!(!set.contains(2));
        assert!(!set.contains(4));
    }

    #[test]
    fn test_collect() {
        let set: PositionSet = vec![3, 1, 2].into_iter().collect();
        assert_eq!(set.iter().collect::<Vec<_>>(), vec![1, 2, 3]);
    }

    #[test]
    fn test_extend_from_span_range() {
        let mut set = PositionSet::new();
        set.extend_from_span(&PositionSpan::range(5, 10));
        assert_eq!(set.len(), 5);
        assert!(set.contains(5));
        assert!(set.contains(9));
        assert!(!set.contains(4));
        assert!(!set.contains(10));
    }

    #[test]
    fn test_extend_from_span_discrete() {
        let mut set = PositionSet::new();
        set.extend_from_span(&PositionSpan::from_positions(vec![1, 3, 5]));
        assert_eq!(set.len(), 3);
        assert!(set.contains(1));
        assert!(set.contains(3));
        assert!(set.contains(5));
        assert!(!set.contains(2));
    }

    #[test]
    fn test_extend_from_span_merge() {
        let mut set = PositionSet::from_usize_iter(vec![1, 2, 3]);
        set.extend_from_span(&PositionSpan::range(2, 6));
        assert_eq!(set.len(), 5);
        assert_eq!(set.iter().collect::<Vec<_>>(), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_overlaps_span_range_yes() {
        let set = PositionSet::from_usize_iter(vec![5, 6, 7]);
        assert!(set.overlaps_span(&PositionSpan::range(6, 10)));
        assert!(set.overlaps_span(&PositionSpan::range(0, 6)));
    }

    #[test]
    fn test_overlaps_span_range_no() {
        let set = PositionSet::from_usize_iter(vec![1, 2, 3]);
        assert!(!set.overlaps_span(&PositionSpan::range(5, 10)));
        assert!(!set.overlaps_span(&PositionSpan::range(10, 20)));
    }

    #[test]
    fn test_overlaps_span_discrete_yes() {
        let set = PositionSet::from_usize_iter(vec![1, 2, 3, 10, 11]);
        assert!(set.overlaps_span(&PositionSpan::from_positions(vec![3, 4, 5])));
        assert!(set.overlaps_span(&PositionSpan::from_positions(vec![0, 1])));
    }

    #[test]
    fn test_overlaps_span_discrete_no() {
        let set = PositionSet::from_usize_iter(vec![1, 2, 3]);
        assert!(!set.overlaps_span(&PositionSpan::from_positions(vec![5, 6, 7])));
    }

    #[test]
    fn test_overlaps_span_empty() {
        let set = PositionSet::from_usize_iter(vec![1, 2, 3]);
        assert!(!set.overlaps_span(&PositionSpan::empty()));
    }

    #[test]
    fn test_contains_range_yes() {
        let set = PositionSet::from_usize_iter(vec![1, 2, 3, 4, 5]);
        assert!(set.contains_range(1..6));
        assert!(set.contains_range(2..4));
        assert!(set.contains_range(1..6));
    }

    #[test]
    fn test_contains_range_no() {
        let set = PositionSet::from_usize_iter(vec![1, 2, 3]);
        assert!(!set.contains_range(0..4));
        assert!(!set.contains_range(3..5));
        assert!(!set.contains_range(5..10));
    }

    #[test]
    fn test_contains_range_empty() {
        let set = PositionSet::from_usize_iter(vec![1, 2, 3]);
        assert!(set.contains_range(5..5));
        assert!(set.contains_range(0..0));
    }

    #[test]
    fn test_contains_range_disjoint() {
        let set = PositionSet::from_usize_iter(vec![10, 11, 12]);
        assert!(!set.contains_range(0..5));
        assert!(!set.contains_range(15..20));
    }

    #[test]
    fn test_to_position_span_empty() {
        let set = PositionSet::new();
        let span = set.to_position_span();
        assert!(span.is_empty());
    }

    #[test]
    fn test_to_position_span_contiguous() {
        let set = PositionSet::from_usize_iter(vec![5, 6, 7, 8]);
        let span = set.to_position_span();
        assert_eq!(span, PositionSpan::range(5, 9));
    }

    #[test]
    fn test_to_position_span_single() {
        let set = PositionSet::from_usize_iter(vec![10]);
        let span = set.to_position_span();
        assert_eq!(span, PositionSpan::range(10, 11));
    }

    #[test]
    fn test_to_position_span_discrete() {
        let set = PositionSet::from_usize_iter(vec![1, 3, 5, 7]);
        let span = set.to_position_span();
        assert_eq!(span, PositionSpan::from_positions(vec![1, 3, 5, 7]));
    }

    #[test]
    fn test_to_position_span_two_with_gap() {
        let set = PositionSet::from_usize_iter(vec![1, 3]);
        let span = set.to_position_span();
        assert_eq!(span, PositionSpan::from_positions(vec![1, 3]));
    }

    #[test]
    fn test_min_max_pos() {
        let set = PositionSet::from_usize_iter(vec![5, 10, 15]);
        assert_eq!(set.min_pos(), 5);
        assert_eq!(set.max_pos(), 15);
    }

    #[test]
    fn test_min_max_pos_empty() {
        let set = PositionSet::new();
        assert_eq!(set.min_pos(), usize::MAX);
        assert_eq!(set.max_pos(), 0);
    }
}
