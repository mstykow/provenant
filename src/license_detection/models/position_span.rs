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

#[derive(Debug, Clone, PartialEq)]
pub enum PositionSpan {
    Range { start: usize, end: usize },
    Discrete(Vec<usize>),
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
    #[allow(dead_code)]
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
}
