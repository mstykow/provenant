// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::prepare::prepare_text_line;
use crate::models::LineNumber;

pub(super) struct PreparedLineCache<'a> {
    raw_lines: &'a [&'a str],
    prepared: Vec<Option<String>>,
}

impl<'a> PreparedLineCache<'a> {
    pub(super) fn new(raw_lines: &'a [&'a str]) -> Self {
        Self {
            raw_lines,
            prepared: vec![None; raw_lines.len()],
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

    pub(super) fn get(&mut self, line_number: usize) -> Option<&str> {
        let idx = line_number.checked_sub(1)?;
        self.get_by_index(idx)
    }

    pub(super) fn get_by_index(&mut self, idx: usize) -> Option<&str> {
        if idx >= self.raw_lines.len() {
            return None;
        }
        if self.prepared[idx].is_none() {
            let raw = self.raw_lines[idx];
            self.prepared[idx] = Some(prepare_text_line(raw));
        }
        self.prepared[idx].as_deref()
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
