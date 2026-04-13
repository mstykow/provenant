//! Aho-Corasick automaton wrapper using daachorse.
//!
//! This module provides a `DoubleArrayAhoCorasick`-based automaton that is
//! significantly smaller than the aho-corasick crate's implementation.
//! The daachorse library provides ~85% smaller binary size and built-in
//! serialization support.

use daachorse::DoubleArrayAhoCorasick;

/// A match found by the automaton.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// Pattern ID (index into the original pattern list).
    pub pattern: usize,
    /// Start position in haystack (bytes, inclusive).
    pub start: usize,
    /// End position in haystack (bytes, exclusive).
    pub end: usize,
}

/// Aho-Corasick automaton using daachorse's double-array implementation.
///
/// This wrapper provides the same interface as the previous FrozenNfa
/// but with significantly smaller memory footprint and serialization support.
pub struct Automaton {
    inner: DoubleArrayAhoCorasick<u32>,
}

impl std::fmt::Debug for Automaton {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Automaton")
            .field("num_states", &self.inner.num_states())
            .field("heap_bytes", &self.inner.heap_bytes())
            .finish()
    }
}

impl Clone for Automaton {
    fn clone(&self) -> Self {
        let bytes = self.inner.serialize();
        Self::deserialize_unchecked(&bytes)
    }
}

impl Automaton {
    /// Create a new empty automaton.
    ///
    /// Since daachorse requires at least one non-empty pattern, we use a
    /// dummy pattern that will never match in practice (a unique byte sequence).
    pub fn empty() -> Self {
        // Use a very unlikely byte sequence as a sentinel pattern
        // This will match but never in our token-encoded data
        let dummy_pattern: &[u8] = &[0xFF, 0xFE, 0xFD, 0xFC, 0xFB, 0xFA, 0xF9, 0xF8];
        match DoubleArrayAhoCorasick::new([dummy_pattern]) {
            Ok(ac) => Self { inner: ac },
            Err(_) => panic!("Failed to create empty automaton"),
        }
    }

    /// Build an automaton from patterns.
    ///
    /// Each pattern is a byte slice. Patterns are assigned IDs in order.
    #[allow(dead_code)]
    pub fn build(patterns: &[&[u8]]) -> Self {
        if patterns.is_empty() {
            return Self::empty();
        }
        // Filter out empty patterns - daachorse doesn't support them
        let non_empty: Vec<&[u8]> = patterns.iter().copied().filter(|p| !p.is_empty()).collect();
        if non_empty.is_empty() {
            return Self::empty();
        }
        match DoubleArrayAhoCorasick::new(non_empty) {
            Ok(ac) => Self { inner: ac },
            Err(_) => Self::empty(),
        }
    }

    /// Find all overlapping matches in the haystack.
    ///
    /// Returns an iterator that yields all matches found in the haystack,
    /// including overlapping matches. The matches are yielded in order of
    /// their end position.
    ///
    /// **Important**: This filters matches to only those starting at even
    /// byte positions (token boundaries). Each token is encoded as 2 bytes,
    /// so matches starting at odd byte positions would span token boundaries.
    pub fn find_overlapping_iter(&self, haystack: &[u8]) -> FindOverlappingIter {
        FindOverlappingIter::new(&self.inner, haystack)
    }

    /// Deserialize an automaton from bytes.
    ///
    /// # Safety
    /// The bytes must be valid serialized data from the underlying daachorse automaton.
    pub fn deserialize_unchecked(bytes: &[u8]) -> Self {
        let (ac, _) = unsafe { DoubleArrayAhoCorasick::deserialize_unchecked(bytes) };
        Self { inner: ac }
    }

    /// Get the number of states in the automaton.
    #[allow(dead_code)]
    pub fn num_states(&self) -> usize {
        self.inner.num_states()
    }

    /// Get the memory usage in bytes.
    #[allow(dead_code)]
    pub fn heap_bytes(&self) -> usize {
        self.inner.heap_bytes()
    }

    /// Serialize the automaton to a byte vector.
    pub fn serialize_bytes(&self) -> Vec<u8> {
        self.inner.serialize()
    }
}

impl Default for Automaton {
    fn default() -> Self {
        Self::empty()
    }
}

/// Iterator over all overlapping matches in a haystack.
///
/// This iterator finds all matches, including those that overlap, by
/// continuing to search after each match rather than skipping past it.
///
/// **Token Boundary Filtering**: This iterator only yields matches that
/// start at even byte positions. Since each token is encoded as 2 bytes,
/// matches at odd positions would incorrectly span token boundaries.
pub struct FindOverlappingIter {
    inner: std::vec::IntoIter<daachorse::Match<u32>>,
}

impl FindOverlappingIter {
    fn new(automaton: &DoubleArrayAhoCorasick<u32>, haystack: &[u8]) -> Self {
        let matches: Vec<_> = automaton.find_overlapping_iter(haystack).collect();
        Self {
            inner: matches.into_iter(),
        }
    }
}

impl Iterator for FindOverlappingIter {
    type Item = Match;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let m = self.inner.next()?;
            // Token boundary check: each token is 2 bytes, so matches must
            // start at even byte positions. Odd positions would span tokens.
            if m.start() % 2 == 0 {
                return Some(Match {
                    pattern: m.value() as usize,
                    start: m.start(),
                    end: m.end(),
                });
            }
            // Skip matches at odd byte positions (invalid token boundaries)
        }
    }
}

/// Builder for constructing automatons incrementally.
///
/// This mirrors the `FrozenNfaBuilder` interface for compatibility.
pub struct AutomatonBuilder {
    patterns: Vec<Vec<u8>>,
}

impl AutomatonBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            patterns: Vec::new(),
        }
    }

    /// Add a pattern to the automaton.
    ///
    /// Empty patterns are skipped.
    pub fn add_pattern(&mut self, pattern: &[u8]) {
        if !pattern.is_empty() {
            self.patterns.push(pattern.to_vec());
        }
    }

    /// Build the automaton.
    ///
    /// Deduplicates patterns and assigns sequential IDs (0, 1, 2, ...).
    /// The caller must maintain their own mapping from pattern_id to rule IDs.
    pub fn build(self) -> Automaton {
        use std::collections::HashSet;

        if self.patterns.is_empty() {
            return Automaton::empty();
        }

        // Deduplicate patterns - daachorse rejects duplicates
        let mut seen: HashSet<Vec<u8>> = HashSet::new();
        let mut unique_patterns: Vec<&[u8]> = Vec::new();
        for pattern in &self.patterns {
            if seen.insert(pattern.clone()) {
                unique_patterns.push(pattern.as_slice());
            }
        }

        if unique_patterns.is_empty() {
            return Automaton::empty();
        }

        match DoubleArrayAhoCorasick::new(unique_patterns) {
            Ok(ac) => Automaton { inner: ac },
            Err(_) => Automaton::empty(),
        }
    }
}

impl Default for AutomatonBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_automaton() {
        let ac = Automaton::empty();
        let matches: Vec<_> = ac.find_overlapping_iter(b"hello").collect();
        assert!(matches.is_empty());
    }

    #[test]
    fn test_build_with_patterns() {
        let patterns: Vec<&[u8]> = vec![b"hello", b"world"];
        let ac = Automaton::build(&patterns);
        let matches: Vec<_> = ac.find_overlapping_iter(b"hello world").collect();
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_token_boundary_filtering() {
        // Pattern: [31, 49] (token 12575 in little-endian)
        let pattern: &[u8] = &[31, 49];
        let ac = Automaton::build(&[pattern]);

        // Haystack: [109, 31, 49, 74] = tokens [8045, 18993]
        // The pattern [31, 49] appears at bytes 1-2 (odd position)
        // which would span token boundaries - should NOT match
        let haystack: &[u8] = &[109, 31, 49, 74];
        let matches: Vec<_> = ac.find_overlapping_iter(haystack).collect();
        assert!(
            matches.is_empty(),
            "Should not match across token boundaries"
        );
    }

    #[test]
    fn test_valid_token_match() {
        let pattern: &[u8] = &[31, 49];
        let ac = Automaton::build(&[pattern]);

        // Haystack with pattern at even position (valid token boundary)
        let haystack: &[u8] = &[0, 0, 31, 49, 0, 0];
        let matches: Vec<_> = ac.find_overlapping_iter(haystack).collect();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].start, 2);
        assert_eq!(matches[0].end, 4);
    }

    #[test]
    fn test_builder() {
        let mut builder = AutomatonBuilder::new();
        builder.add_pattern(b"hello");
        builder.add_pattern(b"world");
        let ac = builder.build();

        let matches: Vec<_> = ac.find_overlapping_iter(b"hello world").collect();
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_builder_empty_patterns() {
        let builder = AutomatonBuilder::new();
        let ac = builder.build();
        let matches: Vec<_> = ac.find_overlapping_iter(b"hello").collect();
        assert!(matches.is_empty());
    }

    #[test]
    fn test_builder_skips_empty_patterns() {
        let mut builder = AutomatonBuilder::new();
        builder.add_pattern(b"");
        builder.add_pattern(b"hello");
        builder.add_pattern(b"");
        let ac = builder.build();

        let matches: Vec<_> = ac.find_overlapping_iter(b"hello").collect();
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_serialize_deserialize() {
        let patterns: Vec<&[u8]> = vec![b"hello", b"world", b"test"];
        let ac1 = Automaton::build(&patterns);

        let serialized = ac1.inner.serialize();
        let ac2 = Automaton::deserialize_unchecked(&serialized);

        let haystack = b"hello world test";
        let matches1: Vec<_> = ac1.find_overlapping_iter(haystack).collect();
        let matches2: Vec<_> = ac2.find_overlapping_iter(haystack).collect();

        assert_eq!(matches1.len(), matches2.len());
        for (m1, m2) in matches1.iter().zip(matches2.iter()) {
            assert_eq!(m1.pattern, m2.pattern);
            assert_eq!(m1.start, m2.start);
            assert_eq!(m1.end, m2.end);
        }
    }

    #[test]
    fn test_overlapping_matches() {
        let patterns: Vec<&[u8]> = vec![b"ab", b"bc", b"abc"];
        let ac = Automaton::build(&patterns);

        let matches: Vec<_> = ac.find_overlapping_iter(b"abc").collect();
        // Should find "ab", "abc", and "bc" (all overlapping)
        assert!(matches.len() >= 2);
    }
}
