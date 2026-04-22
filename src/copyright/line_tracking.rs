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
        Some(PreparedLine {
            line_number,
            raw: self.raw_lines.get(idx).copied()?,
            prepared: self.prepared.get(idx).map(String::as_str)?,
        })
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = PreparedLine<'_>> + '_ {
        self.raw_lines
            .iter()
            .copied()
            .zip(self.prepared.iter().map(String::as_str))
            .enumerate()
            .map(|(idx, (raw, prepared))| PreparedLine {
                line_number: LineNumber::from_0_indexed(idx),
                raw,
                prepared,
            })
    }

    pub(super) fn iter_non_empty(&self) -> impl Iterator<Item = PreparedLine<'_>> + '_ {
        self.iter().filter(|line| !line.prepared.is_empty())
    }

    pub(super) fn adjacent_pairs(
        &self,
    ) -> impl Iterator<Item = (PreparedLine<'_>, PreparedLine<'_>)> + '_ {
        self.raw_lines
            .iter()
            .copied()
            .zip(self.prepared.iter().map(String::as_str))
            .zip(
                self.raw_lines
                    .iter()
                    .copied()
                    .skip(1)
                    .zip(self.prepared.iter().skip(1).map(String::as_str)),
            )
            .enumerate()
            .map(|(idx, ((raw1, prepared1), (raw2, prepared2)))| {
                (
                    PreparedLine {
                        line_number: LineNumber::from_0_indexed(idx),
                        raw: raw1,
                        prepared: prepared1,
                    },
                    PreparedLine {
                        line_number: LineNumber::from_0_indexed(idx + 1),
                        raw: raw2,
                        prepared: prepared2,
                    },
                )
            })
    }

    pub(super) fn adjacent_triples(
        &self,
    ) -> impl Iterator<Item = (PreparedLine<'_>, PreparedLine<'_>, PreparedLine<'_>)> + '_ {
        self.raw_lines
            .iter()
            .copied()
            .zip(self.prepared.iter().map(String::as_str))
            .zip(
                self.raw_lines
                    .iter()
                    .copied()
                    .skip(1)
                    .zip(self.prepared.iter().skip(1).map(String::as_str)),
            )
            .zip(
                self.raw_lines
                    .iter()
                    .copied()
                    .skip(2)
                    .zip(self.prepared.iter().skip(2).map(String::as_str)),
            )
            .enumerate()
            .map(
                |(idx, (((raw1, prepared1), (raw2, prepared2)), (raw3, prepared3)))| {
                    (
                        PreparedLine {
                            line_number: LineNumber::from_0_indexed(idx),
                            raw: raw1,
                            prepared: prepared1,
                        },
                        PreparedLine {
                            line_number: LineNumber::from_0_indexed(idx + 1),
                            raw: raw2,
                            prepared: prepared2,
                        },
                        PreparedLine {
                            line_number: LineNumber::from_0_indexed(idx + 2),
                            raw: raw3,
                            prepared: prepared3,
                        },
                    )
                },
            )
    }

    pub(super) fn get(&self, line_number: usize) -> Option<&str> {
        let idx = line_number.checked_sub(1)?;
        self.get_by_index(idx)
    }

    pub(super) fn get_by_index(&self, idx: usize) -> Option<&str> {
        self.prepared.get(idx).map(String::as_str)
    }

    pub(super) fn raw_by_index(&self, idx: usize) -> Option<&str> {
        self.raw_lines.get(idx).copied()
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
