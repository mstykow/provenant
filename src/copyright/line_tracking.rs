// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::prepare::prepare_text_line;
use crate::models::LineNumber;

pub(super) struct PreparedLineCache<'a> {
    raw_lines: &'a [&'a str],
    prepared: Vec<Option<String>>,
}

pub(super) struct PreparedLines<'a> {
    raw_lines: &'a [&'a str],
    prepared: Vec<String>,
}

#[derive(Clone, Copy)]
pub(super) struct PreparedLine<'a> {
    pub(super) line_number: LineNumber,
    pub(super) raw: &'a str,
    pub(super) prepared: &'a str,
}

pub(super) struct PreparedLinesIter<'a> {
    lines: &'a PreparedLines<'a>,
    idx: usize,
}

pub(super) struct PreparedLinesNonEmptyIter<'a> {
    lines: &'a PreparedLines<'a>,
    idx: usize,
}

pub(super) struct PreparedLinesAdjacentPairsIter<'a> {
    lines: &'a PreparedLines<'a>,
    idx: usize,
}

pub(super) struct PreparedLinesAdjacentTriplesIter<'a> {
    lines: &'a PreparedLines<'a>,
    idx: usize,
}

impl<'a> PreparedLineCache<'a> {
    pub(super) fn new(raw_lines: &'a [&'a str]) -> Self {
        Self {
            raw_lines,
            prepared: vec![None; raw_lines.len()],
        }
    }

    pub(super) fn materialize(self) -> PreparedLines<'a> {
        let prepared = self
            .prepared
            .into_iter()
            .zip(self.raw_lines.iter().copied())
            .map(|(prepared, raw)| prepared.unwrap_or_else(|| prepare_text_line(raw)))
            .collect();

        PreparedLines {
            raw_lines: self.raw_lines,
            prepared,
        }
    }
}

impl<'a> PreparedLines<'a> {
    fn prepared_line_at_index(&self, idx: usize) -> PreparedLine<'_> {
        PreparedLine {
            line_number: LineNumber::from_0_indexed(idx),
            raw: self.raw_lines[idx],
            prepared: self.prepared[idx].as_str(),
        }
    }

    pub(super) fn raw_line_count(&self) -> usize {
        self.raw_lines.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.raw_lines.is_empty()
    }

    pub(super) fn len(&self) -> usize {
        self.raw_lines.len()
    }

    pub(super) fn line(&self, line_number: LineNumber) -> Option<PreparedLine<'_>> {
        let idx = line_number - 1;
        (idx < self.len()).then(|| self.prepared_line_at_index(idx))
    }

    pub(super) fn iter(&self) -> PreparedLinesIter<'_> {
        PreparedLinesIter {
            lines: self,
            idx: 0,
        }
    }

    pub(super) fn iter_non_empty(&self) -> PreparedLinesNonEmptyIter<'_> {
        PreparedLinesNonEmptyIter {
            lines: self,
            idx: 0,
        }
    }

    pub(super) fn next_non_empty_line(&self, line_number: LineNumber) -> Option<PreparedLine<'_>> {
        let mut idx = line_number.get();
        while idx < self.len() {
            if !self.prepared[idx].is_empty() {
                return Some(self.prepared_line_at_index(idx));
            }
            idx += 1;
        }
        None
    }

    pub(super) fn adjacent_pairs(&self) -> PreparedLinesAdjacentPairsIter<'_> {
        PreparedLinesAdjacentPairsIter {
            lines: self,
            idx: 0,
        }
    }

    pub(super) fn adjacent_triples(&self) -> PreparedLinesAdjacentTriplesIter<'_> {
        PreparedLinesAdjacentTriplesIter {
            lines: self,
            idx: 0,
        }
    }

    pub(super) fn get(&self, line_number: usize) -> Option<&str> {
        let idx = line_number.checked_sub(1)?;
        self.get_by_index(idx)
    }

    pub(super) fn get_by_index(&self, idx: usize) -> Option<&str> {
        self.prepared.get(idx).map(String::as_str)
    }

    pub(super) fn contains_ci(&self, pattern: &str) -> bool {
        let pattern_bytes = pattern.as_bytes();
        if pattern_bytes.is_empty() {
            return true;
        }
        self.raw_lines.iter().any(|line| {
            line.as_bytes()
                .windows(pattern_bytes.len())
                .any(|w| w.eq_ignore_ascii_case(pattern_bytes))
        })
    }
}

impl<'a> Iterator for PreparedLinesIter<'a> {
    type Item = PreparedLine<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx >= self.lines.len() {
            return None;
        }

        let line = self.lines.prepared_line_at_index(self.idx);
        self.idx += 1;
        Some(line)
    }
}

impl<'a> Iterator for PreparedLinesNonEmptyIter<'a> {
    type Item = PreparedLine<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.idx < self.lines.len() {
            let idx = self.idx;
            self.idx += 1;
            if self.lines.prepared[idx].is_empty() {
                continue;
            }
            return Some(self.lines.prepared_line_at_index(idx));
        }

        None
    }
}

impl<'a> Iterator for PreparedLinesAdjacentPairsIter<'a> {
    type Item = (PreparedLine<'a>, PreparedLine<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx + 1 >= self.lines.len() {
            return None;
        }

        let pair = (
            self.lines.prepared_line_at_index(self.idx),
            self.lines.prepared_line_at_index(self.idx + 1),
        );
        self.idx += 1;
        Some(pair)
    }
}

impl<'a> Iterator for PreparedLinesAdjacentTriplesIter<'a> {
    type Item = (PreparedLine<'a>, PreparedLine<'a>, PreparedLine<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx + 2 >= self.lines.len() {
            return None;
        }

        let triple = (
            self.lines.prepared_line_at_index(self.idx),
            self.lines.prepared_line_at_index(self.idx + 1),
            self.lines.prepared_line_at_index(self.idx + 2),
        );
        self.idx += 1;
        Some(triple)
    }
}

pub(super) struct LineNumberIndex {
    newline_offsets: Vec<usize>,
    content_len: usize,
}

impl LineNumberIndex {
    pub(super) fn new(content: &str) -> Self {
        let newline_offsets = content
            .as_bytes()
            .iter()
            .enumerate()
            .filter_map(|(idx, b)| (*b == b'\n').then_some(idx))
            .collect();

        Self {
            newline_offsets,
            content_len: content.len(),
        }
    }

    pub(super) fn line_number_at_offset(&self, byte_offset: usize) -> LineNumber {
        let offset = byte_offset.min(self.content_len);
        LineNumber::from_0_indexed(
            self.newline_offsets
                .partition_point(|&line_break| line_break < offset),
        )
    }
}
